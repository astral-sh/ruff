use std::hash::Hash;
use std::str::FromStr;

use bitflags::bitflags;
use call::{CallDunderError, CallError, CallErrorKind};
use context::InferContext;
use diagnostic::{INVALID_CONTEXT_MANAGER, NOT_ITERABLE};
use ruff_db::files::File;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_text_size::{Ranged, TextRange};
use type_ordering::union_or_intersection_elements_ordering;

pub(crate) use self::builder::{IntersectionBuilder, UnionBuilder};
pub(crate) use self::diagnostic::register_lints;
pub use self::diagnostic::{TypeCheckDiagnostic, TypeCheckDiagnostics};
pub(crate) use self::display::TypeArrayDisplay;
pub(crate) use self::infer::{
    infer_deferred_types, infer_definition_types, infer_expression_type, infer_expression_types,
    infer_scope_types,
};
pub use self::narrow::KnownConstraintFunction;
pub(crate) use self::signatures::{CallableSignature, Signature, Signatures};
pub use self::subclass_of::SubclassOfType;
use crate::module_name::ModuleName;
use crate::module_resolver::{file_to_module, resolve_module, KnownModule};
use crate::semantic_index::ast_ids::HasScopedExpressionId;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::ScopeId;
use crate::semantic_index::{imported_modules, semantic_index};
use crate::suppression::check_suppressions;
use crate::symbol::{imported_symbol, Boundness, Symbol, SymbolAndQualifiers};
use crate::types::call::{Bindings, CallArguments};
use crate::types::class_base::ClassBase;
use crate::types::diagnostic::{INVALID_TYPE_FORM, UNSUPPORTED_BOOL_CONVERSION};
use crate::types::infer::infer_unpack_types;
use crate::types::mro::{Mro, MroError, MroIterator};
pub(crate) use crate::types::narrow::infer_narrowing_constraint;
use crate::types::signatures::{Parameter, ParameterKind, Parameters};
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

/// The descriptor protocol distiguishes two kinds of descriptors. Non-data descriptors
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

/// Dunder methods are looked up on the meta-type of a type without potentially falling
/// back on attributes on the type itself. For example, when implicitly invoked on an
/// instance, dunder methods are not looked up as instance attributes. And when invoked
/// on a class, dunder methods are only looked up on the metaclass, not the class itself.
///
/// All other attributes use the `WithInstanceFallback` policy.
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
enum MemberLookupPolicy {
    /// Only look up the attribute on the meta-type.
    NoInstanceFallback,
    /// Look up the attribute on the meta-type, but fall back to attributes on the instance
    /// if the meta-type attribute is not found or if the meta-type attribute is not a data
    /// descriptor.
    WithInstanceFallback,
}

impl AttributeKind {
    const fn is_data(self) -> bool {
        matches!(self, Self::DataDescriptor)
    }
}

/// Meta data for `Type::Todo`, which represents a known limitation in red-knot.
#[cfg(debug_assertions)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TodoType {
    FileAndLine(&'static str, u32),
    Message(&'static str),
}

#[cfg(debug_assertions)]
impl std::fmt::Display for TodoType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TodoType::FileAndLine(file, line) => write!(f, "[{file}:{line}]"),
            TodoType::Message(msg) => write!(f, "({msg})"),
        }
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
/// It can be used with a custom message (preferred): `todo_type!("PEP 604 not supported")`,
/// or simply using `todo_type!()`, which will include information about the file and line.
#[cfg(debug_assertions)]
macro_rules! todo_type {
    () => {
        $crate::types::Type::Dynamic($crate::types::DynamicType::Todo(
            $crate::types::TodoType::FileAndLine(file!(), line!()),
        ))
    };
    ($message:literal) => {
        $crate::types::Type::Dynamic($crate::types::DynamicType::Todo(
            $crate::types::TodoType::Message($message),
        ))
    };
    ($message:ident) => {
        $crate::types::Type::Dynamic($crate::types::DynamicType::Todo(
            $crate::types::TodoType::Message($message),
        ))
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

pub(crate) use todo_type;

/// Representation of a type: a set of possible values at runtime.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update)]
pub enum Type<'db> {
    /// The dynamic type: a statically unknown set of values
    Dynamic(DynamicType),
    /// The empty set of values
    Never,
    /// A specific function object
    FunctionLiteral(FunctionType<'db>),
    /// A callable object
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
    // TODO protocols, callable types, overloads, generics, type vars
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

    pub fn is_object(&self, db: &'db dyn Db) -> bool {
        self.into_instance()
            .is_some_and(|instance| instance.class().is_object(db))
    }

    pub const fn is_todo(&self) -> bool {
        matches!(self, Type::Dynamic(DynamicType::Todo(_)))
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

    pub const fn is_class_literal(&self) -> bool {
        matches!(self, Type::ClassLiteral(..))
    }

    pub const fn is_instance(&self) -> bool {
        matches!(self, Type::Instance(..))
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

    /// Return a normalized version of `self` in which all unions and intersections are sorted
    /// according to a canonical order, no matter how "deeply" a union/intersection may be nested.
    #[must_use]
    pub fn with_sorted_unions_and_intersections(self, db: &'db dyn Db) -> Self {
        match self {
            Type::Union(union) => Type::Union(union.to_sorted_union(db)),
            Type::Intersection(intersection) => {
                Type::Intersection(intersection.to_sorted_intersection(db))
            }
            Type::Tuple(tuple) => Type::Tuple(tuple.with_sorted_unions_and_intersections(db)),
            Type::LiteralString
            | Type::Instance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::BooleanLiteral(_)
            | Type::SliceLiteral(_)
            | Type::BytesLiteral(_)
            | Type::StringLiteral(_)
            | Type::Dynamic(_)
            | Type::Never
            | Type::FunctionLiteral(_)
            | Type::Callable(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::KnownInstance(_)
            | Type::IntLiteral(_)
            | Type::SubclassOf(_) => self,
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

            (Type::Union(union), _) => union
                .elements(db)
                .iter()
                .all(|&elem_ty| elem_ty.is_subtype_of(db, target)),

            (_, Type::Union(union)) => union
                .elements(db)
                .iter()
                .any(|&elem_ty| self.is_subtype_of(db, elem_ty)),

            // `object` is the only type that can be known to be a supertype of any intersection,
            // even an intersection with no positive elements
            (Type::Intersection(_), Type::Instance(InstanceType { class }))
                if class.is_object(db) =>
            {
                true
            }

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

            // A `FunctionLiteral` type is a single-valued type like the other literals handled above,
            // so it also, for now, just delegates to its instance fallback.
            // This will change in a way similar to the `LiteralString`/`StringLiteral()` case above
            // when we add support for `typing.Callable`.
            (Type::FunctionLiteral(_), _) => KnownClass::FunctionType
                .to_instance(db)
                .is_subtype_of(db, target),

            // The same reasoning applies for these special callable types:
            (Type::Callable(CallableType::BoundMethod(_)), _) => KnownClass::MethodType
                .to_instance(db)
                .is_subtype_of(db, target),
            (Type::Callable(CallableType::MethodWrapperDunderGet(_)), _) => {
                KnownClass::WrapperDescriptorType
                    .to_instance(db)
                    .is_subtype_of(db, target)
            }
            (Type::Callable(CallableType::WrapperDescriptorDunderGet), _) => {
                KnownClass::WrapperDescriptorType
                    .to_instance(db)
                    .is_subtype_of(db, target)
            }

            (Type::Callable(CallableType::General(_)), _) => {
                // TODO: Implement subtyping for general callable types
                false
            }

            // A fully static heterogenous tuple type `A` is a subtype of a fully static heterogeneous tuple type `B`
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
            // All heterogenous tuple types are subtypes of `Instance(<tuple>)`:
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

            // Other than the special cases enumerated above,
            // `Instance` types are never subtypes of any other variants
            (Type::Instance(_), _) => false,
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

            (Type::Tuple(first), Type::Tuple(second)) => first.is_gradual_equivalent_to(db, second),

            (Type::Union(first), Type::Union(second)) => first.is_gradual_equivalent_to(db, second),

            (Type::Intersection(first), Type::Intersection(second)) => {
                first.is_gradual_equivalent_to(db, second)
            }

            (
                Type::Callable(CallableType::General(first)),
                Type::Callable(CallableType::General(second)),
            ) => first.is_gradual_equivalent_to(db, second),

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
                | Type::Callable(
                    CallableType::BoundMethod(..)
                    | CallableType::MethodWrapperDunderGet(..)
                    | CallableType::WrapperDescriptorDunderGet,
                )
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)
                | Type::KnownInstance(..)),
                right @ (Type::BooleanLiteral(..)
                | Type::IntLiteral(..)
                | Type::StringLiteral(..)
                | Type::BytesLiteral(..)
                | Type::SliceLiteral(..)
                | Type::FunctionLiteral(..)
                | Type::Callable(
                    CallableType::BoundMethod(..)
                    | CallableType::MethodWrapperDunderGet(..)
                    | CallableType::WrapperDescriptorDunderGet,
                )
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
                | Type::Callable(..)
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
                | Type::Callable(..)
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
                | Type::Callable(..)
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
                | Type::Callable(..)
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
            // so although the type is dynamic we can still determine disjointness in some situations
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

            (
                Type::Callable(CallableType::BoundMethod(_)),
                Type::Instance(InstanceType { class }),
            )
            | (
                Type::Instance(InstanceType { class }),
                Type::Callable(CallableType::BoundMethod(_)),
            ) => !KnownClass::MethodType.is_subclass_of(db, class),

            (
                Type::Callable(CallableType::MethodWrapperDunderGet(_)),
                Type::Instance(InstanceType { class }),
            )
            | (
                Type::Instance(InstanceType { class }),
                Type::Callable(CallableType::MethodWrapperDunderGet(_)),
            ) => !KnownClass::MethodWrapperType.is_subclass_of(db, class),

            (
                Type::Callable(CallableType::WrapperDescriptorDunderGet),
                Type::Instance(InstanceType { class }),
            )
            | (
                Type::Instance(InstanceType { class }),
                Type::Callable(CallableType::WrapperDescriptorDunderGet),
            ) => !KnownClass::WrapperDescriptorType.is_subclass_of(db, class),

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

            (Type::Callable(CallableType::General(_)), _)
            | (_, Type::Callable(CallableType::General(_))) => {
                // TODO: Implement disjointness for general callable types
                false
            }
        }
    }

    /// Returns true if the type does not contain any gradual forms (as a sub-part).
    pub(crate) fn is_fully_static(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::Dynamic(_) => false,
            Type::Never
            | Type::FunctionLiteral(..)
            | Type::Callable(
                CallableType::BoundMethod(_)
                | CallableType::MethodWrapperDunderGet(_)
                | CallableType::WrapperDescriptorDunderGet,
            )
            | Type::ModuleLiteral(..)
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::BytesLiteral(_)
            | Type::SliceLiteral(_)
            | Type::KnownInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy => true,
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
            Type::Callable(CallableType::General(callable)) => callable.is_fully_static(db),
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
            // We eagerly transform `SubclassOf` to `ClassLiteral` for final types, so `SubclassOf` is never a singleton.
            Type::SubclassOf(..) => false,
            Type::BooleanLiteral(_)
            | Type::FunctionLiteral(..)
            | Type::Callable(
                CallableType::BoundMethod(_)
                | CallableType::MethodWrapperDunderGet(_)
                | CallableType::WrapperDescriptorDunderGet,
            )
            | Type::ClassLiteral(..)
            | Type::ModuleLiteral(..)
            | Type::KnownInstance(..) => true,
            Type::Callable(CallableType::General(_)) => {
                // A general callable type is never a singleton because for any given signature,
                // there could be any number of distinct objects that are all callable with that
                // signature.
                false
            }
            Type::Instance(InstanceType { class }) => {
                class.known(db).is_some_and(KnownClass::is_singleton)
            }
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
            | Type::Callable(
                CallableType::BoundMethod(..)
                | CallableType::MethodWrapperDunderGet(..)
                | CallableType::WrapperDescriptorDunderGet,
            )
            | Type::ModuleLiteral(..)
            | Type::ClassLiteral(..)
            | Type::IntLiteral(..)
            | Type::BooleanLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::SliceLiteral(..)
            | Type::KnownInstance(..) => true,

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
            | Type::Callable(CallableType::General(_)) => false,
        }
    }

    /// This function is roughly equivalent to `find_name_in_mro` as defined in the [descriptor guide] or
    /// [`_PyType_Lookup`] in CPython's `Objects/typeobject.c`. It should typically be called through
    /// [Type::class_member], unless it is known that `self` is a class-like type. This function returns
    /// `None` if called on an instance-like type.
    ///
    /// [descriptor guide]: https://docs.python.org/3/howto/descriptor.html#invocation-from-an-instance
    /// [`_PyType_Lookup`]: https://github.com/python/cpython/blob/e285232c76606e3be7bf216efb1be1e742423e4b/Objects/typeobject.c#L5223
    fn find_name_in_mro(&self, db: &'db dyn Db, name: &str) -> Option<SymbolAndQualifiers<'db>> {
        match self {
            Type::Union(union) => Some(union.map_with_boundness_and_qualifiers(db, |elem| {
                elem.find_name_in_mro(db, name)
                    // If some elements are classes, and some are not, we simply fall back to `Unbound` for the non-class
                    // elements instead of short-circuiting the whole result to `None`. We would need a more detailed
                    // return type otherwise, and since `find_name_in_mro` is usually called via `class_member`, this is
                    // not a problem.
                    .unwrap_or_default()
            })),
            Type::Intersection(inter) => {
                Some(inter.map_with_boundness_and_qualifiers(db, |elem| {
                    elem.find_name_in_mro(db, name)
                        // Fall back to Unbound, similar to the union case (see above).
                        .unwrap_or_default()
                }))
            }

            Type::Dynamic(_) | Type::Never => Some(Symbol::bound(self).into()),

            Type::ClassLiteral(class_literal @ ClassLiteralType { class }) => {
                match (class.known(db), name) {
                    (Some(KnownClass::FunctionType), "__get__") => Some(
                        Symbol::bound(Type::Callable(CallableType::WrapperDescriptorDunderGet))
                            .into(),
                    ),
                    (Some(KnownClass::FunctionType), "__set__" | "__delete__") => {
                        // Hard code this knowledge, as we look up `__set__` and `__delete__` on `FunctionType` often.
                        Some(Symbol::Unbound.into())
                    }
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

                    _ => Some(class_literal.class_member(db, name)),
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
            Type::SubclassOf(subclass_of_ty) => subclass_of_ty.find_name_in_mro(db, name),

            // We eagerly normalize type[object], i.e. Type::SubclassOf(object) to `type`, i.e. Type::Instance(type).
            // So looking up a name in the MRO of `Type::Instance(type)` is equivalent to looking up the name in the
            // MRO of the class `object`.
            Type::Instance(InstanceType { class }) if class.is_known(db, KnownClass::Type) => {
                KnownClass::Object
                    .to_class_literal(db)
                    .find_name_in_mro(db, name)
            }

            Type::FunctionLiteral(_)
            | Type::Callable(_)
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
            | Type::Instance(_) => None,
        }
    }

    /// Look up an attribute in the MRO of the meta-type of `self`. This returns class-level attributes
    /// when called on an instance-like type, and metaclass attributes when called on a class-like type.
    ///
    /// Basically corresponds to `self.to_meta_type().find_name_in_mro(name)`, except for the handling
    /// of union and intersection types.
    #[salsa::tracked]
    fn class_member(self, db: &'db dyn Db, name: Name) -> SymbolAndQualifiers<'db> {
        tracing::trace!("class_member: {}.{}", self.display(db), name);
        match self {
            Type::Union(union) => union
                .map_with_boundness_and_qualifiers(db, |elem| elem.class_member(db, name.clone())),
            Type::Intersection(inter) => inter
                .map_with_boundness_and_qualifiers(db, |elem| elem.class_member(db, name.clone())),
            _ => self
                .to_meta_type(db)
                .find_name_in_mro(db, name.as_str())
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

            Type::Callable(CallableType::BoundMethod(_)) => KnownClass::MethodType
                .to_instance(db)
                .instance_member(db, name),
            Type::Callable(CallableType::MethodWrapperDunderGet(_)) => {
                KnownClass::MethodWrapperType
                    .to_instance(db)
                    .instance_member(db, name)
            }
            Type::Callable(CallableType::WrapperDescriptorDunderGet) => {
                KnownClass::WrapperDescriptorType
                    .to_instance(db)
                    .instance_member(db, name)
            }
            Type::Callable(CallableType::General(_)) => {
                KnownClass::Object.to_instance(db).instance_member(db, name)
            }

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
        } else if let symbol @ Symbol::Type(_, _) = self.class_member(db, name.into()).symbol {
            symbol
        } else if let Some(symbol @ Symbol::Type(_, _)) =
            self.find_name_in_mro(db, name).map(|inner| inner.symbol)
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
        let descr_get = self.class_member(db, "__get__".into()).symbol;

        if let Symbol::Type(descr_get, descr_get_boundness) = descr_get {
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

            let descriptor_kind = if self.class_member(db, "__set__".into()).symbol.is_unbound()
                && self
                    .class_member(db, "__delete__".into())
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
    ) -> SymbolAndQualifiers<'db> {
        let (
            SymbolAndQualifiers {
                symbol: meta_attr,
                qualifiers: meta_attr_qualifiers,
            },
            meta_attr_kind,
        ) = Self::try_call_dunder_get_on_attribute(
            db,
            self.class_member(db, name.into()),
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
        self.member_lookup_with_policy(db, name.into(), MemberLookupPolicy::WithInstanceFallback)
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
                .map_with_boundness(db, |elem| elem.member(db, &name).symbol)
                .into(),

            Type::Intersection(intersection) => intersection
                .map_with_boundness(db, |elem| elem.member(db, &name).symbol)
                .into(),

            Type::Dynamic(..) | Type::Never => Symbol::bound(self).into(),

            Type::FunctionLiteral(function) if name == "__get__" => Symbol::bound(Type::Callable(
                CallableType::MethodWrapperDunderGet(function),
            ))
            .into(),

            Type::ClassLiteral(ClassLiteralType { class })
                if name == "__get__" && class.is_known(db, KnownClass::FunctionType) =>
            {
                Symbol::bound(Type::Callable(CallableType::WrapperDescriptorDunderGet)).into()
            }

            Type::Callable(CallableType::BoundMethod(bound_method)) => match name_str {
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
            Type::Callable(CallableType::MethodWrapperDunderGet(_)) => {
                KnownClass::MethodWrapperType
                    .to_instance(db)
                    .member(db, &name)
            }
            Type::Callable(CallableType::WrapperDescriptorDunderGet) => {
                KnownClass::WrapperDescriptorType
                    .to_instance(db)
                    .member(db, &name)
            }
            Type::Callable(CallableType::General(_)) => {
                KnownClass::Object.to_instance(db).member(db, &name)
            }

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

            Type::IntLiteral(_) if matches!(name_str, "real" | "numerator") => {
                Symbol::bound(self).into()
            }

            Type::BooleanLiteral(bool_value) if matches!(name_str, "real" | "numerator") => {
                Symbol::bound(Type::IntLiteral(i64::from(bool_value))).into()
            }

            Type::ModuleLiteral(module) => module.static_member(db, name_str).into(),

            Type::AlwaysFalsy | Type::AlwaysTruthy => self.class_member(db, name),

            _ if policy == MemberLookupPolicy::NoInstanceFallback => self
                .invoke_descriptor_protocol(
                    db,
                    name_str,
                    Symbol::Unbound.into(),
                    InstanceFallbackShadowsNonDataDescriptor::No,
                ),

            Type::Instance(..)
            | Type::BooleanLiteral(..)
            | Type::IntLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::LiteralString
            | Type::SliceLiteral(..)
            | Type::Tuple(..)
            | Type::KnownInstance(..)
            | Type::FunctionLiteral(..) => {
                let fallback = self.instance_member(db, name_str);

                let result = self.invoke_descriptor_protocol(
                    db,
                    name_str,
                    fallback,
                    InstanceFallbackShadowsNonDataDescriptor::No,
                );

                let custom_getattr_result =
                    || {
                        // Typeshed has a fake `__getattr__` on `types.ModuleType` to help out with dynamic imports.
                        // We explicitly hide it here to prevent arbitrary attributes from being available on modules.
                        if self.into_instance().is_some_and(|instance| {
                            instance.class.is_known(db, KnownClass::ModuleType)
                        }) {
                            return Symbol::Unbound.into();
                        }

                        self.try_call_dunder(
                            db,
                            "__getattr__",
                            &CallArguments::positional([Type::StringLiteral(
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
                let class_attr_plain = self.find_name_in_mro(db, name_str).expect(
                    "Calling `find_name_in_mro` on class literals and subclass-of types should always return `Some`",
                );

                if name == "__mro__" {
                    return class_attr_plain;
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
        let truthiness = match self {
            Type::Dynamic(_) | Type::Never => Truthiness::Ambiguous,
            Type::FunctionLiteral(_) => Truthiness::AlwaysTrue,
            Type::Callable(_) => Truthiness::AlwaysTrue,
            Type::ModuleLiteral(_) => Truthiness::AlwaysTrue,
            Type::ClassLiteral(ClassLiteralType { class }) => class
                .metaclass_instance_type(db)
                .try_bool_impl(db, allow_short_circuit)?,
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                ClassBase::Dynamic(_) => Truthiness::Ambiguous,
                ClassBase::Class(class) => {
                    Type::class_literal(class).try_bool_impl(db, allow_short_circuit)?
                }
            },
            Type::AlwaysTruthy => Truthiness::AlwaysTrue,
            Type::AlwaysFalsy => Truthiness::AlwaysFalse,
            instance_ty @ Type::Instance(InstanceType { class }) => match class.known(db) {
                Some(known_class) => known_class.bool(),
                None => {
                    // We only check the `__bool__` method for truth testing, even though at
                    // runtime there is a fallback to `__len__`, since `__bool__` takes precedence
                    // and a subclass could add a `__bool__` method.

                    let type_to_truthiness = |ty| {
                        if let Type::BooleanLiteral(bool_val) = ty {
                            Truthiness::from(bool_val)
                        } else {
                            Truthiness::Ambiguous
                        }
                    };

                    match self.try_call_dunder(db, "__bool__", &CallArguments::none()) {
                        Ok(outcome) => {
                            let return_type = outcome.return_type(db);
                            if !return_type.is_assignable_to(db, KnownClass::Bool.to_instance(db)) {
                                // The type has a `__bool__` method, but it doesn't return a
                                // boolean.
                                return Err(BoolError::IncorrectReturnType {
                                    return_type,
                                    not_boolable_type: *instance_ty,
                                });
                            }
                            type_to_truthiness(return_type)
                        }

                        Err(CallDunderError::PossiblyUnbound(outcome)) => {
                            let return_type = outcome.return_type(db);
                            if !return_type.is_assignable_to(db, KnownClass::Bool.to_instance(db)) {
                                // The type has a `__bool__` method, but it doesn't return a
                                // boolean.
                                return Err(BoolError::IncorrectReturnType {
                                    return_type: outcome.return_type(db),
                                    not_boolable_type: *instance_ty,
                                });
                            }

                            // Don't trust possibly unbound `__bool__` method.
                            Truthiness::Ambiguous
                        }

                        Err(CallDunderError::MethodNotAvailable) => Truthiness::Ambiguous,
                        Err(CallDunderError::CallError(CallErrorKind::BindingError, bindings)) => {
                            return Err(BoolError::IncorrectArguments {
                                truthiness: type_to_truthiness(bindings.return_type(db)),
                                not_boolable_type: *instance_ty,
                            });
                        }
                        Err(CallDunderError::CallError(CallErrorKind::NotCallable, _)) => {
                            return Err(BoolError::NotCallable {
                                not_boolable_type: *instance_ty,
                            });
                        }
                        Err(CallDunderError::CallError(CallErrorKind::PossiblyNotCallable, _)) => {
                            return Err(BoolError::Other {
                                not_boolable_type: *self,
                            })
                        }
                    }
                }
            },
            Type::KnownInstance(known_instance) => known_instance.bool(),
            Type::Union(union) => {
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
                        union: *union,
                        truthiness: truthiness.unwrap_or(Truthiness::Ambiguous),
                    });
                }
                truthiness.unwrap_or(Truthiness::Ambiguous)
            }
            Type::Intersection(_) => {
                // TODO
                Truthiness::Ambiguous
            }
            Type::IntLiteral(num) => Truthiness::from(*num != 0),
            Type::BooleanLiteral(bool) => Truthiness::from(*bool),
            Type::StringLiteral(str) => Truthiness::from(!str.value(db).is_empty()),
            Type::LiteralString => Truthiness::Ambiguous,
            Type::BytesLiteral(bytes) => Truthiness::from(!bytes.value(db).is_empty()),
            Type::SliceLiteral(_) => Truthiness::AlwaysTrue,
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

        let return_ty = match self.try_call_dunder(db, "__len__", &CallArguments::none()) {
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
            Type::Callable(CallableType::BoundMethod(bound_method)) => {
                let signature = bound_method.function(db).signature(db);
                let signature = CallableSignature::single(self, signature.clone())
                    .with_bound_type(bound_method.self_instance(db));
                Signatures::single(signature)
            }

            Type::Callable(CallableType::MethodWrapperDunderGet(_)) => {
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

                let not_none = Type::none(db).negate(db);
                let signature = CallableSignature::from_overloads(
                    self,
                    [
                        Signature::new(
                            Parameters::new([
                                Parameter::new(
                                    Some(Type::none(db)),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("instance")),
                                        default_ty: None,
                                    },
                                ),
                                Parameter::new(
                                    Some(KnownClass::Type.to_instance(db)),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("owner")),
                                        default_ty: None,
                                    },
                                ),
                            ]),
                            None,
                        ),
                        Signature::new(
                            Parameters::new([
                                Parameter::new(
                                    Some(not_none),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("instance")),
                                        default_ty: None,
                                    },
                                ),
                                Parameter::new(
                                    Some(UnionType::from_elements(
                                        db,
                                        [KnownClass::Type.to_instance(db), Type::none(db)],
                                    )),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("owner")),
                                        default_ty: Some(Type::none(db)),
                                    },
                                ),
                            ]),
                            None,
                        ),
                    ],
                );
                Signatures::single(signature)
            }

            Type::Callable(CallableType::WrapperDescriptorDunderGet) => {
                // Here, we also model `types.FunctionType.__get__`, but now we consider a call to
                // this as a function, i.e. we also expect the `self` argument to be passed in.

                // TODO: Consider merging this signature with the one in the previous match clause,
                // since the previous one is just this signature with the `self` parameters
                // removed.
                let not_none = Type::none(db).negate(db);
                let signature = CallableSignature::from_overloads(
                    self,
                    [
                        Signature::new(
                            Parameters::new([
                                Parameter::new(
                                    Some(KnownClass::FunctionType.to_instance(db)),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("self")),
                                        default_ty: None,
                                    },
                                ),
                                Parameter::new(
                                    Some(Type::none(db)),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("instance")),
                                        default_ty: None,
                                    },
                                ),
                                Parameter::new(
                                    Some(KnownClass::Type.to_instance(db)),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("owner")),
                                        default_ty: None,
                                    },
                                ),
                            ]),
                            None,
                        ),
                        Signature::new(
                            Parameters::new([
                                Parameter::new(
                                    Some(KnownClass::FunctionType.to_instance(db)),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("self")),
                                        default_ty: None,
                                    },
                                ),
                                Parameter::new(
                                    Some(not_none),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("instance")),
                                        default_ty: None,
                                    },
                                ),
                                Parameter::new(
                                    Some(UnionType::from_elements(
                                        db,
                                        [KnownClass::Type.to_instance(db), Type::none(db)],
                                    )),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("owner")),
                                        default_ty: Some(Type::none(db)),
                                    },
                                ),
                            ]),
                            None,
                        ),
                    ],
                );
                Signatures::single(signature)
            }

            Type::FunctionLiteral(function_type) => Signatures::single(CallableSignature::single(
                self,
                function_type.signature(db).clone(),
            )),

            Type::ClassLiteral(ClassLiteralType { class }) => match class.known(db) {
                Some(KnownClass::Bool) => {
                    // ```py
                    // class bool(int):
                    //     def __new__(cls, o: object = ..., /) -> Self: ...
                    // ```
                    let signature = CallableSignature::single(
                        self,
                        Signature::new(
                            Parameters::new([Parameter::new(
                                Some(Type::any()),
                                ParameterKind::PositionalOnly {
                                    name: Some(Name::new_static("o")),
                                    default_ty: Some(Type::BooleanLiteral(false)),
                                },
                            )]),
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
                                Parameters::new([Parameter::new(
                                    Some(Type::any()),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("o")),
                                        default_ty: Some(Type::string_literal(db, "")),
                                    },
                                )]),
                                Some(KnownClass::Str.to_instance(db)),
                            ),
                            Signature::new(
                                Parameters::new([
                                    Parameter::new(
                                        Some(Type::any()), // TODO: ReadableBuffer
                                        ParameterKind::PositionalOnly {
                                            name: Some(Name::new_static("o")),
                                            default_ty: None,
                                        },
                                    ),
                                    Parameter::new(
                                        Some(KnownClass::Str.to_instance(db)),
                                        ParameterKind::PositionalOnly {
                                            name: Some(Name::new_static("encoding")),
                                            default_ty: None,
                                        },
                                    ),
                                    Parameter::new(
                                        Some(KnownClass::Str.to_instance(db)),
                                        ParameterKind::PositionalOnly {
                                            name: Some(Name::new_static("errors")),
                                            default_ty: None,
                                        },
                                    ),
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
                                Parameters::new([Parameter::new(
                                    Some(Type::any()),
                                    ParameterKind::PositionalOnly {
                                        name: Some(Name::new_static("o")),
                                        default_ty: None,
                                    },
                                )]),
                                Some(KnownClass::Type.to_instance(db)),
                            ),
                            Signature::new(
                                Parameters::new([
                                    Parameter::new(
                                        Some(Type::any()),
                                        ParameterKind::PositionalOnly {
                                            name: Some(Name::new_static("o")),
                                            default_ty: None,
                                        },
                                    ),
                                    Parameter::new(
                                        Some(Type::any()),
                                        ParameterKind::PositionalOnly {
                                            name: Some(Name::new_static("bases")),
                                            default_ty: None,
                                        },
                                    ),
                                    Parameter::new(
                                        Some(Type::any()),
                                        ParameterKind::PositionalOnly {
                                            name: Some(Name::new_static("dict")),
                                            default_ty: None,
                                        },
                                    ),
                                ]),
                                Some(KnownClass::Type.to_instance(db)),
                            ),
                        ],
                    );
                    Signatures::single(signature)
                }

                // TODO annotated return type on `__new__` or metaclass `__call__`
                // TODO check call vs signatures of `__new__` and/or `__init__`
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
                        MemberLookupPolicy::NoInstanceFallback,
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
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<Bindings<'db>, CallError<'db>> {
        let signatures = self.signatures(db);
        let mut bindings = Bindings::bind(db, &signatures, arguments)?;
        for binding in &mut bindings {
            // For certain known callables, we have special-case logic to determine the return type
            // in a way that isn't directly expressible in the type system. Each special case
            // listed here should have a corresponding clause above in `signatures`.
            let binding_type = binding.callable_type;
            let Some((overload_index, overload)) = binding.matching_overload_mut() else {
                continue;
            };

            match binding_type {
                Type::Callable(CallableType::MethodWrapperDunderGet(function)) => {
                    if function.has_known_class_decorator(db, KnownClass::Classmethod)
                        && function.decorators(db).len() == 1
                    {
                        if let Some(owner) = arguments.second_argument() {
                            overload.set_return_type(Type::Callable(CallableType::BoundMethod(
                                BoundMethodType::new(db, function, owner),
                            )));
                        } else if let Some(instance) = arguments.first_argument() {
                            overload.set_return_type(Type::Callable(CallableType::BoundMethod(
                                BoundMethodType::new(db, function, instance.to_meta_type(db)),
                            )));
                        }
                    } else if let Some(first) = arguments.first_argument() {
                        if first.is_none(db) {
                            overload.set_return_type(Type::FunctionLiteral(function));
                        } else {
                            overload.set_return_type(Type::Callable(CallableType::BoundMethod(
                                BoundMethodType::new(db, function, first),
                            )));
                        }
                    }
                }

                Type::Callable(CallableType::WrapperDescriptorDunderGet) => {
                    if let Some(function_ty @ Type::FunctionLiteral(function)) =
                        arguments.first_argument()
                    {
                        if function.has_known_class_decorator(db, KnownClass::Classmethod)
                            && function.decorators(db).len() == 1
                        {
                            if let Some(owner) = arguments.third_argument() {
                                overload.set_return_type(Type::Callable(
                                    CallableType::BoundMethod(BoundMethodType::new(
                                        db, function, owner,
                                    )),
                                ));
                            } else if let Some(instance) = arguments.second_argument() {
                                overload.set_return_type(Type::Callable(
                                    CallableType::BoundMethod(BoundMethodType::new(
                                        db,
                                        function,
                                        instance.to_meta_type(db),
                                    )),
                                ));
                            }
                        } else {
                            match (arguments.second_argument(), arguments.third_argument()) {
                                (Some(instance), _) if instance.is_none(db) => {
                                    overload.set_return_type(function_ty);
                                }

                                (
                                    Some(Type::KnownInstance(KnownInstanceType::TypeAliasType(
                                        type_alias,
                                    ))),
                                    Some(Type::ClassLiteral(ClassLiteralType { class })),
                                ) if class.is_known(db, KnownClass::TypeAliasType)
                                    && function.name(db) == "__name__" =>
                                {
                                    overload.set_return_type(Type::string_literal(
                                        db,
                                        type_alias.name(db),
                                    ));
                                }

                                (
                                    Some(Type::KnownInstance(KnownInstanceType::TypeVar(typevar))),
                                    Some(Type::ClassLiteral(ClassLiteralType { class })),
                                ) if class.is_known(db, KnownClass::TypeVar)
                                    && function.name(db) == "__name__" =>
                                {
                                    overload.set_return_type(Type::string_literal(
                                        db,
                                        typevar.name(db),
                                    ));
                                }

                                (Some(_), _)
                                    if function
                                        .has_known_class_decorator(db, KnownClass::Property) =>
                                {
                                    overload.set_return_type(todo_type!("@property"));
                                }

                                (Some(instance), _) => {
                                    overload.set_return_type(Type::Callable(
                                        CallableType::BoundMethod(BoundMethodType::new(
                                            db, function, instance,
                                        )),
                                    ));
                                }

                                (None, _) => {}
                            }
                        }
                    }
                }

                Type::FunctionLiteral(function_type) => match function_type.known(db) {
                    Some(KnownFunction::IsEquivalentTo) => {
                        if let [ty_a, ty_b] = overload.parameter_types() {
                            overload.set_return_type(Type::BooleanLiteral(
                                ty_a.is_equivalent_to(db, *ty_b),
                            ));
                        }
                    }

                    Some(KnownFunction::IsSubtypeOf) => {
                        if let [ty_a, ty_b] = overload.parameter_types() {
                            overload.set_return_type(Type::BooleanLiteral(
                                ty_a.is_subtype_of(db, *ty_b),
                            ));
                        }
                    }

                    Some(KnownFunction::IsAssignableTo) => {
                        if let [ty_a, ty_b] = overload.parameter_types() {
                            overload.set_return_type(Type::BooleanLiteral(
                                ty_a.is_assignable_to(db, *ty_b),
                            ));
                        }
                    }

                    Some(KnownFunction::IsDisjointFrom) => {
                        if let [ty_a, ty_b] = overload.parameter_types() {
                            overload.set_return_type(Type::BooleanLiteral(
                                ty_a.is_disjoint_from(db, *ty_b),
                            ));
                        }
                    }

                    Some(KnownFunction::IsGradualEquivalentTo) => {
                        if let [ty_a, ty_b] = overload.parameter_types() {
                            overload.set_return_type(Type::BooleanLiteral(
                                ty_a.is_gradual_equivalent_to(db, *ty_b),
                            ));
                        }
                    }

                    Some(KnownFunction::IsFullyStatic) => {
                        if let [ty] = overload.parameter_types() {
                            overload.set_return_type(Type::BooleanLiteral(ty.is_fully_static(db)));
                        }
                    }

                    Some(KnownFunction::IsSingleton) => {
                        if let [ty] = overload.parameter_types() {
                            overload.set_return_type(Type::BooleanLiteral(ty.is_singleton(db)));
                        }
                    }

                    Some(KnownFunction::IsSingleValued) => {
                        if let [ty] = overload.parameter_types() {
                            overload.set_return_type(Type::BooleanLiteral(ty.is_single_valued(db)));
                        }
                    }

                    Some(KnownFunction::Len) => {
                        if let [first_arg] = overload.parameter_types() {
                            if let Some(len_ty) = first_arg.len(db) {
                                overload.set_return_type(len_ty);
                            }
                        };
                    }

                    Some(KnownFunction::Repr) => {
                        if let [first_arg] = overload.parameter_types() {
                            overload.set_return_type(first_arg.repr(db));
                        };
                    }

                    Some(KnownFunction::Cast) => {
                        // TODO: Use `.parameter_types()` exclusively when overloads are supported.
                        if let Some(casted_ty) = arguments.first_argument() {
                            if let [_, _] = overload.parameter_types() {
                                overload.set_return_type(casted_ty);
                            }
                        };
                    }

                    Some(KnownFunction::Overload) => {
                        overload.set_return_type(todo_type!("overload(..) return type"));
                    }

                    Some(KnownFunction::GetattrStatic) => {
                        let [instance_ty, attr_name, default] = overload.parameter_types() else {
                            continue;
                        };

                        let Some(attr_name) = attr_name.into_string_literal() else {
                            continue;
                        };

                        let default = if default.is_unknown() {
                            Type::Never
                        } else {
                            *default
                        };

                        let union_with_default = |ty| UnionType::from_elements(db, [ty, default]);

                        // TODO: we could emit a diagnostic here (if default is not set)
                        overload.set_return_type(
                            match instance_ty.static_member(db, attr_name.value(db)) {
                                Symbol::Type(ty, Boundness::Bound) => {
                                    if instance_ty.is_fully_static(db) {
                                        ty
                                    } else {
                                        // Here, we attempt to model the fact that an attribute lookup on
                                        // a non-fully static type could fail. This is an approximation,
                                        // as there are gradual types like `tuple[Any]`, on which a lookup
                                        // of (e.g. of the `index` method) would always succeed.

                                        union_with_default(ty)
                                    }
                                }
                                Symbol::Type(ty, Boundness::PossiblyUnbound) => {
                                    union_with_default(ty)
                                }
                                Symbol::Unbound => default,
                            },
                        );
                    }

                    _ => {}
                },

                Type::ClassLiteral(ClassLiteralType { class }) => match class.known(db) {
                    Some(KnownClass::Bool) => {
                        overload.set_return_type(
                            arguments
                                .first_argument()
                                .map(|arg| arg.bool(db).into_type(db))
                                .unwrap_or(Type::BooleanLiteral(false)),
                        );
                    }

                    Some(KnownClass::Str) if overload_index == 0 => {
                        overload.set_return_type(
                            arguments
                                .first_argument()
                                .map(|arg| arg.str(db))
                                .unwrap_or_else(|| Type::string_literal(db, "")),
                        );
                    }

                    Some(KnownClass::Type) if overload_index == 0 => {
                        if let Some(arg) = arguments.first_argument() {
                            overload.set_return_type(arg.to_meta_type(db));
                        }
                    }

                    _ => {}
                },

                // Not a special case
                _ => {}
            }
        }

        Ok(bindings)
    }

    /// Look up a dunder method on the meta-type of `self` and call it.
    ///
    /// Returns an `Err` if the dunder method can't be called,
    /// or the given arguments are not valid.
    fn try_call_dunder(
        self,
        db: &'db dyn Db,
        name: &str,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        match self
            .member_lookup_with_policy(db, name.into(), MemberLookupPolicy::NoInstanceFallback)
            .symbol
        {
            Symbol::Type(dunder_callable, boundness) => {
                let signatures = dunder_callable.signatures(db);
                let bindings = Bindings::bind(db, &signatures, arguments)?;
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
                &CallArguments::positional([KnownClass::Int.to_instance(db)]),
            )
            .map(|dunder_getitem_outcome| dunder_getitem_outcome.return_type(db))
        };

        let try_call_dunder_next_on_iterator = |iterator: Type<'db>| {
            iterator
                .try_call_dunder(db, "__next__", &CallArguments::none())
                .map(|dunder_next_outcome| dunder_next_outcome.return_type(db))
        };

        let dunder_iter_result = self
            .try_call_dunder(db, "__iter__", &CallArguments::none())
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
        let enter = self.try_call_dunder(db, "__enter__", &CallArguments::none());
        let exit = self.try_call_dunder(
            db,
            "__exit__",
            &CallArguments::positional([Type::none(db), Type::none(db), Type::none(db)]),
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
            | Type::Instance(_)
            | Type::KnownInstance(_)
            | Type::ModuleLiteral(_)
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::SliceLiteral(_)
            | Type::Tuple(_)
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
            | Type::Callable(_)
            | Type::Never
            | Type::FunctionLiteral(_) => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec![InvalidTypeExpression::InvalidType(*self)],
                fallback_type: Type::unknown(),
            }),

            // We treat `typing.Type` exactly the same as `builtins.type`:
            Type::KnownInstance(KnownInstanceType::Type) => Ok(KnownClass::Type.to_instance(db)),
            Type::KnownInstance(KnownInstanceType::Tuple) => Ok(KnownClass::Tuple.to_instance(db)),

            // Legacy `typing` aliases
            Type::KnownInstance(KnownInstanceType::List) => Ok(KnownClass::List.to_instance(db)),
            Type::KnownInstance(KnownInstanceType::Dict) => Ok(KnownClass::Dict.to_instance(db)),
            Type::KnownInstance(KnownInstanceType::Set) => Ok(KnownClass::Set.to_instance(db)),
            Type::KnownInstance(KnownInstanceType::FrozenSet) => {
                Ok(KnownClass::FrozenSet.to_instance(db))
            }
            Type::KnownInstance(KnownInstanceType::ChainMap) => {
                Ok(KnownClass::ChainMap.to_instance(db))
            }
            Type::KnownInstance(KnownInstanceType::Counter) => {
                Ok(KnownClass::Counter.to_instance(db))
            }
            Type::KnownInstance(KnownInstanceType::DefaultDict) => {
                Ok(KnownClass::DefaultDict.to_instance(db))
            }
            Type::KnownInstance(KnownInstanceType::Deque) => Ok(KnownClass::Deque.to_instance(db)),
            Type::KnownInstance(KnownInstanceType::OrderedDict) => {
                Ok(KnownClass::OrderedDict.to_instance(db))
            }

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
            // TODO map this to a new `Type::TypeVar` variant
            Type::KnownInstance(KnownInstanceType::TypeVar(_)) => Ok(*self),
            Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) => {
                Ok(alias.value_type(db))
            }
            Type::KnownInstance(KnownInstanceType::Never | KnownInstanceType::NoReturn) => {
                Ok(Type::Never)
            }
            Type::KnownInstance(KnownInstanceType::LiteralString) => Ok(Type::LiteralString),
            Type::KnownInstance(KnownInstanceType::Any) => Ok(Type::any()),
            Type::KnownInstance(KnownInstanceType::Annotated) => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec![InvalidTypeExpression::BareAnnotated],
                fallback_type: Type::unknown(),
            }),
            Type::KnownInstance(KnownInstanceType::ClassVar) => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec![
                    InvalidTypeExpression::ClassVarInTypeExpression
                ],
                fallback_type: Type::unknown(),
            }),
            Type::KnownInstance(KnownInstanceType::Final) => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec![
                    InvalidTypeExpression::FinalInTypeExpression
                ],
                fallback_type: Type::unknown(),
            }),
            Type::KnownInstance(KnownInstanceType::Literal) => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec![InvalidTypeExpression::BareLiteral],
                fallback_type: Type::unknown(),
            }),
            Type::KnownInstance(KnownInstanceType::Unknown) => Ok(Type::unknown()),
            Type::KnownInstance(KnownInstanceType::AlwaysTruthy) => Ok(Type::AlwaysTruthy),
            Type::KnownInstance(KnownInstanceType::AlwaysFalsy) => Ok(Type::AlwaysFalsy),
            Type::KnownInstance(KnownInstanceType::Callable) => {
                // TODO: Use an opt-in rule for a bare `Callable`
                Ok(Type::Callable(CallableType::General(
                    GeneralCallableType::unknown(db),
                )))
            }
            Type::KnownInstance(_) => Ok(todo_type!(
                "Invalid or unsupported `KnownInstanceType` in `Type::to_type_expression`"
            )),
            Type::Instance(_) => Ok(todo_type!(
                "Invalid or unsupported `Instance` in `Type::to_type_expression`"
            )),
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
            Type::Union(union) => union.map(db, |ty| ty.to_meta_type(db)),
            Type::BooleanLiteral(_) => KnownClass::Bool.to_class_literal(db),
            Type::BytesLiteral(_) => KnownClass::Bytes.to_class_literal(db),
            Type::SliceLiteral(_) => KnownClass::Slice.to_class_literal(db),
            Type::IntLiteral(_) => KnownClass::Int.to_class_literal(db),
            Type::FunctionLiteral(_) => KnownClass::FunctionType.to_class_literal(db),
            Type::Callable(CallableType::BoundMethod(_)) => {
                KnownClass::MethodType.to_class_literal(db)
            }
            Type::Callable(CallableType::MethodWrapperDunderGet(_)) => {
                KnownClass::MethodWrapperType.to_class_literal(db)
            }
            Type::Callable(CallableType::WrapperDescriptorDunderGet) => {
                KnownClass::WrapperDescriptorType.to_class_literal(db)
            }
            Type::Callable(CallableType::General(_)) => KnownClass::Type.to_instance(db),
            Type::ModuleLiteral(_) => KnownClass::ModuleType.to_class_literal(db),
            Type::Tuple(_) => KnownClass::Tuple.to_class_literal(db),
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
    /// `x: Annotated` is invalid as an annotation
    BareAnnotated,
    /// `x: Literal` is invalid as an annotation
    BareLiteral,
    /// The `ClassVar` type qualifier was used in a type expression
    ClassVarInTypeExpression,
    /// The `Final` type qualifier was used in a type expression
    FinalInTypeExpression,
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
                    InvalidTypeExpression::BareAnnotated => f.write_str(
                        "`Annotated` requires at least two arguments when used in an annotation or type expression"
                    ),
                    InvalidTypeExpression::BareLiteral => f.write_str(
                        "`Literal` requires at least one argument when used in a type expression"
                    ),
                    InvalidTypeExpression::ClassVarInTypeExpression => f.write_str(
                        "Type qualifier `typing.ClassVar` is not allowed in type expressions (only in annotation expressions)"
                    ),
                    InvalidTypeExpression::FinalInTypeExpression => f.write_str(
                        "Type qualifier `typing.Final` is not allowed in type expressions (only in annotation expressions)"
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
/// runtime `typing.TypeVar` object itself). In the future, it will also be referenced also by a
/// new `Type` variant to represent the type that this typevar represents as an annotation: that
/// is, an unknown set of objects, constrained by the upper-bound/constraints on this type var,
/// defaulting to the default type of this type var when not otherwise bound to a type.
///
/// This must be a tracked struct, not an interned one, because typevar equivalence is by identity,
/// not by value. Two typevars that have the same name, bound/constraints, and default, are still
/// different typevars: if used in the same scope, they may be bound to different types.
#[salsa::tracked(debug)]
pub struct TypeVarInstance<'db> {
    /// The name of this TypeVar (e.g. `T`)
    #[return_ref]
    name: ast::name::Name,

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
    Constraints(TupleType<'db>),
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
                .try_call_dunder(db, "__next__", &CallArguments::none())
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
                dunder_iter_type = bindings.callable_type.display(db),
            )),
            Self::IterCallError(CallErrorKind::PossiblyNotCallable, bindings) if bindings.is_single() => {
                report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because its `__iter__` attribute (with type `{dunder_iter_type}`) \
                        may not be callable",
                    iterable_type = iterable_type.display(db),
                    dunder_iter_type = bindings.callable_type.display(db),
                ));
            }
            Self::IterCallError(CallErrorKind::PossiblyNotCallable, bindings) => {
                report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because its `__iter__` attribute (with type `{dunder_iter_type}`) \
                        may not be callable",
                    iterable_type = iterable_type.display(db),
                    dunder_iter_type = bindings.callable_type.display(db),
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
                dunder_iter_type = bindings.callable_type.display(db),
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
                    dunder_getitem_type = bindings.callable_type.display(db),
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
                        dunder_getitem_type = bindings.callable_type.display(db),
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
                    dunder_getitem_type = bindings.callable_type.display(db),
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
                    dunder_getitem_type = bindings.callable_type.display(db),
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
                        dunder_getitem_type = bindings.callable_type.display(db),
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
                    dunder_getitem_type = bindings.callable_type.display(db),
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

#[salsa::interned(debug)]
pub struct FunctionType<'db> {
    /// name of the function at definition
    #[return_ref]
    pub name: ast::name::Name,

    /// Is this a function that we special-case somehow? If so, which one?
    known: Option<KnownFunction>,

    body_scope: ScopeId<'db>,

    /// types of all decorators on this function
    decorators: Box<[Type<'db>]>,
}

#[salsa::tracked]
impl<'db> FunctionType<'db> {
    pub fn has_known_class_decorator(self, db: &dyn Db, decorator: KnownClass) -> bool {
        self.decorators(db).iter().any(|d| {
            d.into_class_literal()
                .is_some_and(|c| c.class.is_known(db, decorator))
        })
    }

    pub fn has_known_function_decorator(self, db: &dyn Db, decorator: KnownFunction) -> bool {
        self.decorators(db).iter().any(|d| {
            d.into_function_literal()
                .is_some_and(|f| f.is_known(db, decorator))
        })
    }

    /// Convert the `FunctionType` into a [`Type::Callable`].
    ///
    /// Returns `None` if the function is overloaded. This powers the `CallableTypeFromFunction`
    /// special form from the `knot_extensions` module.
    pub(crate) fn into_callable_type(self, db: &'db dyn Db) -> Type<'db> {
        // TODO: Add support for overloaded callables
        Type::Callable(CallableType::General(GeneralCallableType::new(
            db,
            self.signature(db).clone(),
        )))
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
    pub fn signature(self, db: &'db dyn Db) -> Signature<'db> {
        let internal_signature = self.internal_signature(db);

        let decorators = self.decorators(db);
        let mut decorators = decorators.iter();

        if let Some(d) = decorators.next() {
            if d.into_class_literal()
                .is_some_and(|c| c.class.is_known(db, KnownClass::Classmethod))
                && decorators.next().is_none()
            {
                internal_signature
            } else {
                Signature::todo("return type of decorated function")
            }
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
        let definition = semantic_index(db, scope.file(db)).definition(function_stmt_node);
        Signature::from_function(db, definition, function_stmt_node)
    }

    pub fn is_known(self, db: &'db dyn Db, known_function: KnownFunction) -> bool {
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

    /// Return the [`ParameterExpectations`] for this function.
    const fn parameter_expectations(self) -> ParameterExpectations {
        match self {
            Self::IsFullyStatic | Self::IsSingleton | Self::IsSingleValued => {
                ParameterExpectations::SingleTypeExpression
            }

            Self::IsEquivalentTo
            | Self::IsSubtypeOf
            | Self::IsAssignableTo
            | Self::IsDisjointFrom
            | Self::IsGradualEquivalentTo => ParameterExpectations::TwoTypeExpressions,

            Self::AssertType => ParameterExpectations::ValueExpressionAndTypeExpression,
            Self::Cast => ParameterExpectations::TypeExpressionAndValueExpression,

            Self::IsInstance
            | Self::IsSubclass
            | Self::Len
            | Self::Repr
            | Self::Overload
            | Self::Final
            | Self::NoTypeCheck
            | Self::RevealType
            | Self::GetattrStatic
            | Self::StaticAssert => ParameterExpectations::AllValueExpressions,
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

/// This type represents a general callable type that are used to represent `typing.Callable`
/// and `lambda` expressions.
#[salsa::interned(debug)]
pub struct GeneralCallableType<'db> {
    #[return_ref]
    signature: Signature<'db>,
}

impl<'db> GeneralCallableType<'db> {
    /// Create a general callable type which accepts any parameters and returns an `Unknown` type.
    pub(crate) fn unknown(db: &'db dyn Db) -> Self {
        GeneralCallableType::new(
            db,
            Signature::new(Parameters::unknown(), Some(Type::unknown())),
        )
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
        let self_signature = self.signature(db);
        let other_signature = other.signature(db);

        if self_signature.parameters().len() != other_signature.parameters().len() {
            return false;
        }

        // Check gradual equivalence between the two optional types. In the context of a callable
        // type, the `None` type represents an `Unknown` type.
        let are_optional_types_gradually_equivalent =
            |self_type: Option<Type<'db>>, other_type: Option<Type<'db>>| {
                self_type
                    .unwrap_or(Type::unknown())
                    .is_gradual_equivalent_to(db, other_type.unwrap_or(Type::unknown()))
            };

        if !are_optional_types_gradually_equivalent(
            self_signature.return_ty,
            other_signature.return_ty,
        ) {
            return false;
        }

        // N.B. We don't need to explicitly check for the use of gradual form (`...`) in the
        // parameters because it is internally represented by adding `*Any` and `**Any` to the
        // parameter list.
        self_signature
            .parameters()
            .iter()
            .zip(other_signature.parameters().iter())
            .all(|(self_param, other_param)| {
                are_optional_types_gradually_equivalent(
                    self_param.annotated_type(),
                    other_param.annotated_type(),
                )
            })
    }
}

/// A type that represents callable objects.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, salsa::Update)]
pub enum CallableType<'db> {
    /// Represents a general callable type.
    General(GeneralCallableType<'db>),

    /// Represents a callable `instance.method` where `instance` is an instance of a class
    /// and `method` is a method (of that class).
    ///
    /// See [`BoundMethodType`] for more information.
    ///
    /// TODO: This could eventually be replaced by a more general `Callable` type, if we
    /// decide to bind the first argument of method calls early, i.e. if we have a method
    /// `def f(self, x: int) -> str`, and see it being called as `instance.f`, we could
    /// partially apply (and check) the `instance` argument against the `self` parameter,
    /// and return a `Callable[[int], str]`. One drawback would be that we could not show
    /// the bound instance when that type is displayed.
    BoundMethod(BoundMethodType<'db>),

    /// Represents the callable `f.__get__` where `f` is a function.
    ///
    /// TODO: This could eventually be replaced by a more general `Callable` type that is
    /// also able to represent overloads. It would need to represent the two overloads of
    /// `types.FunctionType.__get__`:
    ///
    /// ```txt
    ///  * (None,   type)         ->  Literal[function_on_which_it_was_called]
    ///  * (object, type | None)  ->  BoundMethod[instance, function_on_which_it_was_called]
    /// ```
    MethodWrapperDunderGet(FunctionType<'db>),

    /// Represents the callable `FunctionType.__get__`.
    ///
    /// TODO: Similar to above, this could eventually be replaced by a generic `Callable`
    /// type. We currently add this as a separate variant because `FunctionType.__get__`
    /// is an overloaded method and we do not support `@overload` yet.
    WrapperDescriptorDunderGet,
}

/// Describes whether the parameters in a function expect value expressions or type expressions.
///
/// Whether a specific parameter in the function expects a type expression can be queried
/// using [`ParameterExpectations::expectation_at_index`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
enum ParameterExpectations {
    /// All parameters in the function expect value expressions
    #[default]
    AllValueExpressions,
    /// The first parameter in the function expects a type expression
    SingleTypeExpression,
    /// The first two parameters in the function expect type expressions
    TwoTypeExpressions,
    /// The first parameter in the function expects a value expression,
    /// and the second expects a type expression
    ValueExpressionAndTypeExpression,
    /// The first parameter in the function expects a type expression,
    /// and the second expects a value expression
    TypeExpressionAndValueExpression,
}

impl ParameterExpectations {
    /// Query whether the parameter at `parameter_index` expects a value expression or a type expression
    fn expectation_at_index(self, parameter_index: usize) -> ParameterExpectation {
        match self {
            Self::AllValueExpressions => ParameterExpectation::ValueExpression,
            Self::SingleTypeExpression | Self::TypeExpressionAndValueExpression => {
                if parameter_index == 0 {
                    ParameterExpectation::TypeExpression
                } else {
                    ParameterExpectation::ValueExpression
                }
            }
            Self::TwoTypeExpressions => {
                if parameter_index < 2 {
                    ParameterExpectation::TypeExpression
                } else {
                    ParameterExpectation::ValueExpression
                }
            }
            Self::ValueExpressionAndTypeExpression => {
                if parameter_index == 1 {
                    ParameterExpectation::TypeExpression
                } else {
                    ParameterExpectation::ValueExpression
                }
            }
        }
    }
}

/// Whether a single parameter in a given function expects a value expression or a [type expression]
///
/// [type expression]: https://typing.readthedocs.io/en/latest/spec/annotations.html#type-and-annotation-expressions
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
enum ParameterExpectation {
    /// The parameter expects a value expression
    #[default]
    ValueExpression,
    /// The parameter expects a type expression
    TypeExpression,
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
    #[salsa::tracked]
    pub fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        let scope = self.rhs_scope(db);

        let type_alias_stmt_node = scope.node(db).expect_type_alias();
        let definition = semantic_index(db, scope.file(db)).definition(type_alias_stmt_node);

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

    pub fn is_fully_static(self, db: &'db dyn Db) -> bool {
        self.elements(db).iter().all(|ty| ty.is_fully_static(db))
    }

    /// Create a new union type with the elements sorted according to a canonical ordering.
    #[must_use]
    pub fn to_sorted_union(self, db: &'db dyn Db) -> Self {
        let mut new_elements: Vec<Type<'db>> = self
            .elements(db)
            .iter()
            .map(|element| element.with_sorted_unions_and_intersections(db))
            .collect();
        new_elements.sort_unstable_by(|l, r| union_or_intersection_elements_ordering(db, l, r));
        UnionType::new(db, new_elements.into_boxed_slice())
    }

    /// Return `true` if `self` represents the exact same set of possible runtime objects as `other`
    pub fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
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

        let sorted_self = self.to_sorted_union(db);

        if sorted_self == other {
            return true;
        }

        sorted_self == other.to_sorted_union(db)
    }

    /// Return `true` if `self` has exactly the same set of possible static materializations as `other`
    /// (if `self` represents the same set of possible sets of possible runtime objects as `other`)
    pub fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }

        // TODO: `T | Unknown` should be gradually equivalent to `T | Unknown | Any`,
        // since they have exactly the same set of possible static materializations
        // (they represent the same set of possible sets of possible runtime objects)
        if self.elements(db).len() != other.elements(db).len() {
            return false;
        }

        let sorted_self = self.to_sorted_union(db);

        if sorted_self == other {
            return true;
        }

        let sorted_other = other.to_sorted_union(db);

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
    /// according to a canonical ordering.
    #[must_use]
    pub fn to_sorted_intersection(self, db: &'db dyn Db) -> Self {
        fn normalized_set<'db>(
            db: &'db dyn Db,
            elements: &FxOrderSet<Type<'db>>,
        ) -> FxOrderSet<Type<'db>> {
            let mut elements: FxOrderSet<Type<'db>> = elements
                .iter()
                .map(|ty| ty.with_sorted_unions_and_intersections(db))
                .collect();

            elements.sort_unstable_by(|l, r| union_or_intersection_elements_ordering(db, l, r));
            elements
        }

        IntersectionType::new(
            db,
            normalized_set(db, self.positive(db)),
            normalized_set(db, self.negative(db)),
        )
    }

    pub fn is_fully_static(self, db: &'db dyn Db) -> bool {
        self.positive(db).iter().all(|ty| ty.is_fully_static(db))
            && self.negative(db).iter().all(|ty| ty.is_fully_static(db))
    }

    /// Return `true` if `self` represents exactly the same set of possible runtime objects as `other`
    pub fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
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

        let sorted_self = self.to_sorted_intersection(db);

        if sorted_self == other {
            return true;
        }

        sorted_self == other.to_sorted_intersection(db)
    }

    /// Return `true` if `self` has exactly the same set of possible static materializations as `other`
    /// (if `self` represents the same set of possible sets of possible runtime objects as `other`)
    pub fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }

        if self.positive(db).len() != other.positive(db).len()
            || self.negative(db).len() != other.negative(db).len()
        {
            return false;
        }

        let sorted_self = self.to_sorted_intersection(db);

        if sorted_self == other {
            return true;
        }

        let sorted_other = other.to_sorted_intersection(db);

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
}

#[salsa::interned(debug)]
pub struct StringLiteralType<'db> {
    #[return_ref]
    value: Box<str>,
}

impl<'db> StringLiteralType<'db> {
    /// The length of the string, as would be returned by Python's `len()`.
    pub fn python_len(&self, db: &'db dyn Db) -> usize {
        self.value(db).chars().count()
    }
}

#[salsa::interned(debug)]
pub struct BytesLiteralType<'db> {
    #[return_ref]
    value: Box<[u8]>,
}

impl<'db> BytesLiteralType<'db> {
    pub fn python_len(&self, db: &'db dyn Db) -> usize {
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
    pub fn from_elements<T: Into<Type<'db>>>(
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

    /// Return a normalized version of `self` in which all unions and intersections are sorted
    /// according to a canonical order, no matter how "deeply" a union/intersection may be nested.
    #[must_use]
    pub fn with_sorted_unions_and_intersections(self, db: &'db dyn Db) -> Self {
        let elements: Box<[Type<'db>]> = self
            .elements(db)
            .iter()
            .map(|ty| ty.with_sorted_unions_and_intersections(db))
            .collect();
        TupleType::new(db, elements)
    }

    pub fn is_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
        let self_elements = self.elements(db);
        let other_elements = other.elements(db);
        self_elements.len() == other_elements.len()
            && self_elements
                .iter()
                .zip(other_elements)
                .all(|(self_ty, other_ty)| self_ty.is_equivalent_to(db, *other_ty))
    }

    pub fn is_gradual_equivalent_to(self, db: &'db dyn Db, other: Self) -> bool {
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
        let todo3 = todo_type!();
        let todo4 = todo_type!();

        let int = KnownClass::Int.to_instance(&db);

        assert!(int.is_assignable_to(&db, todo1));
        assert!(int.is_assignable_to(&db, todo3));

        assert!(todo1.is_assignable_to(&db, int));
        assert!(todo3.is_assignable_to(&db, int));

        // We lose information when combining several `Todo` types. This is an
        // acknowledged limitation of the current implementation. We can not
        // easily store the meta information of several `Todo`s in a single
        // variant, as `TodoType` needs to implement `Copy`, meaning it can't
        // contain `Vec`/`Box`/etc., and can't be boxed itself.
        //
        // Lifting this restriction would require us to intern `TodoType` in
        // salsa, but that would mean we would have to pass in `db` everywhere.

        // A union of several `Todo` types collapses to a single `Todo` type:
        assert!(UnionType::from_elements(&db, vec![todo1, todo2, todo3, todo4]).is_todo());

        // And similar for intersection types:
        assert!(IntersectionBuilder::new(&db)
            .add_positive(todo1)
            .add_positive(todo2)
            .add_positive(todo3)
            .add_positive(todo4)
            .build()
            .is_todo());
        assert!(IntersectionBuilder::new(&db)
            .add_positive(todo1)
            .add_negative(todo2)
            .add_positive(todo3)
            .add_negative(todo4)
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

            let function_body_scope = known_module_symbol(&db, module, function_name)
                .symbol
                .expect_type()
                .expect_function_literal()
                .body_scope(&db);

            let function_node = function_body_scope.node(&db).expect_function();

            let function_definition =
                semantic_index(&db, function_body_scope.file(&db)).definition(function_node);

            assert_eq!(
                KnownFunction::try_from_definition_and_name(&db, function_definition, function_name),
                Some(function),
                "The strum `EnumString` implementation appears to be incorrect for `{function_name}`"
            );
        }
    }
}
