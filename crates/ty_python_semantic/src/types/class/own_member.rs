//! Shared own-member lookup with direct and queued dependency providers.

use std::convert::Infallible;

use ruff_python_ast::name::Name;

use crate::place::Place;
use crate::types::class::dunder_callable::{
    DunderCallableFacts, DunderCallableTransform, OrdinaryDunderCallableEffects,
    dunder_callable_sync,
};
use crate::types::class::{
    ClassLiteral, ClassType, CodeGeneratorKind, DynamicClassLiteral, DynamicEnumLiteral,
    DynamicNamedTupleLiteral, DynamicTypedDictLiteral, MethodDecorator, StaticClassLiteral,
};
use crate::types::enums::{
    enum_metadata, is_enum_class_by_inheritance, try_unwrap_nonmember_value,
};
use crate::types::generics::Specialization;
use crate::types::member::{Member, class_member};
use crate::types::tuple::TupleSpec;
use crate::types::{
    FunctionType, GenericAlias, GenericContext, KnownClass, Parameter, Parameters,
    PropertyInstanceType, Signature, Type, TypeQualifiers,
};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

pub(in crate::types) fn into_dunder_paramspec_callable<'d>(
    db: &'d dyn Db,
    env: &ProgramEnvironment<'d>,
    ty: Type<'d>,
) -> Type<'d> {
    match dunder_callable_sync(
        ty,
        DunderCallableTransform::DunderParamSpec,
        DunderCallableFacts,
        &OrdinaryDunderCallableEffects { db, env },
    ) {
        Ok(ty) => ty,
        Err(never) => match never {},
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct OwnMemberLookupRequest<'a, 'db> {
    pub(in crate::types) class: StaticClassLiteral<'db>,
    pub(in crate::types) name: &'a str,
    pub(in crate::types) inherited_generic_context: Option<GenericContext<'db>>,
    pub(in crate::types) specialization: Option<Specialization<'db>>,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

#[derive(Clone, Copy)]
pub(in crate::types) struct ClassTypeOwnMemberRequest<'a, 'db> {
    pub(in crate::types) class: ClassType<'db>,
    pub(in crate::types) name: &'a str,
    pub(in crate::types) inherited_generic_context: Option<GenericContext<'db>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ClassTypeOwnMemberWork {
    Admission {
        name_bytes: usize,
    },
    Dispatch,
    TupleClass,
    TuplePayload,
    /// Dispatch one dependency; its provider accounts for the operation's work.
    Dependency,
    Publish,
}

pub(in crate::types) trait ClassTypeOwnMemberEffects<'db>: sealed::Sealed {
    async fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error>;
    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;
    async fn is_tuple(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    async fn specialization_tuple(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Self::Error>;
    type Error;

    async fn checkpoint(&self, work: ClassTypeOwnMemberWork) -> Result<(), Self::Error>;

    async fn dynamic_member(
        &self,
        class: DynamicClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;

    async fn named_tuple_member(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;

    async fn typed_dict_member(
        &self,
        class: DynamicTypedDictLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;

    async fn enum_member(
        &self,
        class: DynamicEnumLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;

    async fn tuple_len(
        &self,
        class: ClassType<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Member<'db>, Self::Error>;

    async fn tuple_getitem(&self, tuple: &'db TupleSpec<'db>) -> Result<Member<'db>, Self::Error>;

    async fn tuple_new(
        &self,
        class: ClassType<'db>,
        specialization: Option<Specialization<'db>>,
        context: Option<GenericContext<'db>>,
    ) -> Result<Member<'db>, Self::Error>;

    async fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;

    async fn static_own_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error>;

    async fn owner_specialize(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
    ) -> Result<Type<'db>, Self::Error>;
}

pub(in crate::types) trait SynchronousClassTypeOwnMemberEffects<'db>:
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
    fn is_tuple(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    fn specialization_tuple(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Self::Error>;
    type Error;

    fn checkpoint(&self, work: ClassTypeOwnMemberWork) -> Result<(), Self::Error>;

    fn dynamic_member(
        &self,
        class: DynamicClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;

    fn named_tuple_member(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;

    fn typed_dict_member(
        &self,
        class: DynamicTypedDictLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;

    fn enum_member(
        &self,
        class: DynamicEnumLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error>;

    fn tuple_len(
        &self,
        class: ClassType<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Member<'db>, Self::Error>;

    fn tuple_getitem(&self, tuple: &'db TupleSpec<'db>) -> Result<Member<'db>, Self::Error>;

    fn tuple_new(
        &self,
        class: ClassType<'db>,
        specialization: Option<Specialization<'db>>,
        context: Option<GenericContext<'db>>,
    ) -> Result<Member<'db>, Self::Error>;

    fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error>;

    fn static_own_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error>;

    fn owner_specialize(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
    ) -> Result<Type<'db>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_class_type_own_member]
pub(in crate::types) async fn class_type_own_member_with<
    'a,
    'db,
    E: ClassTypeOwnMemberEffects<'db>,
>(
    request: ClassTypeOwnMemberRequest<'a, 'db>,
    effects: &E,
) -> Result<Member<'db>, E::Error> {
    let ClassTypeOwnMemberRequest {
        class,
        name,
        inherited_generic_context,
    } = request;
    effects
        .checkpoint(ClassTypeOwnMemberWork::Admission {
            name_bytes: name.len(),
        })
        .await?;
    effects.checkpoint(ClassTypeOwnMemberWork::Dispatch).await?;
    let (class_literal, specialization) = match class {
        ClassType::NonGeneric(ClassLiteral::Dynamic(dynamic)) => {
            effects
                .checkpoint(ClassTypeOwnMemberWork::Dependency)
                .await?;
            let member = effects.dynamic_member(dynamic, name).await?;
            effects.checkpoint(ClassTypeOwnMemberWork::Publish).await?;
            return Ok(member);
        }
        ClassType::NonGeneric(ClassLiteral::DynamicNamedTuple(namedtuple)) => {
            effects
                .checkpoint(ClassTypeOwnMemberWork::Dependency)
                .await?;
            let member = effects.named_tuple_member(namedtuple, name).await?;
            effects.checkpoint(ClassTypeOwnMemberWork::Publish).await?;
            return Ok(member);
        }
        ClassType::NonGeneric(ClassLiteral::DynamicTypedDict(typeddict)) => {
            effects
                .checkpoint(ClassTypeOwnMemberWork::Dependency)
                .await?;
            let member = effects.typed_dict_member(typeddict, name).await?;
            effects.checkpoint(ClassTypeOwnMemberWork::Publish).await?;
            return Ok(member);
        }
        ClassType::NonGeneric(ClassLiteral::DynamicEnum(enum_lit)) => {
            effects
                .checkpoint(ClassTypeOwnMemberWork::Dependency)
                .await?;
            let member = effects.enum_member(enum_lit, name).await?;
            effects.checkpoint(ClassTypeOwnMemberWork::Publish).await?;
            return Ok(member);
        }
        ClassType::NonGeneric(ClassLiteral::Static(class)) => (class, None),
        ClassType::Generic(generic) => (
            effects.alias_origin(generic).await?,
            Some(effects.alias_specialization(generic).await?),
        ),
    };

    match name {
        "__len__"
            if {
                effects
                    .checkpoint(ClassTypeOwnMemberWork::TupleClass)
                    .await?;
                effects.is_tuple(class_literal).await?
            } =>
        {
            effects
                .checkpoint(ClassTypeOwnMemberWork::Dependency)
                .await?;
            let member = effects.tuple_len(class, specialization).await?;
            effects.checkpoint(ClassTypeOwnMemberWork::Publish).await?;
            return Ok(member);
        }
        "__getitem__"
            if {
                effects
                    .checkpoint(ClassTypeOwnMemberWork::TupleClass)
                    .await?;
                effects.is_tuple(class_literal).await?
            } =>
        {
            effects
                .checkpoint(ClassTypeOwnMemberWork::TuplePayload)
                .await?;
            let tuple = match specialization {
                Some(specialization) => effects.specialization_tuple(specialization).await?,
                None => None,
            };
            if let Some(tuple) = tuple {
                effects
                    .checkpoint(ClassTypeOwnMemberWork::Dependency)
                    .await?;
                let member = effects.tuple_getitem(tuple).await?;
                effects.checkpoint(ClassTypeOwnMemberWork::Publish).await?;
                return Ok(member);
            }
        }
        "__new__"
            if {
                effects
                    .checkpoint(ClassTypeOwnMemberWork::TupleClass)
                    .await?;
                effects.is_tuple(class_literal).await?
            } =>
        {
            effects
                .checkpoint(ClassTypeOwnMemberWork::Dependency)
                .await?;
            let member = effects
                .tuple_new(class, specialization, inherited_generic_context)
                .await?;
            effects.checkpoint(ClassTypeOwnMemberWork::Publish).await?;
            return Ok(member);
        }
        _ => {}
    }

    // Tuple methods use the original tuple shape. Other members use the runtime element type.
    let specialization = if let Some(specialization) = specialization {
        effects
            .checkpoint(ClassTypeOwnMemberWork::Dependency)
            .await?;
        Some(effects.tuple_runtime_specialization(specialization).await?)
    } else {
        None
    };
    effects
        .checkpoint(ClassTypeOwnMemberWork::Dependency)
        .await?;
    let mut member = effects
        .static_own_member(OwnMemberLookupRequest {
            class: class_literal,
            name,
            inherited_generic_context,
            specialization,
        })
        .await?;
    if let Place::Defined(defined) = &mut member.inner.place
        && let Some(specialization) = specialization
    {
        effects
            .checkpoint(ClassTypeOwnMemberWork::Dependency)
            .await?;
        defined.ty = effects.owner_specialize(defined.ty, specialization).await?;
    }
    effects.checkpoint(ClassTypeOwnMemberWork::Publish).await?;
    Ok(member)
}

pub(in crate::types) struct InlineClassTypeOwnMemberEffects<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}

impl<'env, 'db> InlineClassTypeOwnMemberEffects<'env, 'db> {
    pub(in crate::types) fn new(db: &'db dyn Db, env: &'env ProgramEnvironment<'db>) -> Self {
        Self { db, env }
    }
}

impl sealed::Sealed for InlineClassTypeOwnMemberEffects<'_, '_> {}

impl<'db> SynchronousClassTypeOwnMemberEffects<'db> for InlineClassTypeOwnMemberEffects<'_, 'db> {
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
    fn is_tuple(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.is_tuple(self.db))
    }
    fn specialization_tuple(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Self::Error> {
        Ok(specialization.tuple(self.db))
    }
    type Error = Infallible;

    fn checkpoint(&self, _work: ClassTypeOwnMemberWork) -> Result<(), Self::Error> {
        Ok(())
    }

    fn dynamic_member(
        &self,
        class: DynamicClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(class.own_class_member(self.db, name))
    }

    fn named_tuple_member(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(class.own_class_member(self.db, name))
    }

    fn typed_dict_member(
        &self,
        class: DynamicTypedDictLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(class.own_class_member(self.db, name))
    }

    fn enum_member(
        &self,
        class: DynamicEnumLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(class.own_class_member(self.db, name))
    }

    fn tuple_len(
        &self,
        class: ClassType<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(class.tuple_len_member(self.db, self.env, specialization))
    }

    fn tuple_getitem(&self, tuple: &'db TupleSpec<'db>) -> Result<Member<'db>, Self::Error> {
        Ok(ClassType::tuple_getitem_member(self.db, self.env, tuple))
    }

    fn tuple_new(
        &self,
        class: ClassType<'db>,
        specialization: Option<Specialization<'db>>,
        context: Option<GenericContext<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(class.tuple_new_member(self.db, self.env, specialization, context))
    }

    fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(specialization.tuple_runtime_element_specialization(self.db))
    }

    fn static_own_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(request.class.own_class_member(
            self.db,
            self.env,
            request.inherited_generic_context,
            request.specialization,
            request.name,
        ))
    }

    fn owner_specialize(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.apply_optional_owner_specialization_to_member(self.db, Some(specialization)))
    }
}

/// Each dependency is admitted separately, before querying or constructing its result.
pub(in crate::types) trait OwnMemberEffects<'db>: sealed::Sealed {
    type Error;
    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;

    async fn raw_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error>;

    async fn slot_exists(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<bool, Self::Error>;

    async fn generated_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;

    async fn explicit_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;

    async fn implicit_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error>;

    async fn is_kw_only(&self, ty: Type<'db>) -> Result<bool, Self::Error>;

    async fn is_enum_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<bool, Self::Error>;

    async fn is_enum_class(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;

    async fn checkpoint(&self) -> Result<(), Self::Error>;

    async fn dataclass_fields(&self) -> Result<Type<'db>, Self::Error>;
    async fn named_tuple_field(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    async fn named_tuple_property(&self, field_type: Type<'db>) -> Result<Type<'db>, Self::Error>;

    async fn dunder_paramspec(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
    async fn constructor_context(
        &self,
        function: FunctionType<'db>,
        context: GenericContext<'db>,
    ) -> Result<FunctionType<'db>, Self::Error>;

    async fn slot_descriptor(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn synthesized_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    async fn nonmember_value(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
}

/// The direct lowering uses the same operation contract without constructing futures.
pub(in crate::types) trait SynchronousOwnMemberEffects<'db>:
    sealed::Sealed
{
    type Error;
    fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;

    fn raw_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error>;

    fn slot_exists(&self, request: OwnMemberLookupRequest<'_, 'db>) -> Result<bool, Self::Error>;

    fn generated_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;

    fn explicit_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;

    fn implicit_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error>;

    fn is_kw_only(&self, ty: Type<'db>) -> Result<bool, Self::Error>;

    fn is_enum_member(&self, request: OwnMemberLookupRequest<'_, 'db>)
    -> Result<bool, Self::Error>;

    fn is_enum_class(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;

    fn checkpoint(&self) -> Result<(), Self::Error>;

    fn dataclass_fields(&self) -> Result<Type<'db>, Self::Error>;
    fn named_tuple_field(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    fn named_tuple_property(&self, field_type: Type<'db>) -> Result<Type<'db>, Self::Error>;

    fn dunder_paramspec(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
    fn constructor_context(
        &self,
        function: FunctionType<'db>,
        context: GenericContext<'db>,
    ) -> Result<FunctionType<'db>, Self::Error>;

    fn slot_descriptor(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error>;

    fn synthesized_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    fn nonmember_value(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_own_member]
#[inline]
pub(in crate::types) async fn own_class_member_with<'a, 'db, E: OwnMemberEffects<'db>>(
    request: OwnMemberLookupRequest<'a, 'db>,
    effects: &E,
) -> Result<Member<'db>, E::Error> {
    let OwnMemberLookupRequest {
        class,
        name,
        inherited_generic_context,
        specialization,
    } = request;

    // Check if this class is dataclass-like (either via @dataclass or via dataclass_transform).
    effects.checkpoint().await?;
    if effects
        .code_generator(class)
        .await?
        .is_some_and(CodeGeneratorKind::is_dataclass_like)
    {
        if name == "__dataclass_fields__" {
            // Make this class look like a subclass of the `DataClassInstance` protocol.
            effects.checkpoint().await?;
            let ty = effects.dataclass_fields().await?;
            let member = Member {
                inner: Place::declared(ty).with_qualifiers(TypeQualifiers::CLASS_VAR),
            };
            effects.checkpoint().await?;
            return Ok(member);
        } else if name == "__dataclass_params__" {
            // There is no typeshed class for this. For now, we model it as `Any`.
            let member = Member {
                inner: Place::declared(Type::any()).with_qualifiers(TypeQualifiers::CLASS_VAR),
            };
            effects.checkpoint().await?;
            return Ok(member);
        }
    }

    effects.checkpoint().await?;
    if let Some(CodeGeneratorKind::NamedTuple) = effects.code_generator(class).await? {
        effects.checkpoint().await?;
        if let Some(field_type) = effects.named_tuple_field(request).await? {
            effects.checkpoint().await?;
            let property = effects.named_tuple_property(field_type).await?;
            let member = Member::definitely_declared(property);
            effects.checkpoint().await?;
            return Ok(member);
        }
    }

    effects.checkpoint().await?;
    let mut member = effects.raw_member(request).await?;
    if let Place::Defined(defined) = &mut member.inner.place {
        // Replacing a raw handle preserves qualifiers, provenance, and the public-type policy.
        if name.starts_with("__") && name.ends_with("__") {
            effects.checkpoint().await?;
            defined.ty = effects.dunder_paramspec(defined.ty).await?;
        }

        // The `__new__` and `__init__` members of a non-specialized generic class are handled
        // specially: they inherit the generic context of their class. That lets us treat them
        // as generic functions when constructing the class, and infer the specialization of
        // the class from the arguments that are passed in.
        //
        // We might decide to handle other class methods the same way, having them inherit the
        // class's generic context, and performing type inference on calls to them to determine
        // the specialization of the class. If we do that, we would update this to also apply
        // to any method with a `@classmethod` decorator. (`__init__` would remain a special
        // case, since it's an _instance_ method where we don't yet know the generic class's
        // specialization.)
        if let (Some(context), Type::FunctionLiteral(function), Some(_), "__new__" | "__init__") =
            (inherited_generic_context, defined.ty, specialization, name)
        {
            effects.checkpoint().await?;
            defined.ty =
                Type::FunctionLiteral(effects.constructor_context(function, context).await?);
        }
    }

    effects.checkpoint().await?;
    if effects.slot_exists(request).await? {
        effects.checkpoint().await?;
        let descriptor = effects.slot_descriptor(request).await?;
        let member = Member::definitely_declared(descriptor);
        effects.checkpoint().await?;
        return Ok(member);
    }

    if member.is_undefined()
        || name == "__slots__"
            && {
                effects.checkpoint().await?;
                effects.generated_slots(class).await?
            }
            && {
                effects.checkpoint().await?;
                !effects.explicit_slots(class).await?
            }
    {
        effects.checkpoint().await?;
        if let Some(ty) = effects.synthesized_member(request).await? {
            let member = Member::definitely_declared(ty);
            effects.checkpoint().await?;
            return Ok(member);
        }
        // The symbol was not found in the class scope. It might still be implicitly defined in `@classmethod`s.
        effects.checkpoint().await?;
        let member = effects.implicit_member(request).await?;
        effects.checkpoint().await?;
        return Ok(member);
    }

    // For dataclass-like classes, `KW_ONLY` sentinel fields are not real
    // class attributes; they are markers used by the dataclass decorator to
    // indicate that subsequent fields are keyword-only. Treat them as
    // undefined so the MRO falls through to parent classes.
    if let Some(ty) = member.inner.place.raw_type()
        && {
            effects.checkpoint().await?;
            effects.is_kw_only(ty).await?
        }
    {
        effects.checkpoint().await?;
        if effects
            .code_generator(class)
            .await?
            .is_some_and(CodeGeneratorKind::is_dataclass_like)
        {
            effects.checkpoint().await?;
            return Ok(Member::unbound());
        }
    }

    // Enum members are read-only on the class, but instances can shadow them.
    effects.checkpoint().await?;
    if effects.is_enum_member(request).await? {
        member.inner.qualifiers.insert(TypeQualifiers::READ_ONLY);
        effects.checkpoint().await?;
        return Ok(member);
    }

    // For enum classes, `nonmember(value)` creates a non-member attribute.
    // At runtime, the enum metaclass unwraps the value, so accessing the attribute
    // returns the inner value, not the `nonmember` wrapper.
    if let Some(ty) = member.inner.place.raw_type()
        && let Some(value) = {
            effects.checkpoint().await?;
            effects.nonmember_value(ty).await?
        }
        && {
            effects.checkpoint().await?;
            effects.is_enum_class(class).await?
        }
    {
        let member = Member::definitely_declared(value);
        effects.checkpoint().await?;
        return Ok(member);
    }

    effects.checkpoint().await?;
    Ok(member)
}

pub(in crate::types) struct InlineOwnMemberEffects<'env, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
}

impl<'env, 'db> InlineOwnMemberEffects<'env, 'db> {
    #[inline]
    pub(in crate::types) fn new(db: &'db dyn Db, env: &'env ProgramEnvironment<'db>) -> Self {
        Self { db, env }
    }
}

impl sealed::Sealed for InlineOwnMemberEffects<'_, '_> {}

impl<'db> SynchronousOwnMemberEffects<'db> for InlineOwnMemberEffects<'_, 'db> {
    type Error = Infallible;

    #[inline]
    fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        Ok(CodeGeneratorKind::from_class(self.db, class.into()))
    }

    #[inline]
    fn raw_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(class_member(
            self.db,
            request.class.body_scope(self.db),
            request.name,
        ))
    }

    #[inline]
    fn slot_exists(&self, request: OwnMemberLookupRequest<'_, 'db>) -> Result<bool, Self::Error> {
        Ok(request.class.has_own_slot_descriptor(self.db, request.name))
    }

    #[inline]
    fn generated_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.has_generated_slots(self.db))
    }

    #[inline]
    fn explicit_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.has_explicit_slots(self.db))
    }

    #[inline]
    fn implicit_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(request
            .class
            .implicit_attribute(self.db, request.name, MethodDecorator::ClassMethod))
    }

    #[inline]
    fn is_kw_only(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(ty.is_instance_of(self.db, KnownClass::KwOnly))
    }

    #[inline]
    fn is_enum_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<bool, Self::Error> {
        Ok(enum_metadata(self.db, request.class.into())
            .is_some_and(|metadata| metadata.contains_member(request.name)))
    }

    #[inline]
    fn is_enum_class(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(is_enum_class_by_inheritance(self.db, self.env, class))
    }
    #[inline]
    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    #[inline]
    fn dataclass_fields(&self) -> Result<Type<'db>, Self::Error> {
        Ok(KnownClass::Dict.to_specialized_instance(
            self.db,
            self.env,
            &[
                KnownClass::Str.to_instance(self.db, self.env),
                KnownClass::Field.to_specialized_instance(self.db, self.env, &[Type::any()]),
            ],
        ))
    }

    #[inline]
    fn named_tuple_field(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(request
            .class
            .own_fields(
                self.db,
                request.specialization,
                CodeGeneratorKind::NamedTuple,
            )
            .get(request.name)
            .map(|field| field.declared_ty))
    }

    #[inline]
    fn named_tuple_property(&self, field_type: Type<'db>) -> Result<Type<'db>, Self::Error> {
        let property_getter_signature = Signature::new(
            Parameters::standard([Parameter::positional_only(Some(Name::new_static("self")))]),
            field_type,
        );
        let property_getter = Type::single_callable(self.db, property_getter_signature);
        let property = PropertyInstanceType::new(self.db, Some(property_getter), None, None);
        Ok(Type::PropertyInstance(property))
    }

    #[inline]
    fn dunder_paramspec(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(into_dunder_paramspec_callable(self.db, self.env, ty))
    }

    #[inline]
    fn constructor_context(
        &self,
        function: FunctionType<'db>,
        context: GenericContext<'db>,
    ) -> Result<FunctionType<'db>, Self::Error> {
        Ok(function.with_inherited_generic_context(self.db, context))
    }

    #[inline]
    fn slot_descriptor(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(request.class.own_slot_descriptor(
            self.db,
            self.env,
            request.specialization,
            request.name,
        ))
    }

    #[inline]
    fn synthesized_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(request.class.own_synthesized_member(
            self.db,
            self.env,
            request.specialization,
            request.inherited_generic_context,
            request.name,
        ))
    }

    #[inline]
    fn nonmember_value(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(try_unwrap_nonmember_value(self.db, self.env, ty))
    }
}
