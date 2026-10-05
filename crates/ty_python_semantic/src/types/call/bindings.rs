//! Resolve an instance's call method before analyzing its arguments.

use std::future::{Future, ready};

use crate::place::{DefinedPlace, Definedness, Place};
use crate::types::constructor::effects::{ConstructorError, checked_source};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::{DescriptorOrigin, MemberLookupPolicy, MemberLookupResult, Type};
use crate::{Db, ProgramEnvironment};

use super::{Bindings, CallableBinding};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum InstanceBindingsWork {
    Lookup,
    Resolve,
    Receiver,
    PossibleAbsence,
    Publish,
}

pub(in crate::types) trait BindingsEffects<'db> {
    type Error;

    fn checkpoint(&self, db: &dyn Db, work: InstanceBindingsWork) -> Result<(), Self::Error>;

    async fn call_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> Result<MemberLookupResult<'db>, Self::Error>;

    async fn bindings_from_descriptor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> Result<Bindings<'db>, Self::Error>;
}

pub(in crate::types) struct InlineBindingsEffects<'a, 'db> {
    pub(in crate::types) recursion_guard: &'a CallableRecursionGuard<'db>,
}

impl<'db> BindingsEffects<'db> for InlineBindingsEffects<'_, 'db> {
    type Error = ConstructorError;

    fn checkpoint(&self, db: &dyn Db, _work: InstanceBindingsWork) -> Result<(), Self::Error> {
        checked_source(db, || Ok(()))
    }

    fn call_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> impl Future<Output = Result<MemberLookupResult<'db>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(ty.member_lookup_with_recursion_guard(
                db,
                env,
                "__call__",
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                None,
                Some(guard),
            ))
        }))
    }

    fn bindings_from_descriptor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> impl Future<Output = Result<Bindings<'db>, Self::Error>> {
        ready(checked_source(db, || {
            Ok(self.recursion_guard.with_dependency(db, origin, || {
                ty.bindings_from_descriptor(db, env, self.recursion_guard, origin)
            }))
        }))
    }
}

pub(in crate::types) async fn instance_bindings_with<'db, E: BindingsEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    receiver: Type<'db>,
    guard: &CallableRecursionGuard<'db>,
    effects: &E,
) -> Result<Bindings<'db>, E::Error> {
    effects.checkpoint(db, InstanceBindingsWork::Lookup)?;
    let member = effects.call_member(db, env, receiver, guard).await?;
    effects.checkpoint(db, InstanceBindingsWork::Resolve)?;
    let member = member.unwrap_or_else(|error| error.fallback_member(db));
    let Place::Defined(DefinedPlace {
        ty: callable,
        definedness,
        ..
    }) = member.member(db).place
    else {
        effects.checkpoint(db, InstanceBindingsWork::Publish)?;
        return Ok(CallableBinding::not_callable(receiver).into());
    };

    let mut bindings = effects
        .bindings_from_descriptor(db, env, callable, member.descriptor_origin(db))
        .await?;
    // Note that for objects that have a (possibly not callable!) `__call__` attribute,
    // we will get the signature of the `__call__` attribute, but will pass in the type
    // of the original object as the "callable type". That ensures that we get errors
    // like "`X` is not callable" instead of "`<type of illegal '__call__'>` is not
    // callable.
    bindings.try_replace_callable_type(callable, receiver, || {
        effects.checkpoint(db, InstanceBindingsWork::Receiver)
    })?;
    if definedness == Definedness::PossiblyUndefined {
        bindings.try_set_dunder_call_is_possibly_unbound(|| {
            effects.checkpoint(db, InstanceBindingsWork::PossibleAbsence)
        })?;
    }
    effects.checkpoint(db, InstanceBindingsWork::Publish)?;
    Ok(bindings)
}
