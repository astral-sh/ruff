use std::convert::Infallible;

use crate::place::{Place, PlaceAndQualifiers};
use crate::types::typevar::TypeVarConstraints;
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, IntersectionType, KnownClass,
    MemberLookupPolicy, NominalInstanceType, ProtocolInstanceType, RecursiveType, SubclassOfInner,
    SubclassOfType, Type, TypeAliasType, TypeVarBoundOrConstraints, UnionType,
    native_class_mro_attribute, property_wrapper_descriptor,
};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) struct MroLookupFacts;

pub(in crate::types) struct OrdinaryMroLookupEffects<'db> {
    pub(in crate::types) db: &'db dyn Db,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousMroLookupEffects)]
    pub(in crate::types) trait MroLookupEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, name: &str) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn lookup(&self, ty: Type<'db>, env: &ProgramEnvironment<'db>, name: &str, policy: MemberLookupPolicy) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;
        #[operation(child)]
        async fn recursive_var(&self) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;
        #[operation(child)]
        async fn union(&self, union: UnionType<'db>, env: &ProgramEnvironment<'db>, name: &str, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn intersection(&self, intersection: IntersectionType<'db>, env: &ProgramEnvironment<'db>, name: &str, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn unfold(&self, recursive: RecursiveType<'db>, env: &ProgramEnvironment<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn is_typed_dict(&self, class: ClassType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn typed_dict_member(&self, class: ClassType<'db>, env: &ProgramEnvironment<'db>, name: &str, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(source)]
        async fn native_class_attribute(&self, class: ClassLiteral<'db>, name: &str) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;
        #[operation(child)]
        async fn class_member(&self, class: ClassType<'db>, env: &ProgramEnvironment<'db>, name: &str, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn property_wrapper(&self, member: PlaceAndQualifiers<'db>, env: &ProgramEnvironment<'db>, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn subclass_member(&self, subclass: SubclassOfType<'db>, env: &ProgramEnvironment<'db>, name: &str, policy: MemberLookupPolicy) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;
        #[operation(child)]
        async fn known_class(&self, class: KnownClass, env: &ProgramEnvironment<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn nominal_is_type(&self, instance: NominalInstanceType<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn alias_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(SynchronousSubclassMroEffects)]
    pub(in crate::types) trait SubclassMroEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, name: &str) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn protocol_meta_member(&self, protocol: ProtocolInstanceType<'db>, env: &ProgramEnvironment<'db>, name: &str) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;
        #[operation(child)]
        async fn transpose_typevar(&self, typevar: BoundTypeVarInstance<'db>, env: &ProgramEnvironment<'db>) -> Result<SubclassOfInner<'db>, Self::Error>;
        #[operation(source)]
        async fn protocol_origin(&self, protocol: ProtocolInstanceType<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn require_bound_or_constraints(&self, typevar: BoundTypeVarInstance<'db>, env: &ProgramEnvironment<'db>) -> Result<TypeVarBoundOrConstraints<'db>, Self::Error>;
        #[operation(child)]
        async fn constraint_types(&self, constraints: TypeVarConstraints<'db>, env: &ProgramEnvironment<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn lookup(&self, ty: Type<'db>, env: &ProgramEnvironment<'db>, name: &str, policy: MemberLookupPolicy) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error>;
    }

    #[finite_capability]
    impl MroLookupFacts {
        fn materialized_fallback<'db>(&self, ty: Type<'db>) -> Option<Type<'db>> {
            ty.materialized_divergent_fallback()
        }

        fn require_concrete(&self, policy: MemberLookupPolicy) -> bool {
            policy.require_concrete()
        }

        fn no_object_fallback(&self, policy: MemberLookupPolicy) -> bool {
            policy.mro_no_object_fallback()
        }

        fn undefined<'db>(&self) -> PlaceAndQualifiers<'db> {
            Place::Undefined.into()
        }

        fn bound<'db>(&self, ty: Type<'db>) -> PlaceAndQualifiers<'db> {
            Place::bound(ty).into()
        }

        fn subclass_inner<'db>(&self, subclass: SubclassOfType<'db>) -> SubclassOfInner<'db> {
            subclass.subclass_of()
        }

        fn class_type<'db>(&self, class: ClassType<'db>) -> Type<'db> {
            Type::from(class)
        }
    }

    #[synchronous(subclass_find_name_in_mro_sync)]
    #[capabilities(effects = SubclassMroEffects, facts = MroLookupFacts)]
    #[passive_values(Type::Dynamic)]
    pub(in crate::types) async fn subclass_find_name_in_mro_with<'db, E: SubclassMroEffects<'db>>(
        subclass: SubclassOfType<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        facts: MroLookupFacts,
        effects: &E,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, E::Error> {
        effects.checkpoint(name).await?;
        let inner = facts.subclass_inner(subclass);
        if let SubclassOfInner::Protocol(protocol) = inner
            && let Some(member) = effects.protocol_meta_member(protocol, env, name).await?
        {
            return Ok(Some(member));
        }

        let inner = match inner {
            SubclassOfInner::TypeVar(typevar) => effects.transpose_typevar(typevar, env).await?,
            _ => inner,
        };
        let class_like = match inner {
            SubclassOfInner::Class(class) => facts.class_type(class),
            SubclassOfInner::Dynamic(dynamic) => Type::Dynamic(dynamic),
            SubclassOfInner::Protocol(protocol) => {
                let Some(origin) = effects.protocol_origin(protocol).await? else {
                    return Ok(None);
                };
                facts.class_type(origin)
            }
            SubclassOfInner::TypeVar(typevar) => {
                match effects.require_bound_or_constraints(typevar, env).await? {
                    TypeVarBoundOrConstraints::UpperBound(bound) => bound,
                    TypeVarBoundOrConstraints::Constraints(constraints) => {
                        effects.constraint_types(constraints, env).await?
                    }
                }
            }
        };
        effects.lookup(class_like, env, name, policy).await
    }

    #[synchronous(find_name_in_mro_sync)]
    #[capabilities(effects = MroLookupEffects, facts = MroLookupFacts)]
    #[passive_values(ClassType::NonGeneric, ClassType::Generic, KnownClass::Super, KnownClass::Object)]
    pub(in crate::types) async fn find_name_in_mro_with<'db, E: MroLookupEffects<'db>>(
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        facts: MroLookupFacts,
        effects: &E,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, E::Error> {
        effects.checkpoint(name).await?;
        if let Some(fallback) = facts.materialized_fallback(ty) {
            return effects.lookup(fallback, env, name, policy).await;
        }

        Ok(match ty {
            Type::RecursiveVar(_) => return effects.recursive_var().await,
            Type::Union(union) => Some(effects.union(union, env, name, policy).await?),
            Type::Intersection(intersection) => Some(effects.intersection(intersection, env, name, policy).await?),

            Type::Dynamic(_) | Type::Divergent(_) if facts.require_concrete(policy) => {
                Some(facts.undefined())
            }

            Type::Recursive(recursive) => {
                if let Some(unfolded) = effects.unfold(recursive, env).await? {
                    return effects.lookup(unfolded, env, name, policy).await;
                }
                Some(facts.bound(ty))
            }

            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => Some(facts.bound(ty)),

            Type::ClassLiteral(class) if effects.is_typed_dict(ClassType::NonGeneric(class)).await? => {
                Some(effects.typed_dict_member(ClassType::NonGeneric(class), env, name, policy).await?)
            }

            Type::ClassLiteral(class) => {
                if let Some(member) = effects.native_class_attribute(class, name).await? {
                    Some(member)
                } else {
                    let member = effects.class_member(ClassType::NonGeneric(class), env, name, policy).await?;
                    Some(effects.property_wrapper(member, env, name).await?)
                }
            }

            Type::GenericAlias(alias) if effects.is_typed_dict(ClassType::Generic(alias)).await? => {
                Some(effects.typed_dict_member(ClassType::Generic(alias), env, name, policy).await?)
            }

            Type::GenericAlias(alias) => {
                let member = effects.class_member(ClassType::Generic(alias), env, name, policy).await?;
                Some(effects.property_wrapper(member, env, name).await?)
            }

            Type::SubclassOf(subclass) => return effects.subclass_member(subclass, env, name, policy).await,

            // Note: `super(pivot, owner).__class__` is `builtins.super`, not the owner's class.
            // `BoundSuper` should look up the name in the MRO of `builtins.super`.
            Type::BoundSuper(_) => {
                let class = effects.known_class(KnownClass::Super, env).await?;
                return effects.lookup(class, env, name, policy).await;
            }

            // We eagerly normalize type[object], i.e. Type::SubclassOf(object) to `type`,
            // i.e. Type::NominalInstance(type). So looking up a name in the MRO of
            // `Type::NominalInstance(type)` is equivalent to looking up the name in the
            // MRO of the class `object`.
            Type::NominalInstance(instance) if effects.nominal_is_type(instance).await? => {
                if facts.no_object_fallback(policy) {
                    Some(facts.undefined())
                } else {
                    let class = effects.known_class(KnownClass::Object, env).await?;
                    return effects.lookup(class, env, name, policy).await;
                }
            }

            Type::TypeAlias(alias) => {
                let value = effects.alias_value(alias).await?;
                return effects.lookup(value, env, name, policy).await;
            }

            Type::FunctionLiteral(_)
            | Type::Callable(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::LiteralValue(_)
            | Type::TypeVar(_)
            | Type::NominalInstance(_)
            | Type::ProtocolInstance(_)
            | Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_)
            | Type::EnumComplement(_)
            | Type::NewTypeInstance(_) => None,
        })
    }
}

impl<'db> SynchronousMroLookupEffects<'db> for OrdinaryMroLookupEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self, _name: &str) -> Result<(), Infallible> {
        Ok(())
    }

    fn lookup(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Infallible> {
        Ok(ty.find_name_in_mro_with_policy(self.db, env, name, policy))
    }

    fn recursive_var(&self) -> Result<Option<PlaceAndQualifiers<'db>>, Infallible> {
        unreachable!("semantic operation on an unbound recursive variable")
    }

    fn union(
        &self,
        union: UnionType<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(
            union.map_with_boundness_and_qualifiers(self.db, env, |elem| {
                elem.find_name_in_mro_with_policy(self.db, env, name, policy)
                    // If some elements are classes, and some are not, we simply fall back to `Unbound` for the non-class
                    // elements instead of short-circuiting the whole result to `None`. We would need a more detailed
                    // return type otherwise, and since `find_name_in_mro` is usually called via `class_member`, this is
                    // not a problem.
                    .unwrap_or_default()
            }),
        )
    }

    fn intersection(
        &self,
        intersection: IntersectionType<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(
            intersection.map_with_boundness_and_qualifiers(self.db, env, |elem| {
                elem.find_name_in_mro_with_policy(self.db, env, name, policy)
                    // Fall back to Unbound, similar to the union case (see above).
                    .unwrap_or_default()
            }),
        )
    }

    fn unfold(
        &self,
        recursive: RecursiveType<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(recursive.unfold(self.db, env).into_unfolded())
    }

    fn is_typed_dict(&self, class: ClassType<'db>) -> Result<bool, Infallible> {
        Ok(class.is_typed_dict(self.db))
    }

    fn typed_dict_member(
        &self,
        class: ClassType<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(match class {
            ClassType::NonGeneric(class) => {
                class.typed_dict_member(self.db, env, None, name, policy)
            }
            ClassType::Generic(alias) => alias.origin(self.db).typed_dict_member(
                self.db,
                env,
                Some(alias.specialization(self.db)),
                name,
                policy,
            ),
        })
    }

    fn native_class_attribute(
        &self,
        class: ClassLiteral<'db>,
        name: &str,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Infallible> {
        Ok(native_class_mro_attribute(class.known(self.db), name))
    }

    fn class_member(
        &self,
        class: ClassType<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(class.class_member(self.db, env, name, policy))
    }

    fn property_wrapper(
        &self,
        member: PlaceAndQualifiers<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(member.map_type(|member| property_wrapper_descriptor(self.db, env, name, member)))
    }

    fn subclass_member(
        &self,
        subclass: SubclassOfType<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Infallible> {
        Ok(subclass.find_name_in_mro_with_policy(self.db, env, name, policy))
    }

    fn known_class(
        &self,
        class: KnownClass,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(class.to_class_literal(self.db, env))
    }

    fn nominal_is_type(&self, instance: NominalInstanceType<'db>) -> Result<bool, Infallible> {
        Ok(instance.has_known_class(self.db, KnownClass::Type))
    }

    fn alias_value(&self, alias: TypeAliasType<'db>) -> Result<Type<'db>, Infallible> {
        Ok(alias.value_type(self.db))
    }
}

impl<'db> SynchronousSubclassMroEffects<'db> for OrdinaryMroLookupEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self, _name: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    fn protocol_meta_member(
        &self,
        protocol: ProtocolInstanceType<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error> {
        Ok(protocol.interface(self.db).meta_member(self.db, env, name))
    }

    fn transpose_typevar(
        &self,
        typevar: BoundTypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<SubclassOfInner<'db>, Self::Error> {
        Ok(SubclassOfInner::TypeVar(typevar).with_transposed_type_var(self.db, env))
    }

    fn protocol_origin(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> Result<Option<ClassType<'db>>, Self::Error> {
        Ok(protocol.class_origin(self.db).map(|origin| *origin))
    }

    fn require_bound_or_constraints(
        &self,
        typevar: BoundTypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<TypeVarBoundOrConstraints<'db>, Self::Error> {
        match typevar.typevar(self.db).bound_or_constraints(self.db, env) {
            None => unreachable!(),
            Some(bounds) => Ok(bounds),
        }
    }

    fn constraint_types(
        &self,
        constraints: TypeVarConstraints<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(constraints.as_type(self.db, env))
    }

    fn lookup(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error> {
        Ok(ty.find_name_in_mro_with_policy(self.db, env, name, policy))
    }
}
