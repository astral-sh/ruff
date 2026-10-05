//! Precedence between a class namespace and its metaclass's instance storage.

use super::member_source::{InlineMemberSourceEffects, runtime_binding_absent_sync};
use std::convert::Infallible;

use super::ClassMetaclass;
use crate::place::{DefinedPlace, Definedness, Place, PlaceAndQualifiers};
use crate::types::class_base::ClassBase;
use crate::types::mro::MroIterator;
use crate::types::{ClassType, MemberLookupPolicy, Type, TypeQualifiers};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy)]
pub(in crate::types) struct NamespaceLookupRequest<'a, 'db> {
    pub(in crate::types) class: ClassType<'db>,
    pub(in crate::types) name: &'a str,
    pub(in crate::types) policy: MemberLookupPolicy,
}

/// The initial MRO member belongs to the lookup type, which can differ from the nominal class.
/// Each dependency is requested only after the preceding lookup has completed.
pub(in crate::types) enum NamespaceLookupStep<'a, 'db> {
    Metaclass(PendingNamespaceMetaclass<'a, 'db>),
    MetaclassStorage(PendingNamespaceMetaclassStorage<'a, 'db>),
    OwnMember(PendingNamespaceOwnMember<'a, 'db>),
    RuntimeBinding(PendingNamespaceRuntimeBinding<'a, 'db>),
    InheritedMember(PendingNamespaceInheritedMember<'a, 'db>),
    MetaclassFallback(PendingNamespaceMetaclassFallback<'db>),
    InheritedFallback(PendingNamespaceInheritedFallback<'db>),
    Complete(PlaceAndQualifiers<'db>),
    DynamicInstanceFallback(PlaceAndQualifiers<'db>),
}

impl<'a, 'db> NamespaceLookupStep<'a, 'db> {
    #[inline]
    pub(in crate::types) fn start(
        request: NamespaceLookupRequest<'a, 'db>,
        class_member: PlaceAndQualifiers<'db>,
    ) -> Self {
        Self::Metaclass(PendingNamespaceMetaclass {
            request,
            class_member,
        })
    }
}

pub(in crate::types) struct PendingNamespaceMetaclass<'a, 'db> {
    request: NamespaceLookupRequest<'a, 'db>,
    class_member: PlaceAndQualifiers<'db>,
}

impl<'a, 'db> PendingNamespaceMetaclass<'a, 'db> {
    #[inline]
    pub(in crate::types) fn class(&self) -> ClassType<'db> {
        self.request.class
    }

    #[inline]
    pub(in crate::types) fn resume(
        self,
        metaclass: Option<ClassType<'db>>,
    ) -> NamespaceLookupStep<'a, 'db> {
        match metaclass {
            Some(metaclass) => {
                NamespaceLookupStep::MetaclassStorage(PendingNamespaceMetaclassStorage {
                    lookup: self,
                    metaclass,
                })
            }
            None => NamespaceLookupStep::Complete(self.class_member),
        }
    }
}

pub(in crate::types) struct PendingNamespaceMetaclassStorage<'a, 'db> {
    lookup: PendingNamespaceMetaclass<'a, 'db>,
    metaclass: ClassType<'db>,
}

impl<'a, 'db> PendingNamespaceMetaclassStorage<'a, 'db> {
    #[inline]
    pub(in crate::types) fn request(&self) -> (ClassType<'db>, &'a str) {
        (self.metaclass, self.lookup.request.name)
    }

    #[inline]
    pub(in crate::types) fn resume(
        self,
        metaclass_member: PlaceAndQualifiers<'db>,
    ) -> NamespaceLookupStep<'a, 'db> {
        if metaclass_member.is_undefined() {
            return NamespaceLookupStep::Complete(self.lookup.class_member);
        }
        NamespaceLookupStep::OwnMember(PendingNamespaceOwnMember {
            request: self.lookup.request,
            metaclass_member,
            metaclass_member_is_implicit: metaclass_member
                .qualifiers
                .contains(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE),
        })
    }
}

pub(in crate::types) struct PendingNamespaceOwnMember<'a, 'db> {
    request: NamespaceLookupRequest<'a, 'db>,
    metaclass_member: PlaceAndQualifiers<'db>,
    metaclass_member_is_implicit: bool,
}

impl<'a, 'db> PendingNamespaceOwnMember<'a, 'db> {
    #[inline]
    pub(in crate::types) fn request(&self) -> NamespaceLookupRequest<'a, 'db> {
        self.request
    }

    #[inline]
    pub(in crate::types) fn resume(
        self,
        own_member: PlaceAndQualifiers<'db>,
    ) -> NamespaceLookupStep<'a, 'db> {
        if own_member.is_class_var() {
            NamespaceLookupStep::InheritedMember(PendingNamespaceInheritedMember {
                lookup: self,
                own_member,
            })
        } else {
            NamespaceLookupStep::RuntimeBinding(PendingNamespaceRuntimeBinding {
                lookup: self,
                own_member,
            })
        }
    }
}

pub(in crate::types) struct PendingNamespaceRuntimeBinding<'a, 'db> {
    lookup: PendingNamespaceOwnMember<'a, 'db>,
    own_member: PlaceAndQualifiers<'db>,
}

impl<'a, 'db> PendingNamespaceRuntimeBinding<'a, 'db> {
    #[inline]
    pub(in crate::types) fn request(&self) -> NamespaceLookupRequest<'a, 'db> {
        self.lookup.request
    }

    #[inline]
    pub(in crate::types) fn resume(self, binding_is_absent: bool) -> NamespaceLookupStep<'a, 'db> {
        // A non-ClassVar declaration-only member describes instance storage but does not add a
        // value to the class namespace.
        let own_member = if binding_is_absent {
            PlaceAndQualifiers::default()
        } else {
            self.own_member
        };
        NamespaceLookupStep::InheritedMember(PendingNamespaceInheritedMember {
            lookup: self.lookup,
            own_member,
        })
    }
}

pub(in crate::types) struct PendingNamespaceInheritedMember<'a, 'db> {
    lookup: PendingNamespaceOwnMember<'a, 'db>,
    own_member: PlaceAndQualifiers<'db>,
}

impl<'a, 'db> PendingNamespaceInheritedMember<'a, 'db> {
    #[inline]
    pub(in crate::types) fn request(&self) -> NamespaceLookupRequest<'a, 'db> {
        self.lookup.request
    }

    #[inline]
    pub(in crate::types) fn resume(
        self,
        inherited_member: PlaceAndQualifiers<'db>,
    ) -> NamespaceLookupStep<'a, 'db> {
        let metaclass_member = if self.lookup.metaclass_member_is_implicit {
            Type::with_definedness(self.lookup.metaclass_member, Definedness::PossiblyUndefined)
        } else {
            self.lookup.metaclass_member
        };
        NamespaceLookupStep::MetaclassFallback(PendingNamespaceMetaclassFallback {
            own_member: self.own_member,
            metaclass_member,
            inherited_member,
            policy: self.lookup.request.policy,
            metaclass_member_is_implicit: self.lookup.metaclass_member_is_implicit,
        })
    }
}

/// Combining members can promote their public types and normalize unions.
pub(in crate::types) struct PendingNamespaceMetaclassFallback<'db> {
    own_member: PlaceAndQualifiers<'db>,
    metaclass_member: PlaceAndQualifiers<'db>,
    inherited_member: PlaceAndQualifiers<'db>,
    policy: MemberLookupPolicy,
    metaclass_member_is_implicit: bool,
}

impl<'db> PendingNamespaceMetaclassFallback<'db> {
    #[inline]
    pub(in crate::types) fn request(&self) -> (PlaceAndQualifiers<'db>, PlaceAndQualifiers<'db>) {
        (self.own_member, self.metaclass_member)
    }

    #[inline]
    pub(in crate::types) fn resume<'a>(
        self,
        member: PlaceAndQualifiers<'db>,
    ) -> NamespaceLookupStep<'a, 'db> {
        NamespaceLookupStep::InheritedFallback(PendingNamespaceInheritedFallback {
            member,
            inherited_member: self.inherited_member,
            policy: self.policy,
            metaclass_member_is_implicit: self.metaclass_member_is_implicit,
        })
    }
}

pub(in crate::types) struct PendingNamespaceInheritedFallback<'db> {
    member: PlaceAndQualifiers<'db>,
    inherited_member: PlaceAndQualifiers<'db>,
    policy: MemberLookupPolicy,
    metaclass_member_is_implicit: bool,
}

impl<'db> PendingNamespaceInheritedFallback<'db> {
    #[inline]
    pub(in crate::types) fn request(&self) -> (PlaceAndQualifiers<'db>, PlaceAndQualifiers<'db>) {
        (self.member, self.inherited_member)
    }

    #[inline]
    pub(in crate::types) fn resume<'a>(
        self,
        member: PlaceAndQualifiers<'db>,
    ) -> NamespaceLookupStep<'a, 'db> {
        let member = if self.metaclass_member_is_implicit {
            // Preserve the existing convention that an inferred instance member is assumed to be
            // available even when no lower-precedence fallback exists.
            Type::with_definedness(member, Definedness::AlwaysDefined)
        } else {
            member
        };
        if self.policy.no_instance_fallback() || self.policy.require_concrete() {
            NamespaceLookupStep::Complete(member)
        } else {
            NamespaceLookupStep::DynamicInstanceFallback(member)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum NamespaceLookupWork {
    Begin,
    Metaclass,
    MetaclassStorage,
    OwnMember,
    RuntimeBinding,
    InheritedMember,
    MetaclassFallback,
    InheritedFallback,
    DynamicStart,
    DynamicAdvance,
    DynamicClassify,
    DescriptorCheck,
    DescriptorFilter,
    DynamicFallback,
    Publish,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

pub(in crate::types) trait NamespaceLookupEffects<'db>: sealed::Sealed {
    type Error;
    type DynamicCursor;

    async fn checkpoint(&self, work: NamespaceLookupWork) -> Result<(), Self::Error>;
    async fn find_in_mro(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;
    async fn inferred_metaclass(
        &self,
        class: ClassType<'db>,
    ) -> Result<ClassMetaclass<'db>, Self::Error>;
    async fn for_inheritance(
        &self,
        metaclass: ClassMetaclass<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    async fn instance_approximation(&self, ty: Type<'db>)
    -> Result<Option<Type<'db>>, Self::Error>;
    async fn nominal_class(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
    async fn instance_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn own_member(
        &self,
        request: NamespaceLookupRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn runtime_binding_absent(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    async fn inherited_member(
        &self,
        request: NamespaceLookupRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn fall_back_to(
        &self,
        member: PlaceAndQualifiers<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn start_dynamic_mro(
        &self,
        class: ClassType<'db>,
    ) -> Result<Self::DynamicCursor, Self::Error>;
    async fn next_dynamic_base(
        &self,
        cursor: &mut Self::DynamicCursor,
    ) -> Result<Option<ClassBase<'db>>, Self::Error>;
    async fn may_be_data_descriptor(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
    async fn filter_possible_data_descriptors(
        &self,
        ty: Type<'db>,
    ) -> Result<(Type<'db>, bool), Self::Error>;
}

pub(in crate::types) trait SynchronousNamespaceLookupEffects<'db>:
    sealed::Sealed
{
    type Error;
    type DynamicCursor;

    fn checkpoint(&self, work: NamespaceLookupWork) -> Result<(), Self::Error>;
    fn find_in_mro(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;
    fn inferred_metaclass(&self, class: ClassType<'db>)
    -> Result<ClassMetaclass<'db>, Self::Error>;
    fn for_inheritance(&self, metaclass: ClassMetaclass<'db>) -> Result<Type<'db>, Self::Error>;
    fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
    fn nominal_class(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
    fn instance_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    fn own_member(
        &self,
        request: NamespaceLookupRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    fn runtime_binding_absent(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    fn inherited_member(
        &self,
        request: NamespaceLookupRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    fn fall_back_to(
        &self,
        member: PlaceAndQualifiers<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    fn start_dynamic_mro(&self, class: ClassType<'db>) -> Result<Self::DynamicCursor, Self::Error>;
    fn next_dynamic_base(
        &self,
        cursor: &mut Self::DynamicCursor,
    ) -> Result<Option<ClassBase<'db>>, Self::Error>;
    fn may_be_data_descriptor(&self, ty: Type<'db>) -> Result<bool, Self::Error>;
    fn filter_possible_data_descriptors(
        &self,
        ty: Type<'db>,
    ) -> Result<(Type<'db>, bool), Self::Error>;
}

#[ty_mapping_probe_macros::dual_namespace_lookup]
pub(in crate::types) async fn namespace_lookup_with<'a, 'db, E: NamespaceLookupEffects<'db>>(
    lookup_ty: Type<'db>,
    request: NamespaceLookupRequest<'a, 'db>,
    effects: &E,
) -> Result<PlaceAndQualifiers<'db>, E::Error> {
    let NamespaceLookupRequest {
        class,
        name,
        policy,
    } = request;
    effects.checkpoint(NamespaceLookupWork::Begin).await?;
    let class_attr = effects
        .find_in_mro(lookup_ty, name, policy)
        .await?
        .expect("The meta-type of an instance-like type should always have an MRO");
    let mut step = NamespaceLookupStep::start(
        NamespaceLookupRequest {
            class,
            name,
            policy,
        },
        class_attr,
    );
    let class_member = loop {
        step = match step {
            NamespaceLookupStep::Metaclass(pending) => {
                effects.checkpoint(NamespaceLookupWork::Metaclass).await?;
                let metaclass = effects.inferred_metaclass(pending.class()).await?;
                let metaclass = effects.for_inheritance(metaclass).await?;
                let metaclass = match effects.instance_approximation(metaclass).await? {
                    Some(metaclass) => effects.nominal_class(metaclass).await?,
                    None => None,
                };
                pending.resume(metaclass)
            }
            NamespaceLookupStep::MetaclassStorage(pending) => {
                effects
                    .checkpoint(NamespaceLookupWork::MetaclassStorage)
                    .await?;
                let (metaclass, name) = pending.request();
                pending.resume(effects.instance_member(metaclass, name).await?)
            }
            NamespaceLookupStep::OwnMember(pending) => {
                effects.checkpoint(NamespaceLookupWork::OwnMember).await?;
                let member = effects.own_member(pending.request()).await?;
                pending.resume(member)
            }
            NamespaceLookupStep::RuntimeBinding(pending) => {
                effects
                    .checkpoint(NamespaceLookupWork::RuntimeBinding)
                    .await?;
                let NamespaceLookupRequest { class, name, .. } = pending.request();
                let binding_is_absent = effects.runtime_binding_absent(class, name).await?;
                pending.resume(binding_is_absent)
            }
            NamespaceLookupStep::InheritedMember(pending) => {
                effects
                    .checkpoint(NamespaceLookupWork::InheritedMember)
                    .await?;
                let member = effects.inherited_member(pending.request()).await?;
                pending.resume(member)
            }
            NamespaceLookupStep::MetaclassFallback(pending) => {
                effects
                    .checkpoint(NamespaceLookupWork::MetaclassFallback)
                    .await?;
                let (member, fallback) = pending.request();
                pending.resume(effects.fall_back_to(member, fallback).await?)
            }
            NamespaceLookupStep::InheritedFallback(pending) => {
                effects
                    .checkpoint(NamespaceLookupWork::InheritedFallback)
                    .await?;
                let (member, fallback) = pending.request();
                pending.resume(effects.fall_back_to(member, fallback).await?)
            }
            NamespaceLookupStep::Complete(member) => {
                effects.checkpoint(NamespaceLookupWork::Publish).await?;
                return Ok(member);
            }
            NamespaceLookupStep::DynamicInstanceFallback(member) => break member,
        };
    };
    effects
        .checkpoint(NamespaceLookupWork::DynamicStart)
        .await?;
    let mut cursor = effects.start_dynamic_mro(class).await?;
    let dynamic_instance_type = loop {
        effects
            .checkpoint(NamespaceLookupWork::DynamicAdvance)
            .await?;
        let Some(base) = effects.next_dynamic_base(&mut cursor).await? else {
            effects.checkpoint(NamespaceLookupWork::Publish).await?;
            return Ok(class_member);
        };
        effects
            .checkpoint(NamespaceLookupWork::DynamicClassify)
            .await?;
        match base {
            ClassBase::Any | ClassBase::Dynamic(_) | ClassBase::Divergent(_) => {
                break Type::from(base);
            }
            _ => {}
        }
    };
    let dynamic_instance_fallback = Place::bound(dynamic_instance_type).into();

    // A dynamic base can provide arbitrary instance storage that shadows non-data class
    // attributes. Preserve only the data-descriptor alternatives before falling back to the
    // actual dynamic type.
    let Some(class_member_ty) = class_member.ignore_possibly_undefined() else {
        effects.checkpoint(NamespaceLookupWork::Publish).await?;
        return Ok(dynamic_instance_fallback);
    };
    effects
        .checkpoint(NamespaceLookupWork::DescriptorCheck)
        .await?;
    if !effects.may_be_data_descriptor(class_member_ty).await? {
        effects.checkpoint(NamespaceLookupWork::Publish).await?;
        return Ok(dynamic_instance_fallback);
    }
    let PlaceAndQualifiers {
        place: Place::Defined(declaration),
        qualifiers,
    } = class_member
    else {
        effects.checkpoint(NamespaceLookupWork::Publish).await?;
        return Ok(dynamic_instance_fallback);
    };
    effects
        .checkpoint(NamespaceLookupWork::DescriptorFilter)
        .await?;
    let (descriptor_ty, all_arms_are_possible_data_descriptors) = effects
        .filter_possible_data_descriptors(declaration.ty)
        .await?;
    let member = Place::Defined(DefinedPlace {
        ty: descriptor_ty,
        definedness: if all_arms_are_possible_data_descriptors {
            declaration.definedness
        } else {
            Definedness::PossiblyUndefined
        },
        ..declaration
    })
    .with_qualifiers(qualifiers);
    effects
        .checkpoint(NamespaceLookupWork::DynamicFallback)
        .await?;
    let result = effects
        .fall_back_to(member, dynamic_instance_fallback)
        .await?;
    effects.checkpoint(NamespaceLookupWork::Publish).await?;
    Ok(result)
}

pub(in crate::types) struct InlineNamespaceLookupEffects<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}

impl<'env, 'db> InlineNamespaceLookupEffects<'env, 'db> {
    pub(in crate::types) fn new(db: &'db dyn Db, env: &'env ProgramEnvironment<'db>) -> Self {
        Self { db, env }
    }
}

impl sealed::Sealed for InlineNamespaceLookupEffects<'_, '_> {}

impl<'db> SynchronousNamespaceLookupEffects<'db> for InlineNamespaceLookupEffects<'_, 'db> {
    type Error = Infallible;
    type DynamicCursor = MroIterator<'db>;

    fn checkpoint(&self, _: NamespaceLookupWork) -> Result<(), Infallible> {
        Ok(())
    }
    fn find_in_mro(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Infallible> {
        Ok(ty.find_name_in_mro_with_policy(self.db, self.env, name, policy))
    }
    fn inferred_metaclass(&self, class: ClassType<'db>) -> Result<ClassMetaclass<'db>, Infallible> {
        Ok(class.inferred_metaclass(self.db))
    }
    fn for_inheritance(&self, metaclass: ClassMetaclass<'db>) -> Result<Type<'db>, Infallible> {
        Ok(metaclass.for_inheritance(self.db, self.env))
    }
    fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Infallible> {
        Ok(ty.to_instance_approximation(self.db, self.env))
    }
    fn nominal_class(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Infallible> {
        Ok(ty.nominal_class(self.db, self.env))
    }
    fn instance_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(class.instance_member(self.db, self.env, name))
    }
    fn own_member(
        &self,
        request: NamespaceLookupRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        let NamespaceLookupRequest {
            class,
            name,
            policy,
        } = request;
        Ok(class.class_literal(self.db).class_member_from_mro(
            self.db,
            self.env,
            name,
            policy,
            class.iter_mro(self.db).take(1),
        ))
    }
    fn runtime_binding_absent(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<bool, Infallible> {
        let Some((class, _)) = class.static_class_literal(self.db) else {
            return Ok(false);
        };
        runtime_binding_absent_sync(
            self.env,
            class.body_scope(self.db),
            name,
            &InlineMemberSourceEffects::new(self.db),
        )
    }

    fn inherited_member(
        &self,
        request: NamespaceLookupRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        let NamespaceLookupRequest {
            class,
            name,
            policy,
        } = request;
        Ok(class.class_literal(self.db).class_member_from_mro(
            self.db,
            self.env,
            name,
            policy,
            class.iter_mro(self.db).skip(1),
        ))
    }
    fn fall_back_to(
        &self,
        member: PlaceAndQualifiers<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(member.or_fall_back_to(self.db, self.env, || fallback))
    }
    fn start_dynamic_mro(&self, class: ClassType<'db>) -> Result<MroIterator<'db>, Infallible> {
        Ok(class.iter_mro(self.db))
    }
    fn next_dynamic_base(
        &self,
        cursor: &mut MroIterator<'db>,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(cursor.next())
    }
    fn may_be_data_descriptor(&self, ty: Type<'db>) -> Result<bool, Infallible> {
        Ok(ty.may_be_data_descriptor(self.db, self.env))
    }
    fn filter_possible_data_descriptors(
        &self,
        ty: Type<'db>,
    ) -> Result<(Type<'db>, bool), Infallible> {
        let mut all_arms_are_possible_data_descriptors = true;
        let descriptor_ty = ty.filter_union(self.db, self.env, |ty| {
            let is_possible_data_descriptor = ty.may_be_data_descriptor(self.db, self.env);
            all_arms_are_possible_data_descriptors &= is_possible_data_descriptor;
            is_possible_data_descriptor
        });
        Ok((descriptor_ty, all_arms_are_possible_data_descriptors))
    }
}
