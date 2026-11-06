//! The class-body and receiver definitions that can supply a member during lookup.

use std::{hash::BuildHasher, sync::Arc};

use ruff_python_ast::name::Name;
use rustc_hash::FxBuildHasher;
use ty_python_core::{
    attribute_scopes, definition::DefinitionKind, place::PlaceTable, scope::ScopeId, semantic_index,
};

use super::{ClassBase, ClassType, CodeGeneratorKind, StaticClassLiteral};
use crate::{Db, types::generics::Specialization};

/// Which sources to consult when looking up a member on this class.
pub(super) enum ClassMemberLookup<'db> {
    SynthesizedOnly,
    Namespace {
        code_generator: Option<CodeGeneratorKind<'db>>,
    },
}

/// Syntax-derived sources for an explicit class member or an implicit receiver attribute.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ClassMemberSources<'db> {
    pub(super) scope: ScopeId<'db>,
    pub(super) table: Arc<PlaceTable>,
    implicit_names: Box<[Name]>,
    instance_names: MemberNameFilter,
}

impl ClassMemberSources<'_> {
    pub(super) fn may_define_instance_member(&self, name: &str) -> bool {
        self.instance_names.might_contain(name)
    }

    pub(super) fn receiver_attribute_name(&self, name: &str) -> Option<&Name> {
        let index = self
            .implicit_names
            .binary_search_by(|candidate| candidate.as_str().cmp(name))
            .ok()?;
        Some(&self.implicit_names[index])
    }
}

#[salsa::tracked]
impl<'db> StaticClassLiteral<'db> {
    /// Names assigned to a method's receiver, in sorted order.
    pub(in crate::types) fn implicit_attribute_definition_names(
        self,
        db: &'db dyn Db,
    ) -> &'db [Name] {
        &self.member_sources(db).implicit_names
    }

    /// The class-body table and receiver assignments used by lookups on this class.
    #[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
    pub(super) fn member_sources(self, db: &'db dyn Db) -> ClassMemberSources<'db> {
        let scope = self.body_scope(db);
        let index = semantic_index(db, scope.program_file(db));
        let file_scope = scope.file_scope_id(db);
        let table = index.shared_place_table(file_scope);
        let implicit_names = collect_implicit_attribute_names(db, scope, true);
        let use_def = index.use_def_map(file_scope);
        let is_stub = scope.file(db).is_stub(db);
        // In a stub, a method can declare the value of an inherited slot. In a source file,
        // a plain method supplies a class member. Receiver assignments are indexed separately.
        let is_plain_method = |symbol| {
            use_def.unconditional_end_of_scope_definition(symbol).is_some_and(|definition| {
                matches!(definition.kind(db), DefinitionKind::Function(function) if !function.has_decorators())
            })
        };
        let instance_names = table
            .defined_symbols()
            .filter(|(id, _)| is_stub || !is_plain_method(*id))
            .map(|(_, symbol)| symbol.name())
            .chain(&implicit_names)
            .collect();
        ClassMemberSources {
            scope,
            table,
            implicit_names,
            instance_names,
        }
    }

    /// Selects which sources can supply this class member. A definite namespace miss still
    /// allows members synthesized by Django, total ordering and frozen dataclass bases.
    pub(super) fn class_member_lookup(self, db: &'db dyn Db, name: &str) -> ClassMemberLookup<'db> {
        match self.ordinary_class_member_names(db) {
            Some(names) if !names.might_contain(name) => ClassMemberLookup::SynthesizedOnly,
            Some(_) => ClassMemberLookup::Namespace {
                code_generator: None,
            },
            None => ClassMemberLookup::Namespace {
                code_generator: CodeGeneratorKind::from_static_class(db, self),
            },
        }
    }

    /// Class-body definitions, receiver assignments and slots can all supply an ordinary class
    /// member. Generated classes and cycles defer to the full lookup.
    #[salsa::tracked(returns(as_ref), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
    fn ordinary_class_member_names(self, db: &'db dyn Db) -> Option<MemberNameFilter> {
        if CodeGeneratorKind::from_static_class(db, self).is_some() {
            return None;
        }
        let sources = self.member_sources(db);
        Some(
            sources
                .table
                .defined_symbols()
                .map(|(_, symbol)| symbol.name())
                .chain(&sources.implicit_names)
                .chain(self.slot_names(db).unwrap_or_default())
                .collect(),
        )
    }

    pub(super) fn may_have_instance_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> bool {
        self.instance_member_names(db, specialization)
            .might_contain(name)
    }

    /// A static base can supply an instance attribute through a class-body declaration or receiver
    /// assignment. A dynamic base or a cycle leaves every name possible.
    #[salsa::tracked(returns(ref), cycle_initial=|_, _, _, _| MemberNameFilter::ALL, heap_size=ruff_memory_usage::heap_size)]
    fn instance_member_names(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> MemberNameFilter {
        let mut names = MemberNameFilter::default();
        for base in self.iter_mro(db, specialization) {
            match base {
                ClassBase::Class(class) => {
                    let Some((literal, _)) = class.static_class_literal(db) else {
                        return MemberNameFilter::ALL;
                    };
                    names.union_with(&literal.member_sources(db).instance_names);
                }
                ClassBase::Generic | ClassBase::Protocol => {}
                ClassBase::Any
                | ClassBase::Dynamic(_)
                | ClassBase::Divergent(_)
                | ClassBase::TypedDict(_) => return MemberNameFilter::ALL,
            }
        }
        names
    }
}

impl<'db> ClassType<'db> {
    /// Whether this class can contribute an instance member or an augmented receiver assignment.
    pub(super) fn may_define_own_instance_member(self, db: &'db dyn Db, name: &str) -> bool {
        self.static_class_literal(db)
            .is_none_or(|(literal, _)| literal.member_sources(db).may_define_instance_member(name))
    }
}

#[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
pub(super) fn implicit_attribute_names<'db>(
    db: &'db dyn Db,
    class_body_scope: ScopeId<'db>,
) -> Box<[Name]> {
    collect_implicit_attribute_names(db, class_body_scope, false)
}

fn collect_implicit_attribute_names<'db>(
    db: &'db dyn Db,
    class_body_scope: ScopeId<'db>,
    defined_only: bool,
) -> Box<[Name]> {
    let index = semantic_index(db, class_body_scope.program_file(db));
    let mut names = Vec::new();

    for function_scope_id in attribute_scopes(db, class_body_scope) {
        let table = index.place_table(function_scope_id);
        if defined_only {
            names.extend(table.defined_instance_attribute_names().map(Name::new));
        } else {
            names.extend(
                table
                    .members()
                    .filter_map(|member| member.as_instance_attribute().map(Name::new)),
            );
        }
    }

    names.sort_unstable();
    names.dedup();
    names.into_boxed_slice()
}

/// A fixed-size Bloom filter for member names. A miss rules out the name. A possible hit
/// runs ordinary lookup. Unioning two filters takes sixteen word operations.
#[derive(Debug, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct MemberNameFilter([u64; 16]);

impl MemberNameFilter {
    const ALL: Self = Self([u64::MAX; 16]);

    fn position(name: &str) -> (usize, u64) {
        let [low, high, ..] = FxBuildHasher.hash_one(name).to_le_bytes();
        let hash = u16::from_le_bytes([low, high]);
        // Four bits select a word. Each remaining group of six selects a bit in that word.
        (
            usize::from(hash >> 12),
            (1 << (hash % 64)) | (1 << ((hash >> 6) % 64)),
        )
    }

    fn might_contain(&self, name: &str) -> bool {
        let (word, mask) = Self::position(name);
        self.0[word] & mask == mask
    }

    fn union_with(&mut self, other: &Self) {
        for (word, other) in self.0.iter_mut().zip(&other.0) {
            *word |= other;
        }
    }
}

impl<'a> FromIterator<&'a Name> for MemberNameFilter {
    fn from_iter<T: IntoIterator<Item = &'a Name>>(names: T) -> Self {
        let mut filter = Self::default();
        for name in names {
            let (word, mask) = Self::position(name);
            filter.0[word] |= mask;
        }
        filter
    }
}
