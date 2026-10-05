//! Descriptor `__get__` evaluation with explicit semantic dependencies.

use crate::place::{DefinedPlace, Definedness, Place};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::signatures::effects::legacy_inline;
use crate::types::{
    AttributeKind, DescriptorGetError, DescriptorGetResult, DescriptorOrigin, MemberLookupPolicy,
    Type, descriptor_get_result,
};
use crate::{Db, Program, ProgramEnvironment};

pub(super) mod effects;

#[cfg(feature = "experimental-analysis")]
mod runtime;
#[cfg(feature = "experimental-analysis")]
pub(in crate::types) use runtime::{
    DescriptorDispatchesMemoSchema, register_descriptor_dispatch_values,
    register_descriptor_dispatches_values, register_descriptor_get_call_context_values,
};

#[cfg(test)]
pub(super) mod scheduled_effects;

use effects::{DescriptorEffects, LegacyInlineEffects};

pub(crate) type DescriptorResult<'db> =
    Result<Option<DescriptorGetResult<'db>>, DescriptorGetError<'db>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct DescriptorRequest<'db> {
    pub(crate) ty: Type<'db>,
    pub(crate) instance: Option<Type<'db>>,
    pub(crate) owner: Type<'db>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct DescriptorMemberRequest<'db> {
    pub(crate) ty: Type<'db>,
    pub(crate) policy: MemberLookupPolicy,
}

/// A complete implicit `__get__` invocation, including argument checking and overload selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct DescriptorInvocationRequest<'db> {
    pub(crate) callable: Type<'db>,
    pub(crate) arguments: [Type<'db>; 3],
}

pub(super) fn evaluate_entry<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    request: DescriptorRequest<'db>,
    recursion_guard: Option<&CallableRecursionGuard<'db>>,
) -> DescriptorResult<'db> {
    legacy_inline(evaluate_entry_with_effects(
        db,
        env,
        request,
        &LegacyInlineEffects { recursion_guard },
    ))
}

/// Dependencies used before a descriptor needs a Python `__get__` invocation.
/// Guarded callers can retain their guard at the protocol boundary while sharing native binding.
pub(in crate::types) trait DescriptorEntryEffects<'db> {
    type Error;

    async fn checkpoint_entry(&self) -> Result<(), Self::Error>;

    async fn slot_value_entry(
        &self,
        db: &'db dyn Db,
        descriptor: super::SlotDescriptorType<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn function_like_entry(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn protocol_entry(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<DescriptorResult<'db>, Self::Error>;
}

impl<'db, E: DescriptorEffects<'db>> DescriptorEntryEffects<'db> for E {
    type Error = E::Error;

    async fn checkpoint_entry(&self) -> Result<(), Self::Error> {
        DescriptorEffects::checkpoint(self).await
    }

    async fn slot_value_entry(
        &self,
        db: &'db dyn Db,
        descriptor: super::SlotDescriptorType<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        DescriptorEffects::slot_value(self, db, descriptor).await
    }

    async fn function_like_entry(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        DescriptorEffects::function_like(self, db, env, request).await
    }

    async fn protocol_entry(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<DescriptorResult<'db>, Self::Error> {
        DescriptorEffects::protocol(self, db, env, request).await
    }
}

/// Keeps native descriptor access outside the cached protocol lookup. In particular, native
/// function binding can revisit a protocol receiver; a protocol query's cycle value would leave
/// that receiver unbound if it were applied to the native binding operation.
pub(super) async fn evaluate_entry_with_effects<'db, E: DescriptorEntryEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    request: DescriptorRequest<'db>,
    effects: &E,
) -> Result<DescriptorResult<'db>, E::Error> {
    effects.checkpoint_entry().await?;
    if matches!(request.ty, Type::BoundMethod(_)) {
        // A stored bound method keeps its receiver. In Python 3.13+ its native `__get__`
        // returns the method itself; older versions have no descriptor slot on MethodType.
        return Ok(Ok(None));
    }
    if let Some(return_type) = effects.function_like_entry(db, env, request).await? {
        return Ok(Ok(Some(DescriptorGetResult {
            return_type,
            origin: DescriptorOrigin::default(),
            kind: AttributeKind::NormalOrNonDataDescriptor,
        })));
    }

    // The interpreter returns the descriptor itself on class access and its stored value on
    // instance access; no Python property accessors participate in either operation.
    if let Type::SlotDescriptor(descriptor) = request.ty {
        return Ok(Ok(Some(DescriptorGetResult {
            return_type: if request.instance.is_some() {
                effects.slot_value_entry(db, descriptor).await?
            } else {
                request.ty
            },
            origin: DescriptorOrigin::default(),
            kind: AttributeKind::DataDescriptor,
        })));
    }

    effects.protocol_entry(db, env, request).await
}

pub(super) fn evaluate<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    ty: Type<'db>,
    instance: Option<Type<'db>>,
    owner: Type<'db>,
    recursion_guard: Option<&CallableRecursionGuard<'db>>,
) -> DescriptorResult<'db> {
    legacy_inline(evaluate_with_effects(
        db,
        &ProgramEnvironment::from_program(program),
        DescriptorRequest {
            ty,
            instance,
            owner,
        },
        &LegacyInlineEffects { recursion_guard },
    ))
}

/// Effect failures are unfinished analysis. The inner result separately retains Python descriptor
/// absence or an invalid call with its declared return type, attribute kind and diagnostic context.
pub(super) async fn evaluate_with_effects<'db, E: DescriptorEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    request: DescriptorRequest<'db>,
    effects: &E,
) -> Result<DescriptorResult<'db>, E::Error> {
    effects.checkpoint().await?;
    if let Some(fallback) = request.ty.materialized_divergent_fallback() {
        return effects
            .descriptor(
                db,
                env,
                DescriptorRequest {
                    ty: fallback,
                    ..request
                },
            )
            .await;
    }

    if let Some(dynamic) = request.ty.dynamic_descriptor_type() {
        return Ok(Ok(Some(DescriptorGetResult {
            return_type: dynamic,
            origin: DescriptorOrigin::default(),
            kind: AttributeKind::DataDescriptor,
        })));
    }

    if let Some(union) = effects.union_like(db, env, request.ty).await? {
        let (mut return_types, elements) = effects.union_parts(db, env, union).await?;
        let mut requests = elements
            .iter()
            .map(|&ty| DescriptorRequest { ty, ..request });
        effects
            .declare_descriptors(db, env, requests.clone())
            .await?;
        let mut error = None;
        let mut any_descriptor = false;
        let mut all_data_descriptors = true;
        let mut origin = DescriptorOrigin::default();
        while let Some(child) = effects.next_descriptor(&mut requests).await? {
            let result = effects
                .descriptor(db, env, child)
                .await?
                .unwrap_or_else(|failure| {
                    error = error.or(Some(failure.context));
                    Some(failure.fallback())
                });
            let return_type = if let Some(result) = result {
                origin = effects.merge_origins(db, origin, result.origin).await?;
                any_descriptor = true;
                all_data_descriptors &= result.kind.is_data();
                result.return_type
            } else {
                all_data_descriptors = false;
                child.ty
            };
            // Normalize each result before consuming the next alternative. Independent child
            // evaluations may already be running, but their results are combined in source order.
            return_types = effects.union_add(return_types, return_type).await?;
        }
        return Ok(if any_descriptor {
            descriptor_get_result(
                effects.union_build(return_types).await?,
                origin,
                if all_data_descriptors {
                    AttributeKind::DataDescriptor
                } else {
                    AttributeKind::NormalOrNonDataDescriptor
                },
                error,
            )
        } else {
            Ok(None)
        });
    }

    if let Type::Intersection(intersection) = request.ty {
        let (mut return_types, elements) = effects.intersection_parts(db, env, intersection).await?;
        let mut requests = elements
            .iter()
            .map(|&ty| DescriptorRequest { ty, ..request });
        effects
            .declare_descriptors(db, env, requests.clone())
            .await?;
        let mut origin = DescriptorOrigin::default();
        let mut error = None;
        let mut any_descriptor = false;
        while let Some(child) = effects.next_descriptor(&mut requests).await? {
            let result = effects
                .descriptor(db, env, child)
                .await?
                .unwrap_or_else(|failure| {
                    error = error.or(Some(failure.context));
                    Some(failure.fallback())
                });
            let (return_type, element_origin) = if let Some(result) = result {
                any_descriptor = true;
                (result.return_type, result.origin)
            } else {
                (child.ty, DescriptorOrigin::default())
            };
            return_types = effects.intersection_add(return_types, return_type).await?;
            origin = effects.merge_origins(db, origin, element_origin).await?;
        }
        return Ok(if any_descriptor {
            descriptor_get_result(
                effects.intersection_build(return_types).await?,
                origin,
                // TODO: Discover data descriptors in intersections without decomposing
                // the descriptor return type into an unsound intersection.
                AttributeKind::NormalOrNonDataDescriptor,
                error,
            )
        } else {
            Ok(None)
        });
    }

    let concrete = effects
        .class_member(
            db,
            env,
            DescriptorMemberRequest {
                ty: request.ty,
                policy: MemberLookupPolicy::REQUIRE_CONCRETE,
            },
        )
        .await?;
    let Place::Defined(DefinedPlace { ty: descr_get, .. }) = concrete else {
        return Ok(Ok(None));
    };
    // A recursive member lookup can yield the internal cycle marker. It does not
    // represent a concrete descriptor method and must not escape through the access.
    if descr_get.is_divergent() {
        return Ok(Ok(None));
    }

    // Descriptor special-method lookup checks the descriptor's type, so instance storage
    // cannot shadow `__get__`. Dynamic MRO entries still participate in the lookup.
    let place = effects
        .class_member(
            db,
            env,
            DescriptorMemberRequest {
                ty: request.ty,
                policy: MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            },
        )
        .await?;
    let Place::Defined(DefinedPlace {
        ty: descr_get,
        definedness,
        ..
    }) = place
    else {
        return Ok(Ok(None));
    };
    let instance_ty = match request.instance {
        Some(instance) => instance,
        None => effects.none_type(db, env).await?,
    };
    let kind = if effects.data_descriptor(db, env, request.ty).await? {
        AttributeKind::DataDescriptor
    } else {
        AttributeKind::NormalOrNonDataDescriptor
    };
    let call = effects.call_context(db, request, descr_get).await?;
    let invocation = DescriptorInvocationRequest {
        callable: descr_get,
        arguments: [request.ty, instance_ty, request.owner],
    };
    let (bindings, error) = match effects.invoke(db, env, invocation).await? {
        Ok(bindings) => (bindings, None),
        Err(error) => (*error.1, Some(call)),
    };
    let origin = effects
        .bindings_origin(db, env, &bindings, &invocation.arguments)
        .await?;
    let return_type = effects.bindings_return_type(db, env, &bindings).await?;
    let return_type = if definedness == Definedness::AlwaysDefined {
        return_type
    } else {
        effects.union_pair(db, env, return_type, request.ty).await?
    };
    Ok(descriptor_get_result(return_type, origin, kind, error))
}
