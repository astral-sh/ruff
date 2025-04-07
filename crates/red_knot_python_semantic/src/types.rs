use std::collections::HashMap;
use std::slice::Iter;
use std::str::FromStr;

use bitflags::bitflags;
use call::{CallDunderError, CallError, CallErrorKind};
use context::InferContext;
use diagnostic::{CALL_POSSIBLY_UNBOUND_METHOD, INVALID_CONTEXT_MANAGER, NOT_ITERABLE};
use itertools::EitherOrBoth;
use ruff_db::files::{File, FileRange};
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_text_size::{Ranged, TextRange};
use type_ordering::union_or_intersection_elements_ordering;

pub(crate) use self::builder::{IntersectionBuilder, UnionBuilder};
pub(crate) use self::diagnostic::register_lints;
pub use self::diagnostic::TypeCheckDiagnostics;
pub(crate) use self::display::TypeArrayDisplay;
pub(crate) use self::infer::{
    infer_deferred_types, infer_definition_types, infer_expression_type, infer_expression_types,
    infer_scope_types,
};
pub(crate) use self::narrow::KnownConstraintFunction;
pub(crate) use self::signatures::{CallableSignature, Signature, Signatures};
pub(crate) use self::subclass_of::SubclassOfType;
use crate::module_name::ModuleName;
use crate::module_resolver::{file_to_module, resolve_module, KnownModule};
use crate::semantic_index::ast_ids::HasScopedExpressionId;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::ScopeId;
use crate::semantic_index::{imported_modules, semantic_index};
use crate::suppression::check_suppressions;
use crate::symbol::{imported_symbol, Boundness, Symbol, SymbolAndQualifiers};
use crate::types::call::{Bindings, CallArgumentTypes};
pub(crate) use crate::types::class_base::ClassBase;
use crate::types::diagnostic::{INVALID_TYPE_FORM, UNSUPPORTED_BOOL_CONVERSION};
use crate::types::infer::infer_unpack_types;
use crate::types::mro::{Mro, MroError, MroIterator};
pub(crate) use crate::types::narrow::infer_narrowing_constraint;
use crate::types::signatures::{Parameter, ParameterForm, ParameterKind, Parameters};
use crate::{Db, FxOrderSet, Module, Program};
pub(crate) use class::{Class, ClassLiteralType, InstanceType, KnownClass, KnownInstanceType};

mod builder;
mod call;
mod class;
mod class_base;
mod context;
mod diagnostic;
mod display;
mod infer;
mod mro;
mod narrow;
mod signatures;
mod slots;
mod string_annotation;
mod subclass_of;
mod type_ordering;
mod unpacker;

mod definition;
#[cfg(test)]
mod property_tests;

#[salsa::tracked(return_ref)]
pub fn check_types(db: &dyn Db, file: File) -> TypeCheckDiagnostics {
    let _span = tracing::trace_span!("check_types", ?file).entered();

    tracing::debug!("Checking file '{path}'", path = file.path(db));

    let index = semantic_index(db, file);
    let mut diagnostics = TypeCheckDiagnostics::default();

    for scope_id in index.scope_ids() {
        let result = infer_scope_types(db, scope_id);
        diagnostics.extend(result.diagnostics());
    }

    check_suppressions(db, file, &mut diagnostics);

    diagnostics
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
    let expr_id = expression.scoped_expression_id(db, scope);
    if scope == definition.scope(db) {
        // expression is in the definition scope
        let inference = infer_definition_types(db, definition);
        if let Some(ty) = inference.try_expression_type(expr_id) {
            ty
        } else {
            infer_deferred_types(db, definition).expression_type(expr_id)
        }
    } else {
        // expression is in a type-params sub-scope
        infer_scope_types(db, scope).expression_type(expr_id)
    }
}

/// The descriptor protocol distinguishes two kinds of descriptors. Non-data descriptors
/// define a `__get__` method, while data descriptors additionally define a `__set__`
/// method or a `__delete__` method. This enum is used to categorize attributes into two
/// groups: (1) data descriptors and (2) normal attributes or non-data descriptors.
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, salsa::Update)]
enum AttributeKind {
    DataDescriptor,
    NormalOrNonDataDescriptor,
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
}

impl Default for MemberLookupPolicy {
    fn default() -> Self {
        Self::empty()
    }
}

impl AttributeKind {
    const fn is_data(self) -> bool {
        matches!(self, Self::DataDescriptor)
    }
}

/// Meta data for `Type::Todo`, which represents a known limitation in red-knot.
#[cfg(debug_assertions)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TodoType(pub &'static str);

#[cfg(debug_assertions)]
impl std::fmt::Display for TodoType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({msg})", msg = self.0)
    }
}

#[cfg(not(debug_assertions))]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
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
    ($message:literal) => {
        $crate::types::Type::Dynamic($crate::types::DynamicType::Todo($crate::types::TodoType(
            $message,
        )))
    };
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
#[salsa::interned(debug)]
pub struct PropertyInstanceType<'db> {
    getter: Option<Type<'db>>,
    setter: Option<Type<'db>>,
}

/// Representation of a type: a set of possible values at runtime.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
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
    /// The type of an arbitrary callable object with a certain specified signature.
    Callable(CallableType<'db>),
    /// A specific module object
    ModuleLiteral(ModuleLiteralType<'db>),
    /// A specific class object
    ClassLiteral(ClassLiteralType<'db>),
    // The set of all class objects that are subclasses of the given class (C), spelled `type[C]`.
    SubclassOf(SubclassOfType<'db>),
    /// The set of Python objects with the given class in their __class__'s method resolution order
    Instance(InstanceType<'db>),
    /// A single Python object that requires special treatment in the type system
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
    /// A string known to originate only from literal values, but whose value is not known (unlike
    /// `StringLiteral` above).
    LiteralString,
    /// A bytes literal
    BytesLiteral(BytesLiteralType<'db>),
    /// A slice literal, e.g. `1:5`, `10:0:-1` or `:`
    SliceLiteral(SliceLiteralType<'db>),
    /// A heterogeneous tuple type, with elements of the given types in source order.
    // TODO: Support variable length homogeneous tuple type like `tuple[int, ...]`.
    Tuple(TupleType<'db>),
    /// An instance of a typevar in a generic class or function. When the generic class or function
    /// is specialized, we will replace this typevar with its specialization.
    TypeVar(TypeVarInstance<'db>),
    // TODO protocols, overloads, generics
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

    fn is_none(&self, db: &'db dyn Db) -> bool {
        self.into_instance()
            .is_some_and(|instance| instance.class().is_known(db, KnownClass::NoneType))
    }

    pub fn is_notimplemented(&self, db: &'db dyn Db) -> bool {
        self.into_instance().is_some_and(|instance| {
            instance
                .class()
                .is_known(db, KnownClass::NotImplementedType)
        })
    }

    pub fn is_object(&self, db: &'db dyn Db) -> bool {
        self.into_instance()
            .is_some_and(|instance| instance.class().is_object(db))
    }

    pub const fn is_todo(&self) -> bool {
        matches!(self, Type::Dynamic(DynamicType::Todo(_)))
    }

    pub fn contains_todo(&self, db: &'db dyn Db) -> bool {
        match self {
            Self::Dynamic(DynamicType::Todo(_) | DynamicType::TodoProtocol) => true,

            Self::AlwaysFalsy
            | Self::AlwaysTruthy
            | Self::Never
            | Self::BooleanLiteral(_)
            | Self::BytesLiteral(_)
            | Self::FunctionLiteral(_)
            | Self::Instance(_)
            | Self::ModuleLiteral(_)
            | Self::ClassLiteral(_)
            | Self::KnownInstance(_)
            | Self::PropertyInstance(_)
            | Self::StringLiteral(_)
            | Self::IntLiteral(_)
            | Self::LiteralString
            | Self::SliceLiteral(_)
            | Self::Dynamic(DynamicType::Unknown | DynamicType::Any)
            | Self::BoundMethod(_)
            | Self::WrapperDescriptor(_)
            | Self::MethodWrapper(_) => false,

            Self::Callable(callable) => {
                let signature = callable.signature(db);
                signature.parameters().iter().any(|param| {
                    param
                        .annotated_type()
                        .is_some_and(|ty| ty.contains_todo(db))
                }) || signature.return_ty.is_some_and(|ty| ty.contains_todo(db))
            }

            Self::SubclassOf(subclass_of) => match subclass_of.subclass_of() {
                ClassBase::Dynamic(DynamicType::Todo(_) | DynamicType::TodoProtocol) => true,
                ClassBase::Dynamic(DynamicType::Unknown | DynamicType::Any) => false,
                ClassBase::Class(_) => false,
            },

            Self::TypeVar(typevar) => match typevar.bound_or_constraints(db) {
                None => false,
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound.contains_todo(db),
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                    .elements(db)
                    .iter()
                    .any(|constraint| constraint.contains_todo(db)),
            },

            Self::Tuple(tuple) => tuple.elements(db).iter().any(|ty| ty.contains_todo(db)),

            Self::Union(union) => union.elements(db).iter().any(|ty| ty.contains_todo(db)),

            Self::Intersection(intersection) => {
                intersection
                    .positive(db)
                    .iter()
                    .any(|ty| ty.contains_todo(db))
                    || intersection
                        .negative(db)
                        .iter()
                        .any(|ty| ty.contains_todo(db))
            }
        }
    }

    pub const fn class_literal(class: Class<'db>) -> Self {
        Self::ClassLiteral(ClassLiteralType { class })
    }

    pub const fn into_class_literal(self) -> Option<ClassLiteralType<'db>> {
        match self {
            Type::ClassLiteral(class_type) => Some(class_type),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_class_literal(self) -> ClassLiteralType<'db> {
        self.into_class_literal()
            .expect("Expected a Type::ClassLiteral variant")
    }

    pub const fn is_subclass_of(&self) -> bool {
        matches!(self, Type::SubclassOf(..))
    }

    pub const fn is_class_literal(&self) -> bool {
        matches!(self, Type::ClassLiteral(..))
    }

    pub const fn is_instance(&self) -> bool {
        matches!(self, Type::Instance(..))
    }

    pub const fn is_property_instance(&self) -> bool {
        matches!(self, Type::PropertyInstance(..))
    }

    pub fn module_literal(db: &'db dyn Db, importing_file: File, submodule: Module) -> Self {
        Self::ModuleLiteral(ModuleLiteralType::new(db, importing_file, submodule))
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
        self.into_union()
            .is_some_and(|union| union.elements(db).iter().all(|ty| ty.is_single_valued(db)))
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

    pub const fn into_instance(self) -> Option<InstanceType<'db>> {
        match self {
            Type::Instance(instance_type) => Some(instance_type),
            _ => None,
        }
    }

    pub const fn into_known_instance(self) -> Option<KnownInstanceType<'db>> {
        match self {
            Type::KnownInstance(known_instance) => Some(known_instance),
            _ => None,
        }
    }

    #[track_caller]
    pub fn expect_known_instance(self) -> KnownInstanceType<'db> {
        self.into_known_instance()
            .expect("Expected a Type::KnownInstance variant")
    }

    pub const fn into_tuple(self) -> Option<TupleType<'db>> {
        match self {
            Type::Tuple(tuple_type) => Some(tuple_type),
            _ => None,
        }
    }

    pub const fn is_boolean_literal(&self) -> bool {
        matches!(self, Type::BooleanLiteral(..))
    }

    pub const fn is_literal_string(&self) -> bool {
        matches!(self, Type::LiteralString)
    }

    pub const fn instance(class: Class<'db>) -> Self {
        Self::Instance(InstanceType { class })
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
        if yes {
            self.negate(db)
        } else {
            *self
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
    #[must_use]
    pub fn normalized(self, db: &'db dyn Db) -> Self {
        match self {
            Type::Union(union) => Type::Union(union.normalized(db)),
            Type::Intersection(intersection) => Type::Intersection(intersection.normalized(db)),
            Type::Tuple(tuple) => Type::Tuple(tuple.normalized(db)),
            Type::Callable(callable) => Type::Callable(callable.normalized(db)),
            Type::LiteralString
            | Type::Instance(_)
            | Type::PropertyInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::BooleanLiteral(_)
            | Type::SliceLiteral(_)
            | Type::BytesLiteral(_)
            | Type::StringLiteral(_)
            | Type::Dynamic(_)
            | Type::Never
            | Type::FunctionLiteral(_)
            | Type::MethodWrapper(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::KnownInstance(_)
            | Type::IntLiteral(_)
            | Type::SubclassOf(_) => self,
            Type::TypeVar(typevar) => match typevar.bound_or_constraints(db) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    Type::TypeVar(TypeVarInstance::new(
                        db,
                        typevar.name(db).clone(),
                        typevar.definition(db),
                        Some(TypeVarBoundOrConstraints::UpperBound(bound.normalized(db))),
                        typevar.default_ty(db),
                    ))
                }
                Some(TypeVarBoundOrConstraints::Constraints(union)) => {
                    Type::TypeVar(TypeVarInstance::new(
                        db,
                        typevar.name(db).clone(),
                        typevar.definition(db),
                        Some(TypeVarBoundOrConstraints::Constraints(union.normalized(db))),
                        typevar.default_ty(db),
                    ))
                }
                None => self,
            },
        }
    }

    /// Return true if this type is a [subtype of] type `target`.
    ///
    /// This method returns `false` if either `self` or `other` is not fully static.
    ///
    /// [subtype of]: https://typing.readthedocs.io/en/latest/spec/concepts.html#subtype-supertype-and-type-equivalence
    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        // Two equivalent types are always subtypes of each other.
        //
        // "Equivalent to" here means that the two types are both fully static
        // and describe exactly the same set of possible runtime objects.
        // For example, `int` is a subtype of `int` because `int` and `int` are equivalent to each other.
        // Equally, `type[object]` is a subtype of `type`,
        // because the former type expresses "all subclasses of `object`"
        // while the latter expresses "all instances of `type`",
        // and these are exactly the same set of objects at runtime.
        if self.is_equivalent_to(db, target) {
            return true;
        }

        // Non-fully-static types do not participate in subtyping.
        //
        // Type `A` can only be a subtype of type `B` if the set of possible runtime objects
        // that `A` represents is a subset of the set of possible runtime objects that `B` represents.
        // But the set of objects described by a non-fully-static type is (either partially or wholly) unknown,
        // so the question is simply unanswerable for non-fully-static types.
        if !self.is_fully_static(db) || !target.is_fully_static(db) {
            return false;
        }

        match (self, target) {
            // We should have handled these immediately above.
            (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => {
                unreachable!("Non-fully-static types do not participate in subtyping!")
            }

            // `Never` is the bottom type, the empty set.
            // It is a subtype of all other fully static types.
            // No other fully static type is a subtype of `Never`.
            (Type::Never, _) => true,
            (_, Type::Never) => false,

            // Everything is a subtype of `object`.
            (_, Type::Instance(InstanceType { class })) if class.is_object(db) => true,

            // A fully static typevar is always a subtype of itself, and is never a subtype of any
            // other typevar, since there is no guarantee that they will be specialized to the same
            // type. (This is true even if both typevars are bounded by the same final class, since
            // you can specialize the typevars to `Never` in addition to that final class.)
            (Type::TypeVar(self_typevar), Type::TypeVar(other_typevar)) => {
                self_typevar == other_typevar
            }

            // A fully static typevar is a subtype of its upper bound, and to something similar to
            // the union of its constraints. An unbound, unconstrained, fully static typevar has an
            // implicit upper bound of `object` (which is handled above).
            (Type::TypeVar(typevar), _) if typevar.bound_or_constraints(db).is_some() => {
                match typevar.bound_or_constraints(db) {
                    None => unreachable!(),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.is_subtype_of(db, target)
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .elements(db)
                        .iter()
                        .all(|constraint| constraint.is_subtype_of(db, target)),
                }
            }

            (Type::Union(union), _) => union
                .elements(db)
                .iter()
                .all(|&elem_ty| elem_ty.is_subtype_of(db, target)),

            (_, Type::Union(union)) => union
                .elements(db)
                .iter()
                .any(|&elem_ty| self.is_subtype_of(db, elem_ty)),

            (_, Type::TypeVar(typevar)) => match typevar.bound_or_constraints(db) {
                // No types are a subtype of a bounded typevar, or of an unbounded unconstrained
                // typevar, since there's no guarantee what type the typevar will be specialized
                // to. If the typevar is bounded, it might be specialized to a smaller type than
                // the bound. (This is true even if the bound is a final class, since the typevar
                // can still be specialized to `Never`.)
                None => false,
                Some(TypeVarBoundOrConstraints::UpperBound(_)) => false,
                // If the typevar is constrained, there must be multiple constraints, and the
                // typevar might be specialized to any one of them. However, the constraints do not
                // have to be disjoint, which means an lhs type might be a subtype of all of the
                // constraints.
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                    .elements(db)
                    .iter()
                    .all(|constraint| self.is_subtype_of(db, *constraint)),
            },

            // If both sides are intersections we need to handle the right side first
            // (A & B & C) is a subtype of (A & B) because the left is a subtype of both A and B,
            // but none of A, B, or C is a subtype of (A & B).
            (_, Type::Intersection(intersection)) => {
                intersection
                    .positive(db)
                    .iter()
                    .all(|&pos_ty| self.is_subtype_of(db, pos_ty))
                    && intersection
                        .negative(db)
                        .iter()
                        .all(|&neg_ty| self.is_disjoint_from(db, neg_ty))
            }

            (Type::Intersection(intersection), _) => intersection
                .positive(db)
                .iter()
                .any(|&elem_ty| elem_ty.is_subtype_of(db, target)),

            // Note that the definition of `Type::AlwaysFalsy` depends on the return value of `__bool__`.
            // If `__bool__` always returns True or False, it can be treated as a subtype of `AlwaysTruthy` or `AlwaysFalsy`, respectively.
            (left, Type::AlwaysFalsy) => left.bool(db).is_always_false(),
            (left, Type::AlwaysTruthy) => left.bool(db).is_always_true(),
            // Currently, the only supertype of `AlwaysFalsy` and `AlwaysTruthy` is the universal set (object instance).
            (Type::AlwaysFalsy | Type::AlwaysTruthy, _) => {
                target.is_equivalent_to(db, Type::object(db))
            }

            // All `StringLiteral` types are a subtype of `LiteralString`.
            (Type::StringLiteral(_), Type::LiteralString) => true,

            // Except for the special `LiteralString` case above,
            // most `Literal` types delegate to their instance fallbacks
            // unless `self` is exactly equivalent to `target` (handled above)
            (Type::StringLiteral(_) | Type::LiteralString, _) => {
                KnownClass::Str.to_instance(db).is_subtype_of(db, target)
            }
            (Type::BooleanLiteral(_), _) => {
                KnownClass::Bool.to_instance(db).is_subtype_of(db, target)
            }
            (Type::IntLiteral(_), _) => KnownClass::Int.to_instance(db).is_subtype_of(db, target),
            (Type::BytesLiteral(_), _) => {
                KnownClass::Bytes.to_instance(db).is_subtype_of(db, target)
            }
            (Type::ModuleLiteral(_), _) => KnownClass::ModuleType
                .to_instance(db)
                .is_subtype_of(db, target),
            (Type::SliceLiteral(_), _) => {
                KnownClass::Slice.to_instance(db).is_subtype_of(db, target)
            }

            (Type::FunctionLiteral(self_function_literal), Type::Callable(_)) => {
                self_function_literal
                    .into_callable_type(db)
                    .is_subtype_of(db, target)
            }

            (Type::BoundMethod(self_bound_method), Type::Callable(_)) => self_bound_method
                .into_callable_type(db)
                .is_subtype_of(db, target),

            // A `FunctionLiteral` type is a single-valued type like the other literals handled above,
            // so it also, for now, just delegates to its instance fallback.
            (Type::FunctionLiteral(_), _) => KnownClass::FunctionType
                .to_instance(db)
                .is_subtype_of(db, target),

            // The same reasoning applies for these special callable types:
            (Type::BoundMethod(_), _) => KnownClass::MethodType
                .to_instance(db)
                .is_subtype_of(db, target),
            (Type::MethodWrapper(_), _) => KnownClass::WrapperDescriptorType
                .to_instance(db)
                .is_subtype_of(db, target),
            (Type::WrapperDescriptor(_), _) => KnownClass::WrapperDescriptorType
                .to_instance(db)
                .is_subtype_of(db, target),

            (Type::Callable(self_callable), Type::Callable(other_callable)) => {
                self_callable.is_subtype_of(db, other_callable)
            }

            (Type::Callable(_), _) => {
                // TODO: Implement subtyping between callable types and other types like
                // function literals, bound methods, class literals, `type[]`, etc.)
                false
            }

            // A fully static heterogeneous tuple type `A` is a subtype of a fully static heterogeneous tuple type `B`
            // iff the two tuple types have the same number of elements and each element-type in `A` is a subtype
            // of the element-type at the same index in `B`. (Now say that 5 times fast.)
            //
            // For example: `tuple[bool, bool]` is a subtype of `tuple[int, int]`,
            // but `tuple[bool, bool, bool]` is not a subtype of `tuple[int, int]`
            (Type::Tuple(self_tuple), Type::Tuple(target_tuple)) => {
                let self_elements = self_tuple.elements(db);
                let target_elements = target_tuple.elements(db);
                self_elements.len() == target_elements.len()
                    && self_elements.iter().zip(target_elements).all(
                        |(self_element, target_element)| {
                            self_element.is_subtype_of(db, *target_element)
                        },
                    )
            }

            // Other than the special tuple-to-tuple case handled, above,
            // tuple subtyping delegates to `Instance(tuple)` in the same way as the literal types.
            //
            // All heterogeneous tuple types are subtypes of `Instance(<tuple>)`:
            // `Instance(<some class T>)` expresses "the set of all possible instances of the class `T`";
            // consequently, `Instance(<tuple>)` expresses "the set of all possible instances of the class `tuple`".
            // This type can be spelled in type annotations as `tuple[object, ...]` (since `tuple` is covariant).
            //
            // Note that this is not the same type as the type spelled in type annotations as `tuple`;
            // as that type is equivalent to `type[Any, ...]` (and therefore not a fully static type).
            (Type::Tuple(_), _) => KnownClass::Tuple.to_instance(db).is_subtype_of(db, target),

            // `Literal[<class 'C'>]` is a subtype of `type[B]` if `C` is a subclass of `B`,
            // since `type[B]` describes all possible runtime subclasses of the class object `B`.
            (
                Type::ClassLiteral(ClassLiteralType { class }),
                Type::SubclassOf(target_subclass_ty),
            ) => target_subclass_ty
                .subclass_of()
                .into_class()
                .is_some_and(|target_class| class.is_subclass_of(db, target_class)),

            // This branch asks: given two types `type[T]` and `type[S]`, is `type[T]` a subtype of `type[S]`?
            (Type::SubclassOf(self_subclass_ty), Type::SubclassOf(target_subclass_ty)) => {
                self_subclass_ty.is_subtype_of(db, target_subclass_ty)
            }

            // `Literal[str]` is a subtype of `type` because the `str` class object is an instance of its metaclass `type`.
            // `Literal[abc.ABC]` is a subtype of `abc.ABCMeta` because the `abc.ABC` class object
            // is an instance of its metaclass `abc.ABCMeta`.
            (Type::ClassLiteral(ClassLiteralType { class }), _) => {
                class.metaclass_instance_type(db).is_subtype_of(db, target)
            }

            // `type[str]` (== `SubclassOf("str")` in red-knot) describes all possible runtime subclasses
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
                .is_some_and(|metaclass_instance_type| {
                    metaclass_instance_type.is_subtype_of(db, target)
                }),

            // For example: `Type::KnownInstance(KnownInstanceType::Type)` is a subtype of `Type::Instance(_SpecialForm)`,
            // because `Type::KnownInstance(KnownInstanceType::Type)` is a set with exactly one runtime value in it
            // (the symbol `typing.Type`), and that symbol is known to be an instance of `typing._SpecialForm` at runtime.
            (Type::KnownInstance(left), right) => {
                left.instance_fallback(db).is_subtype_of(db, right)
            }

            // `bool` is a subtype of `int`, because `bool` subclasses `int`,
            // which means that all instances of `bool` are also instances of `int`
            (Type::Instance(self_instance), Type::Instance(target_instance)) => {
                self_instance.is_subtype_of(db, target_instance)
            }

            (Type::Instance(_), Type::Callable(_)) => {
                let call_symbol = self.member(db, "__call__").symbol;
                match call_symbol {
                    Symbol::Type(Type::BoundMethod(call_function), _) => call_function
                        .into_callable_type(db)
                        .is_subtype_of(db, target),
                    _ => false,
                }
            }

            (Type::PropertyInstance(_), _) => KnownClass::Property
                .to_instance(db)
                .is_subtype_of(db, target),
            (_, Type::PropertyInstance(_)) => {
                self.is_subtype_of(db, KnownClass::Property.to_instance(db))
            }

            // Other than the special cases enumerated above, `Instance` types and typevars are
            // never subtypes of any other variants
            (Type::Instance(_) | Type::TypeVar(_), _) => false,
        }
    }

    /// Return true if this type is [assignable to] type `target`.
    ///
    /// [assignable to]: https://typing.readthedocs.io/en/latest/spec/concepts.html#the-assignable-to-or-consistent-subtyping-relation
    pub(crate) fn is_assignable_to(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        if self.is_gradual_equivalent_to(db, target) {
            return true;
        }

        match (self, target) {
            // Never can be assigned to any type.
            (Type::Never, _) => true,

            // The dynamic type is assignable-to and assignable-from any type.
            (Type::Dynamic(_), _) => true,
            (_, Type::Dynamic(_)) => true,

            // All types are assignable to `object`.
            // TODO this special case might be removable once the below cases are comprehensive
            (_, Type::Instance(InstanceType { class })) if class.is_object(db) => true,

            // A typevar is always assignable to itself, and is never assignable to any other
            // typevar, since there is no guarantee that they will be specialized to the same
            // type. (This is true even if both typevars are bounded by the same final class, since
            // you can specialize the typevars to `Never` in addition to that final class.)
            (Type::TypeVar(self_typevar), Type::TypeVar(other_typevar)) => {
                self_typevar == other_typevar
            }

            // A typevar is assignable to its upper bound, and to something similar to the union of
            // its constraints. An unbound, unconstrained typevar has an implicit upper bound of
            // `object` (which is handled above).
            (Type::TypeVar(typevar), _) if typevar.bound_or_constraints(db).is_some() => {
                match typevar.bound_or_constraints(db) {
                    None => unreachable!(),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.is_assignable_to(db, target)
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .elements(db)
                        .iter()
                        .all(|constraint| constraint.is_assignable_to(db, target)),
                }
            }

            // A union is assignable to a type T iff every element of the union is assignable to T.
            (Type::Union(union), ty) => union
                .elements(db)
                .iter()
                .all(|&elem_ty| elem_ty.is_assignable_to(db, ty)),

            // A type T is assignable to a union iff T is assignable to any element of the union.
            (ty, Type::Union(union)) => union
                .elements(db)
                .iter()
                .any(|&elem_ty| ty.is_assignable_to(db, elem_ty)),

            (_, Type::TypeVar(typevar)) => match typevar.bound_or_constraints(db) {
                // No types are assignable to a bounded typevar, or to an unbounded unconstrained
                // typevar, since there's no guarantee what type the typevar will be specialized
                // to. If the typevar is bounded, it might be specialized to a smaller type than
                // the bound. (This is true even if the bound is a final class, since the typevar
                // can still be specialized to `Never`.)
                None => false,
                Some(TypeVarBoundOrConstraints::UpperBound(_)) => false,
                // If the typevar is constrained, there must be multiple constraints, and the
                // typevar might be specialized to any one of them. However, the constraints do not
                // have to be disjoint, which means an lhs type might be assignable to all of the
                // constraints.
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                    .elements(db)
                    .iter()
                    .all(|constraint| self.is_assignable_to(db, *constraint)),
            },

            // If both sides are intersections we need to handle the right side first
            // (A & B & C) is assignable to (A & B) because the left is assignable to both A and B,
            // but none of A, B, or C is assignable to (A & B).
            //
            // A type S is assignable to an intersection type T if
            // S is assignable to all positive elements of T (e.g. `str & int` is assignable to `str & Any`), and
            // S is disjoint from all negative elements of T (e.g. `int` is not assignable to Intersection[int, Not[Literal[1]]]).
            (ty, Type::Intersection(intersection)) => {
                intersection
                    .positive(db)
                    .iter()
                    .all(|&elem_ty| ty.is_assignable_to(db, elem_ty))
                    && intersection
                        .negative(db)
                        .iter()
                        .all(|&neg_ty| ty.is_disjoint_from(db, neg_ty))
            }

            // An intersection type S is assignable to a type T if
            // Any element of S is assignable to T (e.g. `A & B` is assignable to `A`)
            // Negative elements do not have an effect on assignability - if S is assignable to T then S & ~P is also assignable to T.
            (Type::Intersection(intersection), ty) => intersection
                .positive(db)
                .iter()
                .any(|&elem_ty| elem_ty.is_assignable_to(db, ty)),

            // A tuple type S is assignable to a tuple type T if their lengths are the same, and
            // each element of S is assignable to the corresponding element of T.
            (Type::Tuple(self_tuple), Type::Tuple(target_tuple)) => {
                let self_elements = self_tuple.elements(db);
                let target_elements = target_tuple.elements(db);
                self_elements.len() == target_elements.len()
                    && self_elements.iter().zip(target_elements).all(
                        |(self_element, target_element)| {
                            self_element.is_assignable_to(db, *target_element)
                        },
                    )
            }

            // This special case is required because the left-hand side tuple might be a
            // gradual type, so we can not rely on subtyping. This allows us to assign e.g.
            // `tuple[Any, int]` to `tuple`.
            (Type::Tuple(_), _)
                if KnownClass::Tuple
                    .to_instance(db)
                    .is_assignable_to(db, target) =>
            {
                true
            }

            // `type[Any]` is assignable to any `type[...]` type, because `type[Any]` can
            // materialize to any `type[...]` type.
            (Type::SubclassOf(subclass_of_ty), Type::SubclassOf(_))
                if subclass_of_ty.is_dynamic() =>
            {
                true
            }

            // All `type[...]` types are assignable to `type[Any]`, because `type[Any]` can
            // materialize to any `type[...]` type.
            //
            // Every class literal type is also assignable to `type[Any]`, because the class
            // literal type for a class `C` is a subtype of `type[C]`, and `type[C]` is assignable
            // to `type[Any]`.
            (Type::ClassLiteral(_) | Type::SubclassOf(_), Type::SubclassOf(target_subclass_of))
                if target_subclass_of.is_dynamic() =>
            {
                true
            }

            // `type[Any]` is assignable to any type that `type[object]` is assignable to, because
            // `type[Any]` can materialize to `type[object]`.
            //
            // `type[Any]` is also assignable to any subtype of `type[object]`, because all
            // subtypes of `type[object]` are `type[...]` types (or `Never`), and `type[Any]` can
            // materialize to any `type[...]` type (or to `type[Never]`, which is equivalent to
            // `Never`.)
            (Type::SubclassOf(subclass_of_ty), Type::Instance(_))
                if subclass_of_ty.is_dynamic()
                    && (KnownClass::Type
                        .to_instance(db)
                        .is_assignable_to(db, target)
                        || target.is_subtype_of(db, KnownClass::Type.to_instance(db))) =>
            {
                true
            }

            // Any type that is assignable to `type[object]` is also assignable to `type[Any]`,
            // because `type[Any]` can materialize to `type[object]`.
            (Type::Instance(_), Type::SubclassOf(subclass_of_ty))
                if subclass_of_ty.is_dynamic()
                    && self.is_assignable_to(db, KnownClass::Type.to_instance(db)) =>
            {
                true
            }

            // TODO: This is a workaround to avoid false positives (e.g. when checking function calls
            // with `SupportsIndex` parameters), which should be removed when we understand protocols.
            (lhs, Type::Instance(InstanceType { class }))
                if class.is_known(db, KnownClass::SupportsIndex) =>
            {
                match lhs {
                    Type::Instance(InstanceType { class })
                        if matches!(
                            class.known(db),
                            Some(KnownClass::Int | KnownClass::SupportsIndex)
                        ) =>
                    {
                        true
                    }
                    Type::IntLiteral(_) => true,
                    _ => false,
                }
            }

            // TODO: ditto for avoiding false positives when checking function calls with `Sized` parameters.
            (lhs, Type::Instance(InstanceType { class }))
                if class.is_known(db, KnownClass::Sized) =>
            {
                matches!(
                    lhs.to_meta_type(db).member(db, "__len__"),
                    SymbolAndQualifiers {
                        symbol: Symbol::Type(..),
                        ..
                    }
                )
            }

            (Type::Callable(self_callable), Type::Callable(target_callable)) => {
                self_callable.is_assignable_to(db, target_callable)
            }

            (Type::FunctionLiteral(self_function_literal), Type::Callable(_)) => {
                self_function_literal
                    .into_callable_type(db)
                    .is_assignable_to(db, target)
            }

            // TODO other types containing gradual forms (e.g. generics containing Any/Unknown)
            _ => self.is_subtype_of(db, target),
        }
    }

    /// Return true if this type is [equivalent to] type `other`.
    ///
    /// This method returns `false` if either `self` or `other` is not fully static.
    ///
    /// [equivalent to]: https://typing.readthedocs.io/en/latest/spec/glossary.html#term-equivalent
    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        // TODO equivalent but not identical types: TypedDicts, Protocols, type aliases, etc.

        match (self, other) {
            (Type::Union(left), Type::Union(right)) => left.is_equivalent_to(db, right),
            (Type::Intersection(left), Type::Intersection(right)) => {
                left.is_equivalent_to(db, right)
            }
            (Type::Tuple(left), Type::Tuple(right)) => left.is_equivalent_to(db, right),
            (Type::Callable(left), Type::Callable(right)) => left.is_equivalent_to(db, right),
            _ => self == other && self.is_fully_static(db) && other.is_fully_static(db),
        }
    }

    /// Returns true if both `self` and `other` are the same gradual form
    /// (limited to `Any`, `Unknown`, or `Todo`).
    pub(crate) fn is_same_gradual_form(self, other: Type<'db>) -> bool {
        matches!(
            (self, other),
            (
                Type::Dynamic(DynamicType::Any),
                Type::Dynamic(DynamicType::Any)
            ) | (
                Type::Dynamic(DynamicType::Unknown),
                Type::Dynamic(DynamicType::Unknown)
            ) | (
                Type::Dynamic(DynamicType::Todo(_)),
                Type::Dynamic(DynamicType::Todo(_))
            )
        )
    }

    /// Returns true if this type and `other` are gradual equivalent.
    ///
    /// > Two gradual types `A` and `B` are equivalent
    /// > (that is, the same gradual type, not merely consistent with one another)
    /// > if and only if all materializations of `A` are also materializations of `B`,
    /// > and all materializations of `B` are also materializations of `A`.
    /// >
    /// > &mdash; [Summary of type relations]
    ///
    /// This powers the `assert_type()` directive.
    ///
    /// [Summary of type relations]: https://typing.readthedocs.io/en/latest/spec/concepts.html#summary-of-type-relations
    pub(crate) fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        if self == other {
            return true;
        }

        match (self, other) {
            (Type::Dynamic(_), Type::Dynamic(_)) => true,

            (Type::SubclassOf(first), Type::SubclassOf(second)) => {
                match (first.subclass_of(), second.subclass_of()) {
                    (first, second) if first == second => true,
                    (ClassBase::Dynamic(_), ClassBase::Dynamic(_)) => true,
                    _ => false,
                }
            }

            (Type::TypeVar(first), Type::TypeVar(second)) => first == second,

            (Type::Tuple(first), Type::Tuple(second)) => first.is_gradual_equivalent_to(db, second),

            (Type::Union(first), Type::Union(second)) => first.is_gradual_equivalent_to(db, second),

            (Type::Intersection(first), Type::Intersection(second)) => {
                first.is_gradual_equivalent_to(db, second)
            }

            (Type::Callable(first), Type::Callable(second)) => {
                first.is_gradual_equivalent_to(db, second)
            }

            _ => false,
        }
    }

    /// Return true if this type and `other` have no common elements.
    ///
    /// Note: This function aims to have no false positives, but might return
    /// wrong `false` answers in some cases.
    pub(crate) fn is_disjoint_from(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        match (self, other) {
            (Type::Never, _) | (_, Type::Never) => true,

            (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => false,

            // A typevar is never disjoint from itself, since all occurrences of the typevar must
            // be specialized to the same type. (This is an important difference between typevars
            // and `Any`!) Different typevars might be disjoint, depending on their bounds and
            // constraints, which are handled below.
            (Type::TypeVar(self_typevar), Type::TypeVar(other_typevar))
                if self_typevar == other_typevar =>
            {
                false
            }

            // An unbounded typevar is never disjoint from any other type, since it might be
            // specialized to any type. A bounded typevar is not disjoint from its bound, and is
            // only disjoint from other types if its bound is. A constrained typevar is disjoint
            // from a type if all of its constraints are.
            (Type::TypeVar(typevar), other) | (other, Type::TypeVar(typevar)) => {
                match typevar.bound_or_constraints(db) {
                    None => false,
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.is_disjoint_from(db, other)
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .elements(db)
                        .iter()
                        .all(|constraint| constraint.is_disjoint_from(db, other)),
                }
            }

            (Type::Union(union), other) | (other, Type::Union(union)) => union
                .elements(db)
                .iter()
                .all(|e| e.is_disjoint_from(db, other)),

            // If we have two intersections, we test the positive elements of each one against the other intersection
            // Negative elements need a positive element on the other side in order to be disjoint.
            // This is similar to what would happen if we tried to build a new intersection that combines the two
            (Type::Intersection(self_intersection), Type::Intersection(other_intersection)) => {
                self_intersection
                    .positive(db)
                    .iter()
                    .any(|p| p.is_disjoint_from(db, other))
                    || other_intersection
                        .positive(db)
                        .iter()
                        .any(|p: &Type<'_>| p.is_disjoint_from(db, self))
            }

            (Type::Intersection(intersection), other)
            | (other, Type::Intersection(intersection)) => {
                intersection
                    .positive(db)
                    .iter()
                    .any(|p| p.is_disjoint_from(db, other))
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
                | Type::SliceLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::MethodWrapper(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)
                | Type::KnownInstance(..)),
                right @ (Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::SliceLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::MethodWrapper(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)
                | Type::KnownInstance(..)),
            ) => left != right,

            // One tuple type can be a subtype of another tuple type,
            // but we know for sure that any given tuple type is disjoint from all single-valued types
            (
                Type::Tuple(..),
                Type::ClassLiteral(..)
                | Type::ModuleLiteral(..)
                | Type::BooleanLiteral(..)
                | Type::BytesLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::MethodWrapper(..)
                | Type::WrapperDescriptor(..)
                | Type::IntLiteral(..)
                | Type::SliceLiteral(..)
                | Type::StringLiteral(..)
                | Type::LiteralString,
            )
            | (
                Type::ClassLiteral(..)
                | Type::ModuleLiteral(..)
                | Type::BooleanLiteral(..)
                | Type::BytesLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::BoundMethod(..)
                | Type::MethodWrapper(..)
                | Type::WrapperDescriptor(..)
                | Type::IntLiteral(..)
                | Type::SliceLiteral(..)
                | Type::StringLiteral(..)
                | Type::LiteralString,
                Type::Tuple(..),
            ) => true,

            (
                Type::SubclassOf(subclass_of_ty),
                Type::ClassLiteral(ClassLiteralType { class: class_b }),
            )
            | (
                Type::ClassLiteral(ClassLiteralType { class: class_b }),
                Type::SubclassOf(subclass_of_ty),
            ) => match subclass_of_ty.subclass_of() {
                ClassBase::Dynamic(_) => false,
                ClassBase::Class(class_a) => !class_b.is_subclass_of(db, class_a),
            },

            (
                Type::SubclassOf(_),
                Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::LiteralString
                | Type::BytesLiteral(..)
                | Type::SliceLiteral(..)
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
                | Type::SliceLiteral(..)
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

            // for `type[Any]`/`type[Unknown]`/`type[Todo]`, we know the type cannot be any larger than `type`,
            // so although the type is dynamic we can still determine disjointedness in some situations
            (Type::SubclassOf(subclass_of_ty), other)
            | (other, Type::SubclassOf(subclass_of_ty)) => match subclass_of_ty.subclass_of() {
                ClassBase::Dynamic(_) => {
                    KnownClass::Type.to_instance(db).is_disjoint_from(db, other)
                }
                ClassBase::Class(class) => class
                    .metaclass_instance_type(db)
                    .is_disjoint_from(db, other),
            },

            (Type::KnownInstance(known_instance), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::KnownInstance(known_instance)) => {
                !known_instance.is_instance_of(db, class)
            }

            (known_instance_ty @ Type::KnownInstance(_), Type::Tuple(_))
            | (Type::Tuple(_), known_instance_ty @ Type::KnownInstance(_)) => {
                known_instance_ty.is_disjoint_from(db, KnownClass::Tuple.to_instance(db))
            }

            (Type::BooleanLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::BooleanLiteral(..)) => {
                // A `Type::BooleanLiteral()` must be an instance of exactly `bool`
                // (it cannot be an instance of a `bool` subclass)
                !KnownClass::Bool.is_subclass_of(db, class)
            }

            (Type::BooleanLiteral(..), _) | (_, Type::BooleanLiteral(..)) => true,

            (Type::IntLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::IntLiteral(..)) => {
                // A `Type::IntLiteral()` must be an instance of exactly `int`
                // (it cannot be an instance of an `int` subclass)
                !KnownClass::Int.is_subclass_of(db, class)
            }

            (Type::IntLiteral(..), _) | (_, Type::IntLiteral(..)) => true,

            (Type::StringLiteral(..), Type::LiteralString)
            | (Type::LiteralString, Type::StringLiteral(..)) => false,

            (
                Type::StringLiteral(..) | Type::LiteralString,
                Type::Instance(InstanceType { class }),
            )
            | (
                Type::Instance(InstanceType { class }),
                Type::StringLiteral(..) | Type::LiteralString,
            ) => {
                // A `Type::StringLiteral()` or a `Type::LiteralString` must be an instance of exactly `str`
                // (it cannot be an instance of a `str` subclass)
                !KnownClass::Str.is_subclass_of(db, class)
            }

            (Type::LiteralString, Type::LiteralString) => false,
            (Type::LiteralString, _) | (_, Type::LiteralString) => true,

            (Type::BytesLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::BytesLiteral(..)) => {
                // A `Type::BytesLiteral()` must be an instance of exactly `bytes`
                // (it cannot be an instance of a `bytes` subclass)
                !KnownClass::Bytes.is_subclass_of(db, class)
            }

            (Type::SliceLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::SliceLiteral(..)) => {
                // A `Type::SliceLiteral` must be an instance of exactly `slice`
                // (it cannot be an instance of a `slice` subclass)
                !KnownClass::Slice.is_subclass_of(db, class)
            }

            // A class-literal type `X` is always disjoint from an instance type `Y`,
            // unless the type expressing "all instances of `Z`" is a subtype of of `Y`,
            // where `Z` is `X`'s metaclass.
            (Type::ClassLiteral(ClassLiteralType { class }), instance @ Type::Instance(_))
            | (instance @ Type::Instance(_), Type::ClassLiteral(ClassLiteralType { class })) => {
                !class
                    .metaclass_instance_type(db)
                    .is_subtype_of(db, instance)
            }

            (Type::FunctionLiteral(..), Type::Instance(InstanceType { class }))
            | (Type::Instance(InstanceType { class }), Type::FunctionLiteral(..)) => {
                // A `Type::FunctionLiteral()` must be an instance of exactly `types.FunctionType`
                // (it cannot be an instance of a `types.FunctionType` subclass)
                !KnownClass::FunctionType.is_subclass_of(db, class)
            }

            (Type::BoundMethod(_), other) | (other, Type::BoundMethod(_)) => KnownClass::MethodType
                .to_instance(db)
                .is_disjoint_from(db, other),

            (Type::MethodWrapper(_), other) | (other, Type::MethodWrapper(_)) => {
                KnownClass::MethodWrapperType
                    .to_instance(db)
                    .is_disjoint_from(db, other)
            }

            (Type::WrapperDescriptor(_), other) | (other, Type::WrapperDescriptor(_)) => {
                KnownClass::WrapperDescriptorType
                    .to_instance(db)
                    .is_disjoint_from(db, other)
            }

            (Type::Callable(_) | Type::FunctionLiteral(_), Type::Callable(_))
            | (Type::Callable(_), Type::FunctionLiteral(_)) => {
                // No two callable types are ever disjoint because
                // `(*args: object, **kwargs: object) -> Never` is a subtype of all fully static
                // callable types.
                false
            }

            (
                Type::Callable(_),
                Type::StringLiteral(_) | Type::BytesLiteral(_) | Type::SliceLiteral(_),
            )
            | (
                Type::StringLiteral(_) | Type::BytesLiteral(_) | Type::SliceLiteral(_),
                Type::Callable(_),
            ) => {
                // A callable type is disjoint from other literal types. For example,
                // `Type::StringLiteral` must be an instance of exactly `str`, not a subclass
                // of `str`, and `str` is not callable. The same applies to other literal types.
                true
            }

            (Type::Callable(_), _) | (_, Type::Callable(_)) => {
                // TODO: Implement disjointness for general callable type with other types
                false
            }

            (Type::ModuleLiteral(..), other @ Type::Instance(..))
            | (other @ Type::Instance(..), Type::ModuleLiteral(..)) => {
                // Modules *can* actually be instances of `ModuleType` subclasses
                other.is_disjoint_from(db, KnownClass::ModuleType.to_instance(db))
            }

            (
                Type::Instance(InstanceType { class: left_class }),
                Type::Instance(InstanceType { class: right_class }),
            ) => {
                (left_class.is_final(db) && !left_class.is_subclass_of(db, right_class))
                    || (right_class.is_final(db) && !right_class.is_subclass_of(db, left_class))
            }

            (Type::Tuple(tuple), Type::Tuple(other_tuple)) => {
                let self_elements = tuple.elements(db);
                let other_elements = other_tuple.elements(db);
                self_elements.len() != other_elements.len()
                    || self_elements
                        .iter()
                        .zip(other_elements)
                        .any(|(e1, e2)| e1.is_disjoint_from(db, *e2))
            }

            (Type::Tuple(..), instance @ Type::Instance(_))
            | (instance @ Type::Instance(_), Type::Tuple(..)) => {
                // We cannot be sure if the tuple is disjoint from the instance because:
                //   - 'other' might be the homogeneous arbitrary-length tuple type
                //     tuple[T, ...] (which we don't have support for yet); if all of
                //     our element types are not disjoint with T, this is not disjoint
                //   - 'other' might be a user subtype of tuple, which, if generic
                //     over the same or compatible *Ts, would overlap with tuple.
                //
                // TODO: add checks for the above cases once we support them
                instance.is_disjoint_from(db, KnownClass::Tuple.to_instance(db))
            }

            (Type::PropertyInstance(_), _) | (_, Type::PropertyInstance(_)) => KnownClass::Property
                .to_instance(db)
                .is_disjoint_from(db, other),
        }
    }

    /// Returns true if the type does not contain any gradual forms (as a sub-part).
    pub(crate) fn is_fully_static(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::Dynamic(_) => false,
            Type::Never
            | Type::FunctionLiteral(..)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(_)
            | Type::ModuleLiteral(..)
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::BytesLiteral(_)
            | Type::SliceLiteral(_)
            | Type::KnownInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::PropertyInstance(_) => true,

            Type::TypeVar(typevar) => match typevar.bound_or_constraints(db) {
                None => true,
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound.is_fully_static(db),
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                    .elements(db)
                    .iter()
                    .all(|constraint| constraint.is_fully_static(db)),
            },

            Type::SubclassOf(subclass_of_ty) => subclass_of_ty.is_fully_static(),
            Type::ClassLiteral(_) | Type::Instance(_) => {
                // TODO: Ideally, we would iterate over the MRO of the class, check if all
                // bases are fully static, and only return `true` if that is the case.
                //
                // This does not work yet, because we currently infer `Unknown` for some
                // generic base classes that we don't understand yet. For example, `str`
                // is defined as `class str(Sequence[str])` in typeshed and we currently
                // compute its MRO as `(str, Unknown, object)`. This would make us think
                // that `str` is a gradual type, which causes all sorts of downstream
                // issues because it does not participate in equivalence/subtyping etc.
                //
                // Another problem is that we run into problems if we eagerly query the
                // MRO of class literals here. I have not fully investigated this, but
                // iterating over the MRO alone, without even acting on it, causes us to
                // infer `Unknown` for many classes.

                true
            }
            Type::Union(union) => union.is_fully_static(db),
            Type::Intersection(intersection) => intersection.is_fully_static(db),
            // TODO: Once we support them, make sure that we return `false` for other types
            // containing gradual forms such as `tuple[Any, ...]`.
            // Conversely, make sure to return `true` for homogeneous tuples such as
            // `tuple[int, ...]`, once we add support for them.
            Type::Tuple(tuple) => tuple
                .elements(db)
                .iter()
                .all(|elem| elem.is_fully_static(db)),
            Type::Callable(callable) => callable.is_fully_static(db),
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
            | Type::SliceLiteral(..)
            | Type::LiteralString => {
                // Note: The literal types included in this pattern are not true singletons.
                // There can be multiple Python objects (at different memory locations) that
                // are both of type Literal[345], for example.
                false
            }

            // An unbounded, unconstrained typevar is not a singleton, because it can be
            // specialized to a non-singleton type. A bounded typevar is not a singleton, even if
            // the bound is a final singleton class, since it can still be specialized to `Never`.
            // A constrained typevar is a singleton if all of its constraints are singletons. (Note
            // that you cannot specialize a constrained typevar to a subtype of a constraint.)
            Type::TypeVar(typevar) => match typevar.bound_or_constraints(db) {
                None => false,
                Some(TypeVarBoundOrConstraints::UpperBound(_)) => false,
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                    .elements(db)
                    .iter()
                    .all(|constraint| constraint.is_singleton(db)),
            },

            // We eagerly transform `SubclassOf` to `ClassLiteral` for final types, so `SubclassOf` is never a singleton.
            Type::SubclassOf(..) => false,
            Type::BooleanLiteral(_)
            | Type::FunctionLiteral(..)
            | Type::WrapperDescriptor(..)
            | Type::ClassLiteral(..)
            | Type::ModuleLiteral(..)
            | Type::KnownInstance(..) => true,
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
            Type::Instance(InstanceType { class }) => {
                class.known(db).is_some_and(KnownClass::is_singleton)
            }
            Type::PropertyInstance(_) => false,
            Type::Tuple(..) => {
                // The empty tuple is a singleton on CPython and PyPy, but not on other Python
                // implementations such as GraalPy. Its *use* as a singleton is discouraged and
                // should not be relied on for type narrowing, so we do not treat it as one.
                // See:
                // https://docs.python.org/3/reference/expressions.html#parenthesized-forms
                false
            }
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
            | Type::IntLiteral(..)
            | Type::BooleanLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::SliceLiteral(..)
            | Type::KnownInstance(..) => true,

            // An unbounded, unconstrained typevar is not single-valued, because it can be
            // specialized to a multiple-valued type. A bounded typevar is not single-valued, even
            // if the bound is a final single-valued class, since it can still be specialized to
            // `Never`. A constrained typevar is single-valued if all of its constraints are
            // single-valued. (Note that you cannot specialize a constrained typevar to a subtype
            // of a constraint.)
            Type::TypeVar(typevar) => match typevar.bound_or_constraints(db) {
                None => false,
                Some(TypeVarBoundOrConstraints::UpperBound(_)) => false,
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                    .elements(db)
                    .iter()
                    .all(|constraint| constraint.is_single_valued(db)),
            },

            Type::SubclassOf(..) => {
                // TODO: Same comment as above for `is_singleton`
                false
            }

            Type::Tuple(tuple) => tuple
                .elements(db)
                .iter()
                .all(|elem| elem.is_single_valued(db)),

            Type::Instance(InstanceType { class }) => {
                class.known(db).is_some_and(KnownClass::is_single_valued)
            }

            Type::Dynamic(_)
            | Type::Never
            | Type::Union(..)
            | Type::Intersection(..)
            | Type::LiteralString
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::Callable(_)
            | Type::PropertyInstance(_) => false,
        }
    }

    /// This function is roughly equivalent to `find_name_in_mro` as defined in the [descriptor guide] or
    /// [`_PyType_Lookup`] in CPython's `Objects/typeobject.c`. It should typically be called through
    /// [Type::class_member], unless it is known that `self` is a class-like type. This function returns
    /// `None` if called on an instance-like type.
    ///
    /// [descriptor guide]: https://docs.python.org/3/howto/descriptor.html#invocation-from-an-instance
    /// [`_PyType_Lookup`]: https://github.com/python/cpython/blob/e285232c76606e3be7bf216efb1be1e742423e4b/Objects/typeobject.c#L5223
    fn find_name_in_mro(
        &self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Option<SymbolAndQualifiers<'db>> {
        match self {
            Type::Union(union) => Some(union.map_with_boundness_and_qualifiers(db, |elem| {
                elem.find_name_in_mro(db, name, policy)
                    // If some elements are classes, and some are not, we simply fall back to `Unbound` for the non-class
                    // elements instead of short-circuiting the whole result to `None`. We would need a more detailed
                    // return type otherwise, and since `find_name_in_mro` is usually called via `class_member`, this is
                    // not a problem.
                    .unwrap_or_default()
            })),
            Type::Intersection(inter) => {
                Some(inter.map_with_boundness_and_qualifiers(db, |elem| {
                    elem.find_name_in_mro(db, name, policy)
                        // Fall back to Unbound, similar to the union case (see above).
                        .unwrap_or_default()
                }))
            }

            Type::Dynamic(_) | Type::Never => Some(Symbol::bound(self).into()),

            Type::ClassLiteral(class_literal @ ClassLiteralType { class }) => {
                match (class.known(db), name) {
                    (Some(KnownClass::FunctionType), "__get__") => Some(
                        Symbol::bound(Type::WrapperDescriptor(
                            WrapperDescriptorKind::FunctionTypeDunderGet,
                        ))
                        .into(),
                    ),
                    (Some(KnownClass::FunctionType), "__set__" | "__delete__") => {
                        // Hard code this knowledge, as we look up `__set__` and `__delete__` on `FunctionType` often.
                        Some(Symbol::Unbound.into())
                    }
                    (Some(KnownClass::Property), "__get__") => Some(
                        Symbol::bound(Type::WrapperDescriptor(
                            WrapperDescriptorKind::PropertyDunderGet,
                        ))
                        .into(),
                    ),
                    (Some(KnownClass::Property), "__set__") => Some(
                        Symbol::bound(Type::WrapperDescriptor(
                            WrapperDescriptorKind::PropertyDunderSet,
                        ))
                        .into(),
                    ),
                    // TODO:
                    // We currently hard-code the knowledge that the following known classes are not
                    // descriptors, i.e. that they have no `__get__` method. This is not wrong and
                    // potentially even beneficial for performance, but it's not very principled.
                    // This case can probably be removed eventually, but we include it at the moment
                    // because we make extensive use of these types in our test suite. Note that some
                    // builtin types are not included here, since they do not have generic bases and
                    // are correctly handled by the `find_name_in_mro` method.
                    (
                        Some(
                            KnownClass::Int
                            | KnownClass::Str
                            | KnownClass::Bytes
                            | KnownClass::Tuple
                            | KnownClass::Slice
                            | KnownClass::Range,
                        ),
                        "__get__" | "__set__" | "__delete__",
                    ) => Some(Symbol::Unbound.into()),

                    _ => Some(class_literal.class_member(db, name, policy)),
                }
            }

            Type::SubclassOf(subclass_of)
                if name == "__get__"
                    && matches!(
                        subclass_of
                            .subclass_of()
                            .into_class()
                            .and_then(|c| c.known(db)),
                        Some(
                            KnownClass::Int
                                | KnownClass::Str
                                | KnownClass::Bytes
                                | KnownClass::Tuple
                                | KnownClass::Slice
                                | KnownClass::Range,
                        )
                    ) =>
            {
                Some(Symbol::Unbound.into())
            }
            Type::SubclassOf(subclass_of_ty) => subclass_of_ty.find_name_in_mro(db, name, policy),

            // We eagerly normalize type[object], i.e. Type::SubclassOf(object) to `type`, i.e. Type::Instance(type).
            // So looking up a name in the MRO of `Type::Instance(type)` is equivalent to looking up the name in the
            // MRO of the class `object`.
            Type::Instance(InstanceType { class }) if class.is_known(db, KnownClass::Type) => {
                KnownClass::Object
                    .to_class_literal(db)
                    .find_name_in_mro(db, name, policy)
            }

            Type::FunctionLiteral(_)
            | Type::Callable(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(_)
            | Type::ModuleLiteral(_)
            | Type::KnownInstance(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::BytesLiteral(_)
            | Type::SliceLiteral(_)
            | Type::Tuple(_)
            | Type::TypeVar(_)
            | Type::Instance(_)
            | Type::PropertyInstance(_) => None,
        }
    }

    /// Look up an attribute in the MRO of the meta-type of `self`. This returns class-level attributes
    /// when called on an instance-like type, and metaclass attributes when called on a class-like type.
    ///
    /// Basically corresponds to `self.to_meta_type().find_name_in_mro(name, policy)`, except for the handling
    /// of union and intersection types.
    #[salsa::tracked]
    fn class_member(
        self,
        db: &'db dyn Db,
        name: Name,
        policy: MemberLookupPolicy,
    ) -> SymbolAndQualifiers<'db> {
        tracing::trace!("class_member: {}.{}", self.display(db), name);
        match self {
            Type::Union(union) => union.map_with_boundness_and_qualifiers(db, |elem| {
                elem.class_member(db, name.clone(), policy)
            }),
            Type::Intersection(inter) => inter.map_with_boundness_and_qualifiers(db, |elem| {
                elem.class_member(db, name.clone(), policy)
            }),
            _ => self
                .to_meta_type(db)
                .find_name_in_mro(db, name.as_str(), policy)
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
    fn instance_member(&self, db: &'db dyn Db, name: &str) -> SymbolAndQualifiers<'db> {
        match self {
            Type::Union(union) => {
                union.map_with_boundness_and_qualifiers(db, |elem| elem.instance_member(db, name))
            }

            Type::Intersection(intersection) => intersection
                .map_with_boundness_and_qualifiers(db, |elem| elem.instance_member(db, name)),

            Type::Dynamic(_) | Type::Never => Symbol::bound(self).into(),

            Type::Instance(InstanceType { class }) => class.instance_member(db, name),

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
            Type::Callable(_) => KnownClass::Object.to_instance(db).instance_member(db, name),

            Type::TypeVar(typevar) => match typevar.bound_or_constraints(db) {
                None => KnownClass::Object.to_instance(db).instance_member(db, name),
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    bound.instance_member(db, name)
                }
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                    .map_with_boundness_and_qualifiers(db, |constraint| {
                        constraint.instance_member(db, name)
                    }),
            },

            Type::IntLiteral(_) => KnownClass::Int.to_instance(db).instance_member(db, name),
            Type::BooleanLiteral(_) => KnownClass::Bool.to_instance(db).instance_member(db, name),
            Type::StringLiteral(_) | Type::LiteralString => {
                KnownClass::Str.to_instance(db).instance_member(db, name)
            }
            Type::BytesLiteral(_) => KnownClass::Bytes.to_instance(db).instance_member(db, name),
            Type::SliceLiteral(_) => KnownClass::Slice.to_instance(db).instance_member(db, name),
            Type::Tuple(_) => KnownClass::Tuple.to_instance(db).instance_member(db, name),

            Type::AlwaysTruthy | Type::AlwaysFalsy => Type::object(db).instance_member(db, name),
            Type::ModuleLiteral(_) => KnownClass::ModuleType
                .to_instance(db)
                .instance_member(db, name),

            Type::KnownInstance(_) => Symbol::Unbound.into(),

            Type::PropertyInstance(_) => KnownClass::Property
                .to_instance(db)
                .instance_member(db, name),

            // TODO: we currently don't model the fact that class literals and subclass-of types have
            // a `__dict__` that is filled with class level attributes. Modeling this is currently not
            // required, as `instance_member` is only called for instance-like types through `member`,
            // but we might want to add this in the future.
            Type::ClassLiteral(_) | Type::SubclassOf(_) => Symbol::Unbound.into(),
        }
    }

    /// Access an attribute of this type without invoking the descriptor protocol. This
    /// method corresponds to `inspect.getattr_static(<object of type 'self'>, name)`.
    ///
    /// See also: [`Type::member`]
    fn static_member(&self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        if let Type::ModuleLiteral(module) = self {
            module.static_member(db, name)
        } else if let symbol @ Symbol::Type(_, _) = self
            .class_member(db, name.into(), MemberLookupPolicy::default())
            .symbol
        {
            symbol
        } else if let Some(symbol @ Symbol::Type(_, _)) = self
            .find_name_in_mro(db, name, MemberLookupPolicy::default())
            .map(|inner| inner.symbol)
        {
            symbol
        } else {
            self.instance_member(db, name).symbol
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
    #[salsa::tracked]
    fn try_call_dunder_get(
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
        let descr_get = self
            .class_member(db, "__get__".into(), MemberLookupPolicy::default())
            .symbol;

        if let Symbol::Type(descr_get, descr_get_boundness) = descr_get {
            let return_ty = descr_get
                .try_call(db, CallArgumentTypes::positional([self, instance, owner]))
                .map(|bindings| {
                    if descr_get_boundness == Boundness::Bound {
                        bindings.return_type(db)
                    } else {
                        UnionType::from_elements(db, [bindings.return_type(db), self])
                    }
                })
                .ok()?;

            let descriptor_kind = if self
                .class_member(db, "__set__".into(), MemberLookupPolicy::default())
                .symbol
                .is_unbound()
                && self
                    .class_member(db, "__delete__".into(), MemberLookupPolicy::default())
                    .symbol
                    .is_unbound()
            {
                AttributeKind::NormalOrNonDataDescriptor
            } else {
                AttributeKind::DataDescriptor
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
        attribute: SymbolAndQualifiers<'db>,
        instance: Type<'db>,
        owner: Type<'db>,
    ) -> (SymbolAndQualifiers<'db>, AttributeKind) {
        match attribute {
            // This branch is not strictly needed, but it short-circuits the lookup of various dunder
            // methods and calls that would otherwise be made.
            //
            // Note that attribute accesses on dynamic types always succeed. For this reason, they also
            // have `__get__`, `__set__`, and `__delete__` methods and are therefore considered to be
            // data descriptors.
            //
            // The same is true for `Never`.
            SymbolAndQualifiers {
                symbol: Symbol::Type(Type::Dynamic(_) | Type::Never, _),
                qualifiers: _,
            } => (attribute, AttributeKind::DataDescriptor),

            SymbolAndQualifiers {
                symbol: Symbol::Type(Type::Union(union), boundness),
                qualifiers,
            } => (
                union
                    .map_with_boundness(db, |elem| {
                        Symbol::Type(
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

            SymbolAndQualifiers {
                symbol: Symbol::Type(Type::Intersection(intersection), boundness),
                qualifiers,
            } => (
                intersection
                    .map_with_boundness(db, |elem| {
                        Symbol::Type(
                            elem.try_call_dunder_get(db, instance, owner)
                                .map_or(*elem, |(ty, _)| ty),
                            boundness,
                        )
                    })
                    .with_qualifiers(qualifiers),
                // TODO: Discover data descriptors in intersections.
                AttributeKind::NormalOrNonDataDescriptor,
            ),

            SymbolAndQualifiers {
                symbol: Symbol::Type(attribute_ty, boundness),
                qualifiers: _,
            } => {
                if let Some((return_ty, attribute_kind)) =
                    attribute_ty.try_call_dunder_get(db, instance, owner)
                {
                    (Symbol::Type(return_ty, boundness).into(), attribute_kind)
                } else {
                    (attribute, AttributeKind::NormalOrNonDataDescriptor)
                }
            }

            _ => (attribute, AttributeKind::NormalOrNonDataDescriptor),
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
        fallback: SymbolAndQualifiers<'db>,
        policy: InstanceFallbackShadowsNonDataDescriptor,
        member_policy: MemberLookupPolicy,
    ) -> SymbolAndQualifiers<'db> {
        let (
            SymbolAndQualifiers {
                symbol: meta_attr,
                qualifiers: meta_attr_qualifiers,
            },
            meta_attr_kind,
        ) = Self::try_call_dunder_get_on_attribute(
            db,
            self.class_member(db, name.into(), member_policy),
            self,
            self.to_meta_type(db),
        );

        let SymbolAndQualifiers {
            symbol: fallback,
            qualifiers: fallback_qualifiers,
        } = fallback;

        match (meta_attr, meta_attr_kind, fallback) {
            // The fallback type is unbound, so we can just return `meta_attr` unconditionally,
            // no matter if it's data descriptor, a non-data descriptor, or a normal attribute.
            (meta_attr @ Symbol::Type(_, _), _, Symbol::Unbound) => {
                meta_attr.with_qualifiers(meta_attr_qualifiers)
            }

            // `meta_attr` is the return type of a data descriptor and definitely bound, so we
            // return it.
            (meta_attr @ Symbol::Type(_, Boundness::Bound), AttributeKind::DataDescriptor, _) => {
                meta_attr.with_qualifiers(meta_attr_qualifiers)
            }

            // `meta_attr` is the return type of a data descriptor, but the attribute on the
            // meta-type is possibly-unbound. This means that we "fall through" to the next
            // stage of the descriptor protocol and union with the fallback type.
            (
                Symbol::Type(meta_attr_ty, Boundness::PossiblyUnbound),
                AttributeKind::DataDescriptor,
                Symbol::Type(fallback_ty, fallback_boundness),
            ) => Symbol::Type(
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
                Symbol::Type(_, _),
                AttributeKind::NormalOrNonDataDescriptor,
                fallback @ Symbol::Type(_, Boundness::Bound),
            ) if policy == InstanceFallbackShadowsNonDataDescriptor::Yes => {
                fallback.with_qualifiers(fallback_qualifiers)
            }

            // `meta_attr` is *not* a data descriptor. The `fallback` symbol is either possibly
            // unbound or the policy argument is `No`. In both cases, the `fallback` type does
            // not completely shadow the non-data descriptor, so we build a union of the two.
            (
                Symbol::Type(meta_attr_ty, meta_attr_boundness),
                AttributeKind::NormalOrNonDataDescriptor,
                Symbol::Type(fallback_ty, fallback_boundness),
            ) => Symbol::Type(
                UnionType::from_elements(db, [meta_attr_ty, fallback_ty]),
                meta_attr_boundness.max(fallback_boundness),
            )
            .with_qualifiers(meta_attr_qualifiers.union(fallback_qualifiers)),

            // If the attribute is not found on the meta-type, we simply return the fallback.
            (Symbol::Unbound, _, fallback) => fallback.with_qualifiers(fallback_qualifiers),
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
    pub(crate) fn member(self, db: &'db dyn Db, name: &str) -> SymbolAndQualifiers<'db> {
        self.member_lookup_with_policy(db, name.into(), MemberLookupPolicy::default())
    }

    /// Similar to [`Type::member`], but allows the caller to specify what policy should be used
    /// when looking up attributes. See [`MemberLookupPolicy`] for more information.
    #[salsa::tracked]
    fn member_lookup_with_policy(
        self,
        db: &'db dyn Db,
        name: Name,
        policy: MemberLookupPolicy,
    ) -> SymbolAndQualifiers<'db> {
        tracing::trace!("member_lookup_with_policy: {}.{}", self.display(db), name);
        if name == "__class__" {
            return Symbol::bound(self.to_meta_type(db)).into();
        }

        let name_str = name.as_str();

        match self {
            Type::Union(union) => union
                .map_with_boundness(db, |elem| {
                    elem.member_lookup_with_policy(db, name_str.into(), policy)
                        .symbol
                })
                .into(),

            Type::Intersection(intersection) => intersection
                .map_with_boundness(db, |elem| {
                    elem.member_lookup_with_policy(db, name_str.into(), policy)
                        .symbol
                })
                .into(),

            Type::Dynamic(..) | Type::Never => Symbol::bound(self).into(),

            Type::FunctionLiteral(function) if name == "__get__" => Symbol::bound(
                Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderGet(function)),
            )
            .into(),
            Type::PropertyInstance(property) if name == "__get__" => Symbol::bound(
                Type::MethodWrapper(MethodWrapperKind::PropertyDunderGet(property)),
            )
            .into(),
            Type::PropertyInstance(property) if name == "__set__" => Symbol::bound(
                Type::MethodWrapper(MethodWrapperKind::PropertyDunderSet(property)),
            )
            .into(),

            Type::ClassLiteral(ClassLiteralType { class })
                if name == "__get__" && class.is_known(db, KnownClass::FunctionType) =>
            {
                Symbol::bound(Type::WrapperDescriptor(
                    WrapperDescriptorKind::FunctionTypeDunderGet,
                ))
                .into()
            }
            Type::ClassLiteral(ClassLiteralType { class })
                if name == "__get__" && class.is_known(db, KnownClass::Property) =>
            {
                Symbol::bound(Type::WrapperDescriptor(
                    WrapperDescriptorKind::PropertyDunderGet,
                ))
                .into()
            }
            Type::ClassLiteral(ClassLiteralType { class })
                if name == "__set__" && class.is_known(db, KnownClass::Property) =>
            {
                Symbol::bound(Type::WrapperDescriptor(
                    WrapperDescriptorKind::PropertyDunderSet,
                ))
                .into()
            }
            Type::BoundMethod(bound_method) => match name_str {
                "__self__" => Symbol::bound(bound_method.self_instance(db)).into(),
                "__func__" => {
                    Symbol::bound(Type::FunctionLiteral(bound_method.function(db))).into()
                }
                _ => {
                    KnownClass::MethodType
                        .to_instance(db)
                        .member(db, &name)
                        .or_fall_back_to(db, || {
                            // If an attribute is not available on the bound method object,
                            // it will be looked up on the underlying function object:
                            Type::FunctionLiteral(bound_method.function(db)).member(db, &name)
                        })
                }
            },
            Type::MethodWrapper(_) => KnownClass::MethodWrapperType
                .to_instance(db)
                .member(db, &name),
            Type::WrapperDescriptor(_) => KnownClass::WrapperDescriptorType
                .to_instance(db)
                .member(db, &name),
            Type::Callable(_) => KnownClass::Object.to_instance(db).member(db, &name),

            Type::Instance(InstanceType { class })
                if matches!(name.as_str(), "major" | "minor")
                    && class.is_known(db, KnownClass::VersionInfo) =>
            {
                let python_version = Program::get(db).python_version(db);
                let segment = if name == "major" {
                    python_version.major
                } else {
                    python_version.minor
                };
                Symbol::bound(Type::IntLiteral(segment.into())).into()
            }

            Type::Instance(InstanceType { class }) if class.is_known(db, KnownClass::Super) => {
                SymbolAndQualifiers::todo("super() support")
            }

            Type::PropertyInstance(property) if name == "fget" => {
                Symbol::bound(property.getter(db).unwrap_or(Type::none(db))).into()
            }
            Type::PropertyInstance(property) if name == "fset" => {
                Symbol::bound(property.setter(db).unwrap_or(Type::none(db))).into()
            }

            Type::IntLiteral(_) if matches!(name_str, "real" | "numerator") => {
                Symbol::bound(self).into()
            }

            Type::BooleanLiteral(bool_value) if matches!(name_str, "real" | "numerator") => {
                Symbol::bound(Type::IntLiteral(i64::from(bool_value))).into()
            }

            Type::ModuleLiteral(module) => module.static_member(db, name_str).into(),

            Type::AlwaysFalsy | Type::AlwaysTruthy => self.class_member(db, name, policy),

            _ if policy.no_instance_fallback() => self.invoke_descriptor_protocol(
                db,
                name_str,
                Symbol::Unbound.into(),
                InstanceFallbackShadowsNonDataDescriptor::No,
                policy,
            ),

            Type::Instance(..)
            | Type::BooleanLiteral(..)
            | Type::IntLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::LiteralString
            | Type::SliceLiteral(..)
            | Type::Tuple(..)
            | Type::TypeVar(..)
            | Type::KnownInstance(..)
            | Type::PropertyInstance(..)
            | Type::FunctionLiteral(..) => {
                let fallback = self.instance_member(db, name_str);

                let result = self.invoke_descriptor_protocol(
                    db,
                    name_str,
                    fallback,
                    InstanceFallbackShadowsNonDataDescriptor::No,
                    policy,
                );

                let custom_getattr_result = || {
                    // Typeshed has a fake `__getattr__` on `types.ModuleType` to help out with dynamic imports.
                    // We explicitly hide it here to prevent arbitrary attributes from being available on modules.
                    // Same for `types.GenericAlias` - it's `__getattr__` method will delegate to `__origin__`
                    // to allow looking up attributes on the original type. But in typeshed it's
                    // return type is `Any`. It will need a special handling, so it remember the origin type
                    // to properly resolve the attribute.
                    if self.into_instance().is_some_and(|instance| {
                        instance.class.is_known(db, KnownClass::ModuleType)
                            || instance.class.is_known(db, KnownClass::GenericAlias)
                    }) {
                        return Symbol::Unbound.into();
                    }

                    self.try_call_dunder(
                        db,
                        "__getattr__",
                        CallArgumentTypes::positional([Type::StringLiteral(
                            StringLiteralType::new(db, Box::from(name.as_str())),
                        )]),
                    )
                    .map(|outcome| Symbol::bound(outcome.return_type(db)))
                    // TODO: Handle call errors here.
                    .unwrap_or(Symbol::Unbound)
                    .into()
                };

                match result {
                    member @ SymbolAndQualifiers {
                        symbol: Symbol::Type(_, Boundness::Bound),
                        qualifiers: _,
                    } => member,
                    member @ SymbolAndQualifiers {
                        symbol: Symbol::Type(_, Boundness::PossiblyUnbound),
                        qualifiers: _,
                    } => member.or_fall_back_to(db, custom_getattr_result),
                    SymbolAndQualifiers {
                        symbol: Symbol::Unbound,
                        qualifiers: _,
                    } => custom_getattr_result(),
                }
            }

            Type::ClassLiteral(..) | Type::SubclassOf(..) => {
                let class_attr_plain = self.find_name_in_mro(db, name_str, policy).expect(
                    "Calling `find_name_in_mro` on class literals and subclass-of types should always return `Some`",
                );

                if name == "__mro__" {
                    return class_attr_plain;
                }

                if self.is_subtype_of(db, KnownClass::Enum.to_subclass_of(db)) {
                    return SymbolAndQualifiers::todo("Attribute access on enum classes");
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

            match self.try_call_dunder(db, "__bool__", CallArgumentTypes::none()) {
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
            Type::Dynamic(_) | Type::Never | Type::Callable(_) | Type::LiteralString => {
                Truthiness::Ambiguous
            }

            Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(_)
            | Type::ModuleLiteral(_)
            | Type::SliceLiteral(_)
            | Type::AlwaysTruthy => Truthiness::AlwaysTrue,

            Type::AlwaysFalsy => Truthiness::AlwaysFalse,

            Type::ClassLiteral(ClassLiteralType { class }) => class
                .metaclass_instance_type(db)
                .try_bool_impl(db, allow_short_circuit)?,

            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                ClassBase::Dynamic(_) => Truthiness::Ambiguous,
                ClassBase::Class(class) => {
                    Type::class_literal(class).try_bool_impl(db, allow_short_circuit)?
                }
            },

            Type::TypeVar(typevar) => match typevar.bound_or_constraints(db) {
                None => Truthiness::Ambiguous,
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    bound.try_bool_impl(db, allow_short_circuit)?
                }
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    try_union(constraints)?
                }
            },

            Type::Instance(InstanceType { class }) => match class.known(db) {
                Some(known_class) => known_class.bool(),
                None => try_dunder_bool()?,
            },

            Type::KnownInstance(known_instance) => known_instance.bool(),

            Type::PropertyInstance(_) => Truthiness::AlwaysTrue,

            Type::Union(union) => try_union(*union)?,

            Type::Intersection(_) => {
                // TODO
                Truthiness::Ambiguous
            }

            Type::IntLiteral(num) => Truthiness::from(*num != 0),
            Type::BooleanLiteral(bool) => Truthiness::from(*bool),
            Type::StringLiteral(str) => Truthiness::from(!str.value(db).is_empty()),
            Type::BytesLiteral(bytes) => Truthiness::from(!bytes.value(db).is_empty()),
            Type::Tuple(items) => Truthiness::from(!items.elements(db).is_empty()),
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
                    let mut builder = UnionBuilder::new(db);
                    for element in union.elements(db) {
                        builder = builder.add(non_negative_int_literal(db, *element)?);
                    }
                    Some(builder.build())
                }
                _ => None,
            }
        }

        let usize_len = match self {
            Type::BytesLiteral(bytes) => Some(bytes.python_len(db)),
            Type::StringLiteral(string) => Some(string.python_len(db)),
            Type::Tuple(tuple) => Some(tuple.len(db)),
            _ => None,
        };

        if let Some(usize_len) = usize_len {
            return usize_len.try_into().ok().map(Type::IntLiteral);
        }

        let return_ty = match self.try_call_dunder(db, "__len__", CallArgumentTypes::none()) {
            Ok(bindings) => bindings.return_type(db),
            Err(CallDunderError::PossiblyUnbound(bindings)) => bindings.return_type(db),

            // TODO: emit a diagnostic
            Err(CallDunderError::MethodNotAvailable) => return None,
            Err(CallDunderError::CallError(_, bindings)) => bindings.return_type(db),
        };

        non_negative_int_literal(db, return_ty)
    }

    /// Returns the call signatures of a type.
    ///
    /// Note that all types have a valid [`Signatures`], even if the type is not callable.
    /// Moreover, "callable" can be subtle for a union type, since some union elements might be
    /// callable and some not. A union is callable if every element type is callable — and even
    /// then, the elements might be inconsistent, such that there's no argument list that's valid
    /// for all elements. It's usually best to only worry about "callability" relative to a
    /// particular argument list, via [`try_call`][Self::try_call] and
    /// [`CallErrorKind::NotCallable`].
    fn signatures(self, db: &'db dyn Db) -> Signatures<'db> {
        match self {
            Type::Callable(callable) => Signatures::single(CallableSignature::single(
                self,
                callable.signature(db).clone(),
            )),

            Type::BoundMethod(bound_method) => {
                let signature = bound_method.function(db).signature(db);
                let signature = CallableSignature::single(self, signature.clone())
                    .with_bound_type(bound_method.self_instance(db));
                Signatures::single(signature)
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

                let not_none = Type::none(db).negate(db);
                let signature = CallableSignature::from_overloads(
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
                                    .with_annotated_type(not_none),
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
                );
                Signatures::single(signature)
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
                let not_none = Type::none(db).negate(db);
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
                let signature = CallableSignature::from_overloads(
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
                                    .with_annotated_type(not_none),
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
                );
                Signatures::single(signature)
            }

            Type::MethodWrapper(MethodWrapperKind::PropertyDunderSet(_)) => {
                Signatures::single(CallableSignature::single(
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
                ))
            }
            Type::WrapperDescriptor(WrapperDescriptorKind::PropertyDunderSet) => {
                Signatures::single(CallableSignature::single(
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
                ))
            }

            Type::FunctionLiteral(function_type) => match function_type.known(db) {
                Some(
                    KnownFunction::IsEquivalentTo
                    | KnownFunction::IsSubtypeOf
                    | KnownFunction::IsAssignableTo
                    | KnownFunction::IsDisjointFrom
                    | KnownFunction::IsGradualEquivalentTo,
                ) => {
                    let signature = CallableSignature::single(
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
                    );
                    Signatures::single(signature)
                }

                Some(
                    KnownFunction::IsFullyStatic
                    | KnownFunction::IsSingleton
                    | KnownFunction::IsSingleValued,
                ) => {
                    let signature = CallableSignature::single(
                        self,
                        Signature::new(
                            Parameters::new([Parameter::positional_only(Some(Name::new_static(
                                "a",
                            )))
                            .type_form()
                            .with_annotated_type(Type::any())]),
                            Some(KnownClass::Bool.to_instance(db)),
                        ),
                    );
                    Signatures::single(signature)
                }

                Some(KnownFunction::AssertType) => {
                    let signature = CallableSignature::single(
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
                    );
                    Signatures::single(signature)
                }

                Some(KnownFunction::Cast) => {
                    let signature = CallableSignature::single(
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
                    );
                    Signatures::single(signature)
                }

                _ => Signatures::single(CallableSignature::single(
                    self,
                    function_type.signature(db).clone(),
                )),
            },

            Type::ClassLiteral(ClassLiteralType { class }) => match class.known(db) {
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
                    let signature = CallableSignature::single(
                        self,
                        Signature::new(
                            Parameters::new([Parameter::positional_only(Some(Name::new_static(
                                "o",
                            )))
                            .with_annotated_type(Type::any())
                            .with_default_type(Type::BooleanLiteral(false))]),
                            Some(KnownClass::Bool.to_instance(db)),
                        ),
                    );
                    Signatures::single(signature)
                }

                Some(KnownClass::Str) => {
                    // ```py
                    // class str(Sequence[str]):
                    //     @overload
                    //     def __new__(cls, object: object = ...) -> Self: ...
                    //     @overload
                    //     def __new__(cls, object: ReadableBuffer, encoding: str = ..., errors: str = ...) -> Self: ...
                    // ```
                    let signature = CallableSignature::from_overloads(
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
                    );
                    Signatures::single(signature)
                }

                Some(KnownClass::Type) => {
                    // ```py
                    // class type:
                    //     @overload
                    //     def __init__(self, o: object, /) -> None: ...
                    //     @overload
                    //     def __init__(self, name: str, bases: tuple[type, ...], dict: dict[str, Any], /, **kwds: Any) -> None: ...
                    // ```
                    let signature = CallableSignature::from_overloads(
                        self,
                        [
                            Signature::new(
                                Parameters::new([Parameter::positional_only(Some(
                                    Name::new_static("o"),
                                ))
                                .with_annotated_type(Type::any())]),
                                Some(KnownClass::Type.to_instance(db)),
                            ),
                            Signature::new(
                                Parameters::new([
                                    Parameter::positional_only(Some(Name::new_static("name")))
                                        .with_annotated_type(KnownClass::Str.to_instance(db)),
                                    Parameter::positional_only(Some(Name::new_static("bases")))
                                        // TODO: Should be tuple[type, ...] once we have support for homogenous tuples
                                        .with_annotated_type(KnownClass::Tuple.to_instance(db)),
                                    Parameter::positional_only(Some(Name::new_static("dict")))
                                        // TODO: Should be `dict[str, Any]` once we have support for generics
                                        .with_annotated_type(KnownClass::Dict.to_instance(db)),
                                ]),
                                Some(KnownClass::Type.to_instance(db)),
                            ),
                        ],
                    );
                    Signatures::single(signature)
                }
                Some(KnownClass::Object) => {
                    // ```py
                    // class object:
                    //    def __init__(self) -> None: ...
                    //    def __new__(cls) -> Self: ...
                    // ```
                    let signature = CallableSignature::from_overloads(
                        self,
                        [Signature::new(
                            Parameters::empty(),
                            Some(KnownClass::Object.to_instance(db)),
                        )],
                    );
                    Signatures::single(signature)
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

                    let signature = CallableSignature::single(
                        self,
                        Signature::new(
                            Parameters::new([
                                Parameter::positional_or_keyword(Name::new_static("fget"))
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [
                                            Type::Callable(CallableType::new(db, getter_signature)),
                                            Type::none(db),
                                        ],
                                    ))
                                    .with_default_type(Type::none(db)),
                                Parameter::positional_or_keyword(Name::new_static("fset"))
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [
                                            Type::Callable(CallableType::new(db, setter_signature)),
                                            Type::none(db),
                                        ],
                                    ))
                                    .with_default_type(Type::none(db)),
                                Parameter::positional_or_keyword(Name::new_static("fdel"))
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [
                                            Type::Callable(CallableType::new(
                                                db,
                                                deleter_signature,
                                            )),
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
                    );
                    Signatures::single(signature)
                }

                // Most class literal constructor calls are handled by `try_call_constructor` and
                // not via getting the signature here. This signature can still be used in some
                // cases (e.g. evaluating callable subtyping). TODO improve this definition
                // (intersection of `__new__` and `__init__` signatures? and respect metaclass
                // `__call__`).
                _ => {
                    let signature = CallableSignature::single(
                        self,
                        Signature::new(Parameters::gradual_form(), self.to_instance(db)),
                    );
                    Signatures::single(signature)
                }
            },

            Type::SubclassOf(subclass_of_type) => match subclass_of_type.subclass_of() {
                ClassBase::Dynamic(dynamic_type) => Type::Dynamic(dynamic_type).signatures(db),
                // Most type[] constructor calls are handled by `try_call_constructor` and not via
                // getting the signature here. This signature can still be used in some cases (e.g.
                // evaluating callable subtyping). TODO improve this definition (intersection of
                // `__new__` and `__init__` signatures? and respect metaclass `__call__`).
                ClassBase::Class(class) => Type::class_literal(class).signatures(db),
            },

            Type::Instance(_) => {
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
                    .symbol
                {
                    Symbol::Type(dunder_callable, boundness) => {
                        let mut signatures = dunder_callable.signatures(db).clone();
                        signatures.replace_callable_type(dunder_callable, self);
                        if boundness == Boundness::PossiblyUnbound {
                            signatures.set_dunder_call_is_possibly_unbound();
                        }
                        signatures
                    }
                    Symbol::Unbound => Signatures::not_callable(self),
                }
            }

            // Dynamic types are callable, and the return type is the same dynamic type. Similarly,
            // `Never` is always callable and returns `Never`.
            Type::Dynamic(_) | Type::Never => Signatures::single(CallableSignature::dynamic(self)),

            // Note that this correctly returns `None` if none of the union elements are callable.
            Type::Union(union) => Signatures::from_union(
                self,
                union
                    .elements(db)
                    .iter()
                    .map(|element| element.signatures(db)),
            ),

            Type::Intersection(_) => {
                Signatures::single(CallableSignature::todo("Type::Intersection.call()"))
            }

            _ => Signatures::not_callable(self),
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
        mut argument_types: CallArgumentTypes<'_, 'db>,
    ) -> Result<Bindings<'db>, CallError<'db>> {
        let signatures = self.signatures(db);
        Bindings::match_parameters(signatures, &mut argument_types)
            .check_types(db, &mut argument_types)
    }

    /// Look up a dunder method on the meta-type of `self` and call it.
    ///
    /// Returns an `Err` if the dunder method can't be called,
    /// or the given arguments are not valid.
    fn try_call_dunder(
        self,
        db: &'db dyn Db,
        name: &str,
        mut argument_types: CallArgumentTypes<'_, 'db>,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        match self
            .member_lookup_with_policy(db, name.into(), MemberLookupPolicy::NO_INSTANCE_FALLBACK)
            .symbol
        {
            Symbol::Type(dunder_callable, boundness) => {
                let signatures = dunder_callable.signatures(db);
                let bindings = Bindings::match_parameters(signatures, &mut argument_types)
                    .check_types(db, &mut argument_types)?;
                if boundness == Boundness::PossiblyUnbound {
                    return Err(CallDunderError::PossiblyUnbound(Box::new(bindings)));
                }
                Ok(bindings)
            }
            Symbol::Unbound => Err(CallDunderError::MethodNotAvailable),
        }
    }

    /// Returns the element type when iterating over `self`.
    ///
    /// This method should only be used outside of type checking because it omits any errors.
    /// For type checking, use [`try_iterate`](Self::try_iterate) instead.
    fn iterate(self, db: &'db dyn Db) -> Type<'db> {
        self.try_iterate(db)
            .unwrap_or_else(|err| err.fallback_element_type(db))
    }

    /// Given the type of an object that is iterated over in some way,
    /// return the type of objects that are yielded by that iteration.
    ///
    /// E.g., for the following loop, given the type of `x`, infer the type of `y`:
    /// ```python
    /// for y in x:
    ///     pass
    /// ```
    fn try_iterate(self, db: &'db dyn Db) -> Result<Type<'db>, IterationError<'db>> {
        if let Type::Tuple(tuple_type) = self {
            return Ok(UnionType::from_elements(db, tuple_type.elements(db)));
        }

        let try_call_dunder_getitem = || {
            self.try_call_dunder(
                db,
                "__getitem__",
                CallArgumentTypes::positional([KnownClass::Int.to_instance(db)]),
            )
            .map(|dunder_getitem_outcome| dunder_getitem_outcome.return_type(db))
        };

        let try_call_dunder_next_on_iterator = |iterator: Type<'db>| {
            iterator
                .try_call_dunder(db, "__next__", CallArgumentTypes::none())
                .map(|dunder_next_outcome| dunder_next_outcome.return_type(db))
        };

        let dunder_iter_result = self
            .try_call_dunder(db, "__iter__", CallArgumentTypes::none())
            .map(|dunder_iter_outcome| dunder_iter_outcome.return_type(db));

        match dunder_iter_result {
            Ok(iterator) => {
                // `__iter__` is definitely bound and calling it succeeds.
                // See what calling `__next__` on the object returned by `__iter__` gives us...
                try_call_dunder_next_on_iterator(iterator).map_err(|dunder_next_error| {
                    IterationError::IterReturnsInvalidIterator {
                        iterator,
                        dunder_next_error,
                    }
                })
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
                                UnionType::from_elements(
                                    db,
                                    [dunder_next_return, dunder_getitem_return_type],
                                )
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
                        dunder_next_error,
                    }),
                }
            }

            // `__iter__` is definitely bound but it can't be called with the expected arguments
            Err(CallDunderError::CallError(kind, bindings)) => {
                Err(IterationError::IterCallError(kind, bindings))
            }

            // There's no `__iter__` method. Try `__getitem__` instead...
            Err(CallDunderError::MethodNotAvailable) => {
                try_call_dunder_getitem().map_err(|dunder_getitem_error| {
                    IterationError::UnboundIterAndGetitemError {
                        dunder_getitem_error,
                    }
                })
            }
        }
    }

    /// Returns the type bound from a context manager with type `self`.
    ///
    /// This method should only be used outside of type checking because it omits any errors.
    /// For type checking, use [`try_enter`](Self::try_enter) instead.
    fn enter(self, db: &'db dyn Db) -> Type<'db> {
        self.try_enter(db)
            .unwrap_or_else(|err| err.fallback_enter_type(db))
    }

    /// Given the type of an object that is used as a context manager (i.e. in a `with` statement),
    /// return the return type of its `__enter__` method, which is bound to any potential targets.
    ///
    /// E.g., for the following `with` statement, given the type of `x`, infer the type of `y`:
    /// ```python
    /// with x as y:
    ///     pass
    /// ```
    fn try_enter(self, db: &'db dyn Db) -> Result<Type<'db>, ContextManagerError<'db>> {
        let enter = self.try_call_dunder(db, "__enter__", CallArgumentTypes::none());
        let exit = self.try_call_dunder(
            db,
            "__exit__",
            CallArgumentTypes::positional([Type::none(db), Type::none(db), Type::none(db)]),
        );

        // TODO: Make use of Protocols when we support it (the manager be assignable to `contextlib.AbstractContextManager`).
        match (enter, exit) {
            (Ok(enter), Ok(_)) => Ok(enter.return_type(db)),
            (Ok(enter), Err(exit_error)) => Err(ContextManagerError::Exit {
                enter_return_type: enter.return_type(db),
                exit_error,
            }),
            // TODO: Use the `exit_ty` to determine if any raised exception is suppressed.
            (Err(enter_error), Ok(_)) => Err(ContextManagerError::Enter(enter_error)),
            (Err(enter_error), Err(exit_error)) => Err(ContextManagerError::EnterAndExit {
                enter_error,
                exit_error,
            }),
        }
    }

    /// Given a class literal or non-dynamic SubclassOf type, try calling it (creating an instance)
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
        argument_types: CallArgumentTypes<'_, 'db>,
    ) -> Result<Type<'db>, CreateInstanceCallError<'db>> {
        debug_assert!(matches!(self, Type::ClassLiteral(_) | Type::SubclassOf(_)));

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
        // Note that we currently ignore `__new__` return type, since we do not support `Self`
        // and most builtin classes use it as return type annotation. We always return the instance type.

        // Lookup `__new__` method in the MRO up to, but not including, `object`. Also, we must
        // avoid `__new__` on `type` since per descriptor protocol, if `__new__` is not defined on
        // a class, metaclass attribute would take precedence. But by avoiding `__new__` on `object`
        // we would inadvertently unhide `__new__` on `type`, which is not what we want.
        let new_call_outcome: Option<Result<Bindings<'db>, CallDunderError<'db>>> = match self
            .member_lookup_with_policy(
                db,
                "__new__".into(),
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                    | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            )
            .symbol
        {
            Symbol::Type(dunder_callable, boundness) => {
                let signatures = dunder_callable.signatures(db);
                // `__new__` is a static method, so we much inject the `cls` argument.
                let mut argument_types = argument_types.prepend_synthetic(self);

                Some(
                    match Bindings::match_parameters(signatures, &mut argument_types)
                        .check_types(db, &mut argument_types)
                    {
                        Ok(bindings) => {
                            if boundness == Boundness::PossiblyUnbound {
                                Err(CallDunderError::PossiblyUnbound(Box::new(bindings)))
                            } else {
                                Ok(bindings)
                            }
                        }
                        Err(err) => Err(err.into()),
                    },
                )
            }
            // No explicit `__new__` method found
            Symbol::Unbound => None,
        };

        // TODO: we should use the actual return type of `__new__` to determine the instance type
        let instance_ty = self
            .to_instance(db)
            .expect("Class literal type and subclass-of types should always be convertible to instance type");

        let init_call_outcome = if new_call_outcome.is_none()
            || !instance_ty
                .member_lookup_with_policy(
                    db,
                    "__init__".into(),
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                )
                .symbol
                .is_unbound()
        {
            Some(instance_ty.try_call_dunder(db, "__init__", argument_types))
        } else {
            None
        };

        match (new_call_outcome, init_call_outcome) {
            // All calls are successful or not called at all
            (None | Some(Ok(_)), None | Some(Ok(_))) => Ok(instance_ty),
            (None | Some(Ok(_)), Some(Err(error))) => {
                // no custom `__new__` or it was called and succeeded, but `__init__` failed.
                Err(CreateInstanceCallError::Init(instance_ty, error))
            }
            (Some(Err(error)), None | Some(Ok(_))) => {
                // custom `__new__` was called and failed, but init is ok
                Err(CreateInstanceCallError::New(instance_ty, error))
            }
            (Some(Err(new_error)), Some(Err(init_error))) => {
                // custom `__new__` was called and failed, and `__init__` is also not ok
                Err(CreateInstanceCallError::NewAndInit(
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
            Type::ClassLiteral(ClassLiteralType { class }) => Some(Type::instance(*class)),
            Type::SubclassOf(subclass_of_ty) => Some(subclass_of_ty.to_instance()),
            Type::Union(union) => {
                let mut builder = UnionBuilder::new(db);
                for element in union.elements(db) {
                    builder = builder.add(element.to_instance(db)?);
                }
                Some(builder.build())
            }
            Type::Intersection(_) => Some(todo_type!("Type::Intersection.to_instance()")),
            Type::BooleanLiteral(_)
            | Type::BytesLiteral(_)
            | Type::FunctionLiteral(_)
            | Type::Callable(..)
            | Type::MethodWrapper(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::Instance(_)
            | Type::KnownInstance(_)
            | Type::PropertyInstance(_)
            | Type::ModuleLiteral(_)
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::SliceLiteral(_)
            | Type::Tuple(_)
            | Type::TypeVar(_)
            | Type::LiteralString
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy => None,
        }
    }

    /// If we see a value of this type used as a type expression, what type does it name?
    ///
    /// For example, the builtin `int` as a value expression is of type
    /// `Type::ClassLiteral(builtins.int)`, that is, it is the `int` class itself. As a type
    /// expression, it names the type `Type::Instance(builtins.int)`, that is, all objects whose
    /// `__class__` is `int`.
    pub fn in_type_expression(
        &self,
        db: &'db dyn Db,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        match self {
            // Special cases for `float` and `complex`
            // https://typing.readthedocs.io/en/latest/spec/special-types.html#special-cases-for-float-and-complex
            Type::ClassLiteral(ClassLiteralType { class }) => {
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
                    _ => Type::instance(*class),
                };
                Ok(ty)
            }

            Type::SubclassOf(_)
            | Type::BooleanLiteral(_)
            | Type::BytesLiteral(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::SliceLiteral(_)
            | Type::IntLiteral(_)
            | Type::LiteralString
            | Type::ModuleLiteral(_)
            | Type::StringLiteral(_)
            | Type::Tuple(_)
            | Type::TypeVar(_)
            | Type::Callable(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::MethodWrapper(_)
            | Type::Never
            | Type::FunctionLiteral(_)
            | Type::PropertyInstance(_) => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec![InvalidTypeExpression::InvalidType(*self)],
                fallback_type: Type::unknown(),
            }),

            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::TypeAliasType(alias) => Ok(alias.value_type(db)),
                KnownInstanceType::Never | KnownInstanceType::NoReturn => Ok(Type::Never),
                KnownInstanceType::LiteralString => Ok(Type::LiteralString),
                KnownInstanceType::Any => Ok(Type::any()),
                KnownInstanceType::Unknown => Ok(Type::unknown()),
                KnownInstanceType::AlwaysTruthy => Ok(Type::AlwaysTruthy),
                KnownInstanceType::AlwaysFalsy => Ok(Type::AlwaysFalsy),

                // We treat `typing.Type` exactly the same as `builtins.type`:
                KnownInstanceType::Type => Ok(KnownClass::Type.to_instance(db)),
                KnownInstanceType::Tuple => Ok(KnownClass::Tuple.to_instance(db)),

                // Legacy `typing` aliases
                KnownInstanceType::List => Ok(KnownClass::List.to_instance(db)),
                KnownInstanceType::Dict => Ok(KnownClass::Dict.to_instance(db)),
                KnownInstanceType::Set => Ok(KnownClass::Set.to_instance(db)),
                KnownInstanceType::FrozenSet => Ok(KnownClass::FrozenSet.to_instance(db)),
                KnownInstanceType::ChainMap => Ok(KnownClass::ChainMap.to_instance(db)),
                KnownInstanceType::Counter => Ok(KnownClass::Counter.to_instance(db)),
                KnownInstanceType::DefaultDict => Ok(KnownClass::DefaultDict.to_instance(db)),
                KnownInstanceType::Deque => Ok(KnownClass::Deque.to_instance(db)),
                KnownInstanceType::OrderedDict => Ok(KnownClass::OrderedDict.to_instance(db)),

                KnownInstanceType::TypeVar(typevar) => Ok(Type::TypeVar(*typevar)),

                // TODO: Use an opt-in rule for a bare `Callable`
                KnownInstanceType::Callable => Ok(Type::Callable(CallableType::unknown(db))),

                KnownInstanceType::TypingSelf => Ok(todo_type!("Support for `typing.Self`")),
                KnownInstanceType::TypeAlias => Ok(todo_type!("Support for `typing.TypeAlias`")),

                KnownInstanceType::Protocol => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec![InvalidTypeExpression::Protocol],
                    fallback_type: Type::unknown(),
                }),

                KnownInstanceType::Literal
                | KnownInstanceType::Union
                | KnownInstanceType::Intersection => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec![
                        InvalidTypeExpression::RequiresArguments(*self)
                    ],
                    fallback_type: Type::unknown(),
                }),

                KnownInstanceType::Optional
                | KnownInstanceType::Not
                | KnownInstanceType::TypeOf
                | KnownInstanceType::TypeIs
                | KnownInstanceType::TypeGuard
                | KnownInstanceType::Unpack
                | KnownInstanceType::CallableTypeOf => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec![
                        InvalidTypeExpression::RequiresOneArgument(*self)
                    ],
                    fallback_type: Type::unknown(),
                }),

                KnownInstanceType::Annotated | KnownInstanceType::Concatenate => {
                    Err(InvalidTypeExpressionError {
                        invalid_expressions: smallvec::smallvec![
                            InvalidTypeExpression::RequiresTwoArguments(*self)
                        ],
                        fallback_type: Type::unknown(),
                    })
                }

                KnownInstanceType::ClassVar | KnownInstanceType::Final => {
                    Err(InvalidTypeExpressionError {
                        invalid_expressions: smallvec::smallvec![
                            InvalidTypeExpression::TypeQualifier(*known_instance)
                        ],
                        fallback_type: Type::unknown(),
                    })
                }

                KnownInstanceType::ReadOnly
                | KnownInstanceType::NotRequired
                | KnownInstanceType::Required => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec![
                        InvalidTypeExpression::TypeQualifierRequiresOneArgument(*known_instance)
                    ],
                    fallback_type: Type::unknown(),
                }),
            },

            Type::Union(union) => {
                let mut builder = UnionBuilder::new(db);
                let mut invalid_expressions = smallvec::SmallVec::default();
                for element in union.elements(db) {
                    match element.in_type_expression(db) {
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

            Type::Instance(InstanceType { class }) => match class.known(db) {
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
                _ => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec![InvalidTypeExpression::InvalidType(
                        *self
                    )],
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

    /// Return the type of `tuple(sys.version_info)`.
    ///
    /// This is not exactly the type that `sys.version_info` has at runtime,
    /// but it's a useful fallback for us in order to infer `Literal` types from `sys.version_info` comparisons.
    fn version_info_tuple(db: &'db dyn Db) -> Self {
        let python_version = Program::get(db).python_version(db);
        let int_instance_ty = KnownClass::Int.to_instance(db);

        // TODO: just grab this type from typeshed (it's a `sys._ReleaseLevel` type alias there)
        let release_level_ty = {
            let elements: Box<[Type<'db>]> = ["alpha", "beta", "candidate", "final"]
                .iter()
                .map(|level| Type::string_literal(db, level))
                .collect();

            // For most unions, it's better to go via `UnionType::from_elements` or use `UnionBuilder`;
            // those techniques ensure that union elements are deduplicated and unions are eagerly simplified
            // into other types where necessary. Here, however, we know that there are no duplicates
            // in this union, so it's probably more efficient to use `UnionType::new()` directly.
            Type::Union(UnionType::new(db, elements))
        };

        TupleType::from_elements(
            db,
            [
                Type::IntLiteral(python_version.major.into()),
                Type::IntLiteral(python_version.minor.into()),
                int_instance_ty,
                release_level_ty,
                int_instance_ty,
            ],
        )
    }

    /// Given a type that is assumed to represent an instance of a class,
    /// return a type that represents that class itself.
    #[must_use]
    pub fn to_meta_type(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::Never => Type::Never,
            Type::Instance(InstanceType { class }) => SubclassOfType::from(db, *class),
            Type::KnownInstance(known_instance) => known_instance.class().to_class_literal(db),
            Type::PropertyInstance(_) => KnownClass::Property.to_class_literal(db),
            Type::Union(union) => union.map(db, |ty| ty.to_meta_type(db)),
            Type::BooleanLiteral(_) => KnownClass::Bool.to_class_literal(db),
            Type::BytesLiteral(_) => KnownClass::Bytes.to_class_literal(db),
            Type::SliceLiteral(_) => KnownClass::Slice.to_class_literal(db),
            Type::IntLiteral(_) => KnownClass::Int.to_class_literal(db),
            Type::FunctionLiteral(_) => KnownClass::FunctionType.to_class_literal(db),
            Type::BoundMethod(_) => KnownClass::MethodType.to_class_literal(db),
            Type::MethodWrapper(_) => KnownClass::MethodWrapperType.to_class_literal(db),
            Type::WrapperDescriptor(_) => KnownClass::WrapperDescriptorType.to_class_literal(db),
            Type::Callable(_) => KnownClass::Type.to_instance(db),
            Type::ModuleLiteral(_) => KnownClass::ModuleType.to_class_literal(db),
            Type::Tuple(_) => KnownClass::Tuple.to_class_literal(db),

            Type::TypeVar(typevar) => match typevar.bound_or_constraints(db) {
                None => KnownClass::Object.to_class_literal(db),
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound.to_meta_type(db),
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    // TODO: If we add a proper `OneOf` connector, we should use that here instead
                    // of union. (Using a union here doesn't break anything, but it is imprecise.)
                    constraints.map(db, |constraint| constraint.to_meta_type(db))
                }
            },

            Type::ClassLiteral(ClassLiteralType { class }) => class.metaclass(db),
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                ClassBase::Dynamic(_) => *self,
                ClassBase::Class(class) => SubclassOfType::from(
                    db,
                    ClassBase::try_from_type(db, class.metaclass(db))
                        .unwrap_or(ClassBase::unknown()),
                ),
            },

            Type::StringLiteral(_) | Type::LiteralString => KnownClass::Str.to_class_literal(db),
            Type::Dynamic(dynamic) => SubclassOfType::from(db, ClassBase::Dynamic(*dynamic)),
            // TODO intersections
            Type::Intersection(_) => SubclassOfType::from(
                db,
                ClassBase::try_from_type(db, todo_type!("Intersection meta-type"))
                    .expect("Type::Todo should be a valid ClassBase"),
            ),
            Type::AlwaysTruthy | Type::AlwaysFalsy => KnownClass::Type.to_instance(db),
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
            Type::KnownInstance(known_instance) => {
                Type::string_literal(db, known_instance.repr(db))
            }
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
            Type::KnownInstance(known_instance) => {
                Type::string_literal(db, known_instance.repr(db))
            }
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
                Some(TypeDefinition::Class(class_literal.class().definition(db)))
            }
            Self::Instance(instance) => {
                Some(TypeDefinition::Class(instance.class().definition(db)))
            }
            Self::KnownInstance(instance) => match instance {
                KnownInstanceType::TypeVar(var) => {
                    Some(TypeDefinition::TypeVar(var.definition(db)))
                }
                KnownInstanceType::TypeAliasType(type_alias) => {
                    Some(TypeDefinition::TypeAlias(type_alias.definition(db)))
                }
                _ => None,
            },

            Self::SubclassOf(subclass_of_type) => match subclass_of_type.subclass_of() {
                ClassBase::Class(class) => Some(TypeDefinition::Class(class.definition(db))),
                ClassBase::Dynamic(_) => None,
            },

            Self::StringLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::LiteralString
            | Self::IntLiteral(_)
            | Self::BytesLiteral(_)
            | Self::SliceLiteral(_)
            | Self::MethodWrapper(_)
            | Self::WrapperDescriptor(_)
            | Self::PropertyInstance(_)
            | Self::Tuple(_) => self.to_meta_type(db).definition(db),

            Self::TypeVar(var) => Some(TypeDefinition::TypeVar(var.definition(db))),

            Self::Union(_) | Self::Intersection(_) => None,

            // These types have no definition
            Self::Dynamic(_)
            | Self::Never
            | Self::Callable(_)
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy => None,
        }
    }
}

impl<'db> From<&Type<'db>> for Type<'db> {
    fn from(value: &Type<'db>) -> Self {
        *value
    }
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub enum DynamicType {
    // An explicitly annotated `typing.Any`
    Any,
    // An unannotated value, or a dynamic type resulting from an error
    Unknown,
    /// Temporary type for symbols that can't be inferred yet because of missing implementations.
    ///
    /// This variant should eventually be removed once red-knot is spec-compliant.
    ///
    /// General rule: `Todo` should only propagate when the presence of the input `Todo` caused the
    /// output to be unknown. An output should only be `Todo` if fixing all `Todo` inputs to be not
    /// `Todo` would change the output type.
    ///
    /// This variant should be created with the `todo_type!` macro.
    Todo(TodoType),
    /// Temporary type until we support protocols. We use a separate variant (instead of `Todo(…)`)
    /// in order to be able to match on them explicitly.
    TodoProtocol,
}

impl std::fmt::Display for DynamicType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DynamicType::Any => f.write_str("Any"),
            DynamicType::Unknown => f.write_str("Unknown"),
            // `DynamicType::Todo`'s display should be explicit that is not a valid display of
            // any other type
            DynamicType::Todo(todo) => write!(f, "@Todo{todo}"),
            DynamicType::TodoProtocol => f.write_str(if cfg!(debug_assertions) {
                "@Todo(protocol)"
            } else {
                "@Todo"
            }),
        }
    }
}

bitflags! {
    /// Type qualifiers that appear in an annotation expression.
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
    pub(crate) struct TypeQualifiers: u8 {
        /// `typing.ClassVar`
        const CLASS_VAR = 1 << 0;
        /// `typing.Final`
        const FINAL     = 1 << 1;
    }
}

/// When inferring the type of an annotation expression, we can also encounter type qualifiers
/// such as `ClassVar` or `Final`. These do not affect the inferred type itself, but rather
/// control how a particular symbol can be accessed or modified. This struct holds a type and
/// a set of type qualifiers.
///
/// Example: `Annotated[ClassVar[tuple[int]], "metadata"]` would have type `tuple[int]` and the
/// qualifier `ClassVar`.
#[derive(Clone, Debug, Copy, Eq, PartialEq, salsa::Update)]
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
    fn into_fallback_type(self, context: &InferContext, node: &ast::Expr) -> Type<'db> {
        let InvalidTypeExpressionError {
            fallback_type,
            invalid_expressions,
        } = self;
        for error in invalid_expressions {
            context.report_lint(
                &INVALID_TYPE_FORM,
                node,
                format_args!("{}", error.reason(context.db())),
            );
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
    /// The `Protocol` type is invalid in type expressions
    Protocol,
    /// Type qualifiers are always invalid in *type expressions*,
    /// but these ones are okay with 0 arguments in *annotation expressions*
    TypeQualifier(KnownInstanceType<'db>),
    /// Type qualifiers that are invalid in type expressions,
    /// and which would require exactly one argument even if they appeared in an annotation expression
    TypeQualifierRequiresOneArgument(KnownInstanceType<'db>),
    /// Some types are always invalid in type expressions
    InvalidType(Type<'db>),
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
                    InvalidTypeExpression::Protocol => f.write_str(
                        "`typing.Protocol` is not allowed in type expressions"
                    ),
                    InvalidTypeExpression::TypeQualifier(qualifier) => write!(
                        f,
                        "Type qualifier `{q}` is not allowed in type expressions (only in annotation expressions)",
                        q = qualifier.repr(self.db)
                    ),
                    InvalidTypeExpression::TypeQualifierRequiresOneArgument(qualifier) => write!(
                        f,
                        "Type qualifier `{q}` is not allowed in type expressions (only in annotation expressions, and only with exactly one argument)",
                        q = qualifier.repr(self.db)
                    ),
                    InvalidTypeExpression::InvalidType(ty) => write!(
                        f,
                        "Variable of type `{ty}` is not allowed in a type expression",
                        ty = ty.display(self.db)
                    ),
                }
            }
        }

        Display { error: self, db }
    }
}

/// Data regarding a single type variable.
///
/// This is referenced by `KnownInstanceType::TypeVar` (to represent the singleton type of the
/// runtime `typing.TypeVar` object itself), and by `Type::TypeVar` to represent the type that this
/// typevar represents as an annotation: that is, an unknown set of objects, constrained by the
/// upper-bound/constraints on this type var, defaulting to the default type of this type var when
/// not otherwise bound to a type.
///
/// This must be a tracked struct, not an interned one, because typevar equivalence is by identity,
/// not by value. Two typevars that have the same name, bound/constraints, and default, are still
/// different typevars: if used in the same scope, they may be bound to different types.
#[salsa::tracked(debug)]
pub struct TypeVarInstance<'db> {
    /// The name of this TypeVar (e.g. `T`)
    #[return_ref]
    name: ast::name::Name,

    /// The type var's definition
    pub definition: Definition<'db>,

    /// The upper bound or constraint on the type of this TypeVar
    bound_or_constraints: Option<TypeVarBoundOrConstraints<'db>>,

    /// The default type for this TypeVar
    default_ty: Option<Type<'db>>,
}

impl<'db> TypeVarInstance<'db> {
    #[allow(unused)]
    pub(crate) fn upper_bound(self, db: &'db dyn Db) -> Option<Type<'db>> {
        if let Some(TypeVarBoundOrConstraints::UpperBound(ty)) = self.bound_or_constraints(db) {
            Some(ty)
        } else {
            None
        }
    }

    #[allow(unused)]
    pub(crate) fn constraints(self, db: &'db dyn Db) -> Option<&'db [Type<'db>]> {
        if let Some(TypeVarBoundOrConstraints::Constraints(tuple)) = self.bound_or_constraints(db) {
            Some(tuple.elements(db))
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, salsa::Update)]
pub enum TypeVarBoundOrConstraints<'db> {
    UpperBound(Type<'db>),
    Constraints(UnionType<'db>),
}

/// Error returned if a type is not (or may not be) a context manager.
#[derive(Debug)]
enum ContextManagerError<'db> {
    Enter(CallDunderError<'db>),
    Exit {
        enter_return_type: Type<'db>,
        exit_error: CallDunderError<'db>,
    },
    EnterAndExit {
        enter_error: CallDunderError<'db>,
        exit_error: CallDunderError<'db>,
    },
}

impl<'db> ContextManagerError<'db> {
    fn fallback_enter_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.enter_type(db).unwrap_or(Type::unknown())
    }

    /// Returns the `__enter__` return type if it is known,
    /// or `None` if the type never has a callable `__enter__` attribute
    fn enter_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::Exit {
                enter_return_type,
                exit_error: _,
            } => Some(*enter_return_type),
            Self::Enter(enter_error)
            | Self::EnterAndExit {
                enter_error,
                exit_error: _,
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
        context: &InferContext<'db>,
        context_expression_type: Type<'db>,
        context_expression_node: ast::AnyNodeRef,
    ) {
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
            } => format_call_dunder_error(exit_error, "__exit__"),
            Self::Enter(enter_error) => format_call_dunder_error(enter_error, "__enter__"),
            Self::EnterAndExit {
                enter_error,
                exit_error,
            } => format_call_dunder_errors(enter_error, "__enter__", exit_error, "__exit__"),
        };

        context.report_lint(
            &INVALID_CONTEXT_MANAGER,
            context_expression_node,
            format_args!(
                "Object of type `{context_expression}` cannot be used with `with` because {formatted_errors}",
                context_expression = context_expression_type.display(db)
            ),
        );
    }
}

/// Error returned if a type is not (or may not be) iterable.
#[derive(Debug)]
enum IterationError<'db> {
    /// The object being iterated over has a bound `__iter__` method,
    /// but calling it with the expected arguments results in an error.
    IterCallError(CallErrorKind, Box<Bindings<'db>>),

    /// The object being iterated over has a bound `__iter__` method that can be called
    /// with the expected types, but it returns an object that is not a valid iterator.
    IterReturnsInvalidIterator {
        /// The type of the object returned by the `__iter__` method.
        iterator: Type<'db>,
        /// The error we encountered when we tried to call `__next__` on the type
        /// returned by `__iter__`
        dunder_next_error: CallDunderError<'db>,
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
}

impl<'db> IterationError<'db> {
    fn fallback_element_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.element_type(db).unwrap_or(Type::unknown())
    }

    /// Returns the element type if it is known, or `None` if the type is never iterable.
    fn element_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::IterReturnsInvalidIterator {
                dunder_next_error, ..
            } => dunder_next_error.return_type(db),

            Self::IterCallError(_, dunder_iter_bindings) => dunder_iter_bindings
                .return_type(db)
                .try_call_dunder(db, "__next__", CallArgumentTypes::none())
                .map(|dunder_next_outcome| Some(dunder_next_outcome.return_type(db)))
                .unwrap_or_else(|dunder_next_call_error| dunder_next_call_error.return_type(db)),

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
        }
    }

    /// Reports the diagnostic for this error.
    fn report_diagnostic(
        &self,
        context: &InferContext<'db>,
        iterable_type: Type<'db>,
        iterable_node: ast::AnyNodeRef,
    ) {
        let db = context.db();

        let report_not_iterable = |arguments: std::fmt::Arguments| {
            context.report_lint(&NOT_ITERABLE, iterable_node, arguments);
        };

        // TODO: for all of these error variants, the "explanation" for the diagnostic
        // (everything after the "because") should really be presented as a "help:", "note",
        // or similar, rather than as part of the same sentence as the error message.
        match self {
            Self::IterCallError(CallErrorKind::NotCallable, bindings) => report_not_iterable(format_args!(
                "Object of type `{iterable_type}` is not iterable \
                    because its `__iter__` attribute has type `{dunder_iter_type}`, \
                    which is not callable",
                iterable_type = iterable_type.display(db),
                dunder_iter_type = bindings.callable_type().display(db),
            )),
            Self::IterCallError(CallErrorKind::PossiblyNotCallable, bindings) if bindings.is_single() => {
                report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because its `__iter__` attribute (with type `{dunder_iter_type}`) \
                        may not be callable",
                    iterable_type = iterable_type.display(db),
                    dunder_iter_type = bindings.callable_type().display(db),
                ));
            }
            Self::IterCallError(CallErrorKind::PossiblyNotCallable, bindings) => {
                report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because its `__iter__` attribute (with type `{dunder_iter_type}`) \
                        may not be callable",
                    iterable_type = iterable_type.display(db),
                    dunder_iter_type = bindings.callable_type().display(db),
                ));
            }
            Self::IterCallError(CallErrorKind::BindingError, bindings) if bindings.is_single() => report_not_iterable(format_args!(
                "Object of type `{iterable_type}` is not iterable \
                    because its `__iter__` method has an invalid signature \
                    (expected `def __iter__(self): ...`)",
                iterable_type = iterable_type.display(db),
            )),
            Self::IterCallError(CallErrorKind::BindingError, bindings) => report_not_iterable(format_args!(
                "Object of type `{iterable_type}` may not be iterable \
                    because its `__iter__` method (with type `{dunder_iter_type}`) \
                    may have an invalid signature (expected `def __iter__(self): ...`)",
                iterable_type = iterable_type.display(db),
                dunder_iter_type = bindings.callable_type().display(db),
            )),

            Self::IterReturnsInvalidIterator {
                iterator,
                dunder_next_error
            } => match dunder_next_error {
                CallDunderError::MethodNotAvailable => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` is not iterable \
                        because its `__iter__` method returns an object of type `{iterator_type}`, \
                        which has no `__next__` method",
                    iterable_type = iterable_type.display(db),
                    iterator_type = iterator.display(db),
                )),
                CallDunderError::PossiblyUnbound(_) => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because its `__iter__` method returns an object of type `{iterator_type}`, \
                        which may not have a `__next__` method",
                    iterable_type = iterable_type.display(db),
                    iterator_type = iterator.display(db),
                )),
                CallDunderError::CallError(CallErrorKind::NotCallable, _) => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` is not iterable \
                        because its `__iter__` method returns an object of type `{iterator_type}`, \
                        which has a `__next__` attribute that is not callable",
                    iterable_type = iterable_type.display(db),
                    iterator_type = iterator.display(db),
                )),
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, _) => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because its `__iter__` method returns an object of type `{iterator_type}`, \
                        which has a `__next__` attribute that may not be callable",
                    iterable_type = iterable_type.display(db),
                    iterator_type = iterator.display(db),
                )),
                CallDunderError::CallError(CallErrorKind::BindingError, bindings) if bindings.is_single() => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` is not iterable \
                        because its `__iter__` method returns an object of type `{iterator_type}`, \
                        which has an invalid `__next__` method (expected `def __next__(self): ...`)",
                    iterable_type = iterable_type.display(db),
                    iterator_type = iterator.display(db),
                )),
                CallDunderError::CallError(CallErrorKind::BindingError, _) => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because its `__iter__` method returns an object of type `{iterator_type}`, \
                        which may have an invalid `__next__` method (expected `def __next__(self): ...`)",
                    iterable_type = iterable_type.display(db),
                    iterator_type = iterator.display(db),
                )),
            }

            Self::PossiblyUnboundIterAndGetitemError {
                dunder_getitem_error, ..
            } => match dunder_getitem_error {
                CallDunderError::MethodNotAvailable => report_not_iterable(format_args!(
                    "Object of type `{}` may not be iterable \
                        because it may not have an `__iter__` method \
                        and it doesn't have a `__getitem__` method",
                    iterable_type.display(db)
                )),
                CallDunderError::PossiblyUnbound(_) => report_not_iterable(format_args!(
                    "Object of type `{}` may not be iterable \
                        because it may not have an `__iter__` method or a `__getitem__` method",
                    iterable_type.display(db)
                )),
                CallDunderError::CallError(CallErrorKind::NotCallable, bindings) => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because it may not have an `__iter__` method \
                        and its `__getitem__` attribute has type `{dunder_getitem_type}`, \
                        which is not callable",
                    iterable_type = iterable_type.display(db),
                    dunder_getitem_type = bindings.callable_type().display(db),
                )),
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings) if bindings.is_single() => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because it may not have an `__iter__` method \
                        and its `__getitem__` attribute may not be callable",
                    iterable_type = iterable_type.display(db),
                )),
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings) => {
                    report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because it may not have an `__iter__` method \
                            and its `__getitem__` attribute (with type `{dunder_getitem_type}`) \
                            may not be callable",
                        iterable_type = iterable_type.display(db),
                        dunder_getitem_type = bindings.callable_type().display(db),
                    ));
                }
                CallDunderError::CallError(CallErrorKind::BindingError, bindings) if bindings.is_single() => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because it may not have an `__iter__` method \
                        and its `__getitem__` method has an incorrect signature \
                        for the old-style iteration protocol \
                        (expected a signature at least as permissive as \
                        `def __getitem__(self, key: int): ...`)",
                    iterable_type = iterable_type.display(db),
                )),
                CallDunderError::CallError(CallErrorKind::BindingError, bindings) => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because it may not have an `__iter__` method \
                        and its `__getitem__` method (with type `{dunder_getitem_type}`) \
                        may have an incorrect signature for the old-style iteration protocol \
                        (expected a signature at least as permissive as \
                        `def __getitem__(self, key: int): ...`)",
                    iterable_type = iterable_type.display(db),
                    dunder_getitem_type = bindings.callable_type().display(db),
                )),
            }

            Self::UnboundIterAndGetitemError { dunder_getitem_error } => match dunder_getitem_error {
                CallDunderError::MethodNotAvailable => report_not_iterable(format_args!(
                    "Object of type `{}` is not iterable because it doesn't have \
                        an `__iter__` method or a `__getitem__` method",
                    iterable_type.display(db)
                )),
                CallDunderError::PossiblyUnbound(_) => report_not_iterable(format_args!(
                    "Object of type `{}` may not be iterable because it has no `__iter__` method \
                        and it may not have a `__getitem__` method",
                    iterable_type.display(db)
                )),
                CallDunderError::CallError(CallErrorKind::NotCallable, bindings) => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` is not iterable \
                        because it has no `__iter__` method and \
                        its `__getitem__` attribute has type `{dunder_getitem_type}`, \
                        which is not callable",
                    iterable_type = iterable_type.display(db),
                    dunder_getitem_type = bindings.callable_type().display(db),
                )),
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings) if bindings.is_single() => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because it has no `__iter__` method and its `__getitem__` attribute \
                        may not be callable",
                    iterable_type = iterable_type.display(db),
                )),
                CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, bindings) => {
                    report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because it has no `__iter__` method and its `__getitem__` attribute \
                            (with type `{dunder_getitem_type}`) may not be callable",
                        iterable_type = iterable_type.display(db),
                        dunder_getitem_type = bindings.callable_type().display(db),
                    ));
                }
                CallDunderError::CallError(CallErrorKind::BindingError, bindings) if bindings.is_single() => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` is not iterable \
                        because it has no `__iter__` method and \
                        its `__getitem__` method has an incorrect signature \
                        for the old-style iteration protocol \
                        (expected a signature at least as permissive as \
                        `def __getitem__(self, key: int): ...`)",
                    iterable_type = iterable_type.display(db),
                )),
                CallDunderError::CallError(CallErrorKind::BindingError, bindings) => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because it has no `__iter__` method and \
                        its `__getitem__` method (with type `{dunder_getitem_type}`) \
                        may have an incorrect signature for the old-style iteration protocol \
                        (expected a signature at least as permissive as \
                        `def __getitem__(self, key: int): ...`)",
                    iterable_type = iterable_type.display(db),
                    dunder_getitem_type = bindings.callable_type().display(db),
                )),
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
        match self {
            Self::IncorrectArguments {
                not_boolable_type, ..
            } => {
                context.report_lint(
                    &UNSUPPORTED_BOOL_CONVERSION,
                    condition,
                    format_args!(
                        "Boolean conversion is unsupported for type `{}`; it incorrectly implements `__bool__`",
                        not_boolable_type.display(context.db())
                    ),
                );
            }
            Self::IncorrectReturnType {
                not_boolable_type,
                return_type,
            } => {
                context.report_lint(
                    &UNSUPPORTED_BOOL_CONVERSION,
                    condition,
                    format_args!(
                        "Boolean conversion is unsupported for type `{not_boolable}`; the return type of its bool method (`{return_type}`) isn't assignable to `bool",
                        not_boolable = not_boolable_type.display(context.db()),
                        return_type = return_type.display(context.db())
                    ),
                );
            }
            Self::NotCallable { not_boolable_type } => {
                context.report_lint(
                    &UNSUPPORTED_BOOL_CONVERSION,
                    condition,
                    format_args!(
                        "Boolean conversion is unsupported for type `{}`; its `__bool__` method isn't callable",
                        not_boolable_type.display(context.db())
                    ),
                );
            }
            Self::Union { union, .. } => {
                let first_error = union
                    .elements(context.db())
                    .iter()
                    .find_map(|element| element.try_bool(context.db()).err())
                    .unwrap();

                context.report_lint(
                        &UNSUPPORTED_BOOL_CONVERSION,
                        condition,
                        format_args!(
                            "Boolean conversion is unsupported for union `{}` because `{}` doesn't implement `__bool__` correctly",
                            Type::Union(*union).display(context.db()),
                            first_error.not_boolable_type().display(context.db()),
                        ),
                    );
            }

            Self::Other { not_boolable_type } => {
                context.report_lint(
                    &UNSUPPORTED_BOOL_CONVERSION,
                    condition,
                    format_args!(
                        "Boolean conversion is unsupported for type `{}`; it incorrectly implements `__bool__`",
                        not_boolable_type.display(context.db())
                    ),
                );
            }
        }
    }
}

/// Error returned if a class instantiation call failed
#[derive(Debug)]
enum CreateInstanceCallError<'db> {
    Init(Type<'db>, CallDunderError<'db>),
    New(Type<'db>, CallDunderError<'db>),
    NewAndInit(Type<'db>, CallDunderError<'db>, CallDunderError<'db>),
}

impl<'db> CreateInstanceCallError<'db> {
    fn return_type(&self) -> Type<'db> {
        match self {
            Self::Init(ty, _) => *ty,
            Self::New(ty, _) => *ty,
            Self::NewAndInit(ty, _, _) => *ty,
        }
    }

    fn report_diagnostic(
        &self,
        context: &InferContext<'db>,
        context_expression_type: Type<'db>,
        context_expression_node: ast::AnyNodeRef,
    ) {
        let report_init_error = |call_dunder_error: &CallDunderError<'db>| match call_dunder_error {
            CallDunderError::MethodNotAvailable => {
                // If we are using vendored typeshed, it should be impossible to have missing
                // or unbound `__init__` method on a class, as all classes have `object` in MRO.
                // Thus the following may only trigger if a custom typeshed is used.
                context.report_lint(
                    &CALL_POSSIBLY_UNBOUND_METHOD,
                    context_expression_node,
                    format_args!(
                        "`__init__` method is missing on type `{}`. Make sure your `object` in typeshed has it's definition",
                        context_expression_type.display(context.db()),
                    ),
                );
            }
            CallDunderError::PossiblyUnbound(bindings) => {
                context.report_lint(
                    &CALL_POSSIBLY_UNBOUND_METHOD,
                    context_expression_node,
                    format_args!(
                        "Method `__init__` on type `{}` is possibly unbound",
                        context_expression_type.display(context.db()),
                    ),
                );

                bindings.report_diagnostics(context, context_expression_node);
            }
            CallDunderError::CallError(_, bindings) => {
                bindings.report_diagnostics(context, context_expression_node);
            }
        };

        let report_new_error = |call_dunder_error: &CallDunderError<'db>| match call_dunder_error {
            CallDunderError::MethodNotAvailable => {
                // We are explicitly checking for `__new__` before attempting to call it,
                // so this should never happen.
                unreachable!("`__new__` method may not be called if missing");
            }
            CallDunderError::PossiblyUnbound(bindings) => {
                context.report_lint(
                    &CALL_POSSIBLY_UNBOUND_METHOD,
                    context_expression_node,
                    format_args!(
                        "Method `__new__` on type `{}` is possibly unbound",
                        context_expression_type.display(context.db()),
                    ),
                );

                bindings.report_diagnostics(context, context_expression_node);
            }
            CallDunderError::CallError(_, bindings) => {
                bindings.report_diagnostics(context, context_expression_node);
            }
        };

        match self {
            Self::Init(_, call_dunder_error) => {
                report_init_error(call_dunder_error);
            }
            Self::New(_, call_dunder_error) => {
                report_new_error(call_dunder_error);
            }
            Self::NewAndInit(_, new_call_dunder_error, init_call_dunder_error) => {
                report_new_error(new_call_dunder_error);
                report_init_error(init_call_dunder_error);
            }
        }
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
        if condition {
            self.negate()
        } else {
            self
        }
    }

    fn into_type(self, db: &dyn Db) -> Type {
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

bitflags! {
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Hash)]
    pub struct FunctionDecorators: u8 {
        /// `@classmethod`
        const CLASSMETHOD = 1 << 0;
        /// `@no_type_check`
        const NO_TYPE_CHECK = 1 << 1;
        /// `@overload`
        const OVERLOAD = 1 << 2;
    }
}

#[salsa::interned(debug)]
pub struct FunctionType<'db> {
    /// name of the function at definition
    #[return_ref]
    pub name: ast::name::Name,

    /// Is this a function that we special-case somehow? If so, which one?
    known: Option<KnownFunction>,

    body_scope: ScopeId<'db>,

    /// A set of special decorators that were applied to this function
    decorators: FunctionDecorators,
}

#[salsa::tracked]
impl<'db> FunctionType<'db> {
    pub(crate) fn has_known_decorator(self, db: &dyn Db, decorator: FunctionDecorators) -> bool {
        self.decorators(db).contains(decorator)
    }

    /// Convert the `FunctionType` into a [`Type::Callable`].
    ///
    /// This powers the `CallableTypeOf` special form from the `knot_extensions` module.
    pub(crate) fn into_callable_type(self, db: &'db dyn Db) -> Type<'db> {
        Type::Callable(CallableType::new(db, self.signature(db).clone()))
    }

    /// Returns the [`FileRange`] of the function's name.
    pub fn focus_range(self, db: &dyn Db) -> FileRange {
        FileRange::new(
            self.body_scope(db).file(db),
            self.body_scope(db).node(db).expect_function().name.range,
        )
    }

    pub fn full_range(self, db: &dyn Db) -> FileRange {
        FileRange::new(
            self.body_scope(db).file(db),
            self.body_scope(db).node(db).expect_function().range,
        )
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let body_scope = self.body_scope(db);
        let index = semantic_index(db, body_scope.file(db));
        index.expect_single_definition(body_scope.node(db).expect_function())
    }

    /// Typed externally-visible signature for this function.
    ///
    /// This is the signature as seen by external callers, possibly modified by decorators and/or
    /// overloaded.
    ///
    /// ## Why is this a salsa query?
    ///
    /// This is a salsa query to short-circuit the invalidation
    /// when the function's AST node changes.
    ///
    /// Were this not a salsa query, then the calling query
    /// would depend on the function's AST and rerun for every change in that file.
    #[salsa::tracked(return_ref)]
    pub(crate) fn signature(self, db: &'db dyn Db) -> Signature<'db> {
        let internal_signature = self.internal_signature(db);

        if self.has_known_decorator(db, FunctionDecorators::OVERLOAD) {
            Signature::todo("return type of overloaded function")
        } else {
            internal_signature
        }
    }

    /// Typed internally-visible signature for this function.
    ///
    /// This represents the annotations on the function itself, unmodified by decorators and
    /// overloads.
    ///
    /// These are the parameter and return types that should be used for type checking the body of
    /// the function.
    ///
    /// Don't call this when checking any other file; only when type-checking the function body
    /// scope.
    fn internal_signature(self, db: &'db dyn Db) -> Signature<'db> {
        let scope = self.body_scope(db);
        let function_stmt_node = scope.node(db).expect_function();
        let definition = self.definition(db);
        Signature::from_function(db, definition, function_stmt_node)
    }

    pub(crate) fn is_known(self, db: &'db dyn Db, known_function: KnownFunction) -> bool {
        self.known(db) == Some(known_function)
    }
}

/// Non-exhaustive enumeration of known functions (e.g. `builtins.reveal_type`, ...) that might
/// have special behavior.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, strum_macros::EnumString)]
#[strum(serialize_all = "snake_case")]
#[cfg_attr(test, derive(strum_macros::EnumIter, strum_macros::IntoStaticStr))]
pub enum KnownFunction {
    /// `builtins.isinstance`
    #[strum(serialize = "isinstance")]
    IsInstance,
    /// `builtins.issubclass`
    #[strum(serialize = "issubclass")]
    IsSubclass,
    /// `builtins.reveal_type`, `typing.reveal_type` or `typing_extensions.reveal_type`
    RevealType,
    /// `builtins.len`
    Len,
    /// `builtins.repr`
    Repr,
    /// `typing(_extensions).final`
    Final,

    /// [`typing(_extensions).no_type_check`](https://typing.readthedocs.io/en/latest/spec/directives.html#no-type-check)
    NoTypeCheck,

    /// `typing(_extensions).assert_type`
    AssertType,
    /// `typing(_extensions).cast`
    Cast,
    /// `typing(_extensions).overload`
    Overload,

    /// `abc.abstractmethod`
    #[strum(serialize = "abstractmethod")]
    AbstractMethod,

    /// `inspect.getattr_static`
    GetattrStatic,

    /// `knot_extensions.static_assert`
    StaticAssert,
    /// `knot_extensions.is_equivalent_to`
    IsEquivalentTo,
    /// `knot_extensions.is_subtype_of`
    IsSubtypeOf,
    /// `knot_extensions.is_assignable_to`
    IsAssignableTo,
    /// `knot_extensions.is_disjoint_from`
    IsDisjointFrom,
    /// `knot_extensions.is_gradual_equivalent_to`
    IsGradualEquivalentTo,
    /// `knot_extensions.is_fully_static`
    IsFullyStatic,
    /// `knot_extensions.is_singleton`
    IsSingleton,
    /// `knot_extensions.is_single_valued`
    IsSingleValued,
}

impl KnownFunction {
    pub fn into_constraint_function(self) -> Option<KnownConstraintFunction> {
        match self {
            Self::IsInstance => Some(KnownConstraintFunction::IsInstance),
            Self::IsSubclass => Some(KnownConstraintFunction::IsSubclass),
            _ => None,
        }
    }

    fn try_from_definition_and_name<'db>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        name: &str,
    ) -> Option<Self> {
        let candidate = Self::from_str(name).ok()?;
        candidate
            .check_module(file_to_module(db, definition.file(db))?.known()?)
            .then_some(candidate)
    }

    /// Return `true` if `self` is defined in `module` at runtime.
    const fn check_module(self, module: KnownModule) -> bool {
        match self {
            Self::IsInstance | Self::IsSubclass | Self::Len | Self::Repr => module.is_builtins(),
            Self::AssertType
            | Self::Cast
            | Self::Overload
            | Self::RevealType
            | Self::Final
            | Self::NoTypeCheck => {
                matches!(module, KnownModule::Typing | KnownModule::TypingExtensions)
            }
            Self::AbstractMethod => {
                matches!(module, KnownModule::Abc)
            }
            Self::GetattrStatic => module.is_inspect(),
            Self::IsAssignableTo
            | Self::IsDisjointFrom
            | Self::IsEquivalentTo
            | Self::IsGradualEquivalentTo
            | Self::IsFullyStatic
            | Self::IsSingleValued
            | Self::IsSingleton
            | Self::IsSubtypeOf
            | Self::StaticAssert => module.is_knot_extensions(),
        }
    }
}

/// This type represents bound method objects that are created when a method is accessed
/// on an instance of a class. For example, the expression `Path("a.txt").touch` creates
/// a bound method object that represents the `Path.touch` method which is bound to the
/// instance `Path("a.txt")`.
#[salsa::tracked(debug)]
pub struct BoundMethodType<'db> {
    /// The function that is being bound. Corresponds to the `__func__` attribute on a
    /// bound method object
    pub(crate) function: FunctionType<'db>,
    /// The instance on which this method has been called. Corresponds to the `__self__`
    /// attribute on a bound method object
    self_instance: Type<'db>,
}

impl<'db> BoundMethodType<'db> {
    pub(crate) fn into_callable_type(self, db: &'db dyn Db) -> Type<'db> {
        Type::Callable(CallableType::new(
            db,
            self.function(db).signature(db).bind_self(),
        ))
    }
}

/// This type represents the set of all callable objects with a certain signature.
/// It can be written in type expressions using `typing.Callable`.
/// `lambda` expressions are inferred directly as `CallableType`s; all function-literal types
/// are subtypes of a `CallableType`.
#[salsa::interned(debug)]
pub struct CallableType<'db> {
    #[return_ref]
    signature: Signature<'db>,
}

impl<'db> CallableType<'db> {
    /// Create a callable type which accepts any parameters and returns an `Unknown` type.
    pub(crate) fn unknown(db: &'db dyn Db) -> Self {
        CallableType::new(
            db,
            Signature::new(Parameters::unknown(), Some(Type::unknown())),
        )
    }

    /// Return a "normalized" version of this `Callable` type.
    ///
    /// See [`Type::normalized`] for more details.
    fn normalized(self, db: &'db dyn Db) -> Self {
        let signature = self.signature(db);
        let parameters = signature
            .parameters()
            .iter()
            .map(|param| param.normalized(db))
            .collect();
        let return_ty = signature
            .return_ty
            .map(|return_ty| return_ty.normalized(db));
        CallableType::new(db, Signature::new(parameters, return_ty))
    }

    /// Returns `true` if this is a fully static callable type.
    ///
    /// A callable type is fully static if all of its parameters and return type are fully static
    /// and if it does not use gradual form (`...`) for its parameters.
    pub(crate) fn is_fully_static(self, db: &'db dyn Db) -> bool {
        let signature = self.signature(db);

        if signature.parameters().is_gradual() {
            return false;
        }

        if signature.parameters().iter().any(|parameter| {
            parameter
                .annotated_type()
                .is_none_or(|annotated_type| !annotated_type.is_fully_static(db))
        }) {
            return false;
        }

        signature
            .return_ty
            .is_some_and(|return_type| return_type.is_fully_static(db))
    }

    /// Return `true` if `self` has exactly the same set of possible static materializations as
    /// `other` (if `self` represents the same set of possible sets of possible runtime objects as
    /// `other`).
    pub(crate) fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.is_equivalent_to_impl(db, other, |self_type, other_type| {
            self_type
                .unwrap_or(Type::unknown())
                .is_gradual_equivalent_to(db, other_type.unwrap_or(Type::unknown()))
        })
    }

    /// Return `true` if `self` represents the exact same set of possible runtime objects as `other`.
    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.is_equivalent_to_impl(db, other, |self_type, other_type| {
            match (self_type, other_type) {
                (Some(self_type), Some(other_type)) => self_type.is_equivalent_to(db, other_type),
                // We need the catch-all case here because it's not guaranteed that this is a fully
                // static type.
                _ => false,
            }
        })
    }

    /// Implementation for the [`is_equivalent_to`] and [`is_gradual_equivalent_to`] for callable
    /// types.
    ///
    /// [`is_equivalent_to`]: Self::is_equivalent_to
    /// [`is_gradual_equivalent_to`]: Self::is_gradual_equivalent_to
    fn is_equivalent_to_impl<F>(self, db: &'db dyn Db, other: Self, check_types: F) -> bool
    where
        F: Fn(Option<Type<'db>>, Option<Type<'db>>) -> bool,
    {
        let self_signature = self.signature(db);
        let other_signature = other.signature(db);

        // N.B. We don't need to explicitly check for the use of gradual form (`...`) in the
        // parameters because it is internally represented by adding `*Any` and `**Any` to the
        // parameter list.
        let self_parameters = self_signature.parameters();
        let other_parameters = other_signature.parameters();

        if self_parameters.len() != other_parameters.len() {
            return false;
        }

        if !check_types(self_signature.return_ty, other_signature.return_ty) {
            return false;
        }

        for (self_parameter, other_parameter) in self_parameters.iter().zip(other_parameters) {
            match (self_parameter.kind(), other_parameter.kind()) {
                (
                    ParameterKind::PositionalOnly {
                        default_type: self_default,
                        ..
                    },
                    ParameterKind::PositionalOnly {
                        default_type: other_default,
                        ..
                    },
                ) if self_default.is_some() == other_default.is_some() => {}

                (
                    ParameterKind::PositionalOrKeyword {
                        name: self_name,
                        default_type: self_default,
                    },
                    ParameterKind::PositionalOrKeyword {
                        name: other_name,
                        default_type: other_default,
                    },
                ) if self_default.is_some() == other_default.is_some()
                    && self_name == other_name => {}

                (ParameterKind::Variadic { .. }, ParameterKind::Variadic { .. }) => {}

                (
                    ParameterKind::KeywordOnly {
                        name: self_name,
                        default_type: self_default,
                    },
                    ParameterKind::KeywordOnly {
                        name: other_name,
                        default_type: other_default,
                    },
                ) if self_default.is_some() == other_default.is_some()
                    && self_name == other_name => {}

                (ParameterKind::KeywordVariadic { .. }, ParameterKind::KeywordVariadic { .. }) => {}

                _ => return false,
            }

            if !check_types(
                self_parameter.annotated_type(),
                other_parameter.annotated_type(),
            ) {
                return false;
            }
        }

        true
    }

    /// Return `true` if `self` is assignable to `other`.
    pub(crate) fn is_assignable_to(self, db: &'db dyn Db, other: Self) -> bool {
        self.is_assignable_to_impl(db, other, |type1, type2| {
            // In the context of a callable type, the `None` variant represents an `Unknown` type.
            type1
                .unwrap_or(Type::unknown())
                .is_assignable_to(db, type2.unwrap_or(Type::unknown()))
        })
    }

    /// Return `true` if `self` is a subtype of `other`.
    ///
    /// # Panics
    ///
    /// Panics if `self` or `other` is not a fully static type.
    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, other: Self) -> bool {
        self.is_assignable_to_impl(db, other, |type1, type2| {
            // SAFETY: Subtype relation is only checked for fully static types.
            type1.unwrap().is_subtype_of(db, type2.unwrap())
        })
    }

    /// Implementation for the [`is_assignable_to`] and [`is_subtype_of`] for callable types.
    ///
    /// [`is_assignable_to`]: Self::is_assignable_to
    /// [`is_subtype_of`]: Self::is_subtype_of
    fn is_assignable_to_impl<F>(self, db: &'db dyn Db, other: Self, check_types: F) -> bool
    where
        F: Fn(Option<Type<'db>>, Option<Type<'db>>) -> bool,
    {
        /// A helper struct to zip two slices of parameters together that provides control over the
        /// two iterators individually. It also keeps track of the current parameter in each
        /// iterator.
        struct ParametersZip<'a, 'db> {
            current_self: Option<&'a Parameter<'db>>,
            current_other: Option<&'a Parameter<'db>>,
            iter_self: Iter<'a, Parameter<'db>>,
            iter_other: Iter<'a, Parameter<'db>>,
        }

        impl<'a, 'db> ParametersZip<'a, 'db> {
            /// Move to the next parameter in both the `self` and `other` parameter iterators,
            /// [`None`] if both iterators are exhausted.
            fn next(&mut self) -> Option<EitherOrBoth<&'a Parameter<'db>, &'a Parameter<'db>>> {
                match (self.next_self(), self.next_other()) {
                    (Some(self_param), Some(other_param)) => {
                        Some(EitherOrBoth::Both(self_param, other_param))
                    }
                    (Some(self_param), None) => Some(EitherOrBoth::Left(self_param)),
                    (None, Some(other_param)) => Some(EitherOrBoth::Right(other_param)),
                    (None, None) => None,
                }
            }

            /// Move to the next parameter in the `self` parameter iterator, [`None`] if the
            /// iterator is exhausted.
            fn next_self(&mut self) -> Option<&'a Parameter<'db>> {
                self.current_self = self.iter_self.next();
                self.current_self
            }

            /// Move to the next parameter in the `other` parameter iterator, [`None`] if the
            /// iterator is exhausted.
            fn next_other(&mut self) -> Option<&'a Parameter<'db>> {
                self.current_other = self.iter_other.next();
                self.current_other
            }

            /// Peek at the next parameter in the `other` parameter iterator without consuming it.
            fn peek_other(&mut self) -> Option<&'a Parameter<'db>> {
                self.iter_other.clone().next()
            }

            /// Consumes the `ParametersZip` and returns a two-element tuple containing the
            /// remaining parameters in the `self` and `other` iterators respectively.
            ///
            /// The returned iterators starts with the current parameter, if any, followed by the
            /// remaining parameters in the respective iterators.
            fn into_remaining(
                self,
            ) -> (
                impl Iterator<Item = &'a Parameter<'db>>,
                impl Iterator<Item = &'a Parameter<'db>>,
            ) {
                (
                    self.current_self.into_iter().chain(self.iter_self),
                    self.current_other.into_iter().chain(self.iter_other),
                )
            }
        }

        let self_signature = self.signature(db);
        let other_signature = other.signature(db);

        // Return types are covariant.
        if !check_types(self_signature.return_ty, other_signature.return_ty) {
            return false;
        }

        if self_signature.parameters().is_gradual() || other_signature.parameters().is_gradual() {
            // If either of the parameter lists contains a gradual form (`...`), then it is
            // assignable / subtype to and from any other callable type.
            return true;
        }

        let mut parameters = ParametersZip {
            current_self: None,
            current_other: None,
            iter_self: self_signature.parameters().iter(),
            iter_other: other_signature.parameters().iter(),
        };

        // Collect all the standard parameters that have only been matched against a variadic
        // parameter which means that the keyword variant is still unmatched.
        let mut other_keywords = Vec::new();

        loop {
            let Some(next_parameter) = parameters.next() else {
                // All parameters have been checked or both the parameter lists were empty. In
                // either case, `self` is a subtype of `other`.
                return true;
            };

            match next_parameter {
                EitherOrBoth::Left(self_parameter) => match self_parameter.kind() {
                    ParameterKind::KeywordOnly { .. } | ParameterKind::KeywordVariadic { .. }
                        if !other_keywords.is_empty() =>
                    {
                        // If there are any unmatched keyword parameters in `other`, they need to
                        // be checked against the keyword-only / keyword-variadic parameters that
                        // will be done after this loop.
                        break;
                    }
                    ParameterKind::PositionalOnly { default_type, .. }
                    | ParameterKind::PositionalOrKeyword { default_type, .. }
                    | ParameterKind::KeywordOnly { default_type, .. } => {
                        // For `self <: other` to be valid, if there are no more parameters in
                        // `other`, then the non-variadic parameters in `self` must have a default
                        // value.
                        if default_type.is_none() {
                            return false;
                        }
                    }
                    ParameterKind::Variadic { .. } | ParameterKind::KeywordVariadic { .. } => {
                        // Variadic parameters don't have any restrictions in this context, so
                        // we'll just continue to the next parameter set.
                    }
                },

                EitherOrBoth::Right(_) => {
                    // If there are more parameters in `other` than in `self`, then `self` is not a
                    // subtype of `other`.
                    return false;
                }

                EitherOrBoth::Both(self_parameter, other_parameter) => {
                    match (self_parameter.kind(), other_parameter.kind()) {
                        (
                            ParameterKind::PositionalOnly {
                                default_type: self_default,
                                ..
                            }
                            | ParameterKind::PositionalOrKeyword {
                                default_type: self_default,
                                ..
                            },
                            ParameterKind::PositionalOnly {
                                default_type: other_default,
                                ..
                            },
                        ) => {
                            if self_default.is_none() && other_default.is_some() {
                                return false;
                            }
                            if !check_types(
                                other_parameter.annotated_type(),
                                self_parameter.annotated_type(),
                            ) {
                                return false;
                            }
                        }

                        (
                            ParameterKind::PositionalOrKeyword {
                                name: self_name,
                                default_type: self_default,
                            },
                            ParameterKind::PositionalOrKeyword {
                                name: other_name,
                                default_type: other_default,
                            },
                        ) => {
                            if self_name != other_name {
                                return false;
                            }
                            // The following checks are the same as positional-only parameters.
                            if self_default.is_none() && other_default.is_some() {
                                return false;
                            }
                            if !check_types(
                                other_parameter.annotated_type(),
                                self_parameter.annotated_type(),
                            ) {
                                return false;
                            }
                        }

                        (
                            ParameterKind::Variadic { .. },
                            ParameterKind::PositionalOnly { .. }
                            | ParameterKind::PositionalOrKeyword { .. },
                        ) => {
                            if !check_types(
                                other_parameter.annotated_type(),
                                self_parameter.annotated_type(),
                            ) {
                                return false;
                            }

                            if matches!(
                                other_parameter.kind(),
                                ParameterKind::PositionalOrKeyword { .. }
                            ) {
                                other_keywords.push(other_parameter);
                            }

                            // We've reached a variadic parameter in `self` which means there can
                            // be no more positional parameters after this in a valid AST. But, the
                            // current parameter in `other` is a positional-only which means there
                            // can be more positional parameters after this which could be either
                            // more positional-only parameters, standard parameters or a variadic
                            // parameter.
                            //
                            // So, any remaining positional parameters in `other` would need to be
                            // checked against the variadic parameter in `self`. This loop does
                            // that by only moving the `other` iterator forward.
                            loop {
                                let Some(other_parameter) = parameters.peek_other() else {
                                    break;
                                };
                                match other_parameter.kind() {
                                    ParameterKind::PositionalOrKeyword { .. } => {
                                        other_keywords.push(other_parameter);
                                    }
                                    ParameterKind::PositionalOnly { .. }
                                    | ParameterKind::Variadic { .. } => {}
                                    _ => {
                                        // Any other parameter kind cannot be checked against a
                                        // variadic parameter and is deferred to the next iteration.
                                        break;
                                    }
                                }
                                if !check_types(
                                    other_parameter.annotated_type(),
                                    self_parameter.annotated_type(),
                                ) {
                                    return false;
                                }
                                parameters.next_other();
                            }
                        }

                        (ParameterKind::Variadic { .. }, ParameterKind::Variadic { .. }) => {
                            if !check_types(
                                other_parameter.annotated_type(),
                                self_parameter.annotated_type(),
                            ) {
                                return false;
                            }
                        }

                        (
                            _,
                            ParameterKind::KeywordOnly { .. }
                            | ParameterKind::KeywordVariadic { .. },
                        ) => {
                            // Keyword parameters are not considered in this loop as the order of
                            // parameters is not important for them and so they are checked by
                            // doing name-based lookups.
                            break;
                        }

                        _ => return false,
                    }
                }
            }
        }

        // At this point, the remaining parameters in `other` are keyword-only or keyword variadic.
        // But, `self` could contain any unmatched positional parameters.
        let (self_parameters, other_parameters) = parameters.into_remaining();

        // Collect all the keyword-only parameters and the unmatched standard parameters.
        let mut self_keywords = HashMap::new();

        // Type of the variadic keyword parameter in `self`.
        //
        // This is a nested option where the outer option represents the presence of a keyword
        // variadic parameter in `self` and the inner option represents the annotated type of the
        // keyword variadic parameter.
        let mut self_keyword_variadic: Option<Option<Type<'db>>> = None;

        for self_parameter in self_parameters {
            match self_parameter.kind() {
                ParameterKind::KeywordOnly { name, .. }
                | ParameterKind::PositionalOrKeyword { name, .. } => {
                    self_keywords.insert(name.clone(), self_parameter);
                }
                ParameterKind::KeywordVariadic { .. } => {
                    self_keyword_variadic = Some(self_parameter.annotated_type());
                }
                ParameterKind::PositionalOnly { .. } => {
                    // These are the unmatched positional-only parameters in `self` from the
                    // previous loop. They cannot be matched against any parameter in `other` which
                    // only contains keyword-only and keyword-variadic parameters so the subtype
                    // relation is invalid.
                    return false;
                }
                ParameterKind::Variadic { .. } => {}
            }
        }

        for other_parameter in other_keywords.into_iter().chain(other_parameters) {
            match other_parameter.kind() {
                ParameterKind::KeywordOnly {
                    name: other_name,
                    default_type: other_default,
                }
                | ParameterKind::PositionalOrKeyword {
                    name: other_name,
                    default_type: other_default,
                } => {
                    if let Some(self_parameter) = self_keywords.remove(other_name) {
                        match self_parameter.kind() {
                            ParameterKind::PositionalOrKeyword {
                                default_type: self_default,
                                ..
                            }
                            | ParameterKind::KeywordOnly {
                                default_type: self_default,
                                ..
                            } => {
                                if self_default.is_none() && other_default.is_some() {
                                    return false;
                                }
                                if !check_types(
                                    other_parameter.annotated_type(),
                                    self_parameter.annotated_type(),
                                ) {
                                    return false;
                                }
                            }
                            _ => unreachable!(
                                "`self_keywords` should only contain keyword-only or standard parameters"
                            ),
                        }
                    } else if let Some(self_keyword_variadic_type) = self_keyword_variadic {
                        if !check_types(
                            other_parameter.annotated_type(),
                            self_keyword_variadic_type,
                        ) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                ParameterKind::KeywordVariadic { .. } => {
                    let Some(self_keyword_variadic_type) = self_keyword_variadic else {
                        // For a `self <: other` relationship, if `other` has a keyword variadic
                        // parameter, `self` must also have a keyword variadic parameter.
                        return false;
                    };
                    if !check_types(other_parameter.annotated_type(), self_keyword_variadic_type) {
                        return false;
                    }
                }
                _ => {
                    // This can only occur in case of a syntax error.
                    return false;
                }
            }
        }

        // If there are still unmatched keyword parameters from `self`, then they should be
        // optional otherwise the subtype relation is invalid.
        for (_, self_parameter) in self_keywords {
            if self_parameter.default_type().is_none() {
                return false;
            }
        }

        true
    }
}

/// Represents a specific instance of `types.MethodWrapperType`
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, salsa::Update)]
pub enum MethodWrapperKind<'db> {
    /// Method wrapper for `some_function.__get__`
    FunctionTypeDunderGet(FunctionType<'db>),
    /// Method wrapper for `some_property.__get__`
    PropertyDunderGet(PropertyInstanceType<'db>),
    /// Method wrapper for `some_property.__set__`
    PropertyDunderSet(PropertyInstanceType<'db>),
}

/// Represents a specific instance of `types.WrapperDescriptorType`
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, salsa::Update)]
pub enum WrapperDescriptorKind {
    /// `FunctionType.__get__`
    FunctionTypeDunderGet,
    /// `property.__get__`
    PropertyDunderGet,
    /// `property.__set__`
    PropertyDunderSet,
}

#[salsa::interned(debug)]
pub struct ModuleLiteralType<'db> {
    /// The file in which this module was imported.
    ///
    /// We need this in order to know which submodules should be attached to it as attributes
    /// (because the submodules were also imported in this file).
    pub importing_file: File,

    /// The imported module.
    pub module: Module,
}

impl<'db> ModuleLiteralType<'db> {
    fn static_member(self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        // `__dict__` is a very special member that is never overridden by module globals;
        // we should always look it up directly as an attribute on `types.ModuleType`,
        // never in the global scope of the module.
        if name == "__dict__" {
            return KnownClass::ModuleType
                .to_instance(db)
                .member(db, "__dict__")
                .symbol;
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
        if let Some(submodule_name) = ModuleName::new(name) {
            let importing_file = self.importing_file(db);
            let imported_submodules = imported_modules(db, importing_file);
            let mut full_submodule_name = self.module(db).name().clone();
            full_submodule_name.extend(&submodule_name);
            if imported_submodules.contains(&full_submodule_name) {
                if let Some(submodule) = resolve_module(db, &full_submodule_name) {
                    return Symbol::bound(Type::module_literal(db, importing_file, submodule));
                }
            }
        }

        imported_symbol(db, &self.module(db), name).symbol
    }
}

#[salsa::interned(debug)]
pub struct TypeAliasType<'db> {
    #[return_ref]
    pub name: ast::name::Name,

    rhs_scope: ScopeId<'db>,
}

#[salsa::tracked]
impl<'db> TypeAliasType<'db> {
    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let scope = self.rhs_scope(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();

        semantic_index(db, scope.file(db)).expect_single_definition(type_alias_stmt_node)
    }

    #[salsa::tracked]
    pub(crate) fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        let scope = self.rhs_scope(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();
        let definition = self.definition(db);
        definition_expression_type(db, definition, &type_alias_stmt_node.value)
    }
}

/// Either the explicit `metaclass=` keyword of the class, or the inferred metaclass of one of its base classes.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub(super) struct MetaclassCandidate<'db> {
    metaclass: Class<'db>,
    explicit_metaclass_of: Class<'db>,
}

#[salsa::interned(debug)]
pub struct UnionType<'db> {
    /// The union type includes values in any of these types.
    #[return_ref]
    elements_boxed: Box<[Type<'db>]>,
}

impl<'db> UnionType<'db> {
    fn elements(self, db: &'db dyn Db) -> &'db [Type<'db>] {
        self.elements_boxed(db)
    }

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

    /// Apply a transformation function to all elements of the union,
    /// and create a new union from the resulting set of types.
    pub fn map(
        &self,
        db: &'db dyn Db,
        transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        Self::from_elements(db, self.elements(db).iter().map(transform_fn))
    }

    pub(crate) fn filter(
        self,
        db: &'db dyn Db,
        filter_fn: impl FnMut(&&Type<'db>) -> bool,
    ) -> Type<'db> {
        Self::from_elements(db, self.elements(db).iter().filter(filter_fn))
    }

    pub fn iter(&self, db: &'db dyn Db) -> Iter<Type<'db>> {
        self.elements(db).iter()
    }

    pub(crate) fn map_with_boundness(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Symbol<'db>,
    ) -> Symbol<'db> {
        let mut builder = UnionBuilder::new(db);

        let mut all_unbound = true;
        let mut possibly_unbound = false;
        for ty in self.elements(db) {
            let ty_member = transform_fn(ty);
            match ty_member {
                Symbol::Unbound => {
                    possibly_unbound = true;
                }
                Symbol::Type(ty_member, member_boundness) => {
                    if member_boundness == Boundness::PossiblyUnbound {
                        possibly_unbound = true;
                    }

                    all_unbound = false;
                    builder = builder.add(ty_member);
                }
            }
        }

        if all_unbound {
            Symbol::Unbound
        } else {
            Symbol::Type(
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
        mut transform_fn: impl FnMut(&Type<'db>) -> SymbolAndQualifiers<'db>,
    ) -> SymbolAndQualifiers<'db> {
        let mut builder = UnionBuilder::new(db);
        let mut qualifiers = TypeQualifiers::empty();

        let mut all_unbound = true;
        let mut possibly_unbound = false;
        for ty in self.elements(db) {
            let SymbolAndQualifiers {
                symbol: ty_member,
                qualifiers: new_qualifiers,
            } = transform_fn(ty);
            qualifiers |= new_qualifiers;
            match ty_member {
                Symbol::Unbound => {
                    possibly_unbound = true;
                }
                Symbol::Type(ty_member, member_boundness) => {
                    if member_boundness == Boundness::PossiblyUnbound {
                        possibly_unbound = true;
                    }

                    all_unbound = false;
                    builder = builder.add(ty_member);
                }
            }
        }
        SymbolAndQualifiers {
            symbol: if all_unbound {
                Symbol::Unbound
            } else {
                Symbol::Type(
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

    pub(crate) fn is_fully_static(self, db: &'db dyn Db) -> bool {
        self.elements(db).iter().all(|ty| ty.is_fully_static(db))
    }

    /// Create a new union type with the elements normalized.
    ///
    /// See [`Type::normalized`] for more details.
    #[must_use]
    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        let mut new_elements: Vec<Type<'db>> = self
            .elements(db)
            .iter()
            .map(|element| element.normalized(db))
            .collect();
        new_elements.sort_unstable_by(|l, r| union_or_intersection_elements_ordering(db, l, r));
        UnionType::new(db, new_elements.into_boxed_slice())
    }

    /// Return `true` if `self` represents the exact same set of possible runtime objects as `other`
    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        /// Inlined version of [`UnionType::is_fully_static`] to avoid having to lookup
        /// `self.elements` multiple times in the Salsa db in this single method.
        #[inline]
        fn all_fully_static(db: &dyn Db, elements: &[Type]) -> bool {
            elements.iter().all(|ty| ty.is_fully_static(db))
        }

        let self_elements = self.elements(db);
        let other_elements = other.elements(db);

        if self_elements.len() != other_elements.len() {
            return false;
        }

        if !all_fully_static(db, self_elements) {
            return false;
        }

        if !all_fully_static(db, other_elements) {
            return false;
        }

        if self == other {
            return true;
        }

        let sorted_self = self.normalized(db);

        if sorted_self == other {
            return true;
        }

        sorted_self == other.normalized(db)
    }

    /// Return `true` if `self` has exactly the same set of possible static materializations as `other`
    /// (if `self` represents the same set of possible sets of possible runtime objects as `other`)
    pub(crate) fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }

        // TODO: `T | Unknown` should be gradually equivalent to `T | Unknown | Any`,
        // since they have exactly the same set of possible static materializations
        // (they represent the same set of possible sets of possible runtime objects)
        if self.elements(db).len() != other.elements(db).len() {
            return false;
        }

        let sorted_self = self.normalized(db);

        if sorted_self == other {
            return true;
        }

        let sorted_other = other.normalized(db);

        if sorted_self == sorted_other {
            return true;
        }

        sorted_self
            .elements(db)
            .iter()
            .zip(sorted_other.elements(db))
            .all(|(self_ty, other_ty)| self_ty.is_gradual_equivalent_to(db, *other_ty))
    }
}

#[salsa::interned(debug)]
pub struct IntersectionType<'db> {
    /// The intersection type includes only values in all of these types.
    #[return_ref]
    positive: FxOrderSet<Type<'db>>,

    /// The intersection type does not include any value in any of these types.
    ///
    /// Negation types aren't expressible in annotations, and are most likely to arise from type
    /// narrowing along with intersections (e.g. `if not isinstance(...)`), so we represent them
    /// directly in intersections rather than as a separate type.
    #[return_ref]
    negative: FxOrderSet<Type<'db>>,
}

impl<'db> IntersectionType<'db> {
    /// Return a new `IntersectionType` instance with the positive and negative types sorted
    /// according to a canonical ordering, and other normalizations applied to each element as applicable.
    ///
    /// See [`Type::normalized`] for more details.
    #[must_use]
    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        fn normalized_set<'db>(
            db: &'db dyn Db,
            elements: &FxOrderSet<Type<'db>>,
        ) -> FxOrderSet<Type<'db>> {
            let mut elements: FxOrderSet<Type<'db>> =
                elements.iter().map(|ty| ty.normalized(db)).collect();

            elements.sort_unstable_by(|l, r| union_or_intersection_elements_ordering(db, l, r));
            elements
        }

        IntersectionType::new(
            db,
            normalized_set(db, self.positive(db)),
            normalized_set(db, self.negative(db)),
        )
    }

    pub(crate) fn is_fully_static(self, db: &'db dyn Db) -> bool {
        self.positive(db).iter().all(|ty| ty.is_fully_static(db))
            && self.negative(db).iter().all(|ty| ty.is_fully_static(db))
    }

    /// Return `true` if `self` represents exactly the same set of possible runtime objects as `other`
    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        /// Inlined version of [`IntersectionType::is_fully_static`] to avoid having to lookup
        /// `positive` and `negative` multiple times in the Salsa db in this single method.
        #[inline]
        fn all_fully_static(db: &dyn Db, elements: &FxOrderSet<Type>) -> bool {
            elements.iter().all(|ty| ty.is_fully_static(db))
        }

        let self_positive = self.positive(db);

        if !all_fully_static(db, self_positive) {
            return false;
        }

        let other_positive = other.positive(db);

        if self_positive.len() != other_positive.len() {
            return false;
        }

        if !all_fully_static(db, other_positive) {
            return false;
        }

        let self_negative = self.negative(db);

        if !all_fully_static(db, self_negative) {
            return false;
        }

        let other_negative = other.negative(db);

        if self_negative.len() != other_negative.len() {
            return false;
        }

        if !all_fully_static(db, other_negative) {
            return false;
        }

        if self == other {
            return true;
        }

        let sorted_self = self.normalized(db);

        if sorted_self == other {
            return true;
        }

        sorted_self == other.normalized(db)
    }

    /// Return `true` if `self` has exactly the same set of possible static materializations as `other`
    /// (if `self` represents the same set of possible sets of possible runtime objects as `other`)
    pub(crate) fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }

        if self.positive(db).len() != other.positive(db).len()
            || self.negative(db).len() != other.negative(db).len()
        {
            return false;
        }

        let sorted_self = self.normalized(db);

        if sorted_self == other {
            return true;
        }

        let sorted_other = other.normalized(db);

        if sorted_self == sorted_other {
            return true;
        }

        sorted_self
            .positive(db)
            .iter()
            .zip(sorted_other.positive(db))
            .all(|(self_ty, other_ty)| self_ty.is_gradual_equivalent_to(db, *other_ty))
            && sorted_self
                .negative(db)
                .iter()
                .zip(sorted_other.negative(db))
                .all(|(self_ty, other_ty)| self_ty.is_gradual_equivalent_to(db, *other_ty))
    }

    pub(crate) fn map_with_boundness(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Symbol<'db>,
    ) -> Symbol<'db> {
        if !self.negative(db).is_empty() {
            return Symbol::todo("map_with_boundness: intersections with negative contributions");
        }

        let mut builder = IntersectionBuilder::new(db);

        let mut all_unbound = true;
        let mut any_definitely_bound = false;
        for ty in self.positive(db) {
            let ty_member = transform_fn(ty);
            match ty_member {
                Symbol::Unbound => {}
                Symbol::Type(ty_member, member_boundness) => {
                    all_unbound = false;
                    if member_boundness == Boundness::Bound {
                        any_definitely_bound = true;
                    }

                    builder = builder.add_positive(ty_member);
                }
            }
        }

        if all_unbound {
            Symbol::Unbound
        } else {
            Symbol::Type(
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
        mut transform_fn: impl FnMut(&Type<'db>) -> SymbolAndQualifiers<'db>,
    ) -> SymbolAndQualifiers<'db> {
        if !self.negative(db).is_empty() {
            return Symbol::todo("map_with_boundness: intersections with negative contributions")
                .into();
        }

        let mut builder = IntersectionBuilder::new(db);
        let mut qualifiers = TypeQualifiers::empty();

        let mut any_unbound = false;
        let mut any_possibly_unbound = false;
        for ty in self.positive(db) {
            let SymbolAndQualifiers {
                symbol: member,
                qualifiers: new_qualifiers,
            } = transform_fn(ty);
            qualifiers |= new_qualifiers;
            match member {
                Symbol::Unbound => {
                    any_unbound = true;
                }
                Symbol::Type(ty_member, member_boundness) => {
                    if member_boundness == Boundness::PossiblyUnbound {
                        any_possibly_unbound = true;
                    }

                    builder = builder.add_positive(ty_member);
                }
            }
        }

        SymbolAndQualifiers {
            symbol: if any_unbound {
                Symbol::Unbound
            } else {
                Symbol::Type(
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
}

#[salsa::interned(debug)]
pub struct StringLiteralType<'db> {
    #[return_ref]
    value: Box<str>,
}

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
            .map(|c| StringLiteralType::new(db, c.to_string().as_str()))
    }
}

#[salsa::interned(debug)]
pub struct BytesLiteralType<'db> {
    #[return_ref]
    value: Box<[u8]>,
}

impl<'db> BytesLiteralType<'db> {
    pub(crate) fn python_len(self, db: &'db dyn Db) -> usize {
        self.value(db).len()
    }
}

#[salsa::interned(debug)]
pub struct SliceLiteralType<'db> {
    start: Option<i32>,
    stop: Option<i32>,
    step: Option<i32>,
}

impl SliceLiteralType<'_> {
    fn as_tuple(self, db: &dyn Db) -> (Option<i32>, Option<i32>, Option<i32>) {
        (self.start(db), self.stop(db), self.step(db))
    }
}
#[salsa::interned(debug)]
pub struct TupleType<'db> {
    #[return_ref]
    elements: Box<[Type<'db>]>,
}

impl<'db> TupleType<'db> {
    pub(crate) fn from_elements<T: Into<Type<'db>>>(
        db: &'db dyn Db,
        types: impl IntoIterator<Item = T>,
    ) -> Type<'db> {
        let mut elements = vec![];

        for ty in types {
            let ty = ty.into();
            if ty.is_never() {
                return Type::Never;
            }
            elements.push(ty);
        }

        Type::Tuple(Self::new(db, elements.into_boxed_slice()))
    }

    /// Return a normalized version of `self`.
    ///
    /// See [`Type::normalized`] for more details.
    #[must_use]
    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        let elements: Box<[Type<'db>]> = self
            .elements(db)
            .iter()
            .map(|ty| ty.normalized(db))
            .collect();
        TupleType::new(db, elements)
    }

    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        let self_elements = self.elements(db);
        let other_elements = other.elements(db);
        self_elements.len() == other_elements.len()
            && self_elements
                .iter()
                .zip(other_elements)
                .all(|(self_ty, other_ty)| self_ty.is_equivalent_to(db, *other_ty))
    }

    pub(crate) fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        let self_elements = self.elements(db);
        let other_elements = other.elements(db);
        self_elements.len() == other_elements.len()
            && self_elements
                .iter()
                .zip(other_elements)
                .all(|(self_ty, other_ty)| self_ty.is_gradual_equivalent_to(db, *other_ty))
    }

    pub fn get(&self, db: &'db dyn Db, index: usize) -> Option<Type<'db>> {
        self.elements(db).get(index).copied()
    }

    pub fn len(&self, db: &'db dyn Db) -> usize {
        self.elements(db).len()
    }
}

// Make sure that the `Type` enum does not grow unexpectedly.
#[cfg(not(debug_assertions))]
#[cfg(target_pointer_width = "64")]
static_assertions::assert_eq_size!(Type, [u8; 16]);

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::tests::{setup_db, TestDbBuilder};
    use crate::symbol::{
        global_symbol, known_module_symbol, typing_extensions_symbol, typing_symbol,
    };
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast::PythonVersion;
    use strum::IntoEnumIterator;
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

        let typing_no_default = typing_symbol(&db, "NoDefault").symbol.expect_type();
        let typing_extensions_no_default = typing_extensions_symbol(&db, "NoDefault")
            .symbol
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
        let a = global_symbol(&db, bar, "a").symbol;

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

        let a = global_symbol(&db, bar, "a").symbol;

        assert_eq!(
            a.expect_type(),
            UnionType::from_elements(&db, [Type::unknown(), KnownClass::Int.to_instance(&db)])
        );
        let events = db.take_salsa_events();

        let call = &*parsed_module(&db, bar).syntax().body[1]
            .as_assign_stmt()
            .unwrap()
            .value;
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
        assert!(IntersectionBuilder::new(&db)
            .add_positive(todo1)
            .add_positive(todo2)
            .build()
            .is_todo());
        assert!(IntersectionBuilder::new(&db)
            .add_positive(todo1)
            .add_negative(todo2)
            .build()
            .is_todo());
    }

    #[test]
    fn known_function_roundtrip_from_str() {
        let db = setup_db();

        for function in KnownFunction::iter() {
            let function_name: &'static str = function.into();

            let module = match function {
                KnownFunction::Len
                | KnownFunction::Repr
                | KnownFunction::IsInstance
                | KnownFunction::IsSubclass => KnownModule::Builtins,

                KnownFunction::AbstractMethod => KnownModule::Abc,

                KnownFunction::GetattrStatic => KnownModule::Inspect,

                KnownFunction::Cast
                | KnownFunction::Final
                | KnownFunction::Overload
                | KnownFunction::RevealType
                | KnownFunction::AssertType
                | KnownFunction::NoTypeCheck => KnownModule::TypingExtensions,

                KnownFunction::IsSingleton
                | KnownFunction::IsSubtypeOf
                | KnownFunction::StaticAssert
                | KnownFunction::IsFullyStatic
                | KnownFunction::IsDisjointFrom
                | KnownFunction::IsSingleValued
                | KnownFunction::IsAssignableTo
                | KnownFunction::IsEquivalentTo
                | KnownFunction::IsGradualEquivalentTo => KnownModule::KnotExtensions,
            };

            let function_definition = known_module_symbol(&db, module, function_name)
                .symbol
                .expect_type()
                .expect_function_literal()
                .definition(&db);

            assert_eq!(
                KnownFunction::try_from_definition_and_name(&db, function_definition, function_name),
                Some(function),
                "The strum `EnumString` implementation appears to be incorrect for `{function_name}`"
            );
        }
    }
}
