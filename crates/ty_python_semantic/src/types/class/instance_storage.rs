//! Shared class-family storage lookup and its static MRO entry.

use std::convert::Infallible;

use super::member_source::{InlineMemberSourceEffects, static_own_instance_member_sync};
use super::{
    ClassInstanceFlags, ClassLiteral, ClassType, DynamicClassLiteral, DynamicEnumLiteral,
    DynamicNamedTupleLiteral, InstanceMemberResult, KnownClass, MroLookup, StaticClassLiteral,
    lacks_instance_storage_sync,
};
use crate::place::{Place, PlaceAndQualifiers};
use crate::types::generics::Specialization;
use crate::types::member::Member;
use crate::types::{GenericAlias, Type, TypeContext, TypeMapping};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum InstanceStorageWork {
    Begin,
    Dispatch,
    AliasOrigin,
    AliasSpecialization,
    TypedDict,
    DynamicMember,
    OwnDynamicMember,
    StaticMember,
    OwnStaticMember,
    OwnerSpecialization,
    StorageCheck,
    Mro,
    TypedDictFallback,
    Header,
    InstanceFlags,
    Publish,
}

pub(in crate::types) trait ClassInstanceStorageEffects<'db>:
    sealed::Sealed
{
    async fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error>;
    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;
    type Error;
    async fn checkpoint(&self, work: InstanceStorageWork) -> Result<(), Self::Error>;
    async fn dynamic_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: DynamicClassLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn named_tuple_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: DynamicNamedTupleLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn enum_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: DynamicEnumLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn dynamic_own_instance_member(
        &self,
        class: DynamicClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    async fn named_tuple_own_instance_member(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    async fn enum_own_instance_member(
        &self,
        class: DynamicEnumLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    async fn static_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn static_own_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    async fn specialize_place(
        &self,
        member: PlaceAndQualifiers<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    async fn specialize_member(
        &self,
        member: Member<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Member<'db>, Self::Error>;
}

pub(in crate::types) trait StaticInstanceStorageEffects<'db>:
    sealed::Sealed
{
    type Error;
    async fn storage_checkpoint(&self, work: InstanceStorageWork) -> Result<(), Self::Error>;
    async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    async fn lacks_instance_storage(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    async fn mro_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Result<InstanceMemberResult<'db>, Self::Error>;
    async fn typed_dict_fallback(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
}

pub(in crate::types) trait SynchronousClassInstanceStorageEffects<'db>:
    sealed::Sealed
{
    fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error>;
    fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;
    type Error;
    fn checkpoint(&self, work: InstanceStorageWork) -> Result<(), Self::Error>;
    fn dynamic_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: DynamicClassLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    fn named_tuple_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: DynamicNamedTupleLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    fn enum_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: DynamicEnumLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    fn dynamic_own_instance_member(
        &self,
        class: DynamicClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    fn named_tuple_own_instance_member(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    fn enum_own_instance_member(
        &self,
        class: DynamicEnumLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    fn static_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    fn static_own_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;
    fn specialize_place(
        &self,
        member: PlaceAndQualifiers<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    fn specialize_member(
        &self,
        member: Member<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Member<'db>, Self::Error>;
}

pub(in crate::types) trait SynchronousStaticInstanceStorageEffects<'db>:
    sealed::Sealed
{
    type Error;
    fn storage_checkpoint(&self, work: InstanceStorageWork) -> Result<(), Self::Error>;
    fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    fn lacks_instance_storage(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error>;
    fn mro_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Result<InstanceMemberResult<'db>, Self::Error>;
    fn typed_dict_fallback(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_instance_storage]
pub(in crate::types) async fn class_instance_member_with<
    'a,
    'db,
    E: ClassInstanceStorageEffects<'db>,
>(
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
    name: &'a str,
    effects: &E,
) -> Result<PlaceAndQualifiers<'db>, E::Error> {
    effects.checkpoint(InstanceStorageWork::Begin).await?;
    effects.checkpoint(InstanceStorageWork::Dispatch).await?;
    let result = match class {
        ClassType::NonGeneric(ClassLiteral::Dynamic(class)) => {
            effects
                .checkpoint(InstanceStorageWork::DynamicMember)
                .await?;
            effects.dynamic_instance_member(env, class, name).await?
        }
        ClassType::NonGeneric(ClassLiteral::DynamicNamedTuple(class)) => {
            effects
                .checkpoint(InstanceStorageWork::DynamicMember)
                .await?;
            effects
                .named_tuple_instance_member(env, class, name)
                .await?
        }
        ClassType::NonGeneric(ClassLiteral::DynamicTypedDict(_)) => PlaceAndQualifiers::default(),
        ClassType::NonGeneric(ClassLiteral::DynamicEnum(class)) => {
            effects
                .checkpoint(InstanceStorageWork::DynamicMember)
                .await?;
            effects.enum_instance_member(env, class, name).await?
        }
        ClassType::NonGeneric(ClassLiteral::Static(class)) => {
            effects.checkpoint(InstanceStorageWork::TypedDict).await?;
            if effects.is_typed_dict(class).await? {
                Place::Undefined.into()
            } else {
                effects
                    .checkpoint(InstanceStorageWork::StaticMember)
                    .await?;
                effects
                    .static_instance_member(env, class, None, name)
                    .await?
            }
        }
        ClassType::Generic(generic) => {
            effects.checkpoint(InstanceStorageWork::AliasOrigin).await?;
            let class = effects.alias_origin(generic).await?;
            effects
                .checkpoint(InstanceStorageWork::AliasSpecialization)
                .await?;
            let specialization = Some(effects.alias_specialization(generic).await?);
            effects.checkpoint(InstanceStorageWork::TypedDict).await?;
            if effects.is_typed_dict(class).await? {
                Place::Undefined.into()
            } else {
                effects
                    .checkpoint(InstanceStorageWork::StaticMember)
                    .await?;
                let member = effects
                    .static_instance_member(env, class, specialization, name)
                    .await?;
                effects
                    .checkpoint(InstanceStorageWork::OwnerSpecialization)
                    .await?;
                effects.specialize_place(member, specialization).await?
            }
        }
    };
    effects.checkpoint(InstanceStorageWork::Publish).await?;
    Ok(result)
}

#[ty_mapping_probe_macros::dual_instance_storage]
pub(in crate::types) async fn class_own_instance_member_with<
    'a,
    'db,
    E: ClassInstanceStorageEffects<'db>,
>(
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
    name: &'a str,
    effects: &E,
) -> Result<Member<'db>, E::Error> {
    effects.checkpoint(InstanceStorageWork::Begin).await?;
    effects.checkpoint(InstanceStorageWork::Dispatch).await?;
    let result = match class {
        ClassType::NonGeneric(ClassLiteral::Dynamic(class)) => {
            effects
                .checkpoint(InstanceStorageWork::OwnDynamicMember)
                .await?;
            effects.dynamic_own_instance_member(class, name).await?
        }
        ClassType::NonGeneric(ClassLiteral::DynamicNamedTuple(class)) => {
            effects
                .checkpoint(InstanceStorageWork::OwnDynamicMember)
                .await?;
            effects.named_tuple_own_instance_member(class, name).await?
        }
        ClassType::NonGeneric(ClassLiteral::DynamicTypedDict(_)) => Member::default(),
        ClassType::NonGeneric(ClassLiteral::DynamicEnum(class)) => {
            effects
                .checkpoint(InstanceStorageWork::OwnDynamicMember)
                .await?;
            effects.enum_own_instance_member(class, name).await?
        }
        ClassType::NonGeneric(ClassLiteral::Static(class)) => {
            effects
                .checkpoint(InstanceStorageWork::OwnStaticMember)
                .await?;
            effects.static_own_instance_member(env, class, name).await?
        }
        ClassType::Generic(generic) => {
            effects
                .checkpoint(InstanceStorageWork::AliasSpecialization)
                .await?;
            let specialization = effects.alias_specialization(generic).await?;
            effects.checkpoint(InstanceStorageWork::AliasOrigin).await?;
            let class = effects.alias_origin(generic).await?;
            effects
                .checkpoint(InstanceStorageWork::OwnStaticMember)
                .await?;
            let member = effects.static_own_instance_member(env, class, name).await?;
            effects
                .checkpoint(InstanceStorageWork::OwnerSpecialization)
                .await?;
            effects
                .specialize_member(member, Some(specialization))
                .await?
        }
    };
    effects.checkpoint(InstanceStorageWork::Publish).await?;
    Ok(result)
}

#[ty_mapping_probe_macros::dual_instance_storage]
pub(in crate::types) async fn static_instance_member_with<
    'a,
    'db,
    E: StaticInstanceStorageEffects<'db>,
>(
    env: &ProgramEnvironment<'db>,
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    name: &'a str,
    effects: &E,
) -> Result<PlaceAndQualifiers<'db>, E::Error> {
    effects
        .storage_checkpoint(InstanceStorageWork::Begin)
        .await?;
    effects
        .storage_checkpoint(InstanceStorageWork::TypedDict)
        .await?;
    let result = if effects.is_typed_dict(class).await? || {
        effects
            .storage_checkpoint(InstanceStorageWork::StorageCheck)
            .await?;
        effects.lacks_instance_storage(class, name).await?
    } {
        Place::Undefined.into()
    } else {
        effects.storage_checkpoint(InstanceStorageWork::Mro).await?;
        match effects
            .mro_instance_member(env, class, specialization, name)
            .await?
        {
            InstanceMemberResult::Done(result) => result,
            InstanceMemberResult::TypedDict => {
                effects
                    .storage_checkpoint(InstanceStorageWork::TypedDictFallback)
                    .await?;
                effects.typed_dict_fallback(env, class, name).await?
            }
        }
    };
    effects
        .storage_checkpoint(InstanceStorageWork::Publish)
        .await?;
    Ok(result)
}

struct InstanceClassificationFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousInstanceClassificationEffects)]
    pub(in crate::types) trait InstanceClassificationEffects<'db>: sealed::Sealed {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: InstanceStorageWork) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(source)]
        async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn instance_flags(&self, class: StaticClassLiteral<'db>) -> Result<ClassInstanceFlags, Self::Error>;
    }

    #[finite_capability]
    impl InstanceClassificationFacts {
        fn static_class<'db>(&self, class: ClassLiteral<'db>) -> Option<StaticClassLiteral<'db>> {
            class.as_static()
        }
    }

    #[synchronous(inherits_from_explicit_any_without_inference_impl_sync)]
    #[capabilities(effects = InstanceClassificationEffects, facts = InstanceClassificationFacts)]
    #[passive_values()]
    async fn inherits_from_explicit_any_without_inference_impl_with<'db, E: InstanceClassificationEffects<'db>>(
        class: ClassLiteral<'db>,
        facts: InstanceClassificationFacts,
        effects: &E,
    ) -> Result<Option<bool>, E::Error> {
        if let Some(class) = facts.static_class(class)
            && (matches!(effects.known(class).await?, Some(_)) || !effects.has_explicit_bases(class).await?)
        {
            return Ok(Some(false));
        }
        Ok(None)
    }
}

#[ty_mapping_probe_macros::dual_instance_storage]
pub(in crate::types) async fn static_is_typed_dict_with<
    'db,
    E: InstanceClassificationEffects<'db>,
>(
    class: StaticClassLiteral<'db>,
    effects: &E,
) -> Result<bool, E::Error> {
    effects.checkpoint(InstanceStorageWork::Begin).await?;
    effects.checkpoint(InstanceStorageWork::Header).await?;
    let result = if let Some(known) = effects.known(class).await? {
        KnownClass::is_typed_dict_subclass(known)
    } else if !effects.has_explicit_bases(class).await? {
        false
    } else {
        effects
            .checkpoint(InstanceStorageWork::InstanceFlags)
            .await?;
        let flags = effects.instance_flags(class).await?;
        ClassInstanceFlags::contains(&flags, ClassInstanceFlags::TYPED_DICT)
    };
    effects.checkpoint(InstanceStorageWork::Publish).await?;
    Ok(result)
}

pub(in crate::types) async fn inherits_from_explicit_any_without_inference_with<
    'db,
    E: InstanceClassificationEffects<'db>,
>(
    class: ClassLiteral<'db>,
    effects: &E,
) -> Result<Option<bool>, E::Error> {
    inherits_from_explicit_any_without_inference_impl_with(
        class,
        InstanceClassificationFacts,
        effects,
    )
    .await
}

pub(in crate::types) fn inherits_from_explicit_any_without_inference_sync<
    'db,
    E: SynchronousInstanceClassificationEffects<'db>,
>(
    class: ClassLiteral<'db>,
    effects: &E,
) -> Result<Option<bool>, E::Error> {
    inherits_from_explicit_any_without_inference_impl_sync(
        class,
        InstanceClassificationFacts,
        effects,
    )
}

pub(in crate::types) struct InlineInstanceStorageEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> InlineInstanceStorageEffects<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }
}

impl sealed::Sealed for InlineInstanceStorageEffects<'_> {}

impl<'db> SynchronousClassInstanceStorageEffects<'db> for InlineInstanceStorageEffects<'db> {
    fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        Ok(alias.origin(self.db))
    }
    fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(alias.specialization(self.db))
    }
    type Error = Infallible;

    fn checkpoint(&self, _: InstanceStorageWork) -> Result<(), Self::Error> {
        Ok(())
    }
    fn dynamic_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: DynamicClassLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(class.instance_member(self.db, env, name))
    }
    fn named_tuple_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: DynamicNamedTupleLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(class.instance_member(self.db, env, name))
    }
    fn enum_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: DynamicEnumLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(class.instance_member(self.db, env, name))
    }
    fn dynamic_own_instance_member(
        &self,
        class: DynamicClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(class.own_instance_member(self.db, name))
    }
    fn named_tuple_own_instance_member(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(class.own_instance_member(self.db, name))
    }
    fn enum_own_instance_member(
        &self,
        class: DynamicEnumLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(class.own_instance_member(self.db, name))
    }
    fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        static_is_typed_dict_sync(class, self)
    }
    fn static_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        static_instance_member_sync(env, class, specialization, name, self)
    }
    fn static_own_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        static_own_instance_member_sync(
            env,
            class,
            name,
            &InlineMemberSourceEffects::new(self.db),
        )
    }
    fn specialize_place(
        &self,
        member: PlaceAndQualifiers<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(member.map_type(|ty| {
            ty.apply_optional_owner_specialization_to_member(self.db, specialization)
        }))
    }
    fn specialize_member(
        &self,
        member: Member<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(member.map_type(|ty| {
            ty.apply_optional_owner_specialization_to_member(self.db, specialization)
        }))
    }
}

impl<'db> SynchronousStaticInstanceStorageEffects<'db> for InlineInstanceStorageEffects<'db> {
    type Error = Infallible;

    fn storage_checkpoint(&self, _: InstanceStorageWork) -> Result<(), Self::Error> {
        Ok(())
    }
    fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        static_is_typed_dict_sync(class, self)
    }
    fn lacks_instance_storage(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error> {
        lacks_instance_storage_sync(class, name, &InlineMemberSourceEffects::new(self.db))
    }
    fn mro_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Result<InstanceMemberResult<'db>, Self::Error> {
        Ok(
            MroLookup::new(self.db, env, class.iter_mro(self.db, specialization))
                .instance_member(name),
        )
    }
    fn typed_dict_fallback(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(KnownClass::TypedDictFallback
            .to_instance(self.db, env)
            .instance_member(self.db, env, name)
            .map_type(|ty| {
                ty.apply_type_mapping(
                    self.db,
                    env,
                    &TypeMapping::ReplaceSelf {
                        new_upper_bound: Type::instance(
                            self.db,
                            env,
                            class.unknown_specialization(self.db),
                        ),
                    },
                    TypeContext::default(),
                )
            }))
    }
}

impl<'db> SynchronousInstanceClassificationEffects<'db> for InlineInstanceStorageEffects<'db> {
    fn known(&self, class: StaticClassLiteral<'db>) -> Result<Option<KnownClass>, Self::Error> {
        Ok(class.known(self.db))
    }
    fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.has_explicit_bases(self.db))
    }

    type Error = Infallible;

    fn checkpoint(&self, _: InstanceStorageWork) -> Result<(), Self::Error> {
        Ok(())
    }
    fn instance_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassInstanceFlags, Self::Error> {
        Ok(class.instance_flags(self.db))
    }
}
