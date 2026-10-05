//! Shared constructor member selection and native descriptor resolution.
//!
//! Providers preserve the caller's guard through member and descriptor lookup. Initializer
//! resolution keeps a native bound method separate from a callable returned by `__get__`, because
//! the latter has already selected its receiver.

use std::convert::Infallible;

use super::effects::{ConstructorError, checked_source};
use super::{ConstructorMember, ConstructorMembers, InitializerBinding};
use crate::place::{DefinedPlace, Place, PlaceAndQualifiers, Provenance};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::descriptor::DescriptorRequest;
use crate::types::{
    BoundMethodType, ClassType, DescriptorGetResult, DescriptorOrigin, DynamicType,
    MemberLookupPolicy, Type,
};
use crate::{Db, ProgramEnvironment};

/// Controls whether initializer lookup may reach `object.__init__`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ObjectInitializer {
    Include,
    Exclude,
}

impl ObjectInitializer {
    pub(super) const fn from_include_object(include_object: bool) -> Self {
        if include_object {
            Self::Include
        } else {
            Self::Exclude
        }
    }

    pub(super) const fn policy(self) -> MemberLookupPolicy {
        match self {
            Self::Include => MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            Self::Exclude => MemberLookupPolicy::NO_INSTANCE_FALLBACK
                .union(MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK),
        }
    }
}

/// Dependencies of constructor member lookup, separate from callable expansion and call checking.
/// Every local closure handles only fixed-size values; child operations own all semantic traversal.
pub(in crate::types) trait ConstructorDescriptorEffects<'db> {
    type Error;

    async fn checkpoint(&self) -> Result<(), Self::Error>;

    async fn local<T: Copy>(&self, operation: impl FnOnce() -> T) -> Result<T, Self::Error>;

    async fn descriptor(
        &self,
        request: DescriptorRequest<'db>,
    ) -> Result<Option<DescriptorGetResult<'db>>, Self::Error>;
}

/// Member and initializer children used after the descriptor boundary is available.
pub(in crate::types) trait ConstructorMemberEffects<'db>:
    ConstructorDescriptorEffects<'db>
{
    async fn member(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> Result<ConstructorMember<'db>, Self::Error>;

    async fn new_member(
        &self,
        ty: Type<'db>,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;

    async fn namespace(
        &self,
        ty: Type<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<Place<'db>, Self::Error>;

    async fn function_like(
        &self,
        request: DescriptorRequest<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn bound_function(&self, method: BoundMethodType<'db>) -> Result<Type<'db>, Self::Error>;

    async fn bind_initializer(
        &self,
        members: ConstructorMembers<'db>,
        initializer: Type<'db>,
    ) -> Result<InitializerBinding<'db>, Self::Error>;
}

/// Selects a custom metaclass `__call__`, preserving a specialized or materialized receiver.
pub(in crate::types) async fn metaclass_call_with<'db, E: ConstructorMemberEffects<'db>>(
    members: ConstructorMembers<'db>,
    effects: &E,
) -> Result<ConstructorMember<'db>, E::Error> {
    effects.checkpoint().await?;
    let (lookup_type, receiver, policy) = effects
        .local(|| {
            let lookup_type = Type::from(members.class);
            (
                lookup_type,
                if members.receiver == lookup_type {
                    None
                } else {
                    Some(members.receiver)
                },
                MemberLookupPolicy::NO_INSTANCE_FALLBACK
                    | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            )
        })
        .await?;
    effects
        .member(lookup_type, "__call__", policy, receiver)
        .await
}

/// Resolves the canonical `__new__` member before supplying the constructor's implicit `cls`.
pub(in crate::types) async fn new_method_with<'db, E: ConstructorMemberEffects<'db>>(
    members: ConstructorMembers<'db>,
    effects: &E,
) -> Result<ConstructorMember<'db>, E::Error> {
    effects.checkpoint().await?;
    let lookup_type = effects.local(|| Type::from(members.class)).await?;
    let Some(member) = effects.new_member(lookup_type).await? else {
        return effects.local(ConstructorMember::undefined).await;
    };
    resolve_new_with(members.receiver, member.place, effects).await
}

/// Applies `__new__` descriptor binding without expanding the returned callable.
pub(in crate::types) async fn resolve_new_with<'db, E: ConstructorDescriptorEffects<'db>>(
    receiver: Type<'db>,
    place: Place<'db>,
    effects: &E,
) -> Result<ConstructorMember<'db>, E::Error> {
    effects.checkpoint().await?;
    let Place::Defined(defined) = place else {
        return effects.local(ConstructorMember::undefined).await;
    };
    // If `__new__` itself resolved to `Any`, treat it as absent rather than as a real
    // constructor override. This preserves the known nominal constructor result for
    // subclasses of `Any` while still allowing explicitly typed `__new__` callables
    // returning `Any` to keep their annotated behavior.
    if matches!(defined.ty, Type::Dynamic(DynamicType::Any)) {
        return effects.local(ConstructorMember::undefined).await;
    }
    let request = effects
        .local(|| DescriptorRequest {
            ty: defined.ty,
            instance: None,
            owner: receiver,
        })
        .await?;
    let descriptor = effects.descriptor(request).await?;
    effects
        .local(|| match descriptor {
            Some(descriptor) => ConstructorMember {
                place: Place::Defined(DefinedPlace {
                    ty: descriptor.return_type,
                    provenance: Provenance::Unknown,
                    ..defined
                }),
                origin: descriptor.origin,
            },
            None => ConstructorMember {
                place,
                origin: DescriptorOrigin::default(),
            },
        })
        .await
}

/// Reads the initializer declaration with the requested `object` fallback policy.
pub(in crate::types) async fn raw_initializer_with<'db, E: ConstructorMemberEffects<'db>>(
    members: ConstructorMembers<'db>,
    object: ObjectInitializer,
    effects: &E,
) -> Result<Place<'db>, E::Error> {
    effects.checkpoint().await?;
    let (ty, policy) = effects
        .local(|| (Type::from(members.class), object.policy()))
        .await?;
    effects
        .namespace(ty, members.class, "__init__", policy)
        .await
}

/// Resolves an initializer and retains its declaration's definedness and provenance.
pub(in crate::types) async fn initializer_with<'db, E: ConstructorMemberEffects<'db>>(
    members: ConstructorMembers<'db>,
    object: ObjectInitializer,
    effects: &E,
) -> Result<ConstructorMember<'db>, E::Error> {
    effects.checkpoint().await?;
    match raw_initializer_with(members, object, effects).await? {
        Place::Defined(place) => {
            let initializer = effects.bind_initializer(members, place.ty).await?;
            effects
                .local(|| ConstructorMember {
                    place: Place::Defined(DefinedPlace {
                        ty: initializer
                            .bound_method
                            .map(Type::BoundMethod)
                            .unwrap_or(initializer.callable),
                        ..place
                    }),
                    origin: initializer.origin,
                })
                .await
        }
        Place::Undefined => effects.local(ConstructorMember::undefined).await,
    }
}

/// Binds a native initializer receiver or accepts an already-bound descriptor result.
/// The caller performs eager `Self` substitution after this descriptor decision returns.
pub(in crate::types) async fn resolve_initializer_descriptor_with<
    'db,
    E: ConstructorMemberEffects<'db>,
>(
    members: ConstructorMembers<'db>,
    initializer: Type<'db>,
    effects: &E,
) -> Result<InitializerBinding<'db>, E::Error> {
    effects.checkpoint().await?;
    let request = effects
        .local(|| DescriptorRequest {
            ty: initializer,
            instance: Some(members.instance),
            owner: members.receiver,
        })
        .await?;
    match effects.function_like(request).await? {
        Some(Type::BoundMethod(method)) => {
            let callable = effects.bound_function(method).await?;
            effects
                .local(|| InitializerBinding {
                    callable,
                    bound_method: Some(method),
                    origin: DescriptorOrigin::default(),
                })
                .await
        }
        Some(callable) => {
            effects
                .local(|| InitializerBinding {
                    callable,
                    bound_method: None,
                    origin: DescriptorOrigin::default(),
                })
                .await
        }
        None => {
            let descriptor = effects.descriptor(request).await?;
            effects
                .local(|| InitializerBinding {
                    callable: descriptor
                        .map(|descriptor| descriptor.return_type)
                        .unwrap_or(initializer),
                    bound_method: None,
                    origin: descriptor
                        .map(|descriptor| descriptor.origin)
                        .unwrap_or_default(),
                })
                .await
        }
    }
}

/// Ordinary member dependencies, including the constructor probe's source-read check.
pub(super) struct OrdinaryConstructorMembers<'env, 'guard, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'env ProgramEnvironment<'db>,
    pub(super) guard: Option<&'guard CallableRecursionGuard<'db>>,
}

impl<'db> ConstructorDescriptorEffects<'db> for OrdinaryConstructorMembers<'_, '_, 'db> {
    type Error = ConstructorError;

    async fn checkpoint(&self) -> Result<(), ConstructorError> {
        Ok(())
    }

    async fn local<T: Copy>(&self, operation: impl FnOnce() -> T) -> Result<T, ConstructorError> {
        Ok(operation())
    }

    async fn descriptor(
        &self,
        request: DescriptorRequest<'db>,
    ) -> Result<Option<DescriptorGetResult<'db>>, ConstructorError> {
        checked_source(self.db, || {
            Ok(request.ty.try_call_dunder_get_with_recursion_guard(
                self.db,
                self.env,
                request.instance,
                request.owner,
                self.guard,
            ))
        })
        .map(|descriptor| descriptor.unwrap_or_else(|error| Some(error.fallback())))
    }
}

impl<'db> ConstructorMemberEffects<'db> for OrdinaryConstructorMembers<'_, '_, 'db> {
    async fn member(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> Result<ConstructorMember<'db>, ConstructorError> {
        let member = ty
            .member_lookup_with_recursion_guard(
                self.db, self.env, name, policy, receiver, self.guard,
            )
            .unwrap_or_else(|error| error.fallback_member(self.db));
        Ok(ConstructorMember {
            place: member.member(self.db).place,
            origin: member.descriptor_origin(self.db),
        })
    }

    async fn new_member(
        &self,
        ty: Type<'db>,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, ConstructorError> {
        Ok(ty.lookup_dunder_new(self.db, self.env))
    }

    async fn namespace(
        &self,
        ty: Type<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<Place<'db>, ConstructorError> {
        Ok(ty
            .class_namespace_member(self.db, self.env, class, name, policy)
            .place)
    }

    async fn function_like(
        &self,
        request: DescriptorRequest<'db>,
    ) -> Result<Option<Type<'db>>, ConstructorError> {
        checked_source(self.db, || {
            Ok(request.ty.function_like_dunder_get(
                self.db,
                self.env,
                request.instance,
                Some(request.owner),
            ))
        })
    }

    async fn bound_function(
        &self,
        method: BoundMethodType<'db>,
    ) -> Result<Type<'db>, ConstructorError> {
        Ok(method.func(self.db))
    }

    async fn bind_initializer(
        &self,
        members: ConstructorMembers<'db>,
        initializer: Type<'db>,
    ) -> Result<InitializerBinding<'db>, ConstructorError> {
        members.bind_initializer_with_guard(self.db, self.env, initializer, self.guard)
    }
}

/// Descriptor access for override comparisons, which use the ordinary infallible lookup contract.
pub(super) struct OrdinaryNewDescriptor<'env, 'guard, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'env ProgramEnvironment<'db>,
    pub(super) guard: Option<&'guard CallableRecursionGuard<'db>>,
}

impl<'db> ConstructorDescriptorEffects<'db> for OrdinaryNewDescriptor<'_, '_, 'db> {
    type Error = Infallible;

    async fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    async fn local<T: Copy>(&self, operation: impl FnOnce() -> T) -> Result<T, Infallible> {
        Ok(operation())
    }

    async fn descriptor(
        &self,
        request: DescriptorRequest<'db>,
    ) -> Result<Option<DescriptorGetResult<'db>>, Infallible> {
        Ok(request
            .ty
            .try_call_dunder_get_with_recursion_guard(
                self.db,
                self.env,
                request.instance,
                request.owner,
                self.guard,
            )
            .unwrap_or_else(|error| Some(error.fallback())))
    }
}
