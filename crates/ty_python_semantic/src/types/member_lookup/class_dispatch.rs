use std::convert::Infallible;

use ruff_python_ast::name::Name;

use crate::place::{Place, PlaceAndQualifiers};
use crate::types::typed_dict::SynthesizedTypedDictType;
use crate::types::{
    ClassType, InlineMemberEntry, IntersectionType, KnownClass, LiteralValueType,
    LiteralValueTypeKind, MemberLookupPolicy, NominalInstanceType, Parameter, Parameters,
    PropertyInstanceClass, ProtocolInstanceType, Signature, SubclassOfType, Type, TypedDictType,
    UnionType, class, nominal_class_member_sync,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct ClassMemberDispatchFacts;

#[derive(Clone, Copy)]
pub(in crate::types) enum ClassMemberMroKind {
    KnownType,
    MetaType,
}

pub(in crate::types) struct OrdinaryClassMemberDispatch<'env, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'env ProgramEnvironment<'db>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassMemberDispatchEffects)]
    pub(in crate::types) trait ClassMemberDispatchEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, name: &Name) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn lookup(&self, ty: Type<'db>, name: &Name, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn materialized_protocol(&self, protocol: ProtocolInstanceType<'db>, name: &Name, policy: MemberLookupPolicy) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;
        #[operation(child)]
        async fn union(&self, union: UnionType<'db>, name: &Name, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn intersection(&self, intersection: IntersectionType<'db>, name: &Name, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn synthesized_typed_dict(&self, typed_dict: SynthesizedTypedDictType<'db>, name: &Name, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(source)]
        async fn protocol_has_no_origin(&self, protocol: ProtocolInstanceType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn protocol_instance_member(&self, ty: Type<'db>, name: &Name) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn literal_length(&self, literal: LiteralValueType<'db>) -> Result<Option<i64>, Self::Error>;
        #[operation(child)]
        async fn literal_length_member(&self, ty: Type<'db>, length: i64) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn known_type(&self) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn mro_member(&self, ty: Type<'db>, name: &Name, policy: MemberLookupPolicy, kind: ClassMemberMroKind) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn nominal_member(&self, ty: Type<'db>, instance: NominalInstanceType<'db>, name: &Name, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn meta_type(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn class_object_member(&self, ty: Type<'db>, name: &Name, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    }

    #[synchronous(SynchronousPropertyClassEffects)]
    pub(in crate::types) trait PropertyClassEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn known_class(&self, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn subclass(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn known_instance(&self, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn subclass_instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl ClassMemberDispatchFacts {
        fn materialized_fallback<'db>(&self, ty: Type<'db>) -> Option<Type<'db>> {
            ty.materialized_divergent_fallback()
        }

        fn is_len(&self, name: &Name) -> bool {
            name == "__len__"
        }

        fn is_dynamic(&self, subclass: SubclassOfType<'_>) -> bool {
            subclass.is_dynamic()
        }

        fn is_undefined(&self, member: PlaceAndQualifiers<'_>) -> bool {
            member.place.is_undefined()
        }
    }

    #[synchronous(class_member_dispatch_sync)]
    #[capabilities(effects = ClassMemberDispatchEffects, facts = ClassMemberDispatchFacts)]
    #[passive_values(ClassMemberMroKind::KnownType, ClassMemberMroKind::MetaType)]
    pub(in crate::types) async fn class_member_dispatch_with<'db, E: ClassMemberDispatchEffects<'db>>(
        ty: Type<'db>, name: &Name, policy: MemberLookupPolicy,
        facts: ClassMemberDispatchFacts, effects: &E,
    ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
        effects.checkpoint(name).await?;
        if let Some(fallback) = facts.materialized_fallback(ty) {
            return effects.lookup(fallback, name, policy).await;
        }
        if let Type::ProtocolInstance(protocol) = ty
            && let Some(member) = effects.materialized_protocol(protocol, name, policy).await?
        {
            return Ok(member);
        }

        match ty {
            Type::Union(union) => effects.union(union, name, policy).await,
            Type::Intersection(intersection) => effects.intersection(intersection, name, policy).await,
            Type::TypedDict(TypedDictType::Synthesized(synthesized)) => {
                effects.synthesized_typed_dict(synthesized, name, policy).await
            }
            // TODO: Remove this once synthesized protocols have a precise meta-type.
            Type::ProtocolInstance(protocol) if effects.protocol_has_no_origin(protocol).await? => {
                effects.protocol_instance_member(ty, name).await
            }
            Type::LiteralValue(literal) if facts.is_len(name) => {
                if let Some(length) = effects.literal_length(literal).await? {
                    effects.literal_length_member(ty, length).await
                } else {
                    let meta_type = effects.meta_type(ty).await?;
                    effects.mro_member(meta_type, name, policy, ClassMemberMroKind::MetaType).await
                }
            }
            // `type[Any]` (or `type[Unknown]`, etc.) has an unknown metaclass, but all
            // metaclasses inherit from `type`. Check `type`'s class-level attributes
            // first so that data descriptors like `__mro__` and `__bases__` resolve to
            // their correct types instead of collapsing to `Any`/`Unknown`.
            Type::SubclassOf(subclass) if facts.is_dynamic(subclass) => {
                let type_class = effects.known_type().await?;
                let result = effects.mro_member(type_class, name, policy, ClassMemberMroKind::KnownType).await?;
                if !facts.is_undefined(result) {
                    Ok(result)
                } else {
                    let meta_type = effects.meta_type(ty).await?;
                    effects.mro_member(meta_type, name, policy, ClassMemberMroKind::MetaType).await
                }
            }
            Type::NominalInstance(instance) => effects.nominal_member(ty, instance, name, policy).await,
            Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_) => {
                let meta_type = effects.meta_type(ty).await?;
                effects.class_object_member(meta_type, name, policy).await
            }
            _ => {
                let meta_type = effects.meta_type(ty).await?;
                effects.mro_member(meta_type, name, policy, ClassMemberMroKind::MetaType).await
            }
        }
    }

    #[synchronous(property_class_literal_sync)]
    #[capabilities(effects = PropertyClassEffects)]
    #[passive_values(KnownClass::Property, KnownClass::EnumProperty)]
    pub(in crate::types) async fn property_class_literal_with<'db, E: PropertyClassEffects<'db>>(
        class: PropertyInstanceClass<'db>, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint().await?;
        match class {
            PropertyInstanceClass::Builtin => effects.known_class(KnownClass::Property).await,
            PropertyInstanceClass::Enum => effects.known_class(KnownClass::EnumProperty).await,
            PropertyInstanceClass::Subclass(class) => effects.subclass(class).await,
        }
    }

    #[synchronous(property_class_instance_sync)]
    #[capabilities(effects = PropertyClassEffects)]
    #[passive_values(KnownClass::Property, KnownClass::EnumProperty)]
    pub(in crate::types) async fn property_class_instance_with<'db, E: PropertyClassEffects<'db>>(
        class: PropertyInstanceClass<'db>, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint().await?;
        match class {
            PropertyInstanceClass::Builtin => effects.known_instance(KnownClass::Property).await,
            PropertyInstanceClass::Enum => effects.known_instance(KnownClass::EnumProperty).await,
            PropertyInstanceClass::Subclass(class) => effects.subclass_instance(class).await,
        }
    }
}

impl<'db> SynchronousClassMemberDispatchEffects<'db> for OrdinaryClassMemberDispatch<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self, _name: &Name) -> Result<(), Self::Error> {
        Ok(())
    }

    fn lookup(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(ty.class_member_with_policy(self.db, self.env, name, policy))
    }

    fn materialized_protocol(
        &self,
        protocol: ProtocolInstanceType<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error> {
        let Some(origin) = protocol.materialized_origin(self.db) else {
            return Ok(None);
        };
        let interface = protocol.interface(self.db);
        Ok(Some(if interface.includes_member(self.db, name) {
            interface.instance_member(self.db, self.env, name)
        } else {
            Type::instance(self.db, self.env, *origin)
                .class_member_with_policy(self.db, self.env, name, policy)
        }))
    }

    fn union(
        &self,
        union: UnionType<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(
            union.map_with_boundness_and_qualifiers(self.db, self.env, |elem| {
                elem.class_member_with_policy(self.db, self.env, name, policy)
            }),
        )
    }

    fn intersection(
        &self,
        intersection: IntersectionType<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(
            intersection.map_with_boundness_and_qualifiers(self.db, self.env, |elem| {
                elem.class_member_with_policy(self.db, self.env, name, policy)
            }),
        )
    }

    fn synthesized_typed_dict(
        &self,
        typed_dict: SynthesizedTypedDictType<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(class::synthesized_typed_dict_class_member(
            self.db, self.env, typed_dict, policy, name,
        ))
    }

    fn protocol_has_no_origin(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(protocol.class_origin(self.db).is_none())
    }

    fn protocol_instance_member(
        &self,
        ty: Type<'db>,
        name: &Name,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(ty.instance_member(self.db, self.env, name))
    }

    fn literal_length(&self, literal: LiteralValueType<'db>) -> Result<Option<i64>, Self::Error> {
        let length = match literal.kind() {
            LiteralValueTypeKind::Bytes(bytes) => Some(bytes.python_len(self.db)),
            LiteralValueTypeKind::String(string) => Some(string.python_len(self.db)),
            _ => None,
        };
        Ok(length.and_then(|length| i64::try_from(length).ok()))
    }

    fn literal_length_member(
        &self,
        ty: Type<'db>,
        length: i64,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        let parameters =
            Parameters::standard([
                Parameter::positional_only(Some(Name::new_static("self"))).with_annotated_type(ty)
            ]);
        Ok(Place::bound(Type::function_like_callable(
            self.db,
            Signature::new(parameters, Type::int_literal(length)),
        ))
        .into())
    }

    fn known_type(&self) -> Result<Type<'db>, Self::Error> {
        Ok(KnownClass::Type.to_class_literal(self.db, self.env))
    }

    fn mro_member(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
        kind: ClassMemberMroKind,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        let member = ty.find_name_in_mro_with_policy(self.db, self.env, name, policy);
        Ok(match kind {
            ClassMemberMroKind::KnownType => {
                member.expect("`find_name_in_mro` should return `Some` for a class literal")
            }
            ClassMemberMroKind::MetaType => member.expect(
                "`Type::find_name_in_mro()` should return `Some()` \
                when called on a meta-type",
            ),
        })
    }

    fn nominal_member(
        &self,
        ty: Type<'db>,
        instance: NominalInstanceType<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        nominal_class_member_sync(
            ty,
            instance,
            name,
            policy,
            &InlineMemberEntry {
                db: self.db,
                env: self.env,
                recursion_guard: None,
            },
        )
    }

    fn meta_type(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.to_meta_type(self.db, self.env))
    }

    fn class_object_member(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(ty.class_object_member(self.db, self.env, name, policy))
    }
}

impl<'db> SynchronousPropertyClassEffects<'db> for OrdinaryClassMemberDispatch<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn known_class(&self, class: KnownClass) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_class_literal(self.db, self.env))
    }

    fn subclass(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(class.into())
    }

    fn known_instance(&self, class: KnownClass) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_instance(self.db, self.env))
    }

    fn subclass_instance(&self, class: ClassType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(Type::instance(self.db, self.env, class))
    }
}
