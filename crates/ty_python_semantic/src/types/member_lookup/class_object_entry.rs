//! Complete class-object lookup, from receiver selection through descriptor and subclass finalization.

use std::convert::Infallible;

use ruff_python_ast::name::Name;

use crate::place::{Place, PlaceAndQualifiers};
use crate::types::enums::EnumClassLiteral;
use crate::types::{
    AttributeDescriptorResult, CallableRecursionGuard, ClassLiteral, ClassType, DynamicType,
    EnumLiteralType, InstanceFallbackShadowsNonDataDescriptor, IntersectionType, LookupParts,
    MemberLookupErrorKind, MemberLookupKey, MemberLookupPolicy, MemberLookupResult,
    SubclassOfInner, SubclassOfType, Type, map_member_lookup_type,
    member_lookup_result_with_origin, promote_inferred_attribute_class_literals,
};
use crate::{Db, ProgramEnvironment};

/// The original lookup inputs, retaining the canonical name and any explicit receiver.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct ClassObjectEntryRequest<'name, 'db> {
    pub(in crate::types) key: MemberLookupKey<'db>,
    pub(in crate::types) ty: Type<'db>,
    pub(in crate::types) name: &'name Name,
    pub(in crate::types) policy: MemberLookupPolicy,
    pub(in crate::types) receiver: Option<Type<'db>>,
}

/// Fixed decisions between the semantic children of class-object lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::types) enum ClassObjectEntryWork {
    Receiver,
    Enum,
    PlainMember,
    AttributeResult,
    Finalize,
}

/// Finite receiver, place and subclass inspections for class-object lookup.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct ClassObjectEntryFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies database, conversion and descriptor children for the complete class-object operation.
    #[synchronous(SynchronousClassObjectEntryEffects)]
    pub(in crate::types) trait ClassObjectEntryEffects<'db> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self, work: ClassObjectEntryWork) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn receiver_instance(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn subclass_class(&self, inner: SubclassOfInner<'db>) -> Result<Option<ClassType<'db>>, Self::Error>;
        #[operation(child)]
        async fn class_literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Self::Error>;
        #[operation(child)]
        async fn enum_class(&self, class: ClassLiteral<'db>) -> Result<Option<EnumClassLiteral<'db>>, Self::Error>;
        #[operation(child)]
        async fn enum_member(&self, class: EnumClassLiteral<'db>, name: &Name) -> Result<Option<&'db Name>, Self::Error>;
        #[operation(child)]
        async fn enum_literal(&self, class: EnumClassLiteral<'db>, name: &Name) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn plain_member(&self, ty: Type<'db>, name: &Name, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn bind_self(&self, ty: Type<'db>, receiver: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn attribute_descriptor(&self, attribute: PlaceAndQualifiers<'db>, receiver: Type<'db>) -> Result<AttributeDescriptorResult<'db>, Self::Error>;
        #[operation(child)]
        async fn result(&self, parts: LookupParts<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
        #[operation(child)]
        async fn invoke_descriptor(&self, key: MemberLookupKey<'db>, receiver: Type<'db>, fallback: MemberLookupResult<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
        #[operation(child)]
        async fn fallback(&self, ty: Type<'db>, name: &Name, result: MemberLookupResult<'db>, policy: MemberLookupPolicy) -> Result<MemberLookupResult<'db>, Self::Error>;
        #[operation(child)]
        async fn typevar_upper_bound(&self, subclass: SubclassOfType<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn promote(&self, result: MemberLookupResult<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
        #[operation(child)]
        async fn dynamic_result(&self, result: MemberLookupResult<'db>, dynamic: DynamicType<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
    }

    #[finite_capability]
    impl ClassObjectEntryFacts {
        fn inner<'db>(&self, subclass: SubclassOfType<'db>) -> SubclassOfInner<'db> { subclass.subclass_of() }
        fn raw_type<'db>(&self, member: PlaceAndQualifiers<'db>) -> Option<Type<'db>> { member.place.ignore_possibly_undefined() }
        fn mapped<'db>(&self, member: PlaceAndQualifiers<'db>, ty: Type<'db>) -> PlaceAndQualifiers<'db> { member.map_type(|_| ty) }
        fn no_exact(&self, exact: Option<Type<'_>>) -> bool { exact.is_none() }
        fn bound<'db>(&self, ty: Type<'db>) -> MemberLookupResult<'db> { MemberLookupResult::from(Place::bound(ty)) }
        fn parts<'db>(&self, result: AttributeDescriptorResult<'db>) -> LookupParts<'db> {
            LookupParts { member: result.member, error: result.error.map(MemberLookupErrorKind::DescriptorGet), properties: None, descriptor: result.origin }
        }
    }

    /// Resolves a class-object member with ordinary enum, descriptor and metaclass precedence.
    ///
    /// The explicit receiver survives only when it denotes instantiable class objects. Its
    /// later instance conversion remains after namespace lookup so Self binding preserves
    /// the ordinary semantic read order.
    #[synchronous(class_object_entry_sync)]
    #[capabilities(effects = ClassObjectEntryEffects, facts = ClassObjectEntryFacts)]
    #[passive_values(ClassObjectEntryWork::Receiver, ClassObjectEntryWork::Enum, ClassObjectEntryWork::PlainMember, ClassObjectEntryWork::AttributeResult, ClassObjectEntryWork::Finalize)]
    pub(in crate::types) async fn class_object_entry_with<'db, E: ClassObjectEntryEffects<'db>>(
        request: ClassObjectEntryRequest<'_, 'db>, facts: ClassObjectEntryFacts, effects: &E,
    ) -> Result<MemberLookupResult<'db>, E::Error> {
        effects.checkpoint(ClassObjectEntryWork::Receiver).await?;
        // A class-object lookup can originate from a TypeVar bound such as `type[A]`.
        // Retain that TypeVar as the receiver so `Self` binds to `T'instance`, not `A`,
        // unless its constraints also include non-class-object types.
        let receiver = match request.receiver {
            Some(receiver) => match effects.instance_approximation(receiver).await? {
                Some(_) => receiver,
                None => request.ty,
            },
            None => request.ty,
        };

        effects.checkpoint(ClassObjectEntryWork::Enum).await?;
        let enum_class = match request.ty {
            Type::ClassLiteral(literal) => effects.enum_class(literal).await?,
            Type::SubclassOf(subclass) => match effects.subclass_class(facts.inner(subclass)).await? {
                Some(class) => {
                    let literal = effects.class_literal(class).await?;
                    effects.enum_class(literal).await?
                }
                None => None,
            },
            _ => None,
        };
        if let Some(class) = enum_class
            && let Some(name) = effects.enum_member(class, request.name).await?
        {
            return Ok(facts.bound(effects.enum_literal(class, name).await?));
        }

        let plain = effects.plain_member(request.ty, request.name, request.policy).await?;
        let instance = effects.receiver_instance(receiver).await?;
        effects.checkpoint(ClassObjectEntryWork::PlainMember).await?;
        let plain = match facts.raw_type(plain) {
            Some(ty) => facts.mapped(plain, effects.bind_self(ty, instance).await?),
            None => plain,
        };
        let attribute = effects.attribute_descriptor(plain, receiver).await?;
        effects.checkpoint(ClassObjectEntryWork::AttributeResult).await?;
        let fallback = effects.result(facts.parts(attribute)).await?;
        let result = effects.invoke_descriptor(request.key, receiver, fallback).await?;
        // A class is an instance of its metaclass. If attribute lookup on the class
        // fails, Python falls back to `type(cls).__getattr__` and
        // `type(cls).__getattribute__` on the metaclass, analogous to how instance
        // attribute access falls back to `__getattr__`/`__getattribute__` on the
        // class. `try_call_dunder` adds `NO_INSTANCE_FALLBACK`, which causes the
        // lookup to hit the catch-all that only checks the meta-type (the metaclass).
        let result = effects.fallback(request.ty, request.name, result, request.policy).await?;

        effects.checkpoint(ClassObjectEntryWork::Finalize).await?;
        // Unlike a specific class literal, `type[C]` can represent any subclass of
        // `C`, unless a `TypeVar` upper bound normalizes to a final class.
        let result = match request.ty {
            Type::SubclassOf(subclass) => {
                let exact = match facts.inner(subclass) {
                    SubclassOfInner::TypeVar(_) => effects.typevar_upper_bound(subclass).await?,
                    SubclassOfInner::Class(_) | SubclassOfInner::Protocol(_) | SubclassOfInner::Dynamic(_) => None,
                };
                if facts.no_exact(exact) { effects.promote(result).await? } else { result }
            }
            _ => result,
        };
        // `type[Any]`/`type[Unknown]` are gradual forms with an unknown metaclass
        // (which is at least `type`). Attributes resolved via `type`'s descriptors
        // are intersected with the dynamic type to reflect uncertainty about
        // whether the unknown metaclass overrides them.
        if let Type::SubclassOf(subclass) = request.ty
            && let SubclassOfInner::Dynamic(dynamic) = facts.inner(subclass)
        {
            effects.dynamic_result(result, dynamic).await
        } else {
            Ok(result)
        }
    }
}

/// Ordinary children retain the caller's environment and callable recursion guard.
pub(in crate::types) struct OrdinaryClassObjectEntry<'env, 'guard, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'env ProgramEnvironment<'db>,
    pub(in crate::types) guard: Option<&'guard CallableRecursionGuard<'db>>,
}

impl<'db> SynchronousClassObjectEntryEffects<'db> for OrdinaryClassObjectEntry<'_, '_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self, _work: ClassObjectEntryWork) -> Result<(), Infallible> {
        Ok(())
    }
    fn instance_approximation(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Infallible> {
        Ok(ty.to_instance_approximation(self.db, self.env))
    }
    fn receiver_instance(&self, ty: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(ty
            .to_instance_approximation(self.db, self.env)
            .expect("The receiver for a class-object lookup should always be instantiable"))
    }
    fn subclass_class(
        &self,
        inner: SubclassOfInner<'db>,
    ) -> Result<Option<ClassType<'db>>, Infallible> {
        Ok(inner.into_class(self.db, self.env))
    }
    fn class_literal(&self, class: ClassType<'db>) -> Result<ClassLiteral<'db>, Infallible> {
        Ok(class.class_literal(self.db))
    }
    fn enum_class(
        &self,
        class: ClassLiteral<'db>,
    ) -> Result<Option<EnumClassLiteral<'db>>, Infallible> {
        Ok(class.into_enum_class(self.db))
    }
    fn enum_member(
        &self,
        class: EnumClassLiteral<'db>,
        name: &Name,
    ) -> Result<Option<&'db Name>, Infallible> {
        Ok(class.resolve_member(self.db, name))
    }
    fn enum_literal(
        &self,
        class: EnumClassLiteral<'db>,
        name: &Name,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::enum_literal(EnumLiteralType::new(
            self.db, class, name,
        )))
    }
    fn plain_member(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(ty.class_object_member(self.db, self.env, name.as_str(), policy))
    }
    fn bind_self(&self, ty: Type<'db>, receiver: Type<'db>) -> Result<Type<'db>, Infallible> {
        Ok(ty.bind_self_typevars(self.db, self.env, receiver))
    }
    fn attribute_descriptor(
        &self,
        attribute: PlaceAndQualifiers<'db>,
        receiver: Type<'db>,
    ) -> Result<AttributeDescriptorResult<'db>, Infallible> {
        let (member, kind, error, origin) = Type::try_call_dunder_get_on_attribute(
            self.db, self.env, attribute, None, receiver, self.guard,
        );
        Ok(AttributeDescriptorResult {
            member,
            kind,
            error,
            origin,
        })
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
    fn invoke_descriptor(
        &self,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
        fallback: MemberLookupResult<'db>,
    ) -> Result<MemberLookupResult<'db>, Infallible> {
        Ok(Type::invoke_descriptor_protocol(
            self.db,
            self.env,
            key,
            receiver,
            fallback,
            InstanceFallbackShadowsNonDataDescriptor::Yes,
            self.guard,
        ))
    }
    fn fallback(
        &self,
        ty: Type<'db>,
        name: &Name,
        result: MemberLookupResult<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<MemberLookupResult<'db>, Infallible> {
        Ok(ty.fallback_to_getattr(self.db, self.env, name, result, policy))
    }
    fn typevar_upper_bound(
        &self,
        subclass: SubclassOfType<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(subclass.exact_typevar_upper_bound(self.db, self.env))
    }
    fn promote(
        &self,
        result: MemberLookupResult<'db>,
    ) -> Result<MemberLookupResult<'db>, Infallible> {
        Ok(promote_inferred_attribute_class_literals(
            self.db, self.env, result,
        ))
    }
    fn dynamic_result(
        &self,
        result: MemberLookupResult<'db>,
        dynamic: DynamicType<'db>,
    ) -> Result<MemberLookupResult<'db>, Infallible> {
        Ok(map_member_lookup_type(self.db, result, |ty| {
            if ty.is_dynamic() {
                ty
            } else {
                IntersectionType::from_two_elements(self.db, self.env, ty, Type::Dynamic(dynamic))
            }
        }))
    }
}
