//! The ordered member-lookup dispatch shared by ordinary and controlled execution.

use std::convert::Infallible;

use super::class_object_entry::{ClassObjectEntryFacts, ClassObjectEntryRequest, OrdinaryClassObjectEntry, class_object_entry_sync};

use ruff_python_ast as ast;
use ruff_python_ast::name::Name;

use crate::Db;
use crate::place::{DefinedPlace, Place};
use crate::types::enums::EnumMetadata;
use crate::types::known_instance::{FunctoolsPartialInstance, InternedType, MethodWrapper};
use crate::types::{
    BoundMethodType, BoundSuperType, BoundTypeVarInstance, CallableRecursionGuard, CallableType,
    ClassLiteral, DescriptorOrigin, EnumComplementType, EnumLiteralType, FunctionType,
    InlineMemberEntry, IntersectionType,
    KnownBoundMethodType, KnownClass, KnownInstanceType, LiteralValueType, LookupFacts,
    MemberLookupKey, MemberLookupPolicy, MemberLookupResult,
    ModuleLiteralType, NewType, NominalInstanceType, ParamSpecAttrKind, ProgramEnvironment,
    PropertyDeprecations, PropertyInstanceType, ProtocolInstanceType, RecursiveType,
    StringLiteralType, Type, TypeAliasType, UnionType, WrapperDescriptorKind,
    distribute_member_lookup_over_bound_or_constraints, enum_metadata, enums,
    instance_member_entry_sync, member_lookup_or_fall_back_to,
    member_lookup_result_with_origin, member_lookup_with_policy_impl,
    member_lookup_with_policy_inner,
    restricted_member_entry_sync, union_deprecated_properties,
};

/// Borrows the caller's spelling and retains an existing shared name when available.
#[derive(Clone, Copy)]
pub(in crate::types) enum GeneralMemberName<'a> {
    Text(&'a str),
    Shared(&'a Name),
}

impl<'a> GeneralMemberName<'a> {
    pub(in crate::types) fn as_str(self) -> &'a str {
        match self {
            Self::Text(name) => name,
            Self::Shared(name) => name.as_str(),
        }
    }
}

/// A selected dependency of general member lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(any(test, feature = "experimental-analysis"))]
pub enum GeneralMemberOperation {
    Cycle,
    ExplicitReceiver,
    RecursionGuard,
    DunderClass,
    Lookup,
    ClassMemberDispatch,
    MetaType,
    NamespaceLookup,
    SubclassMro,
    PropertyWrapper,
    InstanceStorage,
    ImplicitAttributeInference,
    InstanceApproximation,
    MemberSelfBinding,
    InferredAttributePromotion,
    GetattrFallback,
    FunctionLike,
    FunctionWrapper,
    ConstraintSetClass,
    FunctionTypeClass,
    CallableFunctionOrStaticmethod,
    CallableStaticOrClassmethod,
    ProtocolOrigin,
    ProtocolMember,
    NewTypeUnion,
    EnumLiteralProperty,
    ParamSpec,
    WrapperDescriptor,
    CallableRuntimeClass,
    NominalEnumMember,
    Bound,
    Recursive,
    RecursiveVar,
    Union,
    Intersection,
    EnumComplement,
    FunctionDunderGet,
    DunderCall,
    UnderlyingFunction,
    PropertyDunderGet,
    PropertyDunderSet,
    PropertyDunderDelete,
    StrStartswith,
    ConstraintSet,
    FunctionTypeDunderGet,
    MethodWrapper,
    BoundMethod,
    KnownBoundMethod,
    WrapperDescriptorFallback,
    DataclassDecoratorFallback,
    ObjectFallback,
    VersionInfo,
    PropertyGetter,
    PropertySetter,
    PropertyDeleter,
    BoolReal,
    Module,
    Undefined,
    TypeAlias,
    Restricted,
    EnumLiteral,
    TypeVar,
    PartialCall,
    Partial,
    Instance,
    ClassObject,
    ClassObjectTypeVarUpperBound,
    ClassObjectDynamicResult,
    BoundSuper,
}
#[cfg(test)]
impl GeneralMemberOperation {
    pub(in crate::types) fn name(self) -> &'static str {
        match self {
            Self::Cycle => "cycle",
            Self::ExplicitReceiver => "explicit_receiver",
            Self::RecursionGuard => "recursion_guard",
            Self::DunderClass => "dunder_class",
            Self::Lookup => "lookup",
            Self::ClassMemberDispatch => "class_member_dispatch",
            Self::MetaType => "meta_type",
            Self::NamespaceLookup => "namespace_lookup",
            Self::SubclassMro => "subclass_mro",
            Self::PropertyWrapper => "property_wrapper",
            Self::InstanceStorage => "instance_storage",
            Self::ImplicitAttributeInference => "implicit_attribute_inference",
            Self::InstanceApproximation => "instance_approximation",
            Self::MemberSelfBinding => "member_self_binding",
            Self::InferredAttributePromotion => "inferred_attribute_promotion",
            Self::GetattrFallback => "getattr_fallback",
            Self::FunctionLike => "function_like",
            Self::FunctionWrapper => "function_wrapper",
            Self::ConstraintSetClass => "constraint_set_class",
            Self::FunctionTypeClass => "function_type_class",
            Self::CallableFunctionOrStaticmethod => "callable_function_or_staticmethod",
            Self::CallableStaticOrClassmethod => "callable_static_or_classmethod",
            Self::ProtocolOrigin => "protocol_origin",
            Self::ProtocolMember => "protocol_member",
            Self::NewTypeUnion => "new_type_union",
            Self::EnumLiteralProperty => "enum_literal_property",
            Self::ParamSpec => "param_spec",
            Self::WrapperDescriptor => "wrapper_descriptor",
            Self::CallableRuntimeClass => "callable_runtime_class",
            Self::NominalEnumMember => "nominal_enum_member",
            Self::Bound => "bound",
            Self::Recursive => "recursive",
            Self::RecursiveVar => "recursive_var",
            Self::Union => "union",
            Self::Intersection => "intersection",
            Self::EnumComplement => "enum_complement",
            Self::FunctionDunderGet => "function_dunder_get",
            Self::DunderCall => "dunder_call",
            Self::UnderlyingFunction => "underlying_function",
            Self::PropertyDunderGet => "property_dunder_get",
            Self::PropertyDunderSet => "property_dunder_set",
            Self::PropertyDunderDelete => "property_dunder_delete",
            Self::StrStartswith => "str_startswith",
            Self::ConstraintSet => "constraint_set",
            Self::FunctionTypeDunderGet => "function_type_dunder_get",
            Self::MethodWrapper => "method_wrapper",
            Self::BoundMethod => "bound_method",
            Self::KnownBoundMethod => "known_bound_method",
            Self::WrapperDescriptorFallback => "wrapper_descriptor_fallback",
            Self::DataclassDecoratorFallback => "dataclass_decorator_fallback",
            Self::ObjectFallback => "object_fallback",
            Self::VersionInfo => "version_info",
            Self::PropertyGetter => "property_getter",
            Self::PropertySetter => "property_setter",
            Self::PropertyDeleter => "property_deleter",
            Self::BoolReal => "bool_real",
            Self::Module => "module",
            Self::Undefined => "undefined",
            Self::TypeAlias => "type_alias",
            Self::Restricted => "restricted",
            Self::EnumLiteral => "enum_literal",
            Self::TypeVar => "type_var",
            Self::PartialCall => "partial_call",
            Self::Partial => "partial",
            Self::Instance => "instance",
            Self::ClassObject => "class_object",
            Self::ClassObjectTypeVarUpperBound => "class_object_typevar_upper_bound",
            Self::ClassObjectDynamicResult => "class_object_dynamic_result",
            Self::BoundSuper => "bound_super",
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) enum GeneralMemberPredicate<'db> {
    FunctionLike(Type<'db>),
    FunctionWrapper(FunctionType<'db>),
    ConstraintSetClass(ClassLiteral<'db>),
    FunctionTypeClass(ClassLiteral<'db>),
    CallableFunctionOrStaticmethod(CallableType<'db>),
    CallableStaticOrClassmethod(CallableType<'db>),
    ProtocolOrigin(ProtocolInstanceType<'db>),
    ProtocolMember(ProtocolInstanceType<'db>),
    NewTypeUnion(Type<'db>),
    EnumLiteralProperty(EnumLiteralType<'db>),
    ParamSpec(BoundTypeVarInstance<'db>),
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl GeneralMemberPredicate<'_> {
    pub(in crate::types) fn operation(self) -> GeneralMemberOperation {
        match self {
            Self::FunctionLike(_) => GeneralMemberOperation::FunctionLike,
            Self::FunctionWrapper(_) => GeneralMemberOperation::FunctionWrapper,
            Self::ConstraintSetClass(_) => GeneralMemberOperation::ConstraintSetClass,
            Self::FunctionTypeClass(_) => GeneralMemberOperation::FunctionTypeClass,
            Self::CallableFunctionOrStaticmethod(_) => {
                GeneralMemberOperation::CallableFunctionOrStaticmethod
            }
            Self::CallableStaticOrClassmethod(_) => {
                GeneralMemberOperation::CallableStaticOrClassmethod
            }
            Self::ProtocolOrigin(_) => GeneralMemberOperation::ProtocolOrigin,
            Self::ProtocolMember(_) => GeneralMemberOperation::ProtocolMember,
            Self::NewTypeUnion(_) => GeneralMemberOperation::NewTypeUnion,
            Self::EnumLiteralProperty(_) => GeneralMemberOperation::EnumLiteralProperty,
            Self::ParamSpec(_) => GeneralMemberOperation::ParamSpec,
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) enum GeneralMemberBranch<'db> {
    Bound(Type<'db>),
    Recursive(RecursiveType<'db>),
    RecursiveVar,
    Union(UnionType<'db>),
    Intersection(IntersectionType<'db>),
    EnumComplement(EnumComplementType<'db>),
    FunctionDunderGet,
    DunderCall,
    UnderlyingFunction,
    PropertyDunderGet(PropertyInstanceType<'db>),
    PropertyDunderSet(PropertyInstanceType<'db>),
    PropertyDunderDelete(PropertyInstanceType<'db>),
    StrStartswith(StringLiteralType<'db>),
    ConstraintSet(KnownBoundMethodType<'db>),
    FunctionTypeDunderGet,
    MethodWrapper(MethodWrapper<'db>),
    BoundMethod(BoundMethodType<'db>),
    KnownBoundMethod(KnownBoundMethodType<'db>),
    WrapperDescriptorFallback,
    DataclassDecoratorFallback,
    CallableRuntimeClass(KnownClass),
    ObjectFallback,
    VersionInfo,
    PropertyGetter(PropertyInstanceType<'db>),
    PropertySetter(PropertyInstanceType<'db>),
    PropertyDeleter(PropertyInstanceType<'db>),
    BoolReal(bool),
    Module(ModuleLiteralType<'db>),
    Undefined,
    NewTypeUnion(NewType<'db>),
    TypeAlias(TypeAliasType<'db>),
    Restricted,
    EnumLiteral(EnumLiteralType<'db>),
    ParamSpec(BoundTypeVarInstance<'db>, ParamSpecAttrKind),
    TypeVar(BoundTypeVarInstance<'db>),
    NominalEnumMember(ClassLiteral<'db>, &'db EnumMetadata<'db>),
    PartialCall(FunctoolsPartialInstance<'db>),
    Partial(FunctoolsPartialInstance<'db>),
    Instance,
    ClassObject,
    BoundSuper(BoundSuperType<'db>),
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl GeneralMemberBranch<'_> {
    pub(in crate::types) fn operation(self) -> GeneralMemberOperation {
        match self {
            Self::Bound(_) => GeneralMemberOperation::Bound,
            Self::Recursive(_) => GeneralMemberOperation::Recursive,
            Self::RecursiveVar => GeneralMemberOperation::RecursiveVar,
            Self::Union(_) => GeneralMemberOperation::Union,
            Self::Intersection(_) => GeneralMemberOperation::Intersection,
            Self::EnumComplement(_) => GeneralMemberOperation::EnumComplement,
            Self::FunctionDunderGet => GeneralMemberOperation::FunctionDunderGet,
            Self::DunderCall => GeneralMemberOperation::DunderCall,
            Self::UnderlyingFunction => GeneralMemberOperation::UnderlyingFunction,
            Self::PropertyDunderGet(_) => GeneralMemberOperation::PropertyDunderGet,
            Self::PropertyDunderSet(_) => GeneralMemberOperation::PropertyDunderSet,
            Self::PropertyDunderDelete(_) => GeneralMemberOperation::PropertyDunderDelete,
            Self::StrStartswith(_) => GeneralMemberOperation::StrStartswith,
            Self::ConstraintSet(_) => GeneralMemberOperation::ConstraintSet,
            Self::FunctionTypeDunderGet => GeneralMemberOperation::FunctionTypeDunderGet,
            Self::MethodWrapper(_) => GeneralMemberOperation::MethodWrapper,
            Self::BoundMethod(_) => GeneralMemberOperation::BoundMethod,
            Self::KnownBoundMethod(_) => GeneralMemberOperation::KnownBoundMethod,
            Self::WrapperDescriptorFallback => GeneralMemberOperation::WrapperDescriptorFallback,
            Self::DataclassDecoratorFallback => GeneralMemberOperation::DataclassDecoratorFallback,
            Self::CallableRuntimeClass(_) => GeneralMemberOperation::CallableRuntimeClass,
            Self::ObjectFallback => GeneralMemberOperation::ObjectFallback,
            Self::VersionInfo => GeneralMemberOperation::VersionInfo,
            Self::PropertyGetter(_) => GeneralMemberOperation::PropertyGetter,
            Self::PropertySetter(_) => GeneralMemberOperation::PropertySetter,
            Self::PropertyDeleter(_) => GeneralMemberOperation::PropertyDeleter,
            Self::BoolReal(_) => GeneralMemberOperation::BoolReal,
            Self::Module(_) => GeneralMemberOperation::Module,
            Self::Undefined => GeneralMemberOperation::Undefined,
            Self::NewTypeUnion(_) => GeneralMemberOperation::NewTypeUnion,
            Self::TypeAlias(_) => GeneralMemberOperation::TypeAlias,
            Self::Restricted => GeneralMemberOperation::Restricted,
            Self::EnumLiteral(_) => GeneralMemberOperation::EnumLiteral,
            Self::ParamSpec(..) => GeneralMemberOperation::ParamSpec,
            Self::TypeVar(_) => GeneralMemberOperation::TypeVar,
            Self::NominalEnumMember(..) => GeneralMemberOperation::NominalEnumMember,
            Self::PartialCall(_) => GeneralMemberOperation::PartialCall,
            Self::Partial(_) => GeneralMemberOperation::Partial,
            Self::Instance => GeneralMemberOperation::Instance,
            Self::ClassObject => GeneralMemberOperation::ClassObject,
            Self::BoundSuper(_) => GeneralMemberOperation::BoundSuper,
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct GeneralMemberFacts;

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousGeneralMemberEffects)]
pub(in crate::types) trait GeneralMemberEffects<'db> {
    type Error;
    #[operation(checkpoint)]
    async fn checkpoint(&self, name: &str) -> Result<(), Self::Error>;
    #[operation(local)]
    async fn key_parts(&self, key: MemberLookupKey<'db>) -> Result<(Type<'db>, &'db Name, MemberLookupPolicy), Self::Error>;
    #[operation(child)]
    async fn predicate(&self, predicate: GeneralMemberPredicate<'db>, name: &str) -> Result<bool, Self::Error>;
    #[operation(child)]
    async fn wrapper_descriptor(&self, ty: Type<'db>, name: &str, policy: MemberLookupPolicy) -> Result<Option<Type<'db>>, Self::Error>;
    #[operation(child)]
    async fn callable_runtime_class(&self, callable: CallableType<'db>) -> Result<Option<KnownClass>, Self::Error>;
    #[operation(child)]
    async fn nominal_enum_member(&self, instance: NominalInstanceType<'db>, name: &str) -> Result<Option<(ClassLiteral<'db>, &'db EnumMetadata<'db>)>, Self::Error>;
    #[operation(child)]
    async fn execute(&self, branch: GeneralMemberBranch<'db>, key: MemberLookupKey<'db>, receiver: Option<Type<'db>>) -> Result<MemberLookupResult<'db>, Self::Error>;
    #[operation(child)]
    async fn lookup(&self, ty: Type<'db>, name: GeneralMemberName<'_>, policy: MemberLookupPolicy, receiver: Option<Type<'db>>) -> Result<MemberLookupResult<'db>, Self::Error>;
    #[operation(child)]
    async fn fallback(&self, ty: Type<'db>, name: GeneralMemberName<'_>, policy: MemberLookupPolicy, receiver: Option<Type<'db>>) -> Result<MemberLookupResult<'db>, Self::Error>;
    #[operation(child)]
    async fn dunder_class(&self, ty: Type<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
    #[operation(local)]
    async fn bound(&self, ty: Type<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
}

#[finite_capability]
impl GeneralMemberFacts {
    fn name_is(&self, name: &str, expected: &'static str) -> bool { name == expected }
    fn function_alias_name(&self, name: &str) -> bool { matches!(name, "__func__" | "__wrapped__") }
    fn descriptor_name(&self, name: &str) -> bool { matches!(name, "__get__" | "__set__" | "__delete__") }
    fn version_segment_name(&self, name: &str) -> bool { matches!(name, "major" | "minor") }
    fn real_or_numerator_name(&self, name: &str) -> bool { matches!(name, "real" | "numerator") }
    fn enum_value_name(&self, name: &str) -> bool { matches!(name, "name" | "_name_" | "value" | "_value_") }
    fn materialized_fallback<'db>(&self, ty: Type<'db>) -> Option<Type<'db>> { ty.materialized_divergent_fallback() }
    fn has_materialized_fallback(&self, ty: Type<'_>) -> bool { ty.materialized_divergent_fallback().is_some() }
    fn name_str<'a>(&self, name: &'a Name) -> &'a str { name.as_str() }
    fn input_name_str<'a>(&self, name: GeneralMemberName<'a>) -> &'a str { name.as_str() }
    fn retained_name<'a>(&self, name: &'a Name) -> GeneralMemberName<'a> { GeneralMemberName::Shared(name) }
    fn string_literal<'db>(&self, literal: LiteralValueType<'db>) -> Option<StringLiteralType<'db>> { literal.as_string() }
    fn is_int(&self, literal: LiteralValueType<'_>) -> bool { literal.is_int() }
    fn bool_literal(&self, literal: LiteralValueType<'_>) -> Option<bool> { literal.as_bool() }
    fn enum_literal<'db>(&self, literal: LiteralValueType<'db>) -> Option<EnumLiteralType<'db>> { literal.as_enum() }
    fn is_sys_version_info(&self, instance: NominalInstanceType<'_>) -> bool { instance.is_sys_version_info() }
    fn mro_no_object_fallback(&self, policy: MemberLookupPolicy) -> bool { policy.mro_no_object_fallback() }
    fn no_instance_fallback(&self, policy: MemberLookupPolicy) -> bool { policy.no_instance_fallback() }
    fn paramspec_attr(&self, name: &str) -> Option<ParamSpecAttrKind> { ParamSpecAttrKind::from_name(name) }
}

#[synchronous(member_lookup_entry_sync)]
#[capabilities(effects = GeneralMemberEffects, facts = GeneralMemberFacts)]
#[passive_values()]
pub(in crate::types) async fn member_lookup_entry_with<'db, E: GeneralMemberEffects<'db>>(
    ty: Type<'db>, name: GeneralMemberName<'_>, policy: MemberLookupPolicy, receiver: Option<Type<'db>>,
    facts: GeneralMemberFacts, effects: &E,
) -> Result<MemberLookupResult<'db>, E::Error> {
    let name_str = facts.input_name_str(name);
    effects.checkpoint(name_str).await?;
    if !facts.has_materialized_fallback(ty) {
        if facts.name_is(name_str, "__class__") {
            return effects.dunder_class(ty).await;
        }
        if matches!(ty, Type::Dynamic(_) | Type::Divergent(_) | Type::Never) {
            return effects.bound(ty).await;
        }
    }
    effects.lookup(ty, name, policy, receiver).await
}

#[synchronous(member_lookup_dispatch_sync)]
#[capabilities(effects = GeneralMemberEffects, facts = GeneralMemberFacts)]
#[passive_values(GeneralMemberBranch::Recursive, GeneralMemberBranch::RecursiveVar, GeneralMemberBranch::Union, GeneralMemberBranch::Intersection, GeneralMemberBranch::EnumComplement, GeneralMemberBranch::Bound, GeneralMemberBranch::FunctionDunderGet, GeneralMemberBranch::DunderCall, GeneralMemberBranch::UnderlyingFunction, GeneralMemberBranch::PropertyDunderGet, GeneralMemberBranch::PropertyDunderSet, GeneralMemberBranch::PropertyDunderDelete, GeneralMemberBranch::StrStartswith, GeneralMemberBranch::ConstraintSet, KnownBoundMethodType::ConstraintSetLowerBound, KnownBoundMethodType::ConstraintSetUpperBound, KnownBoundMethodType::ConstraintSetEquality, KnownBoundMethodType::ConstraintSetRange, KnownBoundMethodType::ConstraintSetAlways, KnownBoundMethodType::ConstraintSetNever, KnownBoundMethodType::ConstraintSetImpliesSubtypeOf, KnownBoundMethodType::ConstraintSetSatisfies, KnownBoundMethodType::ConstraintSetExists, KnownBoundMethodType::ConstraintSetForAll, KnownBoundMethodType::ConstraintSetSolutionsFor, KnownBoundMethodType::ConstraintSetSolutions, KnownBoundMethodType::ConstraintSetWithDetailedDisplay, GeneralMemberBranch::FunctionTypeDunderGet, GeneralMemberBranch::MethodWrapper, GeneralMemberBranch::BoundMethod, GeneralMemberBranch::KnownBoundMethod, GeneralMemberBranch::WrapperDescriptorFallback, GeneralMemberBranch::DataclassDecoratorFallback, GeneralMemberBranch::CallableRuntimeClass, GeneralMemberBranch::ObjectFallback, GeneralMemberBranch::VersionInfo, GeneralMemberBranch::PropertyGetter, GeneralMemberBranch::PropertySetter, GeneralMemberBranch::PropertyDeleter, GeneralMemberBranch::BoolReal, GeneralMemberBranch::Module, GeneralMemberBranch::Undefined, GeneralMemberBranch::NewTypeUnion, GeneralMemberBranch::TypeAlias, GeneralMemberBranch::Restricted, GeneralMemberBranch::EnumLiteral, GeneralMemberBranch::ParamSpec, GeneralMemberBranch::TypeVar, GeneralMemberBranch::NominalEnumMember, GeneralMemberBranch::PartialCall, GeneralMemberBranch::Partial, GeneralMemberBranch::Instance, GeneralMemberBranch::ClassObject, GeneralMemberBranch::BoundSuper, GeneralMemberPredicate::FunctionLike, GeneralMemberPredicate::FunctionWrapper, GeneralMemberPredicate::ConstraintSetClass, GeneralMemberPredicate::FunctionTypeClass, GeneralMemberPredicate::CallableFunctionOrStaticmethod, GeneralMemberPredicate::CallableStaticOrClassmethod, GeneralMemberPredicate::ProtocolOrigin, GeneralMemberPredicate::ProtocolMember, GeneralMemberPredicate::NewTypeUnion, GeneralMemberPredicate::EnumLiteralProperty, GeneralMemberPredicate::ParamSpec)]
pub(in crate::types) async fn member_lookup_dispatch_with<'db, E: GeneralMemberEffects<'db>>(
    key: MemberLookupKey<'db>, receiver: Option<Type<'db>>, facts: GeneralMemberFacts, effects: &E,
) -> Result<MemberLookupResult<'db>, E::Error> {
    let (this, name, policy) = effects.key_parts(key).await?;
    let name_str = facts.name_str(name);
    effects.checkpoint(name_str).await?;
    if let Some(fallback) = facts.materialized_fallback(this) {
        return effects.fallback(fallback, facts.retained_name(name), policy, receiver).await;
    }
    let branch = match this {
        Type::Recursive(recursive) => GeneralMemberBranch::Recursive(recursive),

        Type::RecursiveVar(_) => GeneralMemberBranch::RecursiveVar,

        Type::Union(union) => GeneralMemberBranch::Union(union),

        Type::Intersection(intersection) => GeneralMemberBranch::Intersection(intersection),

        Type::EnumComplement(complement) => GeneralMemberBranch::EnumComplement(complement),

        Type::Dynamic(..) | Type::Divergent(_) | Type::Never => GeneralMemberBranch::Bound(this),

        _ if facts.name_is(name_str, "__get__") && effects.predicate(GeneralMemberPredicate::FunctionLike(this), name_str).await? => GeneralMemberBranch::FunctionDunderGet,

        Type::FunctionLiteral(_) if facts.name_is(name_str, "__call__") => GeneralMemberBranch::DunderCall,

        Type::FunctionLiteral(function)
            if facts.function_alias_name(name_str)
                && effects.predicate(GeneralMemberPredicate::FunctionWrapper(function), name_str).await? => GeneralMemberBranch::UnderlyingFunction,

        Type::PropertyInstance(property) if facts.name_is(name_str, "__get__") => GeneralMemberBranch::PropertyDunderGet(property),

        Type::PropertyInstance(property) if facts.name_is(name_str, "__set__") => GeneralMemberBranch::PropertyDunderSet(property),

        Type::PropertyInstance(property) if facts.name_is(name_str, "__delete__") => GeneralMemberBranch::PropertyDunderDelete(property),

        Type::LiteralValue(literal)
            if facts.name_is(name_str, "startswith")
                && let Some(string_literal) = facts.string_literal(literal) => GeneralMemberBranch::StrStartswith(string_literal),

        Type::ClassLiteral(class)
            if facts.name_is(name_str, "lower_bound") && effects.predicate(GeneralMemberPredicate::ConstraintSetClass(class), name_str).await? => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetLowerBound),

        Type::ClassLiteral(class)
            if facts.name_is(name_str, "upper_bound") && effects.predicate(GeneralMemberPredicate::ConstraintSetClass(class), name_str).await? => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetUpperBound),

        Type::ClassLiteral(class)
            if facts.name_is(name_str, "equality") && effects.predicate(GeneralMemberPredicate::ConstraintSetClass(class), name_str).await? => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetEquality),

        Type::ClassLiteral(class)
            if facts.name_is(name_str, "range") && effects.predicate(GeneralMemberPredicate::ConstraintSetClass(class), name_str).await? => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetRange),

        Type::ClassLiteral(class)
            if facts.name_is(name_str, "always") && effects.predicate(GeneralMemberPredicate::ConstraintSetClass(class), name_str).await? => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetAlways),

        Type::ClassLiteral(class)
            if facts.name_is(name_str, "never") && effects.predicate(GeneralMemberPredicate::ConstraintSetClass(class), name_str).await? => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetNever),

        Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked))
            if facts.name_is(name_str, "implies_subtype_of") => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(tracked)),

        Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked)) if facts.name_is(name_str, "satisfies") => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetSatisfies(tracked)),

        Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked)) if facts.name_is(name_str, "exists") => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetExists(tracked)),

        Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked)) if facts.name_is(name_str, "for_all") => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetForAll(tracked)),

        Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked))
            if facts.name_is(name_str, "solutions_for") => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetSolutionsFor(tracked)),

        Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked)) if facts.name_is(name_str, "solutions") => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetSolutions(tracked)),

        Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked))
            if facts.name_is(name_str, "with_detailed_display") => GeneralMemberBranch::ConstraintSet(KnownBoundMethodType::ConstraintSetWithDetailedDisplay(tracked)),

        Type::ClassLiteral(class)
            if facts.name_is(name_str, "__get__") && effects.predicate(GeneralMemberPredicate::FunctionTypeClass(class), name_str).await? => GeneralMemberBranch::FunctionTypeDunderGet,

        Type::ClassLiteral(_) | Type::GenericAlias(_)
            if facts.descriptor_name(name_str)
                && let Some(wrapper @ Type::WrapperDescriptor(_)) = effects.wrapper_descriptor(this, name_str, policy).await? => GeneralMemberBranch::Bound(wrapper),

        Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)) => GeneralMemberBranch::MethodWrapper(wrapper),

        Type::BoundMethod(bound_method) => GeneralMemberBranch::BoundMethod(bound_method),

        Type::KnownBoundMethod(method) => GeneralMemberBranch::KnownBoundMethod(method),

        Type::WrapperDescriptor(_) => GeneralMemberBranch::WrapperDescriptorFallback,

        Type::DataclassDecorator(_) => GeneralMemberBranch::DataclassDecoratorFallback,

        Type::Callable(callable)
            if facts.name_is(name_str, "__call__")
                && effects.predicate(GeneralMemberPredicate::CallableFunctionOrStaticmethod(callable), name_str).await? => GeneralMemberBranch::DunderCall,

        Type::Callable(_) | Type::DataclassTransformer(_) if facts.name_is(name_str, "__call__") => GeneralMemberBranch::Bound(this),

        Type::Callable(callable)
            if facts.function_alias_name(name_str)
                && effects.predicate(GeneralMemberPredicate::CallableStaticOrClassmethod(callable), name_str).await? => GeneralMemberBranch::UnderlyingFunction,

        Type::Callable(callable) if let Some(class) = effects.callable_runtime_class(callable).await? => GeneralMemberBranch::CallableRuntimeClass(class),

        Type::Callable(_) | Type::DataclassTransformer(_) => GeneralMemberBranch::ObjectFallback,

        Type::NominalInstance(instance)
            if facts.version_segment_name(name_str) && facts.is_sys_version_info(instance) => GeneralMemberBranch::VersionInfo,

        Type::PropertyInstance(property) if facts.name_is(name_str, "fget") => GeneralMemberBranch::PropertyGetter(property),

        Type::PropertyInstance(property) if facts.name_is(name_str, "fset") => GeneralMemberBranch::PropertySetter(property),

        Type::PropertyInstance(property) if facts.name_is(name_str, "fdel") => GeneralMemberBranch::PropertyDeleter(property),

        Type::LiteralValue(literal)
            if facts.is_int(literal) && facts.real_or_numerator_name(name_str) => GeneralMemberBranch::Bound(this),

        Type::LiteralValue(literal)
            if facts.real_or_numerator_name(name_str)
                && let Some(bool_value) = facts.bool_literal(literal) => GeneralMemberBranch::BoolReal(bool_value),

        Type::ModuleLiteral(module) => GeneralMemberBranch::Module(module),

        // If a protocol does not include a member and the policy disables falling back to
        // `object`, we return `Place::Undefined` here. This short-circuits attribute lookup
        // before we find the "fallback to attribute access on `object`" logic later on
        // (otherwise we would infer that all synthesized protocols have `__getattribute__`
        // methods, and therefore that all synthesized protocols have all possible attributes.)
        //
        // Note that we could do this for *all* protocols, but it's only *necessary* for synthesized
        // ones, and the standard logic is *probably* more performant for class-based protocols?
        Type::ProtocolInstance(protocol)
            if effects.predicate(GeneralMemberPredicate::ProtocolOrigin(protocol), name_str).await?
                && facts.mro_no_object_fallback(policy)
                && !effects.predicate(GeneralMemberPredicate::ProtocolMember(protocol), name_str).await? => GeneralMemberBranch::Undefined,

        // This case needs to come before the `no_instance_fallback` catch-all, so that we
        // treat `NewType`s of `float` and `complex` as their special-case union base types.
        // Otherwise we'll look up e.g. `__add__` with a `self` type bound to the `NewType`,
        // which will fail to match e.g. `float.__add__` (because its `self` parameter is just
        // `float` and not `int | float`). However, all other `NewType` cases need to fall
        // through, because we generally do want e.g. methods that return `Self` to return the
        // `NewType`.
        Type::NewTypeInstance(new_type_instance) if effects.predicate(GeneralMemberPredicate::NewTypeUnion(this), name_str).await? => GeneralMemberBranch::NewTypeUnion(new_type_instance),

        Type::TypeAlias(alias) => GeneralMemberBranch::TypeAlias(alias),

        _ if facts.no_instance_fallback(policy) => GeneralMemberBranch::Restricted,

        Type::LiteralValue(literal)
            if facts.enum_value_name(name_str)
                && let Some(enum_literal) = facts.enum_literal(literal)
                && !effects.predicate(GeneralMemberPredicate::EnumLiteralProperty(enum_literal), name_str).await? => GeneralMemberBranch::EnumLiteral(enum_literal),

        Type::TypeVar(typevar)
            if effects.predicate(GeneralMemberPredicate::ParamSpec(typevar), name_str).await?
                && let Some(attr) = facts.paramspec_attr(name_str) => GeneralMemberBranch::ParamSpec(typevar, attr),

        Type::TypeVar(typevar) => GeneralMemberBranch::TypeVar(typevar),

        Type::NominalInstance(instance)
            if facts.enum_value_name(name_str)
                && let Some((class_literal, metadata)) = effects.nominal_enum_member(instance, name_str).await? => GeneralMemberBranch::NominalEnumMember(class_literal, metadata),

        Type::KnownInstance(KnownInstanceType::FunctoolsPartial(partial))
            if facts.name_is(name_str, "__call__") => GeneralMemberBranch::PartialCall(partial),

        Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(_))
            if facts.name_is(name_str, "__call__") => GeneralMemberBranch::Bound(this),

        Type::KnownInstance(KnownInstanceType::FunctoolsPartial(partial)) => GeneralMemberBranch::Partial(partial),

        Type::NominalInstance(..)
        | Type::ProtocolInstance(..)
        | Type::NewTypeInstance(..)
        | Type::LiteralValue(..)
        | Type::SpecialForm(..)
        | Type::KnownInstance(..)
        | Type::PropertyInstance(..)
        | Type::SlotDescriptor(..)
        | Type::FunctionLiteral(..)
        | Type::AlwaysTruthy
        | Type::AlwaysFalsy
        | Type::TypeIs(..)
        | Type::TypeGuard(..)
        | Type::TypeForm(..)
        | Type::TypedDict(_) => GeneralMemberBranch::Instance,

        Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..) => GeneralMemberBranch::ClassObject,

        // Unlike other objects, `super` has a unique member lookup behavior.
        // It's simpler than other objects:
        //
        // 1. Search for the attribute in the MRO, starting just after the pivot class.
        // 2. If the attribute is a descriptor, invoke its `__get__` method.
        Type::BoundSuper(bound_super) => GeneralMemberBranch::BoundSuper(bound_super),
    };
    effects.execute(branch, key, receiver).await
}
}

pub(in crate::types) type ReceiverMemberLookup<'db> =
    fn(&'db dyn Db, MemberLookupKey<'db>, Type<'db>) -> MemberLookupResult<'db>;

pub(in crate::types) struct InlineGeneralMemberEffects<'env, 'guard, 'db> {
    pub(in crate::types) db: &'db dyn Db,
    pub(in crate::types) env: &'env ProgramEnvironment<'db>,
    pub(in crate::types) recursion_guard: Option<&'guard CallableRecursionGuard<'db>>,
    // The entry supplies the existing receiver query without moving its Salsa identity.
    // Dispatch effects use the ordinary entry again if a receiver query is needed.
    pub(in crate::types) receiver_lookup: Option<ReceiverMemberLookup<'db>>,
}

impl<'db> SynchronousGeneralMemberEffects<'db> for InlineGeneralMemberEffects<'_, '_, 'db> {
    type Error = Infallible;

    fn checkpoint(&self, _name: &str) -> Result<(), Self::Error> {
        Ok(())
    }
    fn key_parts(
        &self,
        key: MemberLookupKey<'db>,
    ) -> Result<(Type<'db>, &'db Name, MemberLookupPolicy), Self::Error> {
        Ok((key.ty(self.db), key.name(self.db), key.policy(self.db)))
    }
    fn predicate(
        &self,
        predicate: GeneralMemberPredicate<'db>,
        name: &str,
    ) -> Result<bool, Self::Error> {
        let db = self.db;
        let env = self.env;
        Ok(match predicate {
            GeneralMemberPredicate::FunctionLike(ty) => ty.function_like_kind(db).is_some(),
            GeneralMemberPredicate::FunctionWrapper(function) => {
                function.is_staticmethod(db) || function.is_classmethod(db)
            }
            GeneralMemberPredicate::ConstraintSetClass(class) => {
                class.is_known(db, KnownClass::ConstraintSet)
            }
            GeneralMemberPredicate::FunctionTypeClass(class) => {
                class.is_known(db, KnownClass::FunctionType)
            }
            GeneralMemberPredicate::CallableFunctionOrStaticmethod(callable) => {
                callable.is_function_like(db) || callable.is_staticmethod_like(db)
            }
            GeneralMemberPredicate::CallableStaticOrClassmethod(callable) => {
                callable.is_staticmethod_like(db) || callable.is_classmethod_like(db)
            }
            GeneralMemberPredicate::ProtocolOrigin(protocol) => protocol.class_origin(db).is_none(),
            GeneralMemberPredicate::ProtocolMember(protocol) => {
                protocol.interface(db).includes_member(db, name)
            }
            GeneralMemberPredicate::NewTypeUnion(ty) => ty.as_union_like(db).is_some(),
            GeneralMemberPredicate::EnumLiteralProperty(literal) => {
                enums::class_defines_property(db, env, literal.enum_class(db), name)
            }
            GeneralMemberPredicate::ParamSpec(typevar) => typevar.is_paramspec(db),
        })
    }
    fn wrapper_descriptor(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(ty
            .find_name_in_mro_with_policy(self.db, self.env, name, policy)
            .and_then(|member| member.place.ignore_possibly_undefined()))
    }
    fn callable_runtime_class(
        &self,
        callable: CallableType<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(callable.runtime_class(self.db))
    }
    fn nominal_enum_member(
        &self,
        instance: NominalInstanceType<'db>,
        name: &str,
    ) -> Result<Option<(ClassLiteral<'db>, &'db EnumMetadata<'db>)>, Self::Error> {
        let class = instance.class_literal(self.db, self.env);
        Ok(
            if let Some(metadata) = enum_metadata(self.db, class)
                && !enums::class_defines_property(self.db, self.env, class, name)
            {
                Some((class, metadata))
            } else {
                None
            },
        )
    }
    fn dunder_class(&self, ty: Type<'db>) -> Result<MemberLookupResult<'db>, Self::Error> {
        Ok(Place::bound(ty.dunder_class(self.db, self.env)).into())
    }
    fn bound(&self, ty: Type<'db>) -> Result<MemberLookupResult<'db>, Self::Error> {
        Ok(Place::bound(ty).into())
    }
    fn lookup(
        &self,
        ty: Type<'db>,
        name: GeneralMemberName<'_>,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        let db = self.db;
        let env = self.env;
        let retained_name = match name {
            GeneralMemberName::Text(name) => Name::new(name),
            GeneralMemberName::Shared(name) => name.clone(),
        };
        let key = MemberLookupKey::new(db, env.program(db), ty, retained_name, policy);
        Ok(if self.recursion_guard.is_some() {
            member_lookup_with_policy_impl(db, key, receiver, self.recursion_guard)
        } else {
            match receiver {
                Some(receiver) if let Some(query) = self.receiver_lookup => {
                    query(db, key, receiver)
                }
                Some(receiver) => ty.member_lookup_with_policy_and_receiver(
                    db,
                    env,
                    name.as_str(),
                    policy,
                    Some(receiver),
                ),
                None => member_lookup_with_policy_inner(db, key),
            }
        })
    }
    fn fallback(
        &self,
        ty: Type<'db>,
        name: GeneralMemberName<'_>,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        Ok(ty.member_lookup_with_recursion_guard(
            self.db,
            self.env,
            name.as_str(),
            policy,
            receiver,
            self.recursion_guard,
        ))
    }
    fn execute(
        &self,
        branch: GeneralMemberBranch<'db>,
        key: MemberLookupKey<'db>,
        receiver: Option<Type<'db>>,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        let db = self.db;
        let env = self.env;
        let recursion_guard = self.recursion_guard;
        let this = key.ty(db);
        let name = key.name(db);
        let name_str = name.as_str();
        let policy = key.policy(db);
        Ok(match branch {
            GeneralMemberBranch::Recursive(recursive) => recursive
                .unfold(db, env)
                .map(|unfolded| {
                    unfolded.member_lookup_with_recursion_guard(
                        db,
                        env,
                        name_str,
                        policy,
                        receiver,
                        recursion_guard,
                    )
                })
                .unwrap_or(Place::bound(Type::unknown()).into()),

            GeneralMemberBranch::RecursiveVar => {
                unreachable!("semantic operation on an unbound recursive variable")
            }

            GeneralMemberBranch::Union(union) => {
                let mut error = None;
                let mut properties = None;
                let mut descriptor = DescriptorOrigin::default();
                let member = union.map_with_boundness_and_qualifiers(db, env, |elem| {
                    let result = elem.member_lookup_with_recursion_guard(
                        db,
                        env,
                        name_str,
                        policy,
                        receiver,
                        recursion_guard,
                    );
                    error = error.or_else(|| result.err().map(|error| error.kind(db)));
                    let member = result.unwrap_or_else(|error| error.fallback_member(db));
                    properties = union_deprecated_properties(
                        db,
                        properties,
                        member.deprecated_properties(db),
                    );
                    let origin = member.descriptor_origin(db);
                    descriptor = descriptor.merge(db, origin);
                    member.member(db)
                });
                member_lookup_result_with_origin(db, member, error, properties, descriptor)
            }

            GeneralMemberBranch::Intersection(intersection) => {
                if let Some(complement) = intersection.enum_complement(db, env) {
                    enums::member_lookup_for_enum_complement(db, env, complement, name_str, policy)
                        .into()
                } else {
                    let receiver = Some(receiver.unwrap_or(this));
                    let mut error = None;
                    let mut properties: Option<PropertyDeprecations<'db>> = None;
                    let mut descriptor = DescriptorOrigin::default();
                    let mut all_deprecated = true;
                    let member = intersection.map_with_boundness_and_qualifiers(db, env, |elem| {
                        let result = elem.member_lookup_with_recursion_guard(
                            db,
                            env,
                            name_str,
                            policy,
                            receiver,
                            recursion_guard,
                        );
                        error = error.or_else(|| result.err().map(|error| error.kind(db)));
                        let member = result.unwrap_or_else(|error| error.fallback_member(db));
                        let origin = member.descriptor_origin(db);
                        descriptor = descriptor.merge(db, origin);
                        if let Some(deprecated) = member.deprecated_properties(db) {
                            properties = Some(properties.map_or(deprecated, |properties| {
                                properties.intersection(db, deprecated)
                            }));
                        } else if !member.member(db).place.is_undefined() {
                            all_deprecated = false;
                        }
                        member.member(db)
                    });
                    member_lookup_result_with_origin(
                        db,
                        member,
                        error,
                        properties.filter(|_| all_deprecated && !member.place.is_undefined()),
                        descriptor,
                    )
                }
            }

            GeneralMemberBranch::EnumComplement(complement) => {
                enums::member_lookup_for_enum_complement(db, env, complement, name_str, policy)
                    .into()
            }

            GeneralMemberBranch::Bound(ty) => Place::bound(ty).into(),

            GeneralMemberBranch::FunctionDunderGet => Place::bound(Type::KnownBoundMethod(
                KnownBoundMethodType::FunctionTypeDunderGet(InternedType::new(db, this)),
            ))
            .into(),

            GeneralMemberBranch::DunderCall => Place::bound(Type::KnownBoundMethod(
                KnownBoundMethodType::DunderCall(InternedType::new(db, this)),
            ))
            .into(),

            GeneralMemberBranch::UnderlyingFunction => {
                Place::bound(this.underlying_function(db)).into()
            }

            GeneralMemberBranch::PropertyDunderGet(property) => Place::bound(
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(property)),
            )
            .into(),

            GeneralMemberBranch::PropertyDunderSet(property) => Place::bound(
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderSet(property)),
            )
            .into(),

            GeneralMemberBranch::PropertyDunderDelete(property) => Place::bound(
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderDelete(property)),
            )
            .into(),

            GeneralMemberBranch::StrStartswith(string_literal) => Place::bound(
                Type::KnownBoundMethod(KnownBoundMethodType::StrStartswith(string_literal)),
            )
            .into(),

            GeneralMemberBranch::ConstraintSet(method) => {
                Place::bound(Type::KnownBoundMethod(method)).into()
            }

            GeneralMemberBranch::FunctionTypeDunderGet => Place::bound(Type::WrapperDescriptor(
                WrapperDescriptorKind::FunctionTypeDunderGet,
            ))
            .into(),

            GeneralMemberBranch::MethodWrapper(wrapper) => match name_str {
                "__func__" | "__wrapped__" => Place::bound(wrapper.wrapped(db)).into(),
                "__call__" if wrapper.class(db) == KnownClass::Staticmethod => {
                    Place::bound(Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(
                        InternedType::new(db, this),
                    )))
                    .into()
                }
                _ => wrapper
                    .instance_fallback(db, env)
                    .member_lookup_with_recursion_guard(
                        db,
                        env,
                        name_str,
                        policy,
                        receiver,
                        recursion_guard,
                    ),
            },

            GeneralMemberBranch::BoundMethod(bound_method) => match name_str {
                "__call__" => Place::bound(Type::KnownBoundMethod(
                    KnownBoundMethodType::DunderCall(InternedType::new(db, this)),
                ))
                .into(),
                "__get__" if env.python_version(db) >= ast::PythonVersion::PY313 => Place::bound(
                    Type::KnownBoundMethod(KnownBoundMethodType::MethodTypeDunderGet(bound_method)),
                )
                .into(),
                "__self__" => Place::bound(bound_method.self_instance(db)).into(),
                "__func__" => Place::bound(bound_method.func(db)).into(),
                _ => {
                    let result = KnownClass::MethodType
                        .to_instance(db, env)
                        .member_lookup_with_recursion_guard(
                            db,
                            env,
                            name_str,
                            policy,
                            receiver,
                            recursion_guard,
                        );
                    member_lookup_or_fall_back_to(db, env, result, || {
                        // If an attribute is not available on the bound method object,
                        // it will be looked up on the underlying function object. This
                        // changes the lookup object, so do not forward the bound-method
                        // receiver.
                        bound_method.func(db).member_lookup_with_recursion_guard(
                            db,
                            env,
                            name_str,
                            policy,
                            None,
                            recursion_guard,
                        )
                    })
                }
            },

            GeneralMemberBranch::KnownBoundMethod(method) => method
                .class()
                .to_instance(db, env)
                .member_lookup_with_recursion_guard(
                    db,
                    env,
                    name_str,
                    policy,
                    receiver,
                    recursion_guard,
                ),

            GeneralMemberBranch::WrapperDescriptorFallback => KnownClass::WrapperDescriptorType
                .to_instance(db, env)
                .member_lookup_with_recursion_guard(
                    db,
                    env,
                    name_str,
                    policy,
                    receiver,
                    recursion_guard,
                ),

            GeneralMemberBranch::DataclassDecoratorFallback => KnownClass::FunctionType
                .to_instance(db, env)
                .member_lookup_with_recursion_guard(
                    db,
                    env,
                    name_str,
                    policy,
                    receiver,
                    recursion_guard,
                ),

            GeneralMemberBranch::CallableRuntimeClass(class) => class
                .to_instance(db, env)
                .member_lookup_with_recursion_guard(
                    db,
                    env,
                    name_str,
                    policy,
                    receiver,
                    recursion_guard,
                ),

            GeneralMemberBranch::ObjectFallback => Type::object()
                .member_lookup_with_recursion_guard(
                    db,
                    env,
                    name_str,
                    policy,
                    receiver,
                    recursion_guard,
                ),

            GeneralMemberBranch::VersionInfo => {
                let python_version = env.python_version(db);
                let segment = if name == "major" {
                    python_version.major
                } else {
                    python_version.minor
                };
                Place::bound(Type::int_literal(segment.into())).into()
            }

            GeneralMemberBranch::PropertyGetter(property) => {
                Place::bound(property.getter(db).unwrap_or(Type::none(db, env))).into()
            }

            GeneralMemberBranch::PropertySetter(property) => {
                Place::bound(property.setter(db).unwrap_or(Type::none(db, env))).into()
            }

            GeneralMemberBranch::PropertyDeleter(property) => {
                Place::bound(property.deleter(db).unwrap_or(Type::none(db, env))).into()
            }

            GeneralMemberBranch::BoolReal(bool_value) => {
                Place::bound(Type::int_literal(i64::from(bool_value))).into()
            }

            GeneralMemberBranch::Module(module) => module.static_member(db, env, name_str),

            GeneralMemberBranch::Undefined => Place::Undefined.into(),

            GeneralMemberBranch::NewTypeUnion(new_type_instance) => new_type_instance
                .concrete_base_type(db)
                .member_lookup_with_recursion_guard(
                    db,
                    env,
                    name_str,
                    policy,
                    None,
                    recursion_guard,
                ),

            GeneralMemberBranch::TypeAlias(alias) => {
                alias.value_type(db).member_lookup_with_recursion_guard(
                    db,
                    env,
                    name_str,
                    policy,
                    receiver,
                    recursion_guard,
                )
            }

            GeneralMemberBranch::Restricted => match restricted_member_entry_sync(
                key,
                receiver.unwrap_or(this),
                LookupFacts,
                &InlineMemberEntry {
                    db,
                    env,
                    recursion_guard,
                },
            ) {
                Ok(result) => result,
                Err(never) => match never {},
            },

            GeneralMemberBranch::EnumLiteral(enum_literal) => {
                let enum_class = enum_literal.enum_class_literal(db);
                let is_enum_subclass = Type::ClassLiteral(enum_class.class_literal(db))
                    .is_subtype_of(db, env, KnownClass::Enum.to_subclass_of(db, env));

                let ty = match name_str {
                    "name" if is_enum_subclass => enum_class.name_type(db, enum_literal.name(db)),
                    "_name_" => enum_class.name_type(db, enum_literal.name(db)),
                    "value" if is_enum_subclass => enum_class.value_type(db, enum_literal.name(db)),
                    "_value_" => enum_class.value_type(db, enum_literal.name(db)),
                    _ => None,
                };

                ty.map(Place::bound).unwrap_or_default().into()
            }

            GeneralMemberBranch::ParamSpec(typevar, attr) => {
                Place::declared(Type::TypeVar(typevar.with_paramspec_attr(db, attr))).into()
            }

            GeneralMemberBranch::TypeVar(typevar) => {
                let receiver = receiver.unwrap_or(this);
                if let Some(bound_or_constraints) =
                    typevar.typevar(db).bound_or_constraints(db, env)
                {
                    distribute_member_lookup_over_bound_or_constraints(
                        db,
                        env,
                        bound_or_constraints,
                        receiver,
                        name_str,
                        policy,
                    )
                } else {
                    instance_like_member_lookup(db, env, key, receiver, recursion_guard)
                }
            }

            GeneralMemberBranch::NominalEnumMember(class_literal, metadata) => {
                let is_enum_subclass = Type::ClassLiteral(class_literal).is_subtype_of(
                    db,
                    env,
                    KnownClass::Enum.to_subclass_of(db, env),
                );

                let ty = match name_str {
                    "name" if is_enum_subclass => metadata.instance_name_type(db, env),
                    "_name_" => metadata.instance_name_type(db, env),
                    "value" if is_enum_subclass => metadata.instance_value_type(db, env),
                    "_value_" => metadata.instance_value_type(db, env),
                    _ => None,
                };

                ty.map(Place::bound).unwrap_or_default().into()
            }

            GeneralMemberBranch::PartialCall(partial) => Place::bound(Type::KnownInstance(
                KnownInstanceType::FunctoolsPartialCall(partial),
            ))
            .into(),

            GeneralMemberBranch::Partial(partial) => {
                let wrapped = partial.wrapped(db).inner(db);
                let nominal_lookup = partial
                    .partial(db)
                    .into_functools_partial_instance(db, env)
                    .member_lookup_with_recursion_guard(
                        db,
                        env,
                        name_str,
                        policy,
                        receiver,
                        recursion_guard,
                    );
                if name_str == "func" {
                    match nominal_lookup
                        .unwrap_or_else(|error| error.fallback_member(db))
                        .member(db)
                        .place
                    {
                        Place::Defined(DefinedPlace {
                            origin,
                            definedness,
                            public_type_policy,
                            provenance,
                            ..
                        }) => Place::Defined(DefinedPlace {
                            ty: wrapped,
                            origin,
                            definedness,
                            public_type_policy,
                            provenance,
                        })
                        .into(),
                        Place::Undefined => Place::bound(wrapped).into(),
                    }
                } else {
                    nominal_lookup
                }
            }

            GeneralMemberBranch::Instance => {
                let receiver = receiver.unwrap_or(this);
                instance_like_member_lookup(db, env, key, receiver, recursion_guard)
            }

            GeneralMemberBranch::ClassObject => class_object_entry_sync(
                ClassObjectEntryRequest { key, ty: this, name, policy, receiver },
                ClassObjectEntryFacts,
                &OrdinaryClassObjectEntry { db, env, guard: recursion_guard },
            )?,


            GeneralMemberBranch::BoundSuper(bound_super) => {
                let owner_attr =
                    bound_super.find_name_in_mro_after_pivot(db, env, name_str, policy);

                bound_super
                    .try_call_dunder_get_on_attribute(db, env, owner_attr)
                    .unwrap_or_else(|| owner_attr.into())
            }
        })
    }
}

fn instance_like_member_lookup<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    key: MemberLookupKey<'db>,
    receiver: Type<'db>,
    recursion_guard: Option<&CallableRecursionGuard<'db>>,
) -> MemberLookupResult<'db> {
    match instance_member_entry_sync(
        key,
        receiver,
        LookupFacts,
        &InlineMemberEntry {
            db,
            env,
            recursion_guard,
        },
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}
