//! Legacy type parameters inherited from specialized base classes.

use std::convert::Infallible;

use ty_python_core::definition::Definition;

use super::InlineClassContextEffects;
use crate::types::class::source::{SourceClassEffects, SourceClassError};
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::{BoundTypeVarInstance, GenericContext, StaticClassLiteral, Type};
use crate::{Db, FxOrderSet, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum InheritedContextWork {
    Definition,
    Bases,
    Base,
    CreateVariables,
    Variables { retained: usize },
    DiscardVariables,
    Intern { len: usize },
    Publish,
}

pub(in crate::types) trait InheritedContextEffects<'db> {
    type Error;

    fn checkpoint(&self, work: InheritedContextWork) -> Result<(), Self::Error>;
    fn definition(&self, class: StaticClassLiteral<'db>) -> Result<Definition<'db>, Self::Error>;
    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error>;
    fn find_variables(
        &self,
        env: &ProgramEnvironment<'db>,
        definition: Definition<'db>,
        base: Type<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<(), Self::Error>;
    fn build_context(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<GenericContext<'db>, Self::Error>;
}

pub(in crate::types) fn inherited_context_with<'db, E: InheritedContextEffects<'db>>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<Option<GenericContext<'db>>, E::Error> {
    let _ = db;
    inherited_context_shared_sync(
        class,
        &InheritedContextAdapter { effects },
        InheritedContextFacts,
    )
}

pub(in crate::types) async fn inherited_context_async_with<
    'db,
    E: AsyncInheritedContextEffects<'db>,
>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<Option<GenericContext<'db>>, E::Error> {
    let _ = db;
    inherited_context_shared_with(class, effects, InheritedContextFacts).await
}

pub(in crate::types) struct InheritedContextBaseCursor<'db> {
    bases: std::iter::Copied<std::slice::Iter<'db, Type<'db>>>,
}

impl<'db> InheritedContextBaseCursor<'db> {
    fn new(bases: &'db [Type<'db>]) -> Self {
        Self {
            bases: bases.iter().copied(),
        }
    }

    pub(in crate::types) fn next_base(&mut self) -> Option<Type<'db>> {
        self.bases.next()
    }
}

struct InheritedContextFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousInheritedContextEffects)]
    pub(in crate::types) trait AsyncInheritedContextEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: InheritedContextWork) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn definition(&self, class: StaticClassLiteral<'db>) -> Result<Definition<'db>, Self::Error>;
        #[operation(child)]
        async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&'db [Type<'db>], Self::Error>;
        /// Admission covers construction and disposal of the empty owner on every exit,
        /// including refusal or cancellation while scanning the class's bases.
        #[operation(local)]
        async fn new_variables(&self) -> Result<FxOrderSet<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_base(&self, cursor: &mut InheritedContextBaseCursor<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        /// Admission covers retained variables and their disposal on every exit,
        /// including partial discovery followed by refusal or cancellation.
        #[operation(child)]
        async fn find_variables(
            &self,
            env: &ProgramEnvironment<'db>,
            definition: Definition<'db>,
            base: Type<'db>,
            variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        ) -> Result<(), Self::Error>;
        /// Consumes an empty owner whose disposal was admitted by `new_variables`.
        #[operation(local)]
        async fn discard_variables(&self, variables: FxOrderSet<BoundTypeVarInstance<'db>>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn build_context(
            &self,
            env: &ProgramEnvironment<'db>,
            variables: FxOrderSet<BoundTypeVarInstance<'db>>,
        ) -> Result<GenericContext<'db>, Self::Error>;
    }

    #[finite_capability]
    impl InheritedContextFacts {
        fn environment<'db>(&self, definition: Definition<'db>) -> ProgramEnvironment<'db> {
            ProgramEnvironment::from_definition(definition)
        }

        fn base_cursor<'db>(&self, bases: &'db [Type<'db>]) -> InheritedContextBaseCursor<'db> {
            InheritedContextBaseCursor::new(bases)
        }

        fn variables_len(&self, variables: &FxOrderSet<BoundTypeVarInstance<'_>>) -> usize {
            variables.len()
        }

        fn variables_empty(&self, variables: &FxOrderSet<BoundTypeVarInstance<'_>>) -> bool {
            variables.is_empty()
        }
    }

    #[synchronous(inherited_context_shared_sync)]
    #[capabilities(effects = AsyncInheritedContextEffects, facts = InheritedContextFacts)]
    #[passive_values(InheritedContextWork::Definition, InheritedContextWork::Bases, InheritedContextWork::Variables, InheritedContextWork::Intern, InheritedContextWork::Publish)]
    async fn inherited_context_shared_with<'db, E: AsyncInheritedContextEffects<'db>>(
        class: StaticClassLiteral<'db>,
        effects: &E,
        facts: InheritedContextFacts,
    ) -> Result<Option<GenericContext<'db>>, E::Error> {
        effects.checkpoint(InheritedContextWork::Definition).await?;
        let definition = effects.definition(class).await?;
        effects.checkpoint(InheritedContextWork::Bases).await?;
        let bases = effects.explicit_bases(class).await?;
        let env = facts.environment(definition);
        let mut cursor = facts.base_cursor(bases);
        let mut variables = effects.new_variables().await?;
        #[cursor_loop]
        while let Some(base) = effects.next_base(&mut cursor).await? {
            if matches!(base, Type::GenericAlias(_)) {
                effects.checkpoint(InheritedContextWork::Variables {
                    retained: facts.variables_len(&variables),
                }).await?;
                effects.find_variables(&env, definition, base, &mut variables).await?;
            }
        }
        if facts.variables_empty(&variables) {
            effects.discard_variables(variables).await?;
            effects.checkpoint(InheritedContextWork::Publish).await?;
            return Ok(None);
        }
        effects.checkpoint(InheritedContextWork::Intern {
            len: facts.variables_len(&variables),
        }).await?;
        let context = effects.build_context(&env, variables).await?;
        effects.checkpoint(InheritedContextWork::Publish).await?;
        Ok(Some(context))
    }
}

struct InheritedContextAdapter<'a, E> {
    effects: &'a E,
}

impl<'db, E: InheritedContextEffects<'db>> SynchronousInheritedContextEffects<'db>
    for InheritedContextAdapter<'_, E>
{
    type Error = E::Error;

    fn checkpoint(&self, work: InheritedContextWork) -> Result<(), Self::Error> {
        self.effects.checkpoint(work)
    }

    fn definition(&self, class: StaticClassLiteral<'db>) -> Result<Definition<'db>, Self::Error> {
        self.effects.definition(class)
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        self.effects.explicit_bases(class)
    }

    fn new_variables(&self) -> Result<FxOrderSet<BoundTypeVarInstance<'db>>, Self::Error> {
        self.effects
            .checkpoint(InheritedContextWork::CreateVariables)?;
        Ok(FxOrderSet::default())
    }

    fn next_base(
        &self,
        cursor: &mut InheritedContextBaseCursor<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.effects.checkpoint(InheritedContextWork::Base)?;
        Ok(cursor.next_base())
    }

    fn find_variables(
        &self,
        env: &ProgramEnvironment<'db>,
        definition: Definition<'db>,
        base: Type<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<(), Self::Error> {
        self.effects
            .find_variables(env, definition, base, variables)
    }

    fn discard_variables(
        &self,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<(), Self::Error> {
        self.effects
            .checkpoint(InheritedContextWork::DiscardVariables)?;
        drop(variables);
        Ok(())
    }

    fn build_context(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        self.effects.build_context(env, variables)
    }
}

impl<'db> InheritedContextEffects<'db> for InlineClassContextEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self, _: InheritedContextWork) -> Result<(), Infallible> {
        Ok(())
    }
    fn definition(&self, class: StaticClassLiteral<'db>) -> Result<Definition<'db>, Infallible> {
        Ok(class.definition(self.db))
    }
    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Infallible> {
        Ok(class.explicit_bases(self.db))
    }
    fn find_variables(
        &self,
        env: &ProgramEnvironment<'db>,
        definition: Definition<'db>,
        base: Type<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<(), Infallible> {
        base.find_legacy_typevars(self.db, env, Some(definition), variables);
        Ok(())
    }
    fn build_context(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<GenericContext<'db>, Infallible> {
        Ok(GenericContext::from_typevar_instances(
            self.db, env, variables,
        ))
    }
}

impl<'db> InheritedContextEffects<'db> for SourceClassEffects<'db> {
    type Error = SourceClassError;

    fn checkpoint(&self, _: InheritedContextWork) -> Result<(), SourceClassError> {
        self.check()
    }

    fn definition(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Definition<'db>, SourceClassError> {
        read_source(self, || class.definition(self.db))
    }

    fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], SourceClassError> {
        read_source(self, || class.explicit_bases(self.db))
    }

    fn find_variables(
        &self,
        env: &ProgramEnvironment<'db>,
        definition: Definition<'db>,
        base: Type<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<(), SourceClassError> {
        read_source(self, || {
            base.find_legacy_typevars(self.db, env, Some(definition), variables);
        })
    }

    fn build_context(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> Result<GenericContext<'db>, SourceClassError> {
        read_source(self, || {
            GenericContext::from_typevar_instances(self.db, env, variables)
        })
    }
}
