//! Prepare bound methods while preserving captured receivers and overload specialization.

use std::convert::Infallible;

use smallvec::SmallVec;

use crate::types::call::{Bindings, CallableBinding};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::signatures::effects::legacy_inline;
use crate::types::{BoundMethodType, CallableSignature, DescriptorOrigin, Signature, Type};
use crate::{Db, ProgramEnvironment};

// Most methods have one overload; keep that temporary signature inline.
pub(in crate::types) type BoundMethodOverloads<'db> = SmallVec<[Signature<'db>; 1]>;

/// Semantic children of bound-method binding preparation that can remain unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundMethodPreparationOperation {
    CallableInstanceUpcast,
    ReceiverTypevarSearch,
    ReceiverSpecialization,
}

/// Both receiver types are retained because a constrained receiver can differ from `__self__`.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct BoundMethodReceivers<'db> {
    pub self_instance: Type<'db>,
    pub signature_receiver: Type<'db>,
}

/// Supplies canonical signatures, receiver transformations, and admitted binding storage.
pub(in crate::types) trait BoundMethodBindingEffects<'db> {
    type Error;

    async fn callable(
        &self,
        db: &'db dyn Db,
        method: BoundMethodType<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    async fn unbound_signatures(
        &self,
        db: &'db dyn Db,
        callable: Type<'db>,
    ) -> Result<Option<&'db CallableSignature<'db>>, Self::Error>;
    async fn receivers(
        &self,
        db: &'db dyn Db,
        method: BoundMethodType<'db>,
    ) -> Result<BoundMethodReceivers<'db>, Self::Error>;
    async fn protocol_receiver_is_specialized(
        &self,
        db: &'db dyn Db,
        receiver: Type<'db>,
        signature: &CallableSignature<'db>,
    ) -> Result<bool, Self::Error>;
    async fn from_signature(
        &self,
        callable_type: Type<'db>,
        signature: &CallableSignature<'db>,
        receiver: Type<'db>,
    ) -> Result<Bindings<'db>, Self::Error>;
    async fn bake_receiver(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
    ) -> Result<(), Self::Error>;
    async fn empty_overloads(&self) -> Result<BoundMethodOverloads<'db>, Self::Error>;
    async fn receiver_determines_typevar(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &Signature<'db>,
    ) -> Result<bool, Self::Error>;
    async fn specialize_receiver(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &Signature<'db>,
        method: BoundMethodType<'db>,
        receiver: Type<'db>,
    ) -> Result<Option<CallableSignature<'db>>, Self::Error>;
    async fn append_overloads(
        &self,
        output: &mut BoundMethodOverloads<'db>,
        signatures: &[Signature<'db>],
    ) -> Result<(), Self::Error>;
    async fn finish_overloads(
        &self,
        callable_type: Type<'db>,
        receiver: Type<'db>,
        overloads: BoundMethodOverloads<'db>,
    ) -> Result<Bindings<'db>, Self::Error>;
    async fn upcast(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callable_type: Type<'db>,
        method: BoundMethodType<'db>,
        unknown_is_recovery: bool,
    ) -> Result<Bindings<'db>, Self::Error>;
}

/// Expands a bound method using its unbound signatures and the receiver rules used by direct calls.
/// Callable instances without stored unbound signatures require a separate upcast operation.
pub(in crate::types) async fn bound_method_bindings_with<'db, E: BoundMethodBindingEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    callable_type: Type<'db>,
    method: BoundMethodType<'db>,
    unknown_is_recovery: bool,
    effects: &E,
) -> Result<Bindings<'db>, E::Error> {
    let callable = effects.callable(db, method).await?;
    let Some(signature) = effects.unbound_signatures(db, callable).await? else {
        return effects
            .upcast(db, env, callable_type, method, unknown_is_recovery)
            .await;
    };
    let receivers = effects.receivers(db, method).await?;
    // Class-based protocol member lookup has already specialized the method for this
    // receiver. Bake an implicit positional receiver into the signature instead of
    // checking it structurally again during call inference.
    let protocol_receiver_is_specialized = effects
        .protocol_receiver_is_specialized(db, receivers.self_instance, signature)
        .await?;
    // Synthesized signatures can contain `Self` without a function's generic
    // context, so substitute it when capturing the receiver.
    if callable.as_function_literal().is_none()
        || protocol_receiver_is_specialized
        || receivers.signature_receiver != receivers.self_instance
    {
        let mut bindings = effects
            .from_signature(callable_type, signature, receivers.signature_receiver)
            .await?;
        effects.bake_receiver(db, env, &mut bindings).await?;
        return Ok(bindings);
    }

    // Solve exact receiver constraints before checking the other arguments, but
    // retain the receiver itself for call inference and receiver diagnostics.
    let mut overloads = effects.empty_overloads().await?;
    for overload in &signature.overloads {
        if effects
            .receiver_determines_typevar(db, env, overload)
            .await?
            && let Some(specialized) = effects
                .specialize_receiver(db, env, overload, method, receivers.self_instance)
                .await?
        {
            effects
                .append_overloads(&mut overloads, &specialized.overloads)
                .await?;
        } else {
            effects
                .append_overloads(&mut overloads, std::slice::from_ref(overload))
                .await?;
        }
    }
    effects
        .finish_overloads(callable_type, receivers.self_instance, overloads)
        .await
}

/// Prepares ordinary bindings through [`bound_method_bindings_with`] and the caller's expansion guard.
pub(in crate::types) fn bound_method_bindings<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    callable_type: Type<'db>,
    method: BoundMethodType<'db>,
    unknown_is_recovery: bool,
    guard: &CallableRecursionGuard<'db>,
) -> Bindings<'db> {
    legacy_inline(bound_method_bindings_with(
        db,
        env,
        callable_type,
        method,
        unknown_is_recovery,
        &OrdinaryBoundMethodBindings { guard },
    ))
}

#[derive(Debug)]
struct OrdinaryBoundMethodBindings<'guard, 'db> {
    guard: &'guard CallableRecursionGuard<'db>,
}

impl<'db> BoundMethodBindingEffects<'db> for OrdinaryBoundMethodBindings<'_, 'db> {
    type Error = Infallible;

    async fn callable(
        &self,
        db: &'db dyn Db,
        method: BoundMethodType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(method.func(db))
    }

    async fn unbound_signatures(
        &self,
        db: &'db dyn Db,
        callable: Type<'db>,
    ) -> Result<Option<&'db CallableSignature<'db>>, Self::Error> {
        Ok(match callable {
            Type::FunctionLiteral(function) => Some(function.signature(db)),
            Type::Callable(callable) => Some(callable.signatures(db)),
            _ => None,
        })
    }

    async fn receivers(
        &self,
        db: &'db dyn Db,
        method: BoundMethodType<'db>,
    ) -> Result<BoundMethodReceivers<'db>, Self::Error> {
        Ok(BoundMethodReceivers {
            self_instance: method.self_instance(db),
            signature_receiver: method.signature_receiver(db),
        })
    }

    async fn protocol_receiver_is_specialized(
        &self,
        db: &'db dyn Db,
        receiver: Type<'db>,
        signature: &CallableSignature<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(receiver
            .as_protocol_instance()
            .is_some_and(|protocol| protocol.class_origin(db).is_some())
            && signature
                .overloads
                .iter()
                .all(Signature::has_implicit_positional_receiver_annotation))
    }

    async fn from_signature(
        &self,
        callable_type: Type<'db>,
        signature: &CallableSignature<'db>,
        receiver: Type<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(CallableBinding::from_signature(callable_type, signature)
            .with_bound_type(receiver)
            .into())
    }

    async fn bake_receiver(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
    ) -> Result<(), Self::Error> {
        for binding in bindings.iter_flat_mut() {
            binding.bake_bound_type_into_overloads(db, env);
        }
        Ok(())
    }

    async fn empty_overloads(&self) -> Result<BoundMethodOverloads<'db>, Self::Error> {
        Ok(SmallVec::new())
    }

    async fn receiver_determines_typevar(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &Signature<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(signature.has_receiver_determined_method_typevar(db, env))
    }

    async fn specialize_receiver(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &Signature<'db>,
        method: BoundMethodType<'db>,
        receiver: Type<'db>,
    ) -> Result<Option<CallableSignature<'db>>, Self::Error> {
        Ok(signature.specialize_for_bound_receiver(db, env, receiver, method.typing_self_type(db)))
    }

    async fn append_overloads(
        &self,
        output: &mut BoundMethodOverloads<'db>,
        signatures: &[Signature<'db>],
    ) -> Result<(), Self::Error> {
        output.extend(signatures.iter().cloned());
        Ok(())
    }

    async fn finish_overloads(
        &self,
        callable_type: Type<'db>,
        receiver: Type<'db>,
        overloads: BoundMethodOverloads<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(CallableBinding::from_overloads(callable_type, overloads)
            .with_bound_type(receiver)
            .into())
    }

    async fn upcast(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        callable_type: Type<'db>,
        method: BoundMethodType<'db>,
        unknown_is_recovery: bool,
    ) -> Result<Bindings<'db>, Self::Error> {
        Ok(method
            .func(db)
            .try_upcast_to_callable_from_descriptor(
                db,
                env,
                self.guard,
                DescriptorOrigin {
                    return_contains_recursive_recovery: unknown_is_recovery,
                    ..DescriptorOrigin::default()
                },
            )
            .map(|callables| {
                // Retain the receiver parameter so ordinary argument checking can
                // validate the captured class as well as the explicit arguments.
                Bindings::from_union(
                    callable_type,
                    callables.iter().map(|callable| {
                        CallableBinding::from_overloads(
                            callable_type,
                            callable.signatures(db).overloads.iter().cloned(),
                        )
                        .with_bound_type(method.signature_receiver(db))
                        .into()
                    }),
                )
            })
            .unwrap_or_else(|| CallableBinding::not_callable(callable_type).into()))
    }
}
