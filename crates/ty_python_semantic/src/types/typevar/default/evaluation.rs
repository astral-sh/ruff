use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;

use crate::types::Type;
use crate::types::typevar::{TypeVarDefaultEvaluation, TypeVarDefaultVisitor, TypeVarInstance};
use crate::{Db, ProgramEnvironment};

/// Dispatches stored defaults, reusing a visitor when called during self-reference validation.
pub(in crate::types::typevar) struct OrdinaryTypeVarDefaultEffects<'visitor, 'db> {
    pub(in crate::types::typevar) db: &'db dyn Db,
    pub(in crate::types::typevar) visitor: Option<&'visitor TypeVarDefaultVisitor<'db>>,
}

/// Evaluates a lazy default with the visitor used by its recursive self-reference checks.
pub(in crate::types::typevar) struct OrdinaryLazyTypeVarDefaultEffects<'visitor, 'db> {
    pub(in crate::types::typevar) db: &'db dyn Db,
    pub(in crate::types::typevar) visitor: &'visitor TypeVarDefaultVisitor<'db>,
}

shared_semantic_family! {
    #[synchronous(SynchronousTypeVarDefaultEffects)]
    pub(in crate::types) trait TypeVarDefaultEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn checked_lazy_default(&self, variable: TypeVarInstance<'db>, env: &ProgramEnvironment<'db>) -> Result<Option<Type<'db>>, Self::Error>;
    }

    #[synchronous(SynchronousLazyTypeVarDefaultEffects)]
    /// Fetches and checks lazy defaults using an existing self-reference visitor.
    pub(in crate::types) trait LazyTypeVarDefaultEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn lazy_default_unchecked(&self, variable: TypeVarInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn type_is_self_referential(&self, variable: TypeVarInstance<'db>, env: &ProgramEnvironment<'db>, default: Type<'db>) -> Result<bool, Self::Error>;
    }

    /// Evaluates an already-read default, checking lazy defaults with the effects' self-reference visitor.
    #[synchronous(typevar_default_sync)]
    #[capabilities(effects = TypeVarDefaultEffects)]
    #[passive_values()]
    pub(in crate::types) async fn typevar_default_with<'db, E: TypeVarDefaultEffects<'db>>(
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        stored: Option<TypeVarDefaultEvaluation<'db>>,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        effects.checkpoint().await?;
        match stored {
            None => Ok(None),
            Some(TypeVarDefaultEvaluation::Eager(default)) => Ok(Some(default)),
            Some(TypeVarDefaultEvaluation::Lazy) => effects.checked_lazy_default(variable, env).await,
        }
    }

    #[synchronous(lazy_typevar_default_sync)]
    #[capabilities(effects = LazyTypeVarDefaultEffects)]
    #[passive_values()]
    pub(in crate::types) async fn lazy_typevar_default_with<'db, E: LazyTypeVarDefaultEffects<'db>>(
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        effects.checkpoint().await?;
        let Some(default) = effects.lazy_default_unchecked(variable).await? else {
            return Ok(None);
        };

        // Unlike bounds/constraints, default types are allowed to be generic
        // (https://typing.python.org/en/latest/spec/generics.html#defaults-for-type-parameters).
        // Here we simply check for non-self-referential.
        // TODO: We should also check for non-forward references.
        if effects.type_is_self_referential(variable, env, default).await? {
            return Ok(None);
        }

        Ok(Some(default))
    }
}

impl<'db> SynchronousTypeVarDefaultEffects<'db> for OrdinaryTypeVarDefaultEffects<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn checked_lazy_default(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        let new_visitor;
        let visitor = match self.visitor {
            Some(visitor) => visitor,
            None => {
                new_visitor = TypeVarDefaultVisitor::new(None);
                &new_visitor
            }
        };
        Ok(visitor.visit(self.db, variable, || {
            let Ok(default) = lazy_typevar_default_sync(
                variable,
                env,
                &OrdinaryLazyTypeVarDefaultEffects {
                    db: self.db,
                    visitor,
                },
            );
            default
        }))
    }
}

impl<'db> SynchronousLazyTypeVarDefaultEffects<'db>
    for OrdinaryLazyTypeVarDefaultEffects<'_, 'db>
{
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn lazy_default_unchecked(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(variable.lazy_default_unchecked(self.db))
    }

    fn type_is_self_referential(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        default: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(variable.type_is_self_referential(self.db, env, default, self.visitor))
    }
}

#[cfg(test)]
mod tests;
