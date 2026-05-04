use compact_str::ToCompactString;
use itertools::Itertools;
use ruff_diagnostics::{Edit, Fix};
use rustc_hash::FxHashMap;

use std::borrow::Cow;
use std::cell::OnceCell;
use std::iter;
use std::rc::Rc;
use std::time::Duration;

use bitflags::bitflags;
use call::{CallDunderError, CallError, CallErrorKind};
use context::InferContext;
use ruff_db::Instant;
use ruff_db::diagnostic::{Annotation, Diagnostic, Span};
use ruff_db::files::File;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_text_size::Ranged;
use smallvec::smallvec_inline;
use ty_module_resolver::{KnownModule, Module, ModuleName, resolve_module};

pub(crate) use self::callable::UpcastPolicy;
pub use self::cyclic::CycleDetector;
pub(crate) use self::cyclic::TypeTransformer;
pub(crate) use self::diagnostic::register_lints;
pub use self::diagnostic::{TypeCheckDiagnostics, UNDEFINED_REVEAL, UNRESOLVED_REFERENCE};
pub(crate) use self::infer::{
    TypeContext, infer_complete_scope_types, infer_deferred_types, infer_definition_types,
    infer_expression_type, infer_expression_types, infer_scope_types,
};
pub(crate) use self::iteration::extract_fixed_length_iterable_element_types;
pub use self::known_instance::KnownInstanceType;
pub(crate) use self::relation_error::{ErrorContext, ErrorContextTree, ParameterDescription};
use self::set_theoretic::KnownUnion;
pub(crate) use self::set_theoretic::builder::{
    IntersectionBuilder, UnionAccumulator, UnionBuilder,
};
pub use self::set_theoretic::{
    IntersectionType, NegativeIntersectionElements, NegativeIntersectionElementsIterator, UnionType,
};
pub use self::signatures::ParameterKind;
pub(crate) use self::signatures::Signature;
pub(crate) use self::subclass_of::{SubclassOfInner, SubclassOfType};
pub use crate::diagnostic::add_inferred_python_version_hint_to_diagnostic;
use crate::place::{
    DefinedPlace, Definedness, Place, PlaceAndQualifiers, TypeOrigin, builtins_module_scope,
    imported_symbol, known_module_symbol,
};
use crate::suppression::check_suppressions;
use crate::types::bound_super::BoundSuperType;
use crate::types::call::bind::ConstructorCallableKind;
use crate::types::call::{Binding, Bindings, CallArguments, CallableBinding};
pub(crate) use crate::types::callable::{CallableType, CallableTypes};
pub(crate) use crate::types::class_base::ClassBase;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::context::{LintDiagnosticGuard, LintDiagnosticGuardBuilder};
use crate::types::diagnostic::{INVALID_AWAIT, INVALID_TYPE_FORM};
pub use crate::types::display::{DisplaySettings, TypeDetail, TypeDisplayDetails};
use crate::types::enums::enum_metadata;
use crate::types::function::{
    DataclassTransformerFlags, DataclassTransformerParams, FunctionDecorators, FunctionSpans,
    FunctionType, KnownFunction,
};
pub(crate) use crate::types::generics::GenericContext;
use crate::types::generics::{
    ApplySpecialization, InferableTypeVars, Specialization, bind_typevar,
};
use crate::types::infer::InferenceFlags;
use crate::types::known_instance::{InternedConstraintSet, InternedType, UnionTypeInstance};
pub use crate::types::method::{BoundMethodType, KnownBoundMethodType, WrapperDescriptorKind};
use crate::types::mro::{MroIterator, StaticMroError};
pub(crate) use crate::types::narrow::{NarrowingConstraint, infer_narrowing_constraint};
use crate::types::newtype::NewType;
pub(crate) use crate::types::signatures::{Parameter, Parameters};
use crate::types::signatures::{ParameterForm, walk_signature};
use crate::types::special_form::TypeQualifier;
use crate::types::tuple::TupleSpec;
use crate::types::type_alias::TypeAliasType;
pub(crate) use crate::types::typed_dict::TypedDictType;
use crate::types::typevar::TypeVarInstance;
pub use crate::types::typevar::{
    BindingContext, BoundTypeVarInstance, ParamSpecAttrKind, TypeVarBoundOrConstraints, TypeVarKind,
};
pub use crate::types::variance::TypeVarVariance;
use crate::types::variance::VarianceInferable;
use crate::types::visitor::any_over_type;
use crate::{Db, FxOrderSet, Program};
pub(crate) use class::{ClassLiteral, ClassType, GenericAlias, StaticClassLiteral};
pub use class::{KnownClass, MethodDecorator};
use instance::Protocol;
pub use instance::{NominalInstanceType, ProtocolInstanceType};
pub(crate) use literal::{
    BytesLiteralType, EnumLiteralType, LiteralValueType, LiteralValueTypeKind, StringLiteralType,
};
pub use special_form::SpecialFormType;
use ty_python_core::definition::Definition;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::ScopeId;
use ty_python_core::{Truthiness, place_table, semantic_index};

mod bool;
mod bound_super;
mod call;
mod callable;
mod class;
mod class_base;
mod constraints;
mod context;
mod context_manager;
mod cyclic;
mod diagnostic;
mod display;
mod enums;
mod function;
mod generics;
pub mod ide_support;
mod infer;
mod instance;
mod iteration;
mod known_instance;
pub mod list_members;
mod literal;
mod member;
mod method;
mod mro;
pub(crate) mod narrow;
mod newtype;
mod overrides;
mod protocol_class;
pub(crate) mod relation;
mod relation_error;
mod set_theoretic;
mod signatures;
mod special_form;
mod string_annotation;
mod subclass_of;
#[cfg(test)]
pub(crate) mod tests;
mod tuple;
mod type_alias;
mod typed_dict;
mod typevar;
mod unpacker;
mod variance;
mod visitor;

mod definition;
#[cfg(test)]
mod property_tests;
mod subscript;

pub fn check_types(db: &dyn Db, file: File) -> Vec<Diagnostic> {
    let _span = tracing::trace_span!("check_types", ?file).entered();
    tracing::debug!("Checking file '{path}'", path = file.path(db));

    let start = Instant::now();

    let index = semantic_index(db, file);
    let mut diagnostics = TypeCheckDiagnostics::default();

    for scope_id in index.scope_ids() {
        // Scopes that may require type context are inferred during the inference of
        // their outer scope.
        if scope_id.accepts_type_context(db) {
            continue;
        }

        let result = infer_scope_types(db, scope_id, TypeContext::default());

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

    let diagnostics = check_suppressions(db, file, diagnostics);

    let elapsed = start.elapsed();
    if elapsed >= Duration::from_millis(100) {
        tracing::info!(
            "Checking file `{path}` took more than 100ms ({elapsed:?})",
            path = file.path(db)
        );
    }

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
        infer_complete_scope_types(db, scope).expression_type(expression)
    }
}

struct ApplyDefaultTypeMapping;
struct ApplyTopMaterialization;
struct ApplyBottomMaterialization;
struct ApplyMaterializationEquivalence;

type MaterializationEquivalenceVisitor<'db> =
    Rc<CycleDetector<ApplyMaterializationEquivalence, (Type<'db>, Type<'db>), bool>>;

/// A [`TypeTransformer`] that is used in `apply_type_mapping` methods.
///
/// Materialization is the only mapping mode that needs to visit the same type under two different
/// mappings within a single recursive call chain (`Top` and `Bottom`). Keep separate cycle caches
/// for those modes so invariant checks can safely reuse one visitor.
pub(crate) struct ApplyTypeMappingVisitor<'db> {
    default: OnceCell<TypeTransformer<'db, ApplyDefaultTypeMapping>>,
    top_materialization: OnceCell<TypeTransformer<'db, ApplyTopMaterialization>>,
    bottom_materialization: OnceCell<TypeTransformer<'db, ApplyBottomMaterialization>>,
    materialization_equivalence: OnceCell<MaterializationEquivalenceVisitor<'db>>,
}

impl<'db> ApplyTypeMappingVisitor<'db> {
    fn materialization_equivalence(&self) -> &MaterializationEquivalenceVisitor<'db> {
        self.materialization_equivalence
            .get_or_init(|| Rc::new(CycleDetector::new(true)))
    }

    pub(crate) fn visit(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        func: impl FnOnce() -> Type<'db>,
    ) -> Type<'db> {
        match type_mapping {
            TypeMapping::Materialize(MaterializationKind::Top) => self
                .top_materialization
                .get_or_init(TypeTransformer::default)
                .visit_type(db, ty, func),
            TypeMapping::Materialize(MaterializationKind::Bottom) => self
                .bottom_materialization
                .get_or_init(TypeTransformer::default)
                .visit_type(db, ty, func),
            _ => self
                .default
                .get_or_init(TypeTransformer::default)
                .visit_type(db, ty, func),
        }
    }

    pub(crate) fn is_equivalent_to_materialization(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> bool {
        self.materialization_equivalence().visit((left, right), || {
            left.is_equivalent_to_with_materialization_visitor(db, right, self)
        })
    }

    pub(crate) fn for_new_materialization_root(&self) -> Self {
        let materialization_equivalence = OnceCell::new();
        let was_empty =
            materialization_equivalence.set(Rc::clone(self.materialization_equivalence()));
        debug_assert!(was_empty.is_ok());

        Self {
            default: OnceCell::new(),
            top_materialization: OnceCell::new(),
            bottom_materialization: OnceCell::new(),
            materialization_equivalence,
        }
    }
}

impl Default for ApplyTypeMappingVisitor<'_> {
    fn default() -> Self {
        Self {
            default: OnceCell::new(),
            top_materialization: OnceCell::new(),
            bottom_materialization: OnceCell::new(),
            materialization_equivalence: OnceCell::new(),
        }
    }
}

/// A [`CycleDetector`] that is used in `find_legacy_typevars` methods.
pub(crate) type FindLegacyTypeVarsVisitor<'db> = CycleDetector<FindLegacyTypeVars, Type<'db>, ()>;

#[derive(Debug)]
pub(crate) struct FindLegacyTypeVars;

/// A [`CycleDetector`] that is used in `visit_specialization` methods.
pub(crate) type SpecializationVisitor<'db> = CycleDetector<VisitSpecialization, Type<'db>, ()>;
pub(crate) struct VisitSpecialization;

/// How a generic type has been specialized.
///
/// This matters only if there is at least one invariant type parameter.
/// For example, we represent `Top[list[Any]]` as a `GenericAlias` with
/// `MaterializationKind` set to Top, which we denote as `Top[list[Any]]`.
/// A type `Top[list[T]]` includes all fully static list types `list[U]` where `U` is
/// a supertype of `Bottom[T]` and a subtype of `Top[T]`.
///
/// Similarly, there is `Bottom[list[Any]]`.
/// This type is harder to make sense of in a set-theoretic framework, but
/// it is a subtype of all materializations of `list[Any]`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum MaterializationKind {
    Top,
    Bottom,
}

impl MaterializationKind {
    /// Flip the materialization type: `Top` becomes `Bottom` and vice versa.
    #[must_use]
    pub const fn flip(self) -> Self {
        match self {
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
        }
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
/// the instance) cannot completely shadow a non-data descriptor of the meta-type (the
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

        /// Do not call `__getattr__` during member lookup.
        const NO_GETATTR_LOOKUP = 1 << 4;
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

    /// Do not call `__getattr__` during member lookup.
    pub(crate) const fn no_getattr_lookup(self) -> bool {
        self.contains(Self::NO_GETATTR_LOOKUP)
    }
}

impl Default for MemberLookupPolicy {
    fn default() -> Self {
        Self::empty()
    }
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
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct PropertyInstanceType<'db> {
    pub getter: Option<Type<'db>>,
    pub setter: Option<Type<'db>>,
    pub deleter: Option<Type<'db>>,
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
    if let Some(deleter) = property.deleter(db) {
        visitor.visit_type(db, deleter);
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for PropertyInstanceType<'_> {}

impl<'db> PropertyInstanceType<'db> {
    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let getter = self
            .getter(db)
            .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor));
        let setter = self
            .setter(db)
            .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor));
        let deleter = self
            .deleter(db)
            .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor));
        Self::new(db, getter, setter, deleter)
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let getter = match self.getter(db) {
            Some(ty) if nested => Some(ty.recursive_type_normalized_impl(db, div, true)?),
            Some(ty) => Some(
                ty.recursive_type_normalized_impl(db, div, true)
                    .unwrap_or(div),
            ),
            None => None,
        };
        let setter = match self.setter(db) {
            Some(ty) if nested => Some(ty.recursive_type_normalized_impl(db, div, true)?),
            Some(ty) => Some(
                ty.recursive_type_normalized_impl(db, div, true)
                    .unwrap_or(div),
            ),
            None => None,
        };
        let deleter = match self.deleter(db) {
            Some(ty) if nested => Some(ty.recursive_type_normalized_impl(db, div, true)?),
            Some(ty) => Some(
                ty.recursive_type_normalized_impl(db, div, true)
                    .unwrap_or(div),
            ),
            None => None,
        };
        Some(Self::new(db, getter, setter, deleter))
    }

    fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        if let Some(ty) = self.getter(db) {
            ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
        if let Some(ty) = self.setter(db) {
            ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
        if let Some(ty) = self.deleter(db) {
            ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
    }
}

bitflags! {
    /// Used to store metadata about a dataclass or dataclass-like class.
    /// For the precise meaning of the fields, see [1].
    ///
    /// [1]: https://docs.python.org/3/library/dataclasses.html
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct DataclassFlags: u16 {
        const INIT = 1 << 0;
        const REPR = 1 << 1;
        const EQ = 1 << 2;
        const ORDER = 1 << 3;
        const UNSAFE_HASH = 1 << 4;
        const FROZEN = 1 << 5;
        const MATCH_ARGS = 1 << 6;
        const KW_ONLY = 1 << 7;
        const SLOTS = 1 << 8   ;
        const WEAKREF_SLOT = 1 << 9;
    }
}

pub(crate) const DATACLASS_FLAGS: &[(&str, DataclassFlags)] = &[
    ("init", DataclassFlags::INIT),
    ("repr", DataclassFlags::REPR),
    ("eq", DataclassFlags::EQ),
    ("order", DataclassFlags::ORDER),
    ("unsafe_hash", DataclassFlags::UNSAFE_HASH),
    ("frozen", DataclassFlags::FROZEN),
    ("match_args", DataclassFlags::MATCH_ARGS),
    ("kw_only", DataclassFlags::KW_ONLY),
    ("slots", DataclassFlags::SLOTS),
    ("weakref_slot", DataclassFlags::WEAKREF_SLOT),
];

impl get_size2::GetSize for DataclassFlags {}

impl Default for DataclassFlags {
    fn default() -> Self {
        Self::INIT | Self::REPR | Self::EQ | Self::MATCH_ARGS
    }
}

impl From<DataclassTransformerFlags> for DataclassFlags {
    fn from(params: DataclassTransformerFlags) -> Self {
        let mut result = Self::default();

        result.set(
            Self::EQ,
            params.contains(DataclassTransformerFlags::EQ_DEFAULT),
        );
        result.set(
            Self::ORDER,
            params.contains(DataclassTransformerFlags::ORDER_DEFAULT),
        );
        result.set(
            Self::KW_ONLY,
            params.contains(DataclassTransformerFlags::KW_ONLY_DEFAULT),
        );
        result.set(
            Self::FROZEN,
            params.contains(DataclassTransformerFlags::FROZEN_DEFAULT),
        );

        result
    }
}

/// Metadata for a dataclass. Stored inside a `Type::DataclassDecorator(…)`
/// instance that we use as the return type of a `dataclasses.dataclass` and
/// dataclass-transformer decorator calls.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct DataclassParams<'db> {
    flags: DataclassFlags,

    #[returns(deref)]
    field_specifiers: Box<[Type<'db>]>,
}

impl get_size2::GetSize for DataclassParams<'_> {}

impl<'db> DataclassParams<'db> {
    fn default_params(db: &'db dyn Db) -> Self {
        Self::from_flags(db, DataclassFlags::default())
    }

    fn from_flags(db: &'db dyn Db, flags: DataclassFlags) -> Self {
        let dataclasses_field = known_module_symbol(db, KnownModule::Dataclasses, "field")
            .place
            .ignore_possibly_undefined()
            .unwrap_or_else(Type::unknown);

        Self::new(db, flags, vec![dataclasses_field].into_boxed_slice())
    }

    fn from_transformer_params(db: &'db dyn Db, params: DataclassTransformerParams<'db>) -> Self {
        Self::new(
            db,
            DataclassFlags::from(params.flags(db)),
            params.field_specifiers(db),
        )
    }
}

/// Representation of a type: a set of possible values at runtime.
///
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum Type<'db> {
    /// The dynamic type: a statically unknown set of values
    Dynamic(DynamicType<'db>),
    /// A cycle marker used during recursive type inference.
    Divergent(DivergentType),
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
    /// Represents a specific instance of a bound method type for a builtin class.
    ///
    /// TODO: consider replacing this with `Callable & types.MethodWrapperType` type?
    /// The `Callable` type would need to be overloaded -- e.g. `types.FunctionType.__get__` has
    /// this behaviour when a method is accessed on a class vs an instance:
    ///
    /// ```txt
    ///  * (None,   type)         ->  Literal[function_on_which_it_was_called]
    ///  * (object, type | None)  ->  BoundMethod[instance, function_on_which_it_was_called]
    /// ```
    KnownBoundMethod(KnownBoundMethodType<'db>),
    /// Represents a specific instance of `types.WrapperDescriptorType`.
    ///
    /// TODO: Similar to above, this could eventually be replaced by a generic `Callable`
    /// type.
    WrapperDescriptor(WrapperDescriptorKind),
    /// A special callable that is returned by a `dataclass(…)` call. It is usually
    /// used as a decorator. Note that this is only used as a return type for actual
    /// `dataclass` calls, not for the argumentless `@dataclass` decorator.
    DataclassDecorator(DataclassParams<'db>),
    /// A special callable that is returned by a `dataclass_transform(…)` call.
    DataclassTransformer(DataclassTransformerParams<'db>),
    /// The type of an arbitrary callable object with a certain specified signature.
    Callable(CallableType<'db>),
    /// A specific module object
    ModuleLiteral(ModuleLiteralType<'db>),
    /// A specific class object (either from a `class` statement or `type()` call)
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
    /// A literal value type.
    LiteralValue(LiteralValueType<'db>),
    /// An instance of a typevar. When the generic class or function binding this typevar is
    /// specialized, we will replace the typevar with its specialization.
    TypeVar(BoundTypeVarInstance<'db>),
    /// A bound super object like `super()` or `super(A, A())`
    /// This type doesn't handle an unbound super object like `super(A)`; for that we just use
    /// a `Type::NominalInstance` of `builtins.super`.
    BoundSuper(BoundSuperType<'db>),
    /// A subtype of `bool` that allows narrowing in both positive and negative cases.
    TypeIs(TypeIsType<'db>),
    /// A subtype of `bool` that allows narrowing in only the positive case.
    TypeGuard(TypeGuardType<'db>),
    /// A type that represents an inhabitant of a `TypedDict`.
    TypedDict(TypedDictType<'db>),
    /// An aliased type (lazily not-yet-unpacked to its value type).
    TypeAlias(TypeAliasType<'db>),
    /// The set of Python objects that belong to a `typing.NewType` subtype. Note that
    /// `typing.NewType` itself is a `Type::ClassLiteral` with `KnownClass::NewType`, and the
    /// identity callables it returns (which behave like subtypes in type expressions) are of
    /// `Type::KnownInstance` with `KnownInstanceType::NewType`. This `Type` refers to the objects
    /// wrapped/returned by a specific one of those identity callables, or by another that inherits
    /// from it.
    NewTypeInstance(NewType<'db>),
}

/// Helper for `recursive_type_normalized_impl` for `TypeGuardLike` types.
fn recursive_type_normalize_type_guard_like<'db, T: TypeGuardLike<'db>>(
    db: &'db dyn Db,
    guard: T,
    div: Type<'db>,
    nested: bool,
) -> Option<Type<'db>> {
    let ty = if nested {
        guard
            .type_argument(db)
            .recursive_type_normalized_impl(db, div, true)?
    } else {
        guard
            .type_argument(db)
            .recursive_type_normalized_impl(db, div, true)
            .unwrap_or(div)
    };
    Some(guard.with_type(db, ty))
}

#[derive(Debug, Clone, Copy)]
#[expect(clippy::struct_field_names)]
struct GeneratorTypes<'db> {
    yield_ty: Option<Type<'db>>,
    send_ty: Option<Type<'db>>,
    return_ty: Option<Type<'db>>,
}

#[salsa::tracked]
impl<'db> Type<'db> {
    pub(crate) const fn any() -> Self {
        Self::Dynamic(DynamicType::Any)
    }

    pub const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub(crate) fn divergent(id: salsa::Id) -> Self {
        Self::Divergent(DivergentType::new(id))
    }

    pub(crate) const fn is_divergent(&self) -> bool {
        matches!(self, Type::Divergent(_))
    }

    /// Returns `true` if both `self` and `other` are `Divergent` types originating from the
    /// same cycle (i.e., sharing the same query ID), regardless of materialization state.
    fn same_divergent_marker(self, other: Type<'db>) -> bool {
        match (self, other) {
            (Type::Divergent(left), Type::Divergent(right)) => left.same_marker(right),
            _ => false,
        }
    }

    /// If `self` is a materialized `Divergent` type, returns the concrete type it should
    /// behave as: `object` for top-materialized, `Never` for bottom-materialized.
    /// Returns `None` if `self` is not `Divergent` or has not been materialized.
    fn materialized_divergent_fallback(self) -> Option<Type<'db>> {
        let Type::Divergent(divergent) = self else {
            return None;
        };

        match divergent.materialization_kind() {
            Some(MaterializationKind::Top) => Some(Type::object()),
            Some(MaterializationKind::Bottom) => Some(Type::Never),
            None => None,
        }
    }

    /// Negating a divergent marker preserves the marker and flips its materialization, if any.
    fn negated_divergent(self) -> Option<Type<'db>> {
        let Type::Divergent(divergent) = self else {
            return None;
        };

        Some(match divergent.materialization_kind() {
            Some(materialization_kind) => {
                Type::Divergent(divergent.materialized(materialization_kind.flip()))
            }
            None => Type::Divergent(divergent),
        })
    }

    pub const fn is_unknown(&self) -> bool {
        matches!(
            self,
            Type::Dynamic(DynamicType::Unknown | DynamicType::UnknownGeneric(_))
        )
    }

    pub(crate) const fn is_never(&self) -> bool {
        matches!(
            self,
            Type::Never
                | Type::Divergent(DivergentType {
                    materialization: Some(MaterializationKind::Bottom),
                    ..
                })
        )
    }

    /// Returns `true` if this type contains a `Self` type variable.
    pub(crate) fn contains_self(&self, db: &'db dyn Db) -> bool {
        any_over_type(db, *self, false, |ty| {
            ty.as_typevar().is_some_and(|tv| tv.typevar(db).is_self(db))
        })
    }

    /// Returns `true` if this type supports eager `Self` binding via `bind_self_typevars`.
    ///
    /// `FunctionLiteral`, `BoundMethod`, and function-like `Callable` types return `false`
    /// because their `Self` binding is deferred to call time via the signature binding path.
    fn supports_self_binding(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::FunctionLiteral(_) | Type::BoundMethod(_) | Type::KnownBoundMethod(_) => false,
            Type::Callable(callable) if callable.is_function_like(db) => false,
            _ => self.contains_self(db),
        }
    }

    /// Bind `Self` type variables in this type to a concrete self type.
    ///
    /// Uses MRO-based matching: a `Self` typevar is only bound if its owner class
    /// is in the MRO of the self type's class.
    ///
    /// Types that defer `Self` binding to call time (functions, bound methods, function-like
    /// callables) are skipped; see `supports_self_binding`.
    pub(crate) fn bind_self_typevars(self, db: &'db dyn Db, self_type: Type<'db>) -> Self {
        if !self.supports_self_binding(db) {
            return self;
        }

        self.apply_type_mapping(
            db,
            &TypeMapping::BindSelf(SelfBinding::new(db, self_type, None)),
            TypeContext::default(),
        )
    }

    /// Returns `true` if `self` is [`Type::Callable`].
    pub(crate) const fn is_callable_type(&self) -> bool {
        matches!(self, Type::Callable(..))
    }

    pub(crate) fn cycle_normalized(
        self,
        db: &'db dyn Db,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        // When we encounter a salsa cycle, we want to avoid oscillating between two or more types
        // without converging on a fixed-point result. Most of the time, we union together the
        // types from each cycle iteration to ensure that our result is monotonic, even if we
        // encounter oscillation.
        //
        // However, for the first couple iterations we are prone to get values including Divergent
        // that will soon converge, but where unioning in the early value causes a loss of
        // precision that we can't recover from. For example, a narrowing condition that looks like
        // `is not Divergent` instead of `is not None` in the first iteration may cause us to lose
        // the effect of that narrowing permanently, due to the union-previous-iteration behavior.
        // So we avoid unioning in the first couple iterations, and just use the later iteration's
        // result directly. We still ensure monotonicity after the first couple iterations, which
        // still ensures convergence in cases that are prone to oscillation.
        if cycle.iteration() <= 1 {
            self
        } else {
            // The current type is unioned to the previous type. Unioning in the reverse order can
            // cause the fixed-point iterations to converge slowly or even fail. Consider the case
            // where the order of union types is different between the previous and current cycle.
            // We should use the previous union type as the base and only add new element types in
            // this cycle, if any.
            UnionType::from_elements_cycle_recovery(db, [previous, self])
        }
        .recursive_type_normalized(db, cycle)
    }

    pub fn is_none(&self, db: &'db dyn Db) -> bool {
        self.is_instance_of(db, KnownClass::NoneType)
    }

    fn is_bool(&self, db: &'db dyn Db) -> bool {
        self.is_instance_of(db, KnownClass::Bool)
    }

    fn is_enum(&self, db: &'db dyn Db) -> bool {
        self.as_nominal_instance().is_some_and(|instance| {
            crate::types::enums::enum_metadata(db, instance.class_literal(db)).is_some()
        })
    }

    fn is_typealias_special_form(&self) -> bool {
        matches!(self, Type::SpecialForm(SpecialFormType::TypeAlias))
    }

    /// Return true if this type overrides __eq__ or __ne__ methods
    fn overrides_equality(&self, db: &'db dyn Db) -> bool {
        let check_dunder = |dunder_name, allowed_return_value| {
            // Note that we do explicitly exclude dunder methods on `object`, `int` and `str` here.
            // The reason for this is that we know that these dunder methods behave in a predictable way.
            // Only custom dunder methods need to be examined here, as they might break single-valuedness
            // by always returning `False`, for example.
            let call_result = self.try_call_dunder_with_policy(
                db,
                dunder_name,
                &mut CallArguments::positional([Type::unknown()]),
                TypeContext::default(),
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                    | MemberLookupPolicy::MRO_NO_INT_OR_STR_LOOKUP,
            );
            let call_result = call_result.as_ref();
            call_result.is_ok_and(|bindings| {
                bindings
                    .return_type(db)
                    .as_literal_value()
                    .and_then(literal::LiteralValueType::as_bool)
                    == Some(allowed_return_value)
            }) || call_result.is_err_and(|err| matches!(err, CallDunderError::MethodNotAvailable))
        };

        !(check_dunder("__eq__", true) && check_dunder("__ne__", false))
    }

    pub fn is_notimplemented(&self, db: &'db dyn Db) -> bool {
        self.is_instance_of(db, KnownClass::NotImplementedType)
    }

    pub(crate) fn is_todo(&self) -> bool {
        self.as_dynamic().is_some_and(|dynamic| match dynamic {
            DynamicType::Any
            | DynamicType::Unknown
            | DynamicType::InvalidConcatenateUnknown
            | DynamicType::UnknownGeneric(_)
            | DynamicType::UnspecializedTypeVar => false,
            DynamicType::Todo(_)
            | DynamicType::TodoStarredExpression
            | DynamicType::TodoUnpack
            | DynamicType::TodoTypeVarTuple => true,
        })
    }

    pub const fn is_generic_alias(&self) -> bool {
        matches!(self, Type::GenericAlias(_))
    }

    /// Returns whether this type represents a specialization of a generic type.
    ///
    /// For example, whereas `<class 'list'>` is a generic type, `<class 'list[int]'>`
    /// is a specialization of that type.
    pub(crate) fn is_specialized_generic(self, db: &'db dyn Db) -> bool {
        match self {
            Type::Union(union) => union
                .elements(db)
                .iter()
                .any(|ty| ty.is_specialized_generic(db)),
            Type::Intersection(intersection) => {
                intersection
                    .positive(db)
                    .iter()
                    .any(|ty| ty.is_specialized_generic(db))
                    || intersection
                        .negative(db)
                        .iter()
                        .any(|ty| ty.is_specialized_generic(db))
            }
            Type::NominalInstance(instance_type) => instance_type.is_definition_generic(),
            Type::ProtocolInstance(protocol) => {
                matches!(protocol.inner, Protocol::FromClass(class) if class.is_generic())
            }
            Type::TypedDict(typed_dict) => typed_dict
                .defining_class()
                .is_some_and(ClassType::is_generic),
            Type::Dynamic(dynamic) => {
                matches!(dynamic, DynamicType::UnknownGeneric(_))
            }
            // Due to inheritance rules, enums cannot be generic.
            Type::LiteralValue(literal) if literal.is_enum() => false,
            // Once generic NewType is officially specified, handle it.
            _ => false,
        }
    }

    pub(crate) const fn is_dynamic(&self) -> bool {
        matches!(
            self,
            Type::Dynamic(_)
                | Type::Divergent(DivergentType {
                    materialization: None,
                    ..
                })
        )
    }

    const fn is_non_divergent_dynamic(&self) -> bool {
        self.is_dynamic() && !self.is_divergent()
    }

    /// Returns `true` if this type is an awaitable that should be awaited before being discarded.
    ///
    /// Currently checks for instances of `types.CoroutineType` (returned by `async def` calls).
    /// Unions are considered awaitable only if every element is awaitable.
    /// Intersections are considered awaitable if any positive element is awaitable.
    pub(crate) fn is_awaitable(self, db: &'db dyn Db) -> bool {
        match self {
            Type::NominalInstance(instance) => {
                matches!(instance.known_class(db), Some(KnownClass::CoroutineType))
            }
            Type::Union(union) => {
                let elements = union.elements(db);
                // Guard against empty unions (`Never`), since `all()` on an empty
                // iterator returns `true`.
                !elements.is_empty() && elements.iter().all(|ty| ty.is_awaitable(db))
            }
            Type::Intersection(intersection) => intersection
                .positive(db)
                .iter()
                .any(|ty| ty.is_awaitable(db)),
            _ => false,
        }
    }

    /// Is a value of this type only usable in typing contexts?
    pub fn is_type_check_only(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::ClassLiteral(class_literal) => class_literal.type_check_only(db),
            Type::FunctionLiteral(f) => {
                f.has_known_decorator(db, FunctionDecorators::TYPE_CHECK_ONLY)
            }
            _ => false,
        }
    }

    /// Returns whether this type is marked as deprecated via `@warnings.deprecated`.
    pub fn is_deprecated(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::FunctionLiteral(f) => f.implementation_deprecated(db).is_some(),
            Type::ClassLiteral(c) => c.deprecated(db).is_some(),
            _ => false,
        }
    }

    /// If the type is a specialized instance of the given `KnownClass`, returns the specialization.
    pub(crate) fn known_specialization(
        &self,
        db: &'db dyn Db,
        known_class: KnownClass,
    ) -> Option<Specialization<'db>> {
        let class_literal = known_class.try_to_class_literal(db)?;
        self.specialization_of(db, class_literal)
    }

    /// If the type is a specialized instance of the given class, returns the specialization.
    pub(crate) fn specialization_of(
        self,
        db: &'db dyn Db,
        expected_class: StaticClassLiteral<'_>,
    ) -> Option<Specialization<'db>> {
        self.specialization_of_optional(db, Some(expected_class))
    }

    /// If this type is a class instance, returns its specialization.
    pub(crate) fn class_specialization(self, db: &'db dyn Db) -> Option<Specialization<'db>> {
        self.specialization_of_optional(db, None)
    }

    /// If this type is a class instance, returns its class.
    pub(crate) fn nominal_class(self, db: &'db dyn Db) -> Option<ClassType<'db>> {
        match self {
            Type::NominalInstance(instance) => Some(instance.class(db)),
            Type::ProtocolInstance(instance) => instance.to_nominal_instance().map(|i| i.class(db)),
            Type::TypeAlias(alias) => alias.value_type(db).nominal_class(db),
            Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db).nominal_class(db),
            Type::TypeVar(typevar) => {
                let TypeVarBoundOrConstraints::UpperBound(bound) =
                    typevar.typevar(db).bound_or_constraints(db)?
                else {
                    return None;
                };
                bound.nominal_class(db)
            }
            _ => None,
        }
    }

    fn specialization_of_optional(
        self,
        db: &'db dyn Db,
        expected_class: Option<StaticClassLiteral<'_>>,
    ) -> Option<Specialization<'db>> {
        let class_type = self.nominal_class(db)?;

        let (class_literal, specialization) = class_type.static_class_literal(db)?;
        if expected_class.is_some_and(|expected_class| expected_class != class_literal) {
            return None;
        }

        specialization
    }

    /// Returns `true` if this type may contain preferred type mappings when provided as type context
    /// during generic call inference.
    ///
    /// This is the case for any type which may contain types in non-covariant position within it,
    /// e.g., nominal instances of a generic class, or callables.
    pub(crate) fn may_prefer_declared_type(self, db: &'db dyn Db) -> bool {
        self.class_specialization(db).is_some() || self.expand_eagerly(db).is_callable_type()
    }

    /// Returns the top materialization (or upper bound materialization) of this type, which is the
    /// most general form of the type that is fully static.
    #[must_use]
    pub(crate) fn top_materialization(&self, db: &'db dyn Db) -> Type<'db> {
        self.materialize(
            db,
            MaterializationKind::Top,
            &ApplyTypeMappingVisitor::default(),
        )
    }

    /// Returns the bottom materialization (or lower bound materialization) of this type, which is
    /// the most specific form of the type that is fully static.
    #[must_use]
    pub(crate) fn bottom_materialization(&self, db: &'db dyn Db) -> Type<'db> {
        self.materialize(
            db,
            MaterializationKind::Bottom,
            &ApplyTypeMappingVisitor::default(),
        )
    }

    /// If this type is an instance type where the class has a tuple spec, returns the tuple spec.
    ///
    /// I.e., for the type `tuple[int, str]`, this will return the tuple spec `[int, str]`.
    /// For a subclass of `tuple[int, str]`, it will return the same tuple spec.
    fn tuple_instance_spec(&self, db: &'db dyn Db) -> Option<Cow<'db, TupleSpec<'db>>> {
        self.as_nominal_instance()
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
        self.as_nominal_instance()
            .and_then(|instance| instance.own_tuple_spec(db))
    }

    /// Returns the materialization of this type depending on the given `variance`.
    ///
    /// More concretely, `T'`, the materialization of `T`, is the type `T` with all occurrences of
    /// the dynamic types (`Any`, `Unknown`, `Todo`) replaced as follows:
    ///
    /// - In covariant position, it's replaced with `object` (TODO: it should be the `TypeVar`'s upper
    ///   bound, if any)
    /// - In contravariant position, it's replaced with `Never`
    /// - In invariant position, we replace the object with a special form recording that it's the top
    ///   or bottom materialization.
    ///
    /// This is implemented as a type mapping. Some specific objects have `materialize()` or
    /// `materialize_impl()` methods. The rule of thumb is:
    ///
    /// - `materialize()` calls `apply_type_mapping()` (or `apply_type_mapping_impl()`)
    /// - `materialize_impl()` gets called from `apply_type_mapping()` or from another
    ///   `materialize_impl()`
    pub(crate) fn materialize(
        &self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Type<'db> {
        self.apply_type_mapping_impl(
            db,
            &TypeMapping::Materialize(materialization_kind),
            TypeContext::default(),
            visitor,
        )
    }

    pub(crate) fn has_dynamic(self, db: &'db dyn Db) -> bool {
        any_over_type(db, self, false, |ty| ty.is_dynamic())
    }

    pub(crate) const fn as_special_form(self) -> Option<SpecialFormType> {
        match self {
            Type::SpecialForm(special_form) => Some(special_form),
            _ => None,
        }
    }

    pub const fn as_property_instance(self) -> Option<PropertyInstanceType<'db>> {
        match self {
            Type::PropertyInstance(property) => Some(property),
            _ => None,
        }
    }

    pub const fn as_class_literal(self) -> Option<ClassLiteral<'db>> {
        match self {
            Type::ClassLiteral(class_type) => Some(class_type),
            _ => None,
        }
    }

    pub(crate) const fn as_type_alias(self) -> Option<TypeAliasType<'db>> {
        match self {
            Type::KnownInstance(KnownInstanceType::TypeAliasType(type_alias)) => Some(type_alias),
            _ => None,
        }
    }

    /// If this type is a `Type::TypeAlias`, recursively resolves it to its
    /// underlying value type. Otherwise, returns `self` unchanged.
    pub(crate) fn resolve_type_alias(self, db: &'db dyn Db) -> Type<'db> {
        let mut ty = self;
        while let Type::TypeAlias(alias) = ty {
            ty = alias.value_type(db);
        }
        ty
    }

    /// Returns `Some(UnionType)` if this type behaves like a union. Apart from explicit unions,
    /// this returns `Some` for `TypeAlias`es of unions and `NewType`s of `float` and `complex`.
    pub(crate) fn as_union_like(self, db: &'db dyn Db) -> Option<UnionType<'db>> {
        match self.resolve_type_alias(db) {
            Type::Union(union) => Some(union),
            Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db).as_union_like(db),
            _ => None,
        }
    }

    pub(crate) const fn as_dynamic(self) -> Option<DynamicType<'db>> {
        match self {
            Type::Dynamic(dynamic_type) => Some(dynamic_type),
            _ => None,
        }
    }

    pub(crate) const fn as_callable(self) -> Option<CallableType<'db>> {
        match self {
            Type::Callable(callable_type) => Some(callable_type),
            _ => None,
        }
    }

    pub(crate) const fn expect_dynamic(self) -> DynamicType<'db> {
        self.as_dynamic().expect("Expected a Type::Dynamic variant")
    }

    pub(crate) const fn as_protocol_instance(self) -> Option<ProtocolInstanceType<'db>> {
        match self {
            Type::ProtocolInstance(instance) => Some(instance),
            _ => None,
        }
    }

    #[track_caller]
    pub(crate) const fn expect_class_literal(self) -> ClassLiteral<'db> {
        self.as_class_literal()
            .expect("Expected a Type::ClassLiteral variant")
    }

    pub const fn is_subclass_of(&self) -> bool {
        matches!(self, Type::SubclassOf(..))
    }

    pub const fn is_class_literal(&self) -> bool {
        matches!(self, Type::ClassLiteral(..))
    }

    pub(crate) const fn as_literal_value(self) -> Option<LiteralValueType<'db>> {
        match self {
            Type::LiteralValue(literal) => Some(literal),
            _ => None,
        }
    }

    pub(crate) fn as_literal_value_kind(self) -> Option<LiteralValueTypeKind<'db>> {
        match self {
            Type::LiteralValue(literal) => Some(literal.kind()),
            _ => None,
        }
    }

    pub(crate) const fn is_typed_dict(&self) -> bool {
        matches!(self, Type::TypedDict(..))
    }

    pub(crate) const fn as_typed_dict(self) -> Option<TypedDictType<'db>> {
        match self {
            Type::TypedDict(typed_dict) => Some(typed_dict),
            _ => None,
        }
    }

    /// Turn a class literal (`Type::ClassLiteral` or `Type::GenericAlias`) into a `ClassType`.
    /// Since a `ClassType` must be specialized, apply the default specialization to any
    /// unspecialized generic class literal.
    pub(crate) fn to_class_type(self, db: &'db dyn Db) -> Option<ClassType<'db>> {
        match self {
            Type::ClassLiteral(class_literal) => Some(class_literal.default_specialization(db)),
            Type::GenericAlias(alias) => Some(ClassType::Generic(alias)),
            _ => None,
        }
    }

    pub const fn is_property_instance(&self) -> bool {
        matches!(self, Type::PropertyInstance(..))
    }

    pub(crate) fn module_literal(
        db: &'db dyn Db,
        importing_file: File,
        submodule: Module<'db>,
    ) -> Self {
        Self::ModuleLiteral(ModuleLiteralType::new(
            db,
            submodule,
            submodule.kind(db).is_package().then_some(importing_file),
        ))
    }

    pub(crate) const fn as_module_literal(self) -> Option<ModuleLiteralType<'db>> {
        match self {
            Type::ModuleLiteral(module) => Some(module),
            _ => None,
        }
    }

    pub(crate) const fn is_union(self) -> bool {
        matches!(self, Type::Union(_))
    }

    pub const fn as_union(self) -> Option<UnionType<'db>> {
        match self {
            Type::Union(union_type) => Some(union_type),
            _ => None,
        }
    }

    #[track_caller]
    pub(crate) const fn expect_union(self) -> UnionType<'db> {
        self.as_union().expect("Expected a Type::Union variant")
    }

    pub(crate) const fn is_intersection(self) -> bool {
        matches!(self, Type::Intersection(_))
    }

    /// Returns whether this is a "real" intersection type. (Negated types are represented by an
    /// intersection containing a single negative branch, which this method does _not_ consider a
    /// "real" intersection.)
    pub(crate) fn is_nontrivial_intersection(self, db: &'db dyn Db) -> bool {
        match self {
            Type::Intersection(intersection) => !intersection.is_simple_negation(db),
            _ => false,
        }
    }

    /// Returns the number of union clauses in this type. If the type is not a union, returns 1.
    pub(crate) fn union_size(self, db: &'db dyn Db) -> usize {
        match self {
            Type::Union(union_type) => union_type.elements(db).len(),
            Type::Never => 0,
            _ => 1,
        }
    }

    /// Returns the number of intersection clauses in this type. If the type is a union, this is
    /// the maximum of the `intersection_size` of each union element. If the type is not a union
    /// nor an intersection, returns 1.
    pub(crate) fn intersection_size(self, db: &'db dyn Db) -> usize {
        match self {
            Type::Intersection(intersection) => {
                intersection.positive(db).len() + intersection.negative(db).len()
            }
            Type::Union(union_type) => union_type
                .elements(db)
                .iter()
                .map(|element| element.intersection_size(db))
                .max()
                .unwrap_or(1),
            _ => 1,
        }
    }

    pub const fn as_function_literal(self) -> Option<FunctionType<'db>> {
        match self {
            Type::FunctionLiteral(function_type) => Some(function_type),
            _ => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    pub(crate) fn expect_function_literal(self) -> FunctionType<'db> {
        self.as_function_literal()
            .expect("Expected a Type::FunctionLiteral variant")
    }

    pub(crate) const fn is_function_literal(&self) -> bool {
        matches!(self, Type::FunctionLiteral(..))
    }

    pub(crate) fn as_string_literal(self) -> Option<StringLiteralType<'db>> {
        match self {
            Type::LiteralValue(literal) => literal.as_string(),
            _ => None,
        }
    }

    pub(crate) fn as_int_literal(self) -> Option<i64> {
        match self {
            Type::LiteralValue(literal) => literal.as_int(),
            _ => None,
        }
    }

    pub(crate) fn as_int_like_literal(self) -> Option<i64> {
        match self.as_literal_value_kind() {
            Some(LiteralValueTypeKind::Int(value)) => Some(value.as_i64()),
            Some(LiteralValueTypeKind::Bool(value)) => Some(i64::from(value)),
            _ => None,
        }
    }

    pub(crate) fn as_enum_literal(self) -> Option<EnumLiteralType<'db>> {
        match self {
            Type::LiteralValue(literal) => literal.as_enum(),
            _ => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    pub(crate) fn expect_enum_literal(self) -> EnumLiteralType<'db> {
        match self.as_literal_value_kind() {
            Some(LiteralValueTypeKind::Enum(e)) => e,
            _ => panic!("Expected a `LiteralValueTypeKind::Enum` variant"),
        }
    }

    pub(crate) fn is_string_literal(&self) -> bool {
        self.as_literal_value()
            .is_some_and(literal::LiteralValueType::is_string)
    }

    /// Detects types which are valid to appear inside a `Literal[…]` type annotation.
    pub(crate) fn is_literal_or_union_of_literals(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::Union(union) => union
                .elements(db)
                .iter()
                .all(|ty| ty.is_literal_or_union_of_literals(db)),
            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::String(_)
                | LiteralValueTypeKind::Bytes(_)
                | LiteralValueTypeKind::Int(_)
                | LiteralValueTypeKind::Bool(_)
                | LiteralValueTypeKind::Enum(_) => true,
                LiteralValueTypeKind::LiteralString => false,
            },
            Type::NominalInstance(_) => self.is_none(db) || self.is_bool(db) || self.is_enum(db),
            _ => false,
        }
    }

    pub(crate) fn is_union_of_single_valued(&self, db: &'db dyn Db) -> bool {
        let ty = self.resolve_type_alias(db);
        ty.as_union().is_some_and(|union| {
            union.elements(db).iter().all(|ty| {
                ty.is_single_valued(db)
                    || ty.is_bool(db)
                    || ty.is_subtype_of(db, Type::literal_string())
                    || (ty.is_enum(db) && !ty.overrides_equality(db))
            })
        }) || ty.is_bool(db)
            || ty.is_subtype_of(db, Type::literal_string())
            || (ty.is_enum(db) && !ty.overrides_equality(db))
    }

    pub(crate) fn is_union_with_single_valued(&self, db: &'db dyn Db) -> bool {
        let ty = self.resolve_type_alias(db);
        ty.as_union().is_some_and(|union| {
            union.elements(db).iter().any(|ty| {
                ty.is_single_valued(db)
                    || ty.is_bool(db)
                    || ty.is_subtype_of(db, Type::literal_string())
                    || (ty.is_enum(db) && !ty.overrides_equality(db))
            })
        }) || ty.is_bool(db)
            || ty.is_subtype_of(db, Type::literal_string())
            || (ty.is_enum(db) && !ty.overrides_equality(db))
    }

    /// Create a promotable string literal.
    pub(crate) fn string_literal(db: &'db dyn Db, string: &str) -> Self {
        Self::LiteralValue(LiteralValueType::promotable(StringLiteralType::new(
            db, string,
        )))
    }

    /// Create a promotable enum literal.
    pub(crate) fn enum_literal(value: EnumLiteralType<'db>) -> Self {
        Self::LiteralValue(LiteralValueType::promotable(value))
    }

    /// Create a promotable integer literal.
    pub(crate) fn int_literal(int: i64) -> Self {
        Self::LiteralValue(LiteralValueType::promotable(int))
    }

    /// Create a promotable single-character string literal.
    pub(crate) fn single_char_string_literal(db: &'db dyn Db, c: char) -> Self {
        Self::LiteralValue(LiteralValueType::promotable(StringLiteralType::new(
            db,
            c.to_compact_string(),
        )))
    }

    /// Create a promotable bytes literal.
    pub(crate) fn bytes_literal(db: &'db dyn Db, bytes: &[u8]) -> Self {
        Self::LiteralValue(LiteralValueType::promotable(BytesLiteralType::new(
            db, bytes,
        )))
    }

    /// Create a promotable boolean literal.
    pub fn bool_literal(value: bool) -> Self {
        Self::LiteralValue(LiteralValueType::promotable(value))
    }

    /// Create a `LiteralString`.
    pub(crate) fn literal_string() -> Self {
        // Note that `LiteralString`s are never implicitly inferred, and so are always unpromotable.
        Self::LiteralValue(LiteralValueType::unpromotable(
            LiteralValueTypeKind::LiteralString,
        ))
    }

    pub(crate) fn typed_dict(defining_class: impl Into<ClassType<'db>>) -> Self {
        Self::TypedDict(TypedDictType::new(defining_class.into()))
    }

    #[must_use]
    pub(crate) fn negate(&self, db: &'db dyn Db) -> Type<'db> {
        // Avoid invoking the `IntersectionBuilder` for negations that are trivial.
        //
        // We verify that this always produces the same result as
        // `IntersectionBuilder::new(db).add_negative(*self).build()` via the
        // property test `all_negated_types_identical_to_intersection_with_single_negated_element`
        match self {
            Type::Never => Type::object(),

            Type::Dynamic(_) => *self,

            Type::Divergent(_) => (*self)
                .negated_divergent()
                .expect("matched `Type::Divergent` above"),

            Type::NominalInstance(instance) if instance.is_object() => Type::Never,

            Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::KnownBoundMethod(_)
            | Type::KnownInstance(_)
            | Type::SpecialForm(_)
            | Type::BoundSuper(_)
            | Type::FunctionLiteral(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeVar(_)
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_)
            | Type::NominalInstance(_)
            | Type::ProtocolInstance(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::GenericAlias(_)
            | Type::SubclassOf(_)
            | Type::PropertyInstance(_)
            | Type::LiteralValue(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::Callable(_)
            | Type::WrapperDescriptor(_)
            | Type::TypeAlias(_)
            | Type::BoundMethod(_) => Type::Intersection(IntersectionType::new(
                db,
                FxOrderSet::default(),
                NegativeIntersectionElements::Single(*self),
            )),

            Type::Union(_) | Type::Intersection(_) => {
                IntersectionBuilder::new(db).add_negative(*self).build()
            }
        }
    }

    #[must_use]
    pub(crate) fn negate_if(&self, db: &'db dyn Db, yes: bool) -> Type<'db> {
        if yes { self.negate(db) } else { *self }
    }

    /// Return `true` if it is possible to spell an equivalent type to this one
    /// in user annotations without nonstandard extensions to the type system
    pub(crate) fn is_spellable(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::LiteralValue(_)
            | Type::Never
            | Type::NewTypeInstance(_)
            | Type::NominalInstance(_)
            // `TypedDict` and `Protocol` can be synthesized,
            // but it's always possible to create an equivalent type using a class definition.
            | Type::TypedDict(_)
            | Type::ProtocolInstance(_)
            // Not all `Callable` types are spellable using the `Callable` type form,
            // but they are all spellable using callback protocols.
            | Type::Callable(_)
            // `Unknown` and `@Todo` are nonstandard extensions,
            // but they are both exactly equivalent to `Any`
            | Type::Dynamic(_)
            | Type::TypeVar(_)
            | Type::TypeAlias(_)
            | Type::SubclassOf(_)=> true,
            Type::Intersection(_)
            | Type::Divergent(_)
            | Type::SpecialForm(_)
            | Type::BoundSuper(_)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::PropertyInstance(_)
            | Type::FunctionLiteral(_)
            | Type::ModuleLiteral(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ClassLiteral(_)
            | Type::GenericAlias(_)
            | Type::KnownInstance(_) => false,
            Type::Union(union) => union.elements(db).iter().all(|ty| ty.is_spellable(db)),
        }
    }

    /// Return `true` if `self` is a type that is suitable for displaying
    /// in a "Did you mean...?" hint message in diagnostics
    fn is_hintable(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::NominalInstance(_)
            | Type::NewTypeInstance(_)
            | Type::LiteralValue(_)
            | Type::TypeAlias(_) => true,

            Type::Intersection(_)
            | Type::Divergent(_)
            | Type::SpecialForm(_)
            | Type::BoundSuper(_)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::PropertyInstance(_)
            | Type::FunctionLiteral(_)
            | Type::ModuleLiteral(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ClassLiteral(_)
            | Type::GenericAlias(_)
            | Type::KnownInstance(_) => false,

            // `Never` is spellable and could result from an explicit type annotation,
            // but also could just be the result of us inferring an unreachable region.
            // Best to avoid showing it in hints.
            Type::Never => false,

            // All `Callable` types are spellable in some way,
            // but they're generally not spellable with the syntax we use by default
            // in our type display
            Type::Callable(_) => false,

            Type::SubclassOf(subclass_of) => match subclass_of.subclass_of() {
                SubclassOfInner::Class(_) => true,
                SubclassOfInner::Dynamic(dynamic) => Type::Dynamic(dynamic).is_hintable(db),
                SubclassOfInner::TypeVar(tvar) => Type::TypeVar(tvar).is_hintable(db),
            },

            Type::TypeVar(tvar) => tvar.typevar(db).definition(db).is_some(),

            Type::Union(union) => union.elements(db).iter().all(|ty| ty.is_hintable(db)),

            Type::TypedDict(td) => td.defining_class().is_some(),

            Type::ProtocolInstance(ProtocolInstanceType { inner, .. }) => !inner.is_synthesized(),

            Type::Dynamic(dynamic) => match dynamic {
                DynamicType::Any => true,
                DynamicType::Unknown
                | DynamicType::UnknownGeneric(_)
                | DynamicType::UnspecializedTypeVar
                | DynamicType::TodoUnpack
                | DynamicType::TodoTypeVarTuple
                | DynamicType::Todo(_)
                | DynamicType::InvalidConcatenateUnknown
                | DynamicType::TodoStarredExpression => false,
            },
        }
    }

    /// If the type is a union (or a type alias that resolves to a union), filters union elements
    /// based on the provided predicate.
    ///
    /// Otherwise, returns the type unchanged.
    pub(crate) fn filter_union(
        self,
        db: &'db dyn Db,
        f: impl FnMut(&Type<'db>) -> bool,
    ) -> Type<'db> {
        if let Type::Union(union) = self.resolve_type_alias(db) {
            union.filter(db, f)
        } else {
            self
        }
    }

    /// If the type is a union, removes union elements that are disjoint from `target`.
    ///
    /// Otherwise, returns the type unchanged.
    pub(crate) fn filter_disjoint_elements(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> Type<'db> {
        let constraints = ConstraintSetBuilder::new();
        self.filter_union(db, |elem| {
            !elem
                .when_disjoint_from(db, target, &constraints, inferable)
                .is_always_satisfied(db)
        })
    }

    /// Returns the fallback instance type that a literal is an instance of, or `None` if the type
    /// is not a literal.
    pub(crate) fn literal_fallback_instance(self, db: &'db dyn Db) -> Option<Type<'db>> {
        // There are other literal types that could conceivable be included here: class literals
        // falling back to `type[X]`, for instance. For now, there is not much rigorous thought put
        // into what's included vs not; this is just an empirical choice that makes our ecosystem
        // report look better until we have proper bidirectional type inference.
        match self {
            Type::ModuleLiteral(_) => Some(KnownClass::ModuleType.to_instance(db)),
            Type::FunctionLiteral(_) => Some(KnownClass::FunctionType.to_instance(db)),
            Type::LiteralValue(literal) => Some(literal.fallback_instance(db)),
            _ => None,
        }
    }

    /// Promote (possibly nested) literals to types that these literals are instances of.
    ///
    /// Note that this function tries to promote literals to a more user-friendly form than their
    /// fallback instance type. For example, `def _() -> int` is promoted to `Callable[[], int]`,
    /// as opposed to `FunctionType`.
    pub(crate) fn promote(self, db: &'db dyn Db) -> Type<'db> {
        self.apply_type_mapping(
            db,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular),
            TypeContext::default(),
        )
    }

    /// Promote a top-level singleton type (like `None`, `EllipsisType`) to `T | Unknown`.
    pub(crate) fn promote_singletons(self, db: &'db dyn Db) -> Type<'db> {
        self.promote_singletons_impl(db)
    }

    /// Recursively promote singleton types (like `None`, `EllipsisType`) to
    /// `T | Unknown` within nominal type parameters, without recursing into unions.
    /// Used for collection literal inference so that `[None]` is inferred as
    /// `list[None | Unknown]` rather than `list[None]`.
    pub(crate) fn promote_singletons_recursively(self, db: &'db dyn Db) -> Type<'db> {
        self.apply_type_mapping(
            db,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::SingletonsOnly),
            TypeContext::default(),
        )
    }

    /// Like [`Type::promote`], but does not recurse into nested types.
    fn promote_impl(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::LiteralValue(literal) if literal.is_promotable() => literal.fallback_instance(db),
            Type::FunctionLiteral(literal) => Type::Callable(literal.into_callable_type(db)),
            _ => self,
        }
    }

    /// Like [`Type::promote_singletons_recursively`], but does not recurse into nested types.
    fn promote_singletons_impl(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::NominalInstance(instance) if instance.is_singleton(db) => {
                UnionType::from_two_elements(db, self, Type::unknown())
            }
            _ => self,
        }
    }

    /// Performs nest reduction for recursive types (types that contain `Divergent` types).
    /// For example, consider the following implicit attribute inference:
    /// ```python
    /// class C:
    ///     def f(self, other: "C"):
    ///         self.x = (other.x, 1)
    ///
    /// reveal_type(C().x) # revealed: Unknown | tuple[Divergent, Literal[1]]
    /// ```
    ///
    /// A query that performs implicit attribute type inference enters a cycle because the attribute is recursively defined, and the cycle initial value is set to `Divergent`.
    /// In the next (1st) cycle it is inferred to be `tuple[Divergent, Literal[1]]`, and in the 2nd cycle it becomes `tuple[tuple[Divergent, Literal[1]], Literal[1]]`.
    /// If this continues, the query will not converge, so this method is called in the cycle recovery function.
    /// Then `tuple[tuple[Divergent, Literal[1]], Literal[1]]` is replaced with `tuple[Divergent, Literal[1]]` and the query converges.
    #[must_use]
    pub(crate) fn recursive_type_normalized(self, db: &'db dyn Db, cycle: &salsa::Cycle) -> Self {
        cycle.head_ids().fold(self, |ty, id| {
            ty.recursive_type_normalized_impl(db, Type::divergent(id), false)
                .unwrap_or(Type::divergent(id))
        })
    }

    /// Normalizes types including divergent types (recursive types), which is necessary for convergence of fixed-point iteration.
    /// When `nested` is true, propagate `None`. That is, if the type contains a `Divergent` type, the return value of this method is `None` (so we can use the `?` operator).
    /// When `nested` is false, create a type containing `Divergent` types instead of propagating `None` (we should use `unwrap_or(Divergent)`).
    /// This is to preserve the structure of the non-divergent parts of the type instead of completely collapsing the type containing a `Divergent` type into a `Divergent` type.
    /// ```python
    /// tuple[tuple[Divergent, Literal[1]], Literal[1]].recursive_type_normalized(nested: false)
    /// => tuple[
    ///     tuple[Divergent, Literal[1]].recursive_type_normalized_impl(nested: true).unwrap_or(Divergent),
    ///     Literal[1].recursive_type_normalized_impl(nested: true).unwrap_or(Divergent)
    /// ]
    /// => tuple[Divergent, Literal[1]]
    /// ```
    /// Generic nominal types such as `list[T]` and `tuple[T]` should send `nested=true` for `T`. This is necessary for normalization.
    /// Structural types such as union and intersection do not need to send `nested=true` for element types; that is, types that are "flat" from the perspective of recursive types. `T | U` should send `nested` as is for `T`, `U`.
    /// For other types, the decision depends on whether they are interpreted as nominal or structural.
    /// For example, `KnownInstanceType::UnionType` should simply send `nested` as is.
    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        if nested && self.same_divergent_marker(div) {
            return None;
        }
        match self {
            Type::Union(union) => union.recursive_type_normalized_impl(db, div, nested),
            Type::Intersection(intersection) => intersection
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::Intersection),
            Type::Callable(callable) => callable
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::Callable),
            Type::ProtocolInstance(protocol) => protocol
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::ProtocolInstance),
            Type::NominalInstance(instance) => instance
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::NominalInstance),
            Type::FunctionLiteral(function) => function
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::FunctionLiteral),
            Type::PropertyInstance(property) => property
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::PropertyInstance),
            Type::KnownBoundMethod(method_kind) => method_kind
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::KnownBoundMethod),
            Type::BoundMethod(method) => method
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::BoundMethod),
            Type::BoundSuper(bound_super) => bound_super
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::BoundSuper),
            Type::GenericAlias(generic) => generic
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::GenericAlias),
            Type::SubclassOf(subclass_of) => subclass_of
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::SubclassOf),
            Type::TypeVar(_) => Some(self),
            Type::KnownInstance(known_instance) => known_instance
                .recursive_type_normalized_impl(db, div, nested)
                .map(Type::KnownInstance),
            Type::TypeIs(type_is) => {
                recursive_type_normalize_type_guard_like(db, type_is, div, nested)
            }
            Type::TypeGuard(type_guard) => {
                recursive_type_normalize_type_guard_like(db, type_guard, div, nested)
            }
            Type::Divergent(_) => Some(self),
            Type::Dynamic(dynamic) => Some(Type::Dynamic(dynamic.recursive_type_normalized())),
            Type::TypedDict(_) => {
                // TODO: Normalize TypedDicts
                Some(self)
            }
            Type::TypeAlias(_) => Some(self),
            Type::NewTypeInstance(newtype) => newtype
                .try_map_base_class_type(db, |class_type| {
                    class_type.recursive_type_normalized_impl(db, div, nested)
                })
                .map(Type::NewTypeInstance),
            Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::Never
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::SpecialForm(_)
            | Type::LiteralValue(_) => Some(self),
        }
    }

    /// Recursively visit the specialization of a generic class instance.
    ///
    /// The provided closure will be called on any nested types, along with their variance with
    /// respect to the outermost type.
    pub(crate) fn visit_specialization<F>(self, db: &'db dyn Db, mut f: F)
    where
        F: FnMut(Type<'db>, TypeVarVariance),
    {
        self.visit_specialization_impl(
            db,
            TypeVarVariance::Covariant,
            &mut f,
            &SpecializationVisitor::default(),
        );
    }

    fn visit_specialization_impl(
        self,
        db: &'db dyn Db,
        polarity: TypeVarVariance,
        f: &mut dyn FnMut(Type<'db>, TypeVarVariance),
        visitor: &SpecializationVisitor<'db>,
    ) {
        let Some(specialization) = self.class_specialization(db) else {
            match self {
                Type::Union(union) => {
                    for element in union.elements(db) {
                        element.visit_specialization_impl(db, polarity, f, visitor);
                    }
                }
                Type::Intersection(intersection) => {
                    for element in intersection.positive(db) {
                        element.visit_specialization_impl(db, polarity, f, visitor);
                    }
                }
                Type::TypeAlias(alias) => visitor.visit(self, || {
                    alias
                        .value_type(db)
                        .visit_specialization_impl(db, polarity, f, visitor);
                }),
                Type::Callable(callable) => {
                    for signature in callable.signatures(db) {
                        for parameter in signature.parameters() {
                            let variance = TypeVarVariance::Contravariant.compose(polarity);

                            f(parameter.annotated_type(), variance);

                            visitor.visit(parameter.annotated_type(), || {
                                parameter
                                    .annotated_type()
                                    .visit_specialization_impl(db, variance, f, visitor);
                            });
                        }

                        visitor.visit(signature.return_ty, || {
                            signature
                                .return_ty
                                .visit_specialization_impl(db, polarity, f, visitor);
                        });
                    }
                }
                _ => {}
            }

            return;
        };

        for (typevar, ty) in iter::zip(
            specialization.generic_context(db).variables(db),
            specialization.types(db),
        ) {
            let variance = typevar.variance_with_polarity(db, polarity);

            f(*ty, variance);

            visitor.visit(*ty, || {
                ty.visit_specialization_impl(db, variance, f, visitor);
            });
        }
    }

    /// Return true if there is just a single inhabitant for this type.
    ///
    /// Note: This function aims to have no false positives, but might return `false`
    /// for more complicated types that are actually singletons.
    pub(crate) fn is_singleton(self, db: &'db dyn Db) -> bool {
        match self {
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => false,

            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Int(..)
                | LiteralValueTypeKind::String(..)
                | LiteralValueTypeKind::Bytes(..)
                | LiteralValueTypeKind::LiteralString => {
                    // Note: The literal types included in this pattern are not true singletons.
                    // There can be multiple Python objects (at different memory locations) that
                    // are both of type Literal[345], for example.
                    false
                }

                LiteralValueTypeKind::Bool(_) | LiteralValueTypeKind::Enum(_) => true,
            },

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
            Type::FunctionLiteral(..)
            | Type::WrapperDescriptor(..)
            | Type::ClassLiteral(..)
            | Type::GenericAlias(..)
            | Type::ModuleLiteral(..) => true,
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
            Type::KnownBoundMethod(_) => {
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
            Type::TypeGuard(type_guard) => type_guard.is_bound(db),
            Type::TypedDict(_) => false,
            Type::TypeAlias(alias) => alias.value_type(db).is_singleton(db),
            Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db).is_singleton(db),
        }
    }

    /// Return true if this type is non-empty and all inhabitants of this type compare equal.
    pub(crate) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        match self {
            // Each `partial()` call creates a distinct object at runtime.
            Type::KnownInstance(KnownInstanceType::FunctoolsPartial(_)) => false,

            Type::FunctionLiteral(..)
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(_)
            | Type::ModuleLiteral(..)
            | Type::ClassLiteral(..)
            | Type::GenericAlias(..)
            | Type::SpecialForm(..)
            | Type::KnownInstance(..) => true,

            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Enum(..) => !self.overrides_equality(db),

                LiteralValueTypeKind::Int(..)
                | LiteralValueTypeKind::String(..)
                | LiteralValueTypeKind::Bytes(..)
                | LiteralValueTypeKind::Bool(_) => true,

                LiteralValueTypeKind::LiteralString => false,
            },

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
            Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db).is_single_valued(db),

            Type::BoundSuper(_) => {
                // At runtime two super instances never compare equal, even if their arguments are identical.
                false
            }

            Type::BoundMethod(_) => {
                // Binding the same method to different instances yields different objects: `[].sort != [].sort`
                false
            }

            Type::TypeIs(type_is) => type_is.is_bound(db),
            Type::TypeGuard(type_guard) => type_guard.is_bound(db),

            Type::TypeAlias(alias) => alias.value_type(db).is_single_valued(db),

            Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::Never
            | Type::Union(..)
            | Type::Intersection(..)
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
        if let Some(fallback) = (*self).materialized_divergent_fallback() {
            return fallback.find_name_in_mro_with_policy(db, name, policy);
        }

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

            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => Some(Place::bound(self).into()),

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
                        Some(Place::Undefined.into())
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
                    (Some(KnownClass::Property), "__delete__") => Some(
                        Place::bound(Type::WrapperDescriptor(
                            WrapperDescriptorKind::PropertyDunderDelete,
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
            Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Type) => {
                if policy.mro_no_object_fallback() {
                    Some(Place::Undefined.into())
                } else {
                    KnownClass::Object
                        .to_class_literal(db)
                        .find_name_in_mro_with_policy(db, name, policy)
                }
            }

            Type::TypeAlias(alias) => alias
                .value_type(db)
                .find_name_in_mro_with_policy(db, name, policy),

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
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_) => None,
        }
    }

    fn lookup_dunder_new(self, db: &'db dyn Db) -> Option<PlaceAndQualifiers<'db>> {
        #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
        fn lookup_dunder_new_inner<'db>(
            db: &'db dyn Db,
            ty: Type<'db>,
            _: (),
        ) -> Option<PlaceAndQualifiers<'db>> {
            let mut flags = MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK;
            if !ty.is_subtype_of(db, KnownClass::Type.to_instance(db)) {
                flags |= MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK;
            }
            ty.find_name_in_mro_with_policy(db, "__new__", flags)
        }

        lookup_dunder_new_inner(db, self, ())
    }

    /// Look up an attribute in the MRO of the meta-type of `self`. This returns class-level attributes
    /// when called on an instance-like type, and metaclass attributes when called on a class-like type.
    ///
    /// Basically corresponds to `self.to_meta_type().find_name_in_mro(name)`, except for the handling
    /// of union and intersection types.
    fn class_member(self, db: &'db dyn Db, name: Name) -> PlaceAndQualifiers<'db> {
        self.class_member_with_policy(db, name, MemberLookupPolicy::default())
    }

    #[salsa::tracked(
        cycle_initial=|_, id, _, _, _| Place::bound(Type::divergent(id)).into(),
        cycle_fn=|db, cycle, previous: &PlaceAndQualifiers<'db>, member: PlaceAndQualifiers<'db>, _, _, _| {
            member.cycle_normalized(db, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
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

            // `type[Any]` (or `type[Unknown]`, etc.) has an unknown metaclass, but all
            // metaclasses inherit from `type`. Check `type`'s class-level attributes
            // first so that data descriptors like `__mro__` and `__bases__` resolve to
            // their correct types instead of collapsing to `Any`/`Unknown`.
            Type::SubclassOf(subclass_of) if subclass_of.is_dynamic() => {
                let type_result = KnownClass::Type
                    .to_class_literal(db)
                    .find_name_in_mro_with_policy(db, name.as_str(), policy)
                    .expect("`find_name_in_mro` should return `Some` for a class literal");
                if !type_result.place.is_undefined() {
                    type_result
                } else {
                    self.to_meta_type(db)
                        .find_name_in_mro_with_policy(db, name.as_str(), policy)
                        .expect(
                            "`Type::find_name_in_mro()` should return `Some()` when called on a meta-type",
                        )
                }
            }

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

            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => Place::bound(self).into(),

            Type::NominalInstance(instance) => instance.class(db).instance_member(db, name),
            Type::NewTypeInstance(newtype) => {
                newtype.concrete_base_type(db).instance_member(db, name)
            }

            Type::ProtocolInstance(protocol) => protocol.instance_member(db, name),

            Type::FunctionLiteral(_) => KnownClass::FunctionType
                .to_instance(db)
                .instance_member(db, name),

            Type::BoundMethod(_) => KnownClass::MethodType
                .to_instance(db)
                .instance_member(db, name),
            Type::KnownBoundMethod(method) => {
                method.class().to_instance(db).instance_member(db, name)
            }
            Type::WrapperDescriptor(_) => KnownClass::WrapperDescriptorType
                .to_instance(db)
                .instance_member(db, name),
            Type::DataclassDecorator(_) => KnownClass::FunctionType
                .to_instance(db)
                .instance_member(db, name),
            Type::Callable(_) | Type::DataclassTransformer(_) => {
                Type::object().instance_member(db, name)
            }

            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => Type::object().instance_member(db, name),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.instance_member(db, name)
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .map_with_boundness_and_qualifiers(db, |constraint| {
                            constraint.instance_member(db, name)
                        }),
                }
            }

            Type::TypeIs(_) | Type::TypeGuard(_) => {
                KnownClass::Bool.to_instance(db).instance_member(db, name)
            }

            Type::LiteralValue(literal) => literal.fallback_instance(db).instance_member(db, name),

            Type::AlwaysTruthy | Type::AlwaysFalsy => Type::object().instance_member(db, name),
            Type::ModuleLiteral(_) => KnownClass::ModuleType
                .to_instance(db)
                .instance_member(db, name),

            Type::SpecialForm(_) | Type::KnownInstance(_) => Place::Undefined.into(),

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
                Place::Undefined.into()
            }

            Type::TypedDict(_) => Place::Undefined.into(),

            Type::TypeAlias(alias) => alias.value_type(db).instance_member(db, name),
        }
    }

    /// Access an attribute of this type without invoking the descriptor protocol. This
    /// method corresponds to `inspect.getattr_static(<object of type 'self'>, name)`.
    ///
    /// See also: [`Type::member`]
    fn static_member(&self, db: &'db dyn Db, name: &str) -> Place<'db> {
        if let Type::ModuleLiteral(module) = self {
            module.static_member(db, name).place
        } else if let place @ Place::Defined(_) = self.class_member(db, name.into()).place {
            place
        } else if let Some(place @ Place::Defined(_)) =
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
    #[salsa::tracked(cycle_initial=|_, _, _, _, _| None, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn try_call_dunder_get(
        self,
        db: &'db dyn Db,
        instance: Option<Type<'db>>,
        owner: Type<'db>,
    ) -> Option<(Type<'db>, AttributeKind)> {
        tracing::trace!(
            "try_call_dunder_get: {}, {}, {}",
            self.display(db),
            instance.unwrap_or_else(|| Type::none(db)).display(db),
            owner.display(db)
        );
        if let Some(fallback) = self.materialized_divergent_fallback() {
            return fallback.try_call_dunder_get(db, instance, owner);
        }

        match self {
            Type::Callable(callable) if callable.is_staticmethod_like(db) => {
                // For "staticmethod-like" callables, model the behavior of `staticmethod.__get__`.
                // The underlying function is returned as-is, without binding self.
                return Some((self, AttributeKind::NormalOrNonDataDescriptor));
            }
            Type::Callable(callable)
                if callable.is_function_like(db) || callable.is_classmethod_like(db) =>
            {
                // For "function-like" or "classmethod-like" callables, model the behavior of
                // `FunctionType.__get__` or `classmethod.__get__`.
                //
                // It is a shortcut to model this in `try_call_dunder_get`. If we want to be really precise,
                // we should instead return a new method-wrapper type variant for the synthesized `__get__`
                // method of these synthesized functions. The method-wrapper would then be returned from
                // `find_name_in_mro` when called on function-like `Callable`s. This would allow us to
                // correctly model the behavior of *explicit* `SomeDataclass.__init__.__get__` calls.
                return if instance.is_none() && callable.is_function_like(db) {
                    Some((self, AttributeKind::NormalOrNonDataDescriptor))
                } else {
                    let self_type = instance.unwrap_or_else(|| {
                        // For classmethod-like callables, bind to the owner class.
                        owner.to_instance(db).unwrap_or(owner)
                    });

                    Some((
                        Type::Callable(callable.bind_self(db, Some(self_type))),
                        AttributeKind::NormalOrNonDataDescriptor,
                    ))
                };
            }
            Type::FunctionLiteral(function)
                if instance.is_some_and(|ty| ty.is_none(db))
                    && !function.is_staticmethod(db)
                    && !function.is_classmethod(db) =>
            {
                // When the instance is of type `None` (`NoneType`), we must handle
                // `FunctionType.__get__` here rather than falling through to the generic
                // `__get__` path. The stubs for `FunctionType.__get__` use an overload
                // with `instance: None` to indicate class-level access (returning the
                // unbound function). This incorrectly matches when the instance is actually
                // an instance of `None`
                return Some((
                    Type::BoundMethod(BoundMethodType::new(db, function, instance.unwrap())),
                    AttributeKind::NormalOrNonDataDescriptor,
                ));
            }
            _ => {}
        }

        let descr_get = self.class_member(db, "__get__".into()).place;

        if let Place::Defined(DefinedPlace {
            ty: descr_get,
            definedness: descr_get_boundness,
            ..
        }) = descr_get
        {
            let instance_ty = instance.unwrap_or_else(|| Type::none(db));
            let return_ty = descr_get
                .try_call(db, &CallArguments::positional([self, instance_ty, owner]))
                .map(|bindings| {
                    if descr_get_boundness == Definedness::AlwaysDefined {
                        bindings.return_type(db)
                    } else {
                        UnionType::from_two_elements(db, bindings.return_type(db), self)
                    }
                })
                // TODO: an error when calling `__get__` will lead to a `TypeError` or similar at runtime;
                // we should emit a diagnostic here instead of silently ignoring the error.
                .unwrap_or_else(|CallError(_, bindings)| bindings.return_type(db));

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
        instance: Option<Type<'db>>,
        owner: Type<'db>,
    ) -> (PlaceAndQualifiers<'db>, AttributeKind) {
        if let PlaceAndQualifiers {
            place:
                Place::Defined(DefinedPlace {
                    ty,
                    origin,
                    definedness,
                    public_type_policy,
                }),
            qualifiers,
        } = attribute
            && let Some(fallback) = ty.materialized_divergent_fallback()
        {
            return Self::try_call_dunder_get_on_attribute(
                db,
                Place::Defined(DefinedPlace {
                    ty: fallback,
                    origin,
                    definedness,
                    public_type_policy,
                })
                .with_qualifiers(qualifiers),
                instance,
                owner,
            );
        }

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
                place:
                    Place::Defined(DefinedPlace {
                        ty: Type::Dynamic(_) | Type::Divergent(_) | Type::Never,
                        ..
                    }),
                qualifiers: _,
            } => (attribute, AttributeKind::DataDescriptor),

            PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        ty: Type::Union(union),
                        origin,
                        definedness: boundness,
                        public_type_policy,
                    }),
                qualifiers,
            } => (
                union
                    .map_with_boundness(db, |elem| {
                        Place::Defined(DefinedPlace {
                            ty: elem
                                .try_call_dunder_get(db, instance, owner)
                                .map_or(*elem, |(ty, _)| ty),
                            origin,
                            definedness: boundness,
                            public_type_policy,
                        })
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

            attribute @ PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        ty: Type::Intersection(intersection),
                        origin,
                        definedness,
                        public_type_policy,
                    }),
                qualifiers,
            } => (
                if intersection.positive(db).is_empty() {
                    attribute
                } else {
                    intersection
                        .map_with_boundness(db, |elem| {
                            Place::Defined(DefinedPlace {
                                ty: elem
                                    .try_call_dunder_get(db, instance, owner)
                                    .map_or(*elem, |(ty, _)| ty),
                                origin,
                                definedness,
                                public_type_policy,
                            })
                        })
                        .with_qualifiers(qualifiers)
                },
                // TODO: Discover data descriptors in intersections.
                AttributeKind::NormalOrNonDataDescriptor,
            ),

            PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        ty: attribute_ty,
                        origin,
                        definedness: boundness,
                        public_type_policy,
                    }),
                qualifiers: _,
            } => {
                if let Some((return_ty, attribute_kind)) =
                    attribute_ty.try_call_dunder_get(db, instance, owner)
                {
                    (
                        Place::Defined(DefinedPlace {
                            ty: return_ty,
                            origin,
                            definedness: boundness,
                            public_type_policy,
                        })
                        .into(),
                        attribute_kind,
                    )
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

    /// Returns whether this type should be considered a possible data descriptor.
    /// If this type is a union, returns true if _any_ element is a data descriptor.
    /// This is used to determine whether an attribute assignment is valid for narrowing.
    /// In theory, dynamic types might be data descriptor types, so it is unsafe to use
    /// attribute assignment for narrowing if the inferred type of an attribute contains a dynamic type.
    /// However, strictly applying this rule would disable narrowing too frequently.
    /// Therefore, for practical convenience, we don't consider dynamic types as data descriptors.
    pub(crate) fn may_be_data_descriptor(self, d: &'db dyn Db) -> bool {
        self.is_data_descriptor_impl(d, true)
    }

    fn is_data_descriptor_impl(self, db: &'db dyn Db, any_of_union: bool) -> bool {
        match self {
            Type::Dynamic(_) => !any_of_union,
            Type::Never | Type::PropertyInstance(_) => true,
            Type::Union(union) if any_of_union => union
                .elements(db)
                .iter()
                .any(|ty| ty.is_data_descriptor_impl(db, any_of_union)),
            Type::Union(union) => union
                .elements(db)
                .iter()
                .all(|ty| ty.is_data_descriptor_impl(db, any_of_union)),
            Type::Intersection(intersection) => intersection
                .iter_positive(db)
                .any(|ty| ty.is_data_descriptor_impl(db, any_of_union)),
            _ => {
                !self.class_member(db, "__set__".into()).place.is_undefined()
                    || !self
                        .class_member(db, "__delete__".into())
                        .place
                        .is_undefined()
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
            Some(self),
            self.to_meta_type(db),
        );

        let PlaceAndQualifiers {
            place: fallback,
            qualifiers: fallback_qualifiers,
        } = fallback;

        match (meta_attr, meta_attr_kind, fallback) {
            // The fallback type is unbound, so we can just return `meta_attr` unconditionally,
            // no matter if it's data descriptor, a non-data descriptor, or a normal attribute.
            (meta_attr @ Place::Defined(_), _, Place::Undefined) => {
                meta_attr.with_qualifiers(meta_attr_qualifiers)
            }

            // `meta_attr` is the return type of a data descriptor and definitely bound, so we
            // return it.
            (
                meta_attr @ Place::Defined(DefinedPlace {
                    definedness: Definedness::AlwaysDefined,
                    ..
                }),
                AttributeKind::DataDescriptor,
                _,
            ) => meta_attr.with_qualifiers(meta_attr_qualifiers),

            // `meta_attr` is the return type of a data descriptor, but the attribute on the
            // meta-type is possibly-unbound. This means that we "fall through" to the next
            // stage of the descriptor protocol and union with the fallback type.
            (
                Place::Defined(DefinedPlace {
                    ty: meta_attr_ty,
                    origin: meta_origin,
                    definedness: Definedness::PossiblyUndefined,
                    ..
                }),
                AttributeKind::DataDescriptor,
                Place::Defined(DefinedPlace {
                    ty: fallback_ty,
                    origin: fallback_origin,
                    definedness: fallback_boundness,
                    public_type_policy: fallback_public_type_policy,
                }),
            ) => Place::Defined(DefinedPlace {
                ty: UnionType::from_two_elements(db, meta_attr_ty, fallback_ty),
                origin: meta_origin.merge(fallback_origin),
                definedness: fallback_boundness,
                public_type_policy: fallback_public_type_policy,
            })
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
                Place::Defined(_),
                AttributeKind::NormalOrNonDataDescriptor,
                fallback @ Place::Defined(DefinedPlace {
                    definedness: Definedness::AlwaysDefined,
                    ..
                }),
            ) if policy == InstanceFallbackShadowsNonDataDescriptor::Yes => {
                fallback.with_qualifiers(fallback_qualifiers)
            }

            // `meta_attr` is *not* a data descriptor. The `fallback` symbol is either possibly
            // unbound or the policy argument is `No`. In both cases, the `fallback` type does
            // not completely shadow the non-data descriptor, so we build a union of the two.
            (
                Place::Defined(DefinedPlace {
                    ty: meta_attr_ty,
                    origin: meta_origin,
                    definedness: meta_attr_boundness,
                    ..
                }),
                AttributeKind::NormalOrNonDataDescriptor,
                Place::Defined(DefinedPlace {
                    ty: fallback_ty,
                    origin: fallback_origin,
                    definedness: fallback_boundness,
                    public_type_policy: fallback_public_type_policy,
                }),
            ) => Place::Defined(DefinedPlace {
                ty: UnionType::from_two_elements(db, meta_attr_ty, fallback_ty),
                origin: meta_origin.merge(fallback_origin),
                definedness: meta_attr_boundness.max(fallback_boundness),
                public_type_policy: fallback_public_type_policy,
            })
            .with_qualifiers(meta_attr_qualifiers.union(fallback_qualifiers)),

            // If the attribute is not found on the meta-type, we simply return the fallback.
            (Place::Undefined, _, fallback) => fallback.with_qualifiers(fallback_qualifiers),
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
    #[salsa::tracked(
        cycle_initial=|_, id, _, _, _| Place::bound(Type::divergent(id)).into(),
        cycle_fn=|db, cycle, previous: &PlaceAndQualifiers<'db>, member: PlaceAndQualifiers<'db>, _, _, _| {
            member.cycle_normalized(db, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(crate) fn member_lookup_with_policy(
        self,
        db: &'db dyn Db,
        name: Name,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        tracing::trace!("member_lookup_with_policy: {}.{}", self.display(db), name);
        if let Some(fallback) = self.materialized_divergent_fallback() {
            return fallback.member_lookup_with_policy(db, name, policy);
        }

        if name == "__class__" {
            return Place::bound(self.dunder_class(db)).into();
        }

        let name_str = name.as_str();

        match self {
            Type::Union(union) => union.map_with_boundness_and_qualifiers(db, |elem| {
                elem.member_lookup_with_policy(db, name_str.into(), policy)
            }),

            Type::Intersection(intersection) => intersection
                .map_with_boundness_and_qualifiers(db, |elem| {
                    elem.member_lookup_with_policy(db, name_str.into(), policy)
                }),

            Type::Dynamic(..) | Type::Divergent(_) | Type::Never => Place::bound(self).into(),

            Type::FunctionLiteral(function) if name == "__get__" => Place::bound(
                Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(function)),
            )
            .into(),
            Type::FunctionLiteral(function) if name == "__call__" => Place::bound(
                Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderCall(function)),
            )
            .into(),
            Type::PropertyInstance(property) if name == "__get__" => Place::bound(
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(property)),
            )
            .into(),
            Type::PropertyInstance(property) if name == "__set__" => Place::bound(
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderSet(property)),
            )
            .into(),
            Type::PropertyInstance(property) if name == "__delete__" => Place::bound(
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderDelete(property)),
            )
            .into(),

            Type::LiteralValue(literal) if literal.is_string() && name == "startswith" => {
                let string_literal = literal.as_string().unwrap();
                Place::bound(Type::KnownBoundMethod(KnownBoundMethodType::StrStartswith(
                    string_literal,
                )))
                .into()
            }

            Type::ClassLiteral(class)
                if name == "range" && class.is_known(db, KnownClass::ConstraintSet) =>
            {
                Place::bound(Type::KnownBoundMethod(
                    KnownBoundMethodType::ConstraintSetRange,
                ))
                .into()
            }
            Type::ClassLiteral(class)
                if name == "always" && class.is_known(db, KnownClass::ConstraintSet) =>
            {
                Place::bound(Type::KnownBoundMethod(
                    KnownBoundMethodType::ConstraintSetAlways,
                ))
                .into()
            }
            Type::ClassLiteral(class)
                if name == "never" && class.is_known(db, KnownClass::ConstraintSet) =>
            {
                Place::bound(Type::KnownBoundMethod(
                    KnownBoundMethodType::ConstraintSetNever,
                ))
                .into()
            }
            Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked))
                if name == "implies_subtype_of" =>
            {
                Place::bound(Type::KnownBoundMethod(
                    KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(tracked),
                ))
                .into()
            }
            Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked))
                if name == "satisfies" =>
            {
                Place::bound(Type::KnownBoundMethod(
                    KnownBoundMethodType::ConstraintSetSatisfies(tracked),
                ))
                .into()
            }
            Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked))
                if name == "satisfied_by_all_typevars" =>
            {
                Place::bound(Type::KnownBoundMethod(
                    KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(tracked),
                ))
                .into()
            }

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
            Type::ClassLiteral(class)
                if name == "__delete__" && class.is_known(db, KnownClass::Property) =>
            {
                Place::bound(Type::WrapperDescriptor(
                    WrapperDescriptorKind::PropertyDunderDelete,
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
            Type::KnownBoundMethod(method) => method
                .class()
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
                Type::object().member_lookup_with_policy(db, name, policy)
            }

            Type::NominalInstance(instance)
                if matches!(name.as_str(), "major" | "minor")
                    && instance.has_known_class(db, KnownClass::VersionInfo) =>
            {
                let python_version = Program::get(db).python_version(db);
                let segment = if name == "major" {
                    python_version.major
                } else {
                    python_version.minor
                };
                Place::bound(Type::int_literal(segment.into())).into()
            }

            Type::PropertyInstance(property) if name == "fget" => {
                Place::bound(property.getter(db).unwrap_or(Type::none(db))).into()
            }
            Type::PropertyInstance(property) if name == "fset" => {
                Place::bound(property.setter(db).unwrap_or(Type::none(db))).into()
            }
            Type::PropertyInstance(property) if name == "fdel" => {
                Place::bound(property.deleter(db).unwrap_or(Type::none(db))).into()
            }

            Type::LiteralValue(literal)
                if literal.is_int() && matches!(name_str, "real" | "numerator") =>
            {
                Place::bound(self).into()
            }

            Type::LiteralValue(literal)
                if literal.is_bool() && matches!(name_str, "real" | "numerator") =>
            {
                let bool_value = literal.as_bool().unwrap();
                Place::bound(Type::int_literal(i64::from(bool_value))).into()
            }

            Type::ModuleLiteral(module) => module.static_member(db, name_str),

            // If a protocol does not include a member and the policy disables falling back to
            // `object`, we return `Place::Undefined` here. This short-circuits attribute lookup
            // before we find the "fallback to attribute access on `object`" logic later on
            // (otherwise we would infer that all synthesized protocols have `__getattribute__`
            // methods, and therefore that all synthesized protocols have all possible attributes.)
            //
            // Note that we could do this for *all* protocols, but it's only *necessary* for synthesized
            // ones, and the standard logic is *probably* more performant for class-based protocols?
            Type::ProtocolInstance(ProtocolInstanceType {
                inner: Protocol::Synthesized(protocol),
                ..
            }) if policy.mro_no_object_fallback()
                && !protocol.interface().includes_member(db, name_str) =>
            {
                Place::Undefined.into()
            }

            // This case needs to come before the `no_instance_fallback` catch-all, so that we
            // treat `NewType`s of `float` and `complex` as their special-case union base types.
            // Otherwise we'll look up e.g. `__add__` with a `self` type bound to the `NewType`,
            // which will fail to match e.g. `float.__add__` (because its `self` parameter is just
            // `float` and not `int | float`). However, all other `NewType` cases need to fall
            // through, because we generally do want e.g. methods that return `Self` to return the
            // `NewType`.
            Type::NewTypeInstance(new_type_instance) if self.as_union_like(db).is_some() => {
                new_type_instance
                    .concrete_base_type(db)
                    .member_lookup_with_policy(db, name, policy)
            }

            Type::TypeAlias(alias) => alias
                .value_type(db)
                .member_lookup_with_policy(db, name, policy),

            _ if policy.no_instance_fallback() => self.invoke_descriptor_protocol(
                db,
                name_str,
                Place::Undefined.into(),
                InstanceFallbackShadowsNonDataDescriptor::No,
                policy,
            ),

            Type::LiteralValue(literal)
                if literal.as_enum().is_some()
                    && matches!(name_str, "name" | "_name_" | "value" | "_value_") =>
            {
                let enum_literal = literal.as_enum().unwrap();
                let enum_class = enum_literal.enum_class(db);
                let is_enum_subclass = Type::ClassLiteral(enum_class)
                    .is_subtype_of(db, KnownClass::Enum.to_subclass_of(db));

                enum_metadata(db, enum_class)
                    .and_then(|metadata| match name_str {
                        "name" if is_enum_subclass => metadata.name_type(db, enum_literal.name(db)),
                        "_name_" => metadata.name_type(db, enum_literal.name(db)),
                        "value" if is_enum_subclass => metadata.value_type(enum_literal.name(db)),
                        "_value_" => metadata.value_type(enum_literal.name(db)),
                        _ => None,
                    })
                    .map_or_else(|| Place::Undefined, Place::bound)
                    .into()
            }

            Type::TypeVar(typevar) if name_str == "args" && typevar.is_paramspec(db) => {
                Place::declared(Type::TypeVar(
                    typevar.with_paramspec_attr(db, ParamSpecAttrKind::Args),
                ))
                .into()
            }
            Type::TypeVar(typevar) if name_str == "kwargs" && typevar.is_paramspec(db) => {
                Place::declared(Type::TypeVar(
                    typevar.with_paramspec_attr(db, ParamSpecAttrKind::Kwargs),
                ))
                .into()
            }

            Type::NominalInstance(instance)
                if matches!(name_str, "name" | "_name_" | "value" | "_value_")
                    && enum_metadata(db, instance.class_literal(db)).is_some() =>
            {
                let class_literal = instance.class_literal(db);
                let is_enum_subclass = Type::ClassLiteral(class_literal)
                    .is_subtype_of(db, KnownClass::Enum.to_subclass_of(db));

                enum_metadata(db, class_literal)
                    .and_then(|metadata| match name_str {
                        "name" if is_enum_subclass => metadata.instance_name_type(db),
                        "_name_" => metadata.instance_name_type(db),
                        "value" if is_enum_subclass => metadata.instance_value_type(db),
                        "_value_" => metadata.instance_value_type(db),
                        _ => None,
                    })
                    .map_or_else(Place::default, Place::bound)
                    .into()
            }

            Type::KnownInstance(KnownInstanceType::FunctoolsPartial(partial))
                if name_str == "__call__" =>
            {
                Place::bound(Type::Callable(partial.partial(db))).into()
            }

            Type::KnownInstance(KnownInstanceType::FunctoolsPartial(partial)) => {
                let wrapped = partial.wrapped(db).inner(db);
                let nominal_lookup = partial
                    .partial(db)
                    .into_functools_partial_instance(db)
                    .member_lookup_with_policy(db, name.clone(), policy);
                if name_str == "func" {
                    match nominal_lookup.place {
                        Place::Defined(DefinedPlace {
                            origin,
                            definedness,
                            public_type_policy,
                            ..
                        }) => Place::Defined(DefinedPlace {
                            ty: wrapped,
                            origin,
                            definedness,
                            public_type_policy,
                        })
                        .into(),
                        Place::Undefined => Place::bound(wrapped).into(),
                    }
                } else {
                    nominal_lookup
                }
            }

            Type::NominalInstance(..)
            | Type::ProtocolInstance(..)
            | Type::NewTypeInstance(..)
            | Type::LiteralValue(..)
            | Type::TypeVar(..)
            | Type::SpecialForm(..)
            | Type::KnownInstance(..)
            | Type::PropertyInstance(..)
            | Type::FunctionLiteral(..)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(..)
            | Type::TypeGuard(..)
            | Type::TypedDict(_) => {
                // Enum members can be accessed through enum instances and other enum members,
                // e.g. `answer.YES` or `Answer.YES.NO`.
                let enum_class = match self {
                    Type::LiteralValue(literal) => literal
                        .as_enum()
                        .map(|enum_literal| enum_literal.enum_class(db)),
                    Type::NominalInstance(instance) => Some(instance.class_literal(db)),
                    _ => None,
                };

                if let Some(enum_class) = enum_class
                    && let Some(metadata) = enum_metadata(db, enum_class)
                    && let Some(resolved_name) = metadata.resolve_member(&name)
                {
                    return Place::bound(Type::enum_literal(EnumLiteralType::new(
                        db,
                        enum_class,
                        resolved_name.clone(),
                    )))
                    .into();
                }

                let fallback = self.instance_member(db, name_str);

                let result = self.invoke_descriptor_protocol(
                    db,
                    name_str,
                    fallback,
                    InstanceFallbackShadowsNonDataDescriptor::No,
                    policy,
                );

                if result.is_class_var() && self.is_typed_dict() {
                    // `ClassVar`s on `TypedDictFallback` cannot be accessed on inhabitants of `SomeTypedDict`.
                    // They can only be accessed on `SomeTypedDict` directly.
                    return Place::Undefined.into();
                }

                let result = self.fallback_to_getattr(db, &name, result, policy);

                result.map_type(|ty| ty.bind_self_typevars(db, self))
            }

            Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..) => {
                let enum_class = match self {
                    Type::ClassLiteral(literal) => Some(literal),
                    Type::SubclassOf(subclass_of) => subclass_of
                        .subclass_of()
                        .into_class(db)
                        .map(|class| class.class_literal(db)),
                    _ => None,
                };
                if let Some(enum_class) = enum_class
                    && let Some(metadata) = enum_metadata(db, enum_class)
                    && let Some(resolved_name) = metadata.resolve_member(&name)
                {
                    return Place::bound(Type::enum_literal(EnumLiteralType::new(
                        db,
                        enum_class,
                        resolved_name,
                    )))
                    .into();
                }

                let class_attr_plain = self.find_name_in_mro_with_policy(db, name_str, policy).expect(
                    "Calling `find_name_in_mro` on class literals and subclass-of types should always return `Some`",
                );

                let self_instance = self
                    .to_instance(db)
                    .expect("`to_instance` always returns `Some` for `ClassLiteral`, `GenericAlias`, and `SubclassOf`");
                let class_attr_plain =
                    class_attr_plain.map_type(|ty| ty.bind_self_typevars(db, self_instance));

                let class_attr_fallback =
                    Self::try_call_dunder_get_on_attribute(db, class_attr_plain, None, self).0;

                let result = self.invoke_descriptor_protocol(
                    db,
                    name_str,
                    class_attr_fallback,
                    InstanceFallbackShadowsNonDataDescriptor::Yes,
                    policy,
                );

                // A class is an instance of its metaclass. If attribute lookup on the class
                // fails, Python falls back to `type(cls).__getattr__` and
                // `type(cls).__getattribute__` on the metaclass, analogous to how instance
                // attribute access falls back to `__getattr__`/`__getattribute__` on the
                // class. `try_call_dunder` adds `NO_INSTANCE_FALLBACK`, which causes the
                // lookup to hit the catch-all that only checks the meta-type (the metaclass).
                let result = self.fallback_to_getattr(db, &name, result, policy);

                // `type[Any]`/`type[Unknown]` are gradual forms with an unknown metaclass
                // (which is at least `type`). Attributes resolved via `type`'s descriptors
                // are intersected with the dynamic type to reflect uncertainty about
                // whether the unknown metaclass overrides them.
                if let Type::SubclassOf(subclass_of) = self
                    && let SubclassOfInner::Dynamic(dynamic) = subclass_of.subclass_of()
                {
                    result.map_type(|ty| {
                        if ty.is_dynamic() {
                            ty
                        } else {
                            IntersectionType::from_two_elements(db, ty, Type::Dynamic(dynamic))
                        }
                    })
                } else {
                    result
                }
            }

            // Unlike other objects, `super` has a unique member lookup behavior.
            // It's simpler than other objects:
            //
            // 1. Search for the attribute in the MRO, starting just after the pivot class.
            // 2. If the attribute is a descriptor, invoke its `__get__` method.
            Type::BoundSuper(bound_super) => {
                let owner_attr = bound_super.find_name_in_mro_after_pivot(db, name_str, policy);

                bound_super
                    .try_call_dunder_get_on_attribute(db, owner_attr)
                    .unwrap_or(owner_attr)
            }
        }
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
                Type::LiteralValue(literal) => match literal.kind() {
                    LiteralValueTypeKind::Int(value) => (value.as_i64() >= 0).then_some(ty),
                    LiteralValueTypeKind::Bool(value) => Some(Type::int_literal(i64::from(value))),
                    _ => None,
                },
                Type::Union(union) => {
                    union.try_map(db, |element| non_negative_int_literal(db, *element))
                }
                _ => None,
            }
        }

        let usize_len = match self.as_literal_value_kind() {
            Some(LiteralValueTypeKind::Bytes(bytes)) => Some(bytes.python_len(db)),
            Some(LiteralValueTypeKind::String(string)) => Some(string.python_len(db)),
            _ => None,
        };

        if let Some(usize_len) = usize_len {
            return usize_len.try_into().ok().map(Type::int_literal);
        }

        let return_ty = match self.try_call_dunder(
            db,
            "__len__",
            CallArguments::none(),
            TypeContext::default(),
        ) {
            Ok(bindings) => bindings.return_type(db),
            Err(CallDunderError::PossiblyUnbound { bindings, .. }) => bindings.return_type(db),

            // TODO: emit a diagnostic
            Err(CallDunderError::MethodNotAvailable) => return None,
            Err(CallDunderError::CallError(_, bindings)) => bindings.return_type(db),
        };

        non_negative_int_literal(db, return_ty)
    }

    /// If this type is a `ParamSpec` type variable, returns it. Otherwise, returns `None`.
    fn as_paramspec_typevar(self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Type::TypeVar(tv) if tv.is_paramspec(db) => Some(self),
            _ => None,
        }
    }

    // Returns the value type of a `__getitem__` dunder call on this object.
    //
    // Returns `None` if `__getitem__` is undefined or results in a call error.
    fn getitem_dunder_call(self, db: &'db dyn Db, key: Option<&str>) -> Option<Type<'db>> {
        let key = key
            .map(|key| Type::string_literal(db, key))
            .unwrap_or(Type::unknown());

        match self
            .member_lookup_with_policy(
                db,
                Name::new_static("__getitem__"),
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .place
        {
            Place::Defined(DefinedPlace {
                ty: getitem_method,
                definedness: Definedness::AlwaysDefined,
                ..
            }) => getitem_method
                .try_call(db, &CallArguments::positional([key]))
                .ok()
                .map(|bindings| bindings.return_type(db)),

            _ => None,
        }
    }

    /// Returns the key and value types of this object if it was unpacked using `**`,
    /// or `None` if the object does not support unpacking.
    fn unpack_keys_and_items(self, db: &'db dyn Db) -> Option<(Type<'db>, Type<'db>)> {
        let key_ty = match self
            .member_lookup_with_policy(
                db,
                Name::new_static("keys"),
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .place
        {
            Place::Defined(DefinedPlace {
                ty: keys_method,
                definedness: Definedness::AlwaysDefined,
                ..
            }) => keys_method
                .try_call(db, &CallArguments::none())
                .ok()
                .and_then(|bindings| {
                    Some(
                        bindings
                            .return_type(db)
                            .try_iterate(db)
                            .ok()?
                            .homogeneous_element_type(db),
                    )
                })?,

            _ => return None,
        };

        let value_ty = self
            .getitem_dunder_call(db, None)
            .unwrap_or(Type::unknown());

        Some((key_ty, value_ty))
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
        if let Some(fallback) = self.materialized_divergent_fallback() {
            return fallback.bindings(db);
        }

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

            Type::KnownBoundMethod(method) => {
                CallableBinding::from_overloads(self, method.signatures(db)).into()
            }

            Type::WrapperDescriptor(wrapper_descriptor) => {
                CallableBinding::from_overloads(self, wrapper_descriptor.signatures(db)).into()
            }

            // TODO: We should probably also check the original return type of the function
            // that was decorated with `@dataclass_transform`, to see if it is consistent with
            // with what we configure here.
            Type::DataclassTransformer(_) => Binding::single(
                self,
                Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::positional_only(Some(Name::new_static("func")))
                            .with_annotated_type(Type::object())],
                    ),
                    Type::unknown(),
                ),
            )
            .into(),

            Type::FunctionLiteral(function_type) => match function_type.known(db) {
                Some(
                    KnownFunction::IsEquivalentTo
                    | KnownFunction::IsAssignableTo
                    | KnownFunction::IsSubtypeOf
                    | KnownFunction::IsDisjointFrom,
                ) => Binding::single(
                    self,
                    Signature::new(
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_only(Some(Name::new_static("a")))
                                    .type_form()
                                    .with_annotated_type(Type::any()),
                                Parameter::positional_only(Some(Name::new_static("b")))
                                    .type_form()
                                    .with_annotated_type(Type::any()),
                            ],
                        ),
                        KnownClass::ConstraintSet.to_instance(db),
                    ),
                )
                .into(),

                Some(KnownFunction::IsSingleton | KnownFunction::IsSingleValued) => {
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new(
                                db,
                                [Parameter::positional_only(Some(Name::new_static("a")))
                                    .type_form()
                                    .with_annotated_type(Type::any())],
                            ),
                            KnownClass::Bool.to_instance(db),
                        ),
                    )
                    .into()
                }

                Some(KnownFunction::AssertType) => {
                    let val_ty = BoundTypeVarInstance::synthetic(
                        db,
                        Name::new_static("T"),
                        TypeVarVariance::Invariant,
                    );

                    Binding::single(
                        self,
                        Signature::new_generic(
                            Some(GenericContext::from_typevar_instances(db, [val_ty])),
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("value")))
                                        .with_annotated_type(Type::TypeVar(val_ty)),
                                    Parameter::positional_only(Some(Name::new_static("type")))
                                        .type_form()
                                        .with_annotated_type(Type::any()),
                                ],
                            ),
                            Type::TypeVar(val_ty),
                        ),
                    )
                    .into()
                }

                Some(KnownFunction::AssertNever) => {
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new(
                                db,
                                [Parameter::positional_only(Some(Name::new_static("arg")))
                                    // We need to set the type to `Any` here (instead of `Never`),
                                    // in order for every `assert_never` call to pass the argument
                                    // check. If we set it to `Never`, we'll get invalid-argument-type
                                    // errors instead of `type-assertion-failure` errors.
                                    .with_annotated_type(Type::any())],
                            ),
                            Type::Never,
                        ),
                    )
                    .into()
                }

                Some(KnownFunction::Cast) => Binding::single(
                    self,
                    Signature::new(
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_or_keyword(Name::new_static("typ"))
                                    .type_form()
                                    .with_annotated_type(Type::any()),
                                Parameter::positional_or_keyword(Name::new_static("val"))
                                    .with_annotated_type(Type::any()),
                            ],
                        ),
                        Type::any(),
                    ),
                )
                .into(),

                Some(KnownFunction::Dataclass) => {
                    CallableBinding::from_overloads(
                        self,
                        [
                            // def dataclass(cls: None, /) -> Callable[[type[_T]], type[_T]]: ...
                            Signature::new(
                                Parameters::new(
                                    db,
                                    [Parameter::positional_only(Some(Name::new_static("cls")))
                                        .with_annotated_type(Type::none(db))],
                                ),
                                Type::unknown(),
                            ),
                            // def dataclass(cls: type[_T], /) -> type[_T]: ...
                            Signature::new(
                                Parameters::new(
                                    db,
                                    [Parameter::positional_only(Some(Name::new_static("cls")))
                                        .with_annotated_type(KnownClass::Type.to_instance(db))],
                                ),
                                Type::unknown(),
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
                                Parameters::new(
                                    db,
                                    [
                                        Parameter::keyword_only(Name::new_static("init"))
                                            .with_annotated_type(KnownClass::Bool.to_instance(db))
                                            .with_default_type(Type::bool_literal(true)),
                                        Parameter::keyword_only(Name::new_static("repr"))
                                            .with_annotated_type(KnownClass::Bool.to_instance(db))
                                            .with_default_type(Type::bool_literal(true)),
                                        Parameter::keyword_only(Name::new_static("eq"))
                                            .with_annotated_type(KnownClass::Bool.to_instance(db))
                                            .with_default_type(Type::bool_literal(true)),
                                        Parameter::keyword_only(Name::new_static("order"))
                                            .with_annotated_type(KnownClass::Bool.to_instance(db))
                                            .with_default_type(Type::bool_literal(false)),
                                        Parameter::keyword_only(Name::new_static("unsafe_hash"))
                                            .with_annotated_type(KnownClass::Bool.to_instance(db))
                                            .with_default_type(Type::bool_literal(false)),
                                        Parameter::keyword_only(Name::new_static("frozen"))
                                            .with_annotated_type(KnownClass::Bool.to_instance(db))
                                            .with_default_type(Type::bool_literal(false)),
                                        Parameter::keyword_only(Name::new_static("match_args"))
                                            .with_annotated_type(KnownClass::Bool.to_instance(db))
                                            .with_default_type(Type::bool_literal(true)),
                                        Parameter::keyword_only(Name::new_static("kw_only"))
                                            .with_annotated_type(KnownClass::Bool.to_instance(db))
                                            .with_default_type(Type::bool_literal(false)),
                                        Parameter::keyword_only(Name::new_static("slots"))
                                            .with_annotated_type(KnownClass::Bool.to_instance(db))
                                            .with_default_type(Type::bool_literal(false)),
                                        Parameter::keyword_only(Name::new_static("weakref_slot"))
                                            .with_annotated_type(KnownClass::Bool.to_instance(db))
                                            .with_default_type(Type::bool_literal(false)),
                                    ],
                                ),
                                Type::unknown(),
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

            Type::ClassLiteral(class) => self
                // TODO this should be called from `constructor_bindings` for better consistency
                .known_class_literal_bindings(db, class)
                .unwrap_or_else(|| self.constructor_bindings(db, ClassType::NonGeneric(class))),

            Type::GenericAlias(alias) => self.constructor_bindings(db, ClassType::Generic(alias)),

            Type::SubclassOf(subclass_of_type) => match subclass_of_type.subclass_of() {
                SubclassOfInner::Dynamic(dynamic_type) => {
                    Binding::single(self, Signature::dynamic(Type::Dynamic(dynamic_type))).into()
                }
                SubclassOfInner::Class(class) => self.constructor_bindings(db, class),
                SubclassOfInner::TypeVar(tvar) => {
                    let constructor_instance_type = Type::TypeVar(tvar);
                    let bindings = match tvar.typevar(db).bound_or_constraints(db) {
                        None => KnownClass::Type.to_instance(db).bindings(db),
                        Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                            bound.to_meta_type(db).bindings(db)
                        }
                        Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                            Bindings::from_union(
                                self,
                                constraints
                                    .elements(db)
                                    .iter()
                                    .map(|ty| ty.to_meta_type(db).bindings(db)),
                            )
                        }
                    };
                    // TODO We would ideally be able to just do `into_constructor_bindings` in the
                    // no-bounds/constraints case above (where we get back the bindings for
                    // `Type.__call__`), and just do `with_constructed_instance_type` in the
                    // bound/constrained cases, where we should get back constructor bindings (or
                    // if we don't, we probably shouldn't return `T` from the call?). But currently
                    // we can't because we special-case some built-in types to return regular
                    // (not constructor) bindings from `constructor_bindings()`.
                    bindings
                        // `into_constructor_bindings` is a no-op for already-constructor bindings,
                        // so we are just setting the `MetaclassCall` type for `Type.__call__`, or
                        // the special-cased builtin classes that return regular bindings.
                        .into_constructor_bindings(
                            constructor_instance_type,
                            ConstructorCallableKind::MetaclassCall,
                        )
                        .with_constructed_instance_type(db, constructor_instance_type)
                }
            },

            Type::SpecialForm(SpecialFormType::TypeQualifier(TypeQualifier::InitVar)) => {
                let parameter = Parameter::positional_or_keyword(Name::new_static("type"))
                    .with_annotated_type(Type::any());
                let signature = Signature::new(Parameters::new(db, [parameter]), Type::any());
                Binding::single(self, signature).into()
            }

            Type::NominalInstance(_) | Type::ProtocolInstance(_) | Type::NewTypeInstance(_) => {
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
                    Place::Defined(DefinedPlace {
                        ty: dunder_callable,
                        definedness: boundness,
                        ..
                    }) => {
                        let mut bindings = dunder_callable.bindings(db);
                        bindings.replace_callable_type(dunder_callable, self);
                        if boundness == Definedness::PossiblyUndefined {
                            bindings.set_dunder_call_is_possibly_unbound();
                        }
                        bindings
                    }
                    Place::Undefined => CallableBinding::not_callable(self).into(),
                }
            }

            // Dynamic types are callable, and the return type is the same dynamic type. Similarly,
            // `Never` is always callable and returns `Never`.
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => {
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

            Type::Intersection(intersection) => Bindings::from_intersection(
                self,
                intersection
                    .positive_elements_or_object(db)
                    .map(|element| element.bindings(db)),
            ),

            Type::DataclassDecorator(_) => {
                let typevar = BoundTypeVarInstance::synthetic(
                    db,
                    Name::new_static("T"),
                    TypeVarVariance::Invariant,
                );
                let typevar_meta = SubclassOfType::from(db, typevar);
                let context = GenericContext::from_typevar_instances(db, [typevar]);
                let parameters = [Parameter::positional_only(Some(Name::new_static("cls")))
                    .with_annotated_type(typevar_meta)];
                // Intersect with `Any` for the return type to reflect the fact that the `dataclass()`
                // decorator adds methods to the class
                let returns = IntersectionType::from_two_elements(db, typevar_meta, Type::any());
                let signature =
                    Signature::new_generic(Some(context), Parameters::new(db, parameters), returns);
                Binding::single(self, signature).into()
            }

            // TODO: some `SpecialForm`s are callable (e.g. TypedDicts)
            Type::SpecialForm(_) => CallableBinding::not_callable(self).into(),

            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Enum(enum_literal) => {
                    enum_literal.enum_class_instance(db).bindings(db)
                }
                _ => CallableBinding::not_callable(self).into(),
            },

            Type::KnownInstance(KnownInstanceType::NewType(newtype)) => Binding::single(
                self,
                Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::positional_only(None)
                            .with_annotated_type(newtype.base(db).instance_type(db))],
                    ),
                    Type::NewTypeInstance(newtype),
                ),
            )
            .into(),

            Type::KnownInstance(KnownInstanceType::FunctoolsPartial(partial)) => {
                Type::Callable(partial.partial(db)).bindings(db)
            }

            Type::KnownInstance(known_instance) => {
                known_instance.instance_fallback(db).bindings(db)
            }

            Type::TypeAlias(alias) => alias.value_type(db).bindings(db),

            Type::PropertyInstance(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::BoundSuper(_)
            | Type::ModuleLiteral(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypedDict(_) => CallableBinding::not_callable(self).into(),
        }
    }

    fn known_class_literal_bindings(
        self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Option<Bindings<'db>> {
        // TODO: Some of these cases date back to when we didn't even support overloads yet; see if
        // any can be removed: https://github.com/astral-sh/ty/issues/2715
        match class.known(db)? {
            KnownClass::Bool => {
                // ```py
                // class bool(int):
                //     def __new__(cls, o: object = ..., /) -> Self: ...
                // ```
                Some(
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new(
                                db,
                                [Parameter::positional_only(Some(Name::new_static("o")))
                                    .with_annotated_type(Type::any())
                                    .with_default_type(Type::bool_literal(false))],
                            ),
                            KnownClass::Bool.to_instance(db),
                        ),
                    )
                    .into(),
                )
            }

            KnownClass::Object => {
                // ```py
                // class object:
                //    def __init__(self) -> None: ...
                //    def __new__(cls) -> Self: ...
                // ```
                Some(
                    Binding::single(self, Signature::new(Parameters::empty(), Type::object()))
                        .into(),
                )
            }

            KnownClass::Super => {
                // ```py
                // class super:
                //     @overload
                //     def __init__(self, t: Any, obj: Any, /) -> None: ...
                //     @overload
                //     def __init__(self, t: Any, /) -> None: ...
                //     @overload
                //     def __init__(self) -> None: ...
                // ```
                Some(
                    CallableBinding::from_overloads(
                        self,
                        [
                            Signature::new(
                                Parameters::new(
                                    db,
                                    [
                                        Parameter::positional_only(Some(Name::new_static("t")))
                                            .with_annotated_type(Type::any()),
                                        Parameter::positional_only(Some(Name::new_static("obj")))
                                            .with_annotated_type(Type::any()),
                                    ],
                                ),
                                KnownClass::Super.to_instance(db),
                            ),
                            Signature::new(
                                Parameters::new(
                                    db,
                                    [Parameter::positional_only(Some(Name::new_static("t")))
                                        .with_annotated_type(Type::any())],
                                ),
                                KnownClass::Super.to_instance(db),
                            ),
                            Signature::new(Parameters::empty(), KnownClass::Super.to_instance(db)),
                        ],
                    )
                    .into(),
                )
            }

            KnownClass::Deprecated => {
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
                Some(
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("message")))
                                        .with_annotated_type(Type::literal_string()),
                                    Parameter::keyword_only(Name::new_static("category"))
                                        .with_annotated_type(UnionType::from_two_elements(
                                            db,
                                            // TODO: should be `type[Warning]`
                                            Type::any(),
                                            KnownClass::NoneType.to_instance(db),
                                        ))
                                        // TODO: should be `type[Warning]`
                                        .with_default_type(Type::any()),
                                    Parameter::keyword_only(Name::new_static("stacklevel"))
                                        .with_annotated_type(KnownClass::Int.to_instance(db))
                                        .with_default_type(Type::int_literal(1)),
                                ],
                            ),
                            KnownClass::Deprecated.to_instance(db),
                        ),
                    )
                    .into(),
                )
            }

            KnownClass::TypeAliasType => {
                // ```py
                // def __new__(
                //     cls,
                //     name: str,
                //     value: Any,
                //     *,
                //     type_params: tuple[TypeVar | ParamSpec | TypeVarTuple, ...] = ()
                // ) -> Self: ...
                // ```
                Some(
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new(
                                db,
                                [
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
                                ],
                            ),
                            Type::unknown(),
                        ),
                    )
                    .into(),
                )
            }

            KnownClass::Property => {
                let getter_signature = Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::positional_only(None).with_annotated_type(Type::any())],
                    ),
                    Type::any(),
                );
                let setter_signature = Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(None).with_annotated_type(Type::any()),
                            Parameter::positional_only(None).with_annotated_type(Type::any()),
                        ],
                    ),
                    Type::none(db),
                );
                let deleter_signature = Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::positional_only(None).with_annotated_type(Type::any())],
                    ),
                    Type::any(),
                );

                Some(
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_or_keyword(Name::new_static("fget"))
                                        .with_annotated_type(UnionType::from_two_elements(
                                            db,
                                            Type::single_callable(db, getter_signature),
                                            Type::none(db),
                                        ))
                                        .with_default_type(Type::none(db)),
                                    Parameter::positional_or_keyword(Name::new_static("fset"))
                                        .with_annotated_type(UnionType::from_two_elements(
                                            db,
                                            Type::single_callable(db, setter_signature),
                                            Type::none(db),
                                        ))
                                        .with_default_type(Type::none(db)),
                                    Parameter::positional_or_keyword(Name::new_static("fdel"))
                                        .with_annotated_type(UnionType::from_two_elements(
                                            db,
                                            Type::single_callable(db, deleter_signature),
                                            Type::none(db),
                                        ))
                                        .with_default_type(Type::none(db)),
                                    Parameter::positional_or_keyword(Name::new_static("doc"))
                                        .with_annotated_type(UnionType::from_two_elements(
                                            db,
                                            KnownClass::Str.to_instance(db),
                                            Type::none(db),
                                        ))
                                        .with_default_type(Type::none(db)),
                                ],
                            ),
                            Type::unknown(),
                        ),
                    )
                    .into(),
                )
            }

            KnownClass::FunctoolsPartial => {
                // ```py
                // class partial(Generic[_T]):
                //     def __new__(cls, func: Callable[..., _T], /, *args: Any, **kwargs: Any) -> Self: ...
                // ```
                let return_ty = BoundTypeVarInstance::synthetic(
                    db,
                    Name::new_static("_T"),
                    TypeVarVariance::Covariant,
                );

                Some(
                    Binding::single(
                        self,
                        Signature::new_generic(
                            Some(GenericContext::from_typevar_instances(db, [return_ty])),
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_only(Some(Name::new_static("func")))
                                        .with_annotated_type(Type::single_callable(
                                            db,
                                            Signature::new(
                                                Parameters::gradual_form(),
                                                Type::TypeVar(return_ty),
                                            ),
                                        )),
                                    Parameter::variadic(Name::new_static("args"))
                                        .with_annotated_type(Type::any()),
                                    Parameter::keyword_variadic(Name::new_static("kwargs"))
                                        .with_annotated_type(Type::any()),
                                ],
                            ),
                            KnownClass::FunctoolsPartial
                                .to_specialized_instance(db, &[Type::TypeVar(return_ty)]),
                        ),
                    )
                    .into(),
                )
            }

            KnownClass::Tuple => {
                let element_ty = BoundTypeVarInstance::synthetic(
                    db,
                    Name::new_static("T"),
                    TypeVarVariance::Covariant,
                );

                // ```py
                // class tuple(Sequence[_T_co]):
                //     @overload
                //     def __new__(cls) -> tuple[()]: ...
                //     @overload
                //     def __new__(cls, iterable: Iterable[_T_co]) -> tuple[_T_co, ...]: ...
                // ```
                Some(
                    CallableBinding::from_overloads(
                        self,
                        [
                            Signature::new(Parameters::empty(), Type::empty_tuple(db)),
                            Signature::new_generic(
                                Some(GenericContext::from_typevar_instances(db, [element_ty])),
                                Parameters::new(
                                    db,
                                    [Parameter::positional_only(Some(Name::new_static(
                                        "iterable",
                                    )))
                                    .with_annotated_type(
                                        KnownClass::Iterable.to_specialized_instance(
                                            db,
                                            &[Type::TypeVar(element_ty)],
                                        ),
                                    )],
                                ),
                                Type::homogeneous_tuple(db, Type::TypeVar(element_ty)),
                            ),
                        ],
                    )
                    .into(),
                )
            }

            _ => None,
        }
    }

    // Build bindings for constructor calls by combining `__new__`/`__init__` signatures.
    // Returns fallback bindings for cases that intentionally keep bespoke call behavior.
    fn constructor_bindings(self, db: &'db dyn Db, class: ClassType<'db>) -> Bindings<'db> {
        fn resolve_dunder_new_callable<'db>(
            db: &'db dyn Db,
            owner: Type<'db>,
            place: Place<'db>,
        ) -> Option<(Type<'db>, Definedness)> {
            // If `__new__` itself resolved to `Any`, treat it as absent rather than as a real
            // constructor override. This preserves the known nominal constructor result for
            // subclasses of `Any` while still allowing explicitly typed `__new__` callables
            // returning `Any` to keep their annotated behavior.
            if matches!(
                place,
                Place::Defined(DefinedPlace {
                    ty: Type::Dynamic(DynamicType::Any),
                    ..
                })
            ) {
                return None;
            }
            match place.try_call_dunder_get(db, owner) {
                Place::Defined(DefinedPlace {
                    ty: callable,
                    definedness,
                    ..
                }) => Some((callable, definedness)),
                Place::Undefined => None,
            }
        }
        fn bind_constructor_new<'db>(
            db: &'db dyn Db,
            bindings: Bindings<'db>,
            self_type: Type<'db>,
        ) -> Bindings<'db> {
            bindings.map(|binding| {
                let mut binding = binding;
                // If descriptor binding produced a bound callable, bake that into the signature
                // first, then bind `cls` for constructor-call semantics (the call site omits `cls`).
                // Note: This intentionally preserves `type.__call__` behavior for `@classmethod __new__`,
                // which receives an extra implicit `cls` and errors at call sites.
                binding.bake_bound_type_into_overloads(db);
                binding.bound_type = Some(self_type);
                binding
            })
        }

        let (class_literal, class_specialization) = class.class_literal_and_specialization(db);
        let class_generic_context = class_literal.generic_context(db);

        // Keep bespoke constructor behavior for cases that don't map cleanly to `__new__`/`__init__`.
        let fallback_bindings = || {
            let return_type = self.to_instance(db).unwrap_or(Type::unknown());
            Binding::single(
                self,
                Signature::new_generic(
                    class_generic_context,
                    Parameters::gradual_form(),
                    return_type,
                ),
            )
            .into()
        };

        // Checking TypedDict construction happens in `infer_call_expression_impl`.
        // We don't want to use the synthesized binding for type inference, so here we just
        // return a permissive fallback binding.
        if class_literal.is_typed_dict(db)
            || class::CodeGeneratorKind::TypedDict.matches(db, class_literal, class_specialization)
        {
            return fallback_bindings();
        }

        // These cases are checked in `Type::known_class_literal_bindings`, but currently we only
        // call that for `ClassLiteral` types, so we need a permissive fallback here. TODO Ideally
        // that would be called from `constructor_bindings` for better consistency, but that causes
        // some test failures deserving separate investigation.
        let known = class.known(db);
        if matches!(
            known,
            Some(
                KnownClass::Bool
                    | KnownClass::Type
                    | KnownClass::Object
                    | KnownClass::FunctoolsPartial
                    | KnownClass::Property
                    | KnownClass::Super
                    | KnownClass::TypeAliasType
                    | KnownClass::Deprecated
            )
        ) {
            return fallback_bindings();
        }

        // Temporary special-casing for all subclasses of `enum.Enum` until we support the
        // functional syntax for creating enum classes. TODO we should ideally check e.g.
        // `MyEnum(1)` to make sure `1` is a valid value for `MyEnum`.
        if KnownClass::Enum
            .to_class_literal(db)
            .to_class_type(db)
            .is_some_and(|enum_class| class.is_subclass_of(db, enum_class))
        {
            return fallback_bindings();
        }

        // If we are trying to construct a non-specialized generic class, we should use the
        // constructor parameters to try to infer the class specialization. To do this, we need to
        // tweak our member lookup logic a bit. Normally, when looking up a class or instance
        // member, we first apply the class's default specialization, and apply that specialization
        // to the type of the member. To infer a specialization from the argument types, we need to
        // have the class's typevars still in the method signature when we attempt to call it. To
        // do this, we instead use the _identity_ specialization, which maps each of the class's
        // generic typevars to itself.
        let self_type = match self {
            Type::ClassLiteral(class) if class.generic_context(db).is_some() => {
                Type::from(class.identity_specialization(db))
            }
            _ => self,
        };

        // Check for a custom `__call__` on the metaclass (excluding `type.__call__`).
        // We preserve its full overload set here and defer constructor branching decisions
        // until call-time overload resolution.
        let metaclass_dunder_call = self_type.member_lookup_with_policy(
            db,
            "__call__".into(),
            MemberLookupPolicy::NO_INSTANCE_FALLBACK
                | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
        );

        let Some(constructor_instance_ty) = self_type.to_instance(db) else {
            return fallback_bindings();
        };

        let new_method = self_type.lookup_dunder_new(db);

        let init_method_no_object = constructor_instance_ty.member_lookup_with_policy(
            db,
            "__init__".into(),
            MemberLookupPolicy::NO_INSTANCE_FALLBACK | MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
        );

        let (new_bindings, has_any_new) = match new_method.as_ref().map(|method| method.place) {
            Some(place) => match resolve_dunder_new_callable(db, self_type, place) {
                Some((new_callable, definedness)) => {
                    let mut bindings =
                        bind_constructor_new(db, new_callable.bindings(db), self_type)
                            .into_constructor_bindings(
                                constructor_instance_ty,
                                ConstructorCallableKind::New,
                            )
                            .with_constructed_instance_type(db, constructor_instance_ty);
                    if definedness == Definedness::PossiblyUndefined {
                        bindings.set_implicit_dunder_new_is_possibly_unbound();
                    }
                    (Some(bindings), true)
                }
                None => (None, false),
            },
            None => (None, false),
        };

        // Only fall back to `object.__init__` when `__new__` is absent.
        let init_bindings = match (&init_method_no_object.place, has_any_new) {
            (
                Place::Defined(DefinedPlace {
                    ty: init_method,
                    definedness,
                    ..
                }),
                _,
            ) => {
                let mut bindings = init_method
                    .bindings(db)
                    .into_constructor_bindings(
                        constructor_instance_ty,
                        ConstructorCallableKind::Init,
                    )
                    .with_constructed_instance_type(db, constructor_instance_ty);
                if *definedness == Definedness::PossiblyUndefined {
                    bindings.set_implicit_dunder_init_is_possibly_unbound();
                }
                Some(bindings)
            }
            (Place::Undefined, false) => {
                let init_method_with_object = constructor_instance_ty.member_lookup_with_policy(
                    db,
                    "__init__".into(),
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                );
                match init_method_with_object.place {
                    Place::Defined(DefinedPlace {
                        ty: init_method,
                        definedness,
                        ..
                    }) => {
                        let mut bindings = init_method
                            .bindings(db)
                            .into_constructor_bindings(
                                constructor_instance_ty,
                                ConstructorCallableKind::Init,
                            )
                            .with_constructed_instance_type(db, constructor_instance_ty);
                        if definedness == Definedness::PossiblyUndefined {
                            bindings.set_implicit_dunder_init_is_possibly_unbound();
                        }
                        Some(bindings)
                    }
                    Place::Undefined => {
                        // If we are using vendored typeshed, it should be impossible to have missing
                        // or unbound `__init__` method on a class, as all classes have `object` in MRO.
                        // Thus the following may only trigger if a custom typeshed is used.
                        // Custom/broken typeshed: no `__init__` available even after falling back
                        // to `object`. Keep analysis going and surface the missing-implicit-call
                        // lint via the builder.
                        let mut bindings: Bindings<'db> = Binding::single(
                            self_type,
                            Signature::new(Parameters::gradual_form(), constructor_instance_ty),
                        )
                        .into();
                        bindings = bindings
                            .into_constructor_bindings(
                                constructor_instance_ty,
                                ConstructorCallableKind::Init,
                            )
                            .with_constructed_instance_type(db, constructor_instance_ty);
                        bindings.set_implicit_dunder_init_is_possibly_unbound();
                        Some(bindings)
                    }
                }
            }
            (Place::Undefined, true) => None,
        };

        let constructor_bindings = if let Some(mut new_bindings) = new_bindings {
            // Preserve the full `__new__` signature and defer `__init__` validation until we know
            // which `__new__` overload matched at call time.
            if let Some(init_bindings) = init_bindings.as_ref() {
                new_bindings.set_downstream_constructor(init_bindings);
            }
            Some(new_bindings)
        } else {
            init_bindings
        };

        let bindings = if let Place::Defined(DefinedPlace {
            ty: metaclass_call_method,
            ..
        }) = metaclass_dunder_call.place
        {
            let mut metaclass_bindings = metaclass_call_method
                .bindings(db)
                .into_constructor_bindings(
                    constructor_instance_ty,
                    ConstructorCallableKind::MetaclassCall,
                )
                .with_constructed_instance_type(db, constructor_instance_ty);
            if let Some(downstream_bindings) = constructor_bindings.as_ref() {
                // Preserve the full metaclass `__call__` signature and defer whether constructor
                // downstream checks apply until the matched overload is known.
                metaclass_bindings.set_downstream_constructor(downstream_bindings);
            }
            metaclass_bindings
        } else if let Some(constructor_bindings) = constructor_bindings {
            constructor_bindings
        } else {
            return fallback_bindings();
        };

        bindings.with_generic_context(db, class_generic_context)
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
        let constraints = ConstraintSetBuilder::new();
        self.bindings(db)
            .match_parameters(db, argument_types)
            .check_types(
                db,
                &constraints,
                argument_types,
                TypeContext::default(),
                &[],
            )
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
        tcx: TypeContext<'db>,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        self.try_call_dunder_with_policy(
            db,
            name,
            &mut argument_types,
            tcx,
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
        tcx: TypeContext<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        if let Type::Intersection(intersection) = self {
            return intersection.try_call_dunder_with_policy(db, name, argument_types, tcx, policy);
        }

        if let Type::Union(union) = self {
            return union.try_call_dunder_with_policy(db, name, argument_types, tcx, policy);
        }

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
            Place::Defined(DefinedPlace {
                ty: dunder_callable,
                definedness: boundness,
                ..
            }) => {
                let constraints = ConstraintSetBuilder::new();
                let bindings = dunder_callable
                    .bindings(db)
                    .match_parameters(db, argument_types)
                    .check_types(db, &constraints, argument_types, tcx, &[])?;

                if boundness == Definedness::PossiblyUndefined {
                    return Err(CallDunderError::PossiblyUnbound {
                        bindings: Box::new(bindings),
                        unbound_on: None,
                    });
                }
                Ok(bindings)
            }
            Place::Undefined => Err(CallDunderError::MethodNotAvailable),
        }
    }

    /// Attempt to call a dunder method defined on a class itself.
    ///
    /// This is used for methods like `__class_getitem__` which are implicitly called
    /// when subscripting the class itself (e.g., `MyClass[int]`). These dunder methods
    /// need to be looked up on the metaclass AND the class itself. So unlike
    /// `try_call_dunder`, this does NOT add `NO_INSTANCE_FALLBACK`, allowing the lookup
    /// to find methods defined on the class when `self` is a class literal.
    fn try_call_dunder_on_class(
        self,
        db: &'db dyn Db,
        name: &str,
        argument_types: &CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        match self.member(db, name).place {
            Place::Defined(DefinedPlace {
                ty: dunder_callable,
                definedness: boundness,
                ..
            }) => {
                let constraints = ConstraintSetBuilder::new();
                let bindings = dunder_callable
                    .bindings(db)
                    .match_parameters(db, argument_types)
                    .check_types(db, &constraints, argument_types, tcx, &[])?;

                if boundness == Definedness::PossiblyUndefined {
                    return Err(CallDunderError::PossiblyUnbound {
                        bindings: Box::new(bindings),
                        unbound_on: None,
                    });
                }
                Ok(bindings)
            }
            Place::Undefined => Err(CallDunderError::MethodNotAvailable),
        }
    }

    /// Apply `__getattr__` / `__getattribute__` fallback to an attribute-lookup result.
    ///
    /// If `result` is already always-defined, return it unchanged. Otherwise, fall back to calling
    /// `__getattribute__` (and then `__getattr__`) on the meta-type of `self`.
    fn fallback_to_getattr(
        self,
        db: &'db dyn Db,
        name: &Name,
        result: PlaceAndQualifiers<'db>,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        let custom_getattr_result = || {
            if policy.no_getattr_lookup() {
                return Place::Undefined.into();
            }

            self.try_call_dunder(
                db,
                "__getattr__",
                CallArguments::positional([Type::string_literal(db, name)]),
                TypeContext::default(),
            )
            .map(|outcome| Place::bound(outcome.return_type(db)))
            // TODO: Handle call errors here.
            .unwrap_or_default()
            .into()
        };

        let custom_getattribute_result = || {
            if "__getattribute__" == name.as_str() {
                return Place::Undefined.into();
            }

            // Skip `object.__getattribute__`, which is the default mechanism we
            // already model via the normal attribute-lookup path.
            self.try_call_dunder_with_policy(
                db,
                "__getattribute__",
                &mut CallArguments::positional([Type::string_literal(db, name)]),
                TypeContext::default(),
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
            )
            .map(|outcome| Place::bound(outcome.return_type(db)))
            // TODO: Handle call errors here.
            .unwrap_or_default()
            .into()
        };

        match result {
            member @ PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        definedness: Definedness::AlwaysDefined,
                        ..
                    }),
                qualifiers: _,
            } => member,
            member @ PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        definedness: Definedness::PossiblyUndefined,
                        ..
                    }),
                qualifiers: _,
            } => member
                .or_fall_back_to(db, custom_getattribute_result)
                .or_fall_back_to(db, custom_getattr_result),
            PlaceAndQualifiers {
                place: Place::Undefined,
                qualifiers: _,
            } => custom_getattribute_result().or_fall_back_to(db, custom_getattr_result),
        }
    }

    /// Flatten typevars in a union or intersection by resolving them to their upper bounds
    /// or constraints.
    ///
    /// This function is used to properly handle iteration over intersections containing
    /// typevars with union bounds. For example, given `T & tuple[object, ...]` where
    /// `T: tuple[int, ...] | list[str]`, this will:
    /// 1. Replace `T` with `tuple[int, ...] | list[str]`.
    /// 2. Rebuild through the intersection builder, which distributes to get:
    ///    `(tuple[int, ...] & tuple[object, ...]) | (list[str] & tuple[object, ...])`.
    /// 3. The builder simplifies each part (e.g., list is disjoint from `tuple`, which
    ///    simplifies to `Never`).
    /// 4. Final result: `tuple[int, ...]`.
    ///
    /// This only flattens typevars directly in unions and intersections; it does not descend
    /// into generic types or other nested structures.
    fn flatten_typevars(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::TypeVar(tvar) => match tvar.typevar(db).bound_or_constraints(db) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound.flatten_typevars(db),
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    constraints.as_type(db).flatten_typevars(db)
                }
                // Unbounded typevar is effectively `object`.
                None => Type::object(),
            },
            Type::Union(union) => {
                // Flatten each element and rebuild through the union builder.
                UnionType::from_elements(
                    db,
                    union.elements(db).iter().map(|e| e.flatten_typevars(db)),
                )
            }
            Type::Intersection(intersection) => {
                // Flatten each positive element and rebuild through the intersection builder.
                let mut builder = IntersectionBuilder::new(db);
                for pos in intersection.positive(db) {
                    builder = builder.add_positive(pos.flatten_typevars(db));
                }
                for neg in intersection.negative(db) {
                    builder = builder.add_negative(neg.flatten_typevars(db));
                }
                builder.build()
            }
            // Don't descend into other types; only flatten top-level typevars.
            _ => self,
        }
    }

    /// Resolve the type of an `await …` expression where `self` is the type of the awaitable.
    fn try_await(self, db: &'db dyn Db) -> Result<Type<'db>, AwaitError<'db>> {
        let await_result = self.try_call_dunder(
            db,
            "__await__",
            CallArguments::none(),
            TypeContext::default(),
        );
        match await_result {
            Ok(bindings) => {
                let return_type = bindings.return_type(db);
                Ok(return_type.generator_return_type(db).ok_or_else(|| {
                    AwaitError::InvalidReturnType(return_type, Box::new(bindings))
                })?)
            }
            Err(call_error) => Err(AwaitError::Call(call_error)),
        }
    }

    /// Get the return type of a `yield from …` expression where `self` is the type of the generator.
    ///
    /// This corresponds to the `ReturnT` parameter of the generic `typing.Generator[YieldT, SendT, ReturnT]`
    /// protocol.
    fn generator_types(self, db: &'db dyn Db) -> Option<GeneratorTypes<'db>> {
        // TODO: Ideally, we would first try to upcast `self` to an instance of `Generator` and *then*
        // match on the protocol instance to get the `ReturnType` type parameter. For now, implement
        // an ad-hoc solution that works for protocols and instances of classes that explicitly inherit
        // from the `Generator` protocol, such as `types.GeneratorType`.

        let from_class_base = |base: ClassBase<'db>| {
            let class = base.into_class()?;
            let (_, Some(specialization)) = class.static_class_literal_specialized(db, None)?
            else {
                return None;
            };

            if class.is_known(db, KnownClass::Generator)
                && let [yield_ty, send_ty, return_ty] = specialization.types(db)
            {
                Some(GeneratorTypes {
                    yield_ty: Some(*yield_ty),
                    send_ty: Some(*send_ty),
                    return_ty: Some(*return_ty),
                })
            } else if class.is_known(db, KnownClass::AsyncGenerator)
                && let [yield_ty, send_ty] = specialization.types(db)
            {
                Some(GeneratorTypes {
                    yield_ty: Some(*yield_ty),
                    send_ty: Some(*send_ty),
                    return_ty: None,
                })
            } else if (class.is_known(db, KnownClass::Iterator)
                || class.is_known(db, KnownClass::AsyncIterator))
                && let [yield_ty] = specialization.types(db)
            {
                Some(GeneratorTypes {
                    yield_ty: Some(*yield_ty),
                    send_ty: Some(Type::none(db)),
                    return_ty: Some(Type::none(db)),
                })
            } else {
                None
            }
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
            Type::Union(union) => {
                let mut yield_builder = Some(UnionBuilder::new(db));
                let mut send_builder = Some(UnionBuilder::new(db));
                let mut return_builder = Some(UnionBuilder::new(db));

                for ty in union.elements(db) {
                    let gt = ty.generator_types(db)?;
                    match gt.yield_ty {
                        Some(ty) => yield_builder = yield_builder.map(|b| b.add(ty)),
                        None => yield_builder = None,
                    }
                    match gt.send_ty {
                        Some(ty) => send_builder = send_builder.map(|b| b.add(ty)),
                        None => send_builder = None,
                    }
                    match gt.return_ty {
                        Some(ty) => return_builder = return_builder.map(|b| b.add(ty)),
                        None => return_builder = None,
                    }
                }

                Some(GeneratorTypes {
                    yield_ty: yield_builder.map(UnionBuilder::build),
                    send_ty: send_builder.map(UnionBuilder::build),
                    return_ty: return_builder.map(UnionBuilder::build),
                })
            }
            Type::Intersection(intersection) => {
                // Using `positive()` rather than `positive_elements_or_object()` is safe
                // here because `object` is not a generator, so falling back to it would
                // still return `None`.
                let mut yield_builder = Some(IntersectionBuilder::new(db));
                let mut send_builder = Some(IntersectionBuilder::new(db));
                let mut return_builder = Some(IntersectionBuilder::new(db));
                let mut any_success = false;

                for ty in intersection.positive(db) {
                    let Some(gt) = ty.generator_types(db) else {
                        continue;
                    };
                    any_success = true;
                    match gt.yield_ty {
                        Some(ty) => {
                            yield_builder = yield_builder.map(|b| b.add_positive(ty));
                        }
                        None => yield_builder = None,
                    }
                    match gt.send_ty {
                        Some(ty) => {
                            send_builder = send_builder.map(|b| b.add_positive(ty));
                        }
                        None => send_builder = None,
                    }
                    match gt.return_ty {
                        Some(ty) => {
                            return_builder = return_builder.map(|b| b.add_positive(ty));
                        }
                        None => return_builder = None,
                    }
                }

                if !any_success {
                    return None;
                }

                Some(GeneratorTypes {
                    yield_ty: yield_builder.map(IntersectionBuilder::build),
                    send_ty: send_builder.map(IntersectionBuilder::build),
                    return_ty: return_builder.map(IntersectionBuilder::build),
                })
            }
            ty @ (Type::Dynamic(_) | Type::Divergent(_) | Type::Never) => Some(GeneratorTypes {
                yield_ty: Some(ty),
                send_ty: Some(ty),
                return_ty: Some(ty),
            }),
            _ => None,
        }
    }

    fn generator_return_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        self.generator_types(db)
            .and_then(|generator_types| generator_types.return_ty)
    }

    fn generator_send_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        self.generator_types(db)
            .and_then(|generator_types| generator_types.send_ty)
    }

    #[must_use]
    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => Some(self),
            Type::ClassLiteral(class) => Some(Type::instance(db, class.default_specialization(db))),
            Type::GenericAlias(alias) => Some(Type::instance(db, ClassType::from(alias))),
            Type::SubclassOf(subclass_of_ty) => Some(subclass_of_ty.to_instance(db)),
            Type::KnownInstance(KnownInstanceType::NewType(newtype)) => {
                Some(Type::NewTypeInstance(newtype))
            }
            Type::Union(union) => union.to_instance(db),
            // If there is no bound or constraints on a typevar `T`, `T: object` implicitly, which
            // has no instance type. Otherwise, synthesize a typevar with bound or constraints
            // mapped through `to_instance`.
            Type::TypeVar(bound_typevar) => Some(Type::TypeVar(bound_typevar.to_instance(db)?)),
            Type::TypeAlias(alias) => alias.value_type(db).to_instance(db),
            Type::Intersection(_) => Some(todo_type!("Type::Intersection.to_instance")),
            // An instance of class `C` may itself have instances if `C` is a subclass of `type`.
            Type::NominalInstance(instance)
                if KnownClass::Type
                    .to_class_literal(db)
                    .to_class_type(db)
                    .is_some_and(|type_class| {
                        instance.class(db).is_subclass_of(db, type_class)
                    }) =>
            {
                Some(Type::object())
            }
            Type::FunctionLiteral(_)
            | Type::Callable(..)
            | Type::KnownBoundMethod(_)
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
            | Type::LiteralValue(_)
            | Type::BoundSuper(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_) => None,
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
        inference_flags: InferenceFlags,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        match self {
            // Special cases for `float` and `complex`
            // https://typing.python.org/en/latest/spec/special-types.html#special-cases-for-float-and-complex
            Type::ClassLiteral(class) => {
                let ty = match class.known(db) {
                    Some(KnownClass::Complex) => KnownUnion::Complex.to_type(db),
                    Some(KnownClass::Float) => KnownUnion::Float.to_type(db),
                    _ => Type::instance(db, class.default_specialization(db)),
                };
                Ok(ty)
            }
            Type::GenericAlias(alias) => Ok(Type::instance(db, ClassType::from(*alias))),

            Type::SubclassOf(_)
            | Type::LiteralValue(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::ModuleLiteral(_)
            | Type::TypeVar(_)
            | Type::Callable(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::Never
            | Type::FunctionLiteral(_)
            | Type::BoundSuper(_)
            | Type::ProtocolInstance(_)
            | Type::PropertyInstance(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypedDict(_) => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec_inline![InvalidTypeExpression::InvalidType(
                    *self, scope_id
                )],
                fallback_type: Type::unknown(),
            }),

            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::TypeAliasType(alias) => Ok(Type::TypeAlias(*alias)),
                KnownInstanceType::NewType(newtype) => Ok(Type::NewTypeInstance(*newtype)),
                KnownInstanceType::TypeVar(typevar) => {
                    if !inference_flags.contains(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR)
                        && typevar.is_paramspec(db)
                    {
                        return Err(InvalidTypeExpressionError {
                            invalid_expressions: smallvec_inline![
                                InvalidTypeExpression::InvalidBareParamSpec(*typevar)
                            ],
                            fallback_type: Type::unknown(),
                        });
                    }
                    let index = semantic_index(db, scope_id.file(db));
                    Ok(bind_typevar(
                        db,
                        index,
                        scope_id.file_scope_id(db),
                        typevar_binding_context,
                        *typevar,
                    )
                    .map(Type::TypeVar)
                    .unwrap_or(*self))
                }
                KnownInstanceType::Deprecated(_) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::Deprecated],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::Field(__call__) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::Field],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::ConstraintSet(__call__) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::ConstraintSet],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::GenericContext(__call__) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::GenericContext],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::Specialization(__call__) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::Specialization],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::SubscriptedProtocol(_) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::Protocol],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::SubscriptedGeneric(_) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::Generic],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::NamedTupleSpec(_) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::NamedTupleSpec],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::UnionType(instance) => {
                    // Cloning here is cheap if the result is a `Type` (which is `Copy`). It's more
                    // expensive if there are errors.
                    instance.union_type(db).clone()
                }
                KnownInstanceType::Literal(ty) => Ok(ty.inner(db)),
                KnownInstanceType::Annotated(ty) => Ok(ty.inner(db)),
                KnownInstanceType::TypeGenericAlias(instance) => {
                    // When `type[…]` appears in a value position (e.g. in an implicit type alias),
                    // we infer its argument as a type expression. This ensures that we can emit
                    // diagnostics for invalid type expressions, and more importantly, that we can
                    // make use of stringified annotations. The drawback is that we need to turn
                    // instances back into the corresponding subclass-of types here. This process
                    // (`int` -> instance of `int` -> subclass of `int`) can be lossy, but it is
                    // okay for all valid arguments to `type[…]`.

                    Ok(instance.inner(db).to_meta_type(db))
                }
                KnownInstanceType::Callable(callable) => Ok(Type::Callable(*callable)),
                KnownInstanceType::LiteralStringAlias(ty) => Ok(ty.inner(db)),
                KnownInstanceType::FunctoolsPartial(_) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::InvalidType(
                        *self, scope_id
                    )],
                    fallback_type: Type::unknown(),
                }),
            },

            Type::SpecialForm(special_form) => special_form
                .in_type_expression(db, scope_id, typevar_binding_context, inference_flags)
                .map_err(|err| {
                    let fallback_type = if matches!(
                        err,
                        InvalidTypeExpression::Concatenate
                            | InvalidTypeExpression::RequiresTwoArguments(
                                SpecialFormType::Concatenate
                            )
                    ) {
                        Type::Dynamic(DynamicType::InvalidConcatenateUnknown)
                    } else {
                        Type::unknown()
                    };

                    InvalidTypeExpressionError {
                        fallback_type,
                        invalid_expressions: smallvec_inline![err],
                    }
                }),

            Type::Union(union) => {
                let mut builder = UnionBuilder::new(db);
                let mut invalid_expressions = smallvec::SmallVec::default();
                for element in union.elements(db) {
                    match element.in_type_expression(
                        db,
                        scope_id,
                        typevar_binding_context,
                        inference_flags,
                    ) {
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

            Type::Dynamic(_) | Type::Divergent(_) => Ok(*self),

            Type::NominalInstance(instance) => match instance.known_class(db) {
                Some(KnownClass::NoneType) => Ok(Type::none(db)),
                Some(KnownClass::TypeVar) => Ok(todo_type!(
                    "Support for `typing.TypeVar` instances in type expressions"
                )),
                Some(KnownClass::TypeVarTuple) => Ok(Type::Dynamic(DynamicType::TodoTypeVarTuple)),
                _ => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::InvalidType(
                        *self, scope_id
                    )],
                    fallback_type: Type::unknown(),
                }),
            },

            Type::Intersection(_) => Ok(todo_type!("Type::Intersection.in_type_expression")),

            Type::TypeAlias(alias) => alias.value_type(db).in_type_expression(
                db,
                scope_id,
                typevar_binding_context,
                inference_flags,
            ),

            Type::NewTypeInstance(_) => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec_inline![InvalidTypeExpression::InvalidType(
                    *self, scope_id
                )],
                fallback_type: Type::unknown(),
            }),
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
    pub(crate) fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::Never => Type::Never,
            Type::NominalInstance(instance) => instance.to_meta_type(db),
            Type::KnownInstance(known_instance) => known_instance.to_meta_type(db),
            Type::SpecialForm(special_form) => special_form.to_meta_type(db),
            Type::PropertyInstance(_) => KnownClass::Property.to_class_literal(db),
            Type::Union(union) => union.map(db, |ty| ty.to_meta_type(db)),
            Type::TypeIs(_) | Type::TypeGuard(_) => KnownClass::Bool.to_class_literal(db),
            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Bool(_) => KnownClass::Bool.to_class_literal(db),
                LiteralValueTypeKind::Bytes(_) => KnownClass::Bytes.to_class_literal(db),
                LiteralValueTypeKind::Int(_) => KnownClass::Int.to_class_literal(db),
                LiteralValueTypeKind::Enum(enum_literal) => {
                    Type::ClassLiteral(enum_literal.enum_class(db))
                }
                LiteralValueTypeKind::String(_) | LiteralValueTypeKind::LiteralString => {
                    KnownClass::Str.to_class_literal(db)
                }
            },
            Type::FunctionLiteral(_) => KnownClass::FunctionType.to_class_literal(db),
            Type::BoundMethod(_) => KnownClass::MethodType.to_class_literal(db),
            Type::KnownBoundMethod(method) => method.class().to_class_literal(db),
            Type::WrapperDescriptor(_) => KnownClass::WrapperDescriptorType.to_class_literal(db),
            Type::DataclassDecorator(_) => KnownClass::FunctionType.to_class_literal(db),
            Type::Callable(callable) if callable.is_function_like(db) => {
                KnownClass::FunctionType.to_class_literal(db)
            }
            Type::Callable(_) | Type::DataclassTransformer(_) => KnownClass::Type.to_instance(db),
            Type::ModuleLiteral(_) => KnownClass::ModuleType.to_class_literal(db),
            Type::TypeVar(bound_typevar) => {
                SubclassOfType::from(db, SubclassOfInner::TypeVar(bound_typevar))
            }
            Type::ClassLiteral(class) => class.metaclass(db),
            Type::GenericAlias(alias) => ClassType::from(alias).metaclass(db),
            Type::SubclassOf(subclass_of_ty) => subclass_of_ty.to_meta_type(db),
            Type::Dynamic(dynamic) => SubclassOfType::from(db, SubclassOfInner::Dynamic(dynamic)),
            Type::Divergent(_) => self,
            // TODO intersections
            Type::Intersection(_) => {
                SubclassOfType::try_from_type(db, todo_type!("Intersection meta-type"))
                    .expect("Type::Todo should be a valid `SubclassOfInner`")
            }
            Type::AlwaysTruthy | Type::AlwaysFalsy => KnownClass::Type.to_instance(db),
            Type::BoundSuper(_) => KnownClass::Super.to_class_literal(db),
            Type::ProtocolInstance(protocol) => protocol.to_meta_type(db),
            // `TypedDict` instances are instances of `dict` at runtime, but its important that we
            // understand a more specific meta type in order to correctly handle `__getitem__`.
            Type::TypedDict(typed_dict) => match typed_dict {
                TypedDictType::Class(class) => SubclassOfType::from(db, class),
                TypedDictType::Synthesized(_) => SubclassOfType::from(
                    db,
                    todo_type!("TypedDict synthesized meta-type").expect_dynamic(),
                ),
            },
            Type::TypeAlias(alias) => alias.value_type(db).to_meta_type(db),
            Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db).to_meta_type(db),
        }
    }

    /// Get the type of the `__class__` attribute of this type.
    ///
    /// For most types, this is equivalent to the meta type of this type. For `TypedDict` types,
    /// this returns `type[dict[str, object]]` instead, because inhabitants of a `TypedDict` are
    /// instances of `dict` at runtime.
    #[must_use]
    pub(crate) fn dunder_class(self, db: &'db dyn Db) -> Type<'db> {
        if self.is_typed_dict() {
            return KnownClass::Dict
                .to_specialized_class_type(db, &[KnownClass::Str.to_instance(db), Type::object()])
                .map(Type::from)
                // Guard against user-customized typesheds with a broken `dict` class
                .unwrap_or_else(Type::unknown);
        }

        self.to_meta_type(db)
    }

    #[must_use]
    pub(crate) fn apply_optional_specialization(
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
    #[salsa::tracked(
        cycle_initial=|_, id, _, _| Type::divergent(id),
        cycle_fn=|db, cycle, previous: &Type<'db>, value: Type<'db>, _, _| {
            value.cycle_normalized(db, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(crate) fn apply_specialization(
        self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Type<'db> {
        let type_mapping = match specialization.materialization_kind(db) {
            None => TypeMapping::ApplySpecialization(ApplySpecialization::Specialization(
                specialization,
            )),
            Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
                specialization: ApplySpecialization::Specialization(specialization),
                materialization_kind,
            },
        };

        self.apply_type_mapping(db, &type_mapping, TypeContext::default())
    }

    fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        self.apply_type_mapping_impl(db, type_mapping, tcx, &ApplyTypeMappingVisitor::default())
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Type<'db> {
        // If we are binding `typing.Self`, and this type is what we are binding `Self` to, return
        // early. This is not just an optimization, it also prevents us from infinitely expanding
        // the type, if it's something that can contain a `Self` reference.
        match type_mapping {
            TypeMapping::BindSelf(binding) if self == binding.self_type() => return self,
            _ => {}
        }

        // Recursive singleton promotion only recurses into `NominalInstance` types (tuples
        // and specialized generics). For all other types, return early.
        if matches!(
            type_mapping,
            TypeMapping::Promote(_, PromotionKind::SingletonsOnly)
        ) && !matches!(self, Type::NominalInstance(_))
        {
            return self;
        }

        match self {
            Type::TypeVar(bound_typevar) => bound_typevar.apply_type_mapping_impl(db, type_mapping, visitor),
            Type::KnownInstance(known_instance) => known_instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor),

            Type::FunctionLiteral(function) => visitor.visit(db, self, type_mapping, || {
                match type_mapping {
                    // Promote the types within the signature before promoting the signature to its
                    // callable form.
                    TypeMapping::Promote(PromotionMode::On, _) => {
                        Type::FunctionLiteral(function.apply_type_mapping_impl(
                            db,
                            type_mapping,
                            tcx,
                            visitor,
                        ))
                        .promote_impl(db)
                    }
                    _ => Type::FunctionLiteral(function.apply_type_mapping_impl(
                        db,
                        type_mapping,
                        tcx,
                        visitor,
                    )),
                }
            }),

            Type::BoundMethod(method) => Type::BoundMethod(BoundMethodType::new(
                db,
                method.function(db).apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                method.self_instance(db).apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            )),

            Type::NominalInstance(instance) if matches!(type_mapping, TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular)) => {
                match instance.known_class(db) {
                    Some(KnownClass::Complex) => KnownUnion::Complex.to_type(db),
                    Some(KnownClass::Float) => KnownUnion::Float.to_type(db),
                    _ => instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                }
            }

            Type::NominalInstance(instance) if matches!(type_mapping, TypeMapping::Promote(PromotionMode::On, PromotionKind::SingletonsOnly)) => {
                if instance.is_singleton(db) {
                    self.promote_singletons_impl(db)
                } else {
                    instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                }
            }

            Type::NominalInstance(instance) => {
                instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            },

            Type::NewTypeInstance(newtype) => visitor.visit(db, self, type_mapping, || {
                Type::NewTypeInstance(newtype.map_base_class_type(db, |class_type| {
                    class_type.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                }))
            }),

            Type::ProtocolInstance(instance) => {
                // TODO: Add tests for materialization once subtyping/assignability is implemented for
                // protocols. It _might_ require changing the logic here because:
                //
                // > Subtyping for protocol instances involves taking account of the fact that
                // > read-only property members, and method members, on protocols act covariantly;
                // > write-only property members act contravariantly; and read/write attribute
                // > members on protocols act invariantly
                Type::ProtocolInstance(instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }

            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(function)) => {
                Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(
                    function.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }

            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderCall(function)) => {
                Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderCall(
                    function.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }

            Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(property)) => {
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(
                    property.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }

            Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderSet(property)) => {
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderSet(
                    property.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }
            Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderDelete(property)) => {
                Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderDelete(
                    property.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                ))
            }

            Type::Callable(callable) => visitor.visit(db, self, type_mapping, || {
                Type::Callable(callable.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }),

            Type::GenericAlias(generic) => {
                Type::GenericAlias(generic.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }

            Type::TypedDict(typed_dict) => {
                Type::TypedDict(typed_dict.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }

            Type::SubclassOf(subclass_of) => subclass_of.apply_type_mapping_impl(db, type_mapping, tcx, visitor),

            Type::PropertyInstance(property) => {
                Type::PropertyInstance(property.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }

            Type::Union(union) => union.map_leave_aliases(db, |element| {
                element.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            }),
            Type::Intersection(intersection) => {
                let mut builder = IntersectionBuilder::new(db);
                for positive in intersection.positive(db) {
                    builder =
                        builder.add_positive(positive.apply_type_mapping_impl(db, type_mapping, tcx, visitor));
                }
                // Promotion should remove negative contributions from intersections,
                // so we don't preserve them here when promotion is enabled.
                if !matches!(type_mapping, TypeMapping::Promote(PromotionMode::On, _)) {
                    for negative in intersection.negative(db) {
                        builder = builder.add_negative(
                            negative.apply_type_mapping_impl(db, &type_mapping.flip(), tcx, visitor),
                        );
                    }
                }
                builder.build()
            }

            Type::TypeIs(type_is) => visitor.visit(db, self, type_mapping, || {
                type_is.with_type(
                    db,
                    type_is
                        .type_argument(db)
                        .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                )
            }),

            Type::TypeGuard(type_guard) => visitor.visit(db, self, type_mapping, || {
                type_guard.with_type(
                    db,
                    type_guard
                        .return_type(db)
                        .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                )
            }),

            Type::TypeAlias(alias) => {
                match type_mapping {
                    // For EagerExpansion, expand the raw value type. This path relies on Salsa's cycle
                    // detection rather than the visitor's cycle detection, because the visitor tracks
                    // Type values and `RecursiveList` is different from `RecursiveList[T]`.
                    TypeMapping::EagerExpansion => {
                        alias.raw_value_type(db).expand_eagerly(db)
                    },
                    // When specializing a generic type alias, instead of specializing the expanded type, the type alias itself is specialized.
                    // Without this special handling, recursive type aliases would result in cycles, returning an unspecialized fallback type.
                    TypeMapping::ApplySpecialization(specialization)
                    | TypeMapping::ApplySpecializationWithMaterialization { specialization, .. }
                    if matches!(specialization, ApplySpecialization::Specialization(_) | ApplySpecialization::Partial { .. }) => {
                        let mut current_specialization = specialization.as_specialization(db).unwrap();
                        if let TypeMapping::ApplySpecializationWithMaterialization {
                            materialization_kind,
                            ..
                        } = type_mapping
                        {
                            current_specialization = current_specialization
                                .with_materialization_kind(db, Some(*materialization_kind));
                        }
                        Type::TypeAlias(alias.apply_specialization(
                            db,
                            |generic_context| {
                                alias
                                    .specialization(db)
                                    .unwrap_or_else(|| generic_context.default_specialization(db, None))
                                    .apply_specialization(db, current_specialization)
                            },
                        ))
                    }
                    _ => {
                        // Do not call `value_type` here. `value_type` does the specialization internally, so `apply_type_mapping` is
                        // performed without `visitor` inheritance. In the case of recursive type aliases, this leads to infinite recursion.
                        // Instead, call `raw_value_type` and perform the specialization after the `visitor` cache has been created.
                        //
                        // IMPORTANT: All processing must happen inside a single visitor.visit() call so that if we encounter
                        // this same TypeAlias again (e.g., in `type RecursiveT = int | tuple[RecursiveT, ...]`), the visitor
                        // will detect the cycle and return the fallback value.
                        let mapped = visitor.visit(db, self, type_mapping, || {
                            let value_type = alias.raw_value_type(db).apply_type_mapping_impl(db, type_mapping, tcx, visitor);
                            alias.apply_function_specialization(db, value_type).apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                        });

                        // If the type mapping does not result in any change to this type alias, keep the
                        // alias node instead of eagerly expanding it.
                        if alias.value_type(db) == mapped {
                            self
                        } else {
                            mapped
                        }
                    }
                }
            }

            Type::LiteralValue(_) => match type_mapping {
                TypeMapping::ApplySpecialization(_) |
                TypeMapping::ApplySpecializationWithMaterialization { .. } |
                TypeMapping::BindLegacyTypevars(_) |
                TypeMapping::BindSelf { .. } |
                TypeMapping::ReplaceSelf { .. } |
                TypeMapping::Materialize(_) |
                TypeMapping::ReplaceParameterDefaults |
                TypeMapping::EagerExpansion |
                TypeMapping::RescopeReturnCallables(_) |
                TypeMapping::Promote(PromotionMode::Off, _) |
                TypeMapping::Promote(PromotionMode::On, PromotionKind::SingletonsOnly) => self,
                TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular) => self.promote_impl(db),
            }

            Type::Dynamic(_) => match type_mapping {
                TypeMapping::ApplySpecialization(_) |
                TypeMapping::ApplySpecializationWithMaterialization { .. } |
                TypeMapping::BindLegacyTypevars(_) |
                TypeMapping::BindSelf(..) |
                TypeMapping::ReplaceSelf { .. } |
                TypeMapping::Promote(..) |
                TypeMapping::ReplaceParameterDefaults |
                TypeMapping::EagerExpansion |
                TypeMapping::RescopeReturnCallables(_) => self,
                TypeMapping::Materialize(materialization_kind) => match materialization_kind {
                    MaterializationKind::Top => Type::object(),
                    MaterializationKind::Bottom => Type::Never,
                }
            }
            // `Divergent` is an internal cycle marker rather than a gradual type like `Any` or
            // `Unknown`. Preserve the marker across materialization, while recording whether this
            // occurrence should behave like the top (`object`) or bottom (`Never`) bound.
            Type::Divergent(divergent) => match type_mapping {
                TypeMapping::Materialize(materialization_kind) => {
                    Type::Divergent(divergent.materialized(*materialization_kind))
                }
                _ => self,
            },

            Type::Never
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::WrapperDescriptor(_)
            | Type::ModuleLiteral(_)
            | Type::KnownBoundMethod(
                KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
            )
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            // A non-generic class never needs to be specialized. A generic class is specialized
            // explicitly (via a subscript expression) or implicitly (via a call), and not because
            // some other generic context's specialization is applied to it.
            | Type::ClassLiteral(_)
            | Type::BoundSuper(_)
            | Type::SpecialForm(_) => self,
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
        self.find_legacy_typevars_impl(
            db,
            binding_context,
            typevars,
            &FindLegacyTypeVarsVisitor::default(),
        );
    }

    pub(crate) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        let matching_typevar = |bound_typevar: &BoundTypeVarInstance<'db>| {
            match bound_typevar.typevar(db).kind(db) {
                TypeVarKind::Legacy | TypeVarKind::Pep613Alias | TypeVarKind::TypingSelf
                    if binding_context.is_none_or(|binding_context| {
                        bound_typevar.binding_context(db)
                            == BindingContext::Definition(binding_context)
                    }) =>
                {
                    Some(*bound_typevar)
                }
                TypeVarKind::ParamSpec => {
                    // For `ParamSpec`, we're only interested in `P` itself, not `P.args` or
                    // `P.kwargs`.
                    Some(bound_typevar.without_paramspec_attr(db))
                }
                _ => None,
            }
        };

        match self {
            Type::TypeVar(bound_typevar) => {
                if let Some(bound_typevar) = matching_typevar(&bound_typevar) {
                    typevars.insert(bound_typevar);
                }
            }
            Type::Divergent(_) => {}

            Type::FunctionLiteral(function) => {
                visitor.visit(self, || {
                    function.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
                });
            }

            Type::BoundMethod(method) => {
                method.self_instance(db).find_legacy_typevars_impl(
                    db,
                    binding_context,
                    typevars,
                    visitor,
                );
                method.function(db).find_legacy_typevars_impl(
                    db,
                    binding_context,
                    typevars,
                    visitor,
                );
            }

            Type::KnownBoundMethod(
                KnownBoundMethodType::FunctionTypeDunderGet(function)
                | KnownBoundMethodType::FunctionTypeDunderCall(function),
            ) => {
                function.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }

            Type::KnownBoundMethod(
                KnownBoundMethodType::PropertyDunderGet(property)
                | KnownBoundMethodType::PropertyDunderSet(property)
                | KnownBoundMethodType::PropertyDunderDelete(property),
            ) => {
                property.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }

            Type::Callable(callable) => {
                callable.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }

            Type::PropertyInstance(property) => {
                property.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }

            Type::Union(union) => {
                for element in union.elements(db) {
                    element.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
                }
            }
            Type::Intersection(intersection) => {
                for positive in intersection.positive(db) {
                    positive.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
                }
                for negative in intersection.negative(db) {
                    negative.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
                }
            }

            Type::GenericAlias(alias) => {
                alias.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }

            Type::NominalInstance(instance) => {
                instance.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }

            Type::ProtocolInstance(instance) => {
                instance.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }

            Type::NewTypeInstance(_) => {
                // A newtype can never be constructed from an unspecialized generic class, so it is
                // impossible that we could ever find any legacy typevars in a newtype instance or
                // its underlying class.
            }

            Type::SubclassOf(subclass_of) => {
                subclass_of.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }

            Type::TypeIs(type_is) => {
                type_is.type_argument(db).find_legacy_typevars_impl(
                    db,
                    binding_context,
                    typevars,
                    visitor,
                );
            }

            Type::TypeGuard(type_guard) => {
                type_guard.return_type(db).find_legacy_typevars_impl(
                    db,
                    binding_context,
                    typevars,
                    visitor,
                );
            }

            Type::TypeAlias(alias) => {
                visitor.visit(self, || {
                    alias.value_type(db).find_legacy_typevars_impl(
                        db,
                        binding_context,
                        typevars,
                        visitor,
                    );
                });
            }

            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::UnionType(instance) => {
                    if let Ok(union_type) = instance.union_type(db) {
                        union_type.find_legacy_typevars_impl(
                            db,
                            binding_context,
                            typevars,
                            visitor,
                        );
                    }
                }
                KnownInstanceType::Annotated(ty) => {
                    ty.inner(db)
                        .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
                }
                KnownInstanceType::Callable(callable_type) => {
                    callable_type.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
                }
                KnownInstanceType::TypeGenericAlias(ty) => {
                    ty.inner(db)
                        .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
                }
                KnownInstanceType::SubscriptedProtocol(_)
                | KnownInstanceType::SubscriptedGeneric(_)
                | KnownInstanceType::TypeVar(_)
                | KnownInstanceType::TypeAliasType(_)
                | KnownInstanceType::Deprecated(_)
                | KnownInstanceType::Field(_)
                | KnownInstanceType::ConstraintSet(_)
                | KnownInstanceType::GenericContext(_)
                | KnownInstanceType::Specialization(_)
                | KnownInstanceType::Literal(_)
                | KnownInstanceType::LiteralStringAlias(_)
                | KnownInstanceType::NamedTupleSpec(_)
                | KnownInstanceType::NewType(_)
                | KnownInstanceType::FunctoolsPartial(_) => {
                    // TODO: For some of these, we may need to try to find legacy typevars in inner types.
                }
            },

            Type::Dynamic(DynamicType::UnknownGeneric(generic_context)) => {
                for variable in generic_context.variables(db) {
                    if let Some(variable) = matching_typevar(&variable) {
                        typevars.insert(variable);
                    }
                }
            }

            Type::Dynamic(_)
            | Type::Never
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(
                KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_),
            )
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::LiteralValue(_)
            | Type::BoundSuper(_)
            | Type::SpecialForm(_)
            | Type::TypedDict(_) => {}
        }
    }

    /// Bind all unbound legacy type variables to the given context and then
    /// add all legacy typevars to the provided set.
    pub(crate) fn bind_and_find_all_legacy_typevars(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        self.apply_type_mapping(
            db,
            &TypeMapping::BindLegacyTypevars(
                binding_context
                    .map(BindingContext::Definition)
                    .unwrap_or(BindingContext::Synthetic),
            ),
            TypeContext::default(),
        )
        .find_legacy_typevars(db, None, variables);
    }

    /// Replace default types in parameters of callables with `Unknown`.
    pub(crate) fn replace_parameter_defaults(self, db: &'db dyn Db) -> Type<'db> {
        self.apply_type_mapping(
            db,
            &TypeMapping::ReplaceParameterDefaults,
            TypeContext::default(),
        )
    }

    /// Returns the eagerly expanded type.
    /// In the case of recursive type aliases, this will diverge, so that part will be replaced with `Divergent`.
    fn expand_eagerly(self, db: &'db dyn Db) -> Type<'db> {
        self.expand_eagerly_(db, ())
    }

    #[allow(clippy::used_underscore_binding)]
    #[salsa::tracked(
        cycle_initial=|_, id, _, ()| Type::divergent(id),
        cycle_fn=|db, cycle, previous: &Type<'db>, value: Type<'db>, _, ()| {
            value.cycle_normalized(db, *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    fn expand_eagerly_(self, db: &'db dyn Db, _unit: ()) -> Type<'db> {
        self.apply_type_mapping(db, &TypeMapping::EagerExpansion, TypeContext::default())
    }

    /// Return the string representation of this type when converted to string as it would be
    /// provided by the `__str__` method.
    ///
    /// When not available, this should fall back to the value of `[Type::repr]`.
    /// Note: this method is used in the builtins `format`, `print`, `str.format` and `f-strings`.
    #[must_use]
    pub(crate) fn str(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Int(_) | LiteralValueTypeKind::Bool(_) => self.repr(db),
                LiteralValueTypeKind::String(_) | LiteralValueTypeKind::LiteralString => *self,
                LiteralValueTypeKind::Enum(enum_literal) => Type::string_literal(
                    db,
                    &format!(
                        "{enum_class}.{name}",
                        enum_class = enum_literal.enum_class(db).name(db),
                        name = enum_literal.name(db)
                    ),
                ),
                LiteralValueTypeKind::Bytes(_) => KnownClass::Str.to_instance(db),
            },
            Type::SpecialForm(special_form) => Type::string_literal(db, &special_form.to_string()),
            Type::KnownInstance(known_instance) => {
                Type::string_literal(db, &known_instance.repr(db).to_string())
            }
            ty if ty.is_subtype_of(db, Type::literal_string()) => Type::literal_string(),
            // TODO: handle more complex types
            _ => KnownClass::Str.to_instance(db),
        }
    }

    /// Return the string representation of this type as it would be provided by the  `__repr__`
    /// method at runtime.
    #[must_use]
    pub(crate) fn repr(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Int(number) => Type::string_literal(db, &number.to_string()),
                LiteralValueTypeKind::Bool(true) => Type::string_literal(db, "True"),
                LiteralValueTypeKind::Bool(false) => Type::string_literal(db, "False"),
                LiteralValueTypeKind::String(literal) => {
                    Type::string_literal(db, &format!("'{}'", literal.value(db).escape_default()))
                }
                LiteralValueTypeKind::LiteralString => Type::literal_string(),
                _ => KnownClass::Str.to_instance(db),
            },
            Type::SpecialForm(special_form) => Type::string_literal(db, &special_form.to_string()),
            Type::KnownInstance(known_instance) => {
                Type::string_literal(db, &known_instance.repr(db).to_string())
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
            Self::ClassLiteral(class_literal) => class_literal.type_definition(db),
            Self::GenericAlias(alias) => Some(TypeDefinition::StaticClass(alias.definition(db))),
            Self::NominalInstance(instance) => instance.class(db).type_definition(db),
            Self::KnownInstance(instance) => match instance {
                KnownInstanceType::TypeVar(var) => {
                    Some(TypeDefinition::TypeVar(var.definition(db)?))
                }
                KnownInstanceType::TypeAliasType(type_alias) => {
                    Some(TypeDefinition::TypeAlias(type_alias.definition(db)))
                }
                KnownInstanceType::NewType(newtype) => {
                    Some(TypeDefinition::NewType(newtype.definition(db)))
                }
                _ => None,
            },

            Self::SubclassOf(subclass_of_type) => match subclass_of_type.subclass_of() {
                SubclassOfInner::Dynamic(_) => None,
                SubclassOfInner::Class(class) => class.type_definition(db),
                SubclassOfInner::TypeVar(bound_typevar) => Some(TypeDefinition::TypeVar(
                    bound_typevar.typevar(db).definition(db)?,
                )),
            },

            Self::TypeAlias(alias) => alias.value_type(db).definition(db),
            Self::NewTypeInstance(newtype) => Some(TypeDefinition::NewType(newtype.definition(db))),

            Self::PropertyInstance(property) => property
                .getter(db)
                .and_then(|getter| getter.definition(db))
                .or_else(|| property.setter(db).and_then(|setter| setter.definition(db)))
                .or_else(|| {
                    property
                        .deleter(db)
                        .and_then(|deleter| deleter.definition(db))
                }),

            Self::LiteralValue(literal) => literal
                .as_enum()
                .and_then(|enum_lit| enum_lit.definition(db))
                .map(TypeDefinition::EnumMember)
                .or_else(|| self.to_meta_type(db).definition(db)),

            Self::KnownBoundMethod(_)
            | Self::WrapperDescriptor(_)
            | Self::DataclassDecorator(_)
            | Self::DataclassTransformer(_)
            | Self::BoundSuper(_) => self.to_meta_type(db).definition(db),

            Self::TypeVar(bound_typevar) => Some(TypeDefinition::TypeVar(
                bound_typevar.typevar(db).definition(db)?,
            )),

            Self::ProtocolInstance(protocol) => match protocol.inner {
                Protocol::FromClass(class) => class.type_definition(db),
                Protocol::Synthesized(_) => None,
            },

            Self::TypedDict(typed_dict) => typed_dict.type_definition(db),

            Self::Union(_) | Self::Intersection(_) => None,

            Self::SpecialForm(special_form) => special_form.definition(db),
            Self::Never => Type::SpecialForm(SpecialFormType::Never).definition(db),
            Self::Dynamic(DynamicType::Any) => {
                Type::SpecialForm(SpecialFormType::Any).definition(db)
            }
            Self::Dynamic(DynamicType::Unknown | DynamicType::UnknownGeneric(_)) => {
                Type::SpecialForm(SpecialFormType::Unknown).definition(db)
            }
            Self::AlwaysTruthy => Type::SpecialForm(SpecialFormType::AlwaysTruthy).definition(db),
            Self::AlwaysFalsy => Type::SpecialForm(SpecialFormType::AlwaysFalsy).definition(db),

            // These types have no definition
            Self::Divergent(_)
            | Self::Dynamic(
                DynamicType::Todo(_)
                | DynamicType::TodoUnpack
                | DynamicType::TodoStarredExpression
                | DynamicType::TodoTypeVarTuple
                | DynamicType::InvalidConcatenateUnknown
                | DynamicType::UnspecializedTypeVar,
            )
            | Self::Callable(_)
            | Self::TypeIs(_)
            | Self::TypeGuard(_) => None,
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
        match self {
            Type::FunctionLiteral(function) => Some(function.parameter_span(db, parameter_index)),
            Type::BoundMethod(bound_method) => Some(
                bound_method
                    .function(db)
                    .parameter_span(db, parameter_index),
            ),
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
        match self {
            Type::FunctionLiteral(function) => Some(function.spans(db)),
            Type::BoundMethod(bound_method) => Some(bound_method.function(db).spans(db)),
            _ => None,
        }
    }

    pub(crate) fn generic_origin(self, db: &'db dyn Db) -> Option<StaticClassLiteral<'db>> {
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

    /// Default-specialize all legacy typevars in this type.
    ///
    /// This is used when an implicit type alias is referenced without explicitly specializing it.
    pub(crate) fn default_specialize(self, db: &'db dyn Db) -> Type<'db> {
        let mut variables = FxOrderSet::default();
        self.find_legacy_typevars(db, None, &mut variables);
        let generic_context = GenericContext::from_typevar_instances(db, variables);
        self.apply_specialization(db, generic_context.default_specialization(db, None))
    }

    pub(crate) fn from_truthiness(db: &'db dyn Db, truthiness: Truthiness) -> Self {
        match truthiness {
            Truthiness::AlwaysTrue => Type::bool_literal(true),
            Truthiness::AlwaysFalse => Type::bool_literal(false),
            Truthiness::Ambiguous => KnownClass::Bool.to_instance(db),
        }
    }
}

impl<'db> IntersectionType<'db> {
    // Calls the dunder on each element separately and combines the results.
    // This avoids intersecting bound methods (which often collapses to Never)
    // and instead intersects the return types.
    //
    // TODO: we might be able to remove this after fixing
    // https://github.com/astral-sh/ty/issues/2428.
    fn try_call_dunder_with_policy(
        self,
        db: &'db dyn Db,
        name: &str,
        argument_types: &mut CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        // Using `positive()` rather than `positive_elements_or_object()` is safe
        // here because `object` does not define any of the dunders that are called
        // through this path without `MRO_NO_OBJECT_FALLBACK` (e.g. `__await__`,
        // `__iter__`, `__enter__`, `__bool__`).
        let positive = self.positive(db);
        let mut successful_bindings = Vec::with_capacity(positive.len());
        let mut last_error = None;

        for element in positive {
            match element.try_call_dunder_with_policy(db, name, argument_types, tcx, policy) {
                Ok(bindings) => successful_bindings.push(bindings),
                Err(err) => last_error = Some(err),
            }
        }

        if successful_bindings.is_empty() {
            // TODO we are only showing one of the errors here; should we aggregate
            // them somehow or show all of them?
            return Err(last_error.unwrap_or(CallDunderError::MethodNotAvailable));
        }

        Ok(Bindings::from_intersection(
            Type::Intersection(self),
            successful_bindings,
        ))
    }
}

impl<'db> UnionType<'db> {
    // Performs a lookup for the dunder on each union member separately, then
    // aggregates the results.
    //
    // This alternative to aggregating the dunder lookups with
    // `UnionType.map_with_boundness_and_qualifiers` preserves the information
    // necessary to emit more precise diagnostics for "possibly unbound" errors.
    fn try_call_dunder_with_policy(
        self,
        db: &'db dyn Db,
        name: &str,
        argument_types: &mut CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        let elements = self.elements(db);
        let mut builder = UnionBuilder::new(db);
        let mut unbound_on: Vec<Type<'db>> = Vec::new();
        let mut any_defined = false;
        let mut possibly_undefined = false;

        for element in elements {
            match element
                .member_lookup_with_policy(
                    db,
                    name.into(),
                    policy | MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place
            {
                Place::Defined(DefinedPlace {
                    ty,
                    definedness: Definedness::PossiblyUndefined,
                    ..
                }) => {
                    builder = builder.add(ty);
                    any_defined = true;
                    possibly_undefined = true;
                }
                Place::Defined(DefinedPlace { ty, .. }) => {
                    builder = builder.add(ty);
                    any_defined = true;
                }
                Place::Undefined => {
                    unbound_on.push(*element);
                    possibly_undefined = true;
                }
            }
        }

        if !any_defined {
            return Err(CallDunderError::MethodNotAvailable);
        }

        let dunder_callable = builder.build();
        let constraints = ConstraintSetBuilder::new();
        let bindings = dunder_callable
            .bindings(db)
            .match_parameters(db, argument_types)
            .check_types(db, &constraints, argument_types, tcx, &[])?;

        if possibly_undefined {
            return Err(CallDunderError::PossiblyUnbound {
                bindings: Box::new(bindings),
                unbound_on: (!unbound_on.is_empty()).then(|| unbound_on.into_boxed_slice()),
            });
        }

        Ok(bindings)
    }
}

impl<'db> From<&Type<'db>> for Type<'db> {
    fn from(value: &Type<'db>) -> Self {
        *value
    }
}

impl<'db> VarianceInferable<'db> for Type<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        tracing::trace!(
            "Checking variance of '{tvar}' in `{ty:?}`",
            tvar = typevar.typevar(db).name(db),
            ty = self.display(db),
        );

        let v = match self {
            Type::ClassLiteral(class_literal) => class_literal.variance_of(db, typevar),

            Type::FunctionLiteral(function_type) => {
                // TODO: do we need to replace self?
                function_type.signature(db).variance_of(db, typevar)
            }

            Type::BoundMethod(method_type) => {
                // TODO: do we need to replace self?
                method_type
                    .function(db)
                    .signature(db)
                    .variance_of(db, typevar)
            }

            Type::NominalInstance(nominal_instance_type) => {
                nominal_instance_type.variance_of(db, typevar)
            }
            Type::GenericAlias(generic_alias) => generic_alias.variance_of(db, typevar),
            Type::Callable(callable_type) => callable_type.signatures(db).variance_of(db, typevar),
            // A type variable is always covariant in itself.
            Type::TypeVar(other_typevar) if other_typevar == typevar => {
                // type variables are covariant in themselves
                TypeVarVariance::Covariant
            }
            Type::ProtocolInstance(protocol_instance_type) => {
                protocol_instance_type.variance_of(db, typevar)
            }
            // unions are covariant in their disjuncts
            Type::Union(union_type) => union_type
                .elements(db)
                .iter()
                .map(|ty| ty.variance_of(db, typevar))
                .collect(),

            // Products are covariant in their conjuncts. For negative
            // conjuncts, they're contravariant. To see this, suppose we have
            // `B` a subtype of `A`. A value of type `~B` could be some non-`B`
            // `A`, and so is not assignable to `~A`. On the other hand, a value
            // of type `~A` excludes all `A`s, and thus all `B`s, and so _is_
            // assignable to `~B`.
            Type::Intersection(intersection_type) => intersection_type
                .positive(db)
                .iter()
                .map(|ty| ty.variance_of(db, typevar))
                .chain(intersection_type.negative(db).iter().map(|ty| {
                    ty.with_polarity(TypeVarVariance::Contravariant)
                        .variance_of(db, typevar)
                }))
                .collect(),
            Type::PropertyInstance(property_instance_type) => property_instance_type
                .getter(db)
                .iter()
                .chain(&property_instance_type.setter(db))
                .chain(&property_instance_type.deleter(db))
                .map(|ty| ty.variance_of(db, typevar))
                .collect(),
            Type::SubclassOf(subclass_of_type) => subclass_of_type.variance_of(db, typevar),
            Type::TypeIs(type_is_type) => type_is_type.variance_of(db, typevar),
            Type::TypeGuard(type_guard_type) => type_guard_type.variance_of(db, typevar),
            Type::KnownInstance(known_instance) => known_instance.variance_of(db, typevar),
            Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::Never
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::LiteralValue(_)
            | Type::SpecialForm(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::BoundSuper(_)
            | Type::TypeVar(_)
            | Type::TypedDict(_)
            | Type::TypeAlias(_)
            | Type::NewTypeInstance(_) => TypeVarVariance::Bivariant,
        };

        tracing::trace!(
            "Result of variance of '{tvar}' in `{ty:?}` is `{v:?}`",
            tvar = typevar.typevar(db).name(db),
            ty = self.display(db),
        );
        v
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub enum PromotionMode {
    On,
    Off,
}

impl PromotionMode {
    const fn flip(self) -> Self {
        match self {
            PromotionMode::On => PromotionMode::Off,
            PromotionMode::Off => PromotionMode::On,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, get_size2::GetSize)]
pub enum PromotionKind {
    /// Default promotion behaviour: recurse into nested types
    Regular,
    /// Singleton-only promotion recursively descends through nominal instances
    /// without recursing into unions or non-nominal types.
    SingletonsOnly,
}

/// Returns the [`ClassLiteral`] that "owns" a `Self` typevar (i.e., the class from its upper bound).
fn self_typevar_owner_class_literal<'db>(
    db: &'db dyn Db,
    bound_typevar: BoundTypeVarInstance<'db>,
) -> Option<ClassLiteral<'db>> {
    bound_typevar
        .typevar(db)
        .upper_bound(db)
        .and_then(|ty| ty.nominal_class(db))
        .map(|class| class.class_literal(db))
}

#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn class_mro_literals<'db>(
    db: &'db dyn Db,
    class_literal: ClassLiteral<'db>,
) -> Vec<ClassLiteral<'db>> {
    class_literal
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .map(|class| class.class_literal(db))
        .collect()
}

/// Information needed to bind `Self` typevars to a concrete type.
///
/// Uses MRO-based matching: a `Self` typevar is bound only if its owner class
/// is in the MRO of the self type's class.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct SelfBinding<'db> {
    ty: Type<'db>,
    class_literal: Option<ClassLiteral<'db>>,
    binding_context: Option<BindingContext<'db>>,
}

impl<'db> SelfBinding<'db> {
    pub(crate) fn self_type(&self) -> Type<'db> {
        self.ty
    }

    pub(crate) fn binding_context(&self) -> Option<BindingContext<'db>> {
        self.binding_context
    }
}

impl<'db> SelfBinding<'db> {
    pub(crate) fn new(
        db: &'db dyn Db,
        self_type: Type<'db>,
        binding_context: Option<BindingContext<'db>>,
    ) -> Self {
        let class_literal = match self_type {
            Type::TypeVar(typevar) if typevar.typevar(db).is_self(db) => {
                self_typevar_owner_class_literal(db, typevar)
            }
            _ => self_type
                .nominal_class(db)
                .map(|class| class.class_literal(db)),
        };

        Self {
            ty: self_type,
            class_literal,
            binding_context,
        }
    }

    /// Returns whether `bound_typevar` should be replaced by this binding's concrete self type.
    fn should_bind(&self, db: &'db dyn Db, bound_typevar: BoundTypeVarInstance<'db>) -> bool {
        if !bound_typevar.typevar(db).is_self(db) {
            return false;
        }

        // Fast path for the common method-signature case where the bound `Self`
        // carries the same binding context as this mapping.
        if self.binding_context == Some(bound_typevar.binding_context(db)) {
            return true;
        }

        // Check that the Self typevar's owner class is in the MRO of the self type's class.
        // If we can't determine either class, conservatively don't bind.
        self.class_literal.is_some_and(|class_literal| {
            let class_mro = class_mro_literals(db, class_literal);
            self_typevar_owner_class_literal(db, bound_typevar)
                .is_none_or(|owner_class| class_mro.contains(&owner_class))
        })
    }
}

/// A mapping that can be applied to a type, producing another type. This is applied inductively to
/// the components of complex types.
///
/// This is represented as an enum (with some variants using `Cow`), and not an `FnMut` trait,
/// since we sometimes have to apply type mappings lazily (e.g., to the signature of a function
/// literal).
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub enum TypeMapping<'a, 'db> {
    /// Applies a specialization to the type
    ApplySpecialization(ApplySpecialization<'a, 'db>),
    /// Applies a specialization and materializes only substituted typevars.
    ///
    /// The `materialization_kind` is flipped in contravariant positions.
    ApplySpecializationWithMaterialization {
        specialization: ApplySpecialization<'a, 'db>,
        materialization_kind: MaterializationKind,
    },
    /// Replaces any literal types with their corresponding promoted type form (e.g. `Literal["string"]`
    /// to `str`, or `def _() -> int` to `Callable[[], int]`).
    Promote(PromotionMode, PromotionKind),
    /// Binds a legacy typevar with the generic context (class, function, type alias) that it is
    /// being used in.
    BindLegacyTypevars(BindingContext<'db>),
    /// Binds any `typing.Self` typevar with a particular `self` class.
    BindSelf(SelfBinding<'db>),
    /// Replaces occurrences of `typing.Self` with a new `Self` type variable with the given upper bound.
    ReplaceSelf { new_upper_bound: Type<'db> },
    /// Create the top or bottom materialization of a type.
    Materialize(MaterializationKind),
    /// Replace default types in parameters of callables with `Unknown`. This is used to avoid infinite
    /// recursion when the type of the default value of a parameter depends on the callable itself.
    ReplaceParameterDefaults,
    /// Apply eager expansion to the type.
    /// In the case of recursive type aliases, this will diverge, so that part will be replaced with `Divergent`.
    EagerExpansion,

    /// Updates any `Callable` types in a function signature return type to be generic if possible.
    RescopeReturnCallables(&'a FxHashMap<CallableType<'db>, CallableType<'db>>),
}

impl<'db> TypeMapping<'_, 'db> {
    /// Update the generic context of a [`Signature`] according to the current type mapping
    pub(crate) fn update_signature_generic_context(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> GenericContext<'db> {
        match self {
            TypeMapping::ApplySpecialization(specialization)
            | TypeMapping::ApplySpecializationWithMaterialization { specialization, .. } => {
                // Filter out type variables that are already specialized
                // (i.e., mapped to a non-TypeVar type)
                GenericContext::from_typevar_instances(
                    db,
                    context.variables(db).filter(|bound_typevar| {
                        // Keep the type variable if it's not in the specialization
                        // or if it's mapped to itself (still a TypeVar)
                        match specialization.get(db, *bound_typevar) {
                            None => true,
                            Some(Type::TypeVar(mapped_typevar)) => {
                                // Still a TypeVar, keep it if it's mapping to itself
                                mapped_typevar.identity(db) == bound_typevar.identity(db)
                            }
                            Some(_) => false, // Specialized to a concrete type, filter out
                        }
                    }),
                )
            }
            TypeMapping::Promote(..)
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::Materialize(_)
            | TypeMapping::ReplaceParameterDefaults
            | TypeMapping::EagerExpansion
            | TypeMapping::RescopeReturnCallables(_) => context,
            TypeMapping::BindSelf(binding) => {
                if binding.binding_context().is_some() {
                    context.remove_self(db, binding.binding_context())
                } else {
                    context
                }
            }
            TypeMapping::ReplaceSelf { new_upper_bound } => GenericContext::from_typevar_instances(
                db,
                context.variables(db).map(|typevar| {
                    if typevar.typevar(db).is_self(db) {
                        BoundTypeVarInstance::synthetic_self(
                            db,
                            *new_upper_bound,
                            typevar.binding_context(db),
                        )
                    } else {
                        typevar
                    }
                }),
            ),
        }
    }

    /// Returns a new `TypeMapping` that should be applied in contravariant positions.
    pub(crate) fn flip(&self) -> Self {
        match self {
            TypeMapping::Materialize(materialization_kind) => {
                TypeMapping::Materialize(materialization_kind.flip())
            }
            TypeMapping::ApplySpecializationWithMaterialization {
                specialization,
                materialization_kind,
            } => TypeMapping::ApplySpecializationWithMaterialization {
                specialization: *specialization,
                materialization_kind: materialization_kind.flip(),
            },
            TypeMapping::Promote(mode, kind) => TypeMapping::Promote(mode.flip(), *kind),
            TypeMapping::ApplySpecialization(_)
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::BindSelf(..)
            | TypeMapping::ReplaceSelf { .. }
            | TypeMapping::ReplaceParameterDefaults
            | TypeMapping::EagerExpansion
            | TypeMapping::RescopeReturnCallables(_) => self.clone(),
        }
    }
}

/// A type that is determined to be divergent during recursive type inference.
/// This type must never be eliminated by dynamic type reduction
/// (e.g. `Divergent` is assignable to `@Todo`, but `@Todo | Divergent` must not be reducted to `@Todo`).
/// Otherwise, type inference cannot converge properly.
/// For detailed properties of this type, see the unit test at the end of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub struct DivergentType {
    /// The query ID that caused the cycle.
    id: salsa::Id,
    /// If this divergent marker has been materialized, preserve whether it should behave like the
    /// top (`object`) or bottom (`Never`) bound while still remaining recognizable as divergent.
    materialization: Option<MaterializationKind>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for DivergentType {}

impl DivergentType {
    const fn new(id: salsa::Id) -> Self {
        Self {
            id,
            materialization: None,
        }
    }

    fn same_marker(self, other: Self) -> bool {
        self.id == other.id
    }

    const fn materialized(self, kind: MaterializationKind) -> Self {
        Self {
            id: self.id,
            materialization: Some(kind),
        }
    }

    const fn materialization_kind(self) -> Option<MaterializationKind> {
        self.materialization
    }
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub enum DynamicType<'db> {
    /// An explicitly annotated `typing.Any`
    Any,
    /// An unannotated value, or a dynamic type resulting from an error
    Unknown,
    /// Similar to `Unknown`, this represents a dynamic type that has been explicitly specialized
    /// with legacy typevars, e.g. `UnknownClass[T]`, where `T` is a legacy typevar. We keep track
    /// of the type variables in the generic context in case this type is later specialized again.
    ///
    /// TODO: Once we implement <https://github.com/astral-sh/ty/issues/1711>, this variant might
    /// not be needed anymore.
    UnknownGeneric(GenericContext<'db>),
    /// An unspecialized type variable during generic call inference.
    ///
    /// TODO: This variant should be removed once type variables are unified across nested generic
    /// calls. For now, we replace unspecialized type variables with this marker type, and ignore them
    /// during generic inference.
    UnspecializedTypeVar,
    /// A special variant that represents that `Unknown` was inferred due to an invalid use of
    /// `Concatenate` in a type expression.
    ///
    /// TODO: this is a bit of a hack. `infer_type_expression` should really return a `Result`;
    /// if it did, this variant wouldn't be necessary.
    InvalidConcatenateUnknown,
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
    /// A special Todo-variant for `Unpack[Ts]`, so that we can treat it specially in `Generic[Unpack[Ts]]`
    TodoUnpack,
    /// A special Todo-variant for `*Ts`, so that we can treat it specially in `Generic[*Ts]`
    TodoStarredExpression,
    /// A special Todo-variant for `TypeVarTuple` instances encountered in type expressions
    TodoTypeVarTuple,
}

impl DynamicType<'_> {
    fn recursive_type_normalized(self) -> Self {
        self
    }

    pub(crate) fn is_todo(&self) -> bool {
        matches!(self, Self::Todo(_) | Self::TodoUnpack)
    }
}

impl std::fmt::Display for DynamicType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DynamicType::Any => f.write_str("Any"),
            DynamicType::Unknown
            | DynamicType::UnknownGeneric(_)
            | DynamicType::InvalidConcatenateUnknown => f.write_str("Unknown"),
            DynamicType::UnspecializedTypeVar => f.write_str("UnspecializedTypeVar"),
            // `DynamicType::Todo`'s display should be explicit that is not a valid display of
            // any other type
            DynamicType::Todo(todo) => write!(f, "@Todo{todo}"),
            DynamicType::TodoUnpack => f.write_str("@Todo(typing.Unpack)"),
            DynamicType::TodoStarredExpression => f.write_str("@Todo(StarredExpression)"),
            DynamicType::TodoTypeVarTuple => f.write_str("@Todo(TypeVarTuple)"),
        }
    }
}

bitflags! {
    /// Type qualifiers that appear in an annotation expression.
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Default, salsa::Update, Hash)]
    pub struct TypeQualifiers: u8 {
        /// `typing.ClassVar`
        const CLASS_VAR = 1 << 0;
        /// `typing.Final`
        const FINAL     = 1 << 1;
        /// `dataclasses.InitVar`
        const INIT_VAR  = 1 << 2;
        /// `typing_extensions.Required`
        const REQUIRED = 1 << 3;
        /// `typing_extensions.NotRequired`
        const NOT_REQUIRED = 1 << 4;
        /// `typing_extensions.ReadOnly`
        const READ_ONLY = 1 << 5;
        /// A non-standard type qualifier that marks implicit instance attributes, i.e.
        /// instance attributes that are only implicitly defined via `self.x = …` in
        /// the body of a class method.
        const IMPLICIT_INSTANCE_ATTRIBUTE = 1 << 6;
        /// A non-standard type qualifier that marks a type returned from a module-level
        /// `__getattr__` function. We need this in order to implement precedence of submodules
        /// over module-level `__getattr__`, for compatibility with other type checkers.
        const FROM_MODULE_GETATTR = 1 << 7;
    }
}

impl get_size2::GetSize for TypeQualifiers {}

impl TypeQualifiers {
    /// Get the name of a type qualifier.
    ///
    /// Note that this function can only be called on sets with a single member.
    /// Panics if more than a single bit is set.
    pub fn name(self) -> &'static str {
        match self {
            Self::CLASS_VAR => "ClassVar",
            Self::FINAL => "Final",
            Self::INIT_VAR => "InitVar",
            Self::REQUIRED => "Required",
            Self::NOT_REQUIRED => "NotRequired",
            Self::READ_ONLY => "ReadOnly",
            _ => {
                unreachable!(
                    "Only a single bit should be set when calling `TypeQualifiers::name` (got {self:?})"
                )
            }
        }
    }

    /// Returns `true` if this is a non-standard qualifier.
    ///
    /// Non-standard qualifiers are internal implementation details like
    /// `IMPLICIT_INSTANCE_ATTRIBUTE` and `FROM_MODULE_GETATTR`.
    pub fn is_non_standard(self) -> bool {
        const NON_STANDARD: TypeQualifiers =
            TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE.union(TypeQualifiers::FROM_MODULE_GETATTR);
        self.intersects(NON_STANDARD)
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
    origin: TypeOrigin,
    qualifiers: TypeQualifiers,
}

impl<'db> TypeAndQualifiers<'db> {
    pub(crate) fn new(inner: Type<'db>, origin: TypeOrigin, qualifiers: TypeQualifiers) -> Self {
        Self {
            inner,
            origin,
            qualifiers,
        }
    }

    pub(crate) fn declared(inner: Type<'db>) -> Self {
        Self {
            inner,
            origin: TypeOrigin::Declared,
            qualifiers: TypeQualifiers::empty(),
        }
    }

    /// Forget about type qualifiers and only return the inner type.
    pub(crate) fn inner_type(&self) -> Type<'db> {
        self.inner
    }

    pub(crate) fn origin(&self) -> TypeOrigin {
        self.origin
    }

    /// Return `self` with an additional qualifier added to the set of qualifiers.
    pub(crate) fn with_qualifier(mut self, qualifier: TypeQualifiers) -> Self {
        self.qualifiers |= qualifier;
        self
    }

    /// Return the set of type qualifiers.
    pub(crate) fn qualifiers(&self) -> TypeQualifiers {
        self.qualifiers
    }

    pub(crate) fn map_type(
        &self,
        f: impl FnOnce(Type<'db>) -> Type<'db>,
    ) -> TypeAndQualifiers<'db> {
        TypeAndQualifiers {
            inner: f(self.inner),
            origin: self.origin,
            qualifiers: self.qualifiers,
        }
    }
}

/// Error struct providing information on type(s) that were deemed to be invalid
/// in a type expression context, and the type we should therefore fallback to
/// for the problematic type expression.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::Update)]
pub struct InvalidTypeExpressionError<'db> {
    fallback_type: Type<'db>,
    invalid_expressions: smallvec::SmallVec<[InvalidTypeExpression<'db>; 1]>,
}

impl<'db> InvalidTypeExpressionError<'db> {
    fn into_fallback_type(
        self,
        context: &InferContext,
        node: &impl Ranged,
        flags: InferenceFlags,
    ) -> Type<'db> {
        let InvalidTypeExpressionError {
            fallback_type,
            invalid_expressions,
        } = self;
        for error in invalid_expressions {
            let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, node) else {
                continue;
            };
            let diagnostic = builder.into_diagnostic(error.reason(context.db(), flags));
            error.add_subdiagnostics(context.db(), diagnostic, node);
        }
        fallback_type
    }
}

/// Enumeration of various types that are invalid in type-expression contexts
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::Update)]
enum InvalidTypeExpression<'db> {
    /// Some types always require exactly one argument when used in a type expression
    RequiresOneArgument(SpecialFormType),
    /// Some types always require at least one argument when used in a type expression
    RequiresArguments(SpecialFormType),
    /// Some types always require at least two arguments when used in a type expression
    RequiresTwoArguments(SpecialFormType),
    /// The `Protocol` class is invalid in type expressions
    Protocol,
    /// Same for `Generic`
    Generic,
    /// Same for `@deprecated`
    Deprecated,
    /// Same for `dataclasses.Field`
    Field,
    /// Same for `ty_extensions.ConstraintSet`
    ConstraintSet,
    /// Same for `ty_extensions.GenericContext`
    GenericContext,
    /// Same for `ty_extensions.Specialization`
    Specialization,
    /// Same for `NamedTupleSpec`
    NamedTupleSpec,
    /// Same for `typing.TypedDict`
    TypedDict,
    /// Same for `typing.TypeAlias`, anywhere except for as the sole annotation on an annotated
    /// assignment
    TypeAlias,
    /// Same for `typing.Concatenate`, anywhere except for as the first parameter of a `Callable`
    /// type expression
    Concatenate,
    /// Type qualifiers are always invalid in type expressions
    TypeQualifier(TypeQualifier),
    /// `typing.Self` cannot be used in `@staticmethod` definitions.
    TypingSelfInStaticMethod,
    /// `typing.Self` cannot be used in metaclass definitions.
    TypingSelfInMetaclass,
    /// Some types are always invalid in type expressions
    InvalidType(Type<'db>, ScopeId<'db>),
    InvalidBareParamSpec(TypeVarInstance<'db>),
}

impl<'db> InvalidTypeExpression<'db> {
    const fn reason(self, db: &'db dyn Db, flags: InferenceFlags) -> impl std::fmt::Display + 'db {
        struct Display<'db> {
            error: InvalidTypeExpression<'db>,
            db: &'db dyn Db,
            flags: InferenceFlags,
        }

        impl std::fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let location = self.flags.type_expression_context();

                match self.error {
                    InvalidTypeExpression::RequiresOneArgument(special_form) => write!(
                        f,
                        "`{special_form}` requires exactly one argument when used in a {location}",
                    ),
                    InvalidTypeExpression::RequiresArguments(special_form) => write!(
                        f,
                        "`{special_form}` requires at least one argument when used in a {location}",
                    ),
                    InvalidTypeExpression::RequiresTwoArguments(special_form) => write!(
                        f,
                        "`{special_form}` requires at least two arguments when used in a {location}",
                    ),
                    InvalidTypeExpression::Protocol => {
                        write!(f, "`typing.Protocol` is not allowed in {location}s")
                    }
                    InvalidTypeExpression::Generic => {
                        write!(f, "`typing.Generic` is not allowed in {location}s")
                    }
                    InvalidTypeExpression::Deprecated => {
                        write!(f, "`warnings.deprecated` is not allowed in {location}s")
                    }
                    InvalidTypeExpression::Field => {
                        write!(f, "`dataclasses.Field` is not allowed in {location}s")
                    }
                    InvalidTypeExpression::ConstraintSet => write!(
                        f,
                        "`ty_extensions.ConstraintSet` is not allowed in {location}s",
                    ),
                    InvalidTypeExpression::GenericContext => {
                        write!(
                            f,
                            "`ty_extensions.GenericContext` is not allowed in {location}s"
                        )
                    }
                    InvalidTypeExpression::Specialization => write!(
                        f,
                        "`ty_extensions.GenericContext` is not allowed in {location}s",
                    ),
                    InvalidTypeExpression::NamedTupleSpec => {
                        write!(f, "`NamedTupleSpec` is not allowed in {location}s")
                    }
                    InvalidTypeExpression::TypedDict => write!(
                        f,
                        "The special form `typing.TypedDict` \
                            is not allowed in {location}s",
                    ),
                    InvalidTypeExpression::TypeAlias => f.write_str(
                        "`typing.TypeAlias` is only allowed \
                            as the sole annotation on an annotated assignment",
                    ),
                    InvalidTypeExpression::TypeQualifier(qualifier) => {
                        if self.flags.intersects(
                            InferenceFlags::IN_PARAMETER_ANNOTATION
                                | InferenceFlags::IN_RETURN_TYPE
                                | InferenceFlags::IN_TYPE_ALIAS,
                        ) {
                            write!(
                                f,
                                "Type qualifier `{qualifier}` is not allowed in {location}s",
                            )
                        } else if qualifier.requires_one_argument() {
                            write!(
                                f,
                                "Type qualifier `{qualifier}` is not allowed in type expressions \
                                (only in annotation expressions, and only with \
                                exactly one argument)",
                            )
                        } else {
                            write!(
                                f,
                                "Type qualifier `{qualifier}` is not allowed in type expressions \
                                (only in annotation expressions)"
                            )
                        }
                    }
                    InvalidTypeExpression::TypingSelfInStaticMethod => {
                        f.write_str("`Self` cannot be used in a static method")
                    }
                    InvalidTypeExpression::TypingSelfInMetaclass => {
                        f.write_str("`Self` cannot be used in a metaclass")
                    }
                    InvalidTypeExpression::InvalidType(Type::FunctionLiteral(function), _) => {
                        write!(
                            f,
                            "Function `{function}` is not valid in a {location}",
                            function = function.name(self.db)
                        )
                    }
                    InvalidTypeExpression::InvalidType(Type::ModuleLiteral(module), _) => write!(
                        f,
                        "Module `{module}` is not valid in a {location}",
                        module = module.module(self.db).name(self.db)
                    ),
                    InvalidTypeExpression::InvalidType(ty, _) => write!(
                        f,
                        "Variable of type `{ty}` is not allowed in a {location}",
                        ty = ty.display(self.db)
                    ),
                    InvalidTypeExpression::InvalidBareParamSpec(paramspec) => write!(
                        f,
                        "Bare ParamSpec `{}` is not valid in this context in a {location}",
                        paramspec.name(self.db)
                    ),
                    InvalidTypeExpression::Concatenate => write!(
                        f,
                        "`typing.Concatenate` is not allowed in this context in a {location}",
                    ),
                }
            }
        }

        Display {
            error: self,
            db,
            flags,
        }
    }

    fn add_subdiagnostics(
        self,
        db: &'db dyn Db,
        mut diagnostic: LintDiagnosticGuard,
        node: &impl Ranged,
    ) {
        if let InvalidTypeExpression::InvalidType(Type::Never, _) = self {
            diagnostic.help(
                "The variable may have been inferred as `Never` because \
                its definition was inferred as being unreachable",
            );
        } else if let InvalidTypeExpression::InvalidType(ty @ Type::ModuleLiteral(module), scope) =
            self
        {
            let module = module.module(db);
            let Some(module_name_final_part) = module.name(db).components().next_back() else {
                return;
            };
            let Some(module_member_with_same_name) = ty
                .member(db, module_name_final_part)
                .place
                .ignore_possibly_undefined()
            else {
                return;
            };
            if module_member_with_same_name
                .in_type_expression(db, scope, None, InferenceFlags::empty())
                .is_err()
            {
                return;
            }
            diagnostic.set_primary_message(format_args!(
                "Did you mean to use the module's member \
                `{module_name_final_part}.{module_name_final_part}`?"
            ));
            diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
                format!(".{module_name_final_part}"),
                node.end(),
            )));
        } else if let InvalidTypeExpression::TypedDict = self {
            diagnostic.help(
                "You might have meant to use a concrete TypedDict \
                or `collections.abc.Mapping[str, object]`",
            );
        // It would be nice if we could register `builtins.callable` as a known function,
        // but currently doing this would require reimplementing the signature "manually"
        // in `Type::bindings()`, which isn't worth it given that we have no other special
        // casing for this function.
        } else if let InvalidTypeExpression::InvalidType(Type::FunctionLiteral(function), _) = self
            && function.name(db) == "callable"
            && let function_body_scope = function.literal(db).last_definition.body_scope(db)
            && function_body_scope
                .scope(db)
                .parent()
                .map(|parent| parent.to_scope_id(db, function_body_scope.file(db)))
                == builtins_module_scope(db)
        {
            diagnostic.set_primary_message("Did you mean `collections.abc.Callable`?");
        } else if matches!(self, InvalidTypeExpression::InvalidBareParamSpec(_)) {
            diagnostic.info("A bare ParamSpec is only valid:");
            diagnostic.info(" - as the first argument to `Callable`");
            diagnostic.info(" - as the last argument to `Concatenate`");
            diagnostic.info(" - as the default type for another ParamSpec");
            diagnostic.info(" - as part of a type parameter list when defining a generic class");
            diagnostic.info(" - or as part of an argument list when specializing a generic class");
        } else if matches!(self, InvalidTypeExpression::Concatenate) {
            diagnostic.info("`typing.Concatenate` is only valid:");
            diagnostic.info(" - as the first argument to `typing.Callable`");
            diagnostic.info(" - as a type argument for a `ParamSpec` parameter");
        }
    }
}

/// Whether a given type originates from value expression inference or type expression inference.
/// For example, the symbol `int` would be inferred as `<class 'int'>` in value expression context,
/// and as `int` (i.e. an instance of the class `int`) in type expression context.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::Update)]
pub enum InferredAs {
    ValueExpression,
    TypeExpression,
}

impl InferredAs {
    pub const fn type_expression(self) -> bool {
        matches!(self, InferredAs::TypeExpression)
    }
}

/// Error returned if a type is not awaitable.
#[derive(Debug)]
enum AwaitError<'db> {
    /// `__await__` is either missing, potentially unbound or cannot be called with provided
    /// arguments.
    Call(CallDunderError<'db>),
    /// `__await__` resolved successfully, but its return type is known not to be a generator.
    InvalidReturnType(Type<'db>, Box<Bindings<'db>>),
}

impl<'db> AwaitError<'db> {
    fn report_diagnostic(
        &self,
        context: &InferContext<'db, '_>,
        context_expression_type: Type<'db>,
        context_expression_node: ast::AnyNodeRef,
    ) {
        let Some(builder) = context.report_lint(&INVALID_AWAIT, context_expression_node) else {
            return;
        };

        let db = context.db();

        let mut diag = builder.into_diagnostic(
            format_args!("`{type}` is not awaitable", type = context_expression_type.display(db)),
        );
        match self {
            Self::Call(CallDunderError::CallError(CallErrorKind::BindingError, bindings)) => {
                diag.info("`__await__` requires arguments and cannot be called implicitly");
                if let Some(definition_spans) = bindings.callable_type().function_spans(db) {
                    diag.annotate(
                        Annotation::secondary(definition_spans.parameters)
                            .message("parameters here"),
                    );
                }
            }
            Self::Call(CallDunderError::CallError(
                kind @ (CallErrorKind::NotCallable | CallErrorKind::PossiblyNotCallable),
                bindings,
            )) => {
                let possibly = if matches!(kind, CallErrorKind::PossiblyNotCallable) {
                    " possibly"
                } else {
                    ""
                };
                diag.info(format_args!("`__await__` is{possibly} not callable"));
                if let Some(definition) = bindings.callable_type().definition(db)
                    && let Some(definition_range) = definition.focus_range(db)
                {
                    diag.annotate(
                        Annotation::secondary(definition_range.into())
                            .message("attribute defined here"),
                    );
                }
            }
            Self::Call(CallDunderError::PossiblyUnbound {
                bindings,
                unbound_on,
            }) => {
                diag.info("`__await__` may be missing");
                if let Some(unbound_on) = unbound_on {
                    for ty in unbound_on {
                        diag.info(format_args!(
                            "`{}` does not implement `__await__`",
                            ty.display(db)
                        ));
                    }
                }
                if let Some(definition_spans) = bindings.callable_type().function_spans(db) {
                    diag.annotate(
                        Annotation::secondary(definition_spans.signature)
                            .message("method defined here"),
                    );
                }
            }
            Self::Call(CallDunderError::MethodNotAvailable) => {
                diag.info("`__await__` is missing");
                if let Some(type_definition) = context_expression_type.definition(db)
                    && let Some(definition_range) = type_definition.focus_range(db)
                {
                    diag.annotate(
                        Annotation::secondary(definition_range.into()).message("type defined here"),
                    );
                }
            }
            Self::InvalidReturnType(return_type, bindings) => {
                diag.info(format_args!(
                    "`__await__` returns `{return_type}`, which is not a valid iterator",
                    return_type = return_type.display(db)
                ));
                if let Some(definition_spans) = bindings.callable_type().function_spans(db) {
                    diag.annotate(
                        Annotation::secondary(definition_spans.signature)
                            .message("method defined here"),
                    );
                }
            }
        }
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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

    /// Get the submodule attributes we believe to be defined on this module.
    ///
    /// Note that `ModuleLiteralType` is per-importing-file, so this analysis
    /// includes "imports the importing file has performed".
    ///
    ///
    /// # Danger! Powerful Hammer!
    ///
    /// These results immediately make the attribute always defined in the importing file,
    /// shadowing any other attribute in the module with the same name, even if the
    /// non-submodule-attribute is in fact always the one defined in practice.
    ///
    /// Intuitively this means `available_submodule_attributes` "win all tie-breaks",
    /// with the idea that if we're ever confused about complicated code then usually
    /// the import is the thing people want in scope.
    ///
    /// However this "always defined, always shadows" rule if applied too aggressively
    /// creates VERY confusing conclusions that break perfectly reasonable code.
    ///
    /// For instance, consider a package which has a `myfunc` submodule which defines a
    /// `myfunc` function (a common idiom). If the package "re-exports" this function
    /// (`from .myfunc import myfunc`), then at runtime in python
    /// `from mypackage import myfunc` should import the function and not the submodule.
    ///
    /// However, if we were to consider `from mypackage import myfunc` as introducing
    /// the attribute `mypackage.myfunc` in `available_submodule_attributes`, we would
    /// fail to ever resolve the function. This is because `available_submodule_attributes`
    /// is *so early* and *so powerful* in our analysis that **this conclusion would be
    /// used when actually resolving `from mypackage import myfunc`**!
    ///
    /// This currently cannot be fixed by considering the actual symbols defined in `mypackage`,
    /// because `available_submodule_attributes` is an *input* to that analysis.
    ///
    /// We should therefore avoid marking something as an `available_submodule_attribute`
    /// when the import could be importing a non-submodule (a function, class, or value).
    ///
    ///
    /// # Rules
    ///
    /// Because of the excessive power and danger of this method, we currently have only one rule:
    ///
    /// * If the importing file includes `import x.y` then `x.y` is defined in the importing file.
    ///   This is an easy rule to justify because `import` can only ever import a module, and the
    ///   only reason to do it is to explicitly introduce those submodules and attributes, so it
    ///   *should* shadow any non-submodule of the same name.
    ///
    /// `from x.y import z` instances are currently ignored because the `x.y` part may not be a
    /// side-effect the user actually cares about, and the `z` component may not be a submodule.
    ///
    /// We instead prefer handling most other import effects as definitions in the scope of
    /// the current file (i.e. `ty_python_core::definition::ImportFromDefinitionNodeRef`).
    fn available_submodule_attributes(&self, db: &'db dyn Db) -> impl Iterator<Item = Name> {
        self.importing_file(db)
            .into_iter()
            .flat_map(|file| semantic_index(db, file).imported_modules())
            .filter_map(|submodule_name| submodule_name.relative_to(self.module(db).name(db)))
            .filter_map(|relative_submodule| relative_submodule.components().next().map(Name::from))
    }

    fn resolve_submodule(self, db: &'db dyn Db, name: &str) -> Option<Type<'db>> {
        let importing_file = self.importing_file(db)?;
        let relative_submodule_name = ModuleName::new(name)?;
        let mut absolute_submodule_name = self.module(db).name(db).clone();
        absolute_submodule_name.extend(&relative_submodule_name);
        let submodule = resolve_module(db, importing_file, &absolute_submodule_name)?;
        Some(Type::module_literal(db, importing_file, submodule))
    }

    fn try_module_getattr(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        // For module literals, we want to try calling the module's own `__getattr__` function
        // if it exists. First, we need to look up the `__getattr__` function in the module's scope.
        if let Some(file) = self.module(db).file(db) {
            let getattr_symbol = imported_symbol(db, Some(file), "__getattr__", None);
            // If we found a __getattr__ function, try to call it with the name argument
            if let Place::Defined(place) = getattr_symbol.place
                && let Ok(outcome) = place.ty.try_call(
                    db,
                    &CallArguments::positional([Type::string_literal(db, name)]),
                )
            {
                return PlaceAndQualifiers {
                    place: Place::Defined(DefinedPlace {
                        ty: outcome.return_type(db),
                        ..place
                    }),
                    qualifiers: TypeQualifiers::FROM_MODULE_GETATTR,
                };
            }
        }

        Place::Undefined.into()
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
        if self.available_submodule_attributes(db).contains(name)
            && let Some(submodule) = self.resolve_submodule(db, name)
        {
            return Place::bound(submodule).into();
        }

        let place_and_qualifiers = imported_symbol(db, self.module(db).file(db), name, None);

        // If the normal lookup failed, try to call the module's `__getattr__` function
        if place_and_qualifiers.place.is_undefined() {
            return self.try_module_getattr(db, name);
        }

        place_and_qualifiers
    }
}

/// Either the explicit `metaclass=` keyword of the class, or the inferred metaclass of one of its base classes.
#[derive(Debug, Clone, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(super) struct MetaclassCandidate<'db> {
    metaclass: ClassType<'db>,
    explicit_metaclass_of: StaticClassLiteral<'db>,
}

/// Information about a `@dataclass_transform`-decorated metaclass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) struct MetaclassTransformInfo<'db> {
    pub(super) params: DataclassTransformerParams<'db>,

    /// Whether the metaclass providing these parameters was declared on the class itself
    /// (via an explicit `metaclass=` keyword) rather than inherited from a base class.
    pub(super) from_explicit_metaclass: bool,
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeIsType<'db> {
    type_argument: Type<'db>,
    /// The ID of the scope to which the place belongs
    /// and the ID of the place itself within that scope.
    place_info: Option<(ScopeId<'db>, ScopedPlaceId)>,
}

fn walk_typeis_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    typeis_type: TypeIsType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, typeis_type.type_argument(db));
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypeIsType<'_> {}

impl<'db> TypeIsType<'db> {
    pub(crate) fn place_name(self, db: &'db dyn Db) -> Option<String> {
        let (scope, place) = self.place_info(db)?;
        let table = place_table(db, scope);

        Some(format!("{}", table.place(place)))
    }

    /// Construct an unbound `TypeIs` return type from the user-written type expression.
    ///
    /// The user-written type is preserved for `TypeIs` invariance checks, while the return type
    /// used for narrowing applies the top materialization on demand.
    ///
    /// ```python
    /// from typing import TypeIs
    ///
    /// def is_tuple(value: object) -> TypeIs[tuple[int, ...]]:
    ///     return isinstance(value, tuple)
    /// ```
    pub(crate) fn from_type_expression(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeIs(Self::new(db, ty, None))
    }

    pub(crate) fn return_type(self, db: &'db dyn Db) -> Type<'db> {
        // N.B. Using the top materialization here is a pragmatic decision that
        // makes us produce more intuitive results given how `TypeIs` is used in
        // the real world (in particular, in typeshed). However, there's some
        // debate about whether this is really fully correct. See
        // <https://github.com/astral-sh/ruff/pull/20591> for more discussion.
        self.type_argument(db).top_materialization(db)
    }

    #[must_use]
    pub(crate) fn bind(
        self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
    ) -> Type<'db> {
        Type::TypeIs(Self::new(db, self.type_argument(db), Some((scope, place))))
    }

    #[must_use]
    pub(crate) fn with_type(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeIs(Self::new(db, ty, self.place_info(db)))
    }

    pub(crate) fn is_bound(self, db: &'db dyn Db) -> bool {
        self.place_info(db).is_some()
    }
}

impl<'db> VarianceInferable<'db> for TypeIsType<'db> {
    // See the [typing spec] on why `TypeIs` is invariant in its type.
    // [typing spec]: https://typing.python.org/en/latest/spec/narrowing.html#typeis
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        self.type_argument(db)
            .with_polarity(TypeVarVariance::Invariant)
            .variance_of(db, typevar)
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeGuardType<'db> {
    return_type: Type<'db>,
    /// The ID of the scope to which the place belongs
    /// and the ID of the place itself within that scope.
    place_info: Option<(ScopeId<'db>, ScopedPlaceId)>,
}

fn walk_typeguard_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    typeguard_type: TypeGuardType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, typeguard_type.return_type(db));
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypeGuardType<'_> {}

impl<'db> TypeGuardType<'db> {
    pub(crate) fn place_name(self, db: &'db dyn Db) -> Option<String> {
        let (scope, place) = self.place_info(db)?;
        let table = place_table(db, scope);

        Some(format!("{}", table.place(place)))
    }

    pub(crate) fn unbound(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeGuard(Self::new(db, ty, None))
    }

    pub(crate) fn bound(
        db: &'db dyn Db,
        return_type: Type<'db>,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
    ) -> Type<'db> {
        Type::TypeGuard(Self::new(db, return_type, Some((scope, place))))
    }

    #[must_use]
    pub(crate) fn bind(
        self,
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
    ) -> Type<'db> {
        Self::bound(db, self.return_type(db), scope, place)
    }

    #[must_use]
    pub(crate) fn with_type(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeGuard(Self::new(db, ty, self.place_info(db)))
    }

    pub(crate) fn is_bound(self, db: &'db dyn Db) -> bool {
        self.place_info(db).is_some()
    }
}

impl<'db> VarianceInferable<'db> for TypeGuardType<'db> {
    // `TypeGuard` is covariant in its type parameter. See the `TypeGuard`
    // section of mdtest/generics/pep695/variance.md for details.
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        self.return_type(db).variance_of(db, typevar)
    }
}

/// Common trait for `TypeIs` and `TypeGuard` types that share similar structure
/// but have different semantic behaviors.
pub(crate) trait TypeGuardLike<'db>: Copy {
    /// The name of this type guard form (for error messages and display)
    const FORM_NAME: &'static str;

    /// Get the annotation argument stored in the type guard form.
    fn type_argument(self, db: &'db dyn Db) -> Type<'db>;

    /// Get the human-readable place name if bound
    fn place_name(self, db: &'db dyn Db) -> Option<String>;

    /// Create a new instance with a different type argument, wrapped in Type.
    fn with_type(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db>;

    /// The `SpecialFormType` for display purposes
    fn special_form() -> SpecialFormType;
}

impl<'db> TypeGuardLike<'db> for TypeIsType<'db> {
    const FORM_NAME: &'static str = "TypeIs";

    fn type_argument(self, db: &'db dyn Db) -> Type<'db> {
        TypeIsType::type_argument(self, db)
    }

    fn place_name(self, db: &'db dyn Db) -> Option<String> {
        TypeIsType::place_name(self, db)
    }

    fn with_type(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        TypeIsType::with_type(self, db, ty)
    }

    fn special_form() -> SpecialFormType {
        SpecialFormType::TypeIs
    }
}

impl<'db> TypeGuardLike<'db> for TypeGuardType<'db> {
    const FORM_NAME: &'static str = "TypeGuard";

    fn type_argument(self, db: &'db dyn Db) -> Type<'db> {
        TypeGuardType::return_type(self, db)
    }

    fn place_name(self, db: &'db dyn Db) -> Option<String> {
        TypeGuardType::place_name(self, db)
    }

    fn with_type(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        TypeGuardType::with_type(self, db, ty)
    }

    fn special_form() -> SpecialFormType {
        SpecialFormType::TypeGuard
    }
}

/// Walk the MRO of this class and return the last class just before the specified known base.
/// This can be used to determine upper bounds for `Self` type variables on methods that are
/// being added to the given class.
pub(super) fn determine_upper_bound<'db>(
    db: &'db dyn Db,
    class_literal: ClassLiteral<'db>,
    is_known_base: impl Fn(ClassBase<'db>) -> bool,
) -> Type<'db> {
    let upper_bound = class_literal
        .iter_mro(db)
        .take_while(|base| !is_known_base(*base))
        .filter_map(ClassBase::into_class)
        .last()
        .unwrap_or_else(|| class_literal.unknown_specialization(db));
    Type::instance(db, upper_bound)
}

// Make sure that the `Type` enum does not grow unexpectedly.
#[cfg(not(debug_assertions))]
#[cfg(target_pointer_width = "64")]
static_assertions::assert_eq_size!(Type, [u8; 16]);

// Make sure that `LiteralValueTypeInner` stays at 12 bytes.
// The `LiteralFlags` byte must fit in the discriminant's padding.
#[cfg(not(debug_assertions))]
#[cfg(target_pointer_width = "64")]
static_assertions::assert_eq_size!(literal::LiteralValueType, [u8; 12]);
