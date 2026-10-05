//! Selects direct static bases before comparing inherited method contracts.

use std::convert::Infallible;

#[cfg(test)]
use crate::Db;
use crate::types::context::InferContext;
use crate::types::generics::Specialization;
use crate::types::{ClassBase, ClassType, StaticClassLiteral, Type};

#[derive(Clone, Copy)]
pub(in crate::types) enum InheritedBaseSelection<'db> {
    DirectBase(ClassType<'db>),
    Skip,
    Stop,
}

pub(super) struct OrdinaryInheritedBaseSelectionEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousInheritedBaseSelectionEffects)]
    pub(in crate::types) trait InheritedBaseSelectionEffects<'db> {
        type Error;

        #[operation(local)]
        async fn empty_direct_bases(&self) -> Result<Vec<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_explicit_base(&self, bases: &[Type<'db>], cursor: &mut usize) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn classify_explicit_base(&self, class: StaticClassLiteral<'db>, base: Type<'db>) -> Result<InheritedBaseSelection<'db>, Self::Error>;
        #[operation(source)]
        async fn static_class_literal(&self, class: ClassType<'db>) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error>;
        #[operation(local)]
        async fn append_direct_base(&self, bases: &mut Vec<ClassType<'db>>, base: ClassType<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn has_multiple_direct_bases(&self, bases: &[ClassType<'db>]) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn mro_is_error(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    }

    #[synchronous(select_inherited_direct_bases_sync)]
    #[capabilities(effects = InheritedBaseSelectionEffects)]
    #[passive_values()]
    pub(in crate::types) async fn select_inherited_direct_bases_with<'db, E: InheritedBaseSelectionEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
    ) -> Result<Option<Vec<ClassType<'db>>>, E::Error> {
        let mut direct_bases = effects.empty_direct_bases().await?;
        let explicit_bases = effects.explicit_bases(class).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(base) = effects.next_explicit_base(explicit_bases, &mut cursor).await? {
            match effects.classify_explicit_base(class, base).await? {
                InheritedBaseSelection::DirectBase(base) => {
                    effects.append_direct_base(&mut direct_bases, base).await?;
                }
                InheritedBaseSelection::Skip => {}
                InheritedBaseSelection::Stop => return Ok(None),
            }
        }
        if !effects.has_multiple_direct_bases(&direct_bases).await?
            || effects.mro_is_error(class).await?
        {
            return Ok(None);
        }
        Ok(Some(direct_bases))
    }

    #[synchronous(classify_inherited_base_sync)]
    #[capabilities(effects = InheritedBaseSelectionEffects)]
    #[passive_values(InheritedBaseSelection::DirectBase, InheritedBaseSelection::Skip, InheritedBaseSelection::Stop)]
    pub(in crate::types) async fn classify_inherited_base_with<'db, E: InheritedBaseSelectionEffects<'db>>(
        base: Option<ClassBase<'db>>,
        effects: &E,
    ) -> Result<InheritedBaseSelection<'db>, E::Error> {
        Ok(match base {
            Some(ClassBase::Class(base)) => {
                if let Some(_) = effects.static_class_literal(base).await? {
                    InheritedBaseSelection::DirectBase(base)
                } else {
                    InheritedBaseSelection::Stop
                }
            }
            Some(
                ClassBase::Generic
                | ClassBase::Protocol
                | ClassBase::Any
                | ClassBase::Dynamic(_)
                | ClassBase::Divergent(_),
            ) => InheritedBaseSelection::Skip,
            _ => InheritedBaseSelection::Stop,
        })
    }
}

/// Advances over the actual explicit bases after the caller admits the cursor step.
pub(in crate::types) fn next_inherited_explicit_base<'db>(
    bases: &[Type<'db>],
    cursor: &mut usize,
) -> Option<Type<'db>> {
    let base = *bases.get(*cursor)?;
    *cursor += 1;
    Some(base)
}

/// The caller retains the vector across admission and prepays growth and eventual disposal.
pub(in crate::types) fn append_inherited_direct_base<'db>(
    bases: &mut Vec<ClassType<'db>>,
    base: ClassType<'db>,
) {
    bases.push(base);
}

impl<'db> SynchronousInheritedBaseSelectionEffects<'db>
    for OrdinaryInheritedBaseSelectionEffects<'_, 'db, '_>
{
    type Error = Infallible;

    fn empty_direct_bases(&self) -> Result<Vec<ClassType<'db>>, Self::Error> {
        Ok(Vec::new())
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        Ok(class.explicit_bases(self.context.db()))
    }

    fn next_explicit_base(
        &self,
        bases: &[Type<'db>],
        cursor: &mut usize,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(next_inherited_explicit_base(bases, cursor))
    }

    fn classify_explicit_base(
        &self,
        class: StaticClassLiteral<'db>,
        base: Type<'db>,
    ) -> Result<InheritedBaseSelection<'db>, Self::Error> {
        let db = self.context.db();
        let env = self.context.program_environment();
        classify_inherited_base_sync(
            ClassBase::try_from_explicit_base(db, env, base, Some(class.into())),
            self,
        )
    }

    fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error> {
        Ok(class.static_class_literal(self.context.db()))
    }

    fn append_direct_base(
        &self,
        bases: &mut Vec<ClassType<'db>>,
        base: ClassType<'db>,
    ) -> Result<(), Self::Error> {
        append_inherited_direct_base(bases, base);
        Ok(())
    }

    fn has_multiple_direct_bases(&self, bases: &[ClassType<'db>]) -> Result<bool, Self::Error> {
        Ok(bases.len() >= 2)
    }

    fn mro_is_error(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.try_mro(self.context.db(), None).is_err())
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;

    use super::*;
    use crate::db::tests::TestDbBuilder;
    use crate::place::global_symbol;

    #[test]
    fn direct_bases_preserve_order_and_require_a_valid_join() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/classes.py",
                "\
from typing import Any

class Left: pass
class Right: pass
class Derived(Left): pass

class Pair(Right, Left): pass
class Gradual(Any, Right, Left): pass
class Single(Any, Left): pass
class Invalid(Right, Left, 1): pass
class Inconsistent(Left, Derived): pass
",
            )
            .build()?;
        let file = system_path_to_file(&db, "/src/classes.py")?;
        let program_file = db.program_file(file);
        let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
        let env = db.program_environment();
        let class = |name| {
            global_symbol(&db, program_file, name)
                .place
                .ignore_possibly_undefined()
                .and_then(Type::as_class_literal)
                .and_then(|class| class.as_static())
                .ok_or_else(|| anyhow::anyhow!("missing {name} class"))
        };
        let right = ClassType::NonGeneric(class("Right")?.into());
        let left = ClassType::NonGeneric(class("Left")?.into());

        for (name, expected) in [
            ("Pair", Some(vec![right, left])),
            ("Gradual", Some(vec![right, left])),
            ("Single", None),
            ("Invalid", None),
            ("Inconsistent", None),
        ] {
            let class = class(name)?;
            let context = InferContext::new(
                &db,
                &env,
                class.body_scope(&db),
                file,
                program_file,
                &module,
            );
            let Ok(selected) = select_inherited_direct_bases_sync(
                class,
                &OrdinaryInheritedBaseSelectionEffects { context: &context },
            );
            let _diagnostics = context.finish();
            assert_eq!(selected, expected, "{name}");
        }
        Ok(())
    }
}
