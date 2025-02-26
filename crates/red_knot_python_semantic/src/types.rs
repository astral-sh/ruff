use std::hash::Hash;
use std::str::FromStr;

use bitflags::bitflags;
use call::{CallDunderError, CallError};
use context::InferContext;
use diagnostic::NOT_ITERABLE;
use ruff_db::files::File;
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};
use type_ordering::union_elements_ordering;

pub(crate) use self::builder::{IntersectionBuilder, UnionBuilder};
pub(crate) use self::diagnostic::register_lints;
pub use self::diagnostic::{TypeCheckDiagnostic, TypeCheckDiagnostics};
pub(crate) use self::display::TypeArrayDisplay;
pub(crate) use self::infer::{
    infer_deferred_types, infer_definition_types, infer_expression_type, infer_expression_types,
    infer_scope_types,
};
pub use self::narrow::KnownConstraintFunction;
pub(crate) use self::signatures::Signature;
pub use self::subclass_of::SubclassOfType;
use crate::module_name::ModuleName;
use crate::module_resolver::{file_to_module, resolve_module, KnownModule};
use crate::semantic_index::ast_ids::HasScopedExpressionId;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::symbol::ScopeId;
use crate::semantic_index::{imported_modules, semantic_index};
use crate::suppression::check_suppressions;
use crate::symbol::{imported_symbol, Boundness, Symbol, SymbolAndQualifiers};
use crate::types::call::{bind_call, CallArguments, CallBinding, CallOutcome, UnionCallError};
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
    let _span = tracing::trace_span!("check_types", file=?file.path(db)).entered();

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
    pub fn with_sorted_unions(self, db: &'db dyn Db) -> Self {
        match self {
            Type::Union(union) => Type::Union(union.to_sorted_union(db)),
            Type::Intersection(intersection) => {
                Type::Intersection(intersection.to_sorted_intersection(db))
            }
            Type::Tuple(tuple) => Type::Tuple(tuple.with_sorted_unions(db)),
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

            (Type::Intersection(self_intersection), Type::Intersection(target_intersection)) => {
                // Check that all target positive values are covered in self positive values
                target_intersection
                    .positive(db)
                    .iter()
                    .all(|&target_pos_elem| {
                        self_intersection
                            .positive(db)
                            .iter()
                            .any(|&self_pos_elem| self_pos_elem.is_subtype_of(db, target_pos_elem))
                    })
                    // Check that all target negative values are excluded in self, either by being
                    // subtypes of a self negative value or being disjoint from a self positive value.
                    && target_intersection
                        .negative(db)
                        .iter()
                        .all(|&target_neg_elem| {
                            // Is target negative value is subtype of a self negative value
                            self_intersection.negative(db).iter().any(|&self_neg_elem| {
                                target_neg_elem.is_subtype_of(db, self_neg_elem)
                            // Is target negative value is disjoint from a self positive value?
                            }) || self_intersection.positive(db).iter().any(|&self_pos_elem| {
                                self_pos_elem.is_disjoint_from(db, target_neg_elem)
                            })
                        })
            }

            (Type::Intersection(intersection), _) => intersection
                .positive(db)
                .iter()
                .any(|&elem_ty| elem_ty.is_subtype_of(db, target)),

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
            (Type::ClassLiteral(ClassLiteralType { class }), _) => class
                .metaclass(db)
                .to_instance(db)
                .is_subtype_of(db, target),

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
                .is_some_and(|class| {
                    class
                        .metaclass(db)
                        .to_instance(db)
                        .is_subtype_of(db, target)
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

            (Type::Intersection(intersection), other)
            | (other, Type::Intersection(intersection)) => {
                if intersection
                    .positive(db)
                    .iter()
                    .any(|p| p.is_disjoint_from(db, other))
                {
                    true
                } else {
                    // TODO we can do better here. For example:
                    // X & ~Literal[1] is disjoint from Literal[1]
                    false
                }
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

            (Type::SubclassOf(subclass_of_ty), other)
            | (other, Type::SubclassOf(subclass_of_ty)) => {
                let metaclass_instance_ty = match subclass_of_ty.subclass_of() {
                    // for `type[Any]`/`type[Unknown]`/`type[Todo]`, we know the type cannot be any larger than `type`,
                    // so although the type is dynamic we can still determine disjointness in some situations
                    ClassBase::Dynamic(_) => KnownClass::Type.to_instance(db),
                    ClassBase::Class(class) => class.metaclass(db).to_instance(db),
                };
                other.is_disjoint_from(db, metaclass_instance_ty)
            }

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
                    .metaclass(db)
                    .to_instance(db)
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
            Type::Tuple(tuple) => tuple
                .elements(db)
                .iter()
                .all(|elem| elem.is_fully_static(db)),
            // TODO: Once we support them, make sure that we return `false` for other types
            // containing gradual forms such as `tuple[Any, ...]` or `Callable[..., str]`.
            // Conversely, make sure to return `true` for homogeneous tuples such as
            // `tuple[int, ...]`, once we add support for them.
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
            | Type::AlwaysFalsy => false,
        }
    }

    /// Access an attribute of this type without invoking the descriptor protocol. This
    /// method corresponds to `inspect.getattr_static(<object of type 'self'>, name)`.
    ///
    /// See also: [`Type::member`]
    #[must_use]
    fn static_member(&self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        match self {
            Type::Dynamic(_) => Symbol::bound(self),

            Type::Never => Symbol::todo("attribute lookup on Never"),

            Type::FunctionLiteral(_) => KnownClass::FunctionType
                .to_instance(db)
                .static_member(db, name),

            Type::Callable(CallableType::BoundMethod(_)) => KnownClass::MethodType
                .to_instance(db)
                .static_member(db, name),
            Type::Callable(CallableType::MethodWrapperDunderGet(_)) => {
                KnownClass::MethodWrapperType
                    .to_instance(db)
                    .static_member(db, name)
            }
            Type::Callable(CallableType::WrapperDescriptorDunderGet) => {
                KnownClass::WrapperDescriptorType
                    .to_instance(db)
                    .static_member(db, name)
            }

            Type::ModuleLiteral(module) => module.static_member(db, name),

            Type::ClassLiteral(class_ty) => class_ty.static_member(db, name),

            Type::SubclassOf(subclass_of_ty) => subclass_of_ty.static_member(db, name),

            Type::KnownInstance(known_instance) => known_instance.static_member(db, name),

            Type::Instance(InstanceType { class }) => match (class.known(db), name) {
                (Some(KnownClass::VersionInfo), "major") => Symbol::bound(Type::IntLiteral(
                    Program::get(db).python_version(db).major.into(),
                )),
                (Some(KnownClass::VersionInfo), "minor") => Symbol::bound(Type::IntLiteral(
                    Program::get(db).python_version(db).minor.into(),
                )),
                (Some(KnownClass::FunctionType), "__get__") => {
                    Symbol::bound(Type::Callable(CallableType::WrapperDescriptorDunderGet))
                }

                // TODO:
                // We currently hard-code the knowledge that the following known classes are not
                // descriptors, i.e. that they have no `__get__` method. This is not wrong and
                // potentially even beneficial for performance, but it's not very principled.
                // This case can probably be removed eventually, but we include it at the moment
                // because we make extensive use of these types in our test suite. Note that some
                // builtin types are not included here, since they do not have generic bases and
                // are correctly handled by the `instance_member` method.
                (
                    Some(
                        KnownClass::Str
                        | KnownClass::Bytes
                        | KnownClass::Tuple
                        | KnownClass::Slice
                        | KnownClass::Range,
                    ),
                    "__get__",
                ) => Symbol::Unbound,

                _ => {
                    let SymbolAndQualifiers(symbol, _) = class.instance_member(db, name);
                    symbol
                }
            },

            Type::Union(union) => union.map_with_boundness(db, |elem| elem.static_member(db, name)),

            Type::Intersection(intersection) => {
                intersection.map_with_boundness(db, |elem| elem.static_member(db, name))
            }

            Type::IntLiteral(_) => match name {
                "real" | "numerator" => Symbol::bound(self),
                // TODO more attributes could probably be usefully special-cased
                _ => KnownClass::Int.to_instance(db).static_member(db, name),
            },

            Type::BooleanLiteral(bool_value) => match name {
                "real" | "numerator" => Symbol::bound(Type::IntLiteral(i64::from(*bool_value))),
                _ => KnownClass::Bool.to_instance(db).static_member(db, name),
            },

            Type::StringLiteral(_) | Type::LiteralString => {
                KnownClass::Str.to_instance(db).static_member(db, name)
            }

            Type::BytesLiteral(_) => KnownClass::Bytes.to_instance(db).static_member(db, name),

            // We could plausibly special-case `start`, `step`, and `stop` here,
            // but it doesn't seem worth the complexity given the very narrow range of places
            // where we infer `SliceLiteral` types.
            Type::SliceLiteral(_) => KnownClass::Slice.to_instance(db).static_member(db, name),

            Type::Tuple(_) => {
                // TODO: We might want to special case some attributes here, as the stubs
                // for `builtins.tuple` assume that `self` is a homogeneous tuple, while
                // we're explicitly modeling heterogeneous tuples using `Type::Tuple`.
                KnownClass::Tuple.to_instance(db).static_member(db, name)
            }

            Type::AlwaysTruthy | Type::AlwaysFalsy => match name {
                "__bool__" => {
                    // TODO should be `Callable[[], Literal[True/False]]`
                    Symbol::todo("`__bool__` for `AlwaysTruthy`/`AlwaysFalsy` Type variants")
                }
                _ => Type::object(db).static_member(db, name),
            },
        }
    }

    /// Call the `__get__(instance, owner)` method on a type, and get the return
    /// type of the call.
    ///
    /// If `__get__` is not defined on the type, this method returns `Ok(None)`.
    /// If the call to `__get__` fails, this method returns an error.
    fn try_call_dunder_get(
        self,
        db: &'db dyn Db,
        instance: Option<Type<'db>>,
        owner: Type<'db>,
    ) -> Option<Type<'db>> {
        #[salsa::tracked]
        fn try_call_dunder_get_query<'db>(
            db: &'db dyn Db,
            ty_self: Type<'db>,
            instance: Option<Type<'db>>,
            owner: Type<'db>,
        ) -> Option<Type<'db>> {
            // TODO: Handle possible-unboundness and errors from `__get__` calls.

            match ty_self {
                Type::Union(union) => {
                    let mut builder = UnionBuilder::new(db);
                    for elem in union.elements(db) {
                        let ty = if let Some(result) = elem.try_call_dunder_get(db, instance, owner)
                        {
                            result
                        } else {
                            *elem
                        };
                        builder = builder.add(ty);
                    }
                    Some(builder.build())
                }
                Type::Intersection(intersection) => {
                    if !intersection.negative(db).is_empty() {
                        return Some(todo_type!(
                            "try_call_dunder_get: intersections with negative contributions"
                        ));
                    }

                    let mut builder = IntersectionBuilder::new(db);
                    for elem in intersection.positive(db) {
                        let ty = if let Some(result) = elem.try_call_dunder_get(db, instance, owner)
                        {
                            result
                        } else {
                            *elem
                        };
                        builder = builder.add_positive(ty);
                    }
                    Some(builder.build())
                }
                _ => {
                    // TODO: Handle possible-unboundness of `__get__` method
                    // There is an existing test case for this in `descriptor_protocol.md`.

                    ty_self
                        .member(db, "__get__")
                        .ignore_possibly_unbound()?
                        .try_call(
                            db,
                            &CallArguments::positional([instance.unwrap_or(Type::none(db)), owner]),
                        )
                        .map(|outcome| Some(outcome.return_type(db)))
                        .unwrap_or(None)
                }
            }
        }

        try_call_dunder_get_query(db, self, instance, owner)
    }

    /// Access an attribute of this type, potentially invoking the descriptor protocol.
    /// Corresponds to `getattr(<object of type 'self'>, name)`.
    ///
    /// See also: [`Type::static_member`]
    ///
    /// TODO: We should return a `Result` here to handle errors that can appear during attribute
    /// lookup, like a failed `__get__` call on a descriptor.
    #[must_use]
    pub(crate) fn member(&self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        if name == "__class__" {
            return Symbol::bound(self.to_meta_type(db));
        }

        match self {
            Type::FunctionLiteral(function) if name == "__get__" => Symbol::bound(Type::Callable(
                CallableType::MethodWrapperDunderGet(*function),
            )),

            Type::Callable(CallableType::BoundMethod(bound_method)) => match name {
                "__self__" => Symbol::bound(bound_method.self_instance(db)),
                "__func__" => Symbol::bound(Type::FunctionLiteral(bound_method.function(db))),
                _ => {
                    KnownClass::MethodType
                        .to_instance(db)
                        .member(db, name)
                        .or_fall_back_to(db, || {
                            // If an attribute is not available on the bound method object,
                            // it will be looked up on the underlying function object:
                            Type::FunctionLiteral(bound_method.function(db)).member(db, name)
                        })
                }
            },
            Type::Callable(CallableType::MethodWrapperDunderGet(_)) => {
                KnownClass::MethodWrapperType
                    .to_instance(db)
                    .member(db, name)
            }
            Type::Callable(CallableType::WrapperDescriptorDunderGet) => {
                KnownClass::WrapperDescriptorType
                    .to_instance(db)
                    .member(db, name)
            }

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
                let member = self.static_member(db, name);

                let instance = Some(*self);
                let owner = self.to_meta_type(db);

                // TODO: Handle `__get__` call errors instead of using `.unwrap_or(None)`.
                // There is an existing test case for this in `descriptor_protocol.md`.
                member.map_type(|ty| ty.try_call_dunder_get(db, instance, owner).unwrap_or(ty))
            }
            Type::ClassLiteral(..) | Type::SubclassOf(..) => {
                let member = self.static_member(db, name);

                let instance = None;
                let owner = *self;

                // TODO: Handle `__get__` call errors (see above).
                member.map_type(|ty| ty.try_call_dunder_get(db, instance, owner).unwrap_or(ty))
            }
            Type::Union(union) => union.map_with_boundness(db, |elem| elem.member(db, name)),
            Type::Intersection(intersection) => {
                intersection.map_with_boundness(db, |elem| elem.member(db, name))
            }

            Type::Dynamic(..)
            | Type::Never
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::ModuleLiteral(..) => self.static_member(db, name),
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
            Type::ClassLiteral(ClassLiteralType { class }) => {
                return class
                    .metaclass(db)
                    .to_instance(db)
                    .try_bool_impl(db, allow_short_circuit);
            }
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                ClassBase::Dynamic(_) => Truthiness::Ambiguous,
                ClassBase::Class(class) => {
                    return class
                        .metaclass(db)
                        .to_instance(db)
                        .try_bool_impl(db, allow_short_circuit);
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
                        ref result @ (Ok(ref outcome)
                        | Err(CallDunderError::PossiblyUnbound(ref outcome))) => {
                            let return_type = outcome.return_type(db);

                            // The type has a `__bool__` method, but it doesn't return a boolean.
                            if !return_type.is_assignable_to(db, KnownClass::Bool.to_instance(db)) {
                                return Err(BoolError::IncorrectReturnType {
                                    return_type: outcome.return_type(db),
                                    not_boolable_type: *instance_ty,
                                });
                            }

                            if result.is_ok() {
                                type_to_truthiness(return_type)
                            } else {
                                // Don't trust possibly unbound `__bool__` method.
                                Truthiness::Ambiguous
                            }
                        }
                        Err(CallDunderError::MethodNotAvailable) => Truthiness::Ambiguous,
                        Err(CallDunderError::Call(err)) => {
                            let err = match err {
                                // Unwrap call errors where only a single variant isn't callable.
                                // E.g. in the case of `Unknown & T`
                                // TODO: Improve handling of unions. While this improves messages overall,
                                //   it still results in loosing information. Or should the information
                                //   be recomputed when rendering the diagnostic?
                                CallError::Union(union_error) => {
                                    if let Type::Union(_) = union_error.called_type {
                                        if union_error.errors.len() == 1 {
                                            union_error.errors.into_vec().pop().unwrap()
                                        } else {
                                            CallError::Union(union_error)
                                        }
                                    } else {
                                        CallError::Union(union_error)
                                    }
                                }
                                err => err,
                            };

                            match err {
                                CallError::BindingError { binding } => {
                                    return Err(BoolError::IncorrectArguments {
                                        truthiness: type_to_truthiness(binding.return_type()),
                                        not_boolable_type: *instance_ty,
                                    });
                                }
                                CallError::NotCallable { .. } => {
                                    return Err(BoolError::NotCallable {
                                        not_boolable_type: *instance_ty,
                                    });
                                }

                                CallError::PossiblyUnboundDunderCall { .. }
                                | CallError::Union(..) => {
                                    return Err(BoolError::Other {
                                        not_boolable_type: *self,
                                    })
                                }
                            }
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
            Ok(outcome) | Err(CallDunderError::PossiblyUnbound(outcome)) => outcome.return_type(db),

            // TODO: emit a diagnostic
            Err(err) => err.return_type(db)?,
        };

        non_negative_int_literal(db, return_ty)
    }

    /// Calls `self`
    ///
    /// Returns `Ok` if the call with the given arguments is successful and `Err` otherwise.
    fn try_call(
        self,
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<CallOutcome<'db>, CallError<'db>> {
        match self {
            Type::Callable(CallableType::BoundMethod(bound_method)) => {
                let instance = bound_method.self_instance(db);
                let arguments = arguments.with_self(instance);

                let binding = bind_call(
                    db,
                    &arguments,
                    bound_method.function(db).signature(db),
                    self,
                );

                if binding.has_binding_errors() {
                    Err(CallError::BindingError { binding })
                } else {
                    Ok(CallOutcome::Single(binding))
                }
            }
            Type::Callable(CallableType::MethodWrapperDunderGet(function)) => {
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

                let first_argument_is_none =
                    arguments.first_argument().is_some_and(|ty| ty.is_none(db));

                let signature = Signature::new(
                    Parameters::new([
                        Parameter::new(
                            Some("instance".into()),
                            Some(Type::object(db)),
                            ParameterKind::PositionalOnly { default_ty: None },
                        ),
                        if first_argument_is_none {
                            Parameter::new(
                                Some("owner".into()),
                                Some(KnownClass::Type.to_instance(db)),
                                ParameterKind::PositionalOnly { default_ty: None },
                            )
                        } else {
                            Parameter::new(
                                Some("owner".into()),
                                Some(UnionType::from_elements(
                                    db,
                                    [KnownClass::Type.to_instance(db), Type::none(db)],
                                )),
                                ParameterKind::PositionalOnly {
                                    default_ty: Some(Type::none(db)),
                                },
                            )
                        },
                    ]),
                    if function.has_known_class_decorator(db, KnownClass::Classmethod)
                        && function.decorators(db).len() == 1
                    {
                        if let Some(owner) = arguments.second_argument() {
                            Some(Type::Callable(CallableType::BoundMethod(
                                BoundMethodType::new(db, function, owner),
                            )))
                        } else if let Some(instance) = arguments.first_argument() {
                            Some(Type::Callable(CallableType::BoundMethod(
                                BoundMethodType::new(db, function, instance.to_meta_type(db)),
                            )))
                        } else {
                            Some(Type::unknown())
                        }
                    } else {
                        Some(match arguments.first_argument() {
                            Some(ty) if ty.is_none(db) => Type::FunctionLiteral(function),
                            Some(instance) => Type::Callable(CallableType::BoundMethod(
                                BoundMethodType::new(db, function, instance),
                            )),
                            _ => Type::unknown(),
                        })
                    },
                );

                let binding = bind_call(db, arguments, &signature, self);

                if binding.has_binding_errors() {
                    Err(CallError::BindingError { binding })
                } else {
                    Ok(CallOutcome::Single(binding))
                }
            }
            Type::Callable(CallableType::WrapperDescriptorDunderGet) => {
                // Here, we also model `types.FunctionType.__get__`, but now we consider a call to
                // this as a function, i.e. we also expect the `self` argument to be passed in.

                let second_argument_is_none =
                    arguments.second_argument().is_some_and(|ty| ty.is_none(db));

                let signature = Signature::new(
                    Parameters::new([
                        Parameter::new(
                            Some("self".into()),
                            Some(KnownClass::FunctionType.to_instance(db)),
                            ParameterKind::PositionalOnly { default_ty: None },
                        ),
                        Parameter::new(
                            Some("instance".into()),
                            Some(Type::object(db)),
                            ParameterKind::PositionalOnly { default_ty: None },
                        ),
                        if second_argument_is_none {
                            Parameter::new(
                                Some("owner".into()),
                                Some(KnownClass::Type.to_instance(db)),
                                ParameterKind::PositionalOnly { default_ty: None },
                            )
                        } else {
                            Parameter::new(
                                Some("owner".into()),
                                Some(UnionType::from_elements(
                                    db,
                                    [KnownClass::Type.to_instance(db), Type::none(db)],
                                )),
                                ParameterKind::PositionalOnly {
                                    default_ty: Some(Type::none(db)),
                                },
                            )
                        },
                    ]),
                    Some(
                        if let Some(function_ty @ Type::FunctionLiteral(function)) =
                            arguments.first_argument()
                        {
                            if function.has_known_class_decorator(db, KnownClass::Classmethod)
                                && function.decorators(db).len() == 1
                            {
                                if let Some(owner) = arguments.third_argument() {
                                    Type::Callable(CallableType::BoundMethod(BoundMethodType::new(
                                        db, function, owner,
                                    )))
                                } else if let Some(instance) = arguments.second_argument() {
                                    Type::Callable(CallableType::BoundMethod(BoundMethodType::new(
                                        db,
                                        function,
                                        instance.to_meta_type(db),
                                    )))
                                } else {
                                    Type::unknown()
                                }
                            } else {
                                if let Some(instance) = arguments.second_argument() {
                                    if instance.is_none(db) {
                                        function_ty
                                    } else {
                                        Type::Callable(CallableType::BoundMethod(
                                            BoundMethodType::new(db, function, instance),
                                        ))
                                    }
                                } else {
                                    Type::unknown()
                                }
                            }
                        } else {
                            Type::unknown()
                        },
                    ),
                );

                let binding = bind_call(db, arguments, &signature, self);

                if binding.has_binding_errors() {
                    Err(CallError::BindingError { binding })
                } else {
                    Ok(CallOutcome::Single(binding))
                }
            }
            Type::FunctionLiteral(function_type) => {
                let mut binding = bind_call(db, arguments, function_type.signature(db), self);

                if binding.has_binding_errors() {
                    return Err(CallError::BindingError { binding });
                }

                match function_type.known(db) {
                    Some(KnownFunction::IsEquivalentTo) => {
                        let (ty_a, ty_b) = binding
                            .two_parameter_types()
                            .unwrap_or((Type::unknown(), Type::unknown()));
                        binding
                            .set_return_type(Type::BooleanLiteral(ty_a.is_equivalent_to(db, ty_b)));
                    }
                    Some(KnownFunction::IsSubtypeOf) => {
                        let (ty_a, ty_b) = binding
                            .two_parameter_types()
                            .unwrap_or((Type::unknown(), Type::unknown()));
                        binding.set_return_type(Type::BooleanLiteral(ty_a.is_subtype_of(db, ty_b)));
                    }
                    Some(KnownFunction::IsAssignableTo) => {
                        let (ty_a, ty_b) = binding
                            .two_parameter_types()
                            .unwrap_or((Type::unknown(), Type::unknown()));
                        binding
                            .set_return_type(Type::BooleanLiteral(ty_a.is_assignable_to(db, ty_b)));
                    }
                    Some(KnownFunction::IsDisjointFrom) => {
                        let (ty_a, ty_b) = binding
                            .two_parameter_types()
                            .unwrap_or((Type::unknown(), Type::unknown()));
                        binding
                            .set_return_type(Type::BooleanLiteral(ty_a.is_disjoint_from(db, ty_b)));
                    }
                    Some(KnownFunction::IsGradualEquivalentTo) => {
                        let (ty_a, ty_b) = binding
                            .two_parameter_types()
                            .unwrap_or((Type::unknown(), Type::unknown()));
                        binding.set_return_type(Type::BooleanLiteral(
                            ty_a.is_gradual_equivalent_to(db, ty_b),
                        ));
                    }
                    Some(KnownFunction::IsFullyStatic) => {
                        let ty = binding.one_parameter_type().unwrap_or(Type::unknown());
                        binding.set_return_type(Type::BooleanLiteral(ty.is_fully_static(db)));
                    }
                    Some(KnownFunction::IsSingleton) => {
                        let ty = binding.one_parameter_type().unwrap_or(Type::unknown());
                        binding.set_return_type(Type::BooleanLiteral(ty.is_singleton(db)));
                    }
                    Some(KnownFunction::IsSingleValued) => {
                        let ty = binding.one_parameter_type().unwrap_or(Type::unknown());
                        binding.set_return_type(Type::BooleanLiteral(ty.is_single_valued(db)));
                    }

                    Some(KnownFunction::Len) => {
                        if let Some(first_arg) = binding.one_parameter_type() {
                            if let Some(len_ty) = first_arg.len(db) {
                                binding.set_return_type(len_ty);
                            }
                        };
                    }

                    Some(KnownFunction::Repr) => {
                        if let Some(first_arg) = binding.one_parameter_type() {
                            binding.set_return_type(first_arg.repr(db));
                        };
                    }

                    Some(KnownFunction::Cast) => {
                        // TODO: Use `.two_parameter_tys()` exclusively
                        // when overloads are supported.
                        if let Some(casted_ty) = arguments.first_argument() {
                            if binding.two_parameter_types().is_some() {
                                binding.set_return_type(casted_ty);
                            }
                        };
                    }

                    Some(KnownFunction::Overload) => {
                        binding.set_return_type(todo_type!("overload(..) return type"));
                    }

                    Some(KnownFunction::GetattrStatic) => {
                        let Some((instance_ty, attr_name, default)) =
                            binding.three_parameter_types()
                        else {
                            return Ok(CallOutcome::Single(binding));
                        };

                        let Some(attr_name) = attr_name.into_string_literal() else {
                            return Ok(CallOutcome::Single(binding));
                        };

                        let default = if default.is_unknown() {
                            Type::Never
                        } else {
                            default
                        };

                        let union_with_default = |ty| UnionType::from_elements(db, [ty, default]);

                        // TODO: we could emit a diagnostic here (if default is not set)
                        binding.set_return_type(
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
                };

                if binding.has_binding_errors() {
                    Err(CallError::BindingError { binding })
                } else {
                    Ok(CallOutcome::Single(binding))
                }
            }

            // TODO annotated return type on `__new__` or metaclass `__call__`
            // TODO check call vs signatures of `__new__` and/or `__init__`
            Type::ClassLiteral(ClassLiteralType { class }) => {
                Ok(CallOutcome::Single(CallBinding::from_return_type(
                    match class.known(db) {
                        // TODO: We should check the call signature and error if the bool call doesn't have the
                        //   right signature and return a binding error.

                        // If the class is the builtin-bool class (for example `bool(1)`), we try to
                        // return the specific truthiness value of the input arg, `Literal[True]` for
                        // the example above.
                        Some(KnownClass::Bool) => arguments
                            .first_argument()
                            .map(|arg| arg.bool(db).into_type(db))
                            .unwrap_or(Type::BooleanLiteral(false)),

                        // TODO: Don't ignore the second and third arguments to `str`
                        //   https://github.com/astral-sh/ruff/pull/16161#discussion_r1958425568
                        Some(KnownClass::Str) => arguments
                            .first_argument()
                            .map(|arg| arg.str(db))
                            .unwrap_or_else(|| Type::string_literal(db, "")),

                        Some(KnownClass::Type) => arguments
                            .exactly_one_argument()
                            .map(|arg| arg.to_meta_type(db))
                            .unwrap_or_else(|| KnownClass::Type.to_instance(db)),

                        _ => Type::Instance(InstanceType { class }),
                    },
                )))
            }

            instance_ty @ Type::Instance(_) => {
                instance_ty
                    .try_call_dunder(db, "__call__", arguments)
                    .map_err(|err| match err {
                        CallDunderError::Call(CallError::NotCallable { .. }) => {
                            // Turn "`<type of illegal '__call__'>` not callable" into
                            // "`X` not callable"
                            CallError::NotCallable {
                                not_callable_type: self,
                            }
                        }
                        CallDunderError::Call(CallError::Union(UnionCallError {
                            called_type: _,
                            bindings,
                            errors,
                        })) => CallError::Union(UnionCallError {
                            called_type: self,
                            bindings,
                            errors,
                        }),
                        CallDunderError::Call(error) => error,
                        // Turn "possibly unbound object of type `Literal['__call__']`"
                        // into "`X` not callable (possibly unbound `__call__` method)"
                        CallDunderError::PossiblyUnbound(outcome) => {
                            CallError::PossiblyUnboundDunderCall {
                                called_type: self,
                                outcome: Box::new(outcome),
                            }
                        }
                        CallDunderError::MethodNotAvailable => {
                            // Turn "`X.__call__` unbound" into "`X` not callable"
                            CallError::NotCallable {
                                not_callable_type: self,
                            }
                        }
                    })
            }

            // Dynamic types are callable, and the return type is the same dynamic type
            Type::Dynamic(_) => Ok(CallOutcome::Single(CallBinding::from_return_type(self))),

            Type::Union(union) => {
                CallOutcome::try_call_union(db, union, |element| element.try_call(db, arguments))
            }

            Type::Intersection(_) => Ok(CallOutcome::Single(CallBinding::from_return_type(
                todo_type!("Type::Intersection.call()"),
            ))),

            _ => Err(CallError::NotCallable {
                not_callable_type: self,
            }),
        }
    }

    /// Look up a dunder method on the meta type of `self` and call it.
    ///
    /// Returns an `Err` if the dunder method can't be called,
    /// or the given arguments are not valid.
    fn try_call_dunder(
        self,
        db: &'db dyn Db,
        name: &str,
        arguments: &CallArguments<'_, 'db>,
    ) -> Result<CallOutcome<'db>, CallDunderError<'db>> {
        let meta_type = self.to_meta_type(db);

        match meta_type.static_member(db, name) {
            Symbol::Type(callable_ty, boundness) => {
                // Dunder methods are looked up on the meta type, but they invoke the descriptor
                // protocol *as if they had been called on the instance itself*. This is why we
                // pass `Some(self)` for the `instance` argument here.
                let callable_ty = callable_ty
                    .try_call_dunder_get(db, Some(self), meta_type)
                    .unwrap_or(callable_ty);

                let result = callable_ty.try_call(db, arguments)?;

                if boundness == Boundness::Bound {
                    Ok(result)
                } else {
                    Err(CallDunderError::PossiblyUnbound(result))
                }
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

        let iteration_result = match dunder_iter_result {
            Ok(iterator) => {
                // `__iter__` is definitely bound and calling it succeeds.
                // See what calling `__next__` on the object returned by `__iter__` gives us...
                try_call_dunder_next_on_iterator(iterator).map_err(|dunder_next_error| {
                    IterationErrorKind::IterReturnsInvalidIterator {
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
                                IterationErrorKind::PossiblyUnboundIterAndGetitemError {
                                    dunder_next_return,
                                    dunder_getitem_error,
                                }
                            })
                    }

                    Err(dunder_next_error) => Err(IterationErrorKind::IterReturnsInvalidIterator {
                        iterator,
                        dunder_next_error,
                    }),
                }
            }

            // `__iter__` is definitely bound but it can't be called with the expected arguments
            Err(CallDunderError::Call(dunder_iter_call_error)) => {
                Err(IterationErrorKind::IterCallError(dunder_iter_call_error))
            }

            // There's no `__iter__` method. Try `__getitem__` instead...
            Err(CallDunderError::MethodNotAvailable) => {
                try_call_dunder_getitem().map_err(|dunder_getitem_error| {
                    IterationErrorKind::UnboundIterAndGetitemError {
                        dunder_getitem_error,
                    }
                })
            }
        };

        iteration_result.map_err(|error_kind| IterationError {
            iterable_type: self,
            error_kind,
        })
    }

    #[must_use]
    pub fn to_instance(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::Dynamic(_) => *self,
            Type::Never => Type::Never,
            Type::ClassLiteral(ClassLiteralType { class }) => Type::instance(*class),
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                ClassBase::Class(class) => Type::instance(class),
                ClassBase::Dynamic(dynamic) => Type::Dynamic(dynamic),
            },
            Type::Union(union) => union.map(db, |element| element.to_instance(db)),
            Type::Intersection(_) => todo_type!("Type::Intersection.to_instance()"),
            // TODO: calling `.to_instance()` on any of these should result in a diagnostic,
            // since they already indicate that the object is an instance of some kind:
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
            | Type::AlwaysFalsy => Type::unknown(),
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
            Type::ClassLiteral(ClassLiteralType { class })
                if class.is_known(db, KnownClass::Float) =>
            {
                Ok(UnionType::from_elements(
                    db,
                    [
                        KnownClass::Int.to_instance(db),
                        KnownClass::Float.to_instance(db),
                    ],
                ))
            }
            Type::ClassLiteral(ClassLiteralType { class })
                if class.is_known(db, KnownClass::Complex) =>
            {
                Ok(UnionType::from_elements(
                    db,
                    [
                        KnownClass::Int.to_instance(db),
                        KnownClass::Float.to_instance(db),
                        KnownClass::Complex.to_instance(db),
                    ],
                ))
            }
            // In a type expression, a bare `type` is interpreted as "instance of `type`", which is
            // equivalent to `type[object]`.
            Type::ClassLiteral(_) | Type::SubclassOf(_) => Ok(self.to_instance(db)),
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
            // TODO: Should emit a diagnostic
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
            _ => Ok(todo_type!(
                "Unsupported or invalid type in a type expression"
            )),
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
}

impl std::fmt::Display for DynamicType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DynamicType::Any => f.write_str("Any"),
            DynamicType::Unknown => f.write_str("Unknown"),
            // `DynamicType::Todo`'s display should be explicit that is not a valid display of
            // any other type
            DynamicType::Todo(todo) => write!(f, "@Todo{todo}"),
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
    invalid_expressions: smallvec::SmallVec<[InvalidTypeExpression; 1]>,
}

impl<'db> InvalidTypeExpressionError<'db> {
    fn into_fallback_type(self, context: &InferContext, node: &ast::Expr) -> Type<'db> {
        let InvalidTypeExpressionError {
            fallback_type,
            invalid_expressions,
        } = self;
        for error in invalid_expressions {
            context.report_lint(&INVALID_TYPE_FORM, node, format_args!("{}", error.reason()));
        }
        fallback_type
    }
}

/// Enumeration of various types that are invalid in type-expression contexts
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum InvalidTypeExpression {
    /// `x: Annotated` is invalid as an annotation
    BareAnnotated,
    /// `x: Literal` is invalid as an annotation
    BareLiteral,
    /// The `ClassVar` type qualifier was used in a type expression
    ClassVarInTypeExpression,
    /// The `Final` type qualifier was used in a type expression
    FinalInTypeExpression,
}

impl InvalidTypeExpression {
    const fn reason(self) -> &'static str {
        match self {
            Self::BareAnnotated => "`Annotated` requires at least two arguments when used in an annotation or type expression",
            Self::BareLiteral => "`Literal` requires at least one argument when used in a type expression",
            Self::ClassVarInTypeExpression => "Type qualifier `typing.ClassVar` is not allowed in type expressions (only in annotation expressions)",
            Self::FinalInTypeExpression => "Type qualifier `typing.Final` is not allowed in type expressions (only in annotation expressions)",
        }
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
#[salsa::tracked]
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

/// Error returned if a type is not (or may not be) iterable.
#[derive(Debug)]
struct IterationError<'db> {
    /// The type of the object that the analysed code attempted to iterate over.
    iterable_type: Type<'db>,

    /// The precise kind of error encountered when trying to iterate over the type.
    error_kind: IterationErrorKind<'db>,
}

impl<'db> IterationError<'db> {
    /// Returns the element type if it is known, or `None` if the type is never iterable.
    fn element_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        self.error_kind.element_type(db)
    }

    /// Returns the element type if it is known, or `Type::unknown()` if it is not.
    fn fallback_element_type(&self, db: &'db dyn Db) -> Type<'db> {
        self.element_type(db).unwrap_or(Type::unknown())
    }

    /// Reports the diagnostic for this error.
    fn report_diagnostic(&self, context: &InferContext<'db>, iterable_node: ast::AnyNodeRef) {
        self.error_kind
            .report_diagnostic(context, self.iterable_type, iterable_node);
    }
}

#[derive(Debug)]
enum IterationErrorKind<'db> {
    /// The object being iterated over has a bound `__iter__` method,
    /// but calling it with the expected arguments results in an error.
    IterCallError(CallError<'db>),

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

impl<'db> IterationErrorKind<'db> {
    /// Returns the element type if it is known, or `None` if the type is never iterable.
    fn element_type(&self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Self::IterReturnsInvalidIterator {
                dunder_next_error, ..
            } => dunder_next_error.return_type(db),

            Self::IterCallError(dunder_iter_call_error) => dunder_iter_call_error
                .fallback_return_type(db)
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
                CallDunderError::Call(dunder_getitem_call_error) => Some(
                    dunder_getitem_call_error
                        .return_type(db)
                        .map(|dunder_getitem_return| {
                            let elements = [*dunder_next_return, dunder_getitem_return];
                            UnionType::from_elements(db, elements)
                        })
                        .unwrap_or(*dunder_next_return),
                ),
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

        // TODO: for all of these error variant, the "explanation" for the diagnostic
        // (everything after the "because") should really be presented as a "help:", "note",
        // or similar, rather than as part of the same sentence as the error message.

        match self {
            Self::IterCallError(dunder_iter_call_error) => match dunder_iter_call_error {
                CallError::NotCallable { not_callable_type } => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` is not iterable \
                        because its `__iter__` attribute has type `{dunder_iter_type}`, \
                        which is not callable",
                    iterable_type = iterable_type.display(db),
                    dunder_iter_type = not_callable_type.display(db),
                )),
                CallError::PossiblyUnboundDunderCall { called_type, .. } => {
                    report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because its `__iter__` attribute (with type `{dunder_iter_type}`) \
                            may not be callable",
                        iterable_type = iterable_type.display(db),
                        dunder_iter_type = called_type.display(db),
                    ));
                }
                CallError::Union(union_call_error) if union_call_error.indicates_type_possibly_not_callable() => {
                    report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because its `__iter__` attribute (with type `{dunder_iter_type}`) \
                            may not be callable",
                        iterable_type = iterable_type.display(db),
                        dunder_iter_type = union_call_error.called_type.display(db),
                    ));
                }
                CallError::BindingError { .. } => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` is not iterable \
                        because its `__iter__` method has an invalid signature \
                        (expected `def __iter__(self): ...`)",
                    iterable_type = iterable_type.display(db),
                )),
                CallError::Union(UnionCallError { called_type, .. }) => report_not_iterable(format_args!(
                    "Object of type `{iterable_type}` may not be iterable \
                        because its `__iter__` method (with type `{dunder_iter_type}`) \
                        may have an invalid signature (expected `def __iter__(self): ...`)",
                    iterable_type = iterable_type.display(db),
                    dunder_iter_type = called_type.display(db),
                )),
            }

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
                CallDunderError::Call(dunder_next_call_error) => match dunder_next_call_error {
                    CallError::NotCallable { .. } => report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` is not iterable \
                            because its `__iter__` method returns an object of type `{iterator_type}`, \
                            which has a `__next__` attribute that is not callable",
                        iterable_type = iterable_type.display(db),
                        iterator_type = iterator.display(db),
                    )),
                    CallError::PossiblyUnboundDunderCall { .. } => report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because its `__iter__` method returns an object of type `{iterator_type}`, \
                            which has a `__next__` attribute that may not be callable",
                        iterable_type = iterable_type.display(db),
                        iterator_type = iterator.display(db),
                    )),
                    CallError::Union(union_call_error) if union_call_error.indicates_type_possibly_not_callable() => {
                        report_not_iterable(format_args!(
                            "Object of type `{iterable_type}` may not be iterable \
                                because its `__iter__` method returns an object of type `{iterator_type}`, \
                                which has a `__next__` attribute that may not be callable",
                            iterable_type = iterable_type.display(db),
                            iterator_type = iterator.display(db),
                        ));
                    }
                    CallError::BindingError { .. } => report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` is not iterable \
                            because its `__iter__` method returns an object of type `{iterator_type}`, \
                            which has an invalid `__next__` method (expected `def __next__(self): ...`)",
                        iterable_type = iterable_type.display(db),
                        iterator_type = iterator.display(db),
                    )),
                    CallError::Union(_) => report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because its `__iter__` method returns an object of type `{iterator_type}`, \
                            which may have an invalid `__next__` method (expected `def __next__(self): ...`)",
                        iterable_type = iterable_type.display(db),
                        iterator_type = iterator.display(db),
                    )),
                }
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
                CallDunderError::Call(dunder_getitem_call_error) => match dunder_getitem_call_error {
                    CallError::NotCallable { not_callable_type } => report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because it may not have an `__iter__` method \
                            and its `__getitem__` attribute has type `{dunder_getitem_type}`, \
                            which is not callable",
                        iterable_type = iterable_type.display(db),
                        dunder_getitem_type = not_callable_type.display(db),
                    )),
                    CallError::PossiblyUnboundDunderCall { .. } => report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because it may not have an `__iter__` method \
                            and its `__getitem__` attribute may not be callable",
                        iterable_type = iterable_type.display(db),
                    )),
                    CallError::Union(union_call_error) if union_call_error.indicates_type_possibly_not_callable() => {
                        report_not_iterable(format_args!(
                            "Object of type `{iterable_type}` may not be iterable \
                                because it may not have an `__iter__` method \
                                and its `__getitem__` attribute (with type `{dunder_getitem_type}`) \
                                may not be callable",
                            iterable_type = iterable_type.display(db),
                            dunder_getitem_type = union_call_error.called_type.display(db),
                        ));
                    }
                    CallError::BindingError { .. } => report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because it may not have an `__iter__` method \
                            and its `__getitem__` method has an incorrect signature \
                            for the old-style iteration protocol \
                            (expected a signature at least as permissive as \
                            `def __getitem__(self, key: int): ...`)",
                        iterable_type = iterable_type.display(db),
                    )),
                    CallError::Union(UnionCallError {called_type, ..})=> report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because it may not have an `__iter__` method \
                            and its `__getitem__` method (with type `{dunder_getitem_type}`)
                            may have an incorrect signature for the old-style iteration protocol \
                            (expected a signature at least as permissive as \
                            `def __getitem__(self, key: int): ...`)",
                        iterable_type = iterable_type.display(db),
                        dunder_getitem_type = called_type.display(db),
                    )),
                }
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
                CallDunderError::Call(dunder_getitem_call_error) => match dunder_getitem_call_error {
                    CallError::NotCallable { not_callable_type } => report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` is not iterable \
                            because it has no `__iter__` method and \
                            its `__getitem__` attribute has type `{dunder_getitem_type}`, \
                            which is not callable",
                        iterable_type = iterable_type.display(db),
                        dunder_getitem_type = not_callable_type.display(db),
                    )),
                    CallError::PossiblyUnboundDunderCall { .. } => report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because it has no `__iter__` method and its `__getitem__` attribute \
                            may not be callable",
                        iterable_type = iterable_type.display(db),
                    )),
                    CallError::Union(union_call_error) if union_call_error.indicates_type_possibly_not_callable() => {
                        report_not_iterable(format_args!(
                            "Object of type `{iterable_type}` may not be iterable \
                                because it has no `__iter__` method and its `__getitem__` attribute \
                                (with type `{dunder_getitem_type}`) may not be callable",
                            iterable_type = iterable_type.display(db),
                            dunder_getitem_type = union_call_error.called_type.display(db),
                        ));
                    }
                    CallError::BindingError { .. } => report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` is not iterable \
                            because it has no `__iter__` method and \
                            its `__getitem__` method has an incorrect signature \
                            for the old-style iteration protocol \
                            (expected a signature at least as permissive as \
                            `def __getitem__(self, key: int): ...`)",
                        iterable_type = iterable_type.display(db),
                    )),
                    CallError::Union(UnionCallError { called_type, .. }) => report_not_iterable(format_args!(
                        "Object of type `{iterable_type}` may not be iterable \
                            because it has no `__iter__` method and \
                            its `__getitem__` method (with type `{dunder_getitem_type}`) \
                            may have an incorrect signature for the old-style iteration protocol \
                            (expected a signature at least as permissive as \
                            `def __getitem__(self, key: int): ...`)",
                        iterable_type = iterable_type.display(db),
                        dunder_getitem_type = called_type.display(db),
                    )),
                }
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

#[salsa::interned]
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
#[salsa::tracked]
pub struct BoundMethodType<'db> {
    /// The function that is being bound. Corresponds to the `__func__` attribute on a
    /// bound method object
    pub(crate) function: FunctionType<'db>,
    /// The instance on which this method has been called. Corresponds to the `__self__`
    /// attribute on a bound method object
    self_instance: Type<'db>,
}

/// A type that represents callable objects.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, salsa::Update)]
pub enum CallableType<'db> {
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

#[salsa::interned]
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
                .static_member(db, "__dict__");
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

        imported_symbol(db, &self.module(db), name)
    }
}

#[salsa::interned]
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

#[salsa::interned]
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

    pub fn is_fully_static(self, db: &'db dyn Db) -> bool {
        self.elements(db).iter().all(|ty| ty.is_fully_static(db))
    }

    /// Create a new union type with the elements sorted according to a canonical ordering.
    #[must_use]
    pub fn to_sorted_union(self, db: &'db dyn Db) -> Self {
        let mut new_elements: Vec<Type<'db>> = self
            .elements(db)
            .iter()
            .map(|element| element.with_sorted_unions(db))
            .collect();
        new_elements.sort_unstable_by(union_elements_ordering);
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

#[salsa::interned]
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
                .map(|ty| ty.with_sorted_unions(db))
                .collect();

            elements.sort_unstable_by(union_elements_ordering);
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

        let mut any_unbound = false;
        let mut any_possibly_unbound = false;
        for ty in self.positive(db) {
            let ty_member = transform_fn(ty);
            match ty_member {
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

        if any_unbound {
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
        }
    }
}

#[salsa::interned]
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

#[salsa::interned]
pub struct BytesLiteralType<'db> {
    #[return_ref]
    value: Box<[u8]>,
}

impl<'db> BytesLiteralType<'db> {
    pub fn python_len(&self, db: &'db dyn Db) -> usize {
        self.value(db).len()
    }
}

#[salsa::interned]
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
#[salsa::interned]
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
    pub fn with_sorted_unions(self, db: &'db dyn Db) -> Self {
        let elements: Box<[Type<'db>]> = self
            .elements(db)
            .iter()
            .map(|ty| ty.with_sorted_unions(db))
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
    use ruff_db::system::DbWithTestSystem;
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

        let typing_no_default = typing_symbol(&db, "NoDefault").expect_type();
        let typing_extensions_no_default = typing_extensions_symbol(&db, "NoDefault").expect_type();

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
        let a = global_symbol(&db, bar, "a");

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

        let a = global_symbol(&db, bar, "a");

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
