//! Shared decisions for resolving and matching a `Self` replacement.

use std::convert::Infallible;
use std::future::{Future, ready};

use crate::types::typevar::BindingContext;
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, SelfBinding, Type, class_mro_literals,
    self_typevar_owner_class_literal,
};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(in crate::types) mod attempt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum SelfBindingWork {
    Prepare,
    MatchVariable,
    MroMember,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

pub(in crate::types) trait SelfBindingEffects<'db>: sealed::Sealed {
    type Error;

    fn checkpoint(&self, work: SelfBindingWork) -> impl Future<Output = Result<(), Self::Error>>;

    fn is_self(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    fn binding_context(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<BindingContext<'db>, Self::Error>>;

    fn nominal_owner(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Option<ClassLiteral<'db>>, Self::Error>>;

    fn self_owner(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Option<ClassLiteral<'db>>, Self::Error>>;

    fn mro_literals(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<&'db [ClassLiteral<'db>], Self::Error>>;
}

pub(in crate::types) async fn prepare_with<'db, E: SelfBindingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    self_type: Type<'db>,
    binding_context: Option<BindingContext<'db>>,
    effects: &E,
) -> Result<SelfBinding<'db>, E::Error> {
    effects.checkpoint(SelfBindingWork::Prepare).await?;
    let class_literal = match self_type {
        Type::TypeVar(variable) if effects.is_self(db, variable).await? => {
            effects.self_owner(db, env, variable).await?
        }
        _ => effects.nominal_owner(db, env, self_type).await?,
    };
    Ok(SelfBinding {
        ty: self_type,
        class_literal,
        binding_context,
    })
}

pub(in crate::types) async fn should_bind_with<'db, E: SelfBindingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    binding: &SelfBinding<'db>,
    variable: BoundTypeVarInstance<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.checkpoint(SelfBindingWork::MatchVariable).await?;
    if !effects.is_self(db, variable).await? {
        return Ok(false);
    }

    // A matching method binding context avoids resolving the class hierarchy.
    if binding.binding_context == Some(effects.binding_context(db, variable).await?) {
        return Ok(true);
    }
    let Some(class) = binding.class_literal else {
        return Ok(false);
    };

    // Materialize the receiver's MRO before resolving the variable's owner. This preserves
    // the source dependencies and read order of the ordinary binding operation.
    let mro = effects.mro_literals(db, class).await?;
    let Some(owner) = effects.self_owner(db, env, variable).await? else {
        // An unresolved owner imposes no restriction when the receiver's class is known.
        return Ok(true);
    };
    for candidate in mro {
        effects.checkpoint(SelfBindingWork::MroMember).await?;
        if *candidate == owner {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(in crate::types) struct InlineSelfBindingEffects;

impl sealed::Sealed for InlineSelfBindingEffects {}

impl<'db> SelfBindingEffects<'db> for InlineSelfBindingEffects {
    type Error = Infallible;

    fn checkpoint(&self, _work: SelfBindingWork) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Ok(()))
    }

    fn is_self(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(variable.typevar(db).is_self(db)))
    }

    fn binding_context(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<BindingContext<'db>, Self::Error>> {
        ready(Ok(variable.binding_context(db)))
    }

    fn nominal_owner(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Option<ClassLiteral<'db>>, Self::Error>> {
        ready(Ok(ty
            .nominal_class(db, env)
            .map(|class| class.class_literal(db))))
    }

    fn self_owner(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> impl Future<Output = Result<Option<ClassLiteral<'db>>, Self::Error>> {
        ready(Ok(self_typevar_owner_class_literal(db, env, variable)))
    }

    fn mro_literals(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<&'db [ClassLiteral<'db>], Self::Error>> {
        ready(Ok(class_mro_literals(db, class).as_ref()))
    }
}
