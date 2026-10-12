//! Union construction for ordinary classes, indexed by nominal ancestry.

use rustc_hash::{FxHashMap, FxHashSet};

use super::union_elements::UnionElements;
use crate::types::{ClassBase, ClassLiteral, ClassType, KnownClass, StaticClassLiteral, Type};
use crate::{Db, ProgramEnvironment};

pub(crate) struct NominalUnionMember<'db> {
    ty: Type<'db>,
    class: StaticClassLiteral<'db>,
    bases: Box<[StaticClassLiteral<'db>]>,
}

impl<'db> NominalUnionMember<'db> {
    /// Restrict indexing to classes whose redundancy relation is nominal inheritance.
    pub(crate) fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Option<Self> {
        let Type::NominalInstance(instance) = ty else {
            return None;
        };
        if !instance.is_plain_non_generic_static() {
            return None;
        }
        let class_type = instance.class(db, env);
        let ClassType::NonGeneric(ClassLiteral::Static(class)) = class_type else {
            return None;
        };
        // Hashable has additional union-preservation rules. Invalid inheritance cycles also
        // need the general relation checker and its cycle recovery.
        if class.known(db) == Some(KnownClass::Hashable)
            || ClassBase::Class(class_type).has_cyclic_mro(db)
        {
            return None;
        }
        let bases = class_type
            .iter_mro(db)
            .filter_map(|base| match base {
                ClassBase::Class(ClassType::NonGeneric(ClassLiteral::Static(base))) => Some(base),
                _ => None,
            })
            .collect::<Box<[_]>>();
        Some(Self { ty, class, bases })
    }
}

/// Normalize a sequence of ordinary class instances without comparing every pair.
///
/// The reverse ancestry index identifies existing members subsumed by a newly added base.
/// Members are replaced and removed in the same order as `UnionBuilder`, preserving the
/// displayed union order. Shared member storage keeps every intermediate expression type
/// available without copying the whole prefix.
pub(crate) struct NominalUnionBuilder<'db> {
    members: Vec<NominalUnionMember<'db>>,
    positions: FxHashMap<StaticClassLiteral<'db>, usize>,
    descendants: FxHashMap<StaticClassLiteral<'db>, FxHashSet<StaticClassLiteral<'db>>>,
    elements: UnionElements<'db>,
}

impl<'db> NominalUnionBuilder<'db> {
    pub(crate) fn new() -> Self {
        Self {
            members: Vec::new(),
            positions: FxHashMap::default(),
            descendants: FxHashMap::default(),
            elements: UnionElements::Leaf(Box::new([])),
        }
    }

    pub(crate) fn add(&mut self, db: &'db dyn Db, member: NominalUnionMember<'db>) -> Type<'db> {
        if self.positions.contains_key(&member.class)
            || member
                .bases
                .iter()
                .any(|base| self.positions.contains_key(base))
        {
            return self.elements.clone().into_type(db);
        }

        let mut removed = self
            .descendants
            .get(&member.class)
            .into_iter()
            .flatten()
            .filter_map(|class| self.positions.get(class).copied())
            .collect::<Vec<_>>();
        removed.sort_unstable();

        for &index in &removed {
            let old = &self.members[index];
            self.positions.remove(&old.class);
            for base in &old.bases {
                if let Some(descendants) = self.descendants.get_mut(base) {
                    descendants.remove(&old.class);
                    if descendants.is_empty() {
                        self.descendants.remove(base);
                    }
                }
            }
        }

        let position = removed.first().copied().unwrap_or(self.members.len());
        self.positions.insert(member.class, position);
        for &base in &member.bases {
            self.descendants
                .entry(base)
                .or_default()
                .insert(member.class);
        }

        if let Some((&first, rest)) = removed.split_first() {
            self.elements = self.elements.replace(db, first, member.ty);
            self.members[first] = member;
            for &index in rest.iter().rev() {
                if let Some(last) = self.members.last() {
                    self.elements = self.elements.replace(db, index, last.ty);
                }
                self.elements = self.elements.pop(db);
                self.members.swap_remove(index);
                if let Some(moved) = self.members.get(index) {
                    self.positions.insert(moved.class, index);
                }
            }
        } else {
            self.elements = self.elements.push(db, member.ty);
            self.members.push(member);
        }
        self.elements.clone().into_type(db)
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem as _;
    use ty_python_core::ProgramFile;

    use super::{NominalUnionBuilder, NominalUnionMember};
    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::{Type, UnionBuilder};

    #[test]
    fn nominal_prefixes_match_general_normalization() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/classes.py",
            "
            class A: ...
            class B: ...
            class C: ...
            class D(A): ...
            class E(A, B): ...
            class F(D): ...
            ",
        )?;
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/classes.py")?;
        let file = ProgramFile::new(&db, file, env.program(&db));
        let classes = ["A", "B", "C", "D", "E", "F"].map(|name| {
            let class = global_symbol(&db, file, name)
                .place
                .expect_type()
                .expect_class_literal();
            Type::instance(&db, &env, class.default_specialization(&db))
        });
        // Every ordering exercises both redundant additions and replacement of earlier
        // subclasses. Repeating the sequence also checks the index after swap removals.
        for order in classes.into_iter().permutations(classes.len()) {
            let mut indexed = NominalUnionBuilder::new();
            let mut general = Type::Never;
            for &ty in order.iter().chain(&order) {
                let Some(member) = NominalUnionMember::new(&db, &env, ty) else {
                    anyhow::bail!("ordinary class was not eligible");
                };
                general = UnionBuilder::new(&db, &env).add(general).add(ty).build();
                assert_eq!(indexed.add(&db, member), general, "{order:?}");
            }
        }
        Ok(())
    }
}
