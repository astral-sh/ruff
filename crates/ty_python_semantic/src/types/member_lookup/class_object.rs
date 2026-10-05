//! Class-object namespace lookup, including instance storage supplied by its metaclass.

use std::convert::Infallible;

use crate::place::{DefinedPlace, Definedness, Place, PlaceAndQualifiers, TypeOrigin};
use crate::types::{
    ClassType, MemberLookupPolicy, NominalInstanceType, ProtocolInstanceType, SubclassOfInner,
    SubclassOfType, Type,
};
use crate::{Db, ProgramEnvironment};

/// Fixed decisions and transfers in class-object namespace lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ClassObjectWork {
    Begin,
    OwnClass,
    OwnDeclaration,
    MetaType,
    InstanceApproximation,
    InstanceStorage,
    Fallback,
    Publish,
}

/// Supplies finite variant and declaration inspections to the shared class-object resolver.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct ClassObjectFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassObjectEffects)]
    pub(in crate::types) trait ClassObjectEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: ClassObjectWork) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn find_in_mro(&self, ty: Type<'db>, name: &str, policy: MemberLookupPolicy) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;
        #[operation(child)]
        async fn to_class_type(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn subclass_inner_class(&self, inner: SubclassOfInner<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn protocol_origin(&self, protocol: ProtocolInstanceType<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn own_member(&self, class: ClassType<'db>, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn meta_type(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn instance_member(&self, ty: Type<'db>, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn fall_back_to(&self, member: PlaceAndQualifiers<'db>, fallback: PlaceAndQualifiers<'db>) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn class_instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn subclass_instance(&self, subclass: SubclassOfType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn other_instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn nominal_instance_member(&self, instance: NominalInstanceType<'db>, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn other_instance_member(&self, ty: Type<'db>, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    }

    #[finite_capability]
    impl ClassObjectFacts {
        fn require_mro<'db>(&self, member: Option<PlaceAndQualifiers<'db>>) -> PlaceAndQualifiers<'db> {
            member.expect(
                "Calling `class_object_member` on class literals and subclass-of types \
                should always find an MRO",
            )
        }

        fn subclass_inner<'db>(&self, subclass: SubclassOfType<'db>) -> SubclassOfInner<'db> {
            subclass.subclass_of()
        }

        fn own_declaration<'db>(&self, member: Option<PlaceAndQualifiers<'db>>) -> Option<Definedness> {
            match member {
                Some(PlaceAndQualifiers {
                    place: Place::Defined(DefinedPlace { origin: TypeOrigin::Declared, definedness, .. }),
                    ..
                }) => Some(definedness),
                _ => None,
            }
        }

        fn bound<'db>(&self, ty: Type<'db>) -> PlaceAndQualifiers<'db> {
            PlaceAndQualifiers::from(Place::bound(ty))
        }
    }

    /// Combines a class object's MRO member and metaclass storage using its own declaration's precedence.
    #[synchronous(class_object_member_sync)]
    #[capabilities(effects = ClassObjectEffects, facts = ClassObjectFacts)]
    #[passive_values(ClassObjectWork::Begin, ClassObjectWork::OwnClass, ClassObjectWork::OwnDeclaration, ClassObjectWork::MetaType, ClassObjectWork::Fallback, ClassObjectWork::Publish, Definedness::AlwaysDefined)]
    pub(in crate::types) async fn class_object_member_with<'db, E: ClassObjectEffects<'db>>(
        ty: Type<'db>, name: &str, policy: MemberLookupPolicy, facts: ClassObjectFacts, effects: &E,
    ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
        effects.checkpoint(ClassObjectWork::Begin).await?;
        let class_attr = facts.require_mro(effects.find_in_mro(ty, name, policy).await?);

        effects.checkpoint(ClassObjectWork::OwnClass).await?;
        let own_class = match ty {
            Type::SubclassOf(subclass) => match facts.subclass_inner(subclass) {
                SubclassOfInner::Protocol(protocol) => effects.protocol_origin(protocol).await?,
                inner => effects.subclass_inner_class(inner).await?,
            },
            _ => effects.to_class_type(ty).await?,
        };
        let own_class_attr = match own_class {
            Some(class) => Some(effects.own_member(class, name).await?),
            None => None,
        };

        // A definitely-declared attribute in this class's own namespace is the contract for
        // values populated by metaclass initialization, analogous to a declared instance
        // attribute initialized in `__init__`. An inherited declaration does not mask a value
        // that the metaclass stores directly on the newly constructed subclass.
        effects.checkpoint(ClassObjectWork::OwnDeclaration).await?;
        let own_declaration_definedness = facts.own_declaration(own_class_attr);
        if let Some(Definedness::AlwaysDefined) = own_declaration_definedness {
            effects.checkpoint(ClassObjectWork::Publish).await?;
            return Ok(class_attr);
        }

        effects.checkpoint(ClassObjectWork::MetaType).await?;
        let metaclass = effects.meta_type(ty).await?;
        let Some(metaclass_instance) = effects.instance_approximation(metaclass).await? else {
            effects.checkpoint(ClassObjectWork::Publish).await?;
            return Ok(class_attr);
        };
        let metaclass_attr = effects.instance_member(metaclass_instance, name).await?;

        effects.checkpoint(ClassObjectWork::Fallback).await?;
        let member = match own_declaration_definedness {
            // A conditionally-declared attribute is a contract only on paths where that
            // declaration is present; the metaclass value is the fallback on other paths.
            Some(_) => effects.fall_back_to(class_attr, metaclass_attr).await?,
            None => effects.fall_back_to(metaclass_attr, class_attr).await?,
        };
        effects.checkpoint(ClassObjectWork::Publish).await?;
        Ok(member)
    }

    /// Converts a metaclass to its instances, leaving other conversion families to their provider.
    #[synchronous(class_object_instance_approximation_sync)]
    #[capabilities(effects = ClassObjectEffects)]
    #[passive_values(ClassObjectWork::InstanceApproximation)]
    pub(in crate::types) async fn class_object_instance_approximation_with<'db, E: ClassObjectEffects<'db>>(
        ty: Type<'db>, effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        effects.checkpoint(ClassObjectWork::InstanceApproximation).await?;
        match ty {
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => Ok(Some(ty)),
            Type::ClassLiteral(_) | Type::GenericAlias(_) => effects.class_instance_approximation(ty).await,
            Type::SubclassOf(subclass) => Ok(Some(effects.subclass_instance(subclass).await?)),
            _ => effects.other_instance_approximation(ty).await,
        }
    }

    /// Reads raw metaclass instance storage without binding descriptors or looking up class members.
    #[synchronous(class_object_instance_member_sync)]
    #[capabilities(effects = ClassObjectEffects, facts = ClassObjectFacts)]
    #[passive_values(ClassObjectWork::InstanceStorage)]
    pub(in crate::types) async fn class_object_instance_member_with<'db, E: ClassObjectEffects<'db>>(
        ty: Type<'db>, name: &str, facts: ClassObjectFacts, effects: &E,
    ) -> Result<PlaceAndQualifiers<'db>, E::Error> {
        effects.checkpoint(ClassObjectWork::InstanceStorage).await?;
        match ty {
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => Ok(facts.bound(ty)),
            Type::NominalInstance(instance) => effects.nominal_instance_member(instance, name).await,
            _ => effects.other_instance_member(ty, name).await,
        }
    }
}

/// Resolves ordinary class-object children using the caller's database and environment.
pub(in crate::types) struct OrdinaryClassObjectEffects<'a, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'a ProgramEnvironment<'db>,
}

impl<'db> SynchronousClassObjectEffects<'db> for OrdinaryClassObjectEffects<'_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self, _work: ClassObjectWork) -> Result<(), Infallible> {
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

    fn to_class_type(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Infallible> {
        Ok(ty.to_class_type(self.db))
    }

    fn subclass_inner_class(
        &self,
        inner: SubclassOfInner<'db>,
    ) -> Result<Option<ClassType<'db>>, Infallible> {
        Ok(inner.into_class(self.db, self.env))
    }

    fn protocol_origin(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<Option<ClassType<'db>>, Infallible> {
        Ok(protocol.class_origin(self.db).map(|origin| *origin))
    }

    fn own_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(class.own_class_member(self.db, self.env, None, name).inner)
    }

    fn meta_type(&self, ty: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(ty.to_meta_type(self.db, self.env))
    }

    fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Infallible> {
        class_object_instance_approximation_sync(ty, self)
    }

    fn instance_member(
        &self,
        ty: Type<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        class_object_instance_member_sync(ty, name, ClassObjectFacts, self)
    }

    fn fall_back_to(
        &self,
        member: PlaceAndQualifiers<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(member.or_fall_back_to(self.db, self.env, || fallback))
    }

    fn class_instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Infallible> {
        Ok(ty.to_instance_approximation(self.db, self.env))
    }

    fn subclass_instance(&self, subclass: SubclassOfType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(subclass.to_instance(self.db, self.env))
    }

    fn other_instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Infallible> {
        Ok(ty.to_instance_approximation(self.db, self.env))
    }

    fn nominal_instance_member(
        &self,
        instance: NominalInstanceType<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(instance
            .class(self.db, self.env)
            .instance_member(self.db, self.env, name))
    }

    fn other_instance_member(
        &self,
        ty: Type<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(ty.instance_member(self.db, self.env, name))
    }
}
