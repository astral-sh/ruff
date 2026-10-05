use std::convert::Infallible;

use ruff_python_ast::name::Name;

use crate::place::{DefinedPlace, Place, PlaceAndQualifiers, TypeOrigin};
use crate::types::call::{CallArguments, CallDunderError};
use crate::types::class::ClassInstanceFlags;
use crate::types::{
    ClassLiteral, ClassType, DescriptorOrigin, KnownClass, KnownInstanceType, LookupFacts,
    LookupParts, MemberFallbackDecision, MemberLookupErrorKind, MemberLookupPolicy,
    MemberLookupResult, PropertyDeprecations, StaticClassLiteral, Type, TypeContext,
    TypeQualifiers, member_fallback_decision, member_lookup_result_with_origin,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy)]
pub(in crate::types) enum MemberTypeMapping<'db> {
    BindSelf(Type<'db>),
    InferredClassLiterals,
}

#[derive(Clone, Copy)]
pub(in crate::types) enum GetattrCallKind {
    GetAttr,
    GetAttribute,
}

#[derive(Clone, Copy)]
pub(in crate::types) enum GetattrCallResult<'db> {
    Returned(Type<'db>),
    CallError(Type<'db>),
    PossiblyUnbound,
    Missing,
}

pub(in crate::types) struct MemberFinalizationFacts;

pub(in crate::types) struct OrdinaryMemberFinalization<'env, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'env ProgramEnvironment<'db>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousMemberFinalizationEffects)]
    pub(in crate::types) trait MemberFinalizationEffects<'db> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn parts(&self, result: MemberLookupResult<'db>) -> Result<LookupParts<'db>, Self::Error>;
        #[operation(local)]
        async fn result(&self, parts: LookupParts<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
        #[operation(child)]
        async fn map_type(&self, ty: Type<'db>, mapping: MemberTypeMapping<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn nominal_class(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn meta_type(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn metaclass_nominal_class(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(source)]
        async fn static_class(&self, class: ClassType<'db>) -> Result<Option<StaticClassLiteral<'db>>, Self::Error>;
        #[operation(child)]
        async fn instance_flags(&self, class: StaticClassLiteral<'db>) -> Result<ClassInstanceFlags, Self::Error>;
        #[operation(child)]
        async fn custom_getattribute_affects(&self, ty: Type<'db>, result: MemberLookupResult<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn getattribute_member(&self, ty: Type<'db>) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(local)]
        async fn name_type(&self, name: &Name) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn type_member(&self, name: &Name, policy: MemberLookupPolicy) -> Result<MemberLookupResult<'db>, Self::Error>;
        #[operation(child)]
        async fn call_dunder(&self, ty: Type<'db>, name: Type<'db>, kind: GetattrCallKind) -> Result<GetattrCallResult<'db>, Self::Error>;
        #[operation(child)]
        async fn custom_getattr(&self, ty: Type<'db>, name: &Name, policy: MemberLookupPolicy) -> Result<MemberLookupResult<'db>, Self::Error>;
        #[operation(child)]
        async fn getattr_fallback(&self, ty: Type<'db>, name: &Name, result: MemberLookupResult<'db>, policy: MemberLookupPolicy) -> Result<MemberLookupResult<'db>, Self::Error>;
        #[operation(child)]
        async fn merge_place(&self, first: PlaceAndQualifiers<'db>, second: PlaceAndQualifiers<'db>) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn merge_properties(&self, first: PropertyDeprecations<'db>, second: PropertyDeprecations<'db>) -> Result<PropertyDeprecations<'db>, Self::Error>;
        #[operation(child)]
        async fn merge_origins(&self, first: DescriptorOrigin<'db>, second: DescriptorOrigin<'db>) -> Result<DescriptorOrigin<'db>, Self::Error>;
        #[operation(child)]
        async fn fall_back(&self, result: MemberLookupResult<'db>, fallback: MemberLookupResult<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
    }

    #[finite_capability]
    impl MemberFinalizationFacts {
        fn undefined<'db>(&self) -> MemberLookupResult<'db> { Place::Undefined.into() }
        fn raw_type<'db>(&self, parts: LookupParts<'db>) -> Option<Type<'db>> { parts.member.place.raw_type() }
        fn mapped<'db>(&self, parts: LookupParts<'db>, ty: Type<'db>) -> LookupParts<'db> {
            LookupParts { member: parts.member.map_type(|_| ty), ..parts }
        }
        fn promote(&self, parts: LookupParts<'_>) -> bool {
            matches!(parts.member.place, Place::Defined(DefinedPlace { origin: TypeOrigin::Inferred, .. }))
                && !parts.member.qualifiers.contains(TypeQualifiers::FINAL)
        }
        fn decision(&self, parts: LookupParts<'_>) -> MemberFallbackDecision { member_fallback_decision(parts.member.place) }
        fn missing(&self, member: PlaceAndQualifiers<'_>) -> bool { member.place.is_undefined() }
        fn no_getattr(&self, policy: MemberLookupPolicy) -> bool { policy.no_getattr_lookup() }
        fn type_alias(&self, ty: Type<'_>) -> bool { matches!(ty, Type::KnownInstance(KnownInstanceType::TypeGenericAlias(_))) }
        fn custom(&self, flags: ClassInstanceFlags) -> bool { flags.contains(ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE) }
        fn dynamic(&self, flags: ClassInstanceFlags) -> bool { flags.contains(ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE) }
        fn definitely_defined(&self, result: MemberLookupResult<'_>, parts: LookupParts<'_>) -> bool {
            result.is_ok() && matches!(parts.member.place, Place::Defined(place) if place.is_definitely_defined())
        }
        fn result_parts<'db>(&self, ty: Type<'db>, error: Option<MemberLookupErrorKind<'db>>) -> LookupParts<'db> {
            LookupParts { member: Place::bound(ty).into(), error, properties: None, descriptor: DescriptorOrigin::default() }
        }
        fn call_error<'db>(&self, kind: GetattrCallKind, receiver: Type<'db>, name: Type<'db>) -> MemberLookupErrorKind<'db> {
            match kind {
                GetattrCallKind::GetAttr => MemberLookupErrorKind::GetAttr { receiver, name },
                GetattrCallKind::GetAttribute => MemberLookupErrorKind::GetAttribute { receiver, name },
            }
        }
        fn descriptor_error(&self, parts: LookupParts<'_>) -> bool { matches!(parts.error, Some(MemberLookupErrorKind::DescriptorGet(_))) }
        fn has_error(&self, parts: LookupParts<'_>) -> bool { parts.error.is_some() }
        fn first_error<'db>(&self, first: LookupParts<'db>, second: LookupParts<'db>) -> Option<MemberLookupErrorKind<'db>> { first.error.or(second.error) }
        fn simple_origins(&self, first: DescriptorOrigin<'_>, second: DescriptorOrigin<'_>) -> bool {
            !matches!((first.dispatches, second.dispatches), (Some(left), Some(right)) if left != right)
        }
        fn merge_simple_origins<'db>(&self, first: DescriptorOrigin<'db>, second: DescriptorOrigin<'db>) -> DescriptorOrigin<'db> {
            DescriptorOrigin {
                dispatches: first.dispatches.or(second.dispatches),
                incomplete: first.incomplete || second.incomplete,
                return_contains_recursive_recovery: first.return_contains_recursive_recovery || second.return_contains_recursive_recovery,
            }
        }
    }

    #[synchronous(map_member_type_sync)]
    #[capabilities(effects = MemberFinalizationEffects, facts = MemberFinalizationFacts)]
    #[passive_values()]
    pub(in crate::types) async fn map_member_type_with<'db, E: MemberFinalizationEffects<'db>>(
        result: MemberLookupResult<'db>, mapping: MemberTypeMapping<'db>, facts: MemberFinalizationFacts, effects: &E,
    ) -> Result<MemberLookupResult<'db>, E::Error> {
        effects.checkpoint().await?;
        let parts = effects.parts(result).await?;
        let Some(ty) = facts.raw_type(parts) else { return Ok(result); };
        let ty = effects.map_type(ty, mapping).await?;
        effects.result(facts.mapped(parts, ty)).await
    }

    #[synchronous(promote_inferred_member_sync)]
    #[capabilities(effects = MemberFinalizationEffects, facts = MemberFinalizationFacts)]
    #[passive_values(MemberTypeMapping::InferredClassLiterals)]
    pub(in crate::types) async fn promote_inferred_member_with<'db, E: MemberFinalizationEffects<'db>>(
        result: MemberLookupResult<'db>, facts: MemberFinalizationFacts, effects: &E,
    ) -> Result<MemberLookupResult<'db>, E::Error> {
        effects.checkpoint().await?;
        let parts = effects.parts(result).await?;
        if !facts.promote(parts) { return Ok(result); }
        let Some(ty) = facts.raw_type(parts) else { return Ok(result); };
        let ty = effects.map_type(ty, MemberTypeMapping::InferredClassLiterals).await?;
        effects.result(facts.mapped(parts, ty)).await
    }

    /// Returns the nominal class selected from an instance approximation of `ty`'s meta-type.
    /// For a class-object input, this identifies its metaclass. Meta-type conversion precedes
    /// instance conversion; returns `None` if the approximation is absent or has no nominal class.
    #[synchronous(metaclass_nominal_class_sync)]
    #[capabilities(effects = MemberFinalizationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn metaclass_nominal_class_with<'db, E: MemberFinalizationEffects<'db>>(
        ty: Type<'db>, effects: &E,
    ) -> Result<Option<ClassType<'db>>, E::Error> {
        effects.checkpoint().await?;
        let meta = effects.meta_type(ty).await?;
        match effects.instance_approximation(meta).await? {
            Some(instance) => effects.nominal_class(instance).await,
            None => Ok(None),
        }
    }

    #[synchronous(custom_getattribute_affects_sync)]

    #[capabilities(effects = MemberFinalizationEffects, facts = MemberFinalizationFacts)]
    #[passive_values()]
    pub(in crate::types) async fn custom_getattribute_affects_with<'db, E: MemberFinalizationEffects<'db>>(
        ty: Type<'db>, result: MemberLookupResult<'db>, facts: MemberFinalizationFacts, effects: &E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint().await?;
        let class = match effects.nominal_class(ty).await? {
            Some(class) => Some(class),
            None => effects.metaclass_nominal_class(ty).await?,
        };
        let Some(class) = class else { return Ok(true); };
        let Some(class) = effects.static_class(class).await? else { return Ok(true); };
        let flags = effects.instance_flags(class).await?;
        if facts.custom(flags) { return Ok(true); }
        if !facts.dynamic(flags) { return Ok(false); }
        let parts = effects.parts(result).await?;
        Ok(!facts.definitely_defined(result, parts))
    }

    #[synchronous(member_fallback_sync)]
    #[capabilities(effects = MemberFinalizationEffects, facts = MemberFinalizationFacts)]
    #[passive_values(LookupParts)]
    pub(in crate::types) async fn member_fallback_with<'db, E: MemberFinalizationEffects<'db>>(
        result: MemberLookupResult<'db>, fallback: MemberLookupResult<'db>, facts: MemberFinalizationFacts, effects: &E,
    ) -> Result<MemberLookupResult<'db>, E::Error> {
        effects.checkpoint().await?;
        let current = effects.parts(result).await?;
        match facts.decision(current) {
            MemberFallbackDecision::Missing => return Ok(fallback),
            MemberFallbackDecision::Defined => return Ok(result),
            MemberFallbackDecision::PossiblyUndefined => {}
        }
        let fallback = effects.parts(fallback).await?;
        let member = effects.merge_place(current.member, fallback.member).await?;
        let properties = match (current.properties, fallback.properties) {
            (Some(first), Some(second)) => Some(effects.merge_properties(first, second).await?),
            (Some(properties), None) | (None, Some(properties)) => Some(properties),
            (None, None) => None,
        };
        let descriptor = if facts.simple_origins(current.descriptor, fallback.descriptor) {
            facts.merge_simple_origins(current.descriptor, fallback.descriptor)
        } else {
            effects.merge_origins(current.descriptor, fallback.descriptor).await?
        };
        effects.result(LookupParts { member, error: facts.first_error(current, fallback), properties, descriptor }).await
    }

    #[synchronous(custom_getattr_sync)]
    #[capabilities(effects = MemberFinalizationEffects, facts = MemberFinalizationFacts)]
    #[passive_values(GetattrCallKind::GetAttr)]
    pub(in crate::types) async fn custom_getattr_with<'db, E: MemberFinalizationEffects<'db>>(
        ty: Type<'db>, name: &Name, policy: MemberLookupPolicy, facts: MemberFinalizationFacts, effects: &E,
    ) -> Result<MemberLookupResult<'db>, E::Error> {
        effects.checkpoint().await?;
        if facts.no_getattr(policy) { return Ok(facts.undefined()); }
        if facts.type_alias(ty) {
            // `GenericAlias.__getattr__` delegates to `__origin__`. For `type[T]`, the
            // origin is always `type`, not `T`, even when `T` is `Any`.
            return effects.type_member(name, policy).await;
        }
        let name_type = effects.name_type(name).await?;
        match effects.call_dunder(ty, name_type, GetattrCallKind::GetAttr).await? {
            GetattrCallResult::Returned(value) => effects.result(facts.result_parts(value, None)).await,
            GetattrCallResult::CallError(value) => effects.result(facts.result_parts(value, Some(facts.call_error(GetattrCallKind::GetAttr, ty, name_type)))).await,
            GetattrCallResult::PossiblyUnbound | GetattrCallResult::Missing => Ok(facts.undefined()),
        }
    }

    #[synchronous(getattr_fallback_sync)]
    #[capabilities(effects = MemberFinalizationEffects, facts = MemberFinalizationFacts)]
    #[passive_values()]
    pub(in crate::types) async fn getattr_fallback_with<'db, E: MemberFinalizationEffects<'db>>(
        ty: Type<'db>, name: &Name, result: MemberLookupResult<'db>, policy: MemberLookupPolicy, facts: MemberFinalizationFacts, effects: &E,
    ) -> Result<MemberLookupResult<'db>, E::Error> {
        effects.checkpoint().await?;
        let parts = effects.parts(result).await?;
        if let MemberFallbackDecision::Defined = facts.decision(parts) { return Ok(result); }
        let fallback = effects.custom_getattr(ty, name, policy).await?;
        effects.fall_back(result, fallback).await
    }

    #[synchronous(fallback_to_getattr_sync)]
    #[capabilities(effects = MemberFinalizationEffects, facts = MemberFinalizationFacts)]
    #[passive_values(GetattrCallKind::GetAttribute, LookupParts)]
    pub(in crate::types) async fn fallback_to_getattr_with<'db, E: MemberFinalizationEffects<'db>>(
        ty: Type<'db>, name: &Name, result: MemberLookupResult<'db>, policy: MemberLookupPolicy, facts: MemberFinalizationFacts, effects: &E,
    ) -> Result<MemberLookupResult<'db>, E::Error> {
        effects.checkpoint().await?;
        if !effects.custom_getattribute_affects(ty, result).await?
            || facts.missing(effects.getattribute_member(ty).await?)
        {
            return effects.getattr_fallback(ty, name, result, policy).await;
        }
        let name_type = effects.name_type(name).await?;
        let custom = match effects.call_dunder(ty, name_type, GetattrCallKind::GetAttribute).await? {
            GetattrCallResult::Returned(value) => effects.result(facts.result_parts(value, None)).await?,
            GetattrCallResult::CallError(value) => effects.result(facts.result_parts(value, Some(facts.call_error(GetattrCallKind::GetAttribute, ty, name_type)))).await?,
            GetattrCallResult::PossiblyUnbound => facts.undefined(),
            GetattrCallResult::Missing => return effects.getattr_fallback(ty, name, result, policy).await,
        };
        let current = effects.parts(result).await?;
        let custom_parts = effects.parts(custom).await?;
        if facts.has_error(custom_parts) {
            let member = effects.merge_place(current.member, custom_parts.member).await?;
            let descriptor = if facts.simple_origins(current.descriptor, custom_parts.descriptor) {
                facts.merge_simple_origins(current.descriptor, custom_parts.descriptor)
            } else { effects.merge_origins(current.descriptor, custom_parts.descriptor).await? };
            return effects.result(LookupParts { member, error: custom_parts.error, properties: current.properties, descriptor }).await;
        }
        // A custom override runs before the descriptor and might return without invoking it.
        let result = if facts.descriptor_error(current) {
            effects.result(LookupParts { error: None, ..current }).await?
        } else { result };
        let result = effects.fall_back(result, custom).await?;
        effects.getattr_fallback(ty, name, result, policy).await
    }
}

impl<'db> SynchronousMemberFinalizationEffects<'db> for OrdinaryMemberFinalization<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }
    fn parts(&self, result: MemberLookupResult<'db>) -> Result<LookupParts<'db>, Infallible> {
        Ok(LookupFacts.parts(salsa::FieldReads::new(self.db), result))
    }
    fn result(&self, parts: LookupParts<'db>) -> Result<MemberLookupResult<'db>, Infallible> {
        Ok(member_lookup_result_with_origin(
            self.db,
            parts.member,
            parts.error,
            parts.properties,
            parts.descriptor,
        ))
    }
    fn map_type(
        &self,
        ty: Type<'db>,
        mapping: MemberTypeMapping<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(match mapping {
            MemberTypeMapping::BindSelf(receiver) => {
                ty.bind_self_typevars(self.db, self.env, receiver)
            }
            MemberTypeMapping::InferredClassLiterals => {
                ty.promote_class_literals(self.db, self.env)
            }
        })
    }
    fn nominal_class(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Infallible> {
        Ok(ty.nominal_class(self.db, self.env))
    }
    fn meta_type(&self, ty: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(ty.to_meta_type(self.db, self.env))
    }
    fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Infallible> {
        Ok(ty.to_instance_approximation(self.db, self.env))
    }
    fn metaclass_nominal_class(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Infallible> {
        metaclass_nominal_class_sync(ty, self)
    }
    fn static_class(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<StaticClassLiteral<'db>>, Infallible> {
        Ok(class.class_literal(self.db).as_static())
    }
    fn instance_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassInstanceFlags, Infallible> {
        Ok(ClassLiteral::Static(class).instance_flags(self.db))
    }
    fn custom_getattribute_affects(
        &self,
        ty: Type<'db>,
        result: MemberLookupResult<'db>,
    ) -> Result<bool, Infallible> {
        Ok(ty.custom_getattribute_may_affect_lookup(self.db, self.env, result))
    }
    fn getattribute_member(&self, ty: Type<'db>) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(ty.class_member_with_policy(
            self.db,
            self.env,
            "__getattribute__",
            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
        ))
    }
    fn name_type(&self, name: &Name) -> Result<Type<'db>, Infallible> {
        Ok(Type::string_literal(self.db, name))
    }
    fn type_member(
        &self,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Result<MemberLookupResult<'db>, Infallible> {
        Ok(KnownClass::Type
            .to_class_literal(self.db, self.env)
            .member_lookup_with_policy_and_receiver(self.db, self.env, name, policy, None))
    }
    fn call_dunder(
        &self,
        ty: Type<'db>,
        name: Type<'db>,
        kind: GetattrCallKind,
    ) -> Result<GetattrCallResult<'db>, Infallible> {
        let (method, policy) = match kind {
            GetattrCallKind::GetAttr => ("__getattr__", MemberLookupPolicy::default()),
            GetattrCallKind::GetAttribute => (
                "__getattribute__",
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                    | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            ),
        };
        Ok(
            match ty.try_call_dunder_with_policy(
                self.db,
                self.env,
                method,
                &mut CallArguments::positional([name]),
                TypeContext::default(),
                policy,
            ) {
                Ok(bindings) => {
                    GetattrCallResult::Returned(bindings.return_type(self.db, self.env))
                }
                Err(CallDunderError::CallError(_, bindings, _)) => {
                    GetattrCallResult::CallError(bindings.return_type(self.db, self.env))
                }
                Err(CallDunderError::PossiblyUnbound { .. }) => GetattrCallResult::PossiblyUnbound,
                Err(CallDunderError::MethodNotAvailable) => GetattrCallResult::Missing,
            },
        )
    }
    fn custom_getattr(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Result<MemberLookupResult<'db>, Infallible> {
        custom_getattr_sync(ty, name, policy, MemberFinalizationFacts, self)
    }
    fn getattr_fallback(
        &self,
        ty: Type<'db>,
        name: &Name,
        result: MemberLookupResult<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<MemberLookupResult<'db>, Infallible> {
        getattr_fallback_sync(ty, name, result, policy, MemberFinalizationFacts, self)
    }
    fn merge_place(
        &self,
        first: PlaceAndQualifiers<'db>,
        second: PlaceAndQualifiers<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(first.or_fall_back_to(self.db, self.env, || second))
    }
    fn merge_properties(
        &self,
        first: PropertyDeprecations<'db>,
        second: PropertyDeprecations<'db>,
    ) -> Result<PropertyDeprecations<'db>, Infallible> {
        Ok(first.union(self.db, second))
    }
    fn merge_origins(
        &self,
        first: DescriptorOrigin<'db>,
        second: DescriptorOrigin<'db>,
    ) -> Result<DescriptorOrigin<'db>, Infallible> {
        Ok(first.merge(self.db, second))
    }
    fn fall_back(
        &self,
        result: MemberLookupResult<'db>,
        fallback: MemberLookupResult<'db>,
    ) -> Result<MemberLookupResult<'db>, Infallible> {
        member_fallback_sync(result, fallback, MemberFinalizationFacts, self)
    }
}
