use infer::nearest_enclosing_class;
use itertools::{Either, Itertools};
use ruff_db::parsed::parsed_module;

use std::borrow::Cow;
use std::slice::Iter;

use bitflags::bitflags;
use call::{CallDunderError, CallError, CallErrorKind};
use context::InferContext;
use diagnostic::{
    INVALID_CONTEXT_MANAGER, INVALID_SUPER_ARGUMENT, NOT_ITERABLE, POSSIBLY_UNBOUND_IMPLICIT_CALL,
    UNAVAILABLE_IMPLICIT_SUPER_ARGUMENTS,
};
use ruff_db::diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity};
use ruff_db::files::File;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange};
use type_ordering::union_or_intersection_elements_ordering;

pub(crate) use self::builder::{IntersectionBuilder, UnionBuilder};
pub(crate) use self::cyclic::{PairVisitor, TypeTransformer};
pub use self::diagnostic::TypeCheckDiagnostics;
pub(crate) use self::diagnostic::register_lints;
pub(crate) use self::infer::{
    infer_deferred_types, infer_definition_types, infer_expression_type, infer_expression_types,
    infer_scope_types,
};
pub(crate) use self::signatures::{CallableSignature, Signature};
pub(crate) use self::subclass_of::{SubclassOfInner, SubclassOfType};
use crate::module_name::ModuleName;
use crate::module_resolver::{KnownModule, resolve_module};
use crate::place::{Boundness, Place, PlaceAndQualifiers, imported_symbol};
use crate::semantic_index::definition::Definition;
use crate::semantic_index::place::ScopedPlaceId;
use crate::semantic_index::scope::ScopeId;
use crate::semantic_index::{imported_modules, place_table, semantic_index};
use crate::suppression::check_suppressions;
use crate::types::call::{Binding, Bindings, CallArguments, CallableBinding};
use crate::types::class::{CodeGeneratorKind, Field};
pub(crate) use crate::types::class_base::ClassBase;
use crate::types::context::{LintDiagnosticGuard, LintDiagnosticGuardBuilder};
use crate::types::diagnostic::{INVALID_TYPE_FORM, UNSUPPORTED_BOOL_CONVERSION};
use crate::types::enums::{enum_metadata, is_single_member_enum};
use crate::types::function::{
    DataclassTransformerParams, FunctionSpans, FunctionType, KnownFunction,
};
use crate::types::generics::{
    GenericContext, PartialSpecialization, Specialization, bind_typevar, walk_generic_context,
    walk_partial_specialization, walk_specialization,
};
pub use crate::types::ide_support::{
    CallSignatureDetails, Member, all_members, call_signature_details, definition_kind_for_name,
    definitions_for_attribute, definitions_for_imported_symbol, definitions_for_keyword_argument,
    definitions_for_name,
};
use crate::types::infer::infer_unpack_types;
use crate::types::mro::{Mro, MroError, MroIterator};
pub(crate) use crate::types::narrow::infer_narrowing_constraint;
use crate::types::signatures::{Parameter, ParameterForm, Parameters, walk_signature};
use crate::types::tuple::TupleSpec;
use crate::unpack::EvaluationMode;
pub use crate::util::diagnostics::add_inferred_python_version_hint_to_diagnostic;
use crate::{Db, FxOrderMap, FxOrderSet, Module, Program};
pub(crate) use class::{ClassLiteral, ClassType, GenericAlias, KnownClass};
use instance::Protocol;
pub use instance::{NominalInstanceType, ProtocolInstanceType};
pub use special_form::SpecialFormType;

mod builder;
mod call;
mod class;
mod class_base;
mod context;
mod cyclic;
mod diagnostic;
mod display;
mod enums;
mod function;
mod generics;
pub(crate) mod ide_support;
mod infer;
mod instance;
mod mro;
mod narrow;
mod protocol_class;
mod signatures;
mod special_form;
mod string_annotation;
mod subclass_of;
mod tuple;
mod type_ordering;
mod unpacker;
mod visitor;

mod definition;
#[cfg(test)]
mod property_tests;

pub fn check_types(db: &dyn Db, file: File) -> Vec<Diagnostic> {
    let _span = tracing::trace_span!("check_types", ?file).entered();

    tracing::debug!("Checking file '{path}'", path = file.path(db));

    let index = semantic_index(db, file);
    let mut diagnostics = TypeCheckDiagnostics::default();

    for scope_id in index.scope_ids() {
        let result = infer_scope_types(db, scope_id);

        if let Some(scope_diagnostics) = result.diagnostics() {
            diagnostics.extend(scope_diagnostics);
        }
    }

    diagnostics.extend_diagnostics(
        index
            .semantic_syntax_errors()
            .iter()
            .map(|error| Diagnostic::invalid_syntax(file, error, error)),
    );

    check_suppressions(db, file, &mut diagnostics);

    diagnostics.into_diagnostics()
}

/// Infer the type of a binding.
pub(crate) fn binding_type<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Type<'db> {
    let inference = infer_definition_types(db, definition);
    inference.binding_type(definition)
}

/// Infer the type of a declaration.
pub(crate) fn declaration_type<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> TypeAndQualifiers<'db> {
    let inference = infer_definition_types(db, definition);
    inference.declaration_type(definition)
}

/// Infer the type of a (possibly deferred) sub-expression of a [`Definition`].
///
/// Supports expressions that are evaluated within a type-params sub-scope.
///
/// ## Panics
/// If the given expression is not a sub-expression of the given [`Definition`].
fn definition_expression_type<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    expression: &ast::Expr,
) -> Type<'db> {
    let file = definition.file(db);
    let index = semantic_index(db, file);
    let file_scope = index.expression_scope_id(expression);
    let scope = file_scope.to_scope_id(db, file);
    if scope == definition.scope(db) {
        // expression is in the definition scope
        let inference = infer_definition_types(db, definition);
        if let Some(ty) = inference.try_expression_type(expression) {
            ty
        } else {
            infer_deferred_types(db, definition).expression_type(expression)
        }
    } else {
        // expression is in a type-params sub-scope
        infer_scope_types(db, scope).expression_type(expression)
    }
}

/// The descriptor protocol distinguishes two kinds of descriptors. Non-data descriptors
/// define a `__get__` method, while data descriptors additionally define a `__set__`
/// method or a `__delete__` method. This enum is used to categorize attributes into two
/// groups: (1) data descriptors and (2) normal attributes or non-data descriptors.
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum AttributeKind {
    DataDescriptor,
    NormalOrNonDataDescriptor,
}

impl AttributeKind {
    const fn is_data(self) -> bool {
        matches!(self, Self::DataDescriptor)
    }
}

/// This enum is used to control the behavior of the descriptor protocol implementation.
/// When invoked on a class object, the fallback type (a class attribute) can shadow a
/// non-data descriptor of the meta-type (the class's metaclass). However, this is not
/// true for instances. When invoked on an instance, the fallback type (an attribute on
/// the instance) can not completely shadow a non-data descriptor of the meta-type (the
/// class), because we do not currently attempt to statically infer if an instance
/// attribute is definitely defined (i.e. to check whether a particular method has been
/// called).
#[derive(Clone, Debug, Copy, PartialEq)]
enum InstanceFallbackShadowsNonDataDescriptor {
    Yes,
    No,
}

bitflags! {
    #[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
    pub(crate) struct MemberLookupPolicy: u8 {
        /// Dunder methods are looked up on the meta-type of a type without potentially falling
        /// back on attributes on the type itself. For example, when implicitly invoked on an
        /// instance, dunder methods are not looked up as instance attributes. And when invoked
        /// on a class, dunder methods are only looked up on the metaclass, not the class itself.
        ///
        /// All other attributes use the `WithInstanceFallback` policy.
        ///
        /// If this flag is set - look up the attribute on the meta-type only.
        const NO_INSTANCE_FALLBACK = 1 << 0;

        /// When looking up an attribute on a class, we sometimes need to avoid
        /// looking up attributes defined on the `object` class. Usually because
        /// typeshed doesn't properly encode runtime behavior (e.g. see how `__new__` & `__init__`
        /// are handled during class creation).
        ///
        /// If this flag is set - exclude attributes defined on `object` when looking up attributes.
        const MRO_NO_OBJECT_FALLBACK = 1 << 1;

        /// When looking up an attribute on a class, we sometimes need to avoid
        /// looking up attributes defined on `type` if this is the metaclass of the class.
        ///
        /// This is similar to no object fallback above
        const META_CLASS_NO_TYPE_FALLBACK = 1 << 2;

        /// Skip looking up attributes on the builtin `int` and `str` classes.
        const MRO_NO_INT_OR_STR_LOOKUP = 1 << 3;
    }
}

impl MemberLookupPolicy {
    /// Only look up the attribute on the meta-type.
    ///
    /// If false - Look up the attribute on the meta-type, but fall back to attributes on the instance
    /// if the meta-type attribute is not found or if the meta-type attribute is not a data
    /// descriptor.
    pub(crate) const fn no_instance_fallback(self) -> bool {
        self.contains(Self::NO_INSTANCE_FALLBACK)
    }

    /// Exclude attributes defined on `object` when looking up attributes.
    pub(crate) const fn mro_no_object_fallback(self) -> bool {
        self.contains(Self::MRO_NO_OBJECT_FALLBACK)
    }

    /// Exclude attributes defined on `type` when looking up meta-class-attributes.
    pub(crate) const fn meta_class_no_type_fallback(self) -> bool {
        self.contains(Self::META_CLASS_NO_TYPE_FALLBACK)
    }

    /// Exclude attributes defined on `int` or `str` when looking up attributes.
    pub(crate) const fn mro_no_int_or_str_fallback(self) -> bool {
        self.contains(Self::MRO_NO_INT_OR_STR_LOOKUP)
    }
}

impl Default for MemberLookupPolicy {
    fn default() -> Self {
        Self::empty()
    }
}

fn member_lookup_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &PlaceAndQualifiers<'db>,
    _count: u32,
    _self: Type<'db>,
    _name: Name,
    _policy: MemberLookupPolicy,
) -> salsa::CycleRecoveryAction<PlaceAndQualifiers<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn member_lookup_cycle_initial<'db>(
    _db: &'db dyn Db,
    _self: Type<'db>,
    _name: Name,
    _policy: MemberLookupPolicy,
) -> PlaceAndQualifiers<'db> {
    Place::bound(Type::Never).into()
}

fn class_lookup_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &PlaceAndQualifiers<'db>,
    _count: u32,
    _self: Type<'db>,
    _name: Name,
    _policy: MemberLookupPolicy,
) -> salsa::CycleRecoveryAction<PlaceAndQualifiers<'db>> {
    salsa::CycleRecoveryAction::Iterate
}

fn class_lookup_cycle_initial<'db>(
    _db: &'db dyn Db,
    _self: Type<'db>,
    _name: Name,
    _policy: MemberLookupPolicy,
) -> PlaceAndQualifiers<'db> {
    Place::bound(Type::Never).into()
}

/// Meta data for `Type::Todo`, which represents a known limitation in ty.
#[cfg(debug_assertions)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct TodoType(pub &'static str);

#[cfg(debug_assertions)]
impl std::fmt::Display for TodoType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({msg})", msg = self.0)
    }
}

#[cfg(not(debug_assertions))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct TodoType;

#[cfg(not(debug_assertions))]
impl std::fmt::Display for TodoType {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

/// Create a `Type::Todo` variant to represent a known limitation in the type system.
///
/// It can be created by specifying a custom message: `todo_type!("PEP 604 not supported")`.
#[cfg(debug_assertions)]
macro_rules! todo_type {
    ($message:literal) => {{
        const _: () = {
            let s = $message;

            if !s.is_ascii() {
                panic!("todo_type! message must be ASCII");
            }

            let bytes = s.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                // Check each byte for '(' or ')'
                let ch = bytes[i];

                assert!(
                    !40u8.eq_ignore_ascii_case(&ch) && !41u8.eq_ignore_ascii_case(&ch),
                    "todo_type! message must not contain parentheses",
                );
                i += 1;
            }
        };
        $crate::types::Type::Dynamic($crate::types::DynamicType::Todo($crate::types::TodoType(
            $message,
        )))
    }};
    ($message:ident) => {
        $crate::types::Type::Dynamic($crate::types::DynamicType::Todo($crate::types::TodoType(
            $message,
        )))
    };
}

#[cfg(not(debug_assertions))]
macro_rules! todo_type {
    () => {
        $crate::types::Type::Dynamic($crate::types::DynamicType::Todo(crate::types::TodoType))
    };
    ($message:literal) => {
        $crate::types::Type::Dynamic($crate::types::DynamicType::Todo(crate::types::TodoType))
    };
    ($message:ident) => {
        $crate::types::Type::Dynamic($crate::types::DynamicType::Todo(crate::types::TodoType))
    };
}

pub use crate::types::definition::TypeDefinition;
pub(crate) use todo_type;

/// Represents an instance of `builtins.property`.
///
/// # Ordering
/// Ordering is based on the property instance's salsa-assigned id and not on its values.
/// The id may change between runs, or when the property instance was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct PropertyInstanceType<'db> {
    getter: Option<Type<'db>>,
    setter: Option<Type<'db>>,
}

fn walk_property_instance_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    property: PropertyInstanceType<'db>,
    visitor: &V,
) {
    if let Some(getter) = property.getter(db) {
        visitor.visit_type(db, getter);
    }
    if let Some(setter) = property.setter(db) {
        visitor.visit_type(db, setter);
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for PropertyInstanceType<'_> {}

impl<'db> PropertyInstanceType<'db> {
    fn apply_type_mapping<'a>(self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        let getter = self
            .getter(db)
            .map(|ty| ty.apply_type_mapping(db, type_mapping));
        let setter = self
            .setter(db)
            .map(|ty| ty.apply_type_mapping(db, type_mapping));
        Self::new(db, getter, setter)
    }

    fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        Self::new(
            db,
            self.getter(db).map(|ty| ty.normalized_impl(db, visitor)),
            self.setter(db).map(|ty| ty.normalized_impl(db, visitor)),
        )
    }

    fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        if let Some(ty) = self.getter(db) {
            ty.find_legacy_typevars(db, binding_context, typevars);
        }
        if let Some(ty) = self.setter(db) {
            ty.find_legacy_typevars(db, binding_context, typevars);
        }
    }

    fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self::new(
            db,
            self.getter(db).map(|ty| ty.materialize(db, variance)),
            self.setter(db).map(|ty| ty.materialize(db, variance)),
        )
    }
}

bitflags! {
    /// Used for the return type of `dataclass(…)` calls. Keeps track of the arguments
    /// that were passed in. For the precise meaning of the fields, see [1].
    ///
    /// [1]: https://docs.python.org/3/library/dataclasses.html
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct DataclassParams: u16 {
        const INIT = 0b0000_0000_0001;
        const REPR = 0b0000_0000_0010;
        const EQ = 0b0000_0000_0100;
        const ORDER = 0b0000_0000_1000;
        const UNSAFE_HASH = 0b0000_0001_0000;
        const FROZEN = 0b0000_0010_0000;
        const MATCH_ARGS = 0b0000_0100_0000;
        const KW_ONLY = 0b0000_1000_0000;
        const SLOTS = 0b0001_0000_0000;
        const WEAKREF_SLOT = 0b0010_0000_0000;
    }
}

impl get_size2::GetSize for DataclassParams {}

impl Default for DataclassParams {
    fn default() -> Self {
        Self::INIT | Self::REPR | Self::EQ | Self::MATCH_ARGS
    }
}

impl From<DataclassTransformerParams> for DataclassParams {
    fn from(params: DataclassTransformerParams) -> Self {
        let mut result = Self::default();

        result.set(
            Self::EQ,
            params.contains(DataclassTransformerParams::EQ_DEFAULT),
        );
        result.set(
            Self::ORDER,
            params.contains(DataclassTransformerParams::ORDER_DEFAULT),
        );
        result.set(
            Self::KW_ONLY,
            params.contains(DataclassTransformerParams::KW_ONLY_DEFAULT),
        );
        result.set(
            Self::FROZEN,
            params.contains(DataclassTransformerParams::FROZEN_DEFAULT),
        );

        result
    }
}

/// Representation of a type: a set of possible values at runtime.
///
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum Type<'db> {
    /// The dynamic type: a statically unknown set of values
    Dynamic(DynamicType),
    /// The empty set of values
    Never,
    /// A specific function object
    FunctionLiteral(FunctionType<'db>),
    /// Represents a callable `instance.method` where `instance` is an instance of a class
    /// and `method` is a method (of that class).
    ///
    /// See [`BoundMethodType`] for more information.
    ///
    /// TODO: consider replacing this with `Callable & Instance(MethodType)`?
    /// I.e. if we have a method `def f(self, x: int) -> str`, and see it being called as
    /// `instance.f`, we could partially apply (and check) the `instance` argument against
    /// the `self` parameter, and return a `MethodType & Callable[[int], str]`.
    /// One drawback would be that we could not show the bound instance when that type is displayed.
    BoundMethod(BoundMethodType<'db>),
    /// Represents a specific instance of `types.MethodWrapperType`.
    ///
    /// TODO: consider replacing this with `Callable & types.MethodWrapperType` type?
    /// Requires `Callable` to be able to represent overloads, e.g. `types.FunctionType.__get__` has
    /// this behaviour when a method is accessed on a class vs an instance:
    ///
    /// ```txt
    ///  * (None,   type)         ->  Literal[function_on_which_it_was_called]
    ///  * (object, type | None)  ->  BoundMethod[instance, function_on_which_it_was_called]
    /// ```
    MethodWrapper(MethodWrapperKind<'db>),
    /// Represents a specific instance of `types.WrapperDescriptorType`.
    ///
    /// TODO: Similar to above, this could eventually be replaced by a generic `Callable`
    /// type. We currently add this as a separate variant because `FunctionType.__get__`
    /// is an overloaded method and we do not support `@overload` yet.
    WrapperDescriptor(WrapperDescriptorKind),
    /// A special callable that is returned by a `dataclass(…)` call. It is usually
    /// used as a decorator. Note that this is only used as a return type for actual
    /// `dataclass` calls, not for the argumentless `@dataclass` decorator.
    DataclassDecorator(DataclassParams),
    /// A special callable that is returned by a `dataclass_transform(…)` call.
    DataclassTransformer(DataclassTransformerParams),
    /// The type of an arbitrary callable object with a certain specified signature.
    Callable(CallableType<'db>),
    /// A specific module object
    ModuleLiteral(ModuleLiteralType<'db>),
    /// A specific class object
    ClassLiteral(ClassLiteral<'db>),
    /// A specialization of a generic class
    GenericAlias(GenericAlias<'db>),
    /// The set of all class objects that are subclasses of the given class (C), spelled `type[C]`.
    SubclassOf(SubclassOfType<'db>),
    /// The set of Python objects with the given class in their __class__'s method resolution order.
    /// Construct this variant using the `Type::instance` constructor function.
    NominalInstance(NominalInstanceType<'db>),
    /// The set of Python objects that conform to the interface described by a given protocol.
    /// Construct this variant using the `Type::instance` constructor function.
    ProtocolInstance(ProtocolInstanceType<'db>),
    /// A single Python object that requires special treatment in the type system,
    /// and which exists at a location that can be known prior to any analysis by ty.
    SpecialForm(SpecialFormType),
    /// Singleton types that are heavily special-cased by ty, and which are usually
    /// created as a result of some runtime operation (e.g. a type-alias statement,
    /// a typevar definition, or `Generic[T]` in a class's bases list).
    KnownInstance(KnownInstanceType<'db>),
    /// An instance of `builtins.property`
    PropertyInstance(PropertyInstanceType<'db>),
    /// The set of objects in any of the types in the union
    Union(UnionType<'db>),
    /// The set of objects in all of the types in the intersection
    Intersection(IntersectionType<'db>),
    /// Represents objects whose `__bool__` method is deterministic:
    /// - `AlwaysTruthy`: `__bool__` always returns `True`
    /// - `AlwaysFalsy`: `__bool__` always returns `False`
    AlwaysTruthy,
    AlwaysFalsy,
    /// An integer literal
    IntLiteral(i64),
    /// A boolean literal, either `True` or `False`.
    BooleanLiteral(bool),
    /// A string literal whose value is known
    StringLiteral(StringLiteralType<'db>),
    /// A singleton type that represents a specific enum member
    EnumLiteral(EnumLiteralType<'db>),
    /// A string known to originate only from literal values, but whose value is not known (unlike
    /// `StringLiteral` above).
    LiteralString,
    /// A bytes literal
    BytesLiteral(BytesLiteralType<'db>),
    /// An instance of a typevar in a generic class or function. When the generic class or function
    /// is specialized, we will replace this typevar with its specialization.
    TypeVar(BoundTypeVarInstance<'db>),
    /// A bound super object like `super()` or `super(A, A())`
    /// This type doesn't handle an unbound super object like `super(A)`; for that we just use
    /// a `Type::NominalInstance` of `builtins.super`.
    BoundSuper(BoundSuperType<'db>),
    /// A subtype of `bool` that allows narrowing in both positive and negative cases.
    TypeIs(TypeIsType<'db>),
    /// A type that represents an inhabitant of a `TypedDict`.
    TypedDict(TypedDictType<'db>),
}

#[salsa::tracked]
impl<'db> Type<'db> {
    pub const fn any() -> Self {
        Self::Dynamic(DynamicType::Any)
    }

    pub const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub fn object(db: &'db dyn Db) -> Self {
        KnownClass::Object.to_instance(db)
    }

    pub const fn is_unknown(&self) -> bool {
        matches!(self, Type::Dynamic(DynamicType::Unknown))
    }

    pub const fn is_never(&self) -> bool {
        matches!(self, Type::Never)
    }

    /// Returns `true` if `self` is [`Type::Callable`].
    pub const fn is_callable_type(&self) -> bool {
        matches!(self, Type::Callable(..))
    }

    fn is_none(&self, db: &'db dyn Db) -> bool {
        self.into_nominal_instance()
            .is_some_and(|instance| instance.class(db).is_known(db, KnownClass::NoneType))
    }

    fn is_bool(&self, db: &'db dyn Db) -> bool {
        self.into_nominal_instance()
            .is_some_and(|instance| instance.class(db).is_known(db, KnownClass::Bool))
    }

    pub fn is_notimplemented(&self, db: &'db dyn Db) -> bool {
        self.into_nominal_instance().is_some_and(|instance| {
            instance
                .class(db)
                .is_known(db, KnownClass::NotImplementedType)
        })
    }

    pub fn is_object(&self, db: &'db dyn Db) -> bool {
        self.into_nominal_instance()
            .is_some_and(|instance| instance.is_object(db))
    }

    pub const fn is_todo(&self) -> bool {
        matches!(self, Type::Dynamic(DynamicType::Todo(_)))
    }

    pub const fn is_generic_alias(&self) -> bool {
        matches!(self, Type::GenericAlias(_))
    }

    const fn is_dynamic(&self) -> bool {
        matches!(self, Type::Dynamic(_))
    }

    /// Returns the top materialization (or upper bound materialization) of this type, which is the
    /// most general form of the type that is fully static.
    #[must_use]
    pub(crate) fn top_materialization(&self, db: &'db dyn Db) -> Type<'db> {
        self.materialize(db, TypeVarVariance::Covariant)
    }

    /// Returns the bottom materialization (or lower bound materialization) of this type, which is
    /// the most specific form of the type that is fully static.
    #[must_use]
    pub(crate) fn bottom_materialization(&self, db: &'db dyn Db) -> Type<'db> {
        self.materialize(db, TypeVarVariance::Contravariant)
    }

    /// If this type is an instance type where the class has a tuple spec, returns the tuple spec.
    ///
    /// I.e., for the type `tuple[int, str]`, this will return the tuple spec `[int, str]`.
    /// For a subclass of `tuple[int, str]`, it will return the same tuple spec.
    fn tuple_instance_spec(&self, db: &'db dyn Db) -> Option<Cow<'db, TupleSpec<'db>>> {
        self.into_nominal_instance()
            .and_then(|instance| instance.tuple_spec(db))
    }

    /// If this type is an *exact* tuple type (*not* a subclass of `tuple`), returns the
    /// tuple spec.
    ///
    /// You usually don't want to use this method, as you usually want to consider a subclass
    /// of a tuple type in the same way as the `tuple` type itself. Only use this method if you
    /// are certain that a *literal tuple* is required, and that a subclass of tuple will not
    /// do.
    ///
    /// I.e., for the type `tuple[int, str]`, this will return the tuple spec `[int, str]`.
    /// But for a subclass of `tuple[int, str]`, it will return `None`.
    fn exact_tuple_instance_spec(&self, db: &'db dyn Db) -> Option<Cow<'db, TupleSpec<'db>>> {
        self.into_nominal_instance()
            .and_then(|instance| instance.own_tuple_spec(db))
    }

    /// Returns the materialization of this type depending on the given `variance`.
    ///
    /// More concretely, `T'`, the materialization of `T`, is the type `T` with all occurrences of
    /// the dynamic types (`Any`, `Unknown`, `Todo`) replaced as follows:
    ///
    /// - In covariant position, it's replaced with `object`
    /// - In contravariant position, it's replaced with `Never`
    /// - In invariant position, it's replaced with an unresolved type variable
    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Type<'db> {
        match self {
            Type::Dynamic(_) => match variance {
                // TODO: For an invariant position, e.g. `list[Any]`, it should be replaced with an
                // existential type representing "all lists, containing any type." We currently
                // represent this by replacing `Any` in invariant position with an unresolved type
                // variable.
                TypeVarVariance::Invariant => Type::TypeVar(BoundTypeVarInstance::new(
                    db,
                    TypeVarInstance::new(
                        db,
                        Name::new_static("T_all"),
                        None,
                        None,
                        variance,
                        None,
                        TypeVarKind::Pep695,
                    ),
                    BindingContext::Synthetic,
                )),
                TypeVarVariance::Covariant => Type::object(db),
                TypeVarVariance::Contravariant => Type::Never,
                TypeVarVariance::Bivariant => unreachable!(),
            },

            Type::Never
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::ClassLiteral(_)
            | Type::BoundSuper(_) => *self,

            Type::PropertyInstance(property_instance) => {
                Type::PropertyInstance(property_instance.materialize(db, variance))
            }

            Type::FunctionLiteral(_) | Type::BoundMethod(_) => {
                // TODO: Subtyping between function / methods with a callable accounts for the
                // signature (parameters and return type), so we might need to do something here
                *self
            }

            Type::NominalInstance(instance) => instance.materialize(db, variance),
            Type::GenericAlias(generic_alias) => {
                Type::GenericAlias(generic_alias.materialize(db, variance))
            }
            Type::Callable(callable_type) => {
                Type::Callable(callable_type.materialize(db, variance))
            }
            Type::SubclassOf(subclass_of_type) => subclass_of_type.materialize(db, variance),
            Type::ProtocolInstance(protocol_instance_type) => {
                // TODO: Add tests for this once subtyping/assignability is implemented for
                // protocols. It _might_ require changing the logic here because:
                //
                // > Subtyping for protocol instances involves taking account of the fact that
                // > read-only property members, and method members, on protocols act covariantly;
                // > write-only property members act contravariantly; and read/write attribute
                // > members on protocols act invariantly
                Type::ProtocolInstance(protocol_instance_type.materialize(db, variance))
            }
            Type::Union(union_type) => union_type.map(db, |ty| ty.materialize(db, variance)),
            Type::Intersection(intersection_type) => IntersectionBuilder::new(db)
                .positive_elements(
                    intersection_type
                        .positive(db)
                        .iter()
                        .map(|ty| ty.materialize(db, variance)),
                )
                .negative_elements(
                    intersection_type
                        .negative(db)
                        .iter()
                        .map(|ty| ty.materialize(db, variance.flip())),
                )
                .build(),
            Type::TypeVar(bound_typevar) => Type::TypeVar(bound_typevar.materialize(db, variance)),
            Type::TypeIs(type_is) => {
                type_is.with_type(db, type_is.return_type(db).materialize(db, variance))
            }
            Type::TypedDict(_) => {
                // TODO: Materialization of gradual TypedDicts
                *self
            }
        }
    }

    pub const fn into_class_literal(self) -> Option<ClassLiteral<'db>> {
        match self {
            Type::ClassLiteral(class_type) => Some(class_type),
            _ => None,
        }
    }

    pub const fn into_subclass_of(self) -> Option<SubclassOfType<'db>> {
        match self {
            Type::SubclassOf(subclass_of) => Some(subclass_of),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_class_literal(self) -> ClassLiteral<'db> {
        self.into_class_literal()
            .expect("Expected a Type::ClassLiteral variant")
    }

    pub const fn is_subclass_of(&self) -> bool {
        matches!(self, Type::SubclassOf(..))
    }

    pub const fn is_class_literal(&self) -> bool {
        matches!(self, Type::ClassLiteral(..))
    }

    pub fn into_enum_literal(self) -> Option<EnumLiteralType<'db>> {
        match self {
            Type::EnumLiteral(enum_literal) => Some(enum_literal),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_enum_literal(self) -> EnumLiteralType<'db> {
        self.into_enum_literal()
            .expect("Expected a Type::EnumLiteral variant")
    }

    pub(crate) const fn is_typed_dict(&self) -> bool {
        matches!(self, Type::TypedDict(..))
    }

    pub(crate) fn into_typed_dict(self) -> Option<TypedDictType<'db>> {
        match self {
            Type::TypedDict(typed_dict) => Some(typed_dict),
            _ => None,
        }
    }

    /// Turn a class literal (`Type::ClassLiteral` or `Type::GenericAlias`) into a `ClassType`.
    /// Since a `ClassType` must be specialized, apply the default specialization to any
    /// unspecialized generic class literal.
    pub fn to_class_type(self, db: &'db dyn Db) -> Option<ClassType<'db>> {
        match self {
            Type::ClassLiteral(class_literal) => Some(class_literal.default_specialization(db)),
            Type::GenericAlias(alias) => Some(ClassType::Generic(alias)),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_class_type(self, db: &'db dyn Db) -> ClassType<'db> {
        self.to_class_type(db)
            .expect("Expected a Type::GenericAlias or Type::ClassLiteral variant")
    }

    pub fn is_class_type(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::ClassLiteral(class) if class.generic_context(db).is_none() => true,
            Type::GenericAlias(_) => true,
            _ => false,
        }
    }

    pub const fn is_property_instance(&self) -> bool {
        matches!(self, Type::PropertyInstance(..))
    }

    pub fn module_literal(db: &'db dyn Db, importing_file: File, submodule: Module<'db>) -> Self {
        Self::ModuleLiteral(ModuleLiteralType::new(
            db,
            submodule,
            submodule.kind(db).is_package().then_some(importing_file),
        ))
    }

    pub const fn into_module_literal(self) -> Option<ModuleLiteralType<'db>> {
        match self {
            Type::ModuleLiteral(module) => Some(module),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_module_literal(self) -> ModuleLiteralType<'db> {
        self.into_module_literal()
            .expect("Expected a Type::ModuleLiteral variant")
    }

    pub const fn into_union(self) -> Option<UnionType<'db>> {
        match self {
            Type::Union(union_type) => Some(union_type),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_union(self) -> UnionType<'db> {
        self.into_union().expect("Expected a Type::Union variant")
    }

    pub const fn is_union(&self) -> bool {
        matches!(self, Type::Union(..))
    }

    pub const fn into_intersection(self) -> Option<IntersectionType<'db>> {
        match self {
            Type::Intersection(intersection_type) => Some(intersection_type),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_intersection(self) -> IntersectionType<'db> {
        self.into_intersection()
            .expect("Expected a Type::Intersection variant")
    }

    pub const fn into_function_literal(self) -> Option<FunctionType<'db>> {
        match self {
            Type::FunctionLiteral(function_type) => Some(function_type),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_function_literal(self) -> FunctionType<'db> {
        self.into_function_literal()
            .expect("Expected a Type::FunctionLiteral variant")
    }

    pub const fn is_function_literal(&self) -> bool {
        matches!(self, Type::FunctionLiteral(..))
    }

    pub const fn is_bound_method(&self) -> bool {
        matches!(self, Type::BoundMethod(..))
    }

    pub fn is_union_of_single_valued(&self, db: &'db dyn Db) -> bool {
        self.into_union().is_some_and(|union| {
            union
                .elements(db)
                .iter()
                .all(|ty| ty.is_single_valued(db) || ty.is_bool(db) || ty.is_literal_string())
        }) || self.is_bool(db)
            || self.is_literal_string()
    }

    pub const fn into_int_literal(self) -> Option<i64> {
        match self {
            Type::IntLiteral(value) => Some(value),
            _ => None,
        }
    }

    pub fn into_string_literal(self) -> Option<StringLiteralType<'db>> {
        match self {
            Type::StringLiteral(string_literal) => Some(string_literal),
            _ => None,
        }
    }

    pub fn is_string_literal(&self) -> bool {
        matches!(self, Type::StringLiteral(..))
    }

    #[track_caller]
    pub fn expect_int_literal(self) -> i64 {
        self.into_int_literal()
            .expect("Expected a Type::IntLiteral variant")
    }

    pub const fn is_boolean_literal(&self) -> bool {
        matches!(self, Type::BooleanLiteral(..))
    }

    pub const fn is_literal_string(&self) -> bool {
        matches!(self, Type::LiteralString)
    }

    pub fn string_literal(db: &'db dyn Db, string: &str) -> Self {
        Self::StringLiteral(StringLiteralType::new(db, string))
    }

    pub fn bytes_literal(db: &'db dyn Db, bytes: &[u8]) -> Self {
        Self::BytesLiteral(BytesLiteralType::new(db, bytes))
    }

    #[must_use]
    pub fn negate(&self, db: &'db dyn Db) -> Type<'db> {
        IntersectionBuilder::new(db).add_negative(*self).build()
    }

    #[must_use]
    pub fn negate_if(&self, db: &'db dyn Db, yes: bool) -> Type<'db> {
        if yes { self.negate(db) } else { *self }
    }

    /// Returns the fallback instance type that a literal is an instance of, or `None` if the type
    /// is not a literal.
    pub fn literal_fallback_instance(self, db: &'db dyn Db) -> Option<Type<'db>> {
        // There are other literal types that could conceivable be included here: class literals
        // falling back to `type[X]`, for instance. For now, there is not much rigorous thought put
        // into what's included vs not; this is just an empirical choice that makes our ecosystem
        // report look better until we have proper bidirectional type inference.
        match self {
            Type::StringLiteral(_) | Type::LiteralString => Some(KnownClass::Str.to_instance(db)),
            Type::BooleanLiteral(_) => Some(KnownClass::Bool.to_instance(db)),
            Type::IntLiteral(_) => Some(KnownClass::Int.to_instance(db)),
            Type::BytesLiteral(_) => Some(KnownClass::Bytes.to_instance(db)),
            Type::ModuleLiteral(_) => Some(KnownClass::ModuleType.to_instance(db)),
            Type::EnumLiteral(literal) => Some(literal.enum_class_instance(db)),
            _ => None,
        }
    }

    /// Return a "normalized" version of `self` that ensures that equivalent types have the same Salsa ID.
    ///
    /// A normalized type:
    /// - Has all unions and intersections sorted according to a canonical order,
    ///   no matter how "deeply" a union/intersection may be nested.
    /// - Strips the names of positional-only parameters and variadic parameters from `Callable` types,
    ///   as these are irrelevant to whether a callable type `X` is equivalent to a callable type `Y`.
    /// - Strips the types of default values from parameters in `Callable` types: only whether a parameter
    ///   *has* or *does not have* a default value is relevant to whether two `Callable` types  are equivalent.
    /// - Converts class-based protocols into synthesized protocols
    #[must_use]
    pub fn normalized(self, db: &'db dyn Db) -> Self {
        self.normalized_impl(db, &TypeTransformer::default())
    }

    #[must_use]
    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        match self {
            Type::Union(union) => {
                visitor.visit(self, || Type::Union(union.normalized_impl(db, visitor)))
            }
            Type::Intersection(intersection) => visitor.visit(self, || {
                Type::Intersection(intersection.normalized_impl(db, visitor))
            }),
            Type::Callable(callable) => visitor.visit(self, || {
                Type::Callable(callable.normalized_impl(db, visitor))
            }),
            Type::ProtocolInstance(protocol) => {
                visitor.visit(self, || protocol.normalized_impl(db, visitor))
            }
            Type::NominalInstance(instance) => {
                visitor.visit(self, || instance.normalized_impl(db, visitor))
            }
            Type::FunctionLiteral(function) => visitor.visit(self, || {
                Type::FunctionLiteral(function.normalized_impl(db, visitor))
            }),
            Type::PropertyInstance(property) => visitor.visit(self, || {
                Type::PropertyInstance(property.normalized_impl(db, visitor))
            }),
            Type::MethodWrapper(method_kind) => visitor.visit(self, || {
                Type::MethodWrapper(method_kind.normalized_impl(db, visitor))
            }),
            Type::BoundMethod(method) => visitor.visit(self, || {
                Type::BoundMethod(method.normalized_impl(db, visitor))
            }),
            Type::BoundSuper(bound_super) => visitor.visit(self, || {
                Type::BoundSuper(bound_super.normalized_impl(db, visitor))
            }),
            Type::GenericAlias(generic) => visitor.visit(self, || {
                Type::GenericAlias(generic.normalized_impl(db, visitor))
            }),
            Type::SubclassOf(subclass_of) => visitor.visit(self, || {
                Type::SubclassOf(subclass_of.normalized_impl(db, visitor))
            }),
            Type::TypeVar(bound_typevar) => visitor.visit(self, || {
                Type::TypeVar(bound_typevar.normalized_impl(db, visitor))
            }),
            Type::KnownInstance(known_instance) => visitor.visit(self, || {
                Type::KnownInstance(known_instance.normalized_impl(db, visitor))
            }),
            Type::TypeIs(type_is) => visitor.visit(self, || {
                type_is.with_type(db, type_is.return_type(db).normalized_impl(db, visitor))
            }),
            Type::Dynamic(dynamic) => Type::Dynamic(dynamic.normalized()),
            Type::EnumLiteral(enum_literal)
                if is_single_member_enum(db, enum_literal.enum_class(db)) =>
            {
                // Always normalize single-member enums to their class instance (`Literal[Single.VALUE]` => `Single`)
                enum_literal.enum_class_instance(db)
            }

            Type::TypedDict(_) => {
                // TODO: Normalize TypedDicts
                self
            }

            Type::LiteralString
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::BooleanLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::StringLiteral(_)
            | Type::Never
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::SpecialForm(_)
            | Type::IntLiteral(_) => self,
        }
    }

    /// Return `true` if subtyping is always reflexive for this type; `T <: T` is always true for
    /// any `T` of this type.
    ///
    /// This is true for fully static types, but also for some types that may not be fully static.
    /// For example, a `ClassLiteral` may inherit `Any`, but its subtyping is still reflexive.
    ///
    /// This method may have false negatives, but it should not have false positives. It should be
    /// a cheap shallow check, not an exhaustive recursive check.
    fn subtyping_is_always_reflexive(self) -> bool {
        match self {
            Type::Never
            | Type::FunctionLiteral(..)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(..)
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::PropertyInstance(_)
            // might inherit `Any`, but subtyping is still reflexive
            | Type::ClassLiteral(_)
             => true,
            Type::Dynamic(_)
            | Type::NominalInstance(_)
            | Type::ProtocolInstance(_)
            | Type::GenericAlias(_)
            | Type::SubclassOf(_)
            | Type::Union(_)
            | Type::Intersection(_)
            | Type::Callable(_)
            | Type::TypeVar(_)
            | Type::BoundSuper(_)
            | Type::TypeIs(_)
            | Type::TypedDict(_) => false,
        }
    }

    pub(crate) fn into_callable(self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Type::Callable(_) => Some(self),

            Type::Dynamic(_) => Some(CallableType::single(db, Signature::dynamic(self))),

            Type::FunctionLiteral(function_literal) => {
                Some(Type::Callable(function_literal.into_callable_type(db)))
            }
            Type::BoundMethod(bound_method) => {
                Some(Type::Callable(bound_method.into_callable_type(db)))
            }

            Type::NominalInstance(_) | Type::ProtocolInstance(_) => {
                let call_symbol = self
                    .member_lookup_with_policy(
                        db,
                        Name::new_static("__call__"),
                        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    )
                    .place;

                if let Place::Type(ty, Boundness::Bound) = call_symbol {
                    ty.into_callable(db)
                } else {
                    None
                }
            }
            Type::ClassLiteral(class_literal) => {
                Some(ClassType::NonGeneric(class_literal).into_callable(db))
            }

            Type::GenericAlias(alias) => Some(ClassType::Generic(alias).into_callable(db)),

            // TODO: This is unsound so in future we can consider an opt-in option to disable it.
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Class(class) => Some(class.into_callable(db)),
                SubclassOfInner::Dynamic(dynamic) => Some(CallableType::single(
                    db,
                    Signature::new(Parameters::unknown(), Some(Type::Dynamic(dynamic))),
                )),
            },

            Type::Union(union) => union.try_map(db, |element| element.into_callable(db)),

            Type::EnumLiteral(enum_literal) => {
                enum_literal.enum_class_instance(db).into_callable(db)
            }

            Type::Never
            | Type::DataclassTransformer(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::BytesLiteral(_)
            | Type::TypeIs(_)
            | Type::TypedDict(_) => None,

            // TODO
            Type::MethodWrapper(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::ModuleLiteral(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::PropertyInstance(_)
            | Type::Intersection(_)
            | Type::TypeVar(_)
            | Type::BoundSuper(_) => None,
        }
    }
    /// Return true if this type is a [subtype of] type `target`.
    ///
    /// For fully static types, this means that the set of objects represented by `self` is a
    /// subset of the objects represented by `target`.
    ///
    /// For gradual types, it means that the union of all possible sets of values represented by
    /// `self` (the "top materialization" of `self`) is a subtype of the intersection of all
    /// possible sets of values represented by `target` (the "bottom materialization" of
    /// `target`). In other words, for all possible pairs of materializations `self'` and
    /// `target'`, `self'` is always a subtype of `target'`.
    ///
    /// Note that this latter expansion of the subtyping relation to non-fully-static types is not
    /// described in the typing spec, but the primary use of the subtyping relation is for
    /// simplifying unions and intersections, and this expansion to gradual types is sound and
    /// allows us to better simplify many unions and intersections. This definition does mean the
    /// subtyping relation is not reflexive for non-fully-static types (e.g. `Any` is not a subtype
    /// of `Any`).
    ///
    /// [subtype of]: https://typing.python.org/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
    ///
    /// There would be an even more general definition of subtyping for gradual types, allowing a
    /// type `S` to be a subtype of a type `T` if the top materialization of `S` (`S+`) is a
    /// subtype of `T+`, and the bottom materialization of `S` (`S-`) is a subtype of `T-`. This
    /// definition is attractive in that it would restore reflexivity of subtyping for all types,
    /// and would mean that gradual equivalence of `S` and `T` could be defined simply as `S <: T
    /// && T <: S`. It would also be sound, in that simplifying unions or intersections according
    /// to this definition of subtyping would still result in an equivalent type.
    ///
    /// Unfortunately using this definition would break transitivity of subtyping when both nominal
    /// and structural types are involved, because Liskov enforcement for nominal types is based on
    /// assignability, so we can have class `A` with method `def meth(self) -> Any` and a subclass
    /// `B(A)` with method `def meth(self) -> int`. In this case, `A` would be a subtype of a
    /// protocol `P` with method `def meth(self) -> Any`, but `B` would not be a subtype of `P`,
    /// and yet `B` is (by nominal subtyping) a subtype of `A`, so we would have `B <: A` and `A <:
    /// P`, but not `B <: P`. Losing transitivity of subtyping is not tenable (it makes union and
    /// intersection simplification dependent on the order in which elements are added), so we do
    /// not use this more general definition of subtyping.
    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        self.has_relation_to(db, target, TypeRelation::Subtyping)
    }

    /// Return true if this type is [assignable to] type `target`.
    ///
    /// [assignable to]: https://typing.python.org/en/latest/spec/concepts.html#the-assignable-to-or-consistent-subtyping-relation
    pub(crate) fn is_assignable_to(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        self.has_relation_to(db, target, TypeRelation::Assignability)
    }

    fn has_relation_to(self, db: &'db dyn Db, target: Type<'db>, relation: TypeRelation) -> bool {
        // Subtyping implies assignability, so if subtyping is reflexive and the two types are
        // equal, it is both a subtype and assignable. Assignability is always reflexive.
        //
        // Note that we could do a full equivalence check here, but that would be both expensive
        // and unnecessary. This early return is only an optimisation.
        if (relation.is_assignability() || self.subtyping_is_always_reflexive()) && self == target {
            return true;
        }

        match (self, target) {
            // Everything is a subtype of `object`.
            (_, Type::NominalInstance(instance)) if instance.is_object(db) => true,

            // `Never` is the bottom type, the empty set.
            // It is a subtype of all other types.
            (Type::Never, _) => true,

            // Dynamic is only a subtype of `object` and only a supertype of `Never`; both were
            // handled above. It's always assignable, though.
            (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => relation.is_assignability(),

            // Pretend that instances of `dataclasses.Field` are assignable to their default type.
            // This allows field definitions like `name: str = field(default="")` in dataclasses
            // to pass the assignability check of the inferred type to the declared type.
            (Type::KnownInstance(KnownInstanceType::Field(field)), right)
                if relation.is_assignability() =>
            {
                field.default_type(db).has_relation_to(db, right, relation)
            }

            (Type::TypedDict(_), _) | (_, Type::TypedDict(_)) => {
                // TODO: Implement assignability and subtyping for TypedDict
                relation.is_assignability()
            }

            // In general, a TypeVar `T` is not a subtype of a type `S` unless one of the two conditions is satisfied:
            // 1. `T` is a bound TypeVar and `T`'s upper bound is a subtype of `S`.
            //    TypeVars without an explicit upper bound are treated as having an implicit upper bound of `object`.
            // 2. `T` is a constrained TypeVar and all of `T`'s constraints are subtypes of `S`.
            //
            // However, there is one exception to this general rule: for any given typevar `T`,
            // `T` will always be a subtype of any union containing `T`.
            // A similar rule applies in reverse to intersection types.
            (Type::TypeVar(_), Type::Union(union)) if union.elements(db).contains(&self) => true,
            (Type::Intersection(intersection), Type::TypeVar(_))
                if intersection.positive(db).contains(&target) =>
            {
                true
            }
            (Type::Intersection(intersection), Type::TypeVar(_))
                if intersection.negative(db).contains(&target) =>
            {
                false
            }

            // Two identical typevars must always solve to the same type, so they are always
            // subtypes of each other and assignable to each other.
            //
            // Note that this is not handled by the early return at the beginning of this method,
            // since subtyping between a TypeVar and an arbitrary other type cannot be guaranteed to be reflexive.
            (Type::TypeVar(lhs_bound_typevar), Type::TypeVar(rhs_bound_typevar))
                if lhs_bound_typevar == rhs_bound_typevar =>
            {
                true
            }

            // A fully static typevar is a subtype of its upper bound, and to something similar to
            // the union of its constraints. An unbound, unconstrained, fully static typevar has an
            // implicit upper bound of `object` (which is handled above).
            (Type::TypeVar(bound_typevar), _)
                if bound_typevar.typevar(db).bound_or_constraints(db).is_some() =>
            {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => unreachable!(),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.has_relation_to(db, target, relation)
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .elements(db)
                        .iter()
                        .all(|constraint| constraint.has_relation_to(db, target, relation)),
                }
            }

            // If the typevar is constrained, there must be multiple constraints, and the typevar
            // might be specialized to any one of them. However, the constraints do not have to be
            // disjoint, which means an lhs type might be a subtype of all of the constraints.
            (_, Type::TypeVar(bound_typevar))
                if bound_typevar
                    .typevar(db)
                    .constraints(db)
                    .is_some_and(|constraints| {
                        constraints
                            .iter()
                            .all(|constraint| self.has_relation_to(db, *constraint, relation))
                    }) =>
            {
                true
            }

            // `Never` is the bottom type, the empty set.
            // Other than one unlikely edge case (TypeVars bound to `Never`),
            // no other type is a subtype of or assignable to `Never`.
            (_, Type::Never) => false,

            (Type::Union(union), _) => union
                .elements(db)
                .iter()
                .all(|&elem_ty| elem_ty.has_relation_to(db, target, relation)),

            (_, Type::Union(union)) => union
                .elements(db)
                .iter()
                .any(|&elem_ty| self.has_relation_to(db, elem_ty, relation)),

            // If both sides are intersections we need to handle the right side first
            // (A & B & C) is a subtype of (A & B) because the left is a subtype of both A and B,
            // but none of A, B, or C is a subtype of (A & B).
            (_, Type::Intersection(intersection)) => {
                intersection
                    .positive(db)
                    .iter()
                    .all(|&pos_ty| self.has_relation_to(db, pos_ty, relation))
                    && intersection
                        .negative(db)
                        .iter()
                        .all(|&neg_ty| self.is_disjoint_from(db, neg_ty))
            }

            (Type::Intersection(intersection), _) => intersection
                .positive(db)
                .iter()
                .any(|&elem_ty| elem_ty.has_relation_to(db, target, relation)),

            // Other than the special cases checked above, no other types are a subtype of a
            // typevar, since there's no guarantee what type the typevar will be specialized to.
            // (If the typevar is bounded, it might be specialized to a smaller type than the
            // bound. This is true even if the bound is a final class, since the typevar can still
            // be specialized to `Never`.)
            (_, Type::TypeVar(_)) => false,

            // Note that the definition of `Type::AlwaysFalsy` depends on the return value of `__bool__`.
            // If `__bool__` always returns True or False, it can be treated as a subtype of `AlwaysTruthy` or `AlwaysFalsy`, respectively.
            (left, Type::AlwaysFalsy) => left.bool(db).is_always_false(),
            (left, Type::AlwaysTruthy) => left.bool(db).is_always_true(),
            // Currently, the only supertype of `AlwaysFalsy` and `AlwaysTruthy` is the universal set (object instance).
            (Type::AlwaysFalsy | Type::AlwaysTruthy, _) => {
                target.is_equivalent_to(db, Type::object(db))
            }

            // These clauses handle type variants that include function literals. A function
            // literal is the subtype of itself, and not of any other function literal. However,
            // our representation of a function literal includes any specialization that should be
            // applied to the signature. Different specializations of the same function literal are
            // only subtypes of each other if they result in the same signature.
            (Type::FunctionLiteral(self_function), Type::FunctionLiteral(target_function)) => {
                self_function.has_relation_to(db, target_function, relation)
            }
            (Type::BoundMethod(self_method), Type::BoundMethod(target_method)) => {
                self_method.has_relation_to(db, target_method, relation)
            }
            (Type::MethodWrapper(self_method), Type::MethodWrapper(target_method)) => {
                self_method.has_relation_to(db, target_method, relation)
            }

            // No literal type is a subtype of any other literal type, unless they are the same
            // type (which is handled above). This case is not necessary from a correctness
            // perspective (the fallback cases below will handle it correctly), but it is important
            // for performance of simplifying large unions of literal types.
            (
                Type::StringLiteral(_)
                | Type::IntLiteral(_)
                | Type::BytesLiteral(_)
                | Type::ClassLiteral(_)
                | Type::FunctionLiteral(_)
                | Type::ModuleLiteral(_)
                | Type::EnumLiteral(_),
                Type::StringLiteral(_)
                | Type::IntLiteral(_)
                | Type::BytesLiteral(_)
                | Type::ClassLiteral(_)
                | Type::FunctionLiteral(_)
                | Type::ModuleLiteral(_)
                | Type::EnumLiteral(_),
            ) => false,

            (Type::Callable(self_callable), Type::Callable(other_callable)) => {
                self_callable.has_relation_to(db, other_callable, relation)
            }

            (_, Type::Callable(_)) => self
                .into_callable(db)
                .is_some_and(|callable| callable.has_relation_to(db, target, relation)),

            (Type::ProtocolInstance(left), Type::ProtocolInstance(right)) => {
                left.has_relation_to(db, right, relation)
            }
            // A protocol instance can never be a subtype of a nominal type, with the *sole* exception of `object`.
            (Type::ProtocolInstance(_), _) => false,
            (_, Type::ProtocolInstance(protocol)) => {
                self.satisfies_protocol(db, protocol, relation)
            }

            // All `StringLiteral` types are a subtype of `LiteralString`.
            (Type::StringLiteral(_), Type::LiteralString) => true,

            // An instance is a subtype of an enum literal, if it is an instance of the enum class
            // and the enum has only one member.
            (Type::NominalInstance(_), Type::EnumLiteral(target_enum_literal)) => {
                if target_enum_literal.enum_class_instance(db) != self {
                    return false;
                }

                is_single_member_enum(db, target_enum_literal.enum_class(db))
            }

            // Except for the special `LiteralString` case above,
            // most `Literal` types delegate to their instance fallbacks
            // unless `self` is exactly equivalent to `target` (handled above)
            (
                Type::StringLiteral(_)
                | Type::LiteralString
                | Type::BooleanLiteral(_)
                | Type::IntLiteral(_)
                | Type::BytesLiteral(_)
                | Type::ModuleLiteral(_)
                | Type::EnumLiteral(_),
                _,
            ) => (self.literal_fallback_instance(db))
                .is_some_and(|instance| instance.has_relation_to(db, target, relation)),

            // A `FunctionLiteral` type is a single-valued type like the other literals handled above,
            // so it also, for now, just delegates to its instance fallback.
            (Type::FunctionLiteral(_), _) => KnownClass::FunctionType
                .to_instance(db)
                .has_relation_to(db, target, relation),

            // The same reasoning applies for these special callable types:
            (Type::BoundMethod(_), _) => KnownClass::MethodType
                .to_instance(db)
                .has_relation_to(db, target, relation),
            (Type::MethodWrapper(_), _) => KnownClass::WrapperDescriptorType
                .to_instance(db)
                .has_relation_to(db, target, relation),
            (Type::WrapperDescriptor(_), _) => KnownClass::WrapperDescriptorType
                .to_instance(db)
                .has_relation_to(db, target, relation),

            (Type::DataclassDecorator(_) | Type::DataclassTransformer(_), _) => {
                // TODO: Implement subtyping using an equivalent `Callable` type.
                false
            }

            // `TypeIs` is invariant.
            (Type::TypeIs(left), Type::TypeIs(right)) => {
                left.return_type(db)
                    .has_relation_to(db, right.return_type(db), relation)
                    && right
                        .return_type(db)
                        .has_relation_to(db, left.return_type(db), relation)
            }

            // `TypeIs[T]` is a subtype of `bool`.
            (Type::TypeIs(_), _) => KnownClass::Bool
                .to_instance(db)
                .has_relation_to(db, target, relation),

            // Function-like callables are subtypes of `FunctionType`
            (Type::Callable(callable), _)
                if callable.is_function_like(db)
                    && KnownClass::FunctionType
                        .to_instance(db)
                        .has_relation_to(db, target, relation) =>
            {
                true
            }

            (Type::Callable(_), _) => false,

            (Type::BoundSuper(_), Type::BoundSuper(_)) => self.is_equivalent_to(db, target),
            (Type::BoundSuper(_), _) => KnownClass::Super
                .to_instance(db)
                .has_relation_to(db, target, relation),

            // `Literal[<class 'C'>]` is a subtype of `type[B]` if `C` is a subclass of `B`,
            // since `type[B]` describes all possible runtime subclasses of the class object `B`.
            (Type::ClassLiteral(class), Type::SubclassOf(target_subclass_ty)) => target_subclass_ty
                .subclass_of()
                .into_class()
                .map(|subclass_of_class| {
                    ClassType::NonGeneric(class).has_relation_to(db, subclass_of_class, relation)
                })
                .unwrap_or(relation.is_assignability()),
            (Type::GenericAlias(alias), Type::SubclassOf(target_subclass_ty)) => target_subclass_ty
                .subclass_of()
                .into_class()
                .map(|subclass_of_class| {
                    ClassType::Generic(alias).has_relation_to(db, subclass_of_class, relation)
                })
                .unwrap_or(relation.is_assignability()),

            // This branch asks: given two types `type[T]` and `type[S]`, is `type[T]` a subtype of `type[S]`?
            (Type::SubclassOf(self_subclass_ty), Type::SubclassOf(target_subclass_ty)) => {
                self_subclass_ty.has_relation_to(db, target_subclass_ty, relation)
            }

            // `Literal[str]` is a subtype of `type` because the `str` class object is an instance of its metaclass `type`.
            // `Literal[abc.ABC]` is a subtype of `abc.ABCMeta` because the `abc.ABC` class object
            // is an instance of its metaclass `abc.ABCMeta`.
            (Type::ClassLiteral(class), _) => class
                .metaclass_instance_type(db)
                .has_relation_to(db, target, relation),
            (Type::GenericAlias(alias), _) => ClassType::from(alias)
                .metaclass_instance_type(db)
                .has_relation_to(db, target, relation),

            // `type[Any]` is a subtype of `type[object]`, and is assignable to any `type[...]`
            (Type::SubclassOf(subclass_of_ty), other) if subclass_of_ty.is_dynamic() => {
                KnownClass::Type
                    .to_instance(db)
                    .has_relation_to(db, other, relation)
                    || (relation.is_assignability()
                        && other.has_relation_to(db, KnownClass::Type.to_instance(db), relation))
            }

            // Any `type[...]` type is assignable to `type[Any]`
            (other, Type::SubclassOf(subclass_of_ty))
                if subclass_of_ty.is_dynamic() && relation.is_assignability() =>
            {
                other.has_relation_to(db, KnownClass::Type.to_instance(db), relation)
            }

            // `type[str]` (== `SubclassOf("str")` in ty) describes all possible runtime subclasses
            // of the class object `str`. It is a subtype of `type` (== `Instance("type")`) because `str`
            // is an instance of `type`, and so all possible subclasses of `str` will also be instances of `type`.
            //
            // Similarly `type[enum.Enum]`  is a subtype of `enum.EnumMeta` because `enum.Enum`
            // is an instance of `enum.EnumMeta`. `type[Any]` and `type[Unknown]` do not participate in subtyping,
            // however, as they are not fully static types.
            (Type::SubclassOf(subclass_of_ty), _) => subclass_of_ty
                .subclass_of()
                .into_class()
                .map(|class| class.metaclass_instance_type(db))
                .unwrap_or_else(|| KnownClass::Type.to_instance(db))
                .has_relation_to(db, target, relation),

            // For example: `Type::SpecialForm(SpecialFormType::Type)` is a subtype of `Type::NominalInstance(_SpecialForm)`,
            // because `Type::SpecialForm(SpecialFormType::Type)` is a set with exactly one runtime value in it
            // (the symbol `typing.Type`), and that symbol is known to be an instance of `typing._SpecialForm` at runtime.
            (Type::SpecialForm(left), right) => left
                .instance_fallback(db)
                .has_relation_to(db, right, relation),

            (Type::KnownInstance(left), right) => left
                .instance_fallback(db)
                .has_relation_to(db, right, relation),

            // `bool` is a subtype of `int`, because `bool` subclasses `int`,
            // which means that all instances of `bool` are also instances of `int`
            (Type::NominalInstance(self_instance), Type::NominalInstance(target_instance)) => {
                self_instance.has_relation_to(db, target_instance, relation)
            }

            (Type::PropertyInstance(_), _) => KnownClass::Property
                .to_instance(db)
                .has_relation_to(db, target, relation),
            (_, Type::PropertyInstance(_)) => {
                self.has_relation_to(db, KnownClass::Property.to_instance(db), relation)
            }

            // Other than the special cases enumerated above, `Instance` types and typevars are
            // never subtypes of any other variants
            (Type::NominalInstance(_) | Type::TypeVar(_), _) => false,
        }
    }

    /// Return true if this type is [equivalent to] type `other`.
    ///
    /// Two equivalent types represent the same sets of values.
    ///
    /// > Two gradual types `A` and `B` are equivalent
    /// > (that is, the same gradual type, not merely consistent with one another)
    /// > if and only if all materializations of `A` are also materializations of `B`,
    /// > and all materializations of `B` are also materializations of `A`.
    /// >
    /// > &mdash; [Summary of type relations]
    ///
    /// [equivalent to]: https://typing.python.org/en/latest/spec/glossary.html#term-equivalent
    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        if self == other {
            return true;
        }

        match (self, other) {
            (Type::Dynamic(_), Type::Dynamic(_)) => true,

            (Type::SubclassOf(first), Type::SubclassOf(second)) => {
                match (first.subclass_of(), second.subclass_of()) {
                    (first, second) if first == second => true,
                    (SubclassOfInner::Dynamic(_), SubclassOfInner::Dynamic(_)) => true,
                    _ => false,
                }
            }

            (Type::NominalInstance(first), Type::NominalInstance(second)) => {
                first.is_equivalent_to(db, second)
            }

            (Type::Union(first), Type::Union(second)) => first.is_equivalent_to(db, second),

            (Type::Intersection(first), Type::Intersection(second)) => {
                first.is_equivalent_to(db, second)
            }

            (Type::FunctionLiteral(self_function), Type::FunctionLiteral(target_function)) => {
                self_function.is_equivalent_to(db, target_function)
            }
            (Type::BoundMethod(self_method), Type::BoundMethod(target_method)) => {
                self_method.is_equivalent_to(db, target_method)
            }
            (Type::MethodWrapper(self_method), Type::MethodWrapper(target_method)) => {
                self_method.is_equivalent_to(db, target_method)
            }
            (Type::Callable(first), Type::Callable(second)) => first.is_equivalent_to(db, second),

            (Type::ProtocolInstance(first), Type::ProtocolInstance(second)) => {
                first.is_equivalent_to(db, second)
            }
            (Type::ProtocolInstance(protocol), nominal @ Type::NominalInstance(n))
            | (nominal @ Type::NominalInstance(n), Type::ProtocolInstance(protocol)) => {
                n.is_object(db) && protocol.normalized(db) == nominal
            }
            // An instance of an enum class is equivalent to an enum literal of that class,
            // if that enum has only has one member.
            (Type::NominalInstance(instance), Type::EnumLiteral(literal))
            | (Type::EnumLiteral(literal), Type::NominalInstance(instance)) => {
                if literal.enum_class_instance(db) != Type::NominalInstance(instance) {
                    return false;
                }

                let class_literal = instance.class(db).class_literal(db).0;
                is_single_member_enum(db, class_literal)
            }
            _ => false,
        }
    }

    /// Return true if this type and `other` have no common elements.
    ///
    /// Note: This function aims to have no false positives, but might return
    /// wrong `false` answers in some cases.
    pub(crate) fn is_disjoint_from(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        self.is_disjoint_from_impl(db, other, &PairVisitor::new(false))
    }

    pub(crate) fn is_disjoint_from_impl(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        visitor: &PairVisitor<'db>,
    ) -> bool {
        fn any_protocol_members_absent_or_disjoint<'db>(
            db: &'db dyn Db,
            protocol: ProtocolInstanceType<'db>,
            other: Type<'db>,
            visitor: &PairVisitor<'db>,
        ) -> bool {
            protocol.interface(db).members(db).any(|member| {
                other
                    .member(db, member.name())
                    .place
                    .ignore_possibly_unbound()
                    .is_none_or(|attribute_type| {
                        member.has_disjoint_type_from(db, attribute_type, visitor)
                    })
            })
        }

        match (self, other) {
            (Type::Never, _) | (_, Type::Never) => true,

            (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => false,

            (Type::TypedDict(_), _) | (_, Type::TypedDict(_)) => {
                // TODO: Implement disjointness for TypedDict
                false
            }

            // A typevar is never disjoint from itself, since all occurrences of the typevar must
            // be specialized to the same type. (This is an important difference between typevars
            // and `Any`!) Different typevars might be disjoint, depending on their bounds and
            // constraints, which are handled below.
            (Type::TypeVar(self_bound_typevar), Type::TypeVar(other_bound_typevar))
                if self_bound_typevar == other_bound_typevar =>
            {
                false
            }

            (tvar @ Type::TypeVar(_), Type::Intersection(intersection))
            | (Type::Intersection(intersection), tvar @ Type::TypeVar(_))
                if intersection.negative(db).contains(&tvar) =>
            {
                true
            }

            // An unbounded typevar is never disjoint from any other type, since it might be
            // specialized to any type. A bounded typevar is not disjoint from its bound, and is
            // only disjoint from other types if its bound is. A constrained typevar is disjoint
            // from a type if all of its constraints are.
            (Type::TypeVar(bound_typevar), other) | (other, Type::TypeVar(bound_typevar)) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => false,
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.is_disjoint_from_impl(db, other, visitor)
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .elements(db)
                        .iter()
                        .all(|constraint| constraint.is_disjoint_from_impl(db, other, visitor)),
                }
            }

            (Type::Union(union), other) | (other, Type::Union(union)) => union
                .elements(db)
                .iter()
                .all(|e| e.is_disjoint_from_impl(db, other, visitor)),

            // If we have two intersections, we test the positive elements of each one against the other intersection
            // Negative elements need a positive element on the other side in order to be disjoint.
            // This is similar to what would happen if we tried to build a new intersection that combines the two
            (Type::Intersection(self_intersection), Type::Intersection(other_intersection)) => {
                self_intersection
                    .positive(db)
                    .iter()
                    .any(|p| p.is_disjoint_from_impl(db, other, visitor))
                    || other_intersection
                        .positive(db)
                        .iter()
                        .any(|p: &Type<'_>| p.is_disjoint_from_impl(db, self, visitor))
            }

            (Type::Intersection(intersection), other)
            | (other, Type::Intersection(intersection)) => {
                intersection
                    .positive(db)
                    .iter()
                    .any(|p| p.is_disjoint_from_impl(db, other, visitor))
                    // A & B & Not[C] is disjoint from C
                    || intersection
                        .negative(db)
                        .iter()
                        .any(|&neg_ty| other.is_subtype_of(db, neg_ty))
            }

            // any single-valued type is disjoint from another single-valued type
            // iff the two types are nonequal
            (
                left @ (Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::EnumLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::MethodWrapper(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)
                | Type::GenericAlias(..)
                | Type::SpecialForm(..)
                | Type::KnownInstance(..)),
                right @ (Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::EnumLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::MethodWrapper(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)
                | Type::GenericAlias(..)
                | Type::SpecialForm(..)
                | Type::KnownInstance(..)),
            ) => left != right,

            (
                Type::SubclassOf(_),
                Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::LiteralString
                | Type::BytesLiteral(..)
                | Type::EnumLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::MethodWrapper(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..),
            )
            | (
                Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::LiteralString
                | Type::BytesLiteral(..)
                | Type::EnumLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::MethodWrapper(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..),
                Type::SubclassOf(_),
            ) => true,

            (Type::AlwaysTruthy, ty) | (ty, Type::AlwaysTruthy) => {
                // `Truthiness::Ambiguous` may include `AlwaysTrue` as a subset, so it's not guaranteed to be disjoint.
                // Thus, they are only disjoint if `ty.bool() == AlwaysFalse`.
                ty.bool(db).is_always_false()
            }
            (Type::AlwaysFalsy, ty) | (ty, Type::AlwaysFalsy) => {
                // Similarly, they are only disjoint if `ty.bool() == AlwaysTrue`.
                ty.bool(db).is_always_true()
            }

            (Type::ProtocolInstance(left), Type::ProtocolInstance(right)) => {
                left.is_disjoint_from_impl(db, right, visitor)
            }

            (Type::ProtocolInstance(protocol), Type::SpecialForm(special_form))
            | (Type::SpecialForm(special_form), Type::ProtocolInstance(protocol)) => {
                any_protocol_members_absent_or_disjoint(db, protocol, special_form.instance_fallback(db), visitor)
            }

            (Type::ProtocolInstance(protocol), Type::KnownInstance(known_instance))
            | (Type::KnownInstance(known_instance), Type::ProtocolInstance(protocol)) => {
                any_protocol_members_absent_or_disjoint(db, protocol, known_instance.instance_fallback(db), visitor)
            }

            // The absence of a protocol member on one of these types guarantees
            // that the type will be disjoint from the protocol,
            // but the type will not be disjoint from the protocol if it has a member
            // that is of the correct type but is possibly unbound.
            // If accessing a member on this type returns a possibly unbound `Place`,
            // the type will not be a subtype of the protocol but it will also not be
            // disjoint from the protocol, since there are possible subtypes of the type
            // that could satisfy the protocol.
            //
            // ```py
            // class Foo:
            //     if coinflip():
            //         X = 42
            //
            // class HasX(Protocol):
            //     @property
            //     def x(self) -> int: ...
            //
            // # `TypeOf[Foo]` (a class-literal type) is not a subtype of `HasX`,
            // # but `TypeOf[Foo]` & HasX` should not simplify to `Never`,
            // # or this branch would be incorrectly understood to be unreachable,
            // # since we would understand the type of `Foo` in this branch to be
            // # `TypeOf[Foo] & HasX` due to `hasattr()` narrowing.
            //
            // if hasattr(Foo, "X"):
            //     print(Foo.X)
            // ```
            (
                ty @ (Type::LiteralString
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::BooleanLiteral(..)
                | Type::ClassLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::ModuleLiteral(..)
                | Type::GenericAlias(..)
                | Type::IntLiteral(..)
                | Type::EnumLiteral(..)
            ),
                Type::ProtocolInstance(protocol),
            )
            | (
                Type::ProtocolInstance(protocol),
                ty @ (Type::LiteralString
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::BooleanLiteral(..)
                | Type::ClassLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::ModuleLiteral(..)
                | Type::GenericAlias(..)
                | Type::IntLiteral(..)
                | Type::EnumLiteral(..)),
            )  => any_protocol_members_absent_or_disjoint(db, protocol, ty, visitor),

            // This is the same as the branch above --
            // once guard patterns are stabilised, it could be unified with that branch
            // (<https://github.com/rust-lang/rust/issues/129967>)
            (Type::ProtocolInstance(protocol), nominal @ Type::NominalInstance(n))
            | (nominal @ Type::NominalInstance(n), Type::ProtocolInstance(protocol))
                if n.class(db).is_final(db) =>
            {
                any_protocol_members_absent_or_disjoint(db, protocol, nominal, visitor)
            }

            (Type::ProtocolInstance(protocol), other)
            | (other, Type::ProtocolInstance(protocol)) => {
                protocol.interface(db).members(db).any(|member| {
                    matches!(
                        other.member(db, member.name()).place,
                        Place::Type(attribute_type, _) if member.has_disjoint_type_from(db, attribute_type, visitor)
                    )
                })
            }

            (Type::SubclassOf(subclass_of_ty), Type::ClassLiteral(class_b))
            | (Type::ClassLiteral(class_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => false,
                    SubclassOfInner::Class(class_a) => !class_b.is_subclass_of(db, None, class_a),
                }
            }

            (Type::SubclassOf(subclass_of_ty), Type::GenericAlias(alias_b))
            | (Type::GenericAlias(alias_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => false,
                    SubclassOfInner::Class(class_a) => {
                        !ClassType::from(alias_b).is_subclass_of(db, class_a)
                    }
                }
            }

            (Type::SubclassOf(left), Type::SubclassOf(right)) => left.is_disjoint_from_impl(db, right),

            // for `type[Any]`/`type[Unknown]`/`type[Todo]`, we know the type cannot be any larger than `type`,
            // so although the type is dynamic we can still determine disjointedness in some situations
            (Type::SubclassOf(subclass_of_ty), other)
            | (other, Type::SubclassOf(subclass_of_ty)) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Dynamic(_) => {
                    KnownClass::Type.to_instance(db).is_disjoint_from_impl(db, other, visitor)
                }
                SubclassOfInner::Class(class) => class
                    .metaclass_instance_type(db)
                    .is_disjoint_from_impl(db, other, visitor),
            },

            (Type::SpecialForm(special_form), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::SpecialForm(special_form)) => {
                !special_form.is_instance_of(db, instance.class(db))
            }

            (Type::KnownInstance(known_instance), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::KnownInstance(known_instance)) => {
                !known_instance.is_instance_of(db, instance.class(db))
            }

            (Type::BooleanLiteral(..) | Type::TypeIs(_), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::BooleanLiteral(..) | Type::TypeIs(_)) => {
                // A `Type::BooleanLiteral()` must be an instance of exactly `bool`
                // (it cannot be an instance of a `bool` subclass)
                !KnownClass::Bool.is_subclass_of(db, instance.class(db))
            }

            (Type::BooleanLiteral(..) | Type::TypeIs(_), _)
            | (_, Type::BooleanLiteral(..) | Type::TypeIs(_)) => true,

            (Type::IntLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::IntLiteral(..)) => {
                // A `Type::IntLiteral()` must be an instance of exactly `int`
                // (it cannot be an instance of an `int` subclass)
                !KnownClass::Int.is_subclass_of(db, instance.class(db))
            }

            (Type::IntLiteral(..), _) | (_, Type::IntLiteral(..)) => true,

            (Type::StringLiteral(..), Type::LiteralString)
            | (Type::LiteralString, Type::StringLiteral(..)) => false,

            (Type::StringLiteral(..) | Type::LiteralString, Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::StringLiteral(..) | Type::LiteralString) => {
                // A `Type::StringLiteral()` or a `Type::LiteralString` must be an instance of exactly `str`
                // (it cannot be an instance of a `str` subclass)
                !KnownClass::Str.is_subclass_of(db, instance.class(db))
            }

            (Type::LiteralString, Type::LiteralString) => false,
            (Type::LiteralString, _) | (_, Type::LiteralString) => true,

            (Type::BytesLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::BytesLiteral(..)) => {
                // A `Type::BytesLiteral()` must be an instance of exactly `bytes`
                // (it cannot be an instance of a `bytes` subclass)
                !KnownClass::Bytes.is_subclass_of(db, instance.class(db))
            }

            (Type::EnumLiteral(enum_literal), instance@Type::NominalInstance(_))
            | (instance@Type::NominalInstance(_), Type::EnumLiteral(enum_literal)) => {
                !enum_literal.enum_class_instance(db).is_subtype_of(db, instance)
            }
            (Type::EnumLiteral(..), _) | (_, Type::EnumLiteral(..)) => true,

            // A class-literal type `X` is always disjoint from an instance type `Y`,
            // unless the type expressing "all instances of `Z`" is a subtype of of `Y`,
            // where `Z` is `X`'s metaclass.
            (Type::ClassLiteral(class), instance @ Type::NominalInstance(_))
            | (instance @ Type::NominalInstance(_), Type::ClassLiteral(class)) => !class
                .metaclass_instance_type(db)
                .is_subtype_of(db, instance),
            (Type::GenericAlias(alias), instance @ Type::NominalInstance(_))
            | (instance @ Type::NominalInstance(_), Type::GenericAlias(alias)) => {
                !ClassType::from(alias)
                    .metaclass_instance_type(db)
                    .is_subtype_of(db, instance)
            }

            (Type::FunctionLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::FunctionLiteral(..)) => {
                // A `Type::FunctionLiteral()` must be an instance of exactly `types.FunctionType`
                // (it cannot be an instance of a `types.FunctionType` subclass)
                !KnownClass::FunctionType.is_subclass_of(db, instance.class(db))
            }

            (Type::BoundMethod(_), other) | (other, Type::BoundMethod(_)) => KnownClass::MethodType
                .to_instance(db)
                .is_disjoint_from_impl(db, other, visitor),

            (Type::MethodWrapper(_), other) | (other, Type::MethodWrapper(_)) => {
                KnownClass::MethodWrapperType
                    .to_instance(db)
                    .is_disjoint_from_impl(db, other, visitor)
            }

            (Type::WrapperDescriptor(_), other) | (other, Type::WrapperDescriptor(_)) => {
                KnownClass::WrapperDescriptorType
                    .to_instance(db)
                    .is_disjoint_from_impl(db, other, visitor)
            }

            (Type::Callable(_) | Type::FunctionLiteral(_), Type::Callable(_))
            | (Type::Callable(_), Type::FunctionLiteral(_)) => {
                // No two callable types are ever disjoint because
                // `(*args: object, **kwargs: object) -> Never` is a subtype of all fully static
                // callable types.
                false
            }

            (Type::Callable(_), Type::StringLiteral(_) | Type::BytesLiteral(_))
            | (Type::StringLiteral(_) | Type::BytesLiteral(_), Type::Callable(_)) => {
                // A callable type is disjoint from other literal types. For example,
                // `Type::StringLiteral` must be an instance of exactly `str`, not a subclass
                // of `str`, and `str` is not callable. The same applies to other literal types.
                true
            }

            (Type::Callable(_), Type::SpecialForm(special_form))
            | (Type::SpecialForm(special_form), Type::Callable(_)) => {
                // A callable type is disjoint from special form types, except for special forms
                // that are callable (like TypedDict and collection constructors).
                // Most special forms are type constructors/annotations (like `typing.Literal`,
                // `typing.Union`, etc.) that are subscripted, not called.
                !special_form.is_callable()
            }

            (
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
                instance @ Type::NominalInstance(nominal),
            )
            | (
                instance @ Type::NominalInstance(nominal),
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
            ) if nominal.class(db).is_final(db) => instance
                .member_lookup_with_policy(
                    db,
                    Name::new_static("__call__"),
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place
                .ignore_possibly_unbound()
                .is_none_or(|dunder_call| {
                    !dunder_call.is_assignable_to(db, CallableType::unknown(db))
                }),

            (
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
                _,
            )
            | (
                _,
                Type::Callable(_) | Type::DataclassDecorator(_) | Type::DataclassTransformer(_),
            ) => {
                // TODO: Implement disjointness for general callable type with other types
                false
            }

            (Type::ModuleLiteral(..), other @ Type::NominalInstance(..))
            | (other @ Type::NominalInstance(..), Type::ModuleLiteral(..)) => {
                // Modules *can* actually be instances of `ModuleType` subclasses
                other.is_disjoint_from_impl(db, KnownClass::ModuleType.to_instance(db), visitor)
            }

            (Type::NominalInstance(left), Type::NominalInstance(right)) => {
                left.is_disjoint_from_impl(db, right, visitor)
            }

            (Type::PropertyInstance(_), other) | (other, Type::PropertyInstance(_)) => {
                KnownClass::Property
                    .to_instance(db)
                    .is_disjoint_from_impl(db, other, visitor)
            }

            (Type::BoundSuper(_), Type::BoundSuper(_)) => !self.is_equivalent_to(db, other),
            (Type::BoundSuper(_), other) | (other, Type::BoundSuper(_)) => KnownClass::Super
                .to_instance(db)
                .is_disjoint_from_impl(db, other, visitor),
        }
    }

    /// Return true if there is just a single inhabitant for this type.
    ///
    /// Note: This function aims to have no false positives, but might return `false`
    /// for more complicated types that are actually singletons.
    pub(crate) fn is_singleton(self, db: &'db dyn Db) -> bool {
        match self {
            Type::Dynamic(_)
            | Type::Never
            | Type::IntLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::LiteralString => {
                // Note: The literal types included in this pattern are not true singletons.
                // There can be multiple Python objects (at different memory locations) that
                // are both of type Literal[345], for example.
                false
            }

            Type::ProtocolInstance(..) => {
                // It *might* be possible to have a singleton protocol-instance type...?
                //
                // E.g.:
                //
                // ```py
                // from typing import Protocol, Callable
                //
                // class WeirdAndWacky(Protocol):
                //     @property
                //     def __class__(self) -> Callable[[], None]: ...
                // ```
                //
                // `WeirdAndWacky` only has a single possible inhabitant: `None`!
                // It is thus a singleton type.
                // However, going out of our way to recognise it as such is probably not worth it.
                // Such cases should anyway be exceedingly rare and/or contrived.
                false
            }

            // An unbounded, unconstrained typevar is not a singleton, because it can be
            // specialized to a non-singleton type. A bounded typevar is not a singleton, even if
            // the bound is a final singleton class, since it can still be specialized to `Never`.
            // A constrained typevar is a singleton if all of its constraints are singletons. (Note
            // that you cannot specialize a constrained typevar to a subtype of a constraint.)
            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => false,
                    Some(TypeVarBoundOrConstraints::UpperBound(_)) => false,
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .elements(db)
                        .iter()
                        .all(|constraint| constraint.is_singleton(db)),
                }
            }

            // We eagerly transform `SubclassOf` to `ClassLiteral` for final types, so `SubclassOf` is never a singleton.
            Type::SubclassOf(..) => false,
            Type::BoundSuper(..) => false,
            Type::BooleanLiteral(_)
            | Type::FunctionLiteral(..)
            | Type::WrapperDescriptor(..)
            | Type::ClassLiteral(..)
            | Type::GenericAlias(..)
            | Type::ModuleLiteral(..)
            | Type::EnumLiteral(..) => true,
            Type::SpecialForm(special_form) => {
                // Nearly all `SpecialForm` types are singletons, but if a symbol could validly
                // originate from either `typing` or `typing_extensions` then this is not guaranteed.
                // E.g. `typing.TypeGuard` is equivalent to `typing_extensions.TypeGuard`, so both are treated
                // as inhabiting the type `SpecialFormType::TypeGuard` in our model, but they are actually
                // distinct symbols at different memory addresses at runtime.
                !(special_form.check_module(KnownModule::Typing)
                    && special_form.check_module(KnownModule::TypingExtensions))
            }
            Type::KnownInstance(_) => false,
            Type::Callable(_) => {
                // A callable type is never a singleton because for any given signature,
                // there could be any number of distinct objects that are all callable with that
                // signature.
                false
            }
            Type::BoundMethod(..) => {
                // `BoundMethod` types are single-valued types, but not singleton types:
                // ```pycon
                // >>> class Foo:
                // ...     def bar(self): pass
                // >>> f = Foo()
                // >>> f.bar is f.bar
                // False
                // ```
                false
            }
            Type::MethodWrapper(_) => {
                // Just a special case of `BoundMethod` really
                // (this variant represents `f.__get__`, where `f` is any function)
                false
            }
            Type::DataclassDecorator(_) | Type::DataclassTransformer(_) => false,
            Type::NominalInstance(instance) => instance.is_singleton(db),
            Type::PropertyInstance(_) => false,
            Type::Union(..) => {
                // A single-element union, where the sole element was a singleton, would itself
                // be a singleton type. However, unions with length < 2 should never appear in
                // our model due to [`UnionBuilder::build`].
                false
            }
            Type::Intersection(..) => {
                // Here, we assume that all intersection types that are singletons would have
                // been reduced to a different form via [`IntersectionBuilder::build`] by now.
                // For example:
                //
                //   bool & ~Literal[False]   = Literal[True]
                //   None & (None | int)      = None | None & int = None
                //
                false
            }
            Type::AlwaysTruthy | Type::AlwaysFalsy => false,
            Type::TypeIs(type_is) => type_is.is_bound(db),
            Type::TypedDict(_) => false,
        }
    }

    /// Return true if this type is non-empty and all inhabitants of this type compare equal.
    pub(crate) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        match self {
            Type::FunctionLiteral(..)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(_)
            | Type::ModuleLiteral(..)
            | Type::ClassLiteral(..)
            | Type::GenericAlias(..)
            | Type::IntLiteral(..)
            | Type::BooleanLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::SpecialForm(..)
            | Type::KnownInstance(..) => true,

            Type::EnumLiteral(_) => {
                let check_dunder = |dunder_name, allowed_return_value| {
                    // Note that we do explicitly exclude dunder methods on `object`, `int` and `str` here.
                    // The reason for this is that we know that these dunder methods behave in a predictable way.
                    // Only custom dunder methods need to be examined here, as they might break single-valuedness
                    // by always returning `False`, for example.
                    let call_result = self.try_call_dunder_with_policy(
                        db,
                        dunder_name,
                        &mut CallArguments::positional([Type::unknown()]),
                        MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                            | MemberLookupPolicy::MRO_NO_INT_OR_STR_LOOKUP,
                    );
                    let call_result = call_result.as_ref();
                    call_result.is_ok_and(|bindings| {
                        bindings.return_type(db) == Type::BooleanLiteral(allowed_return_value)
                    }) || call_result
                        .is_err_and(|err| matches!(err, CallDunderError::MethodNotAvailable))
                };

                check_dunder("__eq__", true) && check_dunder("__ne__", false)
            }

            Type::ProtocolInstance(..) => {
                // See comment in the `Type::ProtocolInstance` branch for `Type::is_singleton`.
                false
            }

            // An unbounded, unconstrained typevar is not single-valued, because it can be
            // specialized to a multiple-valued type. A bounded typevar is not single-valued, even
            // if the bound is a final single-valued class, since it can still be specialized to
            // `Never`. A constrained typevar is single-valued if all of its constraints are
            // single-valued. (Note that you cannot specialize a constrained typevar to a subtype
            // of a constraint.)
            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => false,
                    Some(TypeVarBoundOrConstraints::UpperBound(_)) => false,
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .elements(db)
                        .iter()
                        .all(|constraint| constraint.is_single_valued(db)),
                }
            }

            Type::SubclassOf(..) => {
                // TODO: Same comment as above for `is_singleton`
                false
            }

            Type::NominalInstance(instance) => instance.is_single_valued(db),

            Type::BoundSuper(_) => {
                // At runtime two super instances never compare equal, even if their arguments are identical.
                false
            }

            Type::TypeIs(type_is) => type_is.is_bound(db),

            Type::Dynamic(_)
            | Type::Never
            | Type::Union(..)
            | Type::Intersection(..)
            | Type::LiteralString
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::Callable(_)
            | Type::PropertyInstance(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::TypedDict(_) => false,
        }
    }

    /// This function is roughly equivalent to `find_name_in_mro` as defined in the [descriptor guide] or
    /// [`_PyType_Lookup`] in CPython's `Objects/typeobject.c`. It should typically be called through
    /// [`Type::class_member`], unless it is known that `self` is a class-like type. This function returns
    /// `None` if called on an instance-like type.
    ///
    /// [descriptor guide]: https://docs.python.org/3/howto/descriptor.html#invocation-from-an-instance
    /// [`_PyType_Lookup`]: https://github.com/python/cpython/blob/e285232c76606e3be7bf216efb1be1e742423e4b/Objects/typeobject.c#L5223
    fn find_name_in_mro(&self, db: &'db dyn Db, name: &str) -> Option<PlaceAndQualifiers<'db>> {
        self.find_name_in_mro_with_policy(db, name, MemberLookupPolicy::default())
    }

    fn find_name_in_mro_with_policy(
        &self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Option<PlaceAndQualifiers<'db>> {
        match self {
            Type::Union(union) => Some(union.map_with_boundness_and_qualifiers(db, |elem| {
                elem.find_name_in_mro_with_policy(db, name, policy)
                    // If some elements are classes, and some are not, we simply fall back to `Unbound` for the non-class
                    // elements instead of short-circuiting the whole result to `None`. We would need a more detailed
                    // return type otherwise, and since `find_name_in_mro` is usually called via `class_member`, this is
                    // not a problem.
                    .unwrap_or_default()
            })),
            Type::Intersection(inter) => {
                Some(inter.map_with_boundness_and_qualifiers(db, |elem| {
                    elem.find_name_in_mro_with_policy(db, name, policy)
                        // Fall back to Unbound, similar to the union case (see above).
                        .unwrap_or_default()
                }))
            }

            Type::Dynamic(_) | Type::Never => Some(Place::bound(self).into()),

            Type::ClassLiteral(class) if class.is_typed_dict(db) => {
                Some(class.typed_dict_member(db, None, name, policy))
            }

            Type::ClassLiteral(class) => {
                match (class.known(db), name) {
                    (Some(KnownClass::FunctionType), "__get__") => Some(
                        Place::bound(Type::WrapperDescriptor(
                            WrapperDescriptorKind::FunctionTypeDunderGet,
                        ))
                        .into(),
                    ),
                    (Some(KnownClass::FunctionType), "__set__" | "__delete__") => {
                        // Hard code this knowledge, as we look up `__set__` and `__delete__` on `FunctionType` often.
                        Some(Place::Unbound.into())
                    }
                    (Some(KnownClass::Property), "__get__") => Some(
                        Place::bound(Type::WrapperDescriptor(
                            WrapperDescriptorKind::PropertyDunderGet,
                        ))
                        .into(),
                    ),
                    (Some(KnownClass::Property), "__set__") => Some(
                        Place::bound(Type::WrapperDescriptor(
                            WrapperDescriptorKind::PropertyDunderSet,
                        ))
                        .into(),
                    ),

                    _ => Some(class.class_member(db, name, policy)),
                }
            }

            Type::GenericAlias(alias) if alias.is_typed_dict(db) => {
                Some(alias.origin(db).typed_dict_member(db, None, name, policy))
            }

            Type::GenericAlias(alias) => {
                Some(ClassType::from(*alias).class_member(db, name, policy))
            }

            Type::SubclassOf(subclass_of_ty) => {
                subclass_of_ty.find_name_in_mro_with_policy(db, name, policy)
            }

            // Note: `super(pivot, owner).__class__` is `builtins.super`, not the owner's class.
            // `BoundSuper` should look up the name in the MRO of `builtins.super`.
            Type::BoundSuper(_) => KnownClass::Super
                .to_class_literal(db)
                .find_name_in_mro_with_policy(db, name, policy),

            // We eagerly normalize type[object], i.e. Type::SubclassOf(object) to `type`,
            // i.e. Type::NominalInstance(type). So looking up a name in the MRO of
            // `Type::NominalInstance(type)` is equivalent to looking up the name in the
            // MRO of the class `object`.
            Type::NominalInstance(instance)
                if instance.class(db).is_known(db, KnownClass::Type) =>
            {
                if policy.mro_no_object_fallback() {
                    Some(Place::Unbound.into())
                } else {
                    KnownClass::Object
                        .to_class_literal(db)
                        .find_name_in_mro_with_policy(db, name, policy)
                }
            }

            Type::FunctionLiteral(_)
            | Type::Callable(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::TypeVar(_)
            | Type::NominalInstance(_)
            | Type::ProtocolInstance(_)
            | Type::PropertyInstance(_)
            | Type::TypeIs(_)
            | Type::TypedDict(_) => None,
        }
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    #[allow(unused_variables)]
    // If we choose name `_unit`, the macro will generate code that uses `_unit`, causing clippy to fail.
    fn lookup_dunder_new(self, db: &'db dyn Db, unit: ()) -> Option<PlaceAndQualifiers<'db>> {
        self.find_name_in_mro_with_policy(
            db,
            "__new__",
            MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
        )
    }

    /// Look up an attribute in the MRO of the meta-type of `self`. This returns class-level attributes
    /// when called on an instance-like type, and metaclass attributes when called on a class-like type.
    ///
    /// Basically corresponds to `self.to_meta_type().find_name_in_mro(name)`, except for the handling
    /// of union and intersection types.
    fn class_member(self, db: &'db dyn Db, name: Name) -> PlaceAndQualifiers<'db> {
        self.class_member_with_policy(db, name, MemberLookupPolicy::default())
    }

    #[salsa::tracked(cycle_fn=class_lookup_cycle_recover, cycle_initial=class_lookup_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    fn class_member_with_policy(
        self,
        db: &'db dyn Db,
        name: Name,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        tracing::trace!("class_member: {}.{}", self.display(db), name);
        match self {
            Type::Union(union) => union.map_with_boundness_and_qualifiers(db, |elem| {
                elem.class_member_with_policy(db, name.clone(), policy)
            }),
            Type::Intersection(inter) => inter.map_with_boundness_and_qualifiers(db, |elem| {
                elem.class_member_with_policy(db, name.clone(), policy)
            }),
            // TODO: Once `to_meta_type` for the synthesized protocol is fully implemented, this handling should be removed.
            Type::ProtocolInstance(ProtocolInstanceType {
                inner: Protocol::Synthesized(_),
                ..
            }) => self.instance_member(db, &name),
            _ => self
                .to_meta_type(db)
                .find_name_in_mro_with_policy(db, name.as_str(), policy)
                .expect(
                    "`Type::find_name_in_mro()` should return `Some()` when called on a meta-type",
                ),
        }
    }

    /// This function roughly corresponds to looking up an attribute in the `__dict__` of an object.
    /// For instance-like types, this goes through the classes MRO and discovers attribute assignments
    /// in methods, as well as class-body declarations that we consider to be evidence for the presence
    /// of an instance attribute.
    ///
    /// For example, an instance of the following class has instance members `a` and `b`, but `c` is
    /// just a class attribute that would not be discovered by this method:
    /// ```py
    /// class C:
    ///     a: int
    ///
    ///     c = 1
    ///
    ///     def __init__(self):
    ///         self.b: str = "a"
    /// ```
    fn instance_member(&self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        match self {
            Type::Union(union) => {
                union.map_with_boundness_and_qualifiers(db, |elem| elem.instance_member(db, name))
            }

            Type::Intersection(intersection) => intersection
                .map_with_boundness_and_qualifiers(db, |elem| elem.instance_member(db, name)),

            Type::Dynamic(_) | Type::Never => Place::bound(self).into(),

            Type::NominalInstance(instance) => instance.class(db).instance_member(db, name),

            Type::ProtocolInstance(protocol) => protocol.instance_member(db, name),

            Type::FunctionLiteral(_) => KnownClass::FunctionType
                .to_instance(db)
                .instance_member(db, name),

            Type::BoundMethod(_) => KnownClass::MethodType
                .to_instance(db)
                .instance_member(db, name),
            Type::MethodWrapper(_) => KnownClass::MethodWrapperType
                .to_instance(db)
                .instance_member(db, name),
            Type::WrapperDescriptor(_) => KnownClass::WrapperDescriptorType
                .to_instance(db)
                .instance_member(db, name),
            Type::DataclassDecorator(_) => KnownClass::FunctionType
                .to_instance(db)
                .instance_member(db, name),
            Type::Callable(_) | Type::DataclassTransformer(_) => {
                Type::object(db).instance_member(db, name)
            }

            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => Type::object(db).instance_member(db, name),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.instance_member(db, name)
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .map_with_boundness_and_qualifiers(db, |constraint| {
                            constraint.instance_member(db, name)
                        }),
                }
            }

            Type::IntLiteral(_) => KnownClass::Int.to_instance(db).instance_member(db, name),
            Type::BooleanLiteral(_) | Type::TypeIs(_) => {
                KnownClass::Bool.to_instance(db).instance_member(db, name)
            }
            Type::StringLiteral(_) | Type::LiteralString => {
                KnownClass::Str.to_instance(db).instance_member(db, name)
            }
            Type::BytesLiteral(_) => KnownClass::Bytes.to_instance(db).instance_member(db, name),
            Type::EnumLiteral(enum_literal) => enum_literal
                .enum_class_instance(db)
                .instance_member(db, name),

            Type::AlwaysTruthy | Type::AlwaysFalsy => Type::object(db).instance_member(db, name),
            Type::ModuleLiteral(_) => KnownClass::ModuleType
                .to_instance(db)
                .instance_member(db, name),

            Type::SpecialForm(_) | Type::KnownInstance(_) => Place::Unbound.into(),

            Type::PropertyInstance(_) => KnownClass::Property
                .to_instance(db)
                .instance_member(db, name),

            // Note: `super(pivot, owner).__dict__` refers to the `__dict__` of the `builtins.super` instance,
            // not that of the owner.
            // This means we should only look up instance members defined on the `builtins.super()` instance itself.
            // If you want to look up a member in the MRO of the `super`'s owner,
            // refer to [`Type::member`] instead.
            Type::BoundSuper(_) => KnownClass::Super.to_instance(db).instance_member(db, name),

            // TODO: we currently don't model the fact that class literals and subclass-of types have
            // a `__dict__` that is filled with class level attributes. Modeling this is currently not
            // required, as `instance_member` is only called for instance-like types through `member`,
            // but we might want to add this in the future.
            Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_) => {
                Place::Unbound.into()
            }

            Type::TypedDict(_) => Place::Unbound.into(),
        }
    }

    /// Access an attribute of this type without invoking the descriptor protocol. This
    /// method corresponds to `inspect.getattr_static(<object of type 'self'>, name)`.
    ///
    /// See also: [`Type::member`]
    fn static_member(&self, db: &'db dyn Db, name: &str) -> Place<'db> {
        if let Type::ModuleLiteral(module) = self {
            module.static_member(db, name).place
        } else if let place @ Place::Type(_, _) = self.class_member(db, name.into()).place {
            place
        } else if let Some(place @ Place::Type(_, _)) =
            self.find_name_in_mro(db, name).map(|inner| inner.place)
        {
            place
        } else {
            self.instance_member(db, name).place
        }
    }

    /// Look up `__get__` on the meta-type of self, and call it with the arguments `self`, `instance`,
    /// and `owner`. `__get__` is different than other dunder methods in that it is not looked up using
    /// the descriptor protocol itself.
    ///
    /// In addition to the return type of `__get__`, this method also returns the *kind* of attribute
    /// that `self` represents: (1) a data descriptor or (2) a non-data descriptor / normal attribute.
    ///
    /// If `__get__` is not defined on the meta-type, this method returns `None`.
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn try_call_dunder_get(
        self,
        db: &'db dyn Db,
        instance: Type<'db>,
        owner: Type<'db>,
    ) -> Option<(Type<'db>, AttributeKind)> {
        tracing::trace!(
            "try_call_dunder_get: {}, {}, {}",
            self.display(db),
            instance.display(db),
            owner.display(db)
        );
        match self {
            Type::Callable(callable) if callable.is_function_like(db) => {
                // For "function-like" callables, model the the behavior of `FunctionType.__get__`.
                //
                // It is a shortcut to model this in `try_call_dunder_get`. If we want to be really precise,
                // we should instead return a new method-wrapper type variant for the synthesized `__get__`
                // method of these synthesized functions. The method-wrapper would then be returned from
                // `find_name_in_mro` when called on function-like `Callable`s. This would allow us to
                // correctly model the behavior of *explicit* `SomeDataclass.__init__.__get__` calls.
                return if instance.is_none(db) {
                    Some((self, AttributeKind::NormalOrNonDataDescriptor))
                } else {
                    Some((
                        callable.bind_self(db),
                        AttributeKind::NormalOrNonDataDescriptor,
                    ))
                };
            }
            _ => {}
        }

        let descr_get = self.class_member(db, "__get__".into()).place;

        if let Place::Type(descr_get, descr_get_boundness) = descr_get {
            let return_ty = descr_get
                .try_call(db, &CallArguments::positional([self, instance, owner]))
                .map(|bindings| {
                    if descr_get_boundness == Boundness::Bound {
                        bindings.return_type(db)
                    } else {
                        UnionType::from_elements(db, [bindings.return_type(db), self])
                    }
                })
                .ok()?;

            let descriptor_kind = if self.is_data_descriptor(db) {
                AttributeKind::DataDescriptor
            } else {
                AttributeKind::NormalOrNonDataDescriptor
            };

            Some((return_ty, descriptor_kind))
        } else {
            None
        }
    }

    /// Look up `__get__` on the meta-type of `attribute`, and call it with `attribute`, `instance`,
    /// and `owner` as arguments. This method exists as a separate step as we need to handle unions
    /// and intersections explicitly.
    fn try_call_dunder_get_on_attribute(
        db: &'db dyn Db,
        attribute: PlaceAndQualifiers<'db>,
        instance: Type<'db>,
        owner: Type<'db>,
    ) -> (PlaceAndQualifiers<'db>, AttributeKind) {
        match attribute {
            // This branch is not strictly needed, but it short-circuits the lookup of various dunder
            // methods and calls that would otherwise be made.
            //
            // Note that attribute accesses on dynamic types always succeed. For this reason, they also
            // have `__get__`, `__set__`, and `__delete__` methods and are therefore considered to be
            // data descriptors.
            //
            // The same is true for `Never`.
            PlaceAndQualifiers {
                place: Place::Type(Type::Dynamic(_) | Type::Never, _),
                qualifiers: _,
            } => (attribute, AttributeKind::DataDescriptor),

            PlaceAndQualifiers {
                place: Place::Type(Type::Union(union), boundness),
                qualifiers,
            } => (
                union
                    .map_with_boundness(db, |elem| {
                        Place::Type(
                            elem.try_call_dunder_get(db, instance, owner)
                                .map_or(*elem, |(ty, _)| ty),
                            boundness,
                        )
                    })
                    .with_qualifiers(qualifiers),
                // TODO: avoid the duplication here:
                if union.elements(db).iter().all(|elem| {
                    elem.try_call_dunder_get(db, instance, owner)
                        .is_some_and(|(_, kind)| kind.is_data())
                }) {
                    AttributeKind::DataDescriptor
                } else {
                    AttributeKind::NormalOrNonDataDescriptor
                },
            ),

            PlaceAndQualifiers {
                place: Place::Type(Type::Intersection(intersection), boundness),
                qualifiers,
            } => (
                intersection
                    .map_with_boundness(db, |elem| {
                        Place::Type(
                            elem.try_call_dunder_get(db, instance, owner)
                                .map_or(*elem, |(ty, _)| ty),
                            boundness,
                        )
                    })
                    .with_qualifiers(qualifiers),
                // TODO: Discover data descriptors in intersections.
                AttributeKind::NormalOrNonDataDescriptor,
            ),

            PlaceAndQualifiers {
                place: Place::Type(attribute_ty, boundness),
                qualifiers: _,
            } => {
                if let Some((return_ty, attribute_kind)) =
                    attribute_ty.try_call_dunder_get(db, instance, owner)
                {
                    (Place::Type(return_ty, boundness).into(), attribute_kind)
                } else {
                    (attribute, AttributeKind::NormalOrNonDataDescriptor)
                }
            }

            _ => (attribute, AttributeKind::NormalOrNonDataDescriptor),
        }
    }

    /// Returns whether this type is a data descriptor, i.e. defines `__set__` or `__delete__`.
    /// If this type is a union, requires all elements of union to be data descriptors.
    pub(crate) fn is_data_descriptor(self, d: &'db dyn Db) -> bool {
        self.is_data_descriptor_impl(d, false)
    }

    /// Returns whether this type may be a data descriptor.
    /// If this type is a union, returns true if _any_ element is a data descriptor.
    pub(crate) fn may_be_data_descriptor(self, d: &'db dyn Db) -> bool {
        self.is_data_descriptor_impl(d, true)
    }

    fn is_data_descriptor_impl(self, db: &'db dyn Db, any_of_union: bool) -> bool {
        match self {
            Type::Dynamic(_) | Type::Never | Type::PropertyInstance(_) => true,
            Type::Union(union) if any_of_union => union
                .elements(db)
                .iter()
                // Types of instance attributes that are not explicitly typed are unioned with `Unknown`, it should be excluded when checking.
                .filter(|ty| !ty.is_unknown())
                .any(|ty| ty.is_data_descriptor_impl(db, any_of_union)),
            Type::Union(union) => union
                .elements(db)
                .iter()
                .all(|ty| ty.is_data_descriptor_impl(db, any_of_union)),
            Type::Intersection(intersection) => intersection
                .iter_positive(db)
                .any(|ty| ty.is_data_descriptor_impl(db, any_of_union)),
            _ => {
                !self.class_member(db, "__set__".into()).place.is_unbound()
                    || !self
                        .class_member(db, "__delete__".into())
                        .place
                        .is_unbound()
            }
        }
    }

    /// Implementation of the descriptor protocol.
    ///
    /// This method roughly performs the following steps:
    ///
    /// - Look up the attribute `name` on the meta-type of `self`. Call the result `meta_attr`.
    /// - Call `__get__` on the meta-type of `meta_attr`, if it exists. If the call succeeds,
    ///   replace `meta_attr` with the result of the call. Also check if `meta_attr` is a *data*
    ///   descriptor by testing if `__set__` or `__delete__` exist.
    /// - If `meta_attr` is a data descriptor, return it.
    /// - Otherwise, if `fallback` is bound, return `fallback`.
    /// - Otherwise, return `meta_attr`.
    ///
    /// In addition to that, we also handle various cases of possibly-unbound symbols and fall
    /// back to lower-precedence stages of the descriptor protocol by building union types.
    fn invoke_descriptor_protocol(
        self,
        db: &'db dyn Db,
        name: &str,
        fallback: PlaceAndQualifiers<'db>,
        policy: InstanceFallbackShadowsNonDataDescriptor,
        member_policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        let (
            PlaceAndQualifiers {
                place: meta_attr,
                qualifiers: meta_attr_qualifiers,
            },
            meta_attr_kind,
        ) = Self::try_call_dunder_get_on_attribute(
            db,
            self.class_member_with_policy(db, name.into(), member_policy),
            self,
            self.to_meta_type(db),
        );

        let PlaceAndQualifiers {
            place: fallback,
            qualifiers: fallback_qualifiers,
        } = fallback;

        match (meta_attr, meta_attr_kind, fallback) {
            // The fallback type is unbound, so we can just return `meta_attr` unconditionally,
            // no matter if it's data descriptor, a non-data descriptor, or a normal attribute.
            (meta_attr @ Place::Type(_, _), _, Place::Unbound) => {
                meta_attr.with_qualifiers(meta_attr_qualifiers)
            }

            // `meta_attr` is the return type of a data descriptor and definitely bound, so we
            // return it.
            (meta_attr @ Place::Type(_, Boundness::Bound), AttributeKind::DataDescriptor, _) => {
                meta_attr.with_qualifiers(meta_attr_qualifiers)
            }

            // `meta_attr` is the return type of a data descriptor, but the attribute on the
            // meta-type is possibly-unbound. This means that we "fall through" to the next
            // stage of the descriptor protocol and union with the fallback type.
            (
                Place::Type(meta_attr_ty, Boundness::PossiblyUnbound),
                AttributeKind::DataDescriptor,
                Place::Type(fallback_ty, fallback_boundness),
            ) => Place::Type(
                UnionType::from_elements(db, [meta_attr_ty, fallback_ty]),
                fallback_boundness,
            )
            .with_qualifiers(meta_attr_qualifiers.union(fallback_qualifiers)),

            // `meta_attr` is *not* a data descriptor. This means that the `fallback` type has
            // now the highest priority. However, we only return the pure `fallback` type if the
            // policy allows it. When invoked on class objects, the policy is set to `Yes`, which
            // means that class-level attributes (the fallback) can shadow non-data descriptors
            // on metaclasses. However, for instances, the policy is set to `No`, because we do
            // allow instance-level attributes to shadow class-level non-data descriptors. This
            // would require us to statically infer if an instance attribute is always set, which
            // is something we currently don't attempt to do.
            (
                Place::Type(_, _),
                AttributeKind::NormalOrNonDataDescriptor,
                fallback @ Place::Type(_, Boundness::Bound),
            ) if policy == InstanceFallbackShadowsNonDataDescriptor::Yes => {
                fallback.with_qualifiers(fallback_qualifiers)
            }

            // `meta_attr` is *not* a data descriptor. The `fallback` symbol is either possibly
            // unbound or the policy argument is `No`. In both cases, the `fallback` type does
            // not completely shadow the non-data descriptor, so we build a union of the two.
            (
                Place::Type(meta_attr_ty, meta_attr_boundness),
                AttributeKind::NormalOrNonDataDescriptor,
                Place::Type(fallback_ty, fallback_boundness),
            ) => Place::Type(
                UnionType::from_elements(db, [meta_attr_ty, fallback_ty]),
                meta_attr_boundness.max(fallback_boundness),
            )
            .with_qualifiers(meta_attr_qualifiers.union(fallback_qualifiers)),

            // If the attribute is not found on the meta-type, we simply return the fallback.
            (Place::Unbound, _, fallback) => fallback.with_qualifiers(fallback_qualifiers),
        }
    }

    /// Access an attribute of this type, potentially invoking the descriptor protocol.
    /// Corresponds to `getattr(<object of type 'self'>, name)`.
    ///
    /// See also: [`Type::static_member`]
    ///
    /// TODO: We should return a `Result` here to handle errors that can appear during attribute
    /// lookup, like a failed `__get__` call on a descriptor.
    #[must_use]
    pub(crate) fn member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        self.member_lookup_with_policy(db, name.into(), MemberLookupPolicy::default())
    }

    /// Similar to [`Type::member`], but allows the caller to specify what policy should be used
    /// when looking up attributes. See [`MemberLookupPolicy`] for more information.
    #[salsa::tracked(cycle_fn=member_lookup_cycle_recover, cycle_initial=member_lookup_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    fn member_lookup_with_policy(
        self,
        db: &'db dyn Db,
        name: Name,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        tracing::trace!("member_lookup_with_policy: {}.{}", self.display(db), name);
        if name == "__class__" {
            return Place::bound(self.dunder_class(db)).into();
        }

        let name_str = name.as_str();

        match self {
            Type::Union(union) => union
                .map_with_boundness(db, |elem| {
                    elem.member_lookup_with_policy(db, name_str.into(), policy)
                        .place
                })
                .into(),

            Type::Intersection(intersection) => intersection
                .map_with_boundness(db, |elem| {
                    elem.member_lookup_with_policy(db, name_str.into(), policy)
                        .place
                })
                .into(),

            Type::Dynamic(..) | Type::Never => Place::bound(self).into(),

            Type::FunctionLiteral(function) if name == "__get__" => Place::bound(
                Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderGet(function)),
            )
            .into(),
            Type::FunctionLiteral(function) if name == "__call__" => Place::bound(
                Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderCall(function)),
            )
            .into(),
            Type::PropertyInstance(property) if name == "__get__" => Place::bound(
                Type::MethodWrapper(MethodWrapperKind::PropertyDunderGet(property)),
            )
            .into(),
            Type::PropertyInstance(property) if name == "__set__" => Place::bound(
                Type::MethodWrapper(MethodWrapperKind::PropertyDunderSet(property)),
            )
            .into(),
            Type::StringLiteral(literal) if name == "startswith" => Place::bound(
                Type::MethodWrapper(MethodWrapperKind::StrStartswith(literal)),
            )
            .into(),

            Type::ClassLiteral(class)
                if name == "__get__" && class.is_known(db, KnownClass::FunctionType) =>
            {
                Place::bound(Type::WrapperDescriptor(
                    WrapperDescriptorKind::FunctionTypeDunderGet,
                ))
                .into()
            }
            Type::ClassLiteral(class)
                if name == "__get__" && class.is_known(db, KnownClass::Property) =>
            {
                Place::bound(Type::WrapperDescriptor(
                    WrapperDescriptorKind::PropertyDunderGet,
                ))
                .into()
            }
            Type::ClassLiteral(class)
                if name == "__set__" && class.is_known(db, KnownClass::Property) =>
            {
                Place::bound(Type::WrapperDescriptor(
                    WrapperDescriptorKind::PropertyDunderSet,
                ))
                .into()
            }
            Type::BoundMethod(bound_method) => match name_str {
                "__self__" => Place::bound(bound_method.self_instance(db)).into(),
                "__func__" => Place::bound(Type::FunctionLiteral(bound_method.function(db))).into(),
                _ => {
                    KnownClass::MethodType
                        .to_instance(db)
                        .member_lookup_with_policy(db, name.clone(), policy)
                        .or_fall_back_to(db, || {
                            // If an attribute is not available on the bound method object,
                            // it will be looked up on the underlying function object:
                            Type::FunctionLiteral(bound_method.function(db))
                                .member_lookup_with_policy(db, name, policy)
                        })
                }
            },
            Type::MethodWrapper(_) => KnownClass::MethodWrapperType
                .to_instance(db)
                .member_lookup_with_policy(db, name, policy),
            Type::WrapperDescriptor(_) => KnownClass::WrapperDescriptorType
                .to_instance(db)
                .member_lookup_with_policy(db, name, policy),
            Type::DataclassDecorator(_) => KnownClass::FunctionType
                .to_instance(db)
                .member_lookup_with_policy(db, name, policy),

            Type::Callable(_) | Type::DataclassTransformer(_) if name_str == "__call__" => {
                Place::bound(self).into()
            }

            Type::Callable(callable) if callable.is_function_like(db) => KnownClass::FunctionType
                .to_instance(db)
                .member_lookup_with_policy(db, name, policy),

            Type::Callable(_) | Type::DataclassTransformer(_) => {
                Type::object(db).member_lookup_with_policy(db, name, policy)
            }

            Type::NominalInstance(instance)
                if matches!(name.as_str(), "major" | "minor")
                    && instance.class(db).is_known(db, KnownClass::VersionInfo) =>
            {
                let python_version = Program::get(db).python_version(db);
                let segment = if name == "major" {
                    python_version.major
                } else {
                    python_version.minor
                };
                Place::bound(Type::IntLiteral(segment.into())).into()
            }

            Type::PropertyInstance(property) if name == "fget" => {
                Place::bound(property.getter(db).unwrap_or(Type::none(db))).into()
            }
            Type::PropertyInstance(property) if name == "fset" => {
                Place::bound(property.setter(db).unwrap_or(Type::none(db))).into()
            }

            Type::IntLiteral(_) if matches!(name_str, "real" | "numerator") => {
                Place::bound(self).into()
            }

            Type::BooleanLiteral(bool_value) if matches!(name_str, "real" | "numerator") => {
                Place::bound(Type::IntLiteral(i64::from(bool_value))).into()
            }

            Type::ModuleLiteral(module) => module.static_member(db, name_str),

            _ if policy.no_instance_fallback() => self.invoke_descriptor_protocol(
                db,
                name_str,
                Place::Unbound.into(),
                InstanceFallbackShadowsNonDataDescriptor::No,
                policy,
            ),

            Type::NominalInstance(..)
            | Type::ProtocolInstance(..)
            | Type::BooleanLiteral(..)
            | Type::IntLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::EnumLiteral(..)
            | Type::LiteralString
            | Type::TypeVar(..)
            | Type::SpecialForm(..)
            | Type::KnownInstance(..)
            | Type::PropertyInstance(..)
            | Type::FunctionLiteral(..)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(..)
            | Type::TypedDict(_) => {
                let fallback = self.instance_member(db, name_str);

                let result = self.invoke_descriptor_protocol(
                    db,
                    name_str,
                    fallback,
                    InstanceFallbackShadowsNonDataDescriptor::No,
                    policy,
                );

                let custom_getattr_result = || {
                    // Typeshed has a fake `__getattr__` on `types.ModuleType` to help out with
                    // dynamic imports. We explicitly hide it here to prevent arbitrary attributes
                    // from being available on modules. Same for `types.GenericAlias` - its
                    // `__getattr__` method will delegate to `__origin__` to allow looking up
                    // attributes on the original type. But in typeshed its return type is `Any`.
                    // It will need a special handling, so it remember the origin type to properly
                    // resolve the attribute.
                    if matches!(
                        self.into_nominal_instance()
                            .and_then(|instance| instance.class(db).known(db)),
                        Some(KnownClass::ModuleType | KnownClass::GenericAlias)
                    ) {
                        return Place::Unbound.into();
                    }

                    self.try_call_dunder(
                        db,
                        "__getattr__",
                        CallArguments::positional([Type::string_literal(db, &name)]),
                    )
                    .map(|outcome| Place::bound(outcome.return_type(db)))
                    // TODO: Handle call errors here.
                    .unwrap_or(Place::Unbound)
                    .into()
                };

                let custom_getattribute_result = || {
                    // Avoid cycles when looking up `__getattribute__`
                    if "__getattribute__" == name.as_str() {
                        return Place::Unbound.into();
                    }

                    // Typeshed has a `__getattribute__` method defined on `builtins.object` so we
                    // explicitly hide it here using `MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK`.
                    self.try_call_dunder_with_policy(
                        db,
                        "__getattribute__",
                        &mut CallArguments::positional([Type::string_literal(db, &name)]),
                        MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                    )
                    .map(|outcome| Place::bound(outcome.return_type(db)))
                    // TODO: Handle call errors here.
                    .unwrap_or(Place::Unbound)
                    .into()
                };

                if result.is_class_var() && self.is_typed_dict() {
                    // `ClassVar`s on `TypedDictFallback` can not be accessed on inhabitants of `SomeTypedDict`.
                    // They can only be accessed on `SomeTypedDict` directly.
                    return Place::Unbound.into();
                }

                match result {
                    member @ PlaceAndQualifiers {
                        place: Place::Type(_, Boundness::Bound),
                        qualifiers: _,
                    } => member,
                    member @ PlaceAndQualifiers {
                        place: Place::Type(_, Boundness::PossiblyUnbound),
                        qualifiers: _,
                    } => member
                        .or_fall_back_to(db, custom_getattribute_result)
                        .or_fall_back_to(db, custom_getattr_result),
                    PlaceAndQualifiers {
                        place: Place::Unbound,
                        qualifiers: _,
                    } => custom_getattribute_result().or_fall_back_to(db, custom_getattr_result),
                }
            }

            Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..) => {
                let class_attr_plain = self.find_name_in_mro_with_policy(db, name_str,policy).expect(
                    "Calling `find_name_in_mro` on class literals and subclass-of types should always return `Some`",
                );

                if name == "__mro__" {
                    return class_attr_plain;
                }

                if let Some(enum_class) = match self {
                    Type::ClassLiteral(literal) => Some(literal),
                    Type::SubclassOf(subclass_of) => subclass_of
                        .subclass_of()
                        .into_class()
                        .map(|class| class.class_literal(db).0),
                    _ => None,
                } {
                    if let Some(metadata) = enum_metadata(db, enum_class) {
                        if let Some(resolved_name) = metadata.resolve_member(&name) {
                            return Place::Type(
                                Type::EnumLiteral(EnumLiteralType::new(
                                    db,
                                    enum_class,
                                    resolved_name,
                                )),
                                Boundness::Bound,
                            )
                            .into();
                        }
                    }
                }

                let class_attr_fallback = Self::try_call_dunder_get_on_attribute(
                    db,
                    class_attr_plain,
                    Type::none(db),
                    self,
                )
                .0;

                self.invoke_descriptor_protocol(
                    db,
                    name_str,
                    class_attr_fallback,
                    InstanceFallbackShadowsNonDataDescriptor::Yes,
                    policy,
                )
            }

            // Unlike other objects, `super` has a unique member lookup behavior.
            // It's simpler than other objects:
            //
            // 1. Search for the attribute in the MRO, starting just after the pivot class.
            // 2. If the attribute is a descriptor, invoke its `__get__` method.
            Type::BoundSuper(bound_super) => {
                let owner_attr = bound_super.find_name_in_mro_after_pivot(db, name_str, policy);

                bound_super
                    .try_call_dunder_get_on_attribute(db, owner_attr.clone())
                    .unwrap_or(owner_attr)
            }
        }
    }

    /// Resolves the boolean value of the type and falls back to [`Truthiness::Ambiguous`] if the type doesn't implement `__bool__` correctly.
    ///
    /// This method should only be used outside type checking or when evaluating if a type
    /// is truthy or falsy in a context where Python doesn't make an implicit `bool` call.
    /// Use [`try_bool`](Self::try_bool) for type checking or implicit `bool` calls.
    pub(crate) fn bool(&self, db: &'db dyn Db) -> Truthiness {
        self.try_bool_impl(db, true)
            .unwrap_or_else(|err| err.fallback_truthiness())
    }

    /// Resolves the boolean value of a type.
    ///
    /// This is used to determine the value that would be returned
    /// when `bool(x)` is called on an object `x`.
    ///
    /// Returns an error if the type doesn't implement `__bool__` correctly.
    pub(crate) fn try_bool(&self, db: &'db dyn Db) -> Result<Truthiness, BoolError<'db>> {
        self.try_bool_impl(db, false)
    }

    /// Resolves the boolean value of a type.
    ///
    /// Setting `allow_short_circuit` to `true` allows the implementation to
    /// early return if the bool value of any union variant is `Truthiness::Ambiguous`.
    /// Early returning shows a 1-2% perf improvement on our benchmarks because
    /// `bool` (which doesn't care about errors) is used heavily when evaluating statically known branches.
    ///
    /// An alternative to this flag is to implement a trait similar to Rust's `Try` trait.
    /// The advantage of that is that it would allow collecting the errors as well. However,
    /// it is significantly more complex and duplicating the logic into `bool` without the error
    /// handling didn't show any significant performance difference to when using the `allow_short_circuit` flag.
    #[inline]
    fn try_bool_impl(
        &self,
        db: &'db dyn Db,
        allow_short_circuit: bool,
    ) -> Result<Truthiness, BoolError<'db>> {
        let type_to_truthiness = |ty| {
            if let Type::BooleanLiteral(bool_val) = ty {
                Truthiness::from(bool_val)
            } else {
                Truthiness::Ambiguous
            }
        };

        let try_dunder_bool = || {
            // We only check the `__bool__` method for truth testing, even though at
            // runtime there is a fallback to `__len__`, since `__bool__` takes precedence
            // and a subclass could add a `__bool__` method.

            match self.try_call_dunder(db, "__bool__", CallArguments::none()) {
                Ok(outcome) => {
                    let return_type = outcome.return_type(db);
                    if !return_type.is_assignable_to(db, KnownClass::Bool.to_instance(db)) {
                        // The type has a `__bool__` method, but it doesn't return a
                        // boolean.
                        return Err(BoolError::IncorrectReturnType {
                            return_type,
                            not_boolable_type: *self,
                        });
                    }
                    Ok(type_to_truthiness(return_type))
                }

                Err(CallDunderError::PossiblyUnbound(outcome)) => {
                    let return_type = outcome.return_type(db);
                    if !return_type.is_assignable_to(db, KnownClass::Bool.to_instance(db)) {
                        // The type has a `__bool__` method, but it doesn't return a
                        // boolean.
                        return Err(BoolError::IncorrectReturnType {
                            return_type: outcome.return_type(db),
                            not_boolable_type: *self,
                        });
                    }

                    // Don't trust possibly unbound `__bool__` method.
                    Ok(Truthiness::Ambiguous)
                }

                Err(CallDunderError::MethodNotAvailable) => Ok(Truthiness::Ambiguous),
                Err(CallDunderError::CallError(CallErrorKind::BindingError, bindings)) => {
                    Err(BoolError::IncorrectArguments {
                        truthiness: type_to_truthiness(bindings.return_type(db)),
                        not_boolable_type: *self,
                    })
                }
                Err(CallDunderError::CallError(CallErrorKind::NotCallable, _)) => {
                    Err(BoolError::NotCallable {
                        not_boolable_type: *self,
                    })
                }
                Err(CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, _)) => {
                    Err(BoolError::Other {
                        not_boolable_type: *self,
                    })
                }
            }
        };

        let try_union = |union: UnionType<'db>| {
            let mut truthiness = None;
            let mut all_not_callable = true;
            let mut has_errors = false;

            for element in union.elements(db) {
                let element_truthiness = match element.try_bool_impl(db, allow_short_circuit) {
                    Ok(truthiness) => truthiness,
                    Err(err) => {
                        has_errors = true;
                        all_not_callable &= matches!(err, BoolError::NotCallable { .. });
                        err.fallback_truthiness()
                    }
                };

                truthiness.get_or_insert(element_truthiness);

                if Some(element_truthiness) != truthiness {
                    truthiness = Some(Truthiness::Ambiguous);

                    if allow_short_circuit {
                        return Ok(Truthiness::Ambiguous);
                    }
                }
            }

            if has_errors {
                if all_not_callable {
                    return Err(BoolError::NotCallable {
                        not_boolable_type: *self,
                    });
                }
                return Err(BoolError::Union {
                    union,
                    truthiness: truthiness.unwrap_or(Truthiness::Ambiguous),
                });
            }
            Ok(truthiness.unwrap_or(Truthiness::Ambiguous))
        };

        let truthiness = match self {
            Type::Dynamic(_)
            | Type::Never
            | Type::Callable(_)
            | Type::LiteralString
            | Type::TypeIs(_) => Truthiness::Ambiguous,

            Type::TypedDict(_) => {
                // TODO: We could do better here, but it's unclear if this is important.
                // See existing `TypedDict`-related tests in `truthiness.md`
                Truthiness::Ambiguous
            }

            Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::PropertyInstance(_)
            | Type::BoundSuper(_)
            | Type::KnownInstance(_)
            | Type::SpecialForm(_)
            | Type::AlwaysTruthy => Truthiness::AlwaysTrue,

            Type::AlwaysFalsy => Truthiness::AlwaysFalse,

            Type::ClassLiteral(class) => class
                .metaclass_instance_type(db)
                .try_bool_impl(db, allow_short_circuit)?,
            Type::GenericAlias(alias) => ClassType::from(*alias)
                .metaclass_instance_type(db)
                .try_bool_impl(db, allow_short_circuit)?,

            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Dynamic(_) => Truthiness::Ambiguous,
                SubclassOfInner::Class(class) => {
                    Type::from(class).try_bool_impl(db, allow_short_circuit)?
                }
            },

            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => Truthiness::Ambiguous,
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.try_bool_impl(db, allow_short_circuit)?
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        try_union(constraints)?
                    }
                }
            }

            Type::NominalInstance(instance) => instance
                .class(db)
                .known(db)
                .and_then(KnownClass::bool)
                .map(Ok)
                .unwrap_or_else(try_dunder_bool)?,

            Type::ProtocolInstance(_) => try_dunder_bool()?,

            Type::Union(union) => try_union(*union)?,

            Type::Intersection(_) => {
                // TODO
                Truthiness::Ambiguous
            }

            Type::EnumLiteral(_) => {
                // We currently make no attempt to infer the precise truthiness, but it's not impossible to do so.
                // Note that custom `__bool__` or `__len__` methods on the class or superclasses affect the outcome.
                Truthiness::Ambiguous
            }

            Type::IntLiteral(num) => Truthiness::from(*num != 0),
            Type::BooleanLiteral(bool) => Truthiness::from(*bool),
            Type::StringLiteral(str) => Truthiness::from(!str.value(db).is_empty()),
            Type::BytesLiteral(bytes) => Truthiness::from(!bytes.value(db).is_empty()),
        };

        Ok(truthiness)
    }

    /// Return the type of `len()` on a type if it is known more precisely than `int`,
    /// or `None` otherwise.
    ///
    /// In the second case, the return type of `len()` in `typeshed` (`int`)
    /// is used as a fallback.
    fn len(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        fn non_negative_int_literal<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Type<'db>> {
            match ty {
                // TODO: Emit diagnostic for non-integers and negative integers
                Type::IntLiteral(value) => (value >= 0).then_some(ty),
                Type::BooleanLiteral(value) => Some(Type::IntLiteral(value.into())),
                Type::Union(union) => {
                    union.try_map(db, |element| non_negative_int_literal(db, *element))
                }
                _ => None,
            }
        }

        let usize_len = match self {
            Type::BytesLiteral(bytes) => Some(bytes.python_len(db)),
            Type::StringLiteral(string) => Some(string.python_len(db)),
            _ => None,
        };

        if let Some(usize_len) = usize_len {
            return usize_len.try_into().ok().map(Type::IntLiteral);
        }

        let return_ty = match self.try_call_dunder(db, "__len__", CallArguments::none()) {
            Ok(bindings) => bindings.return_type(db),
            Err(CallDunderError::PossiblyUnbound(bindings)) => bindings.return_type(db),

            // TODO: emit a diagnostic
            Err(CallDunderError::MethodNotAvailable) => return None,
            Err(CallDunderError::CallError(_, bindings)) => bindings.return_type(db),
        };

        non_negative_int_literal(db, return_ty)
    }

    /// Returns a [`Bindings`] that can be used to analyze a call to this type. You must call
    /// [`match_parameters`][Bindings::match_parameters] and [`check_types`][Bindings::check_types]
    /// to fully analyze a particular call site.
    ///
    /// Note that we return a [`Bindings`] for all types, even if the type is not callable.
    /// "Callable" can be subtle for a union type, since some union elements might be callable and
    /// some not. A union is callable if every element type is callable — but even then, the
    /// elements might be inconsistent, such that there's no argument list that's valid for all
    /// elements. It's usually best to only worry about "callability" relative to a particular
    /// argument list, via [`try_call`][Self::try_call] and [`CallErrorKind::NotCallable`].
    fn bindings(self, db: &'db dyn Db) -> Bindings<'db> {
        match self {
            Type::Callable(callable) => {
                CallableBinding::from_overloads(self, callable.signatures(db).iter().cloned())
                    .into()
            }

            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => CallableBinding::not_callable(self).into(),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound.bindings(db),
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        Bindings::from_union(
                            self,
                            constraints.elements(db).iter().map(|ty| ty.bindings(db)),
                        )
                    }
                }
            }

            Type::BoundMethod(bound_method) => {
                let signature = bound_method.function(db).signature(db);
                CallableBinding::from_overloads(self, signature.overloads.iter().cloned())
                    .with_bound_type(bound_method.self_instance(db))
                    .into()
            }

            Type::MethodWrapper(
                MethodWrapperKind::FunctionTypeDunderGet(_)
                | MethodWrapperKind::PropertyDunderGet(_),
            ) => {
                // Here, we dynamically model the overloaded function signature of `types.FunctionType.__get__`.
                // This is required because we need to return more precise types than what the signature in
                // typeshed provides:
                //
                // ```py
                // class FunctionType:
                //     # ...
                //     @overload
                //     def __get__(self, instance: None, owner: type, /) -> FunctionType: ...
                //     @overload
                //     def __get__(self, instance: object, owner: type | None = None, /) -> MethodType: ...
                // ```
                //
                // For `builtins.property.__get__`, we use the same signature. The return types are not
                // specified yet, they will be dynamically added in `Bindings::evaluate_known_cases`.

                CallableBinding::from_overloads(
                    self,
                    [
                        Signature::new(
                            Parameters::new([
                                Parameter::positional_only(Some(Name::new_static("instance")))
                                    .with_annotated_type(Type::none(db)),
                                Parameter::positional_only(Some(Name::new_static("owner")))
                                    .with_annotated_type(KnownClass::Type.to_instance(db)),
                            ]),
                            None,
                        ),
                        Signature::new(
                            Parameters::new([
                                Parameter::positional_only(Some(Name::new_static("instance")))
                                    .with_annotated_type(Type::object(db)),
                                Parameter::positional_only(Some(Name::new_static("owner")))
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [KnownClass::Type.to_instance(db), Type::none(db)],
                                    ))
                                    .with_default_type(Type::none(db)),
                            ]),
                            None,
                        ),
                    ],
                )
                .into()
            }

            Type::WrapperDescriptor(
                kind @ (WrapperDescriptorKind::FunctionTypeDunderGet
                | WrapperDescriptorKind::PropertyDunderGet),
            ) => {
                // Here, we also model `types.FunctionType.__get__` (or builtins.property.__get__),
                // but now we consider a call to this as a function, i.e. we also expect the `self`
                // argument to be passed in.

                // TODO: Consider merging this signature with the one in the previous match clause,
                // since the previous one is just this signature with the `self` parameters
                // removed.
                let descriptor = match kind {
                    WrapperDescriptorKind::FunctionTypeDunderGet => {
                        KnownClass::FunctionType.to_instance(db)
                    }
                    WrapperDescriptorKind::PropertyDunderGet => {
                        KnownClass::Property.to_instance(db)
                    }
                    WrapperDescriptorKind::PropertyDunderSet => {
                        unreachable!("Not part of outer match pattern")
                    }
                };
                CallableBinding::from_overloads(
                    self,
                    [
                        Signature::new(
                            Parameters::new([
                                Parameter::positional_only(Some(Name::new_static("self")))
                                    .with_annotated_type(descriptor),
                                Parameter::positional_only(Some(Name::new_static("instance")))
                                    .with_annotated_type(Type::none(db)),
                                Parameter::positional_only(Some(Name::new_static("owner")))
                                    .with_annotated_type(KnownClass::Type.to_instance(db)),
                            ]),
                            None,
                        ),
                        Signature::new(
                            Parameters::new([
                                Parameter::positional_only(Some(Name::new_static("self")))
                                    .with_annotated_type(descriptor),
                                Parameter::positional_only(Some(Name::new_static("instance")))
                                    .with_annotated_type(Type::object(db)),
                                Parameter::positional_only(Some(Name::new_static("owner")))
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [KnownClass::Type.to_instance(db), Type::none(db)],
                                    ))
                                    .with_default_type(Type::none(db)),
                            ]),
                            None,
                        ),
                    ],
                )
                .into()
            }

            Type::MethodWrapper(MethodWrapperKind::PropertyDunderSet(_)) => Binding::single(
                self,
                Signature::new(
                    Parameters::new([
                        Parameter::positional_only(Some(Name::new_static("instance")))
                            .with_annotated_type(Type::object(db)),
                        Parameter::positional_only(Some(Name::new_static("value")))
                            .with_annotated_type(Type::object(db)),
                    ]),
                    None,
                ),
            )
            .into(),

            Type::WrapperDescriptor(WrapperDescriptorKind::PropertyDunderSet) => Binding::single(
                self,
                Signature::new(
                    Parameters::new([
                        Parameter::positional_only(Some(Name::new_static("self")))
                            .with_annotated_type(KnownClass::Property.to_instance(db)),
                        Parameter::positional_only(Some(Name::new_static("instance")))
                            .with_annotated_type(Type::object(db)),
                        Parameter::positional_only(Some(Name::new_static("value")))
                            .with_annotated_type(Type::object(db)),
                    ]),
                    None,
                ),
            )
            .into(),

            Type::MethodWrapper(MethodWrapperKind::StrStartswith(_)) => Binding::single(
                self,
                Signature::new(
                    Parameters::new([
                        Parameter::positional_only(Some(Name::new_static("prefix")))
                            .with_annotated_type(UnionType::from_elements(
                                db,
                                [
                                    KnownClass::Str.to_instance(db),
                                    Type::homogeneous_tuple(db, KnownClass::Str.to_instance(db)),
                                ],
                            )),
                        Parameter::positional_only(Some(Name::new_static("start")))
                            .with_annotated_type(UnionType::from_elements(
                                db,
                                [KnownClass::SupportsIndex.to_instance(db), Type::none(db)],
                            ))
                            .with_default_type(Type::none(db)),
                        Parameter::positional_only(Some(Name::new_static("end")))
                            .with_annotated_type(UnionType::from_elements(
                                db,
                                [KnownClass::SupportsIndex.to_instance(db), Type::none(db)],
                            ))
                            .with_default_type(Type::none(db)),
                    ]),
                    Some(KnownClass::Bool.to_instance(db)),
                ),
            )
            .into(),

            // TODO: We should probably also check the original return type of the function
            // that was decorated with `@dataclass_transform`, to see if it is consistent with
            // with what we configure here.
            Type::DataclassTransformer(_) => Binding::single(
                self,
                Signature::new(
                    Parameters::new([Parameter::positional_only(Some(Name::new_static("func")))
                        .with_annotated_type(Type::object(db))]),
                    None,
                ),
            )
            .into(),

            Type::FunctionLiteral(function_type) => match function_type.known(db) {
                Some(
                    KnownFunction::IsEquivalentTo
                    | KnownFunction::IsSubtypeOf
                    | KnownFunction::IsAssignableTo
                    | KnownFunction::IsDisjointFrom,
                ) => Binding::single(
                    self,
                    Signature::new(
                        Parameters::new([
                            Parameter::positional_only(Some(Name::new_static("a")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                            Parameter::positional_only(Some(Name::new_static("b")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                        ]),
                        Some(KnownClass::Bool.to_instance(db)),
                    ),
                )
                .into(),

                Some(KnownFunction::IsSingleton | KnownFunction::IsSingleValued) => {
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new([Parameter::positional_only(Some(Name::new_static(
                                "a",
                            )))
                            .type_form()
                            .with_annotated_type(Type::any())]),
                            Some(KnownClass::Bool.to_instance(db)),
                        ),
                    )
                    .into()
                }

                Some(KnownFunction::TopMaterialization | KnownFunction::BottomMaterialization) => {
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new([Parameter::positional_only(Some(Name::new_static(
                                "type",
                            )))
                            .type_form()
                            .with_annotated_type(Type::any())]),
                            Some(Type::any()),
                        ),
                    )
                    .into()
                }

                Some(KnownFunction::AssertType) => Binding::single(
                    self,
                    Signature::new(
                        Parameters::new([
                            Parameter::positional_only(Some(Name::new_static("value")))
                                .with_annotated_type(Type::any()),
                            Parameter::positional_only(Some(Name::new_static("type")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                        ]),
                        Some(Type::none(db)),
                    ),
                )
                .into(),

                Some(KnownFunction::AssertNever) => {
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new([Parameter::positional_only(Some(Name::new_static(
                                "arg",
                            )))
                            // We need to set the type to `Any` here (instead of `Never`),
                            // in order for every `assert_never` call to pass the argument
                            // check. If we set it to `Never`, we'll get invalid-argument-type
                            // errors instead of `type-assertion-failure` errors.
                            .with_annotated_type(Type::any())]),
                            Some(Type::none(db)),
                        ),
                    )
                    .into()
                }

                Some(KnownFunction::Cast) => Binding::single(
                    self,
                    Signature::new(
                        Parameters::new([
                            Parameter::positional_or_keyword(Name::new_static("typ"))
                                .type_form()
                                .with_annotated_type(Type::any()),
                            Parameter::positional_or_keyword(Name::new_static("val"))
                                .with_annotated_type(Type::any()),
                        ]),
                        Some(Type::any()),
                    ),
                )
                .into(),

                Some(KnownFunction::Dataclass) => {
                    CallableBinding::from_overloads(
                        self,
                        [
                            // def dataclass(cls: None, /) -> Callable[[type[_T]], type[_T]]: ...
                            Signature::new(
                                Parameters::new([Parameter::positional_only(Some(
                                    Name::new_static("cls"),
                                ))
                                .with_annotated_type(Type::none(db))]),
                                None,
                            ),
                            // def dataclass(cls: type[_T], /) -> type[_T]: ...
                            Signature::new(
                                Parameters::new([Parameter::positional_only(Some(
                                    Name::new_static("cls"),
                                ))
                                .with_annotated_type(KnownClass::Type.to_instance(db))]),
                                None,
                            ),
                            // TODO: make this overload Python-version-dependent

                            // def dataclass(
                            //     *,
                            //     init: bool = True,
                            //     repr: bool = True,
                            //     eq: bool = True,
                            //     order: bool = False,
                            //     unsafe_hash: bool = False,
                            //     frozen: bool = False,
                            //     match_args: bool = True,
                            //     kw_only: bool = False,
                            //     slots: bool = False,
                            //     weakref_slot: bool = False,
                            // ) -> Callable[[type[_T]], type[_T]]: ...
                            Signature::new(
                                Parameters::new([
                                    Parameter::keyword_only(Name::new_static("init"))
                                        .with_annotated_type(KnownClass::Bool.to_instance(db))
                                        .with_default_type(Type::BooleanLiteral(true)),
                                    Parameter::keyword_only(Name::new_static("repr"))
                                        .with_annotated_type(KnownClass::Bool.to_instance(db))
                                        .with_default_type(Type::BooleanLiteral(true)),
                                    Parameter::keyword_only(Name::new_static("eq"))
                                        .with_annotated_type(KnownClass::Bool.to_instance(db))
                                        .with_default_type(Type::BooleanLiteral(true)),
                                    Parameter::keyword_only(Name::new_static("order"))
                                        .with_annotated_type(KnownClass::Bool.to_instance(db))
                                        .with_default_type(Type::BooleanLiteral(false)),
                                    Parameter::keyword_only(Name::new_static("unsafe_hash"))
                                        .with_annotated_type(KnownClass::Bool.to_instance(db))
                                        .with_default_type(Type::BooleanLiteral(false)),
                                    Parameter::keyword_only(Name::new_static("frozen"))
                                        .with_annotated_type(KnownClass::Bool.to_instance(db))
                                        .with_default_type(Type::BooleanLiteral(false)),
                                    Parameter::keyword_only(Name::new_static("match_args"))
                                        .with_annotated_type(KnownClass::Bool.to_instance(db))
                                        .with_default_type(Type::BooleanLiteral(true)),
                                    Parameter::keyword_only(Name::new_static("kw_only"))
                                        .with_annotated_type(KnownClass::Bool.to_instance(db))
                                        .with_default_type(Type::BooleanLiteral(false)),
                                    Parameter::keyword_only(Name::new_static("slots"))
                                        .with_annotated_type(KnownClass::Bool.to_instance(db))
                                        .with_default_type(Type::BooleanLiteral(false)),
                                    Parameter::keyword_only(Name::new_static("weakref_slot"))
                                        .with_annotated_type(KnownClass::Bool.to_instance(db))
                                        .with_default_type(Type::BooleanLiteral(false)),
                                ]),
                                None,
                            ),
                        ],
                    )
                    .into()
                }

                _ => CallableBinding::from_overloads(
                    self,
                    function_type.signature(db).overloads.iter().cloned(),
                )
                .into(),
            },

            Type::ClassLiteral(class) => match class.known(db) {
                // TODO: Ideally we'd use `try_call_constructor` for all constructor calls.
                // Currently we don't for a few special known types, either because their
                // constructors are defined with overloads, or because we want to special case
                // their return type beyond what typeshed provides (though this support could
                // likely be moved into the `try_call_constructor` path). Once we support
                // overloads, re-evaluate the need for these arms.
                Some(KnownClass::Bool) => {
                    // ```py
                    // class bool(int):
                    //     def __new__(cls, o: object = ..., /) -> Self: ...
                    // ```
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new([Parameter::positional_only(Some(Name::new_static(
                                "o",
                            )))
                            .with_annotated_type(Type::any())
                            .with_default_type(Type::BooleanLiteral(false))]),
                            Some(KnownClass::Bool.to_instance(db)),
                        ),
                    )
                    .into()
                }

                Some(KnownClass::Str) => {
                    // ```py
                    // class str(Sequence[str]):
                    //     @overload
                    //     def __new__(cls, object: object = ...) -> Self: ...
                    //     @overload
                    //     def __new__(cls, object: ReadableBuffer, encoding: str = ..., errors: str = ...) -> Self: ...
                    // ```
                    CallableBinding::from_overloads(
                        self,
                        [
                            Signature::new(
                                Parameters::new([Parameter::positional_or_keyword(
                                    Name::new_static("object"),
                                )
                                .with_annotated_type(Type::object(db))
                                .with_default_type(Type::string_literal(db, ""))]),
                                Some(KnownClass::Str.to_instance(db)),
                            ),
                            Signature::new(
                                Parameters::new([
                                    Parameter::positional_or_keyword(Name::new_static("object"))
                                        // TODO: Should be `ReadableBuffer` instead of this union type:
                                        .with_annotated_type(UnionType::from_elements(
                                            db,
                                            [
                                                KnownClass::Bytes.to_instance(db),
                                                KnownClass::Bytearray.to_instance(db),
                                            ],
                                        ))
                                        .with_default_type(Type::bytes_literal(db, b"")),
                                    Parameter::positional_or_keyword(Name::new_static("encoding"))
                                        .with_annotated_type(KnownClass::Str.to_instance(db))
                                        .with_default_type(Type::string_literal(db, "utf-8")),
                                    Parameter::positional_or_keyword(Name::new_static("errors"))
                                        .with_annotated_type(KnownClass::Str.to_instance(db))
                                        .with_default_type(Type::string_literal(db, "strict")),
                                ]),
                                Some(KnownClass::Str.to_instance(db)),
                            ),
                        ],
                    )
                    .into()
                }

                Some(KnownClass::Type) => {
                    let str_instance = KnownClass::Str.to_instance(db);
                    let type_instance = KnownClass::Type.to_instance(db);

                    // ```py
                    // class type:
                    //     @overload
                    //     def __init__(self, o: object, /) -> None: ...
                    //     @overload
                    //     def __init__(self, name: str, bases: tuple[type, ...], dict: dict[str, Any], /, **kwds: Any) -> None: ...
                    // ```
                    CallableBinding::from_overloads(
                        self,
                        [
                            Signature::new(
                                Parameters::new([Parameter::positional_only(Some(
                                    Name::new_static("o"),
                                ))
                                .with_annotated_type(Type::any())]),
                                Some(type_instance),
                            ),
                            Signature::new(
                                Parameters::new([
                                    Parameter::positional_only(Some(Name::new_static("name")))
                                        .with_annotated_type(str_instance),
                                    Parameter::positional_only(Some(Name::new_static("bases")))
                                        .with_annotated_type(Type::homogeneous_tuple(
                                            db,
                                            type_instance,
                                        )),
                                    Parameter::positional_only(Some(Name::new_static("dict")))
                                        .with_annotated_type(
                                            KnownClass::Dict.to_specialized_instance(
                                                db,
                                                [str_instance, Type::any()],
                                            ),
                                        ),
                                ]),
                                Some(type_instance),
                            ),
                        ],
                    )
                    .into()
                }

                Some(KnownClass::NamedTuple) => {
                    Binding::single(self, Signature::todo("functional `NamedTuple` syntax")).into()
                }

                Some(KnownClass::Object) => {
                    // ```py
                    // class object:
                    //    def __init__(self) -> None: ...
                    //    def __new__(cls) -> Self: ...
                    // ```
                    Binding::single(
                        self,
                        Signature::new(Parameters::empty(), Some(Type::object(db))),
                    )
                    .into()
                }

                Some(KnownClass::Enum) => {
                    Binding::single(self, Signature::todo("functional `Enum` syntax")).into()
                }

                Some(KnownClass::Super) => {
                    // ```py
                    // class super:
                    //     @overload
                    //     def __init__(self, t: Any, obj: Any, /) -> None: ...
                    //     @overload
                    //     def __init__(self, t: Any, /) -> None: ...
                    //     @overload
                    //     def __init__(self) -> None: ...
                    // ```
                    CallableBinding::from_overloads(
                        self,
                        [
                            Signature::new(
                                Parameters::new([
                                    Parameter::positional_only(Some(Name::new_static("t")))
                                        .with_annotated_type(Type::any()),
                                    Parameter::positional_only(Some(Name::new_static("obj")))
                                        .with_annotated_type(Type::any()),
                                ]),
                                Some(KnownClass::Super.to_instance(db)),
                            ),
                            Signature::new(
                                Parameters::new([Parameter::positional_only(Some(
                                    Name::new_static("t"),
                                ))
                                .with_annotated_type(Type::any())]),
                                Some(KnownClass::Super.to_instance(db)),
                            ),
                            Signature::new(
                                Parameters::empty(),
                                Some(KnownClass::Super.to_instance(db)),
                            ),
                        ],
                    )
                    .into()
                }

                Some(KnownClass::TypeVar) => {
                    // ```py
                    // class TypeVar:
                    //     def __new__(
                    //         cls,
                    //         name: str,
                    //         *constraints: Any,
                    //         bound: Any | None = None,
                    //         contravariant: bool = False,
                    //         covariant: bool = False,
                    //         infer_variance: bool = False,
                    //         default: Any = ...,
                    //     ) -> Self: ...
                    // ```
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new([
                                Parameter::positional_or_keyword(Name::new_static("name"))
                                    .with_annotated_type(Type::LiteralString),
                                Parameter::variadic(Name::new_static("constraints"))
                                    .type_form()
                                    .with_annotated_type(Type::any()),
                                Parameter::keyword_only(Name::new_static("bound"))
                                    .type_form()
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [Type::any(), Type::none(db)],
                                    ))
                                    .with_default_type(Type::none(db)),
                                Parameter::keyword_only(Name::new_static("default"))
                                    .type_form()
                                    .with_annotated_type(Type::any())
                                    .with_default_type(KnownClass::NoneType.to_instance(db)),
                                Parameter::keyword_only(Name::new_static("contravariant"))
                                    .with_annotated_type(KnownClass::Bool.to_instance(db))
                                    .with_default_type(Type::BooleanLiteral(false)),
                                Parameter::keyword_only(Name::new_static("covariant"))
                                    .with_annotated_type(KnownClass::Bool.to_instance(db))
                                    .with_default_type(Type::BooleanLiteral(false)),
                                Parameter::keyword_only(Name::new_static("infer_variance"))
                                    .with_annotated_type(KnownClass::Bool.to_instance(db))
                                    .with_default_type(Type::BooleanLiteral(false)),
                            ]),
                            Some(KnownClass::TypeVar.to_instance(db)),
                        ),
                    )
                    .into()
                }

                Some(KnownClass::Deprecated) => {
                    // ```py
                    // class deprecated:
                    //     def __new__(
                    //         cls,
                    //         message: LiteralString,
                    //         /,
                    //         *,
                    //         category: type[Warning] | None = ...,
                    //         stacklevel: int = 1
                    //     ) -> Self: ...
                    // ```
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new([
                                Parameter::positional_only(Some(Name::new_static("message")))
                                    .with_annotated_type(Type::LiteralString),
                                Parameter::keyword_only(Name::new_static("category"))
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [
                                            // TODO: should be `type[Warning]`
                                            Type::any(),
                                            KnownClass::NoneType.to_instance(db),
                                        ],
                                    ))
                                    // TODO: should be `type[Warning]`
                                    .with_default_type(Type::any()),
                                Parameter::keyword_only(Name::new_static("stacklevel"))
                                    .with_annotated_type(KnownClass::Int.to_instance(db))
                                    .with_default_type(Type::IntLiteral(1)),
                            ]),
                            Some(KnownClass::Deprecated.to_instance(db)),
                        ),
                    )
                    .into()
                }

                Some(KnownClass::TypeAliasType) => {
                    // ```py
                    // def __new__(
                    //     cls,
                    //     name: str,
                    //     value: Any,
                    //     *,
                    //     type_params: tuple[TypeVar | ParamSpec | TypeVarTuple, ...] = ()
                    // ) -> Self: ...
                    // ```
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new([
                                Parameter::positional_or_keyword(Name::new_static("name"))
                                    .with_annotated_type(KnownClass::Str.to_instance(db)),
                                Parameter::positional_or_keyword(Name::new_static("value"))
                                    .with_annotated_type(Type::any())
                                    .type_form(),
                                Parameter::keyword_only(Name::new_static("type_params"))
                                    .with_annotated_type(Type::homogeneous_tuple(
                                        db,
                                        UnionType::from_elements(
                                            db,
                                            [
                                                KnownClass::TypeVar.to_instance(db),
                                                KnownClass::ParamSpec.to_instance(db),
                                                KnownClass::TypeVarTuple.to_instance(db),
                                            ],
                                        ),
                                    ))
                                    .with_default_type(Type::empty_tuple(db)),
                            ]),
                            None,
                        ),
                    )
                    .into()
                }

                Some(KnownClass::Property) => {
                    let getter_signature = Signature::new(
                        Parameters::new([
                            Parameter::positional_only(None).with_annotated_type(Type::any())
                        ]),
                        Some(Type::any()),
                    );
                    let setter_signature = Signature::new(
                        Parameters::new([
                            Parameter::positional_only(None).with_annotated_type(Type::any()),
                            Parameter::positional_only(None).with_annotated_type(Type::any()),
                        ]),
                        Some(Type::none(db)),
                    );
                    let deleter_signature = Signature::new(
                        Parameters::new([
                            Parameter::positional_only(None).with_annotated_type(Type::any())
                        ]),
                        Some(Type::any()),
                    );

                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new([
                                Parameter::positional_or_keyword(Name::new_static("fget"))
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [
                                            CallableType::single(db, getter_signature),
                                            Type::none(db),
                                        ],
                                    ))
                                    .with_default_type(Type::none(db)),
                                Parameter::positional_or_keyword(Name::new_static("fset"))
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [
                                            CallableType::single(db, setter_signature),
                                            Type::none(db),
                                        ],
                                    ))
                                    .with_default_type(Type::none(db)),
                                Parameter::positional_or_keyword(Name::new_static("fdel"))
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [
                                            CallableType::single(db, deleter_signature),
                                            Type::none(db),
                                        ],
                                    ))
                                    .with_default_type(Type::none(db)),
                                Parameter::positional_or_keyword(Name::new_static("doc"))
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [KnownClass::Str.to_instance(db), Type::none(db)],
                                    ))
                                    .with_default_type(Type::none(db)),
                            ]),
                            None,
                        ),
                    )
                    .into()
                }

                Some(KnownClass::Tuple) => {
                    let object = Type::object(db);

                    // ```py
                    // class tuple:
                    //     @overload
                    //     def __new__(cls) -> tuple[()]: ...
                    //     @overload
                    //     def __new__(cls, iterable: Iterable[object]) -> tuple[object, ...]: ...
                    // ```
                    CallableBinding::from_overloads(
                        self,
                        [
                            Signature::new(Parameters::empty(), Some(Type::empty_tuple(db))),
                            Signature::new(
                                Parameters::new([Parameter::positional_only(Some(
                                    Name::new_static("iterable"),
                                ))
                                .with_annotated_type(
                                    KnownClass::Iterable.to_specialized_instance(db, [object]),
                                )]),
                                Some(Type::homogeneous_tuple(db, object)),
                            ),
                        ],
                    )
                    .into()
                }

                // Most class literal constructor calls are handled by `try_call_constructor` and
                // not via getting the signature here. This signature can still be used in some
                // cases (e.g. evaluating callable subtyping). TODO improve this definition
                // (intersection of `__new__` and `__init__` signatures? and respect metaclass
                // `__call__`).
                _ => Binding::single(
                    self,
                    Signature::new_generic(
                        class.generic_context(db),
                        Parameters::gradual_form(),
                        self.to_instance(db),
                    ),
                )
                .into(),
            },

            Type::SpecialForm(SpecialFormType::TypedDict) => {
                Binding::single(
                    self,
                    Signature::new(
                        Parameters::new([
                            Parameter::positional_only(Some(Name::new_static("typename")))
                                .with_annotated_type(KnownClass::Str.to_instance(db)),
                            Parameter::positional_only(Some(Name::new_static("fields")))
                                .with_annotated_type(KnownClass::Dict.to_instance(db))
                                .with_default_type(Type::any()),
                            Parameter::keyword_only(Name::new_static("total"))
                                .with_annotated_type(KnownClass::Bool.to_instance(db))
                                .with_default_type(Type::BooleanLiteral(true)),
                            // Future compatibility, in case new keyword arguments will be added:
                            Parameter::keyword_variadic(Name::new_static("kwargs"))
                                .with_annotated_type(Type::any()),
                        ]),
                        None,
                    ),
                )
                .into()
            }

            Type::GenericAlias(_) => {
                // TODO annotated return type on `__new__` or metaclass `__call__`
                // TODO check call vs signatures of `__new__` and/or `__init__`
                Binding::single(
                    self,
                    Signature::new(Parameters::gradual_form(), self.to_instance(db)),
                )
                .into()
            }

            Type::SubclassOf(subclass_of_type) => match subclass_of_type.subclass_of() {
                SubclassOfInner::Dynamic(dynamic_type) => Type::Dynamic(dynamic_type).bindings(db),

                // Most type[] constructor calls are handled by `try_call_constructor` and not via
                // getting the signature here. This signature can still be used in some cases (e.g.
                // evaluating callable subtyping). TODO improve this definition (intersection of
                // `__new__` and `__init__` signatures? and respect metaclass `__call__`).
                SubclassOfInner::Class(class) => Type::from(class).bindings(db),
            },

            Type::NominalInstance(_) | Type::ProtocolInstance(_) => {
                // Note that for objects that have a (possibly not callable!) `__call__` attribute,
                // we will get the signature of the `__call__` attribute, but will pass in the type
                // of the original object as the "callable type". That ensures that we get errors
                // like "`X` is not callable" instead of "`<type of illegal '__call__'>` is not
                // callable".
                match self
                    .member_lookup_with_policy(
                        db,
                        Name::new_static("__call__"),
                        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    )
                    .place
                {
                    Place::Type(dunder_callable, boundness) => {
                        let mut bindings = dunder_callable.bindings(db);
                        bindings.replace_callable_type(dunder_callable, self);
                        if boundness == Boundness::PossiblyUnbound {
                            bindings.set_dunder_call_is_possibly_unbound();
                        }
                        bindings
                    }
                    Place::Unbound => CallableBinding::not_callable(self).into(),
                }
            }

            // Dynamic types are callable, and the return type is the same dynamic type. Similarly,
            // `Never` is always callable and returns `Never`.
            Type::Dynamic(_) | Type::Never => {
                Binding::single(self, Signature::dynamic(self)).into()
            }

            // Note that this correctly returns `None` if none of the union elements are callable.
            Type::Union(union) => Bindings::from_union(
                self,
                union
                    .elements(db)
                    .iter()
                    .map(|element| element.bindings(db)),
            ),

            Type::Intersection(_) => {
                Binding::single(self, Signature::todo("Type::Intersection.call()")).into()
            }

            // TODO: these are actually callable
            Type::MethodWrapper(_) | Type::DataclassDecorator(_) => {
                CallableBinding::not_callable(self).into()
            }

            // TODO: some `SpecialForm`s are callable (e.g. TypedDicts)
            Type::SpecialForm(_) => CallableBinding::not_callable(self).into(),

            Type::EnumLiteral(enum_literal) => enum_literal.enum_class_instance(db).bindings(db),

            Type::KnownInstance(known_instance) => {
                known_instance.instance_fallback(db).bindings(db)
            }

            Type::PropertyInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::LiteralString
            | Type::BoundSuper(_)
            | Type::ModuleLiteral(_)
            | Type::TypeIs(_)
            | Type::TypedDict(_) => CallableBinding::not_callable(self).into(),
        }
    }

    /// Calls `self`. Returns a [`CallError`] if `self` is (always or possibly) not callable, or if
    /// the arguments are not compatible with the formal parameters.
    ///
    /// You get back a [`Bindings`] for both successful and unsuccessful calls.
    /// It contains information about which formal parameters each argument was matched to,
    /// and about any errors matching arguments and parameters.
    fn try_call(
        self,
        db: &'db dyn Db,
        argument_types: &CallArguments<'_, 'db>,
    ) -> Result<Bindings<'db>, CallError<'db>> {
        self.bindings(db)
            .match_parameters(argument_types)
            .check_types(db, argument_types)
    }

    /// Look up a dunder method on the meta-type of `self` and call it.
    ///
    /// Returns an `Err` if the dunder method can't be called,
    /// or the given arguments are not valid.
    fn try_call_dunder(
        self,
        db: &'db dyn Db,
        name: &str,
        mut argument_types: CallArguments<'_, 'db>,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        self.try_call_dunder_with_policy(
            db,
            name,
            &mut argument_types,
            MemberLookupPolicy::default(),
        )
    }

    /// Same as `try_call_dunder`, but allows specifying a policy for the member lookup. In
    /// particular, this allows to specify `MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK` to avoid
    /// looking up dunder methods on `object`, which is needed for functions like `__init__`,
    /// `__new__`, or `__setattr__`.
    ///
    /// Note that `NO_INSTANCE_FALLBACK` is always added to the policy, since implicit calls to
    /// dunder methods never access instance members.
    fn try_call_dunder_with_policy(
        self,
        db: &'db dyn Db,
        name: &str,
        argument_types: &mut CallArguments<'_, 'db>,
        policy: MemberLookupPolicy,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        // Implicit calls to dunder methods never access instance members, so we pass
        // `NO_INSTANCE_FALLBACK` here in addition to other policies:
        match self
            .member_lookup_with_policy(
                db,
                name.into(),
                policy | MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .place
        {
            Place::Type(dunder_callable, boundness) => {
                let bindings = dunder_callable
                    .bindings(db)
                    .match_parameters(argument_types)
                    .check_types(db, argument_types)?;
                if boundness == Boundness::PossiblyUnbound {
                    return Err(CallDunderError::PossiblyUnbound(Box::new(bindings)));
                }
                Ok(bindings)
            }
            Place::Unbound => Err(CallDunderError::MethodNotAvailable),
        }
    }

    /// Returns a tuple spec describing the elements that are produced when iterating over `self`.
    ///
    /// This method should only be used outside of type checking because it omits any errors.
    /// For type checking, use [`try_iterate`](Self::try_iterate) instead.
    fn iterate(self, db: &'db dyn Db) -> Cow<'db, TupleSpec<'db>> {
        self.try_iterate(db)
            .unwrap_or_else(|err| Cow::Owned(TupleSpec::homogeneous(err.fallback_element_type(db))))
    }

    /// Given the type of an object that is iterated over in some way,
    /// return a tuple spec describing the type of objects that are yielded by that iteration.
    ///
    /// E.g., for the following call, given the type of `x`, infer the types of the values that are
    /// splatted into `y`'s positional arguments:
    /// ```python
    /// y(*x)
    /// ```
    fn try_iterate(self, db: &'db dyn Db) -> Result<Cow<'db, TupleSpec<'db>>, IterationError<'db>> {
        self.try_iterate_with_mode(db, EvaluationMode::Sync)
    }

    fn try_iterate_with_mode(
        self,
        db: &'db dyn Db,
        mode: EvaluationMode,
    ) -> Result<Cow<'db, TupleSpec<'db>>, IterationError<'db>> {
        if mode.is_async() {
            let try_call_dunder_anext_on_iterator = |iterator: Type<'db>| {
                iterator
                    .try_call_dunder(db, "__anext__", CallArguments::none())
                    .map(|dunder_anext_outcome| {
                        dunder_anext_outcome.return_type(db).resolve_await(db)
                    })
            };

            return match self.try_call_dunder(db, "__aiter__", CallArguments::none()) {
                Ok(dunder_aiter_bindings) => {
                    let iterator = dunder_aiter_bindings.return_type(db);
                    match try_call_dunder_anext_on_iterator(iterator) {
                        Ok(result) => Ok(Cow::Owned(TupleSpec::homogeneous(result))),
                        Err(dunder_anext_error) => {
                            Err(IterationError::IterReturnsInvalidIterator {
                                iterator,
                                dunder_error: dunder_anext_error,
                                mode,
                            })
                        }
                    }
                }
                Err(CallDunderError::PossiblyUnbound(dunder_aiter_bindings)) => {
                    let iterator = dunder_aiter_bindings.return_type(db);
                    match try_call_dunder_anext_on_iterator(iterator) {
                        Ok(_) => Err(IterationError::IterCallError {
                            kind: CallErrorKind::PossiblyNotCallable,
                            bindings: dunder_aiter_bindings,
                            mode,
                        }),
                        Err(dunder_anext_error) => {
                            Err(IterationError::IterReturnsInvalidIterator {
                                iterator,
                                dunder_error: dunder_anext_error,
                                mode,
                            })
                        }
                    }
                }
                Err(CallDunderError::CallError(kind, bindings)) => {
                    Err(IterationError::IterCallError {
                        kind,
                        bindings,
                        mode,
                    })
                }
                Err(CallDunderError::MethodNotAvailable) => Err(IterationError::UnboundAiterError),
            };
        }

        match self {
            Type::NominalInstance(nominal) => {
                if let Some(spec) = nominal.tuple_spec(db) {
                    return Ok(spec);
                }
            }
            Type::GenericAlias(alias) if alias.origin(db).is_tuple(db) => {
                return Ok(Cow::Owned(TupleSpec::homogeneous(todo_type!(
                    "*tuple[] annotations"
                ))));
            }
            Type::StringLiteral(string_literal_ty) => {
                // We could go further and deconstruct to an array of `StringLiteral`
                // with each individual character, instead of just an array of
                // `LiteralString`, but there would be a cost and it's not clear that
                // it's worth it.
                return Ok(Cow::Owned(TupleSpec::from_elements(std::iter::repeat_n(
                    Type::LiteralString,
                    string_literal_ty.python_len(db),
                ))));
            }
            Type::Never => {
                // The dunder logic below would have us return `tuple[Never, ...]`, which eagerly
                // simplifies to `tuple[()]`. That will will cause us to emit false positives if we
                // index into the tuple. Using `tuple[Unknown, ...]` avoids these false positives.
                // TODO: Consider removing this special case, and instead hide the indexing
                // diagnostic in unreachable code.
                return Ok(Cow::Owned(TupleSpec::homogeneous(Type::unknown())));
            }
            _ => {}
        }

        let try_call_dunder_getitem = || {
            self.try_call_dunder(
                db,
                "__getitem__",
                CallArguments::positional([KnownClass::Int.to_instance(db)]),
            )
            .map(|dunder_getitem_outcome| dunder_getitem_outcome.return_type(db))
        };

        let try_call_dunder_next_on_iterator = |iterator: Type<'db>| {
            iterator
                .try_call_dunder(db, "__next__", CallArguments::none())
                .map(|dunder_next_outcome| dunder_next_outcome.return_type(db))
        };

        let dunder_iter_result = self
            .try_call_dunder(db, "__iter__", CallArguments::none())
            .map(|dunder_iter_outcome| dunder_iter_outcome.return_type(db));

        match dunder_iter_result {
            Ok(iterator) => {
                // `__iter__` is definitely bound and calling it succeeds.
                // See what calling `__next__` on the object returned by `__iter__` gives us...
                try_call_dunder_next_on_iterator(iterator)
                    .map(|ty| Cow::Owned(TupleSpec::homogeneous(ty)))
                    .map_err(
                        |dunder_next_error| IterationError::IterReturnsInvalidIterator {
                            iterator,
                            dunder_error: dunder_next_error,
                            mode,
                        },
                    )
            }

            // `__iter__` is possibly unbound...
            Err(CallDunderError::PossiblyUnbound(dunder_iter_outcome)) => {
                let iterator = dunder_iter_outcome.return_type(db);

                match try_call_dunder_next_on_iterator(iterator) {
                    Ok(dunder_next_return) => {
                        try_call_dunder_getitem()
                            .map(|dunder_getitem_return_type| {
                                // If `__iter__` is possibly unbound,
                                // but it returns an object that has a bound and valid `__next__` method,
                                // *and* the object has a bound and valid `__getitem__` method,
                                // we infer a union of the type returned by the `__next__` method
                                // and the type returned by the `__getitem__` method.
                                //
                                // No diagnostic is emitted; iteration will always succeed!
                                Cow::Owned(TupleSpec::homogeneous(UnionType::from_elements(
                                    db,
                                    [dunder_next_return, dunder_getitem_return_type],
                                )))
                            })
                            .map_err(|dunder_getitem_error| {
                                IterationError::PossiblyUnboundIterAndGetitemError {
                                    dunder_next_return,
                                    dunder_getitem_error,
                                }
                            })
                    }

                    Err(dunder_next_error) => Err(IterationError::IterReturnsInvalidIterator {
                        iterator,
                        dunder_error: dunder_next_error,
                        mode,
                    }),
                }
            }

            // `__iter__` is definitely bound but it can't be called with the expected arguments
            Err(CallDunderError::CallError(kind, bindings)) => Err(IterationError::IterCallError {
                kind,
                bindings,
                mode,
            }),

            // There's no `__iter__` method. Try `__getitem__` instead...
            Err(CallDunderError::MethodNotAvailable) => try_call_dunder_getitem()
                .map(|ty| Cow::Owned(TupleSpec::homogeneous(ty)))
                .map_err(
                    |dunder_getitem_error| IterationError::UnboundIterAndGetitemError {
                        dunder_getitem_error,
                    },
                ),
        }
    }

    /// Returns the type bound from a context manager with type `self`.
    ///
    /// This method should only be used outside of type checking because it omits any errors.
    /// For type checking, use [`try_enter_with_mode`](Self::try_enter_with_mode) instead.
    fn enter(self, db: &'db dyn Db) -> Type<'db> {
        self.try_enter_with_mode(db, EvaluationMode::Sync)
            .unwrap_or_else(|err| err.fallback_enter_type(db))
    }

    /// Returns the type bound from a context manager with type `self`.
    ///
    /// This method should only be used outside of type checking because it omits any errors.
    /// For type checking, use [`try_enter_with_mode`](Self::try_enter_with_mode) instead.
    fn aenter(self, db: &'db dyn Db) -> Type<'db> {
        self.try_enter_with_mode(db, EvaluationMode::Async)
            .unwrap_or_else(|err| err.fallback_enter_type(db))
    }

    /// Given the type of an object that is used as a context manager (i.e. in a `with` statement),
    /// return the return type of its `__enter__` or `__aenter__` method, which is bound to any potential targets.
    ///
    /// E.g., for the following `with` statement, given the type of `x`, infer the type of `y`:
    /// ```python
    /// with x as y:
    ///     pass
    /// ```
    fn try_enter_with_mode(
        self,
        db: &'db dyn Db,
        mode: EvaluationMode,
    ) -> Result<Type<'db>, ContextManagerError<'db>> {
        let (enter_method, exit_method) = match mode {
            EvaluationMode::Async => ("__aenter__", "__aexit__"),
            EvaluationMode::Sync => ("__enter__", "__exit__"),
        };

        let enter = self.try_call_dunder(db, enter_method, CallArguments::none());
        let exit = self.try_call_dunder(
            db,
            exit_method,
            CallArguments::positional([Type::none(db), Type::none(db), Type::none(db)]),
        );

        // TODO: Make use of Protocols when we support it (the manager be assignable to `contextlib.AbstractContextManager`).
        match (enter, exit) {
            (Ok(enter), Ok(_)) => {
                let ty = enter.return_type(db);
                Ok(if mode.is_async() {
                    ty.resolve_await(db)
                } else {
                    ty
                })
            }
            (Ok(enter), Err(exit_error)) => {
                let ty = enter.return_type(db);
                Err(ContextManagerError::Exit {
                    enter_return_type: if mode.is_async() {
                        ty.resolve_await(db)
                    } else {
                        ty
                    },
                    exit_error,
                    mode,
                })
            }
            // TODO: Use the `exit_ty` to determine if any raised exception is suppressed.
            (Err(enter_error), Ok(_)) => Err(ContextManagerError::Enter(enter_error, mode)),
            (Err(enter_error), Err(exit_error)) => Err(ContextManagerError::EnterAndExit {
                enter_error,
                exit_error,
                mode,
            }),
        }
    }

    /// Resolve the type of an `await …` expression where `self` is the type of the awaitable.
    fn resolve_await(self, db: &'db dyn Db) -> Type<'db> {
        // TODO: Add proper error handling and rename this method to `try_await`.
        self.try_call_dunder(db, "__await__", CallArguments::none())
            .map_or(Type::unknown(), |result| {
                result
                    .return_type(db)
                    .generator_return_type(db)
                    .unwrap_or_else(Type::unknown)
            })
    }

    /// Get the return type of a `yield from …` expression where `self` is the type of the generator.
    ///
    /// This corresponds to the `ReturnT` parameter of the generic `typing.Generator[YieldT, SendT, ReturnT]`
    /// protocol.
    fn generator_return_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        // TODO: Ideally, we would first try to upcast `self` to an instance of `Generator` and *then*
        // match on the protocol instance to get the `ReturnType` type parameter. For now, implement
        // an ad-hoc solution that works for protocols and instances of classes that explicitly inherit
        // from the `Generator` protocol, such as `types.GeneratorType`.

        let from_class_base = |base: ClassBase<'db>| {
            let class = base.into_class()?;
            if class.is_known(db, KnownClass::Generator) {
                if let Some(specialization) = class.class_literal_specialized(db, None).1 {
                    if let [_, _, return_ty] = specialization.types(db) {
                        return Some(*return_ty);
                    }
                }
            }
            None
        };

        match self {
            Type::NominalInstance(instance) => {
                instance.class(db).iter_mro(db).find_map(from_class_base)
            }
            Type::ProtocolInstance(instance) => {
                if let Protocol::FromClass(class) = instance.inner {
                    class.iter_mro(db).find_map(from_class_base)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Given a class literal or non-dynamic `SubclassOf` type, try calling it (creating an instance)
    /// and return the resulting instance type.
    ///
    /// Models `type.__call__` behavior.
    /// TODO: model metaclass `__call__`.
    ///
    /// E.g., for the following code, infer the type of `Foo()`:
    /// ```python
    /// class Foo:
    ///     pass
    ///
    /// Foo()
    /// ```
    fn try_call_constructor(
        self,
        db: &'db dyn Db,
        argument_types: CallArguments<'_, 'db>,
    ) -> Result<Type<'db>, ConstructorCallError<'db>> {
        debug_assert!(matches!(
            self,
            Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)
        ));

        // If we are trying to construct a non-specialized generic class, we should use the
        // constructor parameters to try to infer the class specialization. To do this, we need to
        // tweak our member lookup logic a bit. Normally, when looking up a class or instance
        // member, we first apply the class's default specialization, and apply that specialization
        // to the type of the member. To infer a specialization from the argument types, we need to
        // have the class's typevars still in the method signature when we attempt to call it. To
        // do this, we instead use the _identity_ specialization, which maps each of the class's
        // generic typevars to itself.
        let (generic_origin, generic_context, self_type) =
            match self {
                Type::ClassLiteral(class) => match class.generic_context(db) {
                    Some(generic_context) => (
                        Some(class),
                        Some(generic_context),
                        Type::from(class.apply_specialization(db, |_| {
                            generic_context.identity_specialization(db)
                        })),
                    ),
                    _ => (None, None, self),
                },
                _ => (None, None, self),
            };

        // As of now we do not model custom `__call__` on meta-classes, so the code below
        // only deals with interplay between `__new__` and `__init__` methods.
        // The logic is roughly as follows:
        // 1. If `__new__` is defined anywhere in the MRO (except for `object`, since it is always
        //    present), we call it and analyze outcome. We then analyze `__init__` call, but only
        //    if it is defined somewhere except object. This is because `object.__init__`
        //    allows arbitrary arguments if and only if `__new__` is defined, but typeshed
        //    defines `__init__` for `object` with no arguments.
        // 2. If `__new__` is not found, we call `__init__`. Here, we allow it to fallback all
        //    the way to `object` (single `self` argument call). This time it is correct to
        //    fallback to `object.__init__`, since it will indeed check that no arguments are
        //    passed.
        //
        // Note that we currently ignore `__new__` return type, since we do not yet support `Self`
        // and most builtin classes use it as return type annotation. We always return the instance
        // type.

        // Lookup `__new__` method in the MRO up to, but not including, `object`. Also, we must
        // avoid `__new__` on `type` since per descriptor protocol, if `__new__` is not defined on
        // a class, metaclass attribute would take precedence. But by avoiding `__new__` on
        // `object` we would inadvertently unhide `__new__` on `type`, which is not what we want.
        // An alternative might be to not skip `object.__new__` but instead mark it such that it's
        // easy to check if that's the one we found?
        // Note that `__new__` is a static method, so we must inject the `cls` argument.
        let new_method = self_type.lookup_dunder_new(db, ());
        let new_call_outcome = new_method.and_then(|new_method| {
            match new_method.place.try_call_dunder_get(db, self_type) {
                Place::Type(new_method, boundness) => {
                    let result =
                        new_method.try_call(db, argument_types.with_self(Some(self_type)).as_ref());
                    if boundness == Boundness::PossiblyUnbound {
                        Some(Err(DunderNewCallError::PossiblyUnbound(result.err())))
                    } else {
                        Some(result.map_err(DunderNewCallError::CallError))
                    }
                }
                Place::Unbound => None,
            }
        });

        // Construct an instance type that we can use to look up the `__init__` instance method.
        // This performs the same logic as `Type::to_instance`, except for generic class literals.
        // TODO: we should use the actual return type of `__new__` to determine the instance type
        let init_ty = self_type
            .to_instance(db)
            .expect("type should be convertible to instance type");

        let init_call_outcome = if new_call_outcome.is_none()
            || !init_ty
                .member_lookup_with_policy(
                    db,
                    "__init__".into(),
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK
                        | MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                )
                .place
                .is_unbound()
        {
            Some(init_ty.try_call_dunder(db, "__init__", argument_types))
        } else {
            None
        };

        // Note that we use `self` here, not `self_type`, so that if constructor argument inference
        // fails, we fail back to the default specialization.
        let instance_ty = self
            .to_instance(db)
            .expect("type should be convertible to instance type");

        match (generic_origin, new_call_outcome, init_call_outcome) {
            // All calls are successful or not called at all
            (
                Some(generic_origin),
                new_call_outcome @ (None | Some(Ok(_))),
                init_call_outcome @ (None | Some(Ok(_))),
            ) => {
                fn combine_specializations<'db>(
                    db: &'db dyn Db,
                    s1: Option<Specialization<'db>>,
                    s2: Option<Specialization<'db>>,
                ) -> Option<Specialization<'db>> {
                    match (s1, s2) {
                        (None, None) => None,
                        (Some(s), None) | (None, Some(s)) => Some(s),
                        (Some(s1), Some(s2)) => Some(s1.combine(db, s2)),
                    }
                }

                let new_specialization = new_call_outcome
                    .and_then(Result::ok)
                    .as_ref()
                    .and_then(Bindings::single_element)
                    .into_iter()
                    .flat_map(CallableBinding::matching_overloads)
                    .next()
                    .and_then(|(_, binding)| binding.inherited_specialization())
                    .filter(|specialization| {
                        Some(specialization.generic_context(db)) == generic_context
                    });
                let init_specialization = init_call_outcome
                    .and_then(Result::ok)
                    .as_ref()
                    .and_then(Bindings::single_element)
                    .into_iter()
                    .flat_map(CallableBinding::matching_overloads)
                    .next()
                    .and_then(|(_, binding)| binding.inherited_specialization())
                    .filter(|specialization| {
                        Some(specialization.generic_context(db)) == generic_context
                    });
                let specialization =
                    combine_specializations(db, new_specialization, init_specialization);
                let specialized = specialization
                    .map(|specialization| {
                        Type::instance(
                            db,
                            generic_origin.apply_specialization(db, |_| specialization),
                        )
                    })
                    .unwrap_or(instance_ty);
                Ok(specialized)
            }

            (None, None | Some(Ok(_)), None | Some(Ok(_))) => Ok(instance_ty),

            (_, None | Some(Ok(_)), Some(Err(error))) => {
                // no custom `__new__` or it was called and succeeded, but `__init__` failed.
                Err(ConstructorCallError::Init(instance_ty, error))
            }
            (_, Some(Err(error)), None | Some(Ok(_))) => {
                // custom `__new__` was called and failed, but init is ok
                Err(ConstructorCallError::New(instance_ty, error))
            }
            (_, Some(Err(new_error)), Some(Err(init_error))) => {
                // custom `__new__` was called and failed, and `__init__` is also not ok
                Err(ConstructorCallError::NewAndInit(
                    instance_ty,
                    new_error,
                    init_error,
                ))
            }
        }
    }

    #[must_use]
    pub fn to_instance(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Type::Dynamic(_) | Type::Never => Some(*self),
            Type::ClassLiteral(class) => Some(Type::instance(db, class.default_specialization(db))),
            Type::GenericAlias(alias) => Some(Type::instance(db, ClassType::from(*alias))),
            Type::SubclassOf(subclass_of_ty) => Some(subclass_of_ty.to_instance(db)),
            Type::Union(union) => union.to_instance(db),
            // If there is no bound or constraints on a typevar `T`, `T: object` implicitly, which
            // has no instance type. Otherwise, synthesize a typevar with bound or constraints
            // mapped through `to_instance`.
            Type::TypeVar(bound_typevar) => {
                let typevar = bound_typevar.typevar(db);
                let bound_or_constraints = match typevar.bound_or_constraints(db)? {
                    TypeVarBoundOrConstraints::UpperBound(upper_bound) => {
                        TypeVarBoundOrConstraints::UpperBound(upper_bound.to_instance(db)?)
                    }
                    TypeVarBoundOrConstraints::Constraints(constraints) => {
                        TypeVarBoundOrConstraints::Constraints(
                            constraints.to_instance(db)?.into_union()?,
                        )
                    }
                };
                Some(Type::TypeVar(BoundTypeVarInstance::new(
                    db,
                    TypeVarInstance::new(
                        db,
                        Name::new(format!("{}'instance", typevar.name(db))),
                        None,
                        Some(bound_or_constraints),
                        typevar.variance(db),
                        None,
                        typevar.kind(db),
                    ),
                    bound_typevar.binding_context(db),
                )))
            }
            Type::Intersection(_) => Some(todo_type!("Type::Intersection.to_instance")),
            Type::BooleanLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::FunctionLiteral(_)
            | Type::Callable(..)
            | Type::MethodWrapper(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::NominalInstance(_)
            | Type::ProtocolInstance(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::PropertyInstance(_)
            | Type::ModuleLiteral(_)
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::BoundSuper(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypedDict(_) => None,
        }
    }

    /// If we see a value of this type used as a type expression, what type does it name?
    ///
    /// For example, the builtin `int` as a value expression is of type
    /// `Type::ClassLiteral(builtins.int)`, that is, it is the `int` class itself. As a type
    /// expression, it names the type `Type::NominalInstance(builtins.int)`, that is, all objects whose
    /// `__class__` is `int`.
    ///
    /// The `scope_id` and `typevar_binding_context` arguments must always come from the file we are currently inferring, so
    /// as to avoid cross-module AST dependency.
    pub(crate) fn in_type_expression(
        &self,
        db: &'db dyn Db,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        match self {
            // Special cases for `float` and `complex`
            // https://typing.python.org/en/latest/spec/special-types.html#special-cases-for-float-and-complex
            Type::ClassLiteral(class) => {
                let ty = match class.known(db) {
                    Some(KnownClass::Any) => Type::any(),
                    Some(KnownClass::Complex) => UnionType::from_elements(
                        db,
                        [
                            KnownClass::Int.to_instance(db),
                            KnownClass::Float.to_instance(db),
                            KnownClass::Complex.to_instance(db),
                        ],
                    ),
                    Some(KnownClass::Float) => UnionType::from_elements(
                        db,
                        [
                            KnownClass::Int.to_instance(db),
                            KnownClass::Float.to_instance(db),
                        ],
                    ),
                    _ if class.is_typed_dict(db) => {
                        TypedDictType::from(db, ClassType::NonGeneric(*class))
                    }
                    _ => Type::instance(db, class.default_specialization(db)),
                };
                Ok(ty)
            }
            Type::GenericAlias(alias) if alias.is_typed_dict(db) => {
                Ok(TypedDictType::from(db, ClassType::from(*alias)))
            }
            Type::GenericAlias(alias) => Ok(Type::instance(db, ClassType::from(*alias))),

            Type::SubclassOf(_)
            | Type::BooleanLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::IntLiteral(_)
            | Type::LiteralString
            | Type::ModuleLiteral(_)
            | Type::StringLiteral(_)
            | Type::TypeVar(_)
            | Type::Callable(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::Never
            | Type::FunctionLiteral(_)
            | Type::BoundSuper(_)
            | Type::ProtocolInstance(_)
            | Type::PropertyInstance(_)
            | Type::TypeIs(_)
            | Type::TypedDict(_) => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![
                    InvalidTypeExpression::InvalidType(*self, scope_id)
                ],
                fallback_type: Type::unknown(),
            }),

            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::TypeAliasType(alias) => Ok(alias.value_type(db)),
                KnownInstanceType::TypeVar(typevar) => {
                    let module = parsed_module(db, scope_id.file(db)).load(db);
                    let index = semantic_index(db, scope_id.file(db));
                    Ok(bind_typevar(
                        db,
                        &module,
                        index,
                        scope_id.file_scope_id(db),
                        typevar_binding_context,
                        *typevar,
                    )
                    .map(Type::TypeVar)
                    .unwrap_or(*self))
                }
                KnownInstanceType::Deprecated(_) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec![InvalidTypeExpression::Deprecated],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::Field(__call__) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec![InvalidTypeExpression::Field],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::SubscriptedProtocol(_) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![
                        InvalidTypeExpression::Protocol
                    ],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::SubscriptedGeneric(_) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![InvalidTypeExpression::Generic],
                    fallback_type: Type::unknown(),
                }),
            },

            Type::SpecialForm(special_form) => match special_form {
                SpecialFormType::Never | SpecialFormType::NoReturn => Ok(Type::Never),
                SpecialFormType::LiteralString => Ok(Type::LiteralString),
                SpecialFormType::Unknown => Ok(Type::unknown()),
                SpecialFormType::AlwaysTruthy => Ok(Type::AlwaysTruthy),
                SpecialFormType::AlwaysFalsy => Ok(Type::AlwaysFalsy),

                // We treat `typing.Type` exactly the same as `builtins.type`:
                SpecialFormType::Type => Ok(KnownClass::Type.to_instance(db)),
                SpecialFormType::Tuple => Ok(Type::homogeneous_tuple(db, Type::unknown())),

                // Legacy `typing` aliases
                SpecialFormType::List => Ok(KnownClass::List.to_instance(db)),
                SpecialFormType::Dict => Ok(KnownClass::Dict.to_instance(db)),
                SpecialFormType::Set => Ok(KnownClass::Set.to_instance(db)),
                SpecialFormType::FrozenSet => Ok(KnownClass::FrozenSet.to_instance(db)),
                SpecialFormType::ChainMap => Ok(KnownClass::ChainMap.to_instance(db)),
                SpecialFormType::Counter => Ok(KnownClass::Counter.to_instance(db)),
                SpecialFormType::DefaultDict => Ok(KnownClass::DefaultDict.to_instance(db)),
                SpecialFormType::Deque => Ok(KnownClass::Deque.to_instance(db)),
                SpecialFormType::OrderedDict => Ok(KnownClass::OrderedDict.to_instance(db)),

                // TODO: Use an opt-in rule for a bare `Callable`
                SpecialFormType::Callable => Ok(CallableType::unknown(db)),

                SpecialFormType::TypingSelf => {
                    let module = parsed_module(db, scope_id.file(db)).load(db);
                    let index = semantic_index(db, scope_id.file(db));
                    let Some(class) = nearest_enclosing_class(db, index, scope_id, &module) else {
                        return Err(InvalidTypeExpressionError {
                            fallback_type: Type::unknown(),
                            invalid_expressions: smallvec::smallvec_inline![
                                InvalidTypeExpression::InvalidType(*self, scope_id)
                            ],
                        });
                    };
                    let instance = Type::ClassLiteral(class).to_instance(db).expect(
                        "nearest_enclosing_class must return type that can be instantiated",
                    );
                    let class_definition = class.definition(db);
                    let typevar = TypeVarInstance::new(
                        db,
                        ast::name::Name::new_static("Self"),
                        Some(class_definition),
                        Some(TypeVarBoundOrConstraints::UpperBound(instance)),
                        TypeVarVariance::Invariant,
                        None,
                        TypeVarKind::Implicit,
                    );
                    Ok(bind_typevar(
                        db,
                        &module,
                        index,
                        scope_id.file_scope_id(db),
                        typevar_binding_context,
                        typevar,
                    )
                    .map(Type::TypeVar)
                    .unwrap_or(*self))
                }
                SpecialFormType::TypeAlias => Ok(Type::Dynamic(DynamicType::TodoTypeAlias)),
                SpecialFormType::TypedDict => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![
                        InvalidTypeExpression::TypedDict
                    ],
                    fallback_type: Type::unknown(),
                }),

                SpecialFormType::Literal
                | SpecialFormType::Union
                | SpecialFormType::Intersection => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![
                        InvalidTypeExpression::RequiresArguments(*self)
                    ],
                    fallback_type: Type::unknown(),
                }),

                SpecialFormType::Protocol => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![
                        InvalidTypeExpression::Protocol
                    ],
                    fallback_type: Type::unknown(),
                }),
                SpecialFormType::Generic => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![InvalidTypeExpression::Generic],
                    fallback_type: Type::unknown(),
                }),

                SpecialFormType::Optional
                | SpecialFormType::Not
                | SpecialFormType::TypeOf
                | SpecialFormType::TypeIs
                | SpecialFormType::TypeGuard
                | SpecialFormType::Unpack
                | SpecialFormType::CallableTypeOf => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![
                        InvalidTypeExpression::RequiresOneArgument(*self)
                    ],
                    fallback_type: Type::unknown(),
                }),

                SpecialFormType::Annotated | SpecialFormType::Concatenate => {
                    Err(InvalidTypeExpressionError {
                        invalid_expressions: smallvec::smallvec_inline![
                            InvalidTypeExpression::RequiresTwoArguments(*self)
                        ],
                        fallback_type: Type::unknown(),
                    })
                }

                SpecialFormType::ClassVar | SpecialFormType::Final => {
                    Err(InvalidTypeExpressionError {
                        invalid_expressions: smallvec::smallvec_inline![
                            InvalidTypeExpression::TypeQualifier(*special_form)
                        ],
                        fallback_type: Type::unknown(),
                    })
                }

                SpecialFormType::ReadOnly
                | SpecialFormType::NotRequired
                | SpecialFormType::Required => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![
                        InvalidTypeExpression::TypeQualifierRequiresOneArgument(*special_form)
                    ],
                    fallback_type: Type::unknown(),
                }),
            },

            Type::Union(union) => {
                let mut builder = UnionBuilder::new(db);
                let mut invalid_expressions = smallvec::SmallVec::default();
                for element in union.elements(db) {
                    match element.in_type_expression(db, scope_id, typevar_binding_context) {
                        Ok(type_expr) => builder = builder.add(type_expr),
                        Err(InvalidTypeExpressionError {
                            fallback_type,
                            invalid_expressions: new_invalid_expressions,
                        }) => {
                            invalid_expressions.extend(new_invalid_expressions);
                            builder = builder.add(fallback_type);
                        }
                    }
                }
                if invalid_expressions.is_empty() {
                    Ok(builder.build())
                } else {
                    Err(InvalidTypeExpressionError {
                        fallback_type: builder.build(),
                        invalid_expressions,
                    })
                }
            }

            Type::Dynamic(_) => Ok(*self),

            Type::NominalInstance(instance) => match instance.class(db).known(db) {
                Some(KnownClass::TypeVar) => Ok(todo_type!(
                    "Support for `typing.TypeVar` instances in type expressions"
                )),
                Some(
                    KnownClass::ParamSpec | KnownClass::ParamSpecArgs | KnownClass::ParamSpecKwargs,
                ) => Ok(todo_type!("Support for `typing.ParamSpec`")),
                Some(KnownClass::TypeVarTuple) => Ok(todo_type!(
                    "Support for `typing.TypeVarTuple` instances in type expressions"
                )),
                Some(KnownClass::NewType) => Ok(todo_type!(
                    "Support for `typing.NewType` instances in type expressions"
                )),
                Some(KnownClass::GenericAlias) => Ok(todo_type!(
                    "Support for `typing.GenericAlias` instances in type expressions"
                )),
                Some(KnownClass::UnionType) => Ok(todo_type!(
                    "Support for `types.UnionType` instances in type expressions"
                )),
                _ => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![
                        InvalidTypeExpression::InvalidType(*self, scope_id)
                    ],
                    fallback_type: Type::unknown(),
                }),
            },

            Type::Intersection(_) => Ok(todo_type!("Type::Intersection.in_type_expression")),
        }
    }

    /// The type `NoneType` / `None`
    pub fn none(db: &'db dyn Db) -> Type<'db> {
        KnownClass::NoneType.to_instance(db)
    }

    /// Given a type that is assumed to represent an instance of a class,
    /// return a type that represents that class itself.
    ///
    /// Note: the return type of `type(obj)` is subtly different from this.
    /// See `Self::dunder_class` for more details.
    #[must_use]
    pub fn to_meta_type(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::Never => Type::Never,
            Type::NominalInstance(instance) => instance.to_meta_type(db),
            Type::KnownInstance(known_instance) => known_instance.to_meta_type(db),
            Type::SpecialForm(special_form) => special_form.to_meta_type(db),
            Type::PropertyInstance(_) => KnownClass::Property.to_class_literal(db),
            Type::Union(union) => union.map(db, |ty| ty.to_meta_type(db)),
            Type::BooleanLiteral(_) | Type::TypeIs(_) => KnownClass::Bool.to_class_literal(db),
            Type::BytesLiteral(_) => KnownClass::Bytes.to_class_literal(db),
            Type::IntLiteral(_) => KnownClass::Int.to_class_literal(db),
            Type::EnumLiteral(enum_literal) => Type::ClassLiteral(enum_literal.enum_class(db)),
            Type::FunctionLiteral(_) => KnownClass::FunctionType.to_class_literal(db),
            Type::BoundMethod(_) => KnownClass::MethodType.to_class_literal(db),
            Type::MethodWrapper(_) => KnownClass::MethodWrapperType.to_class_literal(db),
            Type::WrapperDescriptor(_) => KnownClass::WrapperDescriptorType.to_class_literal(db),
            Type::DataclassDecorator(_) => KnownClass::FunctionType.to_class_literal(db),
            Type::Callable(callable) if callable.is_function_like(db) => {
                KnownClass::FunctionType.to_class_literal(db)
            }
            Type::Callable(_) | Type::DataclassTransformer(_) => KnownClass::Type.to_instance(db),
            Type::ModuleLiteral(_) => KnownClass::ModuleType.to_class_literal(db),
            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => KnownClass::Type.to_instance(db),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound.to_meta_type(db),
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        // TODO: If we add a proper `OneOf` connector, we should use that here instead
                        // of union. (Using a union here doesn't break anything, but it is imprecise.)
                        constraints.map(db, |constraint| constraint.to_meta_type(db))
                    }
                }
            }

            Type::ClassLiteral(class) => class.metaclass(db),
            Type::GenericAlias(alias) => ClassType::from(*alias).metaclass(db),
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Dynamic(_) => *self,
                SubclassOfInner::Class(class) => SubclassOfType::from(
                    db,
                    SubclassOfInner::try_from_type(db, class.metaclass(db))
                        .unwrap_or(SubclassOfInner::unknown()),
                ),
            },

            Type::StringLiteral(_) | Type::LiteralString => KnownClass::Str.to_class_literal(db),
            Type::Dynamic(dynamic) => SubclassOfType::from(db, SubclassOfInner::Dynamic(*dynamic)),
            // TODO intersections
            Type::Intersection(_) => SubclassOfType::from(
                db,
                SubclassOfInner::try_from_type(db, todo_type!("Intersection meta-type"))
                    .expect("Type::Todo should be a valid `SubclassOfInner`"),
            ),
            Type::AlwaysTruthy | Type::AlwaysFalsy => KnownClass::Type.to_instance(db),
            Type::BoundSuper(_) => KnownClass::Super.to_class_literal(db),
            Type::ProtocolInstance(protocol) => protocol.to_meta_type(db),
            Type::TypedDict(typed_dict) => SubclassOfType::from(db, typed_dict.defining_class(db)),
        }
    }

    /// Get the type of the `__class__` attribute of this type.
    ///
    /// For most types, this is equivalent to the meta type of this type. For `TypedDict` types,
    /// this returns `type[dict[str, object]]` instead, because inhabitants of a `TypedDict` are
    /// instances of `dict` at runtime.
    #[must_use]
    pub fn dunder_class(self, db: &'db dyn Db) -> Type<'db> {
        if self.is_typed_dict() {
            return KnownClass::Dict
                .to_specialized_class_type(db, [KnownClass::Str.to_instance(db), Type::object(db)])
                .map(Type::from)
                // Guard against user-customized typesheds with a broken `dict` class
                .unwrap_or_else(Type::unknown);
        }

        self.to_meta_type(db)
    }

    #[must_use]
    pub fn apply_optional_specialization(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Type<'db> {
        if let Some(specialization) = specialization {
            self.apply_specialization(db, specialization)
        } else {
            self
        }
    }

    /// Applies a specialization to this type, replacing any typevars with the types that they are
    /// specialized to.
    ///
    /// Note that this does not specialize generic classes, functions, or type aliases! That is a
    /// different operation that is performed explicitly (via a subscript operation), or implicitly
    /// via a call to the generic object.
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    pub fn apply_specialization(
        self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Type<'db> {
        self.apply_type_mapping(db, &TypeMapping::Specialization(specialization))
    }

    fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Type<'db> {
        match self {
            Type::TypeVar(bound_typevar) => match type_mapping {
                TypeMapping::Specialization(specialization) => {
                    specialization.get(db, bound_typevar).unwrap_or(self)
                }
                TypeMapping::PartialSpecialization(partial) => {
                    partial.get(db, bound_typevar).unwrap_or(self)
                }
                TypeMapping::PromoteLiterals | TypeMapping::BindLegacyTypevars(_) => self,
            }

            Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) => match type_mapping {
                TypeMapping::Specialization(_) |
                TypeMapping::PartialSpecialization(_) |
                TypeMapping::PromoteLiterals => self,
                TypeMapping::BindLegacyTypevars(binding_context) => {
                    Type::TypeVar(BoundTypeVarInstance::new(db, typevar, *binding_context))
                }
            }

            Type::FunctionLiteral(function) => {
                Type::FunctionLiteral(function.with_type_mapping(db, type_mapping))
            }

            Type::BoundMethod(method) => Type::BoundMethod(BoundMethodType::new(
                db,
                method.function(db).with_type_mapping(db, type_mapping),
                method.self_instance(db).apply_type_mapping(db, type_mapping),
            )),

            Type::NominalInstance(instance) =>
                instance.apply_type_mapping(db, type_mapping),

            Type::ProtocolInstance(instance) => {
                Type::ProtocolInstance(instance.apply_type_mapping(db, type_mapping))
            }

            Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderGet(function)) => {
                Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderGet(
                    function.with_type_mapping(db, type_mapping),
                ))
            }

            Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderCall(function)) => {
                Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderCall(
                    function.with_type_mapping(db, type_mapping),
                ))
            }

            Type::MethodWrapper(MethodWrapperKind::PropertyDunderGet(property)) => {
                Type::MethodWrapper(MethodWrapperKind::PropertyDunderGet(
                    property.apply_type_mapping(db, type_mapping),
                ))
            }

            Type::MethodWrapper(MethodWrapperKind::PropertyDunderSet(property)) => {
                Type::MethodWrapper(MethodWrapperKind::PropertyDunderSet(
                    property.apply_type_mapping(db, type_mapping),
                ))
            }

            Type::Callable(callable) => {
                Type::Callable(callable.apply_type_mapping(db, type_mapping))
            }

            Type::GenericAlias(generic) => {
                Type::GenericAlias(generic.apply_type_mapping(db, type_mapping))
            }

            Type::TypedDict(typed_dict) => {
                Type::TypedDict(typed_dict.apply_type_mapping(db, type_mapping))
            }

            Type::SubclassOf(subclass_of) => Type::SubclassOf(
                subclass_of.apply_type_mapping(db, type_mapping),
            ),

            Type::PropertyInstance(property) => {
                Type::PropertyInstance(property.apply_type_mapping(db, type_mapping))
            }

            Type::Union(union) => union.map(db, |element| {
                element.apply_type_mapping(db, type_mapping)
            }),
            Type::Intersection(intersection) => {
                let mut builder = IntersectionBuilder::new(db);
                for positive in intersection.positive(db) {
                    builder =
                        builder.add_positive(positive.apply_type_mapping(db, type_mapping));
                }
                for negative in intersection.negative(db) {
                    builder =
                        builder.add_negative(negative.apply_type_mapping(db, type_mapping));
                }
                builder.build()
            }

            Type::TypeIs(type_is) => type_is.with_type(db, type_is.return_type(db).apply_type_mapping(db, type_mapping)),

            Type::ModuleLiteral(_)
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::LiteralString
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_) => match type_mapping {
                TypeMapping::Specialization(_) |
                TypeMapping::PartialSpecialization(_) |
                TypeMapping::BindLegacyTypevars(_) => self,
                TypeMapping::PromoteLiterals => self.literal_fallback_instance(db)
                    .expect("literal type should have fallback instance type"),
            }

            Type::Dynamic(_)
            | Type::Never
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(MethodWrapperKind::StrStartswith(_))
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            // A non-generic class never needs to be specialized. A generic class is specialized
            // explicitly (via a subscript expression) or implicitly (via a call), and not because
            // some other generic context's specialization is applied to it.
            | Type::ClassLiteral(_)
            | Type::BoundSuper(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_) => self,
        }
    }

    /// Locates any legacy `TypeVar`s in this type, and adds them to a set. This is used to build
    /// up a generic context from any legacy `TypeVar`s that appear in a function parameter list or
    /// `Generic` specialization.
    pub(crate) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        match self {
            Type::TypeVar(bound_typevar) => {
                if matches!(
                    bound_typevar.typevar(db).kind(db),
                    TypeVarKind::Legacy | TypeVarKind::Implicit
                ) && binding_context.is_none_or(|binding_context| {
                    bound_typevar.binding_context(db) == BindingContext::Definition(binding_context)
                }) {
                    typevars.insert(bound_typevar);
                }
            }

            Type::FunctionLiteral(function) => {
                function.find_legacy_typevars(db, binding_context, typevars);
            }

            Type::BoundMethod(method) => {
                method
                    .self_instance(db)
                    .find_legacy_typevars(db, binding_context, typevars);
                method
                    .function(db)
                    .find_legacy_typevars(db, binding_context, typevars);
            }

            Type::MethodWrapper(
                MethodWrapperKind::FunctionTypeDunderGet(function)
                | MethodWrapperKind::FunctionTypeDunderCall(function),
            ) => {
                function.find_legacy_typevars(db, binding_context, typevars);
            }

            Type::MethodWrapper(
                MethodWrapperKind::PropertyDunderGet(property)
                | MethodWrapperKind::PropertyDunderSet(property),
            ) => {
                property.find_legacy_typevars(db, binding_context, typevars);
            }

            Type::Callable(callable) => {
                callable.find_legacy_typevars(db, binding_context, typevars);
            }

            Type::PropertyInstance(property) => {
                property.find_legacy_typevars(db, binding_context, typevars);
            }

            Type::Union(union) => {
                for element in union.iter(db) {
                    element.find_legacy_typevars(db, binding_context, typevars);
                }
            }
            Type::Intersection(intersection) => {
                for positive in intersection.positive(db) {
                    positive.find_legacy_typevars(db, binding_context, typevars);
                }
                for negative in intersection.negative(db) {
                    negative.find_legacy_typevars(db, binding_context, typevars);
                }
            }

            Type::GenericAlias(alias) => {
                alias.find_legacy_typevars(db, binding_context, typevars);
            }

            Type::NominalInstance(instance) => {
                instance.find_legacy_typevars(db, binding_context, typevars);
            }

            Type::ProtocolInstance(instance) => {
                instance.find_legacy_typevars(db, binding_context, typevars);
            }

            Type::SubclassOf(subclass_of) => {
                subclass_of.find_legacy_typevars(db, binding_context, typevars);
            }

            Type::TypeIs(type_is) => {
                type_is
                    .return_type(db)
                    .find_legacy_typevars(db, binding_context, typevars);
            }

            Type::Dynamic(_)
            | Type::Never
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(MethodWrapperKind::StrStartswith(_))
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::LiteralString
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::BoundSuper(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::TypedDict(_) => {}
        }
    }

    /// Return the string representation of this type when converted to string as it would be
    /// provided by the `__str__` method.
    ///
    /// When not available, this should fall back to the value of `[Type::repr]`.
    /// Note: this method is used in the builtins `format`, `print`, `str.format` and `f-strings`.
    #[must_use]
    pub fn str(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::IntLiteral(_) | Type::BooleanLiteral(_) => self.repr(db),
            Type::StringLiteral(_) | Type::LiteralString => *self,
            Type::EnumLiteral(enum_literal) => Type::string_literal(
                db,
                &format!(
                    "{enum_class}.{name}",
                    enum_class = enum_literal.enum_class(db).name(db),
                    name = enum_literal.name(db)
                ),
            ),
            Type::SpecialForm(special_form) => Type::string_literal(db, special_form.repr()),
            Type::KnownInstance(known_instance) => Type::StringLiteral(StringLiteralType::new(
                db,
                known_instance.repr(db).to_string().into_boxed_str(),
            )),
            // TODO: handle more complex types
            _ => KnownClass::Str.to_instance(db),
        }
    }

    /// Return the string representation of this type as it would be provided by the  `__repr__`
    /// method at runtime.
    #[must_use]
    pub fn repr(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::IntLiteral(number) => Type::string_literal(db, &number.to_string()),
            Type::BooleanLiteral(true) => Type::string_literal(db, "True"),
            Type::BooleanLiteral(false) => Type::string_literal(db, "False"),
            Type::StringLiteral(literal) => {
                Type::string_literal(db, &format!("'{}'", literal.value(db).escape_default()))
            }
            Type::LiteralString => Type::LiteralString,
            Type::SpecialForm(special_form) => Type::string_literal(db, special_form.repr()),
            Type::KnownInstance(known_instance) => Type::StringLiteral(StringLiteralType::new(
                db,
                known_instance.repr(db).to_string().into_boxed_str(),
            )),
            // TODO: handle more complex types
            _ => KnownClass::Str.to_instance(db),
        }
    }

    /// Returns where this type is defined.
    ///
    /// It's the foundation for the editor's "Go to type definition" feature
    /// where the user clicks on a value and it takes them to where the value's type is defined.
    ///
    /// This method returns `None` for unions and intersections because how these
    /// should be handled, especially when some variants don't have definitions, is
    /// specific to the call site.
    pub fn definition(&self, db: &'db dyn Db) -> Option<TypeDefinition<'db>> {
        match self {
            Self::BoundMethod(method) => {
                Some(TypeDefinition::Function(method.function(db).definition(db)))
            }
            Self::FunctionLiteral(function) => {
                Some(TypeDefinition::Function(function.definition(db)))
            }
            Self::ModuleLiteral(module) => Some(TypeDefinition::Module(module.module(db))),
            Self::ClassLiteral(class_literal) => {
                Some(TypeDefinition::Class(class_literal.definition(db)))
            }
            Self::GenericAlias(alias) => Some(TypeDefinition::Class(alias.definition(db))),
            Self::NominalInstance(instance) => {
                Some(TypeDefinition::Class(instance.class(db).definition(db)))
            }
            Self::KnownInstance(instance) => match instance {
                KnownInstanceType::TypeVar(var) => {
                    Some(TypeDefinition::TypeVar(var.definition(db)?))
                }
                KnownInstanceType::TypeAliasType(type_alias) => {
                    type_alias.definition(db).map(TypeDefinition::TypeAlias)
                }
                _ => None,
            },

            Self::SubclassOf(subclass_of_type) => match subclass_of_type.subclass_of() {
                SubclassOfInner::Class(class) => Some(TypeDefinition::Class(class.definition(db))),
                SubclassOfInner::Dynamic(_) => None,
            },

            Self::StringLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::LiteralString
            | Self::IntLiteral(_)
            | Self::BytesLiteral(_)
            // TODO: For enum literals, it would be even better to jump to the definition of the specific member
            | Self::EnumLiteral(_)
            | Self::MethodWrapper(_)
            | Self::WrapperDescriptor(_)
            | Self::DataclassDecorator(_)
            | Self::DataclassTransformer(_)
            | Self::PropertyInstance(_)
            | Self::BoundSuper(_) => self.to_meta_type(db).definition(db),

            Self::TypeVar(bound_typevar) => Some(TypeDefinition::TypeVar(bound_typevar.typevar(db).definition(db)?)),

            Self::ProtocolInstance(protocol) => match protocol.inner {
                Protocol::FromClass(class) => Some(TypeDefinition::Class(class.definition(db))),
                Protocol::Synthesized(_) => None,
            },

            Type::TypedDict(typed_dict) => {
                Some(TypeDefinition::Class(typed_dict.defining_class(db).definition(db)))
            }

            Self::Union(_) | Self::Intersection(_) => None,

            // These types have no definition
            Self::Dynamic(_)
            | Self::Never
            | Self::Callable(_)
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy
            | Self::SpecialForm(_)
            | Self::TypeIs(_) => None,
        }
    }

    /// Returns a tuple of two spans. The first is
    /// the span for the identifier of the function
    /// definition for `self`. The second is
    /// the span for the parameter in the function
    /// definition for `self`.
    ///
    /// If there are no meaningful spans, then this
    /// returns `None`. For example, when this type
    /// isn't callable.
    ///
    /// When `parameter_index` is `None`, then the
    /// second span returned covers the entire parameter
    /// list.
    ///
    /// # Performance
    ///
    /// Note that this may introduce cross-module
    /// dependencies. This can have an impact on
    /// the effectiveness of incremental caching
    /// and should therefore be used judiciously.
    ///
    /// An example of a good use case is to improve
    /// a diagnostic.
    fn parameter_span(
        &self,
        db: &'db dyn Db,
        parameter_index: Option<usize>,
    ) -> Option<(Span, Span)> {
        match *self {
            Type::FunctionLiteral(function) => function.parameter_span(db, parameter_index),
            Type::BoundMethod(bound_method) => bound_method
                .function(db)
                .parameter_span(db, parameter_index),
            _ => None,
        }
    }

    /// Returns a collection of useful spans for a
    /// function signature. These are useful for
    /// creating annotations on diagnostics.
    ///
    /// If there are no meaningful spans, then this
    /// returns `None`. For example, when this type
    /// isn't callable.
    ///
    /// # Performance
    ///
    /// Note that this may introduce cross-module
    /// dependencies. This can have an impact on
    /// the effectiveness of incremental caching
    /// and should therefore be used judiciously.
    ///
    /// An example of a good use case is to improve
    /// a diagnostic.
    fn function_spans(&self, db: &'db dyn Db) -> Option<FunctionSpans> {
        match *self {
            Type::FunctionLiteral(function) => function.spans(db),
            Type::BoundMethod(bound_method) => bound_method.function(db).spans(db),
            _ => None,
        }
    }

    pub(crate) fn generic_origin(self, db: &'db dyn Db) -> Option<ClassLiteral<'db>> {
        match self {
            Type::GenericAlias(generic) => Some(generic.origin(db)),
            Type::NominalInstance(instance) => {
                if let ClassType::Generic(generic) = instance.class(db) {
                    Some(generic.origin(db))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl<'db> From<&Type<'db>> for Type<'db> {
    fn from(value: &Type<'db>) -> Self {
        *value
    }
}

/// A mapping that can be applied to a type, producing another type. This is applied inductively to
/// the components of complex types.
///
/// This is represented as an enum (with some variants using `Cow`), and not an `FnMut` trait,
/// since we sometimes have to apply type mappings lazily (e.g., to the signature of a function
/// literal).
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub enum TypeMapping<'a, 'db> {
    /// Applies a specialization to the type
    Specialization(Specialization<'db>),
    /// Applies a partial specialization to the type
    PartialSpecialization(PartialSpecialization<'a, 'db>),
    /// Promotes any literal types to their corresponding instance types (e.g. `Literal["string"]`
    /// to `str`)
    PromoteLiterals,
    /// Binds a legacy typevar with the generic context (class, function, type alias) that it is
    /// being used in.
    BindLegacyTypevars(BindingContext<'db>),
}

fn walk_type_mapping<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    mapping: &TypeMapping<'_, 'db>,
    visitor: &V,
) {
    match mapping {
        TypeMapping::Specialization(specialization) => {
            walk_specialization(db, *specialization, visitor);
        }
        TypeMapping::PartialSpecialization(specialization) => {
            walk_partial_specialization(db, specialization, visitor);
        }
        TypeMapping::PromoteLiterals | TypeMapping::BindLegacyTypevars(_) => {}
    }
}

impl<'db> TypeMapping<'_, 'db> {
    fn to_owned(&self) -> TypeMapping<'db, 'db> {
        match self {
            TypeMapping::Specialization(specialization) => {
                TypeMapping::Specialization(*specialization)
            }
            TypeMapping::PartialSpecialization(partial) => {
                TypeMapping::PartialSpecialization(partial.to_owned())
            }
            TypeMapping::PromoteLiterals => TypeMapping::PromoteLiterals,
            TypeMapping::BindLegacyTypevars(binding_context) => {
                TypeMapping::BindLegacyTypevars(*binding_context)
            }
        }
    }

    fn normalized_impl(&self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        match self {
            TypeMapping::Specialization(specialization) => {
                TypeMapping::Specialization(specialization.normalized_impl(db, visitor))
            }
            TypeMapping::PartialSpecialization(partial) => {
                TypeMapping::PartialSpecialization(partial.normalized_impl(db, visitor))
            }
            TypeMapping::PromoteLiterals => TypeMapping::PromoteLiterals,
            TypeMapping::BindLegacyTypevars(binding_context) => {
                TypeMapping::BindLegacyTypevars(*binding_context)
            }
        }
    }
}

/// Singleton types that are heavily special-cased by ty. Despite its name,
/// quite a different type to [`NominalInstanceType`].
///
/// In many ways, this enum behaves similarly to [`SpecialFormType`].
/// Unlike instances of that variant, however, `Type::KnownInstance`s do not exist
/// at a location that can be known prior to any analysis by ty, and each variant
/// of `KnownInstanceType` can have multiple instances (as, unlike `SpecialFormType`,
/// `KnownInstanceType` variants can hold associated data). Instances of this type
/// are generally created by operations at runtime in some way, such as a type alias
/// statement, a typevar definition, or an instance of `Generic[T]` in a class's
/// bases list.
///
/// # Ordering
///
/// Ordering between variants is stable and should be the same between runs.
/// Ordering within variants is based on the wrapped data's salsa-assigned id and not on its values.
/// The id may change between runs, or when e.g. a `TypeVarInstance` was garbage-collected and recreated.
#[derive(
    Copy, Clone, Debug, Eq, Hash, PartialEq, salsa::Update, Ord, PartialOrd, get_size2::GetSize,
)]
pub enum KnownInstanceType<'db> {
    /// The type of `Protocol[T]`, `Protocol[U, S]`, etc -- usually only found in a class's bases list.
    ///
    /// Note that unsubscripted `Protocol` is represented by [`SpecialFormType::Protocol`], not this type.
    SubscriptedProtocol(GenericContext<'db>),

    /// The type of `Generic[T]`, `Generic[U, S]`, etc -- usually only found in a class's bases list.
    ///
    /// Note that unsubscripted `Generic` is represented by [`SpecialFormType::Generic`], not this type.
    SubscriptedGeneric(GenericContext<'db>),

    /// A single instance of `typing.TypeVar`
    TypeVar(TypeVarInstance<'db>),

    /// A single instance of `typing.TypeAliasType` (PEP 695 type alias)
    TypeAliasType(TypeAliasType<'db>),

    /// A single instance of `warnings.deprecated` or `typing_extensions.deprecated`
    Deprecated(DeprecatedInstance<'db>),

    /// A single instance of `dataclasses.Field`
    Field(FieldInstance<'db>),
}

fn walk_known_instance_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    known_instance: KnownInstanceType<'db>,
    visitor: &V,
) {
    match known_instance {
        KnownInstanceType::SubscriptedProtocol(context)
        | KnownInstanceType::SubscriptedGeneric(context) => {
            walk_generic_context(db, context, visitor);
        }
        KnownInstanceType::TypeVar(typevar) => {
            visitor.visit_type_var_type(db, typevar);
        }
        KnownInstanceType::TypeAliasType(type_alias) => {
            visitor.visit_type_alias_type(db, type_alias);
        }
        KnownInstanceType::Deprecated(_) => {
            // Nothing to visit
        }
        KnownInstanceType::Field(field) => {
            visitor.visit_type(db, field.default_type(db));
        }
    }
}

impl<'db> KnownInstanceType<'db> {
    fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        match self {
            Self::SubscriptedProtocol(context) => {
                Self::SubscriptedProtocol(context.normalized_impl(db, visitor))
            }
            Self::SubscriptedGeneric(context) => {
                Self::SubscriptedGeneric(context.normalized_impl(db, visitor))
            }
            Self::TypeVar(typevar) => Self::TypeVar(typevar.normalized_impl(db, visitor)),
            Self::TypeAliasType(type_alias) => {
                Self::TypeAliasType(type_alias.normalized_impl(db, visitor))
            }
            Self::Deprecated(deprecated) => {
                // Nothing to normalize
                Self::Deprecated(deprecated)
            }
            Self::Field(field) => Self::Field(field.normalized_impl(db, visitor)),
        }
    }

    const fn class(self) -> KnownClass {
        match self {
            Self::SubscriptedProtocol(_) | Self::SubscriptedGeneric(_) => KnownClass::SpecialForm,
            Self::TypeVar(_) => KnownClass::TypeVar,
            Self::TypeAliasType(_) => KnownClass::TypeAliasType,
            Self::Deprecated(_) => KnownClass::Deprecated,
            Self::Field(_) => KnownClass::Field,
        }
    }

    fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        self.class().to_class_literal(db)
    }

    /// Return the instance type which this type is a subtype of.
    ///
    /// For example, an alias created using the `type` statement is an instance of
    /// `typing.TypeAliasType`, so `KnownInstanceType::TypeAliasType(_).instance_fallback(db)`
    /// returns `Type::NominalInstance(NominalInstanceType { class: <typing.TypeAliasType> })`.
    fn instance_fallback(self, db: &dyn Db) -> Type<'_> {
        self.class().to_instance(db)
    }

    /// Return `true` if this symbol is an instance of `class`.
    fn is_instance_of(self, db: &dyn Db, class: ClassType) -> bool {
        self.class().is_subclass_of(db, class)
    }

    /// Return the repr of the symbol at runtime
    fn repr(self, db: &'db dyn Db) -> impl std::fmt::Display + 'db {
        struct KnownInstanceRepr<'db> {
            known_instance: KnownInstanceType<'db>,
            db: &'db dyn Db,
        }

        impl std::fmt::Display for KnownInstanceRepr<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.known_instance {
                    KnownInstanceType::SubscriptedProtocol(generic_context) => {
                        f.write_str("typing.Protocol")?;
                        generic_context.display(self.db).fmt(f)
                    }
                    KnownInstanceType::SubscriptedGeneric(generic_context) => {
                        f.write_str("typing.Generic")?;
                        generic_context.display(self.db).fmt(f)
                    }
                    KnownInstanceType::TypeAliasType(_) => f.write_str("typing.TypeAliasType"),
                    // This is a legacy `TypeVar` _outside_ of any generic class or function, so we render
                    // it as an instance of `typing.TypeVar`. Inside of a generic class or function, we'll
                    // have a `Type::TypeVar(_)`, which is rendered as the typevar's name.
                    KnownInstanceType::TypeVar(typevar) => {
                        write!(f, "typing.TypeVar({})", typevar.display(self.db))
                    }
                    KnownInstanceType::Deprecated(_) => f.write_str("warnings.deprecated"),
                    KnownInstanceType::Field(field) => {
                        f.write_str("dataclasses.Field[")?;
                        field.default_type(self.db).display(self.db).fmt(f)?;
                        f.write_str("]")
                    }
                }
            }
        }

        KnownInstanceRepr {
            known_instance: self,
            db,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub enum DynamicType {
    /// An explicitly annotated `typing.Any`
    Any,
    /// An unannotated value, or a dynamic type resulting from an error
    Unknown,
    /// Temporary type for symbols that can't be inferred yet because of missing implementations.
    ///
    /// This variant should eventually be removed once ty is spec-compliant.
    ///
    /// General rule: `Todo` should only propagate when the presence of the input `Todo` caused the
    /// output to be unknown. An output should only be `Todo` if fixing all `Todo` inputs to be not
    /// `Todo` would change the output type.
    ///
    /// This variant should be created with the `todo_type!` macro.
    Todo(TodoType),
    /// A special Todo-variant for PEP-695 `ParamSpec` types. A temporary variant to detect and special-
    /// case the handling of these types in `Callable` annotations.
    TodoPEP695ParamSpec,
    /// A special Todo-variant for type aliases declared using `typing.TypeAlias`.
    /// A temporary variant to detect and special-case the handling of these aliases in autocomplete suggestions.
    TodoTypeAlias,
    /// A special Todo-variant for `Unpack[Ts]`, so that we can treat it specially in `Generic[Unpack[Ts]]`
    TodoUnpack,
}

impl DynamicType {
    #[expect(clippy::unused_self)]
    fn normalized(self) -> Self {
        Self::Any
    }
}

impl std::fmt::Display for DynamicType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DynamicType::Any => f.write_str("Any"),
            DynamicType::Unknown => f.write_str("Unknown"),
            // `DynamicType::Todo`'s display should be explicit that is not a valid display of
            // any other type
            DynamicType::Todo(todo) => write!(f, "@Todo{todo}"),
            DynamicType::TodoPEP695ParamSpec => {
                if cfg!(debug_assertions) {
                    f.write_str("@Todo(ParamSpec)")
                } else {
                    f.write_str("@Todo")
                }
            }
            DynamicType::TodoUnpack => {
                if cfg!(debug_assertions) {
                    f.write_str("@Todo(typing.Unpack)")
                } else {
                    f.write_str("@Todo")
                }
            }
            DynamicType::TodoTypeAlias => {
                if cfg!(debug_assertions) {
                    f.write_str("@Todo(Support for `typing.TypeAlias`)")
                } else {
                    f.write_str("@Todo")
                }
            }
        }
    }
}

bitflags! {
    /// Type qualifiers that appear in an annotation expression.
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Default, salsa::Update, Hash)]
    pub(crate) struct TypeQualifiers: u8 {
        /// `typing.ClassVar`
        const CLASS_VAR = 1 << 0;
        /// `typing.Final`
        const FINAL     = 1 << 1;
        /// `dataclasses.InitVar`
        const INIT_VAR  = 1 << 2;
    }
}

impl get_size2::GetSize for TypeQualifiers {}

impl TypeQualifiers {
    /// Get the name of a type qualifier.
    ///
    /// Note that this function can only be called on sets with a single member.
    /// Panics if more than a single bit is set.
    fn name(self) -> &'static str {
        match self {
            Self::CLASS_VAR => "ClassVar",
            Self::FINAL => "Final",
            Self::INIT_VAR => "InitVar",
            _ => {
                unreachable!("Only a single bit should be set when calling `TypeQualifiers::name`")
            }
        }
    }
}

/// When inferring the type of an annotation expression, we can also encounter type qualifiers
/// such as `ClassVar` or `Final`. These do not affect the inferred type itself, but rather
/// control how a particular place can be accessed or modified. This struct holds a type and
/// a set of type qualifiers.
///
/// Example: `Annotated[ClassVar[tuple[int]], "metadata"]` would have type `tuple[int]` and the
/// qualifier `ClassVar`.
#[derive(Clone, Debug, Copy, Eq, PartialEq, salsa::Update, get_size2::GetSize)]
pub(crate) struct TypeAndQualifiers<'db> {
    inner: Type<'db>,
    qualifiers: TypeQualifiers,
}

impl<'db> TypeAndQualifiers<'db> {
    pub(crate) fn new(inner: Type<'db>, qualifiers: TypeQualifiers) -> Self {
        Self { inner, qualifiers }
    }

    /// Constructor that creates a [`TypeAndQualifiers`] instance with type `Unknown` and no qualifiers.
    pub(crate) fn unknown() -> Self {
        Self {
            inner: Type::unknown(),
            qualifiers: TypeQualifiers::empty(),
        }
    }

    /// Forget about type qualifiers and only return the inner type.
    pub(crate) fn inner_type(&self) -> Type<'db> {
        self.inner
    }

    /// Insert/add an additional type qualifier.
    pub(crate) fn add_qualifier(&mut self, qualifier: TypeQualifiers) {
        self.qualifiers |= qualifier;
    }

    /// Return the set of type qualifiers.
    pub(crate) fn qualifiers(&self) -> TypeQualifiers {
        self.qualifiers
    }
}

impl<'db> From<Type<'db>> for TypeAndQualifiers<'db> {
    fn from(inner: Type<'db>) -> Self {
        Self {
            inner,
            qualifiers: TypeQualifiers::empty(),
        }
    }
}

/// Error struct providing information on type(s) that were deemed to be invalid
/// in a type expression context, and the type we should therefore fallback to
/// for the problematic type expression.
#[derive(Debug, PartialEq, Eq)]
pub struct InvalidTypeExpressionError<'db> {
    fallback_type: Type<'db>,
    invalid_expressions: smallvec::SmallVec<[InvalidTypeExpression<'db>; 1]>,
}

impl<'db> InvalidTypeExpressionError<'db> {
    fn into_fallback_type(
        self,
        context: &InferContext,
        node: &ast::Expr,
        is_reachable: bool,
    ) -> Type<'db> {
        let InvalidTypeExpressionError {
            fallback_type,
            invalid_expressions,
        } = self;
        if is_reachable {
            for error in invalid_expressions {
                let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, node) else {
                    continue;
                };
                let diagnostic = builder.into_diagnostic(error.reason(context.db()));
                error.add_subdiagnostics(context.db(), diagnostic);
            }
        }
        fallback_type
    }
}

/// Enumeration of various types that are invalid in type-expression contexts
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum InvalidTypeExpression<'db> {
    /// Some types always require exactly one argument when used in a type expression
    RequiresOneArgument(Type<'db>),
    /// Some types always require at least one argument when used in a type expression
    RequiresArguments(Type<'db>),
    /// Some types always require at least two arguments when used in a type expression
    RequiresTwoArguments(Type<'db>),
    /// The `Protocol` class is invalid in type expressions
    Protocol,
    /// Same for `Generic`
    Generic,
    /// Same for `@deprecated`
    Deprecated,
    /// Same for `dataclasses.Field`
    Field,
    /// Same for `typing.TypedDict`
    TypedDict,
    /// Type qualifiers are always invalid in *type expressions*,
    /// but these ones are okay with 0 arguments in *annotation expressions*
    TypeQualifier(SpecialFormType),
    /// Type qualifiers that are invalid in type expressions,
    /// and which would require exactly one argument even if they appeared in an annotation expression
    TypeQualifierRequiresOneArgument(SpecialFormType),
    /// Some types are always invalid in type expressions
    InvalidType(Type<'db>, ScopeId<'db>),
}

impl<'db> InvalidTypeExpression<'db> {
    const fn reason(self, db: &'db dyn Db) -> impl std::fmt::Display + 'db {
        struct Display<'db> {
            error: InvalidTypeExpression<'db>,
            db: &'db dyn Db,
        }

        impl std::fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.error {
                    InvalidTypeExpression::RequiresOneArgument(ty) => write!(
                        f,
                        "`{ty}` requires exactly one argument when used in a type expression",
                        ty = ty.display(self.db)
                    ),
                    InvalidTypeExpression::RequiresArguments(ty) => write!(
                        f,
                        "`{ty}` requires at least one argument when used in a type expression",
                        ty = ty.display(self.db)
                    ),
                    InvalidTypeExpression::RequiresTwoArguments(ty) => write!(
                        f,
                        "`{ty}` requires at least two arguments when used in a type expression",
                        ty = ty.display(self.db)
                    ),
                    InvalidTypeExpression::Protocol => {
                        f.write_str("`typing.Protocol` is not allowed in type expressions")
                    }
                    InvalidTypeExpression::Generic => {
                        f.write_str("`typing.Generic` is not allowed in type expressions")
                    }
                    InvalidTypeExpression::Deprecated => {
                        f.write_str("`warnings.deprecated` is not allowed in type expressions")
                    }
                    InvalidTypeExpression::Field => {
                        f.write_str("`dataclasses.Field` is not allowed in type expressions")
                    }
                    InvalidTypeExpression::TypedDict => {
                        f.write_str(
                            "The special form `typing.TypedDict` is not allowed in type expressions. \
                            Did you mean to use a concrete TypedDict or `collections.abc.Mapping[str, object]` instead?")
                    }
                    InvalidTypeExpression::TypeQualifier(qualifier) => write!(
                        f,
                        "Type qualifier `{qualifier}` is not allowed in type expressions \
                        (only in annotation expressions)",
                    ),
                    InvalidTypeExpression::TypeQualifierRequiresOneArgument(qualifier) => write!(
                        f,
                        "Type qualifier `{qualifier}` is not allowed in type expressions \
                        (only in annotation expressions, and only with exactly one argument)",
                    ),
                    InvalidTypeExpression::InvalidType(ty, _) => write!(
                        f,
                        "Variable of type `{ty}` is not allowed in a type expression",
                        ty = ty.display(self.db)
                    ),
                }
            }
        }

        Display { error: self, db }
    }

    fn add_subdiagnostics(self, db: &'db dyn Db, mut diagnostic: LintDiagnosticGuard) {
        let InvalidTypeExpression::InvalidType(ty, scope) = self else {
            return;
        };
        let Type::ModuleLiteral(module_type) = ty else {
            return;
        };
        let module = module_type.module(db);
        let Some(module_name_final_part) = module.name(db).components().next_back() else {
            return;
        };
        let Some(module_member_with_same_name) = ty
            .member(db, module_name_final_part)
            .place
            .ignore_possibly_unbound()
        else {
            return;
        };
        if module_member_with_same_name
            .in_type_expression(db, scope, None)
            .is_err()
        {
            return;
        }

        // TODO: showing a diff (and even having an autofix) would be even better
        diagnostic.info(format_args!(
            "Did you mean to use the module's member \
            `{module_name_final_part}.{module_name_final_part}` instead?"
        ));
    }
}

/// Data regarding a `warnings.deprecated` or `typing_extensions.deprecated` decorator.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct DeprecatedInstance<'db> {
    /// The message for the deprecation
    pub message: Option<StringLiteralType<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for DeprecatedInstance<'_> {}

/// Contains information about instances of `dataclasses.Field`, typically created using
/// `dataclasses.field()`.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct FieldInstance<'db> {
    /// The type of the default value for this field. This is derived from the `default` or
    /// `default_factory` arguments to `dataclasses.field()`.
    pub default_type: Type<'db>,

    /// Whether this field is part of the `__init__` signature, or not.
    pub init: bool,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for FieldInstance<'_> {}

impl<'db> FieldInstance<'db> {
    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        FieldInstance::new(
            db,
            self.default_type(db).normalized_impl(db, visitor),
            self.init(db),
        )
    }
}

/// Whether this typevar was created via the legacy `TypeVar` constructor, using PEP 695 syntax,
/// or an implicit typevar like `Self` was used.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub enum TypeVarKind {
    /// `T = TypeVar("T")`
    Legacy,
    /// `def foo[T](x: T) -> T: ...`
    Pep695,
    /// `typing.Self`
    Implicit,
}

/// A type variable that has not been bound to a generic context yet.
///
/// This is usually not the type that you want; if you are working with a typevar, in a generic
/// context, which might be specialized to a concrete type, you want [`BoundTypeVarInstance`]. This
/// type holds information that does not depend on which generic context the typevar is used in.
///
/// For a legacy typevar:
///
/// ```py
/// T = TypeVar("T")                       # [1]
/// def generic_function(t: T) -> T: ...   # [2]
/// ```
///
/// we will create a `TypeVarInstance` for the typevar `T` when it is instantiated. The type of `T`
/// at `[1]` will be a `KnownInstanceType::TypeVar` wrapping this `TypeVarInstance`. The typevar is
/// not yet bound to any generic context at this point.
///
/// The typevar is used in `generic_function`, which binds it to a new generic context. We will
/// create a [`BoundTypeVarInstance`] for this new binding of the typevar. The type of `T` at `[2]`
/// will be a `Type::TypeVar` wrapping this `BoundTypeVarInstance`.
///
/// For a PEP 695 typevar:
///
/// ```py
/// def generic_function[T](t: T) -> T: ...
/// #                          ╰─────╰─────────── [2]
/// #                    ╰─────────────────────── [1]
/// ```
///
/// the typevar is defined and immediately bound to a single generic context. Just like in the
/// legacy case, we will create a `TypeVarInstance` and [`BoundTypeVarInstance`], and the type of
/// `T` at `[1]` and `[2]` will be that `TypeVarInstance` and `BoundTypeVarInstance`, respectively.
///
/// # Ordering
/// Ordering is based on the type var instance's salsa-assigned id and not on its values.
/// The id may change between runs, or when the type var instance was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct TypeVarInstance<'db> {
    /// The name of this TypeVar (e.g. `T`)
    #[returns(ref)]
    name: ast::name::Name,

    /// The type var's definition (None if synthesized)
    pub definition: Option<Definition<'db>>,

    /// The upper bound or constraint on the type of this TypeVar
    bound_or_constraints: Option<TypeVarBoundOrConstraints<'db>>,

    /// The variance of the TypeVar
    variance: TypeVarVariance,

    /// The default type for this TypeVar
    default_ty: Option<Type<'db>>,

    pub kind: TypeVarKind,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypeVarInstance<'_> {}

fn walk_type_var_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    typevar: TypeVarInstance<'db>,
    visitor: &V,
) {
    if let Some(bounds) = typevar.bound_or_constraints(db) {
        walk_type_var_bounds(db, bounds, visitor);
    }
    if let Some(default_type) = typevar.default_ty(db) {
        visitor.visit_type(db, default_type);
    }
}

impl<'db> TypeVarInstance<'db> {
    pub(crate) fn with_binding_context(
        self,
        db: &'db dyn Db,
        binding_context: Definition<'db>,
    ) -> BoundTypeVarInstance<'db> {
        BoundTypeVarInstance::new(db, self, BindingContext::Definition(binding_context))
    }

    pub(crate) fn is_implicit(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), TypeVarKind::Implicit)
    }

    pub(crate) fn upper_bound(self, db: &'db dyn Db) -> Option<Type<'db>> {
        if let Some(TypeVarBoundOrConstraints::UpperBound(ty)) = self.bound_or_constraints(db) {
            Some(ty)
        } else {
            None
        }
    }

    pub(crate) fn constraints(self, db: &'db dyn Db) -> Option<&'db [Type<'db>]> {
        if let Some(TypeVarBoundOrConstraints::Constraints(tuple)) = self.bound_or_constraints(db) {
            Some(tuple.elements(db))
        } else {
            None
        }
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        Self::new(
            db,
            self.name(db),
            self.definition(db),
            self.bound_or_constraints(db)
                .map(|b| b.normalized_impl(db, visitor)),
            self.variance(db),
            self.default_ty(db).map(|d| d.normalized_impl(db, visitor)),
            self.kind(db),
        )
    }

    fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self::new(
            db,
            self.name(db),
            self.definition(db),
            self.bound_or_constraints(db)
                .map(|b| b.materialize(db, variance)),
            self.variance(db),
            self.default_ty(db),
            self.kind(db),
        )
    }
}

/// Where a type variable is bound and usable.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub enum BindingContext<'db> {
    /// The definition of the generic class, function, or type alias that binds this typevar.
    Definition(Definition<'db>),
    /// The typevar is synthesized internally, and is not associated with a particular definition
    /// in the source, but is still bound and eligible for specialization inference.
    Synthetic,
}

impl<'db> BindingContext<'db> {
    fn name(self, db: &'db dyn Db) -> Option<String> {
        match self {
            BindingContext::Definition(definition) => definition.name(db),
            BindingContext::Synthetic => None,
        }
    }
}

/// A type variable that has been bound to a generic context, and which can be specialized to a
/// concrete type.
#[salsa::interned(debug)]
#[derive(PartialOrd, Ord)]
pub struct BoundTypeVarInstance<'db> {
    pub typevar: TypeVarInstance<'db>,
    binding_context: BindingContext<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BoundTypeVarInstance<'_> {}

fn walk_bound_type_var_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    bound_typevar: BoundTypeVarInstance<'db>,
    visitor: &V,
) {
    visitor.visit_type_var_type(db, bound_typevar.typevar(db));
}

impl<'db> BoundTypeVarInstance<'db> {
    /// Returns the default value of this typevar, recursively applying its binding context to any
    /// other typevars that appear in the default.
    ///
    /// For instance, in
    ///
    /// ```py
    /// T = TypeVar("T")
    /// U = TypeVar("U", default=T)
    ///
    /// # revealed: typing.TypeVar[U = typing.TypeVar[T]]
    /// reveal_type(U)
    ///
    /// # revealed: typing.Generic[T, U = T@C]
    /// class C(reveal_type(Generic[T, U])): ...
    /// ```
    ///
    /// In the first case, the use of `U` is unbound, and so we have a [`TypeVarInstance`], and its
    /// default value (`T`) is also unbound.
    ///
    /// By using `U` in the generic class, it becomes bound, and so we have a
    /// `BoundTypeVarInstance`. As part of binding `U` we must also bind its default value
    /// (resulting in `T@C`).
    pub(crate) fn default_ty(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let binding_context = self.binding_context(db);
        self.typevar(db)
            .default_ty(db)
            .map(|ty| ty.apply_type_mapping(db, &TypeMapping::BindLegacyTypevars(binding_context)))
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        Self::new(
            db,
            self.typevar(db).normalized_impl(db, visitor),
            self.binding_context(db),
        )
    }

    fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self::new(
            db,
            self.typevar(db).materialize(db, variance),
            self.binding_context(db),
        )
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub enum TypeVarVariance {
    Invariant,
    Covariant,
    Contravariant,
    Bivariant,
}

impl TypeVarVariance {
    /// Flips the polarity of the variance.
    ///
    /// Covariant becomes contravariant, contravariant becomes covariant, others remain unchanged.
    pub(crate) const fn flip(self) -> Self {
        match self {
            TypeVarVariance::Invariant => TypeVarVariance::Invariant,
            TypeVarVariance::Covariant => TypeVarVariance::Contravariant,
            TypeVarVariance::Contravariant => TypeVarVariance::Covariant,
            TypeVarVariance::Bivariant => TypeVarVariance::Bivariant,
        }
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub enum TypeVarBoundOrConstraints<'db> {
    UpperBound(Type<'db>),
    Constraints(UnionType<'db>),
}

fn walk_type_var_bounds<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    bounds: TypeVarBoundOrConstraints<'db>,
    visitor: &V,
) {
    match bounds {
        TypeVarBoundOrConstraints::UpperBound(bound) => visitor.visit_type(db, bound),
        TypeVarBoundOrConstraints::Constraints(constraints) => {
            visitor.visit_union_type(db, constraints);
        }
    }
}

impl<'db> TypeVarBoundOrConstraints<'db> {
    fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        match self {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                TypeVarBoundOrConstraints::UpperBound(bound.normalized_impl(db, visitor))
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                TypeVarBoundOrConstraints::Constraints(constraints.normalized_impl(db, visitor))
            }
        }
    }

    fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        match self {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                TypeVarBoundOrConstraints::UpperBound(bound.materialize(db, variance))
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                TypeVarBoundOrConstraints::Constraints(UnionType::new(
                    db,
                    constraints
                        .elements(db)
                        .iter()
                        .map(|ty| ty.materialize(db, variance))
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ))
            }
        }
    }
}

/// Error returned if a type is not (or may not be) a context manager.
#[derive(Debug)]
enum ContextManagerError<'db> {
    Enter(CallDunderError<'db>, EvaluationMode),
    Exit {
        enter_return_type: Type<'db>,
        exit_error: CallDunderError<'db>,
        mode: EvaluationMode,
    },
    EnterAndExit {
        enter_error: CallDunderError<'db>,
        exit_error: CallDunderError<'db>,
        mode: EvaluationMode,
    },
}

impl<'db> ContextManagerError<'db> {
    fn fallback_enter_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.enter_type(db).unwrap_or(Type::unknown())
    }

    /// Returns the `__enter__` or `__aenter__` return type if it is known,
    /// or `None` if the type never has a callable `__enter__` or `__aenter__` attribute
    fn enter_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::Exit {
                enter_return_type,
                exit_error: _,
                mode: _,
            } => Some(*enter_return_type),
            Self::Enter(enter_error, _)
            | Self::EnterAndExit {
                enter_error,
                exit_error: _,
                mode: _,
            } => match enter_error {
                CallDunderError::PossiblyUnbound(call_outcome) => {
                    Some(call_outcome.return_type(db))
                }
                CallDunderError::CallError(CallErrorKind::NotCallable, _) => None,
                CallDunderError::CallError(_, bindings) => Some(bindings.return_type(db)),
                CallDunderError::MethodNotAvailable => None,
            },
        }
    }

    fn report_diagnostic(
        &self,
        context: &InferContext<'db, '_>,
        context_expression_type: Type<'db>,
        context_expression_node: ast::AnyNodeRef,
    ) {
        let Some(builder) = context.report_lint(&INVALID_CONTEXT_MANAGER, context_expression_node)
        else {
            return;
        };

        let mode = match self {
            Self::Exit { mode, .. } | Self::Enter(_, mode) | Self::EnterAndExit { mode, .. } => {
                *mode
            }
        };

        let (enter_method, exit_method) = match mode {
            EvaluationMode::Async => ("__aenter__", "__aexit__"),
            EvaluationMode::Sync => ("__enter__", "__exit__"),
        };

        let format_call_dunder_error = |call_dunder_error: &CallDunderError<'db>, name: &str| {
            match call_dunder_error {
                CallDunderError::MethodNotAvailable => format!("it does not implement `{name}`"),
                CallDunderError::PossiblyUnbound(_) => {
                    format!("the method `{name}` is possibly unbound")
                }
                // TODO: Use more specific error messages for the different error cases.
                //  E.g. hint toward the union variant that doesn't correctly implement enter,
                //  distinguish between a not callable `__enter__` attribute and a wrong signature.
                CallDunderError::CallError(_, _) => {
                    format!("it does not correctly implement `{name}`")
                }
            }
        };

        let format_call_dunder_errors = |error_a: &CallDunderError<'db>,
                                         name_a: &str,
                                         error_b: &CallDunderError<'db>,
                                         name_b: &str| {
            match (error_a, error_b) {
                (CallDunderError::PossiblyUnbound(_), CallDunderError::PossiblyUnbound(_)) => {
                    format!("the methods `{name_a}` and `{name_b}` are possibly unbound")
                }
                (CallDunderError::MethodNotAvailable, CallDunderError::MethodNotAvailable) => {
                    format!("it does not implement `{name_a}` and `{name_b}`")
                }
                (CallDunderError::CallError(_, _), CallDunderError::CallError(_, _)) => {
                    format!("it does not correctly implement `{name_a}` or `{name_b}`")
                }
                (_, _) => format!(
                    "{format_a}, and {format_b}",
                    format_a = format_call_dunder_error(error_a, name_a),
                    format_b = format_call_dunder_error(error_b, name_b)
                ),
            }
        };

        let db = context.db();

        let formatted_errors = match self {
            Self::Exit {
                enter_return_type: _,
                exit_error,
                mode: _,
            } => format_call_dunder_error(exit_error, exit_method),
            Self::Enter(enter_error, _) => format_call_dunder_error(enter_error, enter_method),
            Self::EnterAndExit {
                enter_error,
                exit_error,
                mode: _,
            } => format_call_dunder_errors(enter_error, enter_method, exit_error, exit_method),
        };

        // Suggest using `async with` if only async methods are available in a sync context,
        // or suggest using `with` if only sync methods are available in an async context.
        let with_kw = match mode {
            EvaluationMode::Sync => "with",
            EvaluationMode::Async => "async with",
        };

        let mut diag = builder.into_diagnostic(format_args!(
            "Object of type `{}` cannot be used with `{}` because {}",
            context_expression_type.display(db),
            with_kw,
            formatted_errors,
        ));

        let (alt_mode, alt_enter_method, alt_exit_method, alt_with_kw) = match mode {
            EvaluationMode::Sync => ("async", "__aenter__", "__aexit__", "async with"),
            EvaluationMode::Async => ("sync", "__enter__", "__exit__", "with"),
        };

        let alt_enter =
            context_expression_type.try_call_dunder(db, alt_enter_method, CallArguments::none());
        let alt_exit = context_expression_type.try_call_dunder(
            db,
            alt_exit_method,
            CallArguments::positional([Type::unknown(), Type::unknown(), Type::unknown()]),
        );

        if (alt_enter.is_ok() || matches!(alt_enter, Err(CallDunderError::CallError(..))))
            && (alt_exit.is_ok() || matches!(alt_exit, Err(CallDunderError::CallError(..))))
        {
            diag.info(format_args!(
                "Objects of type `{}` can be used as {} context managers",
                context_expression_type.display(db),
                alt_mode
            ));
            diag.info(format!("Consider using `{alt_with_kw}` here"));
        }
    }
}

/// Error returned if a type is not (or may not be) iterable.
#[derive(Debug)]
enum IterationError<'db> {
    /// The object being iterated over has a bound `__(a)iter__` method,
    /// but calling it with the expected arguments results in an error.
    IterCallError {
        kind: CallErrorKind,
        bindings: Box<Bindings<'db>>,
        mode: EvaluationMode,
    },

    /// The object being iterated over has a bound `__(a)iter__` method that can be called
    /// with the expected types, but it returns an object that is not a valid iterator.
    IterReturnsInvalidIterator {
        /// The type of the object returned by the `__(a)iter__` method.
        iterator: Type<'db>,
        /// The error we encountered when we tried to call `__(a)next__` on the type
        /// returned by `__(a)iter__`
        dunder_error: CallDunderError<'db>,
        /// Whether this is a synchronous or an asynchronous iterator.
        mode: EvaluationMode,
    },

    /// The object being iterated over has a bound `__iter__` method that returns a
    /// valid iterator. However, the `__iter__` method is possibly unbound, and there
    /// either isn't a `__getitem__` method to fall back to, or calling the `__getitem__`
    /// method returns some kind of error.
    PossiblyUnboundIterAndGetitemError {
        /// The type of the object returned by the `__next__` method on the iterator.
        /// (The iterator being the type returned by the `__iter__` method on the iterable.)
        dunder_next_return: Type<'db>,
        /// The error we encountered when we tried to call `__getitem__` on the iterable.
        dunder_getitem_error: CallDunderError<'db>,
    },

    /// The object being iterated over doesn't have an `__iter__` method.
    /// It also either doesn't have a `__getitem__` method to fall back to,
    /// or calling the `__getitem__` method returns some kind of error.
    UnboundIterAndGetitemError {
        dunder_getitem_error: CallDunderError<'db>,
    },

    /// The asynchronous iterable has no `__aiter__` method.
    UnboundAiterError,
}

impl<'db> IterationError<'db> {
    fn fallback_element_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.element_type(db).unwrap_or(Type::unknown())
    }

    /// Returns the element type if it is known, or `None` if the type is never iterable.
    fn element_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        let return_type = |result: Result<Bindings<'db>, CallDunderError<'db>>| {
            result
                .map(|outcome| Some(outcome.return_type(db)))
                .unwrap_or_else(|call_error| call_error.return_type(db))
        };

        match self {
            Self::IterReturnsInvalidIterator {
                dunder_error, mode, ..
            } => dunder_error.return_type(db).map(|ty| {
                if mode.is_async() {
                    ty.resolve_await(db)
                } else {
                    ty
                }
            }),

            Self::IterCallError {
                kind: _,
                bindings: dunder_iter_bindings,
                mode,
            } => {
                if mode.is_async() {
                    return_type(dunder_iter_bindings.return_type(db).try_call_dunder(
                        db,
                        "__anext__",
                        CallArguments::none(),
                    ))
                    .map(|ty| ty.resolve_await(db))
                } else {
                    return_type(dunder_iter_bindings.return_type(db).try_call_dunder(
                        db,
                        "__next__",
                        CallArguments::none(),
                    ))
                }
            }

            Self::PossiblyUnboundIterAndGetitemError {
                dunder_next_return,
                dunder_getitem_error,
            } => match dunder_getitem_error {
                CallDunderError::MethodNotAvailable => Some(*dunder_next_return),
                CallDunderError::PossiblyUnbound(dunder_getitem_outcome) => {
                    Some(UnionType::from_elements(
                        db,
                        [*dunder_next_return, dunder_getitem_outcome.return_type(db)],
                    ))
                }
                CallDunderError::CallError(CallErrorKind::NotCallable, _) => {
                    Some(*dunder_next_return)
                }
                CallDunderError::CallError(_, dunder_getitem_bindings) => {
                    let dunder_getitem_return = dunder_getitem_bindings.return_type(db);
                    let elements = [*dunder_next_return, dunder_getitem_return];
                    Some(UnionType::from_elements(db, elements))
                }
            },

            Self::UnboundIterAndGetitemError {
                dunder_getitem_error,
            } => dunder_getitem_error.return_type(db),

            Self::UnboundAiterError => None,
        }
    }

    /// Does this error concern a synchronous or asynchronous iterable?
    fn mode(&self) -> EvaluationMode {
        match self {
            Self::IterCallError { mode, .. } => *mode,
            Self::IterReturnsInvalidIterator { mode, .. } => *mode,
            Self::PossiblyUnboundIterAndGetitemError { .. }
            | Self::UnboundIterAndGetitemError { .. } => EvaluationMode::Sync,
            Self::UnboundAiterError => EvaluationMode::Async,
        }
    }

    /// Reports the diagnostic for this error.
    fn report_diagnostic(
        &self,
        context: &InferContext<'db, '_>,
        iterable_type: Type<'db>,
        iterable_node: ast::AnyNodeRef,
    ) {
        /// A little helper type for emitting a diagnostic
        /// based on the variant of iteration error.
        struct Reporter<'a> {
            db: &'a dyn Db,
            builder: LintDiagnosticGuardBuilder<'a, 'a>,
            iterable_type: Type<'a>,
            mode: EvaluationMode,
        }

        impl<'a> Reporter<'a> {
            /// Emit a diagnostic that is certain that `iterable_type` is not iterable.
            ///
            /// `because` should explain why `iterable_type` is not iterable.
            #[expect(clippy::wrong_self_convention)]
            fn is_not(self, because: impl std::fmt::Display) -> LintDiagnosticGuard<'a, 'a> {
                let mut diag = self.builder.into_diagnostic(format_args!(
                    "Object of type `{iterable_type}` is not {maybe_async}iterable",
                    iterable_type = self.iterable_type.display(self.db),
                    maybe_async = if self.mode.is_async() { "async-" } else { "" }
                ));
                diag.info(because);
                diag
            }

            /// Emit a diagnostic that is uncertain that `iterable_type` is not iterable.
            ///
            /// `because` should explain why `iterable_type` is likely not iterable.
            fn may_not(self, because: impl std::fmt::Display) -> LintDiagnosticGuard<'a, 'a> {
                let mut diag = self.builder.into_diagnostic(format_args!(
                    "Object of type `{iterable_type}` may not be {maybe_async}iterable",
                    iterable_type = self.iterable_type.display(self.db),
                    maybe_async = if self.mode.is_async() { "async-" } else { "" }
                ));
                diag.info(because);
                diag
            }
        }

        let Some(builder) = context.report_lint(&NOT_ITERABLE, iterable_node) else {
            return;
        };
        let db = context.db();
        let mode = self.mode();
        let reporter = Reporter {
            db,
            builder,
            iterable_type,
            mode,
        };

        // TODO: for all of these error variants, the "explanation" for the diagnostic
        // (everything after the "because") should really be presented as a "help:", "note",
        // or similar, rather than as part of the same sentence as the error message.
        match self {
            Self::IterCallError {
                kind,
                bindings,
                mode,
            } => {
                let method = if mode.is_async() {
                    "__aiter__"
                } else {
                    "__iter__"
                };

                match kind {
                    CallErrorKind::NotCallable => {
                        reporter.is_not(format_args!(
                        "Its `{method}` attribute has type `{dunder_iter_type}`, which is not callable",
                        dunder_iter_type = bindings.callable_type().display(db),
                    ));
                    }
                    CallErrorKind::PossiblyNotCallable => {
                        reporter.may_not(format_args!(
                            "Its `{method}` attribute (with type `{dunder_iter_type}`) \
                             may not be callable",
                            dunder_iter_type = bindings.callable_type().display(db),
                        ));
                    }
                    CallErrorKind::BindingError => {
                        if bindings.is_single() {
                            reporter
                                .is_not(format_args!(
                                    "Its `{method}` method has an invalid signature"
                                ))
                                .info(format_args!("Expected signature `def {method}(self): ...`"));
                        } else {
                            let mut diag = reporter.may_not(format_args!(
                                "Its `{method}` method may have an invalid signature"
                            ));
                            diag.info(format_args!(
                                "Type of `{method}` is `{dunder_iter_type}`",
                                dunder_iter_type = bindings.callable_type().display(db),
                            ));
                            diag.info(format_args!(
                                "Expected signature for `{method}` is `def {method}(self): ...`",
                            ));
                        }
                    }
                }
            }

            Self::IterReturnsInvalidIterator {
                iterator,
                dunder_error: dunder_next_error,
                mode,
            } => {
                let dunder_iter_name = if mode.is_async() {
                    "__aiter__"
                } else {
                    "__iter__"
                };
                let dunder_next_name = if mode.is_async() {
                    "__anext__"
                } else {
                    "__next__"
                };
                match dunder_next_error {
                    CallDunderError::MethodNotAvailable => {
                        reporter.is_not(format_args!(
                        "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                         which has no `{dunder_next_name}` method",
                        iterator_type = iterator.display(db),
                    ));
                    }
                    CallDunderError::PossiblyUnbound(_) => {
                        reporter.may_not(format_args!(
                            "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                            which may not have a `{dunder_next_name}` method",
                            iterator_type = iterator.display(db),
                        ));
                    }
                    CallDunderError::CallError(CallErrorKind::NotCallable, _) => {
                        reporter.is_not(format_args!(
                            "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                            which has a `{dunder_next_name}` attribute that is not callable",
                            iterator_type = iterator.display(db),
                        ));
                    }
                    CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, _) => {
                        reporter.may_not(format_args!(
                            "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                            which has a `{dunder_next_name}` attribute that may not be callable",
                            iterator_type = iterator.display(db),
                        ));
                    }
                    CallDunderError::CallError(CallErrorKind::BindingError, bindings)
                        if bindings.is_single() =>
                    {
                        reporter
                            .is_not(format_args!(
                                "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                                which has an invalid `{dunder_next_name}` method",
                                iterator_type = iterator.display(db),
                            ))
                            .info(format_args!("Expected signature for `{dunder_next_name}` is `def {dunder_next_name}(self): ...`"));
                    }
                    CallDunderError::CallError(CallErrorKind::BindingError, _) => {
                        reporter
                            .may_not(format_args!(
                                "Its `{dunder_iter_name}` method returns an object of type `{iterator_type}`, \
                                which may have an invalid `{dunder_next_name}` method",
                                iterator_type = iterator.display(db),
                            ))
                            .info(format_args!("Expected signature for `{dunder_next_name}` is `def {dunder_next_name}(self): ...`"));
                    }
                }
            }

            Self::PossiblyUnboundIterAndGetitemError {
                dunder_getitem_error,
                ..
            } => match dunder_getitem_error {
                CallDunderError::MethodNotAvailable => {
                    reporter.may_not(
                        "It may not have an `__iter__` method \
                         and it doesn't have a `__getitem__` method",
                    );
                }
                CallDunderError::PossiblyUnbound(_) => {
                    reporter
                        .may_not("It may not have an `__iter__` method or a `__getitem__` method");
                }
                CallDunderError::CallError(CallErrorKind::NotCallable, bindings) => {
                    reporter.may_not(format_args!(
                        "It may not have an `__iter__` method \
                         and its `__getitem__` attribute has type `{dunder_getitem_type}`, \
                         which is not callable",
                        dunder_getitem_type = bindings.callable_type().display(db),
                    ));
                }
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings)
                    if bindings.is_single() =>
                {
                    reporter.may_not(
                        "It may not have an `__iter__` method \
                         and its `__getitem__` attribute may not be callable",
                    );
                }
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings) => {
                    reporter.may_not(format_args!(
                        "It may not have an `__iter__` method \
                         and its `__getitem__` attribute (with type `{dunder_getitem_type}`) \
                         may not be callable",
                        dunder_getitem_type = bindings.callable_type().display(db),
                    ));
                }
                CallDunderError::CallError(CallErrorKind::BindingError, bindings)
                    if bindings.is_single() =>
                {
                    reporter
                        .may_not(
                            "It may not have an `__iter__` method \
                             and its `__getitem__` method has an incorrect signature \
                             for the old-style iteration protocol",
                        )
                        .info(
                            "`__getitem__` must be at least as permissive as \
                             `def __getitem__(self, key: int): ...` \
                             to satisfy the old-style iteration protocol",
                        );
                }
                CallDunderError::CallError(CallErrorKind::BindingError, bindings) => {
                    reporter
                        .may_not(format_args!(
                            "It may not have an `__iter__` method \
                             and its `__getitem__` method (with type `{dunder_getitem_type}`) \
                             may have an incorrect signature for the old-style iteration protocol",
                            dunder_getitem_type = bindings.callable_type().display(db),
                        ))
                        .info(
                            "`__getitem__` must be at least as permissive as \
                             `def __getitem__(self, key: int): ...` \
                             to satisfy the old-style iteration protocol",
                        );
                }
            },

            Self::UnboundIterAndGetitemError {
                dunder_getitem_error,
            } => match dunder_getitem_error {
                CallDunderError::MethodNotAvailable => {
                    reporter
                        .is_not("It doesn't have an `__iter__` method or a `__getitem__` method");
                }
                CallDunderError::PossiblyUnbound(_) => {
                    reporter.is_not(
                        "It has no `__iter__` method and it may not have a `__getitem__` method",
                    );
                }
                CallDunderError::CallError(CallErrorKind::NotCallable, bindings) => {
                    reporter.is_not(format_args!(
                        "It has no `__iter__` method and \
                         its `__getitem__` attribute has type `{dunder_getitem_type}`, \
                         which is not callable",
                        dunder_getitem_type = bindings.callable_type().display(db),
                    ));
                }
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings)
                    if bindings.is_single() =>
                {
                    reporter.may_not(
                        "It has no `__iter__` method and its `__getitem__` attribute \
                         may not be callable",
                    );
                }
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings) => {
                    reporter.may_not(
                        "It has no `__iter__` method and its `__getitem__` attribute is invalid",
                    ).info(format_args!(
                        "`__getitem__` has type `{dunder_getitem_type}`, which is not callable",
                        dunder_getitem_type = bindings.callable_type().display(db),
                    ));
                }
                CallDunderError::CallError(CallErrorKind::BindingError, bindings)
                    if bindings.is_single() =>
                {
                    reporter
                        .is_not(
                            "It has no `__iter__` method and \
                             its `__getitem__` method has an incorrect signature \
                             for the old-style iteration protocol",
                        )
                        .info(
                            "`__getitem__` must be at least as permissive as \
                             `def __getitem__(self, key: int): ...` \
                             to satisfy the old-style iteration protocol",
                        );
                }
                CallDunderError::CallError(CallErrorKind::BindingError, bindings) => {
                    reporter
                        .may_not(format_args!(
                            "It has no `__iter__` method and \
                             its `__getitem__` method (with type `{dunder_getitem_type}`) \
                             may have an incorrect signature for the old-style iteration protocol",
                            dunder_getitem_type = bindings.callable_type().display(db),
                        ))
                        .info(
                            "`__getitem__` must be at least as permissive as \
                             `def __getitem__(self, key: int): ...` \
                             to satisfy the old-style iteration protocol",
                        );
                }
            },

            IterationError::UnboundAiterError => {
                reporter.is_not("It has no `__aiter__` method");
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BoolError<'db> {
    /// The type has a `__bool__` attribute but it can't be called.
    NotCallable { not_boolable_type: Type<'db> },

    /// The type has a callable `__bool__` attribute, but it isn't callable
    /// with the given arguments.
    IncorrectArguments {
        not_boolable_type: Type<'db>,
        truthiness: Truthiness,
    },

    /// The type has a `__bool__` method, is callable with the given arguments,
    /// but the return type isn't assignable to `bool`.
    IncorrectReturnType {
        not_boolable_type: Type<'db>,
        return_type: Type<'db>,
    },

    /// A union type doesn't implement `__bool__` correctly.
    Union {
        union: UnionType<'db>,
        truthiness: Truthiness,
    },

    /// Any other reason why the type can't be converted to a bool.
    /// E.g. because calling `__bool__` returns in a union type and not all variants support `__bool__` or
    /// because `__bool__` points to a type that has a possibly unbound `__call__` method.
    Other { not_boolable_type: Type<'db> },
}

impl<'db> BoolError<'db> {
    pub(super) fn fallback_truthiness(&self) -> Truthiness {
        match self {
            BoolError::NotCallable { .. }
            | BoolError::IncorrectReturnType { .. }
            | BoolError::Other { .. } => Truthiness::Ambiguous,
            BoolError::IncorrectArguments { truthiness, .. }
            | BoolError::Union { truthiness, .. } => *truthiness,
        }
    }

    fn not_boolable_type(&self) -> Type<'db> {
        match self {
            BoolError::NotCallable {
                not_boolable_type, ..
            }
            | BoolError::IncorrectArguments {
                not_boolable_type, ..
            }
            | BoolError::Other { not_boolable_type }
            | BoolError::IncorrectReturnType {
                not_boolable_type, ..
            } => *not_boolable_type,
            BoolError::Union { union, .. } => Type::Union(*union),
        }
    }

    pub(super) fn report_diagnostic(&self, context: &InferContext, condition: impl Ranged) {
        self.report_diagnostic_impl(context, condition.range());
    }

    fn report_diagnostic_impl(&self, context: &InferContext, condition: TextRange) {
        let Some(builder) = context.report_lint(&UNSUPPORTED_BOOL_CONVERSION, condition) else {
            return;
        };
        match self {
            Self::IncorrectArguments {
                not_boolable_type, ..
            } => {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Boolean conversion is unsupported for type `{}`",
                    not_boolable_type.display(context.db())
                ));
                let mut sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    "`__bool__` methods must only have a `self` parameter",
                );
                if let Some((func_span, parameter_span)) = not_boolable_type
                    .member(context.db(), "__bool__")
                    .into_lookup_result()
                    .ok()
                    .and_then(|quals| quals.inner_type().parameter_span(context.db(), None))
                {
                    sub.annotate(
                        Annotation::primary(parameter_span).message("Incorrect parameters"),
                    );
                    sub.annotate(Annotation::secondary(func_span).message("Method defined here"));
                }
                diag.sub(sub);
            }
            Self::IncorrectReturnType {
                not_boolable_type,
                return_type,
            } => {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Boolean conversion is unsupported for type `{not_boolable}`",
                    not_boolable = not_boolable_type.display(context.db()),
                ));
                let mut sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format_args!(
                        "`{return_type}` is not assignable to `bool`",
                        return_type = return_type.display(context.db()),
                    ),
                );
                if let Some((func_span, return_type_span)) = not_boolable_type
                    .member(context.db(), "__bool__")
                    .into_lookup_result()
                    .ok()
                    .and_then(|quals| quals.inner_type().function_spans(context.db()))
                    .and_then(|spans| Some((spans.name, spans.return_type?)))
                {
                    sub.annotate(
                        Annotation::primary(return_type_span).message("Incorrect return type"),
                    );
                    sub.annotate(Annotation::secondary(func_span).message("Method defined here"));
                }
                diag.sub(sub);
            }
            Self::NotCallable { not_boolable_type } => {
                let mut diag = builder.into_diagnostic(format_args!(
                    "Boolean conversion is unsupported for type `{}`",
                    not_boolable_type.display(context.db())
                ));
                let sub = SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format_args!(
                        "`__bool__` on `{}` must be callable",
                        not_boolable_type.display(context.db())
                    ),
                );
                // TODO: It would be nice to create an annotation here for
                // where `__bool__` is defined. At time of writing, I couldn't
                // figure out a straight-forward way of doing this. ---AG
                diag.sub(sub);
            }
            Self::Union { union, .. } => {
                let first_error = union
                    .elements(context.db())
                    .iter()
                    .find_map(|element| element.try_bool(context.db()).err())
                    .unwrap();

                builder.into_diagnostic(format_args!(
                    "Boolean conversion is unsupported for union `{}` \
                     because `{}` doesn't implement `__bool__` correctly",
                    Type::Union(*union).display(context.db()),
                    first_error.not_boolable_type().display(context.db()),
                ));
            }

            Self::Other { not_boolable_type } => {
                builder.into_diagnostic(format_args!(
                    "Boolean conversion is unsupported for type `{}`; \
                     it incorrectly implements `__bool__`",
                    not_boolable_type.display(context.db())
                ));
            }
        }
    }
}

/// Represents possibly failure modes of implicit `__new__` calls.
#[derive(Debug)]
enum DunderNewCallError<'db> {
    /// The call to `__new__` failed.
    CallError(CallError<'db>),
    /// The `__new__` method could be unbound. If the call to the
    /// method has also failed, this variant also includes the
    /// corresponding `CallError`.
    PossiblyUnbound(Option<CallError<'db>>),
}

/// Error returned if a class instantiation call failed
#[derive(Debug)]
enum ConstructorCallError<'db> {
    Init(Type<'db>, CallDunderError<'db>),
    New(Type<'db>, DunderNewCallError<'db>),
    NewAndInit(Type<'db>, DunderNewCallError<'db>, CallDunderError<'db>),
}

impl<'db> ConstructorCallError<'db> {
    fn return_type(&self) -> Type<'db> {
        match self {
            Self::Init(ty, _) => *ty,
            Self::New(ty, _) => *ty,
            Self::NewAndInit(ty, _, _) => *ty,
        }
    }

    fn report_diagnostic(
        &self,
        context: &InferContext<'db, '_>,
        context_expression_type: Type<'db>,
        context_expression_node: ast::AnyNodeRef,
    ) {
        let report_init_error = |call_dunder_error: &CallDunderError<'db>| match call_dunder_error {
            CallDunderError::MethodNotAvailable => {
                if let Some(builder) =
                    context.report_lint(&POSSIBLY_UNBOUND_IMPLICIT_CALL, context_expression_node)
                {
                    // If we are using vendored typeshed, it should be impossible to have missing
                    // or unbound `__init__` method on a class, as all classes have `object` in MRO.
                    // Thus the following may only trigger if a custom typeshed is used.
                    builder.into_diagnostic(format_args!(
                        "`__init__` method is missing on type `{}`. \
                         Make sure your `object` in typeshed has its definition.",
                        context_expression_type.display(context.db()),
                    ));
                }
            }
            CallDunderError::PossiblyUnbound(bindings) => {
                if let Some(builder) =
                    context.report_lint(&POSSIBLY_UNBOUND_IMPLICIT_CALL, context_expression_node)
                {
                    builder.into_diagnostic(format_args!(
                        "Method `__init__` on type `{}` is possibly unbound.",
                        context_expression_type.display(context.db()),
                    ));
                }

                bindings.report_diagnostics(context, context_expression_node);
            }
            CallDunderError::CallError(_, bindings) => {
                bindings.report_diagnostics(context, context_expression_node);
            }
        };

        let report_new_error = |error: &DunderNewCallError<'db>| match error {
            DunderNewCallError::PossiblyUnbound(call_error) => {
                if let Some(builder) =
                    context.report_lint(&POSSIBLY_UNBOUND_IMPLICIT_CALL, context_expression_node)
                {
                    builder.into_diagnostic(format_args!(
                        "Method `__new__` on type `{}` is possibly unbound.",
                        context_expression_type.display(context.db()),
                    ));
                }

                if let Some(CallError(_kind, bindings)) = call_error {
                    bindings.report_diagnostics(context, context_expression_node);
                }
            }
            DunderNewCallError::CallError(CallError(_kind, bindings)) => {
                bindings.report_diagnostics(context, context_expression_node);
            }
        };

        match self {
            Self::Init(_, init_call_dunder_error) => {
                report_init_error(init_call_dunder_error);
            }
            Self::New(_, new_call_error) => {
                report_new_error(new_call_error);
            }
            Self::NewAndInit(_, new_call_error, init_call_dunder_error) => {
                report_new_error(new_call_error);
                report_init_error(init_call_dunder_error);
            }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum TypeRelation {
    Subtyping,
    Assignability,
}

impl TypeRelation {
    pub(crate) const fn is_assignability(self) -> bool {
        matches!(self, TypeRelation::Assignability)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Truthiness {
    /// For an object `x`, `bool(x)` will always return `True`
    AlwaysTrue,
    /// For an object `x`, `bool(x)` will always return `False`
    AlwaysFalse,
    /// For an object `x`, `bool(x)` could return either `True` or `False`
    Ambiguous,
}

impl Truthiness {
    pub(crate) const fn is_ambiguous(self) -> bool {
        matches!(self, Truthiness::Ambiguous)
    }

    pub(crate) const fn is_always_false(self) -> bool {
        matches!(self, Truthiness::AlwaysFalse)
    }

    pub(crate) const fn may_be_true(self) -> bool {
        !self.is_always_false()
    }

    pub(crate) const fn is_always_true(self) -> bool {
        matches!(self, Truthiness::AlwaysTrue)
    }

    pub(crate) const fn negate(self) -> Self {
        match self {
            Self::AlwaysTrue => Self::AlwaysFalse,
            Self::AlwaysFalse => Self::AlwaysTrue,
            Self::Ambiguous => Self::Ambiguous,
        }
    }

    pub(crate) const fn negate_if(self, condition: bool) -> Self {
        if condition { self.negate() } else { self }
    }

    pub(crate) fn and(self, other: Self) -> Self {
        match (self, other) {
            (Truthiness::AlwaysTrue, Truthiness::AlwaysTrue) => Truthiness::AlwaysTrue,
            (Truthiness::AlwaysFalse, _) | (_, Truthiness::AlwaysFalse) => Truthiness::AlwaysFalse,
            _ => Truthiness::Ambiguous,
        }
    }

    pub(crate) fn or(self, other: Self) -> Self {
        match (self, other) {
            (Truthiness::AlwaysFalse, Truthiness::AlwaysFalse) => Truthiness::AlwaysFalse,
            (Truthiness::AlwaysTrue, _) | (_, Truthiness::AlwaysTrue) => Truthiness::AlwaysTrue,
            _ => Truthiness::Ambiguous,
        }
    }

    fn into_type(self, db: &dyn Db) -> Type<'_> {
        match self {
            Self::AlwaysTrue => Type::BooleanLiteral(true),
            Self::AlwaysFalse => Type::BooleanLiteral(false),
            Self::Ambiguous => KnownClass::Bool.to_instance(db),
        }
    }
}

impl From<bool> for Truthiness {
    fn from(value: bool) -> Self {
        if value {
            Truthiness::AlwaysTrue
        } else {
            Truthiness::AlwaysFalse
        }
    }
}

/// This type represents bound method objects that are created when a method is accessed
/// on an instance of a class. For example, the expression `Path("a.txt").touch` creates
/// a bound method object that represents the `Path.touch` method which is bound to the
/// instance `Path("a.txt")`.
///
/// # Ordering
/// Ordering is based on the bounded method's salsa-assigned id and not on its values.
/// The id may change between runs, or when the bounded method was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct BoundMethodType<'db> {
    /// The function that is being bound. Corresponds to the `__func__` attribute on a
    /// bound method object
    pub(crate) function: FunctionType<'db>,
    /// The instance on which this method has been called. Corresponds to the `__self__`
    /// attribute on a bound method object
    self_instance: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BoundMethodType<'_> {}

fn walk_bound_method_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    method: BoundMethodType<'db>,
    visitor: &V,
) {
    visitor.visit_function_type(db, method.function(db));
    visitor.visit_type(db, method.self_instance(db));
}

impl<'db> BoundMethodType<'db> {
    pub(crate) fn into_callable_type(self, db: &'db dyn Db) -> CallableType<'db> {
        CallableType::new(
            db,
            CallableSignature::from_overloads(
                self.function(db)
                    .signature(db)
                    .overloads
                    .iter()
                    .map(signatures::Signature::bind_self),
            ),
            false,
        )
    }

    fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        Self::new(
            db,
            self.function(db).normalized_impl(db, visitor),
            self.self_instance(db).normalized_impl(db, visitor),
        )
    }

    fn has_relation_to(self, db: &'db dyn Db, other: Self, relation: TypeRelation) -> bool {
        // A bound method is a typically a subtype of itself. However, we must explicitly verify
        // the subtyping of the underlying function signatures (since they might be specialized
        // differently), and of the bound self parameter (taking care that parameters, including a
        // bound self parameter, are contravariant.)
        self.function(db)
            .has_relation_to(db, other.function(db), relation)
            && other
                .self_instance(db)
                .has_relation_to(db, self.self_instance(db), relation)
    }

    fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.function(db).is_equivalent_to(db, other.function(db))
            && other
                .self_instance(db)
                .is_equivalent_to(db, self.self_instance(db))
    }
}

/// This type represents the set of all callable objects with a certain, possibly overloaded,
/// signature.
///
/// It can be written in type expressions using `typing.Callable`. `lambda` expressions are
/// inferred directly as `CallableType`s; all function-literal types are subtypes of a
/// `CallableType`.
///
/// # Ordering
/// Ordering is based on the callable type's salsa-assigned id and not on its values.
/// The id may change between runs, or when the callable type was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct CallableType<'db> {
    #[returns(ref)]
    pub(crate) signatures: CallableSignature<'db>,

    /// We use `CallableType` to represent function-like objects, like the synthesized methods
    /// of dataclasses or NamedTuples. These callables act like real functions when accessed
    /// as attributes on instances, i.e. they bind `self`.
    is_function_like: bool,
}

pub(super) fn walk_callable_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    ty: CallableType<'db>,
    visitor: &V,
) {
    for signature in &ty.signatures(db).overloads {
        walk_signature(db, signature, visitor);
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for CallableType<'_> {}

impl<'db> CallableType<'db> {
    /// Create a callable type with a single non-overloaded signature.
    pub(crate) fn single(db: &'db dyn Db, signature: Signature<'db>) -> Type<'db> {
        Type::Callable(CallableType::new(
            db,
            CallableSignature::single(signature),
            false,
        ))
    }

    /// Create a non-overloaded, function-like callable type with a single signature.
    ///
    /// A function-like callable will bind `self` when accessed as an attribute on an instance.
    pub(crate) fn function_like(db: &'db dyn Db, signature: Signature<'db>) -> Type<'db> {
        Type::Callable(CallableType::new(
            db,
            CallableSignature::single(signature),
            true,
        ))
    }

    /// Create a callable type which accepts any parameters and returns an `Unknown` type.
    pub(crate) fn unknown(db: &'db dyn Db) -> Type<'db> {
        Self::single(db, Signature::unknown())
    }

    pub(crate) fn bind_self(self, db: &'db dyn Db) -> Type<'db> {
        Type::Callable(CallableType::new(
            db,
            self.signatures(db).bind_self(),
            false,
        ))
    }

    fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        CallableType::new(
            db,
            self.signatures(db).materialize(db, variance),
            self.is_function_like(db),
        )
    }

    /// Create a callable type which represents a fully-static "bottom" callable.
    ///
    /// Specifically, this represents a callable type with a single signature:
    /// `(*args: object, **kwargs: object) -> Never`.
    #[cfg(test)]
    pub(crate) fn bottom(db: &'db dyn Db) -> Type<'db> {
        Self::single(db, Signature::bottom(db))
    }

    /// Return a "normalized" version of this `Callable` type.
    ///
    /// See [`Type::normalized`] for more details.
    fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        CallableType::new(
            db,
            self.signatures(db).normalized_impl(db, visitor),
            self.is_function_like(db),
        )
    }

    fn apply_type_mapping<'a>(self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        CallableType::new(
            db,
            self.signatures(db).apply_type_mapping(db, type_mapping),
            self.is_function_like(db),
        )
    }

    fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        self.signatures(db)
            .find_legacy_typevars(db, binding_context, typevars);
    }

    /// Check whether this callable type has the given relation to another callable type.
    ///
    /// See [`Type::is_subtype_of`] and [`Type::is_assignable_to`] for more details.
    fn has_relation_to(self, db: &'db dyn Db, other: Self, relation: TypeRelation) -> bool {
        if other.is_function_like(db) && !self.is_function_like(db) {
            return false;
        }
        self.signatures(db)
            .has_relation_to(db, other.signatures(db), relation)
    }

    /// Check whether this callable type is equivalent to another callable type.
    ///
    /// See [`Type::is_equivalent_to`] for more details.
    fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }

        self.is_function_like(db) == other.is_function_like(db)
            && self
                .signatures(db)
                .is_equivalent_to(db, other.signatures(db))
    }
}

/// Represents a specific instance of `types.MethodWrapperType`
#[derive(
    Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, salsa::Update, get_size2::GetSize,
)]
pub enum MethodWrapperKind<'db> {
    /// Method wrapper for `some_function.__get__`
    FunctionTypeDunderGet(FunctionType<'db>),
    /// Method wrapper for `some_function.__call__`
    FunctionTypeDunderCall(FunctionType<'db>),
    /// Method wrapper for `some_property.__get__`
    PropertyDunderGet(PropertyInstanceType<'db>),
    /// Method wrapper for `some_property.__set__`
    PropertyDunderSet(PropertyInstanceType<'db>),
    /// Method wrapper for `str.startswith`.
    /// We treat this method specially because we want to be able to infer precise Boolean
    /// literal return types if the instance and the prefix are both string literals, and
    /// this allows us to understand statically known branches for common tests such as
    /// `if sys.platform.startswith("freebsd")`.
    StrStartswith(StringLiteralType<'db>),
}

pub(super) fn walk_method_wrapper_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    method_wrapper: MethodWrapperKind<'db>,
    visitor: &V,
) {
    match method_wrapper {
        MethodWrapperKind::FunctionTypeDunderGet(function) => {
            visitor.visit_function_type(db, function);
        }
        MethodWrapperKind::FunctionTypeDunderCall(function) => {
            visitor.visit_function_type(db, function);
        }
        MethodWrapperKind::PropertyDunderGet(property) => {
            visitor.visit_property_instance_type(db, property);
        }
        MethodWrapperKind::PropertyDunderSet(property) => {
            visitor.visit_property_instance_type(db, property);
        }
        MethodWrapperKind::StrStartswith(string_literal) => {
            visitor.visit_type(db, Type::StringLiteral(string_literal));
        }
    }
}

impl<'db> MethodWrapperKind<'db> {
    fn has_relation_to(self, db: &'db dyn Db, other: Self, relation: TypeRelation) -> bool {
        match (self, other) {
            (
                MethodWrapperKind::FunctionTypeDunderGet(self_function),
                MethodWrapperKind::FunctionTypeDunderGet(other_function),
            ) => self_function.has_relation_to(db, other_function, relation),

            (
                MethodWrapperKind::FunctionTypeDunderCall(self_function),
                MethodWrapperKind::FunctionTypeDunderCall(other_function),
            ) => self_function.has_relation_to(db, other_function, relation),

            (MethodWrapperKind::PropertyDunderGet(_), MethodWrapperKind::PropertyDunderGet(_))
            | (MethodWrapperKind::PropertyDunderSet(_), MethodWrapperKind::PropertyDunderSet(_))
            | (MethodWrapperKind::StrStartswith(_), MethodWrapperKind::StrStartswith(_)) => {
                self == other
            }

            (
                MethodWrapperKind::FunctionTypeDunderGet(_)
                | MethodWrapperKind::FunctionTypeDunderCall(_)
                | MethodWrapperKind::PropertyDunderGet(_)
                | MethodWrapperKind::PropertyDunderSet(_)
                | MethodWrapperKind::StrStartswith(_),
                MethodWrapperKind::FunctionTypeDunderGet(_)
                | MethodWrapperKind::FunctionTypeDunderCall(_)
                | MethodWrapperKind::PropertyDunderGet(_)
                | MethodWrapperKind::PropertyDunderSet(_)
                | MethodWrapperKind::StrStartswith(_),
            ) => false,
        }
    }

    fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        match (self, other) {
            (
                MethodWrapperKind::FunctionTypeDunderGet(self_function),
                MethodWrapperKind::FunctionTypeDunderGet(other_function),
            ) => self_function.is_equivalent_to(db, other_function),

            (
                MethodWrapperKind::FunctionTypeDunderCall(self_function),
                MethodWrapperKind::FunctionTypeDunderCall(other_function),
            ) => self_function.is_equivalent_to(db, other_function),

            (MethodWrapperKind::PropertyDunderGet(_), MethodWrapperKind::PropertyDunderGet(_))
            | (MethodWrapperKind::PropertyDunderSet(_), MethodWrapperKind::PropertyDunderSet(_))
            | (MethodWrapperKind::StrStartswith(_), MethodWrapperKind::StrStartswith(_)) => {
                self == other
            }

            (
                MethodWrapperKind::FunctionTypeDunderGet(_)
                | MethodWrapperKind::FunctionTypeDunderCall(_)
                | MethodWrapperKind::PropertyDunderGet(_)
                | MethodWrapperKind::PropertyDunderSet(_)
                | MethodWrapperKind::StrStartswith(_),
                MethodWrapperKind::FunctionTypeDunderGet(_)
                | MethodWrapperKind::FunctionTypeDunderCall(_)
                | MethodWrapperKind::PropertyDunderGet(_)
                | MethodWrapperKind::PropertyDunderSet(_)
                | MethodWrapperKind::StrStartswith(_),
            ) => false,
        }
    }

    fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        match self {
            MethodWrapperKind::FunctionTypeDunderGet(function) => {
                MethodWrapperKind::FunctionTypeDunderGet(function.normalized_impl(db, visitor))
            }
            MethodWrapperKind::FunctionTypeDunderCall(function) => {
                MethodWrapperKind::FunctionTypeDunderCall(function.normalized_impl(db, visitor))
            }
            MethodWrapperKind::PropertyDunderGet(property) => {
                MethodWrapperKind::PropertyDunderGet(property.normalized_impl(db, visitor))
            }
            MethodWrapperKind::PropertyDunderSet(property) => {
                MethodWrapperKind::PropertyDunderSet(property.normalized_impl(db, visitor))
            }
            MethodWrapperKind::StrStartswith(_) => self,
        }
    }
}

/// Represents a specific instance of `types.WrapperDescriptorType`
#[derive(
    Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, salsa::Update, get_size2::GetSize,
)]
pub enum WrapperDescriptorKind {
    /// `FunctionType.__get__`
    FunctionTypeDunderGet,
    /// `property.__get__`
    PropertyDunderGet,
    /// `property.__set__`
    PropertyDunderSet,
}

/// # Ordering
/// Ordering is based on the module literal's salsa-assigned id and not on its values.
/// The id may change between runs, or when the module literal was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct ModuleLiteralType<'db> {
    /// The imported module.
    pub module: Module<'db>,

    /// The file in which this module was imported.
    ///
    /// If the module is a module that could have submodules (a package),
    /// we need this in order to know which submodules should be attached to it as attributes
    /// (because the submodules were also imported in this file). For a package, this should
    /// therefore always be `Some()`. If the module is not a package, however, this should
    /// always be `None`: this helps reduce memory usage (the information is redundant for
    /// single-file modules), and ensures that two module-literal types that both refer to
    /// the same underlying single-file module are understood by ty as being equivalent types
    /// in all situations.
    _importing_file: Option<File>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ModuleLiteralType<'_> {}

impl<'db> ModuleLiteralType<'db> {
    fn importing_file(self, db: &'db dyn Db) -> Option<File> {
        debug_assert_eq!(
            self._importing_file(db).is_some(),
            self.module(db).kind(db).is_package()
        );
        self._importing_file(db)
    }

    fn available_submodule_attributes(&self, db: &'db dyn Db) -> impl Iterator<Item = Name> {
        self.importing_file(db)
            .into_iter()
            .flat_map(|file| imported_modules(db, file))
            .filter_map(|submodule_name| submodule_name.relative_to(self.module(db).name(db)))
            .filter_map(|relative_submodule| relative_submodule.components().next().map(Name::from))
    }

    fn resolve_submodule(self, db: &'db dyn Db, name: &str) -> Option<Type<'db>> {
        let importing_file = self.importing_file(db)?;
        let relative_submodule_name = ModuleName::new(name)?;
        let mut absolute_submodule_name = self.module(db).name(db).clone();
        absolute_submodule_name.extend(&relative_submodule_name);
        let submodule = resolve_module(db, &absolute_submodule_name)?;
        Some(Type::module_literal(db, importing_file, submodule))
    }

    fn try_module_getattr(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        // For module literals, we want to try calling the module's own `__getattr__` function
        // if it exists. First, we need to look up the `__getattr__` function in the module's scope.
        if let Some(file) = self.module(db).file(db) {
            let getattr_symbol = imported_symbol(db, file, "__getattr__", None);
            if let Place::Type(getattr_type, boundness) = getattr_symbol.place {
                // If we found a __getattr__ function, try to call it with the name argument
                if let Ok(outcome) = getattr_type.try_call(
                    db,
                    &CallArguments::positional([Type::string_literal(db, name)]),
                ) {
                    return Place::Type(outcome.return_type(db), boundness).into();
                }
            }
        }

        Place::Unbound.into()
    }

    fn static_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        // `__dict__` is a very special member that is never overridden by module globals;
        // we should always look it up directly as an attribute on `types.ModuleType`,
        // never in the global scope of the module.
        if name == "__dict__" {
            return KnownClass::ModuleType
                .to_instance(db)
                .member(db, "__dict__");
        }

        // If the file that originally imported the module has also imported a submodule
        // named `name`, then the result is (usually) that submodule, even if the module
        // also defines a (non-module) symbol with that name.
        //
        // Note that technically, either the submodule or the non-module symbol could take
        // priority, depending on the ordering of when the submodule is loaded relative to
        // the parent module's `__init__.py` file being evaluated. That said, we have
        // chosen to always have the submodule take priority. (This matches pyright's
        // current behavior, but is the opposite of mypy's current behavior.)
        if self.available_submodule_attributes(db).contains(name) {
            if let Some(submodule) = self.resolve_submodule(db, name) {
                return Place::bound(submodule).into();
            }
        }

        let place_and_qualifiers = self
            .module(db)
            .file(db)
            .map(|file| imported_symbol(db, file, name, None))
            .unwrap_or_default();

        // If the normal lookup failed, try to call the module's `__getattr__` function
        if place_and_qualifiers.place.is_unbound() {
            return self.try_module_getattr(db, name);
        }

        place_and_qualifiers
    }
}

/// # Ordering
/// Ordering is based on the type alias's salsa-assigned id and not on its values.
/// The id may change between runs, or when the alias was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct PEP695TypeAliasType<'db> {
    #[returns(ref)]
    pub name: ast::name::Name,

    rhs_scope: ScopeId<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for PEP695TypeAliasType<'_> {}

fn walk_pep_695_type_alias<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    type_alias: PEP695TypeAliasType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, type_alias.value_type(db));
}

#[salsa::tracked]
impl<'db> PEP695TypeAliasType<'db> {
    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let scope = self.rhs_scope(db);
        let module = parsed_module(db, scope.file(db)).load(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias(&module);

        semantic_index(db, scope.file(db)).expect_single_definition(type_alias_stmt_node)
    }

    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        let scope = self.rhs_scope(db);
        let module = parsed_module(db, scope.file(db)).load(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias(&module);
        let definition = self.definition(db);
        definition_expression_type(db, definition, &type_alias_stmt_node.value)
    }

    fn normalized_impl(self, _db: &'db dyn Db, _visitor: &TypeTransformer<'db>) -> Self {
        self
    }
}

/// # Ordering
/// Ordering is based on the type alias's salsa-assigned id and not on its values.
/// The id may change between runs, or when the alias was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct BareTypeAliasType<'db> {
    #[returns(ref)]
    pub name: ast::name::Name,
    pub definition: Option<Definition<'db>>,
    pub value: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BareTypeAliasType<'_> {}

fn walk_bare_type_alias<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    type_alias: BareTypeAliasType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, type_alias.value(db));
}

impl<'db> BareTypeAliasType<'db> {
    fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        Self::new(
            db,
            self.name(db),
            self.definition(db),
            self.value(db).normalized_impl(db, visitor),
        )
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, salsa::Update, get_size2::GetSize,
)]
pub enum TypeAliasType<'db> {
    PEP695(PEP695TypeAliasType<'db>),
    Bare(BareTypeAliasType<'db>),
}

fn walk_type_alias_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    type_alias: TypeAliasType<'db>,
    visitor: &V,
) {
    match type_alias {
        TypeAliasType::PEP695(type_alias) => {
            walk_pep_695_type_alias(db, type_alias, visitor);
        }
        TypeAliasType::Bare(type_alias) => {
            walk_bare_type_alias(db, type_alias, visitor);
        }
    }
}

impl<'db> TypeAliasType<'db> {
    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        match self {
            TypeAliasType::PEP695(type_alias) => {
                TypeAliasType::PEP695(type_alias.normalized_impl(db, visitor))
            }
            TypeAliasType::Bare(type_alias) => {
                TypeAliasType::Bare(type_alias.normalized_impl(db, visitor))
            }
        }
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db str {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.name(db),
            TypeAliasType::Bare(type_alias) => type_alias.name(db),
        }
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => Some(type_alias.definition(db)),
            TypeAliasType::Bare(type_alias) => type_alias.definition(db),
        }
    }

    pub(crate) fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.value_type(db),
            TypeAliasType::Bare(type_alias) => type_alias.value(db),
        }
    }
}

/// Either the explicit `metaclass=` keyword of the class, or the inferred metaclass of one of its base classes.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(super) struct MetaclassCandidate<'db> {
    metaclass: ClassType<'db>,
    explicit_metaclass_of: ClassLiteral<'db>,
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct UnionType<'db> {
    /// The union type includes values in any of these types.
    #[returns(deref)]
    pub elements: Box<[Type<'db>]>,
}

pub(crate) fn walk_union<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    union: UnionType<'db>,
    visitor: &V,
) {
    for element in union.elements(db) {
        visitor.visit_type(db, *element);
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for UnionType<'_> {}

impl<'db> UnionType<'db> {
    /// Create a union from a list of elements
    /// (which may be eagerly simplified into a different variant of [`Type`] altogether).
    pub fn from_elements<I, T>(db: &'db dyn Db, elements: I) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        elements
            .into_iter()
            .fold(UnionBuilder::new(db), |builder, element| {
                builder.add(element.into())
            })
            .build()
    }

    /// A fallible version of [`UnionType::from_elements`].
    ///
    /// If all items in `elements` are `Some()`, the result of unioning all elements is returned.
    /// As soon as a `None` element in the iterable is encountered,
    /// the function short-circuits and returns `None`.
    pub(crate) fn try_from_elements<I, T>(db: &'db dyn Db, elements: I) -> Option<Type<'db>>
    where
        I: IntoIterator<Item = Option<T>>,
        T: Into<Type<'db>>,
    {
        let mut builder = UnionBuilder::new(db);
        for element in elements {
            builder = builder.add(element?.into());
        }
        Some(builder.build())
    }

    /// Apply a transformation function to all elements of the union,
    /// and create a new union from the resulting set of types.
    pub fn map(
        &self,
        db: &'db dyn Db,
        transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        Self::from_elements(db, self.elements(db).iter().map(transform_fn))
    }

    /// A fallible version of [`UnionType::map`].
    ///
    /// For each element in `self`, `transform_fn` is called on that element.
    /// If `transform_fn` returns `Some()` for all elements in `self`,
    /// the result of unioning all transformed elements is returned.
    /// As soon as `transform_fn` returns `None` for an element, however,
    /// the function short-circuits and returns `None`.
    pub(crate) fn try_map(
        self,
        db: &'db dyn Db,
        transform_fn: impl FnMut(&Type<'db>) -> Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        Self::try_from_elements(db, self.elements(db).iter().map(transform_fn))
    }

    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Option<Type<'db>> {
        self.try_map(db, |element| element.to_instance(db))
    }

    pub(crate) fn filter(
        self,
        db: &'db dyn Db,
        filter_fn: impl FnMut(&&Type<'db>) -> bool,
    ) -> Type<'db> {
        Self::from_elements(db, self.elements(db).iter().filter(filter_fn))
    }

    pub fn iter(&self, db: &'db dyn Db) -> Iter<'_, Type<'db>> {
        self.elements(db).iter()
    }

    pub(crate) fn map_with_boundness(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Place<'db>,
    ) -> Place<'db> {
        let mut builder = UnionBuilder::new(db);

        let mut all_unbound = true;
        let mut possibly_unbound = false;
        for ty in self.elements(db) {
            let ty_member = transform_fn(ty);
            match ty_member {
                Place::Unbound => {
                    possibly_unbound = true;
                }
                Place::Type(ty_member, member_boundness) => {
                    if member_boundness == Boundness::PossiblyUnbound {
                        possibly_unbound = true;
                    }

                    all_unbound = false;
                    builder = builder.add(ty_member);
                }
            }
        }

        if all_unbound {
            Place::Unbound
        } else {
            Place::Type(
                builder.build(),
                if possibly_unbound {
                    Boundness::PossiblyUnbound
                } else {
                    Boundness::Bound
                },
            )
        }
    }

    pub(crate) fn map_with_boundness_and_qualifiers(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let mut builder = UnionBuilder::new(db);
        let mut qualifiers = TypeQualifiers::empty();

        let mut all_unbound = true;
        let mut possibly_unbound = false;
        for ty in self.elements(db) {
            let PlaceAndQualifiers {
                place: ty_member,
                qualifiers: new_qualifiers,
            } = transform_fn(ty);
            qualifiers |= new_qualifiers;
            match ty_member {
                Place::Unbound => {
                    possibly_unbound = true;
                }
                Place::Type(ty_member, member_boundness) => {
                    if member_boundness == Boundness::PossiblyUnbound {
                        possibly_unbound = true;
                    }

                    all_unbound = false;
                    builder = builder.add(ty_member);
                }
            }
        }
        PlaceAndQualifiers {
            place: if all_unbound {
                Place::Unbound
            } else {
                Place::Type(
                    builder.build(),
                    if possibly_unbound {
                        Boundness::PossiblyUnbound
                    } else {
                        Boundness::Bound
                    },
                )
            },
            qualifiers,
        }
    }

    /// Create a new union type with the elements normalized.
    ///
    /// See [`Type::normalized`] for more details.
    #[must_use]
    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        self.normalized_impl(db, &TypeTransformer::default())
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        let mut new_elements: Vec<Type<'db>> = self
            .elements(db)
            .iter()
            .map(|element| element.normalized_impl(db, visitor))
            .collect();
        new_elements.sort_unstable_by(|l, r| union_or_intersection_elements_ordering(db, l, r));
        UnionType::new(db, new_elements.into_boxed_slice())
    }

    /// Return `true` if `self` represents the exact same sets of possible runtime objects as `other`
    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }

        let self_elements = self.elements(db);
        let other_elements = other.elements(db);

        if self_elements.len() != other_elements.len() {
            return false;
        }

        let sorted_self = self.normalized(db);

        if sorted_self == other {
            return true;
        }

        sorted_self == other.normalized(db)
    }
}

#[salsa::interned(debug, heap_size=IntersectionType::heap_size)]
pub struct IntersectionType<'db> {
    /// The intersection type includes only values in all of these types.
    #[returns(ref)]
    positive: FxOrderSet<Type<'db>>,

    /// The intersection type does not include any value in any of these types.
    ///
    /// Negation types aren't expressible in annotations, and are most likely to arise from type
    /// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
    /// directly in intersections rather than as a separate type.
    #[returns(ref)]
    negative: FxOrderSet<Type<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for IntersectionType<'_> {}

pub(super) fn walk_intersection_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    intersection: IntersectionType<'db>,
    visitor: &V,
) {
    for element in intersection.positive(db) {
        visitor.visit_type(db, *element);
    }
    for element in intersection.negative(db) {
        visitor.visit_type(db, *element);
    }
}

impl<'db> IntersectionType<'db> {
    /// Return a new `IntersectionType` instance with the positive and negative types sorted
    /// according to a canonical ordering, and other normalizations applied to each element as applicable.
    ///
    /// See [`Type::normalized`] for more details.
    #[must_use]
    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        self.normalized_impl(db, &TypeTransformer::default())
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        fn normalized_set<'db>(
            db: &'db dyn Db,
            elements: &FxOrderSet<Type<'db>>,
            visitor: &TypeTransformer<'db>,
        ) -> FxOrderSet<Type<'db>> {
            let mut elements: FxOrderSet<Type<'db>> = elements
                .iter()
                .map(|ty| ty.normalized_impl(db, visitor))
                .collect();

            elements.sort_unstable_by(|l, r| union_or_intersection_elements_ordering(db, l, r));
            elements
        }

        IntersectionType::new(
            db,
            normalized_set(db, self.positive(db), visitor),
            normalized_set(db, self.negative(db), visitor),
        )
    }

    /// Return `true` if `self` represents exactly the same set of possible runtime objects as `other`
    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }

        let self_positive = self.positive(db);
        let other_positive = other.positive(db);

        if self_positive.len() != other_positive.len() {
            return false;
        }

        let self_negative = self.negative(db);
        let other_negative = other.negative(db);

        if self_negative.len() != other_negative.len() {
            return false;
        }

        let sorted_self = self.normalized(db);

        if sorted_self == other {
            return true;
        }

        sorted_self == other.normalized(db)
    }

    /// Returns an iterator over the positive elements of the intersection. If
    /// there are no positive elements, returns a single `object` type.
    fn positive_elements_or_object(&self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> {
        if self.positive(db).is_empty() {
            Either::Left(std::iter::once(Type::object(db)))
        } else {
            Either::Right(self.positive(db).iter().copied())
        }
    }

    pub(crate) fn map_with_boundness(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Place<'db>,
    ) -> Place<'db> {
        let mut builder = IntersectionBuilder::new(db);

        let mut all_unbound = true;
        let mut any_definitely_bound = false;
        for ty in self.positive_elements_or_object(db) {
            let ty_member = transform_fn(&ty);
            match ty_member {
                Place::Unbound => {}
                Place::Type(ty_member, member_boundness) => {
                    all_unbound = false;
                    if member_boundness == Boundness::Bound {
                        any_definitely_bound = true;
                    }

                    builder = builder.add_positive(ty_member);
                }
            }
        }

        if all_unbound {
            Place::Unbound
        } else {
            Place::Type(
                builder.build(),
                if any_definitely_bound {
                    Boundness::Bound
                } else {
                    Boundness::PossiblyUnbound
                },
            )
        }
    }

    pub(crate) fn map_with_boundness_and_qualifiers(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let mut builder = IntersectionBuilder::new(db);
        let mut qualifiers = TypeQualifiers::empty();

        let mut any_unbound = false;
        let mut any_possibly_unbound = false;
        for ty in self.positive_elements_or_object(db) {
            let PlaceAndQualifiers {
                place: member,
                qualifiers: new_qualifiers,
            } = transform_fn(&ty);
            qualifiers |= new_qualifiers;
            match member {
                Place::Unbound => {
                    any_unbound = true;
                }
                Place::Type(ty_member, member_boundness) => {
                    if member_boundness == Boundness::PossiblyUnbound {
                        any_possibly_unbound = true;
                    }

                    builder = builder.add_positive(ty_member);
                }
            }
        }

        PlaceAndQualifiers {
            place: if any_unbound {
                Place::Unbound
            } else {
                Place::Type(
                    builder.build(),
                    if any_possibly_unbound {
                        Boundness::PossiblyUnbound
                    } else {
                        Boundness::Bound
                    },
                )
            },
            qualifiers,
        }
    }

    pub fn iter_positive(&self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> {
        self.positive(db).iter().copied()
    }

    pub fn has_one_element(&self, db: &'db dyn Db) -> bool {
        (self.positive(db).len() + self.negative(db).len()) == 1
    }

    fn heap_size((positive, negative): &(FxOrderSet<Type<'db>>, FxOrderSet<Type<'db>>)) -> usize {
        ruff_memory_usage::order_set_heap_size(positive)
            + ruff_memory_usage::order_set_heap_size(negative)
    }
}

/// # Ordering
/// Ordering is based on the string literal's salsa-assigned id and not on its value.
/// The id may change between runs, or when the string literal was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct StringLiteralType<'db> {
    #[returns(deref)]
    value: Box<str>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for StringLiteralType<'_> {}

impl<'db> StringLiteralType<'db> {
    /// The length of the string, as would be returned by Python's `len()`.
    pub(crate) fn python_len(self, db: &'db dyn Db) -> usize {
        self.value(db).chars().count()
    }

    /// Return an iterator over each character in the string literal.
    /// as would be returned by Python's `iter()`.
    pub(crate) fn iter_each_char(self, db: &'db dyn Db) -> impl Iterator<Item = Self> {
        self.value(db)
            .chars()
            .map(|c| StringLiteralType::new(db, c.to_string().into_boxed_str()))
    }
}

/// # Ordering
/// Ordering is based on the byte literal's salsa-assigned id and not on its value.
/// The id may change between runs, or when the byte literal was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct BytesLiteralType<'db> {
    #[returns(deref)]
    value: Box<[u8]>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BytesLiteralType<'_> {}

impl<'db> BytesLiteralType<'db> {
    pub(crate) fn python_len(self, db: &'db dyn Db) -> usize {
        self.value(db).len()
    }
}

/// A singleton type corresponding to a specific enum member.
///
/// For the enum variant `Answer.YES` of the enum below, this type would store
/// a reference to `Answer` in `enum_class` and the name "YES" in `name`.
/// ```py
/// class Answer(Enum):
///     NO = 0
///     YES = 1
/// ```
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct EnumLiteralType<'db> {
    /// A reference to the enum class this literal belongs to
    enum_class: ClassLiteral<'db>,
    /// The name of the enum member
    #[returns(ref)]
    name: Name,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for EnumLiteralType<'_> {}

impl<'db> EnumLiteralType<'db> {
    pub fn enum_class_instance(self, db: &'db dyn Db) -> Type<'db> {
        self.enum_class(db).to_non_generic_instance(db)
    }
}

/// Type that represents the set of all inhabitants (`dict` instances) that conform to
/// a given `TypedDict` schema.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct TypedDictType<'db> {
    /// A reference to the class (inheriting from `typing.TypedDict`) that specifies the
    /// schema of this `TypedDict`.
    defining_class: ClassType<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypedDictType<'_> {}

impl<'db> TypedDictType<'db> {
    pub(crate) fn from(db: &'db dyn Db, defining_class: ClassType<'db>) -> Type<'db> {
        Type::TypedDict(Self::new(db, defining_class))
    }

    pub(crate) fn items(self, db: &'db dyn Db) -> FxOrderMap<Name, Field<'db>> {
        let (class_literal, specialization) = self.defining_class(db).class_literal(db);
        class_literal.fields(db, specialization, CodeGeneratorKind::TypedDict)
    }

    pub(crate) fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        Self::new(
            db,
            self.defining_class(db).apply_type_mapping(db, type_mapping),
        )
    }
}

fn walk_typed_dict_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    typed_dict: TypedDictType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, typed_dict.defining_class(db).into());
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BoundSuperError<'db> {
    InvalidPivotClassType {
        pivot_class: Type<'db>,
    },
    FailingConditionCheck {
        pivot_class: Type<'db>,
        owner: Type<'db>,
    },
    UnavailableImplicitArguments,
}

impl BoundSuperError<'_> {
    pub(super) fn report_diagnostic(&self, context: &InferContext, node: AnyNodeRef) {
        match self {
            BoundSuperError::InvalidPivotClassType { pivot_class } => {
                if let Some(builder) = context.report_lint(&INVALID_SUPER_ARGUMENT, node) {
                    builder.into_diagnostic(format_args!(
                        "`{pivot_class}` is not a valid class",
                        pivot_class = pivot_class.display(context.db()),
                    ));
                }
            }
            BoundSuperError::FailingConditionCheck { pivot_class, owner } => {
                if let Some(builder) = context.report_lint(&INVALID_SUPER_ARGUMENT, node) {
                    builder.into_diagnostic(format_args!(
                        "`{owner}` is not an instance or subclass of \
                         `{pivot_class}` in `super({pivot_class}, {owner})` call",
                        pivot_class = pivot_class.display(context.db()),
                        owner = owner.display(context.db()),
                    ));
                }
            }
            BoundSuperError::UnavailableImplicitArguments => {
                if let Some(builder) =
                    context.report_lint(&UNAVAILABLE_IMPLICIT_SUPER_ARGUMENTS, node)
                {
                    builder.into_diagnostic(format_args!(
                        "Cannot determine implicit arguments for 'super()' in this context",
                    ));
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, get_size2::GetSize)]
pub enum SuperOwnerKind<'db> {
    Dynamic(DynamicType),
    Class(ClassType<'db>),
    Instance(NominalInstanceType<'db>),
}

impl<'db> SuperOwnerKind<'db> {
    fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        match self {
            SuperOwnerKind::Dynamic(dynamic) => SuperOwnerKind::Dynamic(dynamic.normalized()),
            SuperOwnerKind::Class(class) => {
                SuperOwnerKind::Class(class.normalized_impl(db, visitor))
            }
            SuperOwnerKind::Instance(instance) => instance
                .normalized_impl(db, visitor)
                .into_nominal_instance()
                .map(Self::Instance)
                .unwrap_or(Self::Dynamic(DynamicType::Any)),
        }
    }

    fn iter_mro(self, db: &'db dyn Db) -> impl Iterator<Item = ClassBase<'db>> {
        match self {
            SuperOwnerKind::Dynamic(dynamic) => {
                Either::Left(ClassBase::Dynamic(dynamic).mro(db, None))
            }
            SuperOwnerKind::Class(class) => Either::Right(class.iter_mro(db)),
            SuperOwnerKind::Instance(instance) => Either::Right(instance.class(db).iter_mro(db)),
        }
    }

    fn into_type(self) -> Type<'db> {
        match self {
            SuperOwnerKind::Dynamic(dynamic) => Type::Dynamic(dynamic),
            SuperOwnerKind::Class(class) => class.into(),
            SuperOwnerKind::Instance(instance) => instance.into(),
        }
    }

    fn into_class(self, db: &'db dyn Db) -> Option<ClassType<'db>> {
        match self {
            SuperOwnerKind::Dynamic(_) => None,
            SuperOwnerKind::Class(class) => Some(class),
            SuperOwnerKind::Instance(instance) => Some(instance.class(db)),
        }
    }

    fn try_from_type(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        match ty {
            Type::Dynamic(dynamic) => Some(SuperOwnerKind::Dynamic(dynamic)),
            Type::ClassLiteral(class_literal) => Some(SuperOwnerKind::Class(
                class_literal.apply_optional_specialization(db, None),
            )),
            Type::NominalInstance(instance) => Some(SuperOwnerKind::Instance(instance)),
            Type::BooleanLiteral(_) => {
                SuperOwnerKind::try_from_type(db, KnownClass::Bool.to_instance(db))
            }
            Type::IntLiteral(_) => {
                SuperOwnerKind::try_from_type(db, KnownClass::Int.to_instance(db))
            }
            Type::StringLiteral(_) => {
                SuperOwnerKind::try_from_type(db, KnownClass::Str.to_instance(db))
            }
            Type::LiteralString => {
                SuperOwnerKind::try_from_type(db, KnownClass::Str.to_instance(db))
            }
            Type::BytesLiteral(_) => {
                SuperOwnerKind::try_from_type(db, KnownClass::Bytes.to_instance(db))
            }
            Type::SpecialForm(special_form) => {
                SuperOwnerKind::try_from_type(db, special_form.instance_fallback(db))
            }
            _ => None,
        }
    }
}

impl<'db> From<SuperOwnerKind<'db>> for Type<'db> {
    fn from(owner: SuperOwnerKind<'db>) -> Self {
        match owner {
            SuperOwnerKind::Dynamic(dynamic) => Type::Dynamic(dynamic),
            SuperOwnerKind::Class(class) => class.into(),
            SuperOwnerKind::Instance(instance) => instance.into(),
        }
    }
}

/// Represent a bound super object like `super(PivotClass, owner)`
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct BoundSuperType<'db> {
    pub pivot_class: ClassBase<'db>,
    pub owner: SuperOwnerKind<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BoundSuperType<'_> {}

fn walk_bound_super_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    bound_super: BoundSuperType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, bound_super.pivot_class(db).into());
    visitor.visit_type(db, bound_super.owner(db).into_type());
}

impl<'db> BoundSuperType<'db> {
    /// Attempts to build a `Type::BoundSuper` based on the given `pivot_class` and `owner`.
    ///
    /// This mimics the behavior of Python's built-in `super(pivot, owner)` at runtime.
    /// - `super(pivot, owner_class)` is valid only if `issubclass(owner_class, pivot)`
    /// - `super(pivot, owner_instance)` is valid only if `isinstance(owner_instance, pivot)`
    ///
    /// However, the checking is skipped when any of the arguments is a dynamic type.
    fn build(
        db: &'db dyn Db,
        pivot_class_type: Type<'db>,
        owner_type: Type<'db>,
    ) -> Result<Type<'db>, BoundSuperError<'db>> {
        if let Type::Union(union) = owner_type {
            return Ok(UnionType::from_elements(
                db,
                union
                    .elements(db)
                    .iter()
                    .map(|ty| BoundSuperType::build(db, pivot_class_type, *ty))
                    .collect::<Result<Vec<_>, _>>()?,
            ));
        }

        let pivot_class = ClassBase::try_from_type(db, pivot_class_type).ok_or({
            BoundSuperError::InvalidPivotClassType {
                pivot_class: pivot_class_type,
            }
        })?;

        let owner = SuperOwnerKind::try_from_type(db, owner_type)
            .and_then(|owner| {
                let Some(pivot_class) = pivot_class.into_class() else {
                    return Some(owner);
                };
                let Some(owner_class) = owner.into_class(db) else {
                    return Some(owner);
                };
                if owner_class.is_subclass_of(db, pivot_class) {
                    Some(owner)
                } else {
                    None
                }
            })
            .ok_or(BoundSuperError::FailingConditionCheck {
                pivot_class: pivot_class_type,
                owner: owner_type,
            })?;

        Ok(Type::BoundSuper(BoundSuperType::new(
            db,
            pivot_class,
            owner,
        )))
    }

    /// Skips elements in the MRO up to and including the pivot class.
    ///
    /// If the pivot class is a dynamic type, its MRO can't be determined,
    /// so we fall back to using the MRO of `DynamicType::Unknown`.
    fn skip_until_after_pivot(
        self,
        db: &'db dyn Db,
        mro_iter: impl Iterator<Item = ClassBase<'db>>,
    ) -> impl Iterator<Item = ClassBase<'db>> {
        let Some(pivot_class) = self.pivot_class(db).into_class() else {
            return Either::Left(ClassBase::Dynamic(DynamicType::Unknown).mro(db, None));
        };

        let mut pivot_found = false;

        Either::Right(mro_iter.skip_while(move |superclass| {
            if pivot_found {
                false
            } else if Some(pivot_class) == superclass.into_class() {
                pivot_found = true;
                true
            } else {
                true
            }
        }))
    }

    /// Tries to call `__get__` on the attribute.
    /// The arguments passed to `__get__` depend on whether the owner is an instance or a class.
    /// See the `CPython` implementation for reference:
    /// <https://github.com/python/cpython/blob/3b3720f1a26ab34377542b48eb6a6565f78ff892/Objects/typeobject.c#L11690-L11693>
    fn try_call_dunder_get_on_attribute(
        self,
        db: &'db dyn Db,
        attribute: PlaceAndQualifiers<'db>,
    ) -> Option<PlaceAndQualifiers<'db>> {
        let owner = self.owner(db);

        match owner {
            // If the owner is a dynamic type, we can't tell whether it's a class or an instance.
            // Also, invoking a descriptor on a dynamic attribute is meaningless, so we don't handle this.
            SuperOwnerKind::Dynamic(_) => None,
            SuperOwnerKind::Class(_) => Some(
                Type::try_call_dunder_get_on_attribute(
                    db,
                    attribute,
                    Type::none(db),
                    owner.into_type(),
                )
                .0,
            ),
            SuperOwnerKind::Instance(_) => Some(
                Type::try_call_dunder_get_on_attribute(
                    db,
                    attribute,
                    owner.into_type(),
                    owner.into_type().to_meta_type(db),
                )
                .0,
            ),
        }
    }

    /// Similar to `Type::find_name_in_mro_with_policy`, but performs lookup starting *after* the
    /// pivot class in the MRO, based on the `owner` type instead of the `super` type.
    fn find_name_in_mro_after_pivot(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        let owner = self.owner(db);
        let class = match owner {
            SuperOwnerKind::Dynamic(_) => {
                return owner
                    .into_type()
                    .find_name_in_mro_with_policy(db, name, policy)
                    .expect("Calling `find_name_in_mro` on dynamic type should return `Some`");
            }
            SuperOwnerKind::Class(class) => class,
            SuperOwnerKind::Instance(instance) => instance.class(db),
        };

        let (class_literal, _) = class.class_literal(db);
        // TODO properly support super() with generic types
        // * requires a fix for https://github.com/astral-sh/ruff/issues/17432
        // * also requires understanding how we should handle cases like this:
        //  ```python
        //  b_int: B[int]
        //  b_unknown: B
        //
        //  super(B, b_int)
        //  super(B[int], b_unknown)
        //  ```
        match class_literal.generic_context(db) {
            Some(_) => Place::bound(todo_type!("super in generic class")).into(),
            None => class_literal.class_member_from_mro(
                db,
                name,
                policy,
                self.skip_until_after_pivot(db, owner.iter_mro(db)),
            ),
        }
    }

    pub(super) fn normalized_impl(self, db: &'db dyn Db, visitor: &TypeTransformer<'db>) -> Self {
        Self::new(
            db,
            self.pivot_class(db).normalized_impl(db, visitor),
            self.owner(db).normalized_impl(db, visitor),
        )
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeIsType<'db> {
    return_type: Type<'db>,
    /// The ID of the scope to which the place belongs
    /// and the ID of the place itself within that scope.
    place_info: Option<(ScopeId<'db>, ScopedPlaceId)>,
}

fn walk_typeis_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    typeis_type: TypeIsType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, typeis_type.return_type(db));
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypeIsType<'_> {}

impl<'db> TypeIsType<'db> {
    pub fn place_name(self, db: &'db dyn Db) -> Option<String> {
        let (scope, place) = self.place_info(db)?;
        let table = place_table(db, scope);

        Some(format!("{}", table.place(place)))
    }

    pub fn unbound(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeIs(Self::new(db, ty, None))
    }

    pub fn bound(
        db: &'db dyn Db,
        return_type: Type<'db>,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
    ) -> Type<'db> {
        Type::TypeIs(Self::new(db, return_type, Some((scope, place))))
    }

    #[must_use]
    pub fn bind(self, db: &'db dyn Db, scope: ScopeId<'db>, place: ScopedPlaceId) -> Type<'db> {
        Self::bound(db, self.return_type(db), scope, place)
    }

    #[must_use]
    pub fn with_type(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeIs(Self::new(db, ty, self.place_info(db)))
    }

    pub fn is_bound(&self, db: &'db dyn Db) -> bool {
        self.place_info(db).is_some()
    }

    pub fn is_unbound(&self, db: &'db dyn Db) -> bool {
        self.place_info(db).is_none()
    }
}

// Make sure that the `Type` enum does not grow unexpectedly.
#[cfg(not(debug_assertions))]
#[cfg(target_pointer_width = "64")]
static_assertions::assert_eq_size!(Type, [u8; 16]);

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::tests::{TestDbBuilder, setup_db};
    use crate::place::{global_symbol, typing_extensions_symbol, typing_symbol};
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast::PythonVersion;
    use test_case::test_case;

    /// Explicitly test for Python version <3.13 and >=3.13, to ensure that
    /// the fallback to `typing_extensions` is working correctly.
    /// See [`KnownClass::canonical_module`] for more information.
    #[test_case(PythonVersion::PY312)]
    #[test_case(PythonVersion::PY313)]
    fn no_default_type_is_singleton(python_version: PythonVersion) {
        let db = TestDbBuilder::new()
            .with_python_version(python_version)
            .build()
            .unwrap();

        let no_default = KnownClass::NoDefaultType.to_instance(&db);

        assert!(no_default.is_singleton(&db));
    }

    #[test]
    fn typing_vs_typeshed_no_default() {
        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .build()
            .unwrap();

        let typing_no_default = typing_symbol(&db, "NoDefault").place.expect_type();
        let typing_extensions_no_default = typing_extensions_symbol(&db, "NoDefault")
            .place
            .expect_type();

        assert_eq!(typing_no_default.display(&db).to_string(), "NoDefault");
        assert_eq!(
            typing_extensions_no_default.display(&db).to_string(),
            "NoDefault"
        );
    }

    /// Inferring the result of a call-expression shouldn't need to re-run after
    /// a trivial change to the function's file (e.g. by adding a docstring to the function).
    #[test]
    fn call_type_doesnt_rerun_when_only_callee_changed() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_dedented(
            "src/foo.py",
            r#"
            def foo() -> int:
                return 5
        "#,
        )?;
        db.write_dedented(
            "src/bar.py",
            r#"
            from foo import foo

            a = foo()
            "#,
        )?;

        let bar = system_path_to_file(&db, "src/bar.py")?;
        let a = global_symbol(&db, bar, "a").place;

        assert_eq!(
            a.expect_type(),
            UnionType::from_elements(&db, [Type::unknown(), KnownClass::Int.to_instance(&db)])
        );

        // Add a docstring to foo to trigger a re-run.
        // The bar-call site of foo should not be re-run because of that
        db.write_dedented(
            "src/foo.py",
            r#"
            def foo() -> int:
                "Computes a value"
                return 5
            "#,
        )?;
        db.clear_salsa_events();

        let a = global_symbol(&db, bar, "a").place;

        assert_eq!(
            a.expect_type(),
            UnionType::from_elements(&db, [Type::unknown(), KnownClass::Int.to_instance(&db)])
        );
        let events = db.take_salsa_events();

        let module = parsed_module(&db, bar).load(&db);
        let call = &*module.syntax().body[1].as_assign_stmt().unwrap().value;
        let foo_call = semantic_index(&db, bar).expression(call);

        assert_function_query_was_not_run(&db, infer_expression_types, foo_call, &events);

        Ok(())
    }

    /// All other tests also make sure that `Type::Todo` works as expected. This particular
    /// test makes sure that we handle `Todo` types correctly, even if they originate from
    /// different sources.
    #[test]
    fn todo_types() {
        let db = setup_db();

        let todo1 = todo_type!("1");
        let todo2 = todo_type!("2");

        let int = KnownClass::Int.to_instance(&db);

        assert!(int.is_assignable_to(&db, todo1));

        assert!(todo1.is_assignable_to(&db, int));

        // We lose information when combining several `Todo` types. This is an
        // acknowledged limitation of the current implementation. We can not
        // easily store the meta information of several `Todo`s in a single
        // variant, as `TodoType` needs to implement `Copy`, meaning it can't
        // contain `Vec`/`Box`/etc., and can't be boxed itself.
        //
        // Lifting this restriction would require us to intern `TodoType` in
        // salsa, but that would mean we would have to pass in `db` everywhere.

        // A union of several `Todo` types collapses to a single `Todo` type:
        assert!(UnionType::from_elements(&db, vec![todo1, todo2]).is_todo());

        // And similar for intersection types:
        assert!(
            IntersectionBuilder::new(&db)
                .add_positive(todo1)
                .add_positive(todo2)
                .build()
                .is_todo()
        );
        assert!(
            IntersectionBuilder::new(&db)
                .add_positive(todo1)
                .add_negative(todo2)
                .build()
                .is_todo()
        );
    }
}
