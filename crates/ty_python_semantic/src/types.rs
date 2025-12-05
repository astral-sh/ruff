use compact_str::{CompactString, ToCompactString};
use infer::nearest_enclosing_class;
use itertools::{Either, Itertools};
use ruff_diagnostics::{Edit, Fix};

use std::borrow::Cow;
use std::time::Duration;

use bitflags::bitflags;
use call::{CallDunderError, CallError, CallErrorKind};
use context::InferContext;
use diagnostic::{INVALID_CONTEXT_MANAGER, NOT_ITERABLE, POSSIBLY_MISSING_IMPLICIT_CALL};
use ruff_db::Instant;
use ruff_db::diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_text_size::{Ranged, TextRange};
use smallvec::{SmallVec, smallvec};

use type_ordering::union_or_intersection_elements_ordering;

pub(crate) use self::builder::{IntersectionBuilder, UnionBuilder};
pub use self::cyclic::CycleDetector;
pub(crate) use self::cyclic::{PairVisitor, TypeTransformer};
pub(crate) use self::diagnostic::register_lints;
pub use self::diagnostic::{TypeCheckDiagnostics, UNDEFINED_REVEAL, UNRESOLVED_REFERENCE};
pub(crate) use self::infer::{
    TypeContext, infer_deferred_types, infer_definition_types, infer_expression_type,
    infer_expression_types, infer_scope_types, static_expression_truthiness,
};
pub(crate) use self::signatures::{CallableSignature, Signature};
pub(crate) use self::subclass_of::{SubclassOfInner, SubclassOfType};
pub use crate::diagnostic::add_inferred_python_version_hint_to_diagnostic;
use crate::module_name::ModuleName;
use crate::module_resolver::{KnownModule, resolve_module};
use crate::place::{
    Definedness, Place, PlaceAndQualifiers, TypeOrigin, imported_symbol, known_module_symbol,
};
use crate::semantic_index::definition::{Definition, DefinitionKind};
use crate::semantic_index::place::ScopedPlaceId;
use crate::semantic_index::scope::ScopeId;
use crate::semantic_index::{imported_modules, place_table, semantic_index};
use crate::suppression::check_suppressions;
use crate::types::bound_super::BoundSuperType;
use crate::types::builder::RecursivelyDefined;
use crate::types::call::{Binding, Bindings, CallArguments, CallableBinding};
pub(crate) use crate::types::class_base::ClassBase;
use crate::types::constraints::{
    ConstraintSet, IteratorConstraintsExtension, OptionConstraintsExtension,
};
use crate::types::context::{LintDiagnosticGuard, LintDiagnosticGuardBuilder};
use crate::types::diagnostic::{INVALID_AWAIT, INVALID_TYPE_FORM, UNSUPPORTED_BOOL_CONVERSION};
pub use crate::types::display::{DisplaySettings, TypeDetail, TypeDisplayDetails};
use crate::types::enums::{enum_metadata, is_single_member_enum};
use crate::types::function::{
    DataclassTransformerFlags, DataclassTransformerParams, FunctionDecorators, FunctionSpans,
    FunctionType, KnownFunction,
};
pub(crate) use crate::types::generics::GenericContext;
use crate::types::generics::{
    InferableTypeVars, PartialSpecialization, Specialization, bind_typevar, typing_self,
    walk_generic_context,
};
use crate::types::mro::{Mro, MroError, MroIterator};
pub(crate) use crate::types::narrow::infer_narrowing_constraint;
use crate::types::newtype::NewType;
pub(crate) use crate::types::signatures::{Parameter, Parameters};
use crate::types::signatures::{ParameterForm, walk_signature};
use crate::types::tuple::{Tuple, TupleSpec, TupleSpecBuilder};
pub(crate) use crate::types::typed_dict::{TypedDictParams, TypedDictType, walk_typed_dict_type};
pub use crate::types::variance::TypeVarVariance;
use crate::types::variance::VarianceInferable;
use crate::types::visitor::any_over_type;
use crate::unpack::EvaluationMode;
use crate::{Db, FxOrderSet, Module, Program};
pub use class::KnownClass;
pub(crate) use class::{ClassLiteral, ClassType, GenericAlias};
use instance::Protocol;
pub use instance::{NominalInstanceType, ProtocolInstanceType};
pub use special_form::SpecialFormType;

mod bound_super;
mod builder;
mod call;
mod class;
mod class_base;
mod constraints;
mod context;
mod cyclic;
mod diagnostic;
mod display;
mod enums;
mod function;
mod generics;
pub mod ide_support;
mod infer;
mod instance;
pub mod list_members;
mod member;
mod mro;
mod narrow;
mod newtype;
mod overrides;
mod protocol_class;
mod signatures;
mod special_form;
mod string_annotation;
mod subclass_of;
mod tuple;
mod type_ordering;
mod typed_dict;
mod unpacker;
mod variance;
mod visitor;

mod definition;
#[cfg(test)]
mod property_tests;

pub fn check_types(db: &dyn Db, file: File) -> Vec<Diagnostic> {
    let _span = tracing::trace_span!("check_types", ?file).entered();
    tracing::debug!("Checking file '{path}'", path = file.path(db));

    let start = Instant::now();

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
        infer_scope_types(db, scope).expression_type(expression)
    }
}

/// A [`TypeTransformer`] that is used in `apply_type_mapping` methods.
pub(crate) type ApplyTypeMappingVisitor<'db> = TypeTransformer<'db, TypeMapping<'db, 'db>>;

/// A [`PairVisitor`] that is used in `has_relation_to` methods.
pub(crate) type HasRelationToVisitor<'db> =
    CycleDetector<TypeRelation<'db>, (Type<'db>, Type<'db>, TypeRelation<'db>), ConstraintSet<'db>>;

impl Default for HasRelationToVisitor<'_> {
    fn default() -> Self {
        HasRelationToVisitor::new(ConstraintSet::from(true))
    }
}

/// A [`PairVisitor`] that is used in `is_disjoint_from` methods.
pub(crate) type IsDisjointVisitor<'db> = PairVisitor<'db, IsDisjoint, ConstraintSet<'db>>;

#[derive(Debug)]
pub(crate) struct IsDisjoint;

impl Default for IsDisjointVisitor<'_> {
    fn default() -> Self {
        IsDisjointVisitor::new(ConstraintSet::from(false))
    }
}

/// A [`PairVisitor`] that is used in `is_equivalent` methods.
pub(crate) type IsEquivalentVisitor<'db> = PairVisitor<'db, IsEquivalent, ConstraintSet<'db>>;

#[derive(Debug)]
pub(crate) struct IsEquivalent;

impl Default for IsEquivalentVisitor<'_> {
    fn default() -> Self {
        IsEquivalentVisitor::new(ConstraintSet::from(true))
    }
}

/// A [`CycleDetector`] that is used in `find_legacy_typevars` methods.
pub(crate) type FindLegacyTypeVarsVisitor<'db> = CycleDetector<FindLegacyTypeVars, Type<'db>, ()>;

#[derive(Debug)]
pub(crate) struct FindLegacyTypeVars;

/// A [`CycleDetector`] that is used in `try_bool` methods.
pub(crate) type TryBoolVisitor<'db> =
    CycleDetector<TryBool, Type<'db>, Result<Truthiness, BoolError<'db>>>;
pub(crate) struct TryBool;

/// A [`CycleDetector`] that is used in `visit_specialization` methods.
pub(crate) type SpecializationVisitor<'db> = CycleDetector<VisitSpecialization, Type<'db>, ()>;
pub(crate) struct VisitSpecialization;

/// A [`TypeTransformer`] that is used in `normalized` methods.
pub(crate) type NormalizedVisitor<'db> = TypeTransformer<'db, Normalized>;

#[derive(Debug)]
pub(crate) struct Normalized;

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
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
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

fn member_lookup_cycle_initial<'db>(
    _db: &'db dyn Db,
    id: salsa::Id,
    _self: Type<'db>,
    _name: Name,
    _policy: MemberLookupPolicy,
) -> PlaceAndQualifiers<'db> {
    Place::bound(Type::divergent(id)).into()
}

fn member_lookup_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_member: &PlaceAndQualifiers<'db>,
    member: PlaceAndQualifiers<'db>,
    _self_type: Type<'db>,
    _name: Name,
    _policy: MemberLookupPolicy,
) -> PlaceAndQualifiers<'db> {
    member.cycle_normalized(db, *previous_member, cycle)
}

fn class_lookup_cycle_initial<'db>(
    _db: &'db dyn Db,
    id: salsa::Id,
    _self: Type<'db>,
    _name: Name,
    _policy: MemberLookupPolicy,
) -> PlaceAndQualifiers<'db> {
    Place::bound(Type::divergent(id)).into()
}

fn class_lookup_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_member: &PlaceAndQualifiers<'db>,
    member: PlaceAndQualifiers<'db>,
    _self_type: Type<'db>,
    _name: Name,
    _policy: MemberLookupPolicy,
) -> PlaceAndQualifiers<'db> {
    member.cycle_normalized(db, *previous_member, cycle)
}

fn variance_cycle_initial<'db, T>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: T,
    _typevar: BoundTypeVarInstance<'db>,
) -> TypeVarVariance {
    TypeVarVariance::Bivariant
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
        Self::new(db, getter, setter)
    }

    fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self::new(
            db,
            self.getter(db).map(|ty| ty.normalized_impl(db, visitor)),
            self.setter(db).map(|ty| ty.normalized_impl(db, visitor)),
        )
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
        Some(Self::new(db, getter, setter))
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
    }

    fn when_equivalent_to(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.is_equivalent_to_impl(db, other, inferable, &IsEquivalentVisitor::default())
    }

    fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        let getter_equivalence = if let Some(getter) = self.getter(db) {
            let Some(other_getter) = other.getter(db) else {
                return ConstraintSet::from(false);
            };
            getter.is_equivalent_to_impl(db, other_getter, inferable, visitor)
        } else {
            if other.getter(db).is_some() {
                return ConstraintSet::from(false);
            }
            ConstraintSet::from(true)
        };

        let setter_equivalence = || {
            if let Some(setter) = self.setter(db) {
                let Some(other_setter) = other.setter(db) else {
                    return ConstraintSet::from(false);
                };
                setter.is_equivalent_to_impl(db, other_setter, inferable, visitor)
            } else {
                if other.setter(db).is_some() {
                    return ConstraintSet::from(false);
                }
                ConstraintSet::from(true)
            }
        };

        getter_equivalence.and(db, setter_equivalence)
    }
}

bitflags! {
    /// Used to store metadata about a dataclass or dataclass-like class.
    /// For the precise meaning of the fields, see [1].
    ///
    /// [1]: https://docs.python.org/3/library/dataclasses.html
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
#[derive(PartialOrd, Ord)]
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
    /// An instance of a typevar. When the generic class or function binding this typevar is
    /// specialized, we will replace the typevar with its specialization.
    TypeVar(BoundTypeVarInstance<'db>),
    /// A bound super object like `super()` or `super(A, A())`
    /// This type doesn't handle an unbound super object like `super(A)`; for that we just use
    /// a `Type::NominalInstance` of `builtins.super`.
    BoundSuper(BoundSuperType<'db>),
    /// A subtype of `bool` that allows narrowing in both positive and negative cases.
    TypeIs(TypeIsType<'db>),
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

#[salsa::tracked]
impl<'db> Type<'db> {
    pub(crate) const fn any() -> Self {
        Self::Dynamic(DynamicType::Any)
    }

    pub(crate) const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub(crate) fn divergent(id: salsa::Id) -> Self {
        Self::Dynamic(DynamicType::Divergent(DivergentType { id }))
    }

    pub(crate) const fn is_divergent(&self) -> bool {
        matches!(self, Type::Dynamic(DynamicType::Divergent(_)))
    }

    pub const fn is_unknown(&self) -> bool {
        matches!(
            self,
            Type::Dynamic(DynamicType::Unknown | DynamicType::UnknownGeneric(_))
        )
    }

    pub(crate) const fn is_never(&self) -> bool {
        matches!(self, Type::Never)
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
        UnionType::from_elements_cycle_recovery(db, [self, previous])
            .recursive_type_normalized(db, cycle)
    }

    fn is_none(&self, db: &'db dyn Db) -> bool {
        self.is_instance_of(db, KnownClass::NoneType)
    }

    fn is_bool(&self, db: &'db dyn Db) -> bool {
        self.is_instance_of(db, KnownClass::Bool)
    }

    fn is_enum(&self, db: &'db dyn Db) -> bool {
        self.as_nominal_instance()
            .and_then(|instance| crate::types::enums::enum_metadata(db, instance.class_literal(db)))
            .is_some()
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
                bindings.return_type(db) == Type::BooleanLiteral(allowed_return_value)
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
            | DynamicType::UnknownGeneric(_)
            | DynamicType::Divergent(_) => false,
            DynamicType::Todo(_) | DynamicType::TodoStarredExpression | DynamicType::TodoUnpack => {
                true
            }
        })
    }

    pub const fn is_generic_alias(&self) -> bool {
        matches!(self, Type::GenericAlias(_))
    }

    const fn is_dynamic(&self) -> bool {
        matches!(self, Type::Dynamic(_))
    }

    const fn is_non_divergent_dynamic(&self) -> bool {
        self.is_dynamic() && !self.is_divergent()
    }

    /// Is a value of this type only usable in typing contexts?
    pub(crate) fn is_type_check_only(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::ClassLiteral(class_literal) => class_literal.type_check_only(db),
            Type::FunctionLiteral(f) => {
                f.has_known_decorator(db, FunctionDecorators::TYPE_CHECK_ONLY)
            }
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

    /// If this type is a class instance, returns its specialization.
    pub(crate) fn class_specialization(self, db: &'db dyn Db) -> Option<Specialization<'db>> {
        self.specialization_of_optional(db, None)
    }

    /// If the type is a specialized instance of the given class, returns the specialization.
    pub(crate) fn specialization_of(
        self,
        db: &'db dyn Db,
        expected_class: ClassLiteral<'_>,
    ) -> Option<Specialization<'db>> {
        self.specialization_of_optional(db, Some(expected_class))
    }

    fn specialization_of_optional(
        self,
        db: &'db dyn Db,
        expected_class: Option<ClassLiteral<'_>>,
    ) -> Option<Specialization<'db>> {
        let class_type = match self {
            Type::NominalInstance(instance) => instance,
            Type::TypeAlias(alias) => alias.value_type(db).as_nominal_instance()?,
            _ => return None,
        }
        .class(db);

        let (class_literal, specialization) = class_type.class_literal(db);
        if expected_class.is_some_and(|expected_class| expected_class != class_literal) {
            return None;
        }

        specialization
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

    pub(crate) const fn is_type_var(self) -> bool {
        matches!(self, Type::TypeVar(_))
    }

    pub(crate) const fn as_typevar(self) -> Option<BoundTypeVarInstance<'db>> {
        match self {
            Type::TypeVar(bound_typevar) => Some(bound_typevar),
            _ => None,
        }
    }

    pub(crate) fn has_typevar(self, db: &'db dyn Db) -> bool {
        any_over_type(db, self, &|ty| matches!(ty, Type::TypeVar(_)), false)
    }

    pub(crate) const fn as_special_form(self) -> Option<SpecialFormType> {
        match self {
            Type::SpecialForm(special_form) => Some(special_form),
            _ => None,
        }
    }

    pub(crate) const fn as_class_literal(self) -> Option<ClassLiteral<'db>> {
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

    pub(crate) const fn as_enum_literal(self) -> Option<EnumLiteralType<'db>> {
        match self {
            Type::EnumLiteral(enum_literal) => Some(enum_literal),
            _ => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    pub(crate) const fn expect_enum_literal(self) -> EnumLiteralType<'db> {
        self.as_enum_literal()
            .expect("Expected a Type::EnumLiteral variant")
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

    pub(crate) const fn is_union(&self) -> bool {
        matches!(self, Type::Union(_))
    }

    pub(crate) const fn as_union(self) -> Option<UnionType<'db>> {
        match self {
            Type::Union(union_type) => Some(union_type),
            _ => None,
        }
    }

    #[track_caller]
    pub(crate) const fn expect_union(self) -> UnionType<'db> {
        self.as_union().expect("Expected a Type::Union variant")
    }

    pub(crate) const fn as_function_literal(self) -> Option<FunctionType<'db>> {
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

    /// Detects types which are valid to appear inside a `Literal[…]` type annotation.
    pub(crate) fn is_literal_or_union_of_literals(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::Union(union) => union
                .elements(db)
                .iter()
                .all(|ty| ty.is_literal_or_union_of_literals(db)),
            Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::EnumLiteral(_) => true,
            Type::NominalInstance(_) => self.is_none(db) || self.is_bool(db) || self.is_enum(db),
            _ => false,
        }
    }

    pub(crate) fn is_union_of_single_valued(&self, db: &'db dyn Db) -> bool {
        self.as_union().is_some_and(|union| {
            union.elements(db).iter().all(|ty| {
                ty.is_single_valued(db)
                    || ty.is_bool(db)
                    || ty.is_literal_string()
                    || (ty.is_enum(db) && !ty.overrides_equality(db))
            })
        }) || self.is_bool(db)
            || self.is_literal_string()
            || (self.is_enum(db) && !self.overrides_equality(db))
    }

    pub(crate) fn is_union_with_single_valued(&self, db: &'db dyn Db) -> bool {
        self.as_union().is_some_and(|union| {
            union.elements(db).iter().any(|ty| {
                ty.is_single_valued(db)
                    || ty.is_bool(db)
                    || ty.is_literal_string()
                    || (ty.is_enum(db) && !ty.overrides_equality(db))
            })
        }) || self.is_bool(db)
            || self.is_literal_string()
            || (self.is_enum(db) && !self.overrides_equality(db))
    }

    pub(crate) fn as_string_literal(self) -> Option<StringLiteralType<'db>> {
        match self {
            Type::StringLiteral(string_literal) => Some(string_literal),
            _ => None,
        }
    }

    pub(crate) const fn is_literal_string(&self) -> bool {
        matches!(self, Type::LiteralString)
    }

    pub(crate) fn string_literal(db: &'db dyn Db, string: &str) -> Self {
        Self::StringLiteral(StringLiteralType::new(db, string))
    }

    pub(crate) fn bytes_literal(db: &'db dyn Db, bytes: &[u8]) -> Self {
        Self::BytesLiteral(BytesLiteralType::new(db, bytes))
    }

    pub(crate) fn typed_dict(defining_class: impl Into<ClassType<'db>>) -> Self {
        Self::TypedDict(TypedDictType::new(defining_class.into()))
    }

    #[must_use]
    pub(crate) fn negate(&self, db: &'db dyn Db) -> Type<'db> {
        IntersectionBuilder::new(db).add_negative(*self).build()
    }

    #[must_use]
    pub(crate) fn negate_if(&self, db: &'db dyn Db, yes: bool) -> Type<'db> {
        if yes { self.negate(db) } else { *self }
    }

    /// If the type is a union, filters union elements based on the provided predicate.
    ///
    /// Otherwise, returns the type unchanged.
    pub(crate) fn filter_union(
        self,
        db: &'db dyn Db,
        f: impl FnMut(&Type<'db>) -> bool,
    ) -> Type<'db> {
        if let Type::Union(union) = self {
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
        inferable: InferableTypeVars<'_, 'db>,
    ) -> Type<'db> {
        self.filter_union(db, |elem| {
            !elem
                .when_disjoint_from(db, target, inferable)
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
            Type::StringLiteral(_) | Type::LiteralString => Some(KnownClass::Str.to_instance(db)),
            Type::BooleanLiteral(_) => Some(KnownClass::Bool.to_instance(db)),
            Type::IntLiteral(_) => Some(KnownClass::Int.to_instance(db)),
            Type::BytesLiteral(_) => Some(KnownClass::Bytes.to_instance(db)),
            Type::ModuleLiteral(_) => Some(KnownClass::ModuleType.to_instance(db)),
            Type::FunctionLiteral(_) => Some(KnownClass::FunctionType.to_instance(db)),
            Type::EnumLiteral(literal) => Some(literal.enum_class_instance(db)),
            _ => None,
        }
    }

    /// Promote (possibly nested) literals to types that these literals are instances of.
    ///
    /// Note that this function tries to promote literals to a more user-friendly form than their
    /// fallback instance type. For example, `def _() -> int` is promoted to `Callable[[], int]`,
    /// as opposed to `FunctionType`.
    ///
    /// It also avoids literal promotion if a literal type annotation was provided as type context.
    pub(crate) fn promote_literals(self, db: &'db dyn Db, tcx: TypeContext<'db>) -> Type<'db> {
        self.apply_type_mapping(
            db,
            &TypeMapping::PromoteLiterals(PromoteLiteralsMode::On),
            tcx,
        )
    }

    /// Like [`Type::promote_literals`], but does not recurse into nested types.
    fn promote_literals_impl(self, db: &'db dyn Db, tcx: TypeContext<'db>) -> Type<'db> {
        let promoted = match self {
            Type::StringLiteral(_) => KnownClass::Str.to_instance(db),
            Type::LiteralString => KnownClass::Str.to_instance(db),
            Type::BooleanLiteral(_) => KnownClass::Bool.to_instance(db),
            Type::IntLiteral(_) => KnownClass::Int.to_instance(db),
            Type::BytesLiteral(_) => KnownClass::Bytes.to_instance(db),
            Type::ModuleLiteral(_) => KnownClass::ModuleType.to_instance(db),
            Type::EnumLiteral(literal) => literal.enum_class_instance(db),
            Type::FunctionLiteral(literal) => Type::Callable(literal.into_callable_type(db)),
            _ => return self,
        };

        // Avoid literal promotion if it leads to an unassignable type.
        if tcx.annotation.is_some_and(|annotation| {
            self.is_assignable_to(db, annotation) && !promoted.is_assignable_to(db, annotation)
        }) {
            return self;
        }

        promoted
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
    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        self.normalized_impl(db, &NormalizedVisitor::default())
    }

    #[must_use]
    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        match self {
            Type::Union(union) => visitor.visit(self, || union.normalized_impl(db, visitor)),
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
            Type::KnownBoundMethod(method_kind) => visitor.visit(self, || {
                Type::KnownBoundMethod(method_kind.normalized_impl(db, visitor))
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
            Type::TypeAlias(alias) => alias.value_type(db).normalized_impl(db, visitor),
            Type::NewTypeInstance(newtype) => {
                visitor.visit(self, || {
                    Type::NewTypeInstance(newtype.map_base_class_type(db, |class_type| {
                        class_type.normalized_impl(db, visitor)
                    }))
                })
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
        if nested && self == div {
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
                let ty = if nested {
                    type_is
                        .return_type(db)
                        .recursive_type_normalized_impl(db, div, true)?
                } else {
                    type_is
                        .return_type(db)
                        .recursive_type_normalized_impl(db, div, true)
                        .unwrap_or(div)
                };
                Some(type_is.with_type(db, ty))
            }
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
            | Type::IntLiteral(_) => Some(self),
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
            | Type::KnownBoundMethod(_)
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
            | Type::TypedDict(_)
            | Type::TypeAlias(_)
            | Type::NewTypeInstance(_) => false,
        }
    }

    pub(crate) fn try_upcast_to_callable(self, db: &'db dyn Db) -> Option<CallableTypes<'db>> {
        match self {
            Type::Callable(callable) => Some(CallableTypes::one(callable)),

            Type::Dynamic(_) => Some(CallableTypes::one(CallableType::function_like(
                db,
                Signature::dynamic(self),
            ))),

            Type::FunctionLiteral(function_literal) => {
                Some(CallableTypes::one(function_literal.into_callable_type(db)))
            }
            Type::BoundMethod(bound_method) => {
                Some(CallableTypes::one(bound_method.into_callable_type(db)))
            }

            Type::NominalInstance(_) | Type::ProtocolInstance(_) => {
                let call_symbol = self
                    .member_lookup_with_policy(
                        db,
                        Name::new_static("__call__"),
                        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    )
                    .place;

                if let Place::Defined(ty, _, Definedness::AlwaysDefined) = call_symbol {
                    ty.try_upcast_to_callable(db)
                } else {
                    None
                }
            }
            Type::ClassLiteral(class_literal) => {
                Some(class_literal.default_specialization(db).into_callable(db))
            }

            Type::GenericAlias(alias) => Some(ClassType::Generic(alias).into_callable(db)),

            Type::NewTypeInstance(newtype) => {
                Type::instance(db, newtype.base_class_type(db)).try_upcast_to_callable(db)
            }

            // TODO: This is unsound so in future we can consider an opt-in option to disable it.
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Class(class) => Some(class.into_callable(db)),

                SubclassOfInner::Dynamic(_) | SubclassOfInner::TypeVar(_) => {
                    Some(CallableTypes::one(CallableType::single(
                        db,
                        Signature::new(Parameters::unknown(), Some(Type::from(subclass_of_ty))),
                    )))
                }
            },

            Type::Union(union) => {
                let mut callables = SmallVec::new();
                for element in union.elements(db) {
                    let element_callable = element.try_upcast_to_callable(db)?;
                    callables.extend(element_callable.into_inner());
                }
                Some(CallableTypes(callables))
            }

            Type::EnumLiteral(enum_literal) => enum_literal
                .enum_class_instance(db)
                .try_upcast_to_callable(db),

            Type::TypeAlias(alias) => alias.value_type(db).try_upcast_to_callable(db),

            Type::KnownBoundMethod(method) => Some(CallableTypes::one(CallableType::new(
                db,
                CallableSignature::from_overloads(method.signatures(db)),
                CallableTypeKind::Regular,
            ))),

            Type::WrapperDescriptor(wrapper_descriptor) => {
                Some(CallableTypes::one(CallableType::new(
                    db,
                    CallableSignature::from_overloads(wrapper_descriptor.signatures(db)),
                    CallableTypeKind::Regular,
                )))
            }

            Type::KnownInstance(KnownInstanceType::NewType(newtype)) => {
                Some(CallableTypes::one(CallableType::single(
                    db,
                    Signature::new(
                        Parameters::new(
                            db,
                            [Parameter::positional_only(None)
                                .with_annotated_type(newtype.base(db).instance_type(db))],
                        ),
                        Some(Type::NewTypeInstance(newtype)),
                    ),
                )))
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
            Type::DataclassDecorator(_)
            | Type::ModuleLiteral(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::PropertyInstance(_)
            | Type::Intersection(_)
            | Type::TypeVar(_)
            | Type::BoundSuper(_) => None,
        }
    }

    /// Return true if this type is a subtype of type `target`.
    ///
    /// See [`TypeRelation::Subtyping`] for more details.
    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        self.when_subtype_of(db, target, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    fn when_subtype_of(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to(db, target, inferable, TypeRelation::Subtyping)
    }

    /// Return the constraints under which this type is a subtype of type `target`, assuming that
    /// all of the restrictions in `constraints` hold.
    ///
    /// See [`TypeRelation::SubtypingAssuming`] for more details.
    fn when_subtype_of_assuming(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        assuming: ConstraintSet<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to(
            db,
            target,
            inferable,
            TypeRelation::SubtypingAssuming(assuming),
        )
    }

    /// Return true if this type is assignable to type `target`.
    ///
    /// See `TypeRelation::Assignability` for more details.
    pub fn is_assignable_to(self, db: &'db dyn Db, target: Type<'db>) -> bool {
        self.when_assignable_to(db, target, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    fn when_assignable_to(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to(db, target, inferable, TypeRelation::Assignability)
    }

    /// Return `true` if it would be redundant to add `self` to a union that already contains `other`.
    ///
    /// See [`TypeRelation::Redundancy`] for more details.
    #[salsa::tracked(cycle_initial=is_redundant_with_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn is_redundant_with(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        self.has_relation_to(db, other, InferableTypeVars::None, TypeRelation::Redundancy)
            .is_always_satisfied(db)
    }

    fn has_relation_to(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to_impl(
            db,
            target,
            inferable,
            relation,
            &HasRelationToVisitor::default(),
            &IsDisjointVisitor::default(),
        )
    }

    fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        target: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        // Subtyping implies assignability, so if subtyping is reflexive and the two types are
        // equal, it is both a subtype and assignable. Assignability is always reflexive.
        //
        // Note that we could do a full equivalence check here, but that would be both expensive
        // and unnecessary. This early return is only an optimisation.
        if (!relation.is_subtyping() || self.subtyping_is_always_reflexive()) && self == target {
            return ConstraintSet::from(true);
        }

        // Handle constraint implication first. If either `self` or `target` is a typevar, check
        // the constraint set to see if the corresponding constraint is satisfied.
        if let TypeRelation::SubtypingAssuming(constraints) = relation
            && (self.is_type_var() || target.is_type_var())
        {
            return constraints.implies_subtype_of(db, self, target);
        }

        match (self, target) {
            // Everything is a subtype of `object`.
            (_, Type::NominalInstance(instance)) if instance.is_object() => {
                ConstraintSet::from(true)
            }
            (_, Type::ProtocolInstance(target)) if target.is_equivalent_to_object(db) => {
                ConstraintSet::from(true)
            }

            // `Never` is the bottom type, the empty set.
            // It is a subtype of all other types.
            (Type::Never, _) => ConstraintSet::from(true),

            // In some specific situations, `Any`/`Unknown`/`@Todo` can be simplified out of unions and intersections,
            // but this is not true for divergent types (and moving this case any lower down appears to cause
            // "too many cycle iterations" panics).
            (Type::Dynamic(DynamicType::Divergent(_)), _)
            | (_, Type::Dynamic(DynamicType::Divergent(_))) => {
                ConstraintSet::from(relation.is_assignability())
            }

            (Type::TypeAlias(self_alias), _) => {
                relation_visitor.visit((self, target, relation), || {
                    self_alias.value_type(db).has_relation_to_impl(
                        db,
                        target,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            (_, Type::TypeAlias(target_alias)) => {
                relation_visitor.visit((self, target, relation), || {
                    self.has_relation_to_impl(
                        db,
                        target_alias.value_type(db),
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            // Pretend that instances of `dataclasses.Field` are assignable to their default type.
            // This allows field definitions like `name: str = field(default="")` in dataclasses
            // to pass the assignability check of the inferred type to the declared type.
            (Type::KnownInstance(KnownInstanceType::Field(field)), right)
                if relation.is_assignability() =>
            {
                field.default_type(db).when_none_or(|default_type| {
                    default_type.has_relation_to_impl(
                        db,
                        right,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            // Dynamic is only a subtype of `object` and only a supertype of `Never`; both were
            // handled above. It's always assignable, though.
            //
            // Union simplification sits in between subtyping and assignability. `Any <: T` only
            // holds true if `T` is also a dynamic type or a union that contains a dynamic type.
            // Similarly, `T <: Any` only holds true if `T` is a dynamic type or an intersection
            // that contains a dynamic type.
            (Type::Dynamic(dynamic), _) => {
                // If a `Divergent` type is involved, it must not be eliminated.
                debug_assert!(
                    !matches!(dynamic, DynamicType::Divergent(_)),
                    "DynamicType::Divergent should have been handled in an earlier branch"
                );
                ConstraintSet::from(match relation {
                    TypeRelation::Subtyping | TypeRelation::SubtypingAssuming(_) => false,
                    TypeRelation::Assignability => true,
                    TypeRelation::Redundancy => match target {
                        Type::Dynamic(_) => true,
                        Type::Union(union) => union.elements(db).iter().any(Type::is_dynamic),
                        _ => false,
                    },
                })
            }
            (_, Type::Dynamic(_)) => ConstraintSet::from(match relation {
                TypeRelation::Subtyping | TypeRelation::SubtypingAssuming(_) => false,
                TypeRelation::Assignability => true,
                TypeRelation::Redundancy => match self {
                    Type::Dynamic(_) => true,
                    Type::Intersection(intersection) => {
                        // If a `Divergent` type is involved, it must not be eliminated.
                        intersection
                            .positive(db)
                            .iter()
                            .any(Type::is_non_divergent_dynamic)
                    }
                    _ => false,
                },
            }),

            // In general, a TypeVar `T` is not a subtype of a type `S` unless one of the two conditions is satisfied:
            // 1. `T` is a bound TypeVar and `T`'s upper bound is a subtype of `S`.
            //    TypeVars without an explicit upper bound are treated as having an implicit upper bound of `object`.
            // 2. `T` is a constrained TypeVar and all of `T`'s constraints are subtypes of `S`.
            //
            // However, there is one exception to this general rule: for any given typevar `T`,
            // `T` will always be a subtype of any union containing `T`.
            (Type::TypeVar(bound_typevar), Type::Union(union))
                if !bound_typevar.is_inferable(db, inferable)
                    && union.elements(db).contains(&self) =>
            {
                ConstraintSet::from(true)
            }

            // A similar rule applies in reverse to intersection types.
            (Type::Intersection(intersection), Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, inferable)
                    && intersection.positive(db).contains(&target) =>
            {
                ConstraintSet::from(true)
            }
            (Type::Intersection(intersection), Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, inferable)
                    && intersection.negative(db).contains(&target) =>
            {
                ConstraintSet::from(false)
            }

            // Two identical typevars must always solve to the same type, so they are always
            // subtypes of each other and assignable to each other.
            //
            // Note that this is not handled by the early return at the beginning of this method,
            // since subtyping between a TypeVar and an arbitrary other type cannot be guaranteed to be reflexive.
            (Type::TypeVar(lhs_bound_typevar), Type::TypeVar(rhs_bound_typevar))
                if !lhs_bound_typevar.is_inferable(db, inferable)
                    && lhs_bound_typevar.is_same_typevar_as(db, rhs_bound_typevar) =>
            {
                ConstraintSet::from(true)
            }

            // `type[T]` is a subtype of the class object `A` if every instance of `T` is a subtype of an instance
            // of `A`, and vice versa.
            (Type::SubclassOf(subclass_of), _)
                if !subclass_of
                    .into_type_var()
                    .zip(target.to_instance(db))
                    .when_some_and(|(this_instance, other_instance)| {
                        Type::TypeVar(this_instance).has_relation_to_impl(
                            db,
                            other_instance,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
                    .is_never_satisfied(db) =>
            {
                // TODO: The repetition here isn't great, but we need the fallthrough logic.
                subclass_of
                    .into_type_var()
                    .zip(target.to_instance(db))
                    .when_some_and(|(this_instance, other_instance)| {
                        Type::TypeVar(this_instance).has_relation_to_impl(
                            db,
                            other_instance,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
            }

            (_, Type::SubclassOf(subclass_of))
                if !subclass_of
                    .into_type_var()
                    .zip(self.to_instance(db))
                    .when_some_and(|(other_instance, this_instance)| {
                        this_instance.has_relation_to_impl(
                            db,
                            Type::TypeVar(other_instance),
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
                    .is_never_satisfied(db) =>
            {
                // TODO: The repetition here isn't great, but we need the fallthrough logic.
                subclass_of
                    .into_type_var()
                    .zip(self.to_instance(db))
                    .when_some_and(|(other_instance, this_instance)| {
                        this_instance.has_relation_to_impl(
                            db,
                            Type::TypeVar(other_instance),
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
            }

            // A fully static typevar is a subtype of its upper bound, and to something similar to
            // the union of its constraints. An unbound, unconstrained, fully static typevar has an
            // implicit upper bound of `object` (which is handled above).
            (Type::TypeVar(bound_typevar), _)
                if !bound_typevar.is_inferable(db, inferable)
                    && bound_typevar.typevar(db).bound_or_constraints(db).is_some() =>
            {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => unreachable!(),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound
                        .has_relation_to_impl(
                            db,
                            target,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        ),
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        constraints.elements(db).iter().when_all(db, |constraint| {
                            constraint.has_relation_to_impl(
                                db,
                                target,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        })
                    }
                }
            }

            // If the typevar is constrained, there must be multiple constraints, and the typevar
            // might be specialized to any one of them. However, the constraints do not have to be
            // disjoint, which means an lhs type might be a subtype of all of the constraints.
            (_, Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, inferable)
                    && !bound_typevar
                        .typevar(db)
                        .constraints(db)
                        .when_some_and(|constraints| {
                            constraints.iter().when_all(db, |constraint| {
                                self.has_relation_to_impl(
                                    db,
                                    *constraint,
                                    inferable,
                                    relation,
                                    relation_visitor,
                                    disjointness_visitor,
                                )
                            })
                        })
                        .is_never_satisfied(db) =>
            {
                // TODO: The repetition here isn't great, but we really need the fallthrough logic,
                // where this arm only engages if it returns true (or in the world of constraints,
                // not false). Once we're using real constraint sets instead of bool, we should be
                // able to simplify the typevar logic.
                bound_typevar
                    .typevar(db)
                    .constraints(db)
                    .when_some_and(|constraints| {
                        constraints.iter().when_all(db, |constraint| {
                            self.has_relation_to_impl(
                                db,
                                *constraint,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        })
                    })
            }

            (Type::TypeVar(bound_typevar), _) if bound_typevar.is_inferable(db, inferable) => {
                // The implicit lower bound of a typevar is `Never`, which means
                // that it is always assignable to any other type.

                // TODO: record the unification constraints

                ConstraintSet::from(true)
            }

            // `Never` is the bottom type, the empty set.
            (_, Type::Never) => ConstraintSet::from(false),

            (Type::Union(union), _) => union.elements(db).iter().when_all(db, |&elem_ty| {
                elem_ty.has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }),

            (_, Type::Union(union)) => union.elements(db).iter().when_any(db, |&elem_ty| {
                self.has_relation_to_impl(
                    db,
                    elem_ty,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }),

            // If both sides are intersections we need to handle the right side first
            // (A & B & C) is a subtype of (A & B) because the left is a subtype of both A and B,
            // but none of A, B, or C is a subtype of (A & B).
            (_, Type::Intersection(intersection)) => intersection
                .positive(db)
                .iter()
                .when_all(db, |&pos_ty| {
                    self.has_relation_to_impl(
                        db,
                        pos_ty,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
                .and(db, || {
                    // For subtyping, we would want to check whether the *top materialization* of `self`
                    // is disjoint from the *top materialization* of `neg_ty`. As an optimization, however,
                    // we can avoid this explicit transformation here, since our `Type::is_disjoint_from`
                    // implementation already only returns true for `T.is_disjoint_from(U)` if the *top
                    // materialization* of `T` is disjoint from the *top materialization* of `U`.
                    //
                    // Note that the implementation of redundancy here may be too strict from a
                    // theoretical perspective: under redundancy, `T <: ~U` if `Bottom[T]` is disjoint
                    // from `Top[U]` and `Bottom[U]` is disjoint from `Top[T]`. It's possible that this
                    // could be improved. For now, however, we err on the side of strictness for our
                    // redundancy implementation: a fully complete implementation of redundancy may lead
                    // to non-transitivity (highly undesirable); and pragmatically, a full implementation
                    // of redundancy may not generally lead to simpler types in many situations.
                    let self_ty = match relation {
                        TypeRelation::Subtyping
                        | TypeRelation::Redundancy
                        | TypeRelation::SubtypingAssuming(_) => self,
                        TypeRelation::Assignability => self.bottom_materialization(db),
                    };
                    intersection.negative(db).iter().when_all(db, |&neg_ty| {
                        let neg_ty = match relation {
                            TypeRelation::Subtyping
                            | TypeRelation::Redundancy
                            | TypeRelation::SubtypingAssuming(_) => neg_ty,
                            TypeRelation::Assignability => neg_ty.bottom_materialization(db),
                        };
                        self_ty.is_disjoint_from_impl(
                            db,
                            neg_ty,
                            inferable,
                            disjointness_visitor,
                            relation_visitor,
                        )
                    })
                }),

            (Type::Intersection(intersection), _) => {
                intersection.positive(db).iter().when_any(db, |&elem_ty| {
                    elem_ty.has_relation_to_impl(
                        db,
                        target,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            // Other than the special cases checked above, no other types are a subtype of a
            // typevar, since there's no guarantee what type the typevar will be specialized to.
            // (If the typevar is bounded, it might be specialized to a smaller type than the
            // bound. This is true even if the bound is a final class, since the typevar can still
            // be specialized to `Never`.)
            (_, Type::TypeVar(bound_typevar)) if !bound_typevar.is_inferable(db, inferable) => {
                ConstraintSet::from(false)
            }

            (_, Type::TypeVar(typevar))
                if typevar.is_inferable(db, inferable)
                    && relation.is_assignability()
                    && typevar.typevar(db).upper_bound(db).is_none_or(|bound| {
                        !self
                            .has_relation_to_impl(
                                db,
                                bound,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                            .is_never_satisfied(db)
                    }) =>
            {
                // TODO: record the unification constraints

                typevar.typevar(db).upper_bound(db).when_none_or(|bound| {
                    self.has_relation_to_impl(
                        db,
                        bound,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            // TODO: Infer specializations here
            (_, Type::TypeVar(bound_typevar)) if bound_typevar.is_inferable(db, inferable) => {
                ConstraintSet::from(false)
            }
            (Type::TypeVar(bound_typevar), _) => {
                // All inferable cases should have been handled above
                assert!(!bound_typevar.is_inferable(db, inferable));
                ConstraintSet::from(false)
            }

            // Note that the definition of `Type::AlwaysFalsy` depends on the return value of `__bool__`.
            // If `__bool__` always returns True or False, it can be treated as a subtype of `AlwaysTruthy` or `AlwaysFalsy`, respectively.
            (left, Type::AlwaysFalsy) => ConstraintSet::from(left.bool(db).is_always_false()),
            (left, Type::AlwaysTruthy) => ConstraintSet::from(left.bool(db).is_always_true()),
            // Currently, the only supertype of `AlwaysFalsy` and `AlwaysTruthy` is the universal set (object instance).
            (Type::AlwaysFalsy | Type::AlwaysTruthy, _) => {
                target.when_equivalent_to(db, Type::object(), inferable)
            }

            // These clauses handle type variants that include function literals. A function
            // literal is the subtype of itself, and not of any other function literal. However,
            // our representation of a function literal includes any specialization that should be
            // applied to the signature. Different specializations of the same function literal are
            // only subtypes of each other if they result in the same signature.
            (Type::FunctionLiteral(self_function), Type::FunctionLiteral(target_function)) => {
                self_function.has_relation_to_impl(
                    db,
                    target_function,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }
            (Type::BoundMethod(self_method), Type::BoundMethod(target_method)) => self_method
                .has_relation_to_impl(
                    db,
                    target_method,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                ),
            (Type::KnownBoundMethod(self_method), Type::KnownBoundMethod(target_method)) => {
                self_method.has_relation_to_impl(
                    db,
                    target_method,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
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
            ) => ConstraintSet::from(false),

            (Type::Callable(self_callable), Type::Callable(other_callable)) => relation_visitor
                .visit((self, target, relation), || {
                    self_callable.has_relation_to_impl(
                        db,
                        other_callable,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                }),

            (_, Type::Callable(other_callable)) => {
                relation_visitor.visit((self, target, relation), || {
                    self.try_upcast_to_callable(db).when_some_and(|callables| {
                        callables.has_relation_to_impl(
                            db,
                            other_callable,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    })
                })
            }

            // `type[Any]` is assignable to arbitrary protocols as it has arbitrary attributes
            // (this is handled by a lower-down branch), but it is only a subtype of a given
            // protocol if `type` is a subtype of that protocol. Similarly, `type[T]` will
            // always be assignable to any protocol if `type[<upper bound of T>]` is assignable
            // to that protocol (handled lower down), but it is only a subtype of that protocol
            // if `type` is a subtype of that protocol.
            (Type::SubclassOf(self_subclass_ty), Type::ProtocolInstance(_))
                if (self_subclass_ty.is_dynamic() || self_subclass_ty.is_type_var())
                    && !relation.is_assignability() =>
            {
                KnownClass::Type.to_instance(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            (_, Type::ProtocolInstance(protocol)) => {
                relation_visitor.visit((self, target, relation), || {
                    self.satisfies_protocol(
                        db,
                        protocol,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            // A protocol instance can never be a subtype of a nominal type, with the *sole* exception of `object`.
            (Type::ProtocolInstance(_), _) => ConstraintSet::from(false),

            (Type::TypedDict(self_typeddict), Type::TypedDict(other_typeddict)) => relation_visitor
                .visit((self, target, relation), || {
                    self_typeddict.has_relation_to_impl(
                        db,
                        other_typeddict,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                }),

            // TODO: When we support `closed` and/or `extra_items`, we could allow assignments to other
            // compatible `Mapping`s. `extra_items` could also allow for some assignments to `dict`, as
            // long as `total=False`. (But then again, does anyone want a non-total `TypedDict` where all
            // key types are a supertype of the extra items type?)
            (Type::TypedDict(_), _) => relation_visitor.visit((self, target, relation), || {
                KnownClass::Mapping
                    .to_specialized_instance(db, [KnownClass::Str.to_instance(db), Type::object()])
                    .has_relation_to_impl(
                        db,
                        target,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
            }),

            // A non-`TypedDict` cannot subtype a `TypedDict`
            (_, Type::TypedDict(_)) => ConstraintSet::from(false),

            // All `StringLiteral` types are a subtype of `LiteralString`.
            (Type::StringLiteral(_), Type::LiteralString) => ConstraintSet::from(true),

            // An instance is a subtype of an enum literal, if it is an instance of the enum class
            // and the enum has only one member.
            (Type::NominalInstance(_), Type::EnumLiteral(target_enum_literal)) => {
                if target_enum_literal.enum_class_instance(db) != self {
                    return ConstraintSet::from(false);
                }

                ConstraintSet::from(is_single_member_enum(
                    db,
                    target_enum_literal.enum_class(db),
                ))
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
                | Type::EnumLiteral(_)
                | Type::FunctionLiteral(_),
                _,
            ) => (self.literal_fallback_instance(db)).when_some_and(|instance| {
                instance.has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }),

            // The same reasoning applies for these special callable types:
            (Type::BoundMethod(_), _) => {
                KnownClass::MethodType.to_instance(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }
            (Type::KnownBoundMethod(method), _) => {
                method.class().to_instance(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }
            (Type::WrapperDescriptor(_), _) => KnownClass::WrapperDescriptorType
                .to_instance(db)
                .has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                ),

            (Type::DataclassDecorator(_) | Type::DataclassTransformer(_), _) => {
                // TODO: Implement subtyping using an equivalent `Callable` type.
                ConstraintSet::from(false)
            }

            // `TypeIs` is invariant.
            (Type::TypeIs(left), Type::TypeIs(right)) => left
                .return_type(db)
                .has_relation_to_impl(
                    db,
                    right.return_type(db),
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
                .and(db, || {
                    right.return_type(db).has_relation_to_impl(
                        db,
                        left.return_type(db),
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                }),

            // `TypeIs[T]` is a subtype of `bool`.
            (Type::TypeIs(_), _) => KnownClass::Bool.to_instance(db).has_relation_to_impl(
                db,
                target,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            // Function-like callables are subtypes of `FunctionType`
            (Type::Callable(callable), _) if callable.is_function_like(db) => {
                KnownClass::FunctionType
                    .to_instance(db)
                    .has_relation_to_impl(
                        db,
                        target,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
            }

            (Type::Callable(_), _) => ConstraintSet::from(false),

            (Type::BoundSuper(_), Type::BoundSuper(_)) => {
                self.when_equivalent_to(db, target, inferable)
            }
            (Type::BoundSuper(_), _) => KnownClass::Super.to_instance(db).has_relation_to_impl(
                db,
                target,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            (Type::SubclassOf(subclass_of), _) | (_, Type::SubclassOf(subclass_of))
                if subclass_of.is_type_var() =>
            {
                ConstraintSet::from(false)
            }

            // `Literal[<class 'C'>]` is a subtype of `type[B]` if `C` is a subclass of `B`,
            // since `type[B]` describes all possible runtime subclasses of the class object `B`.
            (Type::ClassLiteral(class), Type::SubclassOf(target_subclass_ty)) => target_subclass_ty
                .subclass_of()
                .into_class(db)
                .map(|subclass_of_class| {
                    class.default_specialization(db).has_relation_to_impl(
                        db,
                        subclass_of_class,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
                .unwrap_or_else(|| ConstraintSet::from(relation.is_assignability())),
            (Type::GenericAlias(alias), Type::SubclassOf(target_subclass_ty)) => target_subclass_ty
                .subclass_of()
                .into_class(db)
                .map(|subclass_of_class| {
                    ClassType::Generic(alias).has_relation_to_impl(
                        db,
                        subclass_of_class,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
                .unwrap_or_else(|| ConstraintSet::from(relation.is_assignability())),

            // This branch asks: given two types `type[T]` and `type[S]`, is `type[T]` a subtype of `type[S]`?
            (Type::SubclassOf(self_subclass_ty), Type::SubclassOf(target_subclass_ty)) => {
                self_subclass_ty.has_relation_to_impl(
                    db,
                    target_subclass_ty,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            // `Literal[str]` is a subtype of `type` because the `str` class object is an instance of its metaclass `type`.
            // `Literal[abc.ABC]` is a subtype of `abc.ABCMeta` because the `abc.ABC` class object
            // is an instance of its metaclass `abc.ABCMeta`.
            (Type::ClassLiteral(class), _) => {
                class.metaclass_instance_type(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }
            (Type::GenericAlias(alias), _) => ClassType::from(alias)
                .metaclass_instance_type(db)
                .has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                ),

            // `type[Any]` is a subtype of `type[object]`, and is assignable to any `type[...]`
            (Type::SubclassOf(subclass_of_ty), other) if subclass_of_ty.is_dynamic() => {
                KnownClass::Type
                    .to_instance(db)
                    .has_relation_to_impl(
                        db,
                        other,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                    .or(db, || {
                        ConstraintSet::from(relation.is_assignability()).and(db, || {
                            other.has_relation_to_impl(
                                db,
                                KnownClass::Type.to_instance(db),
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        })
                    })
            }

            // Any `type[...]` type is assignable to `type[Any]`
            (other, Type::SubclassOf(subclass_of_ty))
                if subclass_of_ty.is_dynamic() && relation.is_assignability() =>
            {
                other.has_relation_to_impl(
                    db,
                    KnownClass::Type.to_instance(db),
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
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
                .into_class(db)
                .map(|class| class.metaclass_instance_type(db))
                .unwrap_or_else(|| KnownClass::Type.to_instance(db))
                .has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                ),

            // For example: `Type::SpecialForm(SpecialFormType::Type)` is a subtype of `Type::NominalInstance(_SpecialForm)`,
            // because `Type::SpecialForm(SpecialFormType::Type)` is a set with exactly one runtime value in it
            // (the symbol `typing.Type`), and that symbol is known to be an instance of `typing._SpecialForm` at runtime.
            (Type::SpecialForm(left), right) => left.instance_fallback(db).has_relation_to_impl(
                db,
                right,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            (Type::KnownInstance(left), right) => left.instance_fallback(db).has_relation_to_impl(
                db,
                right,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            // `bool` is a subtype of `int`, because `bool` subclasses `int`,
            // which means that all instances of `bool` are also instances of `int`
            (Type::NominalInstance(self_instance), Type::NominalInstance(target_instance)) => {
                relation_visitor.visit((self, target, relation), || {
                    self_instance.has_relation_to_impl(
                        db,
                        target_instance,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                })
            }

            (Type::NewTypeInstance(self_newtype), Type::NewTypeInstance(target_newtype)) => {
                self_newtype.has_relation_to_impl(db, target_newtype)
            }

            (
                Type::NewTypeInstance(self_newtype),
                Type::NominalInstance(target_nominal_instance),
            ) => self_newtype.base_class_type(db).has_relation_to_impl(
                db,
                target_nominal_instance.class(db),
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            (Type::PropertyInstance(_), _) => {
                KnownClass::Property.to_instance(db).has_relation_to_impl(
                    db,
                    target,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }
            (_, Type::PropertyInstance(_)) => self.has_relation_to_impl(
                db,
                KnownClass::Property.to_instance(db),
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            // Other than the special cases enumerated above, nominal-instance types, and
            // newtype-instance types are never subtypes of any other variants
            (Type::NominalInstance(_), _) => ConstraintSet::from(false),
            (Type::NewTypeInstance(_), _) => ConstraintSet::from(false),
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
        self.when_equivalent_to(db, other, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    fn when_equivalent_to(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.is_equivalent_to_impl(db, other, inferable, &IsEquivalentVisitor::default())
    }

    pub(crate) fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if self == other {
            return ConstraintSet::from(true);
        }

        match (self, other) {
            // The `Divergent` type is a special type that is not equivalent to other kinds of dynamic types,
            // which prevents `Divergent` from being eliminated during union reduction.
            (Type::Dynamic(_), Type::Dynamic(DynamicType::Divergent(_)))
            | (Type::Dynamic(DynamicType::Divergent(_)), Type::Dynamic(_)) => {
                ConstraintSet::from(false)
            }
            (Type::Dynamic(_), Type::Dynamic(_)) => ConstraintSet::from(true),

            (Type::SubclassOf(first), Type::SubclassOf(second)) => {
                match (first.subclass_of(), second.subclass_of()) {
                    (first, second) if first == second => ConstraintSet::from(true),
                    (SubclassOfInner::Dynamic(_), SubclassOfInner::Dynamic(_)) => {
                        ConstraintSet::from(true)
                    }
                    _ => ConstraintSet::from(false),
                }
            }

            (Type::TypeAlias(self_alias), _) => {
                let self_alias_ty = self_alias.value_type(db).normalized(db);
                visitor.visit((self_alias_ty, other), || {
                    self_alias_ty.is_equivalent_to_impl(db, other, inferable, visitor)
                })
            }

            (_, Type::TypeAlias(other_alias)) => {
                let other_alias_ty = other_alias.value_type(db).normalized(db);
                visitor.visit((self, other_alias_ty), || {
                    self.is_equivalent_to_impl(db, other_alias_ty, inferable, visitor)
                })
            }

            (Type::NewTypeInstance(self_newtype), Type::NewTypeInstance(other_newtype)) => {
                ConstraintSet::from(self_newtype.is_equivalent_to_impl(db, other_newtype))
            }

            (Type::NominalInstance(first), Type::NominalInstance(second)) => {
                first.is_equivalent_to_impl(db, second, inferable, visitor)
            }

            (Type::Union(first), Type::Union(second)) => {
                first.is_equivalent_to_impl(db, second, inferable, visitor)
            }

            (Type::Intersection(first), Type::Intersection(second)) => {
                first.is_equivalent_to_impl(db, second, inferable, visitor)
            }

            (Type::FunctionLiteral(self_function), Type::FunctionLiteral(target_function)) => {
                self_function.is_equivalent_to_impl(db, target_function, inferable, visitor)
            }
            (Type::BoundMethod(self_method), Type::BoundMethod(target_method)) => {
                self_method.is_equivalent_to_impl(db, target_method, inferable, visitor)
            }
            (Type::KnownBoundMethod(self_method), Type::KnownBoundMethod(target_method)) => {
                self_method.is_equivalent_to_impl(db, target_method, inferable, visitor)
            }
            (Type::Callable(first), Type::Callable(second)) => {
                first.is_equivalent_to_impl(db, second, inferable, visitor)
            }

            (Type::ProtocolInstance(first), Type::ProtocolInstance(second)) => {
                first.is_equivalent_to_impl(db, second, inferable, visitor)
            }
            (Type::ProtocolInstance(protocol), nominal @ Type::NominalInstance(n))
            | (nominal @ Type::NominalInstance(n), Type::ProtocolInstance(protocol)) => {
                ConstraintSet::from(n.is_object() && protocol.normalized(db) == nominal)
            }
            // An instance of an enum class is equivalent to an enum literal of that class,
            // if that enum has only has one member.
            (Type::NominalInstance(instance), Type::EnumLiteral(literal))
            | (Type::EnumLiteral(literal), Type::NominalInstance(instance)) => {
                if literal.enum_class_instance(db) != Type::NominalInstance(instance) {
                    return ConstraintSet::from(false);
                }
                ConstraintSet::from(is_single_member_enum(db, instance.class_literal(db)))
            }

            (Type::PropertyInstance(left), Type::PropertyInstance(right)) => {
                left.is_equivalent_to_impl(db, right, inferable, visitor)
            }

            _ => ConstraintSet::from(false),
        }
    }

    /// Return true if `self & other` should simplify to `Never`:
    /// if the intersection of the two types could never be inhabited by any
    /// possible runtime value.
    ///
    /// Our implementation of disjointness for non-fully-static types only
    /// returns true if the *top materialization* of `self` has no overlap with
    /// the *top materialization* of `other`.
    ///
    /// For example, `list[int]` is disjoint from `list[str]`: the two types have
    /// no overlap. But `list[Any]` is not disjoint from `list[str]`: there exists
    /// a fully static materialization of `list[Any]` (`list[str]`) that is a
    /// subtype of `list[str]`
    ///
    /// This function aims to have no false positives, but might return wrong
    /// `false` answers in some cases.
    pub(crate) fn is_disjoint_from(self, db: &'db dyn Db, other: Type<'db>) -> bool {
        self.when_disjoint_from(db, other, InferableTypeVars::None)
            .is_always_satisfied(db)
    }

    fn when_disjoint_from(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.is_disjoint_from_impl(
            db,
            other,
            inferable,
            &IsDisjointVisitor::default(),
            &HasRelationToVisitor::default(),
        )
    }

    pub(crate) fn is_disjoint_from_impl(
        self,
        db: &'db dyn Db,
        other: Type<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
    ) -> ConstraintSet<'db> {
        fn any_protocol_members_absent_or_disjoint<'db>(
            db: &'db dyn Db,
            protocol: ProtocolInstanceType<'db>,
            other: Type<'db>,
            inferable: InferableTypeVars<'_, 'db>,
            disjointness_visitor: &IsDisjointVisitor<'db>,
            relation_visitor: &HasRelationToVisitor<'db>,
        ) -> ConstraintSet<'db> {
            protocol.interface(db).members(db).when_any(db, |member| {
                other
                    .member(db, member.name())
                    .place
                    .ignore_possibly_undefined()
                    .when_none_or(|attribute_type| {
                        member.has_disjoint_type_from(
                            db,
                            attribute_type,
                            inferable,
                            disjointness_visitor,
                            relation_visitor,
                        )
                    })
            })
        }

        match (self, other) {
            (Type::Never, _) | (_, Type::Never) => ConstraintSet::from(true),

            (Type::Dynamic(_), _) | (_, Type::Dynamic(_)) => ConstraintSet::from(false),

            (Type::TypeAlias(alias), _) => {
                let self_alias_ty = alias.value_type(db);
                disjointness_visitor.visit((self, other), || {
                    self_alias_ty.is_disjoint_from_impl(
                        db,
                        other,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            (_, Type::TypeAlias(alias)) => {
                let other_alias_ty = alias.value_type(db);
                disjointness_visitor.visit((self, other), || {
                    self.is_disjoint_from_impl(
                        db,
                        other_alias_ty,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            (Type::TypedDict(_), _) | (_, Type::TypedDict(_)) => {
                // TODO: Implement disjointness for TypedDict
                ConstraintSet::from(false)
            }

            // `type[T]` is disjoint from a callable or protocol instance if its upper bound or constraints are.
            (Type::SubclassOf(subclass_of), Type::Callable(_) | Type::ProtocolInstance(_))
            | (Type::Callable(_) | Type::ProtocolInstance(_), Type::SubclassOf(subclass_of))
                if subclass_of.is_type_var() =>
            {
                let type_var = subclass_of
                    .subclass_of()
                    .with_transposed_type_var(db)
                    .into_type_var()
                    .unwrap();

                Type::TypeVar(type_var).is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }

            // `type[T]` is disjoint from a class object `A` if every instance of `T` is disjoint from an instance of `A`.
            (Type::SubclassOf(subclass_of), other) | (other, Type::SubclassOf(subclass_of))
                if !subclass_of
                    .into_type_var()
                    .zip(other.to_instance(db))
                    .when_none_or(|(this_instance, other_instance)| {
                        Type::TypeVar(this_instance).is_disjoint_from_impl(
                            db,
                            other_instance,
                            inferable,
                            disjointness_visitor,
                            relation_visitor,
                        )
                    })
                    .is_always_satisfied(db) =>
            {
                // TODO: The repetition here isn't great, but we need the fallthrough logic.
                subclass_of
                    .into_type_var()
                    .zip(other.to_instance(db))
                    .when_none_or(|(this_instance, other_instance)| {
                        Type::TypeVar(this_instance).is_disjoint_from_impl(
                            db,
                            other_instance,
                            inferable,
                            disjointness_visitor,
                            relation_visitor,
                        )
                    })
            }

            // A typevar is never disjoint from itself, since all occurrences of the typevar must
            // be specialized to the same type. (This is an important difference between typevars
            // and `Any`!) Different typevars might be disjoint, depending on their bounds and
            // constraints, which are handled below.
            (Type::TypeVar(self_bound_typevar), Type::TypeVar(other_bound_typevar))
                if !self_bound_typevar.is_inferable(db, inferable)
                    && self_bound_typevar.is_same_typevar_as(db, other_bound_typevar) =>
            {
                ConstraintSet::from(false)
            }

            (tvar @ Type::TypeVar(bound_typevar), Type::Intersection(intersection))
            | (Type::Intersection(intersection), tvar @ Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, inferable)
                    && intersection.negative(db).contains(&tvar) =>
            {
                ConstraintSet::from(true)
            }

            // An unbounded typevar is never disjoint from any other type, since it might be
            // specialized to any type. A bounded typevar is not disjoint from its bound, and is
            // only disjoint from other types if its bound is. A constrained typevar is disjoint
            // from a type if all of its constraints are.
            (Type::TypeVar(bound_typevar), other) | (other, Type::TypeVar(bound_typevar))
                if !bound_typevar.is_inferable(db, inferable) =>
            {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => ConstraintSet::from(false),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => bound
                        .is_disjoint_from_impl(
                            db,
                            other,
                            inferable,
                            disjointness_visitor,
                            relation_visitor,
                        ),
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        constraints.elements(db).iter().when_all(db, |constraint| {
                            constraint.is_disjoint_from_impl(
                                db,
                                other,
                                inferable,
                                disjointness_visitor,
                                relation_visitor,
                            )
                        })
                    }
                }
            }

            // TODO: Infer specializations here
            (Type::TypeVar(_), _) | (_, Type::TypeVar(_)) => ConstraintSet::from(false),

            (Type::Union(union), other) | (other, Type::Union(union)) => {
                union.elements(db).iter().when_all(db, |e| {
                    e.is_disjoint_from_impl(
                        db,
                        other,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            // If we have two intersections, we test the positive elements of each one against the other intersection
            // Negative elements need a positive element on the other side in order to be disjoint.
            // This is similar to what would happen if we tried to build a new intersection that combines the two
            (Type::Intersection(self_intersection), Type::Intersection(other_intersection)) => {
                disjointness_visitor.visit((self, other), || {
                    self_intersection
                        .positive(db)
                        .iter()
                        .when_any(db, |p| {
                            p.is_disjoint_from_impl(
                                db,
                                other,
                                inferable,
                                disjointness_visitor,
                                relation_visitor,
                            )
                        })
                        .or(db, || {
                            other_intersection.positive(db).iter().when_any(db, |p| {
                                p.is_disjoint_from_impl(
                                    db,
                                    self,
                                    inferable,
                                    disjointness_visitor,
                                    relation_visitor,
                                )
                            })
                        })
                })
            }

            (Type::Intersection(intersection), non_intersection)
            | (non_intersection, Type::Intersection(intersection)) => {
                disjointness_visitor.visit((self, other), || {
                    intersection
                        .positive(db)
                        .iter()
                        .when_any(db, |p| {
                            p.is_disjoint_from_impl(
                                db,
                                non_intersection,
                                inferable,
                                disjointness_visitor,
                                relation_visitor,
                            )
                        })
                        // A & B & Not[C] is disjoint from C
                        .or(db, || {
                            intersection.negative(db).iter().when_any(db, |&neg_ty| {
                                non_intersection.has_relation_to_impl(
                                    db,
                                    neg_ty,
                                    inferable,
                                    TypeRelation::Subtyping,
                                    relation_visitor,
                                    disjointness_visitor,
                                )
                            })
                        })
                })
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
                | Type::KnownBoundMethod(..)
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
                | Type::KnownBoundMethod(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..)
                | Type::ClassLiteral(..)
                | Type::GenericAlias(..)
                | Type::SpecialForm(..)
                | Type::KnownInstance(..)),
            ) => ConstraintSet::from(left != right),

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
                | Type::KnownBoundMethod(..)
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
                | Type::KnownBoundMethod(..)
                | Type::WrapperDescriptor(..)
                | Type::ModuleLiteral(..),
                Type::SubclassOf(_),
            ) => ConstraintSet::from(true),

            (Type::AlwaysTruthy, ty) | (ty, Type::AlwaysTruthy) => {
                // `Truthiness::Ambiguous` may include `AlwaysTrue` as a subset, so it's not guaranteed to be disjoint.
                // Thus, they are only disjoint if `ty.bool() == AlwaysFalse`.
                ConstraintSet::from(ty.bool(db).is_always_false())
            }
            (Type::AlwaysFalsy, ty) | (ty, Type::AlwaysFalsy) => {
                // Similarly, they are only disjoint if `ty.bool() == AlwaysTrue`.
                ConstraintSet::from(ty.bool(db).is_always_true())
            }

            (Type::ProtocolInstance(left), Type::ProtocolInstance(right)) => disjointness_visitor
                .visit((self, other), || {
                    left.is_disjoint_from_impl(db, right, inferable, disjointness_visitor)
                }),

            (Type::ProtocolInstance(protocol), Type::SpecialForm(special_form))
            | (Type::SpecialForm(special_form), Type::ProtocolInstance(protocol)) => {
                disjointness_visitor.visit((self, other), || {
                    any_protocol_members_absent_or_disjoint(
                        db,
                        protocol,
                        special_form.instance_fallback(db),
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            (Type::ProtocolInstance(protocol), Type::KnownInstance(known_instance))
            | (Type::KnownInstance(known_instance), Type::ProtocolInstance(protocol)) => {
                disjointness_visitor.visit((self, other), || {
                    any_protocol_members_absent_or_disjoint(
                        db,
                        protocol,
                        known_instance.instance_fallback(db),
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
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
                | Type::EnumLiteral(..)),
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
            ) => disjointness_visitor.visit((self, other), || {
                any_protocol_members_absent_or_disjoint(
                    db,
                    protocol,
                    ty,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }),

            // This is the same as the branch above --
            // once guard patterns are stabilised, it could be unified with that branch
            // (<https://github.com/rust-lang/rust/issues/129967>)
            (Type::ProtocolInstance(protocol), nominal @ Type::NominalInstance(n))
            | (nominal @ Type::NominalInstance(n), Type::ProtocolInstance(protocol))
                if n.class(db).is_final(db) =>
            {
                disjointness_visitor.visit((self, other), || {
                    any_protocol_members_absent_or_disjoint(
                        db,
                        protocol,
                        nominal,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                })
            }

            (Type::ProtocolInstance(protocol), other)
            | (other, Type::ProtocolInstance(protocol)) => {
                disjointness_visitor.visit((self, other), || {
                    protocol.interface(db).members(db).when_any(db, |member| {
                        match other.member(db, member.name()).place {
                            Place::Defined(attribute_type, _, _) => member.has_disjoint_type_from(
                                db,
                                attribute_type,
                                inferable,
                                disjointness_visitor,
                                relation_visitor,
                            ),
                            Place::Undefined => ConstraintSet::from(false),
                        }
                    })
                })
            }

            (Type::SubclassOf(subclass_of_ty), _) | (_, Type::SubclassOf(subclass_of_ty))
                if subclass_of_ty.is_type_var() =>
            {
                ConstraintSet::from(true)
            }

            (Type::SubclassOf(subclass_of_ty), Type::ClassLiteral(class_b))
            | (Type::ClassLiteral(class_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => ConstraintSet::from(false),
                    SubclassOfInner::Class(class_a) => {
                        class_b.when_subclass_of(db, None, class_a).negate(db)
                    }
                    SubclassOfInner::TypeVar(_) => unreachable!(),
                }
            }

            (Type::SubclassOf(subclass_of_ty), Type::GenericAlias(alias_b))
            | (Type::GenericAlias(alias_b), Type::SubclassOf(subclass_of_ty)) => {
                match subclass_of_ty.subclass_of() {
                    SubclassOfInner::Dynamic(_) => ConstraintSet::from(false),
                    SubclassOfInner::Class(class_a) => ClassType::from(alias_b)
                        .when_subclass_of(db, class_a, inferable)
                        .negate(db),
                    SubclassOfInner::TypeVar(_) => unreachable!(),
                }
            }

            (Type::SubclassOf(left), Type::SubclassOf(right)) => {
                left.is_disjoint_from_impl(db, right, inferable, disjointness_visitor)
            }

            // for `type[Any]`/`type[Unknown]`/`type[Todo]`, we know the type cannot be any larger than `type`,
            // so although the type is dynamic we can still determine disjointedness in some situations
            (Type::SubclassOf(subclass_of_ty), other)
            | (other, Type::SubclassOf(subclass_of_ty)) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Dynamic(_) => {
                    KnownClass::Type.to_instance(db).is_disjoint_from_impl(
                        db,
                        other,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                }
                SubclassOfInner::Class(class) => {
                    class.metaclass_instance_type(db).is_disjoint_from_impl(
                        db,
                        other,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                }
                SubclassOfInner::TypeVar(_) => unreachable!(),
            },

            (Type::SpecialForm(special_form), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::SpecialForm(special_form)) => {
                ConstraintSet::from(!special_form.is_instance_of(db, instance.class(db)))
            }

            (Type::KnownInstance(known_instance), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::KnownInstance(known_instance)) => {
                ConstraintSet::from(!known_instance.is_instance_of(db, instance.class(db)))
            }

            (Type::BooleanLiteral(..) | Type::TypeIs(_), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::BooleanLiteral(..) | Type::TypeIs(_)) => {
                // A `Type::BooleanLiteral()` must be an instance of exactly `bool`
                // (it cannot be an instance of a `bool` subclass)
                KnownClass::Bool
                    .when_subclass_of(db, instance.class(db))
                    .negate(db)
            }

            (Type::BooleanLiteral(..) | Type::TypeIs(_), _)
            | (_, Type::BooleanLiteral(..) | Type::TypeIs(_)) => ConstraintSet::from(true),

            (Type::IntLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::IntLiteral(..)) => {
                // A `Type::IntLiteral()` must be an instance of exactly `int`
                // (it cannot be an instance of an `int` subclass)
                KnownClass::Int
                    .when_subclass_of(db, instance.class(db))
                    .negate(db)
            }

            (Type::IntLiteral(..), _) | (_, Type::IntLiteral(..)) => ConstraintSet::from(true),

            (Type::StringLiteral(..), Type::LiteralString)
            | (Type::LiteralString, Type::StringLiteral(..)) => ConstraintSet::from(false),

            (Type::StringLiteral(..) | Type::LiteralString, Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::StringLiteral(..) | Type::LiteralString) => {
                // A `Type::StringLiteral()` or a `Type::LiteralString` must be an instance of exactly `str`
                // (it cannot be an instance of a `str` subclass)
                KnownClass::Str
                    .when_subclass_of(db, instance.class(db))
                    .negate(db)
            }

            (Type::LiteralString, Type::LiteralString) => ConstraintSet::from(false),
            (Type::LiteralString, _) | (_, Type::LiteralString) => ConstraintSet::from(true),

            (Type::BytesLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::BytesLiteral(..)) => {
                // A `Type::BytesLiteral()` must be an instance of exactly `bytes`
                // (it cannot be an instance of a `bytes` subclass)
                KnownClass::Bytes
                    .when_subclass_of(db, instance.class(db))
                    .negate(db)
            }

            (Type::EnumLiteral(enum_literal), instance @ Type::NominalInstance(_))
            | (instance @ Type::NominalInstance(_), Type::EnumLiteral(enum_literal)) => {
                enum_literal
                    .enum_class_instance(db)
                    .has_relation_to_impl(
                        db,
                        instance,
                        inferable,
                        TypeRelation::Subtyping,
                        relation_visitor,
                        disjointness_visitor,
                    )
                    .negate(db)
            }
            (Type::EnumLiteral(..), _) | (_, Type::EnumLiteral(..)) => ConstraintSet::from(true),

            // A class-literal type `X` is always disjoint from an instance type `Y`,
            // unless the type expressing "all instances of `Z`" is a subtype of of `Y`,
            // where `Z` is `X`'s metaclass.
            (Type::ClassLiteral(class), instance @ Type::NominalInstance(_))
            | (instance @ Type::NominalInstance(_), Type::ClassLiteral(class)) => class
                .metaclass_instance_type(db)
                .when_subtype_of(db, instance, inferable)
                .negate(db),
            (Type::GenericAlias(alias), instance @ Type::NominalInstance(_))
            | (instance @ Type::NominalInstance(_), Type::GenericAlias(alias)) => {
                ClassType::from(alias)
                    .metaclass_instance_type(db)
                    .has_relation_to_impl(
                        db,
                        instance,
                        inferable,
                        TypeRelation::Subtyping,
                        relation_visitor,
                        disjointness_visitor,
                    )
                    .negate(db)
            }

            (Type::FunctionLiteral(..), Type::NominalInstance(instance))
            | (Type::NominalInstance(instance), Type::FunctionLiteral(..)) => {
                // A `Type::FunctionLiteral()` must be an instance of exactly `types.FunctionType`
                // (it cannot be an instance of a `types.FunctionType` subclass)
                KnownClass::FunctionType
                    .when_subclass_of(db, instance.class(db))
                    .negate(db)
            }

            (Type::BoundMethod(_), other) | (other, Type::BoundMethod(_)) => KnownClass::MethodType
                .to_instance(db)
                .is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                ),

            (Type::KnownBoundMethod(method), other) | (other, Type::KnownBoundMethod(method)) => {
                method.class().to_instance(db).is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }

            (Type::WrapperDescriptor(_), other) | (other, Type::WrapperDescriptor(_)) => {
                KnownClass::WrapperDescriptorType
                    .to_instance(db)
                    .is_disjoint_from_impl(
                        db,
                        other,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
            }

            (Type::Callable(_) | Type::FunctionLiteral(_), Type::Callable(_))
            | (Type::Callable(_), Type::FunctionLiteral(_)) => {
                // No two callable types are ever disjoint because
                // `(*args: object, **kwargs: object) -> Never` is a subtype of all fully static
                // callable types.
                ConstraintSet::from(false)
            }

            (Type::Callable(_), Type::StringLiteral(_) | Type::BytesLiteral(_))
            | (Type::StringLiteral(_) | Type::BytesLiteral(_), Type::Callable(_)) => {
                // A callable type is disjoint from other literal types. For example,
                // `Type::StringLiteral` must be an instance of exactly `str`, not a subclass
                // of `str`, and `str` is not callable. The same applies to other literal types.
                ConstraintSet::from(true)
            }

            (Type::Callable(_), Type::SpecialForm(special_form))
            | (Type::SpecialForm(special_form), Type::Callable(_)) => {
                // A callable type is disjoint from special form types, except for special forms
                // that are callable (like TypedDict and collection constructors).
                // Most special forms are type constructors/annotations (like `typing.Literal`,
                // `typing.Union`, etc.) that are subscripted, not called.
                ConstraintSet::from(!special_form.is_callable())
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
                .ignore_possibly_undefined()
                .when_none_or(|dunder_call| {
                    dunder_call
                        .has_relation_to_impl(
                            db,
                            Type::Callable(CallableType::unknown(db)),
                            inferable,
                            TypeRelation::Assignability,
                            relation_visitor,
                            disjointness_visitor,
                        )
                        .negate(db)
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
                ConstraintSet::from(false)
            }

            (Type::ModuleLiteral(..), other @ Type::NominalInstance(..))
            | (other @ Type::NominalInstance(..), Type::ModuleLiteral(..)) => {
                // Modules *can* actually be instances of `ModuleType` subclasses
                other.is_disjoint_from_impl(
                    db,
                    KnownClass::ModuleType.to_instance(db),
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }

            (Type::NominalInstance(left), Type::NominalInstance(right)) => disjointness_visitor
                .visit((self, other), || {
                    left.is_disjoint_from_impl(
                        db,
                        right,
                        inferable,
                        disjointness_visitor,
                        relation_visitor,
                    )
                }),

            (Type::NewTypeInstance(left), Type::NewTypeInstance(right)) => {
                left.is_disjoint_from_impl(db, right)
            }
            (Type::NewTypeInstance(newtype), other) | (other, Type::NewTypeInstance(newtype)) => {
                Type::instance(db, newtype.base_class_type(db)).is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }

            (Type::PropertyInstance(_), other) | (other, Type::PropertyInstance(_)) => {
                KnownClass::Property.to_instance(db).is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }

            (Type::BoundSuper(_), Type::BoundSuper(_)) => {
                self.when_equivalent_to(db, other, inferable).negate(db)
            }
            (Type::BoundSuper(_), other) | (other, Type::BoundSuper(_)) => {
                KnownClass::Super.to_instance(db).is_disjoint_from_impl(
                    db,
                    other,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                )
            }
        }
    }

    /// Recursively visit the specialization of a generic class instance.
    ///
    /// The provided closure will be called with each assignment of a type variable present in this
    /// type, along with the variance of the outermost type with respect to the type variable.
    ///
    /// If a `TypeContext` is provided, it will be narrowed as nested types are visited, if the
    /// type is a specialized instance of the same class.
    pub(crate) fn visit_specialization<F>(self, db: &'db dyn Db, tcx: TypeContext<'db>, mut f: F)
    where
        F: FnMut(BoundTypeVarInstance<'db>, Type<'db>, TypeVarVariance, TypeContext<'db>),
    {
        self.visit_specialization_impl(
            db,
            tcx,
            TypeVarVariance::Covariant,
            &mut f,
            &SpecializationVisitor::default(),
        );
    }

    fn visit_specialization_impl(
        self,
        db: &'db dyn Db,
        tcx: TypeContext<'db>,
        polarity: TypeVarVariance,
        f: &mut dyn FnMut(BoundTypeVarInstance<'db>, Type<'db>, TypeVarVariance, TypeContext<'db>),
        visitor: &SpecializationVisitor<'db>,
    ) {
        let Type::NominalInstance(instance) = self else {
            match self {
                Type::Union(union) => {
                    for element in union.elements(db) {
                        element.visit_specialization_impl(db, tcx, polarity, f, visitor);
                    }
                }
                Type::Intersection(intersection) => {
                    for element in intersection.positive(db) {
                        element.visit_specialization_impl(db, tcx, polarity, f, visitor);
                    }
                }
                Type::TypeAlias(alias) => visitor.visit(self, || {
                    alias
                        .value_type(db)
                        .visit_specialization_impl(db, tcx, polarity, f, visitor);
                }),
                _ => {}
            }

            return;
        };

        let (class_literal, Some(specialization)) = instance.class(db).class_literal(db) else {
            return;
        };

        let tcx_specialization = tcx.annotation.and_then(|tcx| {
            tcx.filter_union(db, |ty| ty.specialization_of(db, class_literal).is_some())
                .specialization_of(db, class_literal)
        });

        for (typevar, ty) in specialization
            .generic_context(db)
            .variables(db)
            .zip(specialization.types(db))
        {
            let variance = typevar.variance_with_polarity(db, polarity);
            let tcx = TypeContext::new(tcx_specialization.and_then(|spec| spec.get(db, typevar)));

            f(typevar, *ty, variance, tcx);

            visitor.visit(*ty, || {
                ty.visit_specialization_impl(db, tcx, variance, f, visitor);
            });
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
            Type::TypedDict(_) => false,
            Type::TypeAlias(alias) => alias.value_type(db).is_singleton(db),
            Type::NewTypeInstance(newtype) => {
                Type::instance(db, newtype.base_class_type(db)).is_singleton(db)
            }
        }
    }

    /// Return true if this type is non-empty and all inhabitants of this type compare equal.
    pub(crate) fn is_single_valued(self, db: &'db dyn Db) -> bool {
        match self {
            Type::FunctionLiteral(..)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(_)
            | Type::ModuleLiteral(..)
            | Type::ClassLiteral(..)
            | Type::GenericAlias(..)
            | Type::IntLiteral(..)
            | Type::BooleanLiteral(..)
            | Type::StringLiteral(..)
            | Type::BytesLiteral(..)
            | Type::SpecialForm(..)
            | Type::KnownInstance(..) => true,

            Type::EnumLiteral(_) => !self.overrides_equality(db),

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
            Type::NewTypeInstance(newtype) => {
                Type::instance(db, newtype.base_class_type(db)).is_single_valued(db)
            }

            Type::BoundSuper(_) => {
                // At runtime two super instances never compare equal, even if their arguments are identical.
                false
            }

            Type::TypeIs(type_is) => type_is.is_bound(db),

            Type::TypeAlias(alias) => alias.value_type(db).is_single_valued(db),

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

                    _ => Some(class.class_member(db, name, policy)),
                }
            }

            Type::GenericAlias(alias) if alias.is_typed_dict(db) => {
                Some(alias.origin(db).typed_dict_member(db, None, name, policy))
            }

            Type::GenericAlias(alias) => {
                let attr = Some(ClassType::from(*alias).class_member(db, name, policy));
                match alias.specialization(db).materialization_kind(db) {
                    None => attr,
                    Some(materialization_kind) => attr.map(|attr| {
                        attr.materialize(
                            db,
                            materialization_kind,
                            &ApplyTypeMappingVisitor::default(),
                        )
                    }),
                }
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
            Type::NewTypeInstance(newtype) => newtype.base_class_type(db).instance_member(db, name),

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
        } else if let place @ Place::Defined(_, _, _) = self.class_member(db, name.into()).place {
            place
        } else if let Some(place @ Place::Defined(_, _, _)) =
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
                // For "function-like" callables, model the behavior of `FunctionType.__get__`.
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
                        Type::Callable(callable.bind_self(db, None)),
                        AttributeKind::NormalOrNonDataDescriptor,
                    ))
                };
            }
            _ => {}
        }

        let descr_get = self.class_member(db, "__get__".into()).place;

        if let Place::Defined(descr_get, _, descr_get_boundness) = descr_get {
            let return_ty = descr_get
                .try_call(db, &CallArguments::positional([self, instance, owner]))
                .map(|bindings| {
                    if descr_get_boundness == Definedness::AlwaysDefined {
                        bindings.return_type(db)
                    } else {
                        UnionType::from_elements(db, [bindings.return_type(db), self])
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
                place: Place::Defined(Type::Dynamic(_) | Type::Never, _, _),
                qualifiers: _,
            } => (attribute, AttributeKind::DataDescriptor),

            PlaceAndQualifiers {
                place: Place::Defined(Type::Union(union), origin, boundness),
                qualifiers,
            } => (
                union
                    .map_with_boundness(db, |elem| {
                        Place::Defined(
                            elem.try_call_dunder_get(db, instance, owner)
                                .map_or(*elem, |(ty, _)| ty),
                            origin,
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
                place: Place::Defined(Type::Intersection(intersection), origin, boundness),
                qualifiers,
            } => (
                intersection
                    .map_with_boundness(db, |elem| {
                        Place::Defined(
                            elem.try_call_dunder_get(db, instance, owner)
                                .map_or(*elem, |(ty, _)| ty),
                            origin,
                            boundness,
                        )
                    })
                    .with_qualifiers(qualifiers),
                // TODO: Discover data descriptors in intersections.
                AttributeKind::NormalOrNonDataDescriptor,
            ),

            PlaceAndQualifiers {
                place: Place::Defined(attribute_ty, origin, boundness),
                qualifiers: _,
            } => {
                if let Some((return_ty, attribute_kind)) =
                    attribute_ty.try_call_dunder_get(db, instance, owner)
                {
                    (
                        Place::Defined(return_ty, origin, boundness).into(),
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
            (meta_attr @ Place::Defined(_, _, _), _, Place::Undefined) => {
                meta_attr.with_qualifiers(meta_attr_qualifiers)
            }

            // `meta_attr` is the return type of a data descriptor and definitely bound, so we
            // return it.
            (
                meta_attr @ Place::Defined(_, _, Definedness::AlwaysDefined),
                AttributeKind::DataDescriptor,
                _,
            ) => meta_attr.with_qualifiers(meta_attr_qualifiers),

            // `meta_attr` is the return type of a data descriptor, but the attribute on the
            // meta-type is possibly-unbound. This means that we "fall through" to the next
            // stage of the descriptor protocol and union with the fallback type.
            (
                Place::Defined(meta_attr_ty, meta_origin, Definedness::PossiblyUndefined),
                AttributeKind::DataDescriptor,
                Place::Defined(fallback_ty, fallback_origin, fallback_boundness),
            ) => Place::Defined(
                UnionType::from_elements(db, [meta_attr_ty, fallback_ty]),
                meta_origin.merge(fallback_origin),
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
                Place::Defined(_, _, _),
                AttributeKind::NormalOrNonDataDescriptor,
                fallback @ Place::Defined(_, _, Definedness::AlwaysDefined),
            ) if policy == InstanceFallbackShadowsNonDataDescriptor::Yes => {
                fallback.with_qualifiers(fallback_qualifiers)
            }

            // `meta_attr` is *not* a data descriptor. The `fallback` symbol is either possibly
            // unbound or the policy argument is `No`. In both cases, the `fallback` type does
            // not completely shadow the non-data descriptor, so we build a union of the two.
            (
                Place::Defined(meta_attr_ty, meta_origin, meta_attr_boundness),
                AttributeKind::NormalOrNonDataDescriptor,
                Place::Defined(fallback_ty, fallback_origin, fallback_boundness),
            ) => Place::Defined(
                UnionType::from_elements(db, [meta_attr_ty, fallback_ty]),
                meta_origin.merge(fallback_origin),
                meta_attr_boundness.max(fallback_boundness),
            )
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
    #[salsa::tracked(cycle_fn=member_lookup_cycle_recover, cycle_initial=member_lookup_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn member_lookup_with_policy(
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
            Type::Union(union) => union.map_with_boundness_and_qualifiers(db, |elem| {
                elem.member_lookup_with_policy(db, name_str.into(), policy)
            }),

            Type::Intersection(intersection) => intersection
                .map_with_boundness_and_qualifiers(db, |elem| {
                    elem.member_lookup_with_policy(db, name_str.into(), policy)
                }),

            Type::Dynamic(..) | Type::Never => Place::bound(self).into(),

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
            Type::StringLiteral(literal) if name == "startswith" => Place::bound(
                Type::KnownBoundMethod(KnownBoundMethodType::StrStartswith(literal)),
            )
            .into(),

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
            Type::KnownInstance(KnownInstanceType::GenericContext(tracked))
                if name == "specialize_constrained" =>
            {
                Place::bound(Type::KnownBoundMethod(
                    KnownBoundMethodType::GenericContextSpecializeConstrained(tracked),
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

            _ if policy.no_instance_fallback() => self.invoke_descriptor_protocol(
                db,
                name_str,
                Place::Undefined.into(),
                InstanceFallbackShadowsNonDataDescriptor::No,
                policy,
            ),

            Type::TypeAlias(alias) => alias
                .value_type(db)
                .member_lookup_with_policy(db, name, policy),

            Type::EnumLiteral(enum_literal)
                if matches!(name_str, "name" | "_name_")
                    && Type::ClassLiteral(enum_literal.enum_class(db))
                        .is_subtype_of(db, KnownClass::Enum.to_subclass_of(db)) =>
            {
                Place::bound(Type::StringLiteral(StringLiteralType::new(
                    db,
                    enum_literal.name(db).as_str(),
                )))
                .into()
            }

            Type::EnumLiteral(enum_literal)
                if matches!(name_str, "value" | "_value_")
                    && Type::ClassLiteral(enum_literal.enum_class(db))
                        .is_subtype_of(db, KnownClass::Enum.to_subclass_of(db)) =>
            {
                enum_metadata(db, enum_literal.enum_class(db))
                    .and_then(|metadata| metadata.members.get(enum_literal.name(db)))
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
                if matches!(name_str, "value" | "_value_")
                    && is_single_member_enum(db, instance.class(db).class_literal(db).0) =>
            {
                enum_metadata(db, instance.class(db).class_literal(db).0)
                    .and_then(|metadata| metadata.members.get_index(0).map(|(_, v)| *v))
                    .map_or(Place::Undefined, Place::bound)
                    .into()
            }

            Type::NominalInstance(..)
            | Type::ProtocolInstance(..)
            | Type::NewTypeInstance(..)
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
                    if policy.no_getattr_lookup() {
                        return Place::Undefined.into();
                    }

                    self.try_call_dunder(
                        db,
                        "__getattr__",
                        CallArguments::positional([Type::string_literal(db, &name)]),
                        TypeContext::default(),
                    )
                    .map(|outcome| Place::bound(outcome.return_type(db)))
                    // TODO: Handle call errors here.
                    .unwrap_or(Place::Undefined)
                    .into()
                };

                let custom_getattribute_result = || {
                    // Avoid cycles when looking up `__getattribute__`
                    if "__getattribute__" == name.as_str() {
                        return Place::Undefined.into();
                    }

                    // Typeshed has a `__getattribute__` method defined on `builtins.object` so we
                    // explicitly hide it here using `MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK`.
                    self.try_call_dunder_with_policy(
                        db,
                        "__getattribute__",
                        &mut CallArguments::positional([Type::string_literal(db, &name)]),
                        TypeContext::default(),
                        MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                    )
                    .map(|outcome| Place::bound(outcome.return_type(db)))
                    // TODO: Handle call errors here.
                    .unwrap_or(Place::Undefined)
                    .into()
                };

                if result.is_class_var() && self.is_typed_dict() {
                    // `ClassVar`s on `TypedDictFallback` cannot be accessed on inhabitants of `SomeTypedDict`.
                    // They can only be accessed on `SomeTypedDict` directly.
                    return Place::Undefined.into();
                }

                match result {
                    member @ PlaceAndQualifiers {
                        place: Place::Defined(_, _, Definedness::AlwaysDefined),
                        qualifiers: _,
                    } => member,
                    member @ PlaceAndQualifiers {
                        place: Place::Defined(_, _, Definedness::PossiblyUndefined),
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

            Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..) => {
                if let Some(enum_class) = match self {
                    Type::ClassLiteral(literal) => Some(literal),
                    Type::SubclassOf(subclass_of) => subclass_of
                        .subclass_of()
                        .into_class(db)
                        .map(|class| class.class_literal(db).0),
                    _ => None,
                } {
                    if let Some(metadata) = enum_metadata(db, enum_class) {
                        if let Some(resolved_name) = metadata.resolve_member(&name) {
                            return Place::bound(Type::EnumLiteral(EnumLiteralType::new(
                                db,
                                enum_class,
                                resolved_name,
                            )))
                            .into();
                        }
                    }
                }

                let class_attr_plain = self.find_name_in_mro_with_policy(db, name_str, policy).expect(
                    "Calling `find_name_in_mro` on class literals and subclass-of types should always return `Some`",
                );

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
                    .try_call_dunder_get_on_attribute(db, owner_attr)
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
        self.try_bool_impl(db, true, &TryBoolVisitor::new(Ok(Truthiness::Ambiguous)))
            .unwrap_or_else(|err| err.fallback_truthiness())
    }

    /// Resolves the boolean value of a type.
    ///
    /// This is used to determine the value that would be returned
    /// when `bool(x)` is called on an object `x`.
    ///
    /// Returns an error if the type doesn't implement `__bool__` correctly.
    pub(crate) fn try_bool(&self, db: &'db dyn Db) -> Result<Truthiness, BoolError<'db>> {
        self.try_bool_impl(db, false, &TryBoolVisitor::new(Ok(Truthiness::Ambiguous)))
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
        visitor: &TryBoolVisitor<'db>,
    ) -> Result<Truthiness, BoolError<'db>> {
        let type_to_truthiness = |ty| {
            match ty {
                Type::BooleanLiteral(bool_val) => Truthiness::from(bool_val),
                Type::IntLiteral(int_val) => Truthiness::from(int_val != 0),
                // anything else is handled lower down
                _ => Truthiness::Ambiguous,
            }
        };

        let try_dunders = || {
            match self.try_call_dunder(
                db,
                "__bool__",
                CallArguments::none(),
                TypeContext::default(),
            ) {
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

                    // Don't trust possibly missing `__bool__` method.
                    Ok(Truthiness::Ambiguous)
                }

                Err(CallDunderError::MethodNotAvailable) => {
                    // We only consider `__len__` for tuples and `@final` types,
                    // since `__bool__` takes precedence
                    // and a subclass could add a `__bool__` method.
                    //
                    // TODO: with regards to tuple types, we intend to emit a diagnostic
                    // if a tuple subclass defines a `__bool__` method with a return type
                    // that is inconsistent with the tuple's length. Otherwise, the special
                    // handling for tuples here isn't sound.
                    if let Some(instance) = self.as_nominal_instance() {
                        if let Some(tuple_spec) = instance.tuple_spec(db) {
                            Ok(tuple_spec.truthiness())
                        } else if instance.class(db).is_final(db) {
                            match self.try_call_dunder(
                                db,
                                "__len__",
                                CallArguments::none(),
                                TypeContext::default(),
                            ) {
                                Ok(outcome) => {
                                    let return_type = outcome.return_type(db);
                                    if return_type.is_assignable_to(
                                        db,
                                        KnownClass::SupportsIndex.to_instance(db),
                                    ) {
                                        Ok(type_to_truthiness(return_type))
                                    } else {
                                        // TODO: should report a diagnostic similar to if return type of `__bool__`
                                        // is not assignable to `bool`
                                        Ok(Truthiness::Ambiguous)
                                    }
                                }
                                // if a `@final` type does not define `__bool__` or `__len__`, it is always truthy
                                Err(CallDunderError::MethodNotAvailable) => {
                                    Ok(Truthiness::AlwaysTrue)
                                }
                                // TODO: errors during a `__len__` call (if `__len__` exists) should be reported
                                // as diagnostics similar to errors during a `__bool__` call (when `__bool__` exists)
                                Err(_) => Ok(Truthiness::Ambiguous),
                            }
                        } else {
                            Ok(Truthiness::Ambiguous)
                        }
                    } else {
                        Ok(Truthiness::Ambiguous)
                    }
                }

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
                let element_truthiness =
                    match element.try_bool_impl(db, allow_short_circuit, visitor) {
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

            Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked_set)) => {
                let constraints = tracked_set.constraints(db);
                Truthiness::from(constraints.is_always_satisfied(db))
            }

            Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::PropertyInstance(_)
            | Type::BoundSuper(_)
            | Type::KnownInstance(_)
            | Type::SpecialForm(_)
            | Type::AlwaysTruthy => Truthiness::AlwaysTrue,

            Type::AlwaysFalsy => Truthiness::AlwaysFalse,

            Type::ClassLiteral(class) => {
                class
                    .metaclass_instance_type(db)
                    .try_bool_impl(db, allow_short_circuit, visitor)?
            }
            Type::GenericAlias(alias) => ClassType::from(*alias)
                .metaclass_instance_type(db)
                .try_bool_impl(db, allow_short_circuit, visitor)?,

            Type::SubclassOf(subclass_of_ty) => {
                match subclass_of_ty.subclass_of().with_transposed_type_var(db) {
                    SubclassOfInner::Dynamic(_) => Truthiness::Ambiguous,
                    SubclassOfInner::Class(class) => {
                        Type::from(class).try_bool_impl(db, allow_short_circuit, visitor)?
                    }
                    SubclassOfInner::TypeVar(bound_typevar) => Type::TypeVar(bound_typevar)
                        .try_bool_impl(db, allow_short_circuit, visitor)?,
                }
            }

            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => Truthiness::Ambiguous,
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.try_bool_impl(db, allow_short_circuit, visitor)?
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .as_type(db)
                        .try_bool_impl(db, allow_short_circuit, visitor)?,
                }
            }

            Type::NominalInstance(instance) => instance
                .known_class(db)
                .and_then(KnownClass::bool)
                .map(Ok)
                .unwrap_or_else(try_dunders)?,

            Type::ProtocolInstance(_) => try_dunders()?,

            Type::Union(union) => try_union(*union)?,

            Type::Intersection(_) => {
                // TODO
                Truthiness::Ambiguous
            }

            Type::EnumLiteral(enum_type) => {
                enum_type
                    .enum_class_instance(db)
                    .try_bool_impl(db, allow_short_circuit, visitor)?
            }

            Type::IntLiteral(num) => Truthiness::from(*num != 0),
            Type::BooleanLiteral(bool) => Truthiness::from(*bool),
            Type::StringLiteral(str) => Truthiness::from(!str.value(db).is_empty()),
            Type::BytesLiteral(bytes) => Truthiness::from(!bytes.value(db).is_empty()),
            Type::TypeAlias(alias) => visitor.visit(*self, || {
                alias
                    .value_type(db)
                    .try_bool_impl(db, allow_short_circuit, visitor)
            })?,
            Type::NewTypeInstance(newtype) => Type::instance(db, newtype.base_class_type(db))
                .try_bool_impl(db, allow_short_circuit, visitor)?,
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

        let return_ty = match self.try_call_dunder(
            db,
            "__len__",
            CallArguments::none(),
            TypeContext::default(),
        ) {
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
                    None,
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
                        Some(KnownClass::ConstraintSet.to_instance(db)),
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
                            Some(KnownClass::Bool.to_instance(db)),
                        ),
                    )
                    .into()
                }

                Some(KnownFunction::AssertType) => {
                    let val_ty =
                        BoundTypeVarInstance::synthetic(db, "T", TypeVarVariance::Invariant);

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
                            Some(Type::TypeVar(val_ty)),
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
                            Some(Type::none(db)),
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
                                Parameters::new(
                                    db,
                                    [Parameter::positional_only(Some(Name::new_static("cls")))
                                        .with_annotated_type(Type::none(db))],
                                ),
                                None,
                            ),
                            // def dataclass(cls: type[_T], /) -> type[_T]: ...
                            Signature::new(
                                Parameters::new(
                                    db,
                                    [Parameter::positional_only(Some(Name::new_static("cls")))
                                        .with_annotated_type(KnownClass::Type.to_instance(db))],
                                ),
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
                                Parameters::new(
                                    db,
                                    [
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
                                    ],
                                ),
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
                            Parameters::new(
                                db,
                                [Parameter::positional_only(Some(Name::new_static("o")))
                                    .with_annotated_type(Type::any())
                                    .with_default_type(Type::BooleanLiteral(false))],
                            ),
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
                                Parameters::new(
                                    db,
                                    [Parameter::positional_or_keyword(Name::new_static("object"))
                                        .with_annotated_type(Type::object())
                                        .with_default_type(Type::string_literal(db, ""))],
                                ),
                                Some(KnownClass::Str.to_instance(db)),
                            ),
                            Signature::new(
                                Parameters::new(
                                    db,
                                    [
                                        Parameter::positional_or_keyword(Name::new_static(
                                            "object",
                                        ))
                                        // TODO: Should be `ReadableBuffer` instead of this union type:
                                        .with_annotated_type(UnionType::from_elements(
                                            db,
                                            [
                                                KnownClass::Bytes.to_instance(db),
                                                KnownClass::Bytearray.to_instance(db),
                                            ],
                                        ))
                                        .with_default_type(Type::bytes_literal(db, b"")),
                                        Parameter::positional_or_keyword(Name::new_static(
                                            "encoding",
                                        ))
                                        .with_annotated_type(KnownClass::Str.to_instance(db))
                                        .with_default_type(Type::string_literal(db, "utf-8")),
                                        Parameter::positional_or_keyword(Name::new_static(
                                            "errors",
                                        ))
                                        .with_annotated_type(KnownClass::Str.to_instance(db))
                                        .with_default_type(Type::string_literal(db, "strict")),
                                    ],
                                ),
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
                                Parameters::new(
                                    db,
                                    [Parameter::positional_only(Some(Name::new_static("o")))
                                        .with_annotated_type(Type::any())],
                                ),
                                Some(type_instance),
                            ),
                            Signature::new(
                                Parameters::new(
                                    db,
                                    [
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
                                    ],
                                ),
                                Some(type_instance),
                            ),
                        ],
                    )
                    .into()
                }

                Some(KnownClass::Object) => {
                    // ```py
                    // class object:
                    //    def __init__(self) -> None: ...
                    //    def __new__(cls) -> Self: ...
                    // ```
                    Binding::single(
                        self,
                        Signature::new(Parameters::empty(), Some(Type::object())),
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
                                Parameters::new(
                                    db,
                                    [
                                        Parameter::positional_only(Some(Name::new_static("t")))
                                            .with_annotated_type(Type::any()),
                                        Parameter::positional_only(Some(Name::new_static("obj")))
                                            .with_annotated_type(Type::any()),
                                    ],
                                ),
                                Some(KnownClass::Super.to_instance(db)),
                            ),
                            Signature::new(
                                Parameters::new(
                                    db,
                                    [Parameter::positional_only(Some(Name::new_static("t")))
                                        .with_annotated_type(Type::any())],
                                ),
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
                            Parameters::new(
                                db,
                                [
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
                                ],
                            ),
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
                            None,
                        ),
                    )
                    .into()
                }

                Some(KnownClass::Property) => {
                    let getter_signature = Signature::new(
                        Parameters::new(
                            db,
                            [Parameter::positional_only(None).with_annotated_type(Type::any())],
                        ),
                        Some(Type::any()),
                    );
                    let setter_signature = Signature::new(
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_only(None).with_annotated_type(Type::any()),
                                Parameter::positional_only(None).with_annotated_type(Type::any()),
                            ],
                        ),
                        Some(Type::none(db)),
                    );
                    let deleter_signature = Signature::new(
                        Parameters::new(
                            db,
                            [Parameter::positional_only(None).with_annotated_type(Type::any())],
                        ),
                        Some(Type::any()),
                    );

                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::new(
                                db,
                                [
                                    Parameter::positional_or_keyword(Name::new_static("fget"))
                                        .with_annotated_type(UnionType::from_elements(
                                            db,
                                            [
                                                Type::single_callable(db, getter_signature),
                                                Type::none(db),
                                            ],
                                        ))
                                        .with_default_type(Type::none(db)),
                                    Parameter::positional_or_keyword(Name::new_static("fset"))
                                        .with_annotated_type(UnionType::from_elements(
                                            db,
                                            [
                                                Type::single_callable(db, setter_signature),
                                                Type::none(db),
                                            ],
                                        ))
                                        .with_default_type(Type::none(db)),
                                    Parameter::positional_or_keyword(Name::new_static("fdel"))
                                        .with_annotated_type(UnionType::from_elements(
                                            db,
                                            [
                                                Type::single_callable(db, deleter_signature),
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
                                ],
                            ),
                            None,
                        ),
                    )
                    .into()
                }

                Some(KnownClass::Tuple) => {
                    let object = Type::object();

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
                                Parameters::new(
                                    db,
                                    [Parameter::positional_only(Some(Name::new_static(
                                        "iterable",
                                    )))
                                    .with_annotated_type(
                                        KnownClass::Iterable.to_specialized_instance(db, [object]),
                                    )],
                                ),
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
                        Parameters::new(
                            db,
                            [
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
                            ],
                        ),
                        None,
                    ),
                )
                .into()
            }

            Type::SpecialForm(SpecialFormType::NamedTuple) => {
                Binding::single(self, Signature::todo("functional `NamedTuple` syntax")).into()
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

                // TODO annotated return type on `__new__` or metaclass `__call__`
                // TODO check call vs signatures of `__new__` and/or `__init__`
                SubclassOfInner::TypeVar(_) => Binding::single(
                    self,
                    Signature::new(Parameters::gradual_form(), self.to_instance(db)),
                )
                .into(),
            },

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
                    Place::Defined(dunder_callable, _, boundness) => {
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
                Binding::single(self, Signature::todo("Type::Intersection.call")).into()
            }

            Type::DataclassDecorator(_) => {
                let typevar = BoundTypeVarInstance::synthetic(db, "T", TypeVarVariance::Invariant);
                let typevar_meta = SubclassOfType::from(db, typevar);
                let context = GenericContext::from_typevar_instances(db, [typevar]);
                let parameters = [Parameter::positional_only(Some(Name::new_static("cls")))
                    .with_annotated_type(typevar_meta)];
                // Intersect with `Any` for the return type to reflect the fact that the `dataclass()`
                // decorator adds methods to the class
                let returns = IntersectionType::from_elements(db, [typevar_meta, Type::any()]);
                let signature = Signature::new_generic(
                    Some(context),
                    Parameters::new(db, parameters),
                    Some(returns),
                );
                Binding::single(self, signature).into()
            }

            // TODO: some `SpecialForm`s are callable (e.g. TypedDicts)
            Type::SpecialForm(_) => CallableBinding::not_callable(self).into(),

            Type::EnumLiteral(enum_literal) => enum_literal.enum_class_instance(db).bindings(db),

            Type::KnownInstance(KnownInstanceType::NewType(newtype)) => Binding::single(
                self,
                Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::positional_only(None)
                            .with_annotated_type(newtype.base(db).instance_type(db))],
                    ),
                    Some(Type::NewTypeInstance(newtype)),
                ),
            )
            .into(),

            Type::KnownInstance(known_instance) => {
                known_instance.instance_fallback(db).bindings(db)
            }

            Type::TypeAlias(alias) => alias.value_type(db).bindings(db),

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
            .match_parameters(db, argument_types)
            .check_types(db, argument_types, TypeContext::default(), &[])
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
            Place::Defined(dunder_callable, _, boundness) => {
                let bindings = dunder_callable
                    .bindings(db)
                    .match_parameters(db, argument_types)
                    .check_types(db, argument_types, tcx, &[])?;

                if boundness == Definedness::PossiblyUndefined {
                    return Err(CallDunderError::PossiblyUnbound(Box::new(bindings)));
                }
                Ok(bindings)
            }
            Place::Undefined => Err(CallDunderError::MethodNotAvailable),
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
        fn non_async_special_case<'db>(
            db: &'db dyn Db,
            ty: Type<'db>,
        ) -> Option<Cow<'db, TupleSpec<'db>>> {
            // We will not infer precise heterogeneous tuple specs for literals with lengths above this threshold.
            // The threshold here is somewhat arbitrary and conservative; it could be increased if needed.
            // However, it's probably very rare to need heterogeneous unpacking inference for long string literals
            // or bytes literals, and creating long heterogeneous tuple specs has a performance cost.
            const MAX_TUPLE_LENGTH: usize = 128;

            match ty {
                Type::NominalInstance(nominal) => nominal.tuple_spec(db),
                Type::NewTypeInstance(newtype) => non_async_special_case(db, Type::instance(db, newtype.base_class_type(db))),
                Type::GenericAlias(alias) if alias.origin(db).is_tuple(db) => {
                    Some(Cow::Owned(TupleSpec::homogeneous(todo_type!(
                        "*tuple[] annotations"
                    ))))
                }
                Type::StringLiteral(string_literal_ty) => {
                    let string_literal = string_literal_ty.value(db);
                    let spec = if string_literal.len() < MAX_TUPLE_LENGTH {
                        TupleSpec::heterogeneous(
                            string_literal
                                .chars()
                                .map(|c| Type::string_literal(db, &c.to_string())),
                        )
                    } else {
                        TupleSpec::homogeneous(Type::LiteralString)
                    };
                    Some(Cow::Owned(spec))
                }
                Type::BytesLiteral(bytes) => {
                    let bytes_literal = bytes.value(db);
                    let spec = if bytes_literal.len() < MAX_TUPLE_LENGTH {
                        TupleSpec::heterogeneous(
                            bytes_literal
                                .iter()
                                .map(|b| Type::IntLiteral(i64::from(*b))),
                        )
                    } else {
                        TupleSpec::homogeneous(KnownClass::Int.to_instance(db))
                    };
                    Some(Cow::Owned(spec))
                }
                Type::Never => {
                    // The dunder logic below would have us return `tuple[Never, ...]`, which eagerly
                    // simplifies to `tuple[()]`. That will will cause us to emit false positives if we
                    // index into the tuple. Using `tuple[Unknown, ...]` avoids these false positives.
                    // TODO: Consider removing this special case, and instead hide the indexing
                    // diagnostic in unreachable code.
                    Some(Cow::Owned(TupleSpec::homogeneous(Type::unknown())))
                }
                Type::TypeAlias(alias) => {
                    non_async_special_case(db, alias.value_type(db))
                }
                Type::TypeVar(tvar) => match tvar.typevar(db).bound_or_constraints(db)? {
                    TypeVarBoundOrConstraints::UpperBound(bound) => {
                        non_async_special_case(db, bound)
                    }
                    TypeVarBoundOrConstraints::Constraints(constraints) => non_async_special_case(db, constraints.as_type(db)),
                },
                Type::Union(union) => {
                    let elements = union.elements(db);
                    if elements.len() < MAX_TUPLE_LENGTH {
                        let mut elements_iter = elements.iter();
                        let first_element_spec = elements_iter.next()?.try_iterate_with_mode(db, EvaluationMode::Sync).ok()?;
                        let mut builder = TupleSpecBuilder::from(&*first_element_spec);
                        for element in elements_iter {
                            builder = builder.union(db, &*element.try_iterate_with_mode(db, EvaluationMode::Sync).ok()?);
                        }
                        Some(Cow::Owned(builder.build()))
                    } else {
                        None
                    }
                }
                // N.B. These special cases aren't strictly necessary, they're just obvious optimizations
                Type::LiteralString | Type::Dynamic(_) => Some(Cow::Owned(TupleSpec::homogeneous(ty))),

                Type::FunctionLiteral(_)
                | Type::GenericAlias(_)
                | Type::BoundMethod(_)
                | Type::KnownBoundMethod(_)
                | Type::WrapperDescriptor(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::Callable(_)
                | Type::ModuleLiteral(_)
                // We could infer a precise tuple spec for enum classes with members,
                // but it's not clear whether that's worth the added complexity:
                // you'd have to check that `EnumMeta.__iter__` is not overridden for it to be sound
                // (enums can have `EnumMeta` subclasses as their metaclasses).
                | Type::ClassLiteral(_)
                | Type::SubclassOf(_)
                | Type::ProtocolInstance(_)
                | Type::SpecialForm(_)
                | Type::KnownInstance(_)
                | Type::PropertyInstance(_)
                | Type::Intersection(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::IntLiteral(_)
                | Type::BooleanLiteral(_)
                | Type::EnumLiteral(_)
                | Type::BoundSuper(_)
                | Type::TypeIs(_)
                | Type::TypedDict(_) => None
            }
        }

        if mode.is_async() {
            let try_call_dunder_anext_on_iterator = |iterator: Type<'db>| -> Result<
                Result<Type<'db>, AwaitError<'db>>,
                CallDunderError<'db>,
            > {
                iterator
                    .try_call_dunder(
                        db,
                        "__anext__",
                        CallArguments::none(),
                        TypeContext::default(),
                    )
                    .map(|dunder_anext_outcome| dunder_anext_outcome.return_type(db).try_await(db))
            };

            return match self.try_call_dunder(
                db,
                "__aiter__",
                CallArguments::none(),
                TypeContext::default(),
            ) {
                Ok(dunder_aiter_bindings) => {
                    let iterator = dunder_aiter_bindings.return_type(db);
                    match try_call_dunder_anext_on_iterator(iterator) {
                        Ok(Ok(result)) => Ok(Cow::Owned(TupleSpec::homogeneous(result))),
                        Ok(Err(AwaitError::InvalidReturnType(..))) => {
                            Err(IterationError::UnboundAiterError)
                        } // TODO: __anext__ is bound, but is not properly awaitable
                        Err(dunder_anext_error) | Ok(Err(AwaitError::Call(dunder_anext_error))) => {
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

        if let Some(special_case) = non_async_special_case(db, self) {
            return Ok(special_case);
        }

        let try_call_dunder_getitem = || {
            self.try_call_dunder(
                db,
                "__getitem__",
                CallArguments::positional([KnownClass::Int.to_instance(db)]),
                TypeContext::default(),
            )
            .map(|dunder_getitem_outcome| dunder_getitem_outcome.return_type(db))
        };

        let try_call_dunder_next_on_iterator = |iterator: Type<'db>| {
            iterator
                .try_call_dunder(
                    db,
                    "__next__",
                    CallArguments::none(),
                    TypeContext::default(),
                )
                .map(|dunder_next_outcome| dunder_next_outcome.return_type(db))
        };

        let dunder_iter_result = self
            .try_call_dunder(
                db,
                "__iter__",
                CallArguments::none(),
                TypeContext::default(),
            )
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

        let enter = self.try_call_dunder(
            db,
            enter_method,
            CallArguments::none(),
            TypeContext::default(),
        );
        let exit = self.try_call_dunder(
            db,
            exit_method,
            CallArguments::positional([Type::none(db), Type::none(db), Type::none(db)]),
            TypeContext::default(),
        );

        // TODO: Make use of Protocols when we support it (the manager be assignable to `contextlib.AbstractContextManager`).
        match (enter, exit) {
            (Ok(enter), Ok(_)) => {
                let ty = enter.return_type(db);
                Ok(if mode.is_async() {
                    ty.try_await(db).unwrap_or(Type::unknown())
                } else {
                    ty
                })
            }
            (Ok(enter), Err(exit_error)) => {
                let ty = enter.return_type(db);
                Err(ContextManagerError::Exit {
                    enter_return_type: if mode.is_async() {
                        ty.try_await(db).unwrap_or(Type::unknown())
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
            Type::Union(union) => union.try_map(db, |ty| ty.generator_return_type(db)),
            ty @ (Type::Dynamic(_) | Type::Never) => Some(ty),
            _ => None,
        }
    }

    /// Given a class literal or non-dynamic `SubclassOf` type, try calling it (creating an instance)
    /// and return the resulting instance type.
    ///
    /// The `infer_argument_types` closure should be invoked with the signatures of `__new__` and
    /// `__init__`, such that the argument types can be inferred with the correct type context.
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
    fn try_call_constructor<'ast>(
        self,
        db: &'db dyn Db,
        infer_argument_types: impl FnOnce(Option<Bindings<'db>>) -> CallArguments<'ast, 'db>,
        tcx: TypeContext<'db>,
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
        let (generic_origin, generic_context, self_type) = match self {
            Type::ClassLiteral(class) => match class.generic_context(db) {
                Some(generic_context) => (
                    Some(class),
                    Some(generic_context),
                    // It is important that identity_specialization specializes the class with
                    // _inferable_ typevars, so that our specialization inference logic will
                    // try to find a specialization for them.
                    Type::from(class.identity_specialization(db)),
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
        let new_method = self_type.lookup_dunder_new(db);

        // Construct an instance type that we can use to look up the `__init__` instance method.
        // This performs the same logic as `Type::to_instance`, except for generic class literals.
        // TODO: we should use the actual return type of `__new__` to determine the instance type
        let init_ty = self_type
            .to_instance(db)
            .expect("type should be convertible to instance type");

        // Lookup the `__init__` instance method in the MRO.
        let init_method = init_ty.member_lookup_with_policy(
            db,
            "__init__".into(),
            MemberLookupPolicy::NO_INSTANCE_FALLBACK | MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
        );

        // Infer the call argument types, using both `__new__` and `__init__` for type-context.
        let bindings = match (
            new_method.as_ref().map(|method| &method.place),
            &init_method.place,
        ) {
            (Some(Place::Defined(new_method, ..)), Place::Undefined) => Some(
                new_method
                    .bindings(db)
                    .map(|binding| binding.with_bound_type(self_type)),
            ),

            (Some(Place::Undefined) | None, Place::Defined(init_method, ..)) => {
                Some(init_method.bindings(db))
            }

            (Some(Place::Defined(new_method, ..)), Place::Defined(init_method, ..)) => {
                let callable = UnionBuilder::new(db)
                    .add(*new_method)
                    .add(*init_method)
                    .build();

                let new_method_bindings = new_method
                    .bindings(db)
                    .map(|binding| binding.with_bound_type(self_type));

                Some(Bindings::from_union(
                    callable,
                    [new_method_bindings, init_method.bindings(db)],
                ))
            }

            _ => None,
        };

        let argument_types = infer_argument_types(bindings);

        let new_call_outcome = new_method.and_then(|new_method| {
            match new_method.place.try_call_dunder_get(db, self_type) {
                Place::Defined(new_method, _, boundness) => {
                    let argument_types = argument_types.with_self(Some(self_type));
                    let result = new_method
                        .bindings(db)
                        .with_constructor_instance_type(init_ty)
                        .match_parameters(db, &argument_types)
                        .check_types(db, &argument_types, tcx, &[]);

                    if boundness == Definedness::PossiblyUndefined {
                        Some(Err(DunderNewCallError::PossiblyUnbound(result.err())))
                    } else {
                        Some(result.map_err(DunderNewCallError::CallError))
                    }
                }
                Place::Undefined => None,
            }
        });

        let init_call_outcome = if new_call_outcome.is_none() || !init_method.is_undefined() {
            let call_result = match init_ty
                .member_lookup_with_policy(
                    db,
                    "__init__".into(),
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place
            {
                Place::Undefined => Err(CallDunderError::MethodNotAvailable),
                Place::Defined(dunder_callable, _, boundness) => {
                    let bindings = dunder_callable
                        .bindings(db)
                        .with_constructor_instance_type(init_ty);

                    bindings
                        .match_parameters(db, &argument_types)
                        .check_types(db, &argument_types, tcx, &[])
                        .map_err(CallDunderError::from)
                        .and_then(|bindings| {
                            if boundness == Definedness::PossiblyUndefined {
                                Err(CallDunderError::PossiblyUnbound(Box::new(bindings)))
                            } else {
                                Ok(bindings)
                            }
                        })
                }
            };

            Some(call_result)
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

                let specialize_constructor = |outcome: Option<Bindings<'db>>| {
                    let (_, binding) = outcome
                        .as_ref()?
                        .single_element()?
                        .matching_overloads()
                        .next()?;
                    binding.specialization()?.restrict(db, generic_context?)
                };

                let new_specialization =
                    specialize_constructor(new_call_outcome.and_then(Result::ok));
                let init_specialization =
                    specialize_constructor(init_call_outcome.and_then(Result::ok));
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
    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Option<Type<'db>> {
        match self {
            Type::Dynamic(_) | Type::Never => Some(self),
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
            Type::BooleanLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::FunctionLiteral(_)
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
            | Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::LiteralString
            | Type::BoundSuper(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
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
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        match self {
            // Special cases for `float` and `complex`
            // https://typing.python.org/en/latest/spec/special-types.html#special-cases-for-float-and-complex
            Type::ClassLiteral(class) => {
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
                    _ if class.is_typed_dict(db) => {
                        Type::typed_dict(class.default_specialization(db))
                    }
                    _ => Type::instance(db, class.default_specialization(db)),
                };
                Ok(ty)
            }
            Type::GenericAlias(alias) if alias.is_typed_dict(db) => Ok(Type::typed_dict(*alias)),
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
            | Type::KnownBoundMethod(_)
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
                KnownInstanceType::TypeAliasType(alias) => Ok(Type::TypeAlias(*alias)),
                KnownInstanceType::NewType(newtype) => Ok(Type::NewTypeInstance(*newtype)),
                KnownInstanceType::TypeVar(typevar) => {
                    // TODO: A `ParamSpec` type variable cannot be used in type expressions. This
                    // requires storing additional context as it's allowed in some places
                    // (`Concatenate`, `Callable`) but not others.
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
                    invalid_expressions: smallvec::smallvec![InvalidTypeExpression::Deprecated],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::Field(__call__) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec![InvalidTypeExpression::Field],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::ConstraintSet(__call__) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec![InvalidTypeExpression::ConstraintSet],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::GenericContext(__call__) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec![InvalidTypeExpression::GenericContext],
                    fallback_type: Type::unknown(),
                }),
                KnownInstanceType::Specialization(__call__) => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec![InvalidTypeExpression::Specialization],
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
            },

            Type::SpecialForm(special_form) => match special_form {
                SpecialFormType::Never | SpecialFormType::NoReturn => Ok(Type::Never),
                SpecialFormType::LiteralString => Ok(Type::LiteralString),
                SpecialFormType::Any => Ok(Type::any()),
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
                SpecialFormType::Callable => Ok(Type::Callable(CallableType::unknown(db))),

                // Special case: `NamedTuple` in a type expression is understood to describe the type
                // `tuple[object, ...] & <a protocol that any `NamedTuple` class would satisfy>`.
                // This isn't very principled (since at runtime, `NamedTuple` is just a function),
                // but it appears to be what users often expect, and it improves compatibility with
                // other type checkers such as mypy.
                // See conversation in https://github.com/astral-sh/ruff/pull/19915.
                SpecialFormType::NamedTuple => Ok(IntersectionBuilder::new(db)
                    .positive_elements([
                        Type::homogeneous_tuple(db, Type::object()),
                        KnownClass::NamedTupleLike.to_instance(db),
                    ])
                    .build()),

                SpecialFormType::TypingSelf => {
                    let index = semantic_index(db, scope_id.file(db));
                    let Some(class) = nearest_enclosing_class(db, index, scope_id) else {
                        return Err(InvalidTypeExpressionError {
                            fallback_type: Type::unknown(),
                            invalid_expressions: smallvec::smallvec_inline![
                                InvalidTypeExpression::InvalidType(*self, scope_id)
                            ],
                        });
                    };

                    Ok(typing_self(db, scope_id, typevar_binding_context, class).unwrap_or(*self))
                }
                // We ensure that `typing.TypeAlias` used in the expected position (annotating an
                // annotated assignment statement) doesn't reach here. Using it in any other type
                // expression is an error.
                SpecialFormType::TypeAlias => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![
                        InvalidTypeExpression::TypeAlias
                    ],
                    fallback_type: Type::unknown(),
                }),
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
                        InvalidTypeExpression::RequiresArguments(*special_form)
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
                | SpecialFormType::Top
                | SpecialFormType::Bottom
                | SpecialFormType::TypeOf
                | SpecialFormType::TypeIs
                | SpecialFormType::TypeGuard
                | SpecialFormType::Unpack
                | SpecialFormType::CallableTypeOf => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![
                        InvalidTypeExpression::RequiresOneArgument(*special_form)
                    ],
                    fallback_type: Type::unknown(),
                }),

                SpecialFormType::Annotated | SpecialFormType::Concatenate => {
                    Err(InvalidTypeExpressionError {
                        invalid_expressions: smallvec::smallvec_inline![
                            InvalidTypeExpression::RequiresTwoArguments(*special_form)
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

            Type::NominalInstance(instance) => match instance.known_class(db) {
                Some(KnownClass::NoneType) => Ok(Type::none(db)),
                Some(KnownClass::TypeVar) => Ok(todo_type!(
                    "Support for `typing.TypeVar` instances in type expressions"
                )),
                Some(KnownClass::TypeVarTuple) => Ok(todo_type!(
                    "Support for `typing.TypeVarTuple` instances in type expressions"
                )),
                _ => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec::smallvec_inline![
                        InvalidTypeExpression::InvalidType(*self, scope_id)
                    ],
                    fallback_type: Type::unknown(),
                }),
            },

            Type::Intersection(_) => Ok(todo_type!("Type::Intersection.in_type_expression")),

            Type::TypeAlias(alias) => {
                alias
                    .value_type(db)
                    .in_type_expression(db, scope_id, typevar_binding_context)
            }

            Type::NewTypeInstance(_) => Err(InvalidTypeExpressionError {
                invalid_expressions: smallvec::smallvec_inline![
                    InvalidTypeExpression::InvalidType(*self, scope_id)
                ],
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
            Type::BooleanLiteral(_) | Type::TypeIs(_) => KnownClass::Bool.to_class_literal(db),
            Type::BytesLiteral(_) => KnownClass::Bytes.to_class_literal(db),
            Type::IntLiteral(_) => KnownClass::Int.to_class_literal(db),
            Type::EnumLiteral(enum_literal) => Type::ClassLiteral(enum_literal.enum_class(db)),
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
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of().into_class(db) {
                None => self,
                Some(class) => SubclassOfType::try_from_type(db, class.metaclass(db))
                    .unwrap_or(SubclassOfType::subclass_of_unknown()),
            },
            Type::StringLiteral(_) | Type::LiteralString => KnownClass::Str.to_class_literal(db),
            Type::Dynamic(dynamic) => SubclassOfType::from(db, SubclassOfInner::Dynamic(dynamic)),
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
            Type::TypedDict(typed_dict) => SubclassOfType::from(db, typed_dict.defining_class()),
            Type::TypeAlias(alias) => alias.value_type(db).to_meta_type(db),
            Type::NewTypeInstance(newtype) => Type::from(newtype.base_class_type(db)),
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
                .to_specialized_class_type(db, [KnownClass::Str.to_instance(db), Type::object()])
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
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size, cycle_fn=apply_specialization_cycle_recover, cycle_initial=apply_specialization_cycle_initial)]
    pub(crate) fn apply_specialization(
        self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Type<'db> {
        let new_specialization = self.apply_type_mapping(
            db,
            &TypeMapping::Specialization(specialization),
            TypeContext::default(),
        );
        match specialization.materialization_kind(db) {
            None => new_specialization,
            Some(materialization_kind) => new_specialization.materialize(
                db,
                materialization_kind,
                &ApplyTypeMappingVisitor::default(),
            ),
        }
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
            TypeMapping::BindSelf { self_type, .. } if self == *self_type => return self,
            _ => {}
        }

        match self {
            Type::TypeVar(bound_typevar) => bound_typevar.apply_type_mapping_impl(db, type_mapping, visitor),

            Type::KnownInstance(known_instance) => match known_instance {
                KnownInstanceType::TypeVar(typevar) => {
                    match type_mapping {
                        TypeMapping::BindLegacyTypevars(binding_context) => {
                            Type::TypeVar(BoundTypeVarInstance::new(db, typevar, *binding_context, None))
                        }
                        TypeMapping::Specialization(_) |
                        TypeMapping::PartialSpecialization(_) |
                        TypeMapping::PromoteLiterals(_) |
                        TypeMapping::BindSelf { .. } |
                        TypeMapping::ReplaceSelf { .. } |
                        TypeMapping::Materialize(_) |
                        TypeMapping::ReplaceParameterDefaults |
                        TypeMapping::EagerExpansion => self,
                    }
                }
                KnownInstanceType::UnionType(instance) => {
                    if let Ok(union_type) = instance.union_type(db) {
                        Type::KnownInstance(KnownInstanceType::UnionType(
                            UnionTypeInstance::new(
                                db,
                                instance._value_expr_types(db),
                                Ok(union_type.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                            )
                        )))
                    } else {
                        self
                    }
                },
                KnownInstanceType::Annotated(ty) => {
                    Type::KnownInstance(KnownInstanceType::Annotated(
                        InternedType::new(
                            db,
                            ty.inner(db).apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                        )
                    ))
                },
                KnownInstanceType::Callable(callable_type) => {
                    Type::KnownInstance(KnownInstanceType::Callable(
                        callable_type.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                    ))
                },
                KnownInstanceType::TypeGenericAlias(ty) => {
                    Type::KnownInstance(KnownInstanceType::TypeGenericAlias(
                        InternedType::new(
                            db,
                            ty.inner(db).apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                        )
                    ))
                },

                KnownInstanceType::SubscriptedProtocol(_) |
                KnownInstanceType::SubscriptedGeneric(_) |
                KnownInstanceType::TypeAliasType(_) |
                KnownInstanceType::Deprecated(_) |
                KnownInstanceType::Field(_) |
                KnownInstanceType::ConstraintSet(_) |
                KnownInstanceType::GenericContext(_) |
                KnownInstanceType::Specialization(_) |
                KnownInstanceType::Literal(_) |
                KnownInstanceType::LiteralStringAlias(_) |
                KnownInstanceType::NewType(_) => {
                    // TODO: For some of these, we may need to apply the type mapping to inner types.
                    self
                },
            }

            Type::FunctionLiteral(function) => {
                let function = Type::FunctionLiteral(function.apply_type_mapping_impl(db, type_mapping, tcx, visitor));

                match type_mapping {
                    TypeMapping::PromoteLiterals(PromoteLiteralsMode::On) => function.promote_literals_impl(db, tcx),
                    _ => function
                }
            }

            Type::BoundMethod(method) => Type::BoundMethod(BoundMethodType::new(
                db,
                method.function(db).apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                method.self_instance(db).apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            )),

            Type::NominalInstance(instance) => {
                instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            },

            Type::NewTypeInstance(newtype) => visitor.visit(self, || {
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

            Type::Callable(callable) => {
                Type::Callable(callable.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }

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

            Type::Union(union) => union.map(db, |element| {
                element.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            }),
            Type::Intersection(intersection) => {
                let mut builder = IntersectionBuilder::new(db);
                for positive in intersection.positive(db) {
                    builder =
                        builder.add_positive(positive.apply_type_mapping_impl(db, type_mapping, tcx, visitor));
                }
                for negative in intersection.negative(db) {
                    builder =
                        builder.add_negative(negative.apply_type_mapping_impl(db, &type_mapping.flip(), tcx, visitor));
                }
                builder.build()
            }

            // TODO(jelle): Materialize should be handled differently, since TypeIs is invariant
            Type::TypeIs(type_is) => type_is.with_type(db, type_is.return_type(db).apply_type_mapping(db, type_mapping, tcx)),

            Type::TypeAlias(alias) => {
                if TypeMapping::EagerExpansion == *type_mapping {
                    return alias.raw_value_type(db).expand_eagerly(db);
                }
                // Do not call `value_type` here. `value_type` does the specialization internally, so `apply_type_mapping` is performed without `visitor` inheritance.
                // In the case of recursive type aliases, this leads to infinite recursion.
                // Instead, call `raw_value_type` and perform the specialization after the `visitor` cache has been created.
                let value_type = visitor.visit(self, || alias.raw_value_type(db).apply_type_mapping_impl(db, type_mapping, tcx, visitor));
                alias.apply_function_specialization(db, value_type).apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            }

            Type::ModuleLiteral(_)
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::LiteralString
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_) => match type_mapping {
                TypeMapping::Specialization(_) |
                TypeMapping::PartialSpecialization(_) |
                TypeMapping::BindLegacyTypevars(_) |
                TypeMapping::BindSelf { .. } |
                TypeMapping::ReplaceSelf { .. } |
                TypeMapping::Materialize(_) |
                TypeMapping::ReplaceParameterDefaults |
                TypeMapping::EagerExpansion |
                TypeMapping::PromoteLiterals(PromoteLiteralsMode::Off) => self,
                TypeMapping::PromoteLiterals(PromoteLiteralsMode::On) => self.promote_literals_impl(db, tcx)
            }

            Type::Dynamic(_) => match type_mapping {
                TypeMapping::Specialization(_) |
                TypeMapping::PartialSpecialization(_) |
                TypeMapping::BindLegacyTypevars(_) |
                TypeMapping::BindSelf { .. } |
                TypeMapping::ReplaceSelf { .. } |
                TypeMapping::PromoteLiterals(_) |
                TypeMapping::ReplaceParameterDefaults |
                TypeMapping::EagerExpansion => self,
                TypeMapping::Materialize(materialization_kind) => match materialization_kind {
                    MaterializationKind::Top => Type::object(),
                    MaterializationKind::Bottom => Type::Never,
                }
            }

            Type::Never
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
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
                | KnownBoundMethodType::GenericContextSpecializeConstrained(_)
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
                TypeVarKind::Legacy | TypeVarKind::TypingSelf
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

            Type::FunctionLiteral(function) => {
                function.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
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
                | KnownBoundMethodType::PropertyDunderSet(property),
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
                type_is.return_type(db).find_legacy_typevars_impl(
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
                | KnownInstanceType::NewType(_) => {
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
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
                | KnownBoundMethodType::GenericContextSpecializeConstrained(_),
            )
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
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size, cycle_fn=expand_eagerly_cycle_recover, cycle_initial=expand_eagerly_cycle_initial)]
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
            Type::SpecialForm(special_form) => Type::string_literal(db, &special_form.to_string()),
            Type::KnownInstance(known_instance) => Type::StringLiteral(StringLiteralType::new(
                db,
                known_instance.repr(db).to_compact_string(),
            )),
            // TODO: handle more complex types
            _ => KnownClass::Str.to_instance(db),
        }
    }

    /// Return the string representation of this type as it would be provided by the  `__repr__`
    /// method at runtime.
    #[must_use]
    pub(crate) fn repr(&self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Type::IntLiteral(number) => Type::string_literal(db, &number.to_string()),
            Type::BooleanLiteral(true) => Type::string_literal(db, "True"),
            Type::BooleanLiteral(false) => Type::string_literal(db, "False"),
            Type::StringLiteral(literal) => {
                Type::string_literal(db, &format!("'{}'", literal.value(db).escape_default()))
            }
            Type::LiteralString => Type::LiteralString,
            Type::SpecialForm(special_form) => Type::string_literal(db, &special_form.to_string()),
            Type::KnownInstance(known_instance) => Type::StringLiteral(StringLiteralType::new(
                db,
                known_instance.repr(db).to_compact_string(),
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
                SubclassOfInner::TypeVar(bound_typevar) => Some(TypeDefinition::TypeVar(bound_typevar.typevar(db).definition(db)?)),
            },

            Self::TypeAlias(alias) => alias.value_type(db).definition(db),
            Self::NewTypeInstance(newtype) => Some(TypeDefinition::NewType(newtype.definition(db))),

            Self::PropertyInstance(property) => property
                .getter(db)
                .and_then(|getter|getter.definition(db))
                .or_else(||property.setter(db).and_then(|setter|setter.definition(db))),

            Self::StringLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::LiteralString
            | Self::IntLiteral(_)
            | Self::BytesLiteral(_)
            // TODO: For enum literals, it would be even better to jump to the definition of the specific member
            | Self::EnumLiteral(_)
            | Self::KnownBoundMethod(_)
            | Self::WrapperDescriptor(_)
            | Self::DataclassDecorator(_)
            | Self::DataclassTransformer(_)
            | Self::BoundSuper(_) => self.to_meta_type(db).definition(db),

            Self::TypeVar(bound_typevar) => Some(TypeDefinition::TypeVar(bound_typevar.typevar(db).definition(db)?)),

            Self::ProtocolInstance(protocol) => match protocol.inner {
                Protocol::FromClass(class) => Some(TypeDefinition::Class(class.definition(db))),
                Protocol::Synthesized(_) => None,
            },

            Self::TypedDict(typed_dict) => {
                Some(TypeDefinition::Class(typed_dict.defining_class().definition(db)))
            }

            Self::Union(_) | Self::Intersection(_) => None,

            Self::SpecialForm(special_form) => special_form.definition(db),
            Self::Never => Type::SpecialForm(SpecialFormType::Never).definition(db),
            Self::Dynamic(DynamicType::Any) => Type::SpecialForm(SpecialFormType::Any).definition(db),
            Self::Dynamic(DynamicType::Unknown | DynamicType::UnknownGeneric(_)) => Type::SpecialForm(SpecialFormType::Unknown).definition(db),
            Self::AlwaysTruthy => Type::SpecialForm(SpecialFormType::AlwaysTruthy).definition(db),
            Self::AlwaysFalsy => Type::SpecialForm(SpecialFormType::AlwaysFalsy).definition(db),

            // These types have no definition
            Self::Dynamic(DynamicType::Divergent(_) | DynamicType::Todo(_) | DynamicType::TodoUnpack | DynamicType::TodoStarredExpression)
            | Self::Callable(_)
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

    /// Default-specialize all legacy typevars in this type.
    ///
    /// This is used when an implicit type alias is referenced without explicitly specializing it.
    pub(crate) fn default_specialize(self, db: &'db dyn Db) -> Type<'db> {
        let mut variables = FxOrderSet::default();
        self.find_legacy_typevars(db, None, &mut variables);
        let generic_context = GenericContext::from_typevar_instances(db, variables);
        self.apply_specialization(db, generic_context.default_specialization(db, None))
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
                .map(|ty| ty.variance_of(db, typevar))
                .collect(),
            Type::SubclassOf(subclass_of_type) => subclass_of_type.variance_of(db, typevar),
            Type::TypeIs(type_is_type) => type_is_type.variance_of(db, typevar),
            Type::KnownInstance(known_instance) => known_instance.variance_of(db, typevar),
            Type::Dynamic(_)
            | Type::Never
            | Type::WrapperDescriptor(_)
            | Type::KnownBoundMethod(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::EnumLiteral(_)
            | Type::LiteralString
            | Type::BytesLiteral(_)
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

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_redundant_with_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _subtype: Type<'db>,
    _supertype: Type<'db>,
) -> bool {
    true
}

fn apply_specialization_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_value: &Type<'db>,
    value: Type<'db>,
    _self: Type<'db>,
    _specialization: Specialization<'db>,
) -> Type<'db> {
    value.cycle_normalized(db, *previous_value, cycle)
}

fn apply_specialization_cycle_initial<'db>(
    _db: &'db dyn Db,
    id: salsa::Id,
    _self: Type<'db>,
    _specialization: Specialization<'db>,
) -> Type<'db> {
    Type::divergent(id)
}

fn expand_eagerly_cycle_initial<'db>(
    _db: &'db dyn Db,
    id: salsa::Id,
    _self: Type<'db>,
    _unit: (),
) -> Type<'db> {
    Type::divergent(id)
}

fn expand_eagerly_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_value: &Type<'db>,
    value: Type<'db>,
    _self: Type<'db>,
    _unit: (),
) -> Type<'db> {
    value.cycle_normalized(db, *previous_value, cycle)
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub enum PromoteLiteralsMode {
    On,
    Off,
}

impl PromoteLiteralsMode {
    const fn flip(self) -> Self {
        match self {
            PromoteLiteralsMode::On => PromoteLiteralsMode::Off,
            PromoteLiteralsMode::Off => PromoteLiteralsMode::On,
        }
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
    /// Replaces any literal types with their corresponding promoted type form (e.g. `Literal["string"]`
    /// to `str`, or `def _() -> int` to `Callable[[], int]`).
    PromoteLiterals(PromoteLiteralsMode),
    /// Binds a legacy typevar with the generic context (class, function, type alias) that it is
    /// being used in.
    BindLegacyTypevars(BindingContext<'db>),
    /// Binds any `typing.Self` typevar with a particular `self` class.
    BindSelf {
        self_type: Type<'db>,
        binding_context: Option<BindingContext<'db>>,
    },
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
}

impl<'db> TypeMapping<'_, 'db> {
    /// Update the generic context of a [`Signature`] according to the current type mapping
    pub(crate) fn update_signature_generic_context(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> GenericContext<'db> {
        match self {
            TypeMapping::Specialization(_)
            | TypeMapping::PartialSpecialization(_)
            | TypeMapping::PromoteLiterals(_)
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::Materialize(_)
            | TypeMapping::ReplaceParameterDefaults
            | TypeMapping::EagerExpansion => context,
            TypeMapping::BindSelf {
                binding_context, ..
            } => context.remove_self(db, *binding_context),
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
            TypeMapping::PromoteLiterals(mode) => TypeMapping::PromoteLiterals(mode.flip()),
            TypeMapping::Specialization(_)
            | TypeMapping::PartialSpecialization(_)
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::BindSelf { .. }
            | TypeMapping::ReplaceSelf { .. }
            | TypeMapping::ReplaceParameterDefaults
            | TypeMapping::EagerExpansion => self.clone(),
        }
    }
}

/// A Salsa-tracked constraint set. This is only needed to have something appropriately small to
/// put in a [`KnownInstance::ConstraintSet`]. We don't actually manipulate these as part of using
/// constraint sets to check things like assignability; they're only used as a debugging aid in
/// mdtests. That means there's no need for this to be interned; being tracked is sufficient.
#[salsa::tracked(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct TrackedConstraintSet<'db> {
    constraints: ConstraintSet<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TrackedConstraintSet<'_> {}

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

    /// A constraint set, which is exposed in mdtests as an instance of
    /// `ty_extensions.ConstraintSet`.
    ConstraintSet(TrackedConstraintSet<'db>),

    /// A generic context, which is exposed in mdtests as an instance of
    /// `ty_extensions.GenericContext`.
    GenericContext(GenericContext<'db>),

    /// A specialization, which is exposed in mdtests as an instance of
    /// `ty_extensions.Specialization`.
    Specialization(Specialization<'db>),

    /// A single instance of `types.UnionType`, which stores the elements of
    /// a PEP 604 union, or a `typing.Union`.
    UnionType(UnionTypeInstance<'db>),

    /// A single instance of `typing.Literal`
    Literal(InternedType<'db>),

    /// A single instance of `typing.Annotated`
    Annotated(InternedType<'db>),

    /// An instance of `typing.GenericAlias` representing a `type[...]` expression.
    TypeGenericAlias(InternedType<'db>),

    /// An instance of `typing.GenericAlias` representing a `Callable[...]` expression.
    Callable(CallableType<'db>),

    /// A literal string which is the right-hand side of a PEP 613 `TypeAlias`.
    LiteralStringAlias(InternedType<'db>),

    /// An identity callable created with `typing.NewType(name, base)`, which behaves like a
    /// subtype of `base` in type expressions. See the `struct NewType` payload for an example.
    NewType(NewType<'db>),
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
        KnownInstanceType::Deprecated(_)
        | KnownInstanceType::ConstraintSet(_)
        | KnownInstanceType::GenericContext(_)
        | KnownInstanceType::Specialization(_) => {
            // Nothing to visit
        }
        KnownInstanceType::Field(field) => {
            if let Some(default_ty) = field.default_type(db) {
                visitor.visit_type(db, default_ty);
            }
        }
        KnownInstanceType::UnionType(instance) => {
            if let Ok(union_type) = instance.union_type(db) {
                visitor.visit_type(db, *union_type);
            }
        }
        KnownInstanceType::Literal(ty)
        | KnownInstanceType::Annotated(ty)
        | KnownInstanceType::TypeGenericAlias(ty)
        | KnownInstanceType::LiteralStringAlias(ty) => {
            visitor.visit_type(db, ty.inner(db));
        }
        KnownInstanceType::Callable(callable) => {
            visitor.visit_callable_type(db, callable);
        }
        KnownInstanceType::NewType(newtype) => {
            if let ClassType::Generic(generic_alias) = newtype.base_class_type(db) {
                visitor.visit_generic_alias_type(db, generic_alias);
            }
        }
    }
}

impl<'db> VarianceInferable<'db> for KnownInstanceType<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        match self {
            KnownInstanceType::TypeAliasType(type_alias) => {
                type_alias.raw_value_type(db).variance_of(db, typevar)
            }
            _ => TypeVarVariance::Bivariant,
        }
    }
}

impl<'db> KnownInstanceType<'db> {
    fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
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
            Self::Field(field) => Self::Field(field.normalized_impl(db, visitor)),
            Self::UnionType(instance) => Self::UnionType(instance.normalized_impl(db, visitor)),
            Self::Literal(ty) => Self::Literal(ty.normalized_impl(db, visitor)),
            Self::Annotated(ty) => Self::Annotated(ty.normalized_impl(db, visitor)),
            Self::TypeGenericAlias(ty) => Self::TypeGenericAlias(ty.normalized_impl(db, visitor)),
            Self::Callable(callable) => Self::Callable(callable.normalized_impl(db, visitor)),
            Self::LiteralStringAlias(ty) => {
                Self::LiteralStringAlias(ty.normalized_impl(db, visitor))
            }
            Self::NewType(newtype) => Self::NewType(
                newtype
                    .map_base_class_type(db, |class_type| class_type.normalized_impl(db, visitor)),
            ),
            Self::Deprecated(_)
            | Self::ConstraintSet(_)
            | Self::GenericContext(_)
            | Self::Specialization(_) => {
                // Nothing to normalize
                self
            }
        }
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            // Nothing to normalize
            Self::SubscriptedProtocol(context) => Some(Self::SubscriptedProtocol(context)),
            Self::SubscriptedGeneric(context) => Some(Self::SubscriptedGeneric(context)),
            Self::Deprecated(deprecated) => Some(Self::Deprecated(deprecated)),
            Self::ConstraintSet(set) => Some(Self::ConstraintSet(set)),
            Self::TypeVar(typevar) => Some(Self::TypeVar(typevar)),
            Self::TypeAliasType(type_alias) => type_alias
                .recursive_type_normalized_impl(db, div)
                .map(Self::TypeAliasType),
            Self::Field(field) => field
                .recursive_type_normalized_impl(db, div, nested)
                .map(Self::Field),
            Self::UnionType(union_type) => union_type
                .recursive_type_normalized_impl(db, div, nested)
                .map(Self::UnionType),
            Self::Literal(ty) => ty
                .recursive_type_normalized_impl(db, div, true)
                .map(Self::Literal),
            Self::Annotated(ty) => ty
                .recursive_type_normalized_impl(db, div, true)
                .map(Self::Annotated),
            Self::TypeGenericAlias(ty) => ty
                .recursive_type_normalized_impl(db, div, true)
                .map(Self::TypeGenericAlias),
            Self::LiteralStringAlias(ty) => ty
                .recursive_type_normalized_impl(db, div, true)
                .map(Self::LiteralStringAlias),
            Self::Callable(callable) => callable
                .recursive_type_normalized_impl(db, div, nested)
                .map(Self::Callable),
            Self::NewType(newtype) => newtype
                .try_map_base_class_type(db, |class_type| {
                    class_type.recursive_type_normalized_impl(db, div, true)
                })
                .map(Self::NewType),
            Self::GenericContext(generic) => Some(Self::GenericContext(generic)),
            Self::Specialization(specialization) => specialization
                .recursive_type_normalized_impl(db, div, true)
                .map(Self::Specialization),
        }
    }

    fn class(self, db: &'db dyn Db) -> KnownClass {
        match self {
            Self::SubscriptedProtocol(_) | Self::SubscriptedGeneric(_) => KnownClass::SpecialForm,
            Self::TypeVar(typevar_instance) if typevar_instance.is_paramspec(db) => {
                KnownClass::ParamSpec
            }
            Self::TypeVar(_) => KnownClass::TypeVar,
            Self::TypeAliasType(TypeAliasType::PEP695(alias)) if alias.is_specialized(db) => {
                KnownClass::GenericAlias
            }
            Self::TypeAliasType(_) => KnownClass::TypeAliasType,
            Self::Deprecated(_) => KnownClass::Deprecated,
            Self::Field(_) => KnownClass::Field,
            Self::ConstraintSet(_) => KnownClass::ConstraintSet,
            Self::GenericContext(_) => KnownClass::GenericContext,
            Self::Specialization(_) => KnownClass::Specialization,
            Self::UnionType(_) => KnownClass::UnionType,
            Self::Literal(_)
            | Self::Annotated(_)
            | Self::TypeGenericAlias(_)
            | Self::Callable(_) => KnownClass::GenericAlias,
            Self::LiteralStringAlias(_) => KnownClass::Str,
            Self::NewType(_) => KnownClass::NewType,
        }
    }

    fn to_meta_type(self, db: &'db dyn Db) -> Type<'db> {
        self.class(db).to_class_literal(db)
    }

    /// Return the instance type which this type is a subtype of.
    ///
    /// For example, an alias created using the `type` statement is an instance of
    /// `typing.TypeAliasType`, so `KnownInstanceType::TypeAliasType(_).instance_fallback(db)`
    /// returns `Type::NominalInstance(NominalInstanceType { class: <typing.TypeAliasType> })`.
    fn instance_fallback(self, db: &dyn Db) -> Type<'_> {
        self.class(db).to_instance(db)
    }

    /// Return `true` if this symbol is an instance of `class`.
    fn is_instance_of(self, db: &dyn Db, class: ClassType) -> bool {
        self.class(db).is_subclass_of(db, class)
    }

    /// Return the repr of the symbol at runtime
    fn repr(self, db: &'db dyn Db) -> impl std::fmt::Display + 'db {
        self.display_with(db, DisplaySettings::default())
    }
}

/// A type that is determined to be divergent during recursive type inference.
/// This type must never be eliminated by dynamic type reduction
/// (e.g. `Divergent` is assignable to `@Todo`, but `@Todo | Divergent` must not be reducted to `@Todo`).
/// Otherwise, type inference cannot converge properly.
/// For detailed properties of this type, see the unit test at the end of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, salsa::Update)]
pub struct DivergentType {
    /// The query ID that caused the cycle.
    id: salsa::Id,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for DivergentType {}

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
    /// A special Todo-variant for `*Ts`, so that we can treat it specially in `Generic[Unpack[Ts]]`
    TodoStarredExpression,
    /// A type that is determined to be divergent during recursive type inference.
    Divergent(DivergentType),
}

impl DynamicType<'_> {
    fn normalized(self) -> Self {
        if matches!(self, Self::Divergent(_)) {
            self
        } else {
            Self::Any
        }
    }

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
            DynamicType::Unknown | DynamicType::UnknownGeneric(_) => f.write_str("Unknown"),
            // `DynamicType::Todo`'s display should be explicit that is not a valid display of
            // any other type
            DynamicType::Todo(todo) => write!(f, "@Todo{todo}"),
            DynamicType::TodoUnpack => f.write_str("@Todo(typing.Unpack)"),
            DynamicType::TodoStarredExpression => f.write_str("@Todo(StarredExpression)"),
            DynamicType::Divergent(_) => f.write_str("Divergent"),
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
    fn name(self) -> &'static str {
        match self {
            Self::CLASS_VAR => "ClassVar",
            Self::FINAL => "Final",
            Self::INIT_VAR => "InitVar",
            Self::REQUIRED => "Required",
            Self::NOT_REQUIRED => "NotRequired",
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

    /// Insert/add an additional type qualifier.
    pub(crate) fn add_qualifier(&mut self, qualifier: TypeQualifiers) {
        self.qualifiers |= qualifier;
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct InvalidTypeExpressionError<'db> {
    fallback_type: Type<'db>,
    invalid_expressions: smallvec::SmallVec<[InvalidTypeExpression<'db>; 1]>,
}

impl<'db> InvalidTypeExpressionError<'db> {
    fn into_fallback_type(
        self,
        context: &InferContext,
        node: &impl Ranged,
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
                error.add_subdiagnostics(context.db(), diagnostic, node);
            }
        }
        fallback_type
    }
}

/// Enumeration of various types that are invalid in type-expression contexts
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
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
    /// Same for `typing.TypedDict`
    TypedDict,
    /// Same for `typing.TypeAlias`, anywhere except for as the sole annotation on an annotated
    /// assignment
    TypeAlias,
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
                    InvalidTypeExpression::RequiresOneArgument(special_form) => write!(
                        f,
                        "`{special_form}` requires exactly one argument when used in a type expression",
                    ),
                    InvalidTypeExpression::RequiresArguments(special_form) => write!(
                        f,
                        "`{special_form}` requires at least one argument when used in a type expression",
                    ),
                    InvalidTypeExpression::RequiresTwoArguments(special_form) => write!(
                        f,
                        "`{special_form}` requires at least two arguments when used in a type expression",
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
                    InvalidTypeExpression::ConstraintSet => f.write_str(
                        "`ty_extensions.ConstraintSet` is not allowed in type expressions",
                    ),
                    InvalidTypeExpression::GenericContext => f.write_str(
                        "`ty_extensions.GenericContext` is not allowed in type expressions",
                    ),
                    InvalidTypeExpression::Specialization => f.write_str(
                        "`ty_extensions.GenericContext` is not allowed in type expressions",
                    ),
                    InvalidTypeExpression::TypedDict => f.write_str(
                        "The special form `typing.TypedDict` \
                            is not allowed in type expressions",
                    ),
                    InvalidTypeExpression::TypeAlias => f.write_str(
                        "`typing.TypeAlias` is only allowed \
                            as the sole annotation on an annotated assignment",
                    ),
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
                    InvalidTypeExpression::InvalidType(Type::ModuleLiteral(module), _) => write!(
                        f,
                        "Module `{module}` is not valid in a type expression",
                        module = module.module(self.db).name(self.db)
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
                .in_type_expression(db, scope, None)
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
        }
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
    pub default_type: Option<Type<'db>>,

    /// Whether this field is part of the `__init__` signature, or not.
    pub init: bool,

    /// Whether or not this field can only be passed as a keyword argument to `__init__`.
    pub kw_only: Option<bool>,

    /// This name is used to provide an alternative parameter name in the synthesized `__init__` method.
    pub alias: Option<Box<str>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for FieldInstance<'_> {}

impl<'db> FieldInstance<'db> {
    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        FieldInstance::new(
            db,
            self.default_type(db)
                .map(|ty| ty.normalized_impl(db, visitor)),
            self.init(db),
            self.kw_only(db),
            self.alias(db),
        )
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let default_type = match self.default_type(db) {
            Some(default) if nested => Some(default.recursive_type_normalized_impl(db, div, true)?),
            Some(default) => Some(
                default
                    .recursive_type_normalized_impl(db, div, true)
                    .unwrap_or(div),
            ),
            None => None,
        };
        Some(FieldInstance::new(
            db,
            default_type,
            self.init(db),
            self.kw_only(db),
            self.alias(db),
        ))
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
    TypingSelf,
    /// `P = ParamSpec("P")`
    ParamSpec,
    /// `def foo[**P]() -> None: ...`
    Pep695ParamSpec,
}

impl TypeVarKind {
    const fn is_self(self) -> bool {
        matches!(self, Self::TypingSelf)
    }

    const fn is_paramspec(self) -> bool {
        matches!(self, Self::ParamSpec | Self::Pep695ParamSpec)
    }
}

/// The identity of a type variable.
///
/// This represents the core identity of a typevar, independent of its bounds or constraints. Two
/// typevars have the same identity if they represent the same logical typevar, even if their
/// bounds have been materialized differently.
///
/// # Ordering
/// Ordering is based on the identity's salsa-assigned id and not on its values.
/// The id may change between runs, or when the identity was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct TypeVarIdentity<'db> {
    /// The name of this TypeVar (e.g. `T`)
    #[returns(ref)]
    pub(crate) name: ast::name::Name,

    /// The type var's definition (None if synthesized)
    pub(crate) definition: Option<Definition<'db>>,

    /// The kind of typevar (PEP 695, Legacy, or TypingSelf)
    pub(crate) kind: TypeVarKind,
}

impl get_size2::GetSize for TypeVarIdentity<'_> {}

/// A specific instance of a type variable that has not been bound to a generic context yet.
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
    /// The identity of this typevar
    pub(crate) identity: TypeVarIdentity<'db>,

    /// The upper bound or constraint on the type of this TypeVar, if any. Don't use this field
    /// directly; use the `bound_or_constraints` (or `upper_bound` and `constraints`) methods
    /// instead (to evaluate any lazy bound or constraints).
    _bound_or_constraints: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,

    /// The explicitly specified variance of the TypeVar
    explicit_variance: Option<TypeVarVariance>,

    /// The default type for this TypeVar, if any. Don't use this field directly, use the
    /// `default_type` method instead (to evaluate any lazy default).
    _default: Option<TypeVarDefaultEvaluation<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypeVarInstance<'_> {}

fn walk_type_var_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    typevar: TypeVarInstance<'db>,
    visitor: &V,
) {
    if let Some(bound_or_constraints) = if visitor.should_visit_lazy_type_attributes() {
        typevar.bound_or_constraints(db)
    } else {
        match typevar._bound_or_constraints(db) {
            _ if visitor.should_visit_lazy_type_attributes() => typevar.bound_or_constraints(db),
            Some(TypeVarBoundOrConstraintsEvaluation::Eager(bound_or_constraints)) => {
                Some(bound_or_constraints)
            }
            _ => None,
        }
    } {
        walk_type_var_bounds(db, bound_or_constraints, visitor);
    }
    if let Some(default_type) = if visitor.should_visit_lazy_type_attributes() {
        typevar.default_type(db)
    } else {
        match typevar._default(db) {
            Some(TypeVarDefaultEvaluation::Eager(default_type)) => Some(default_type),
            _ => None,
        }
    } {
        visitor.visit_type(db, default_type);
    }
}

#[salsa::tracked]
impl<'db> TypeVarInstance<'db> {
    pub(crate) fn with_binding_context(
        self,
        db: &'db dyn Db,
        binding_context: Definition<'db>,
    ) -> BoundTypeVarInstance<'db> {
        BoundTypeVarInstance::new(db, self, BindingContext::Definition(binding_context), None)
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db ast::name::Name {
        self.identity(db).name(db)
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        self.identity(db).definition(db)
    }

    pub fn kind(self, db: &'db dyn Db) -> TypeVarKind {
        self.identity(db).kind(db)
    }

    pub(crate) fn is_self(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), TypeVarKind::TypingSelf)
    }

    pub(crate) fn is_paramspec(self, db: &'db dyn Db) -> bool {
        self.kind(db).is_paramspec()
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

    pub(crate) fn bound_or_constraints(
        self,
        db: &'db dyn Db,
    ) -> Option<TypeVarBoundOrConstraints<'db>> {
        self._bound_or_constraints(db).and_then(|w| match w {
            TypeVarBoundOrConstraintsEvaluation::Eager(bound_or_constraints) => {
                Some(bound_or_constraints)
            }
            TypeVarBoundOrConstraintsEvaluation::LazyUpperBound => self.lazy_bound(db),
            TypeVarBoundOrConstraintsEvaluation::LazyConstraints => self.lazy_constraints(db),
        })
    }

    pub(crate) fn default_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        self._default(db).and_then(|d| match d {
            TypeVarDefaultEvaluation::Eager(ty) => Some(ty),
            TypeVarDefaultEvaluation::Lazy => self.lazy_default(db),
        })
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self::new(
            db,
            self.identity(db),
            self._bound_or_constraints(db)
                .and_then(|bound_or_constraints| match bound_or_constraints {
                    TypeVarBoundOrConstraintsEvaluation::Eager(bound_or_constraints) => {
                        Some(bound_or_constraints.normalized_impl(db, visitor).into())
                    }
                    TypeVarBoundOrConstraintsEvaluation::LazyUpperBound => self
                        .lazy_bound(db)
                        .map(|bound| bound.normalized_impl(db, visitor).into()),
                    TypeVarBoundOrConstraintsEvaluation::LazyConstraints => self
                        .lazy_constraints(db)
                        .map(|constraints| constraints.normalized_impl(db, visitor).into()),
                }),
            self.explicit_variance(db),
            self._default(db).and_then(|default| match default {
                TypeVarDefaultEvaluation::Eager(ty) => Some(ty.normalized_impl(db, visitor).into()),
                TypeVarDefaultEvaluation::Lazy => self
                    .lazy_default(db)
                    .map(|ty| ty.normalized_impl(db, visitor).into()),
            }),
        )
    }

    fn materialize_impl(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self::new(
            db,
            self.identity(db),
            self._bound_or_constraints(db)
                .and_then(|bound_or_constraints| match bound_or_constraints {
                    TypeVarBoundOrConstraintsEvaluation::Eager(bound_or_constraints) => Some(
                        bound_or_constraints
                            .materialize_impl(db, materialization_kind, visitor)
                            .into(),
                    ),
                    TypeVarBoundOrConstraintsEvaluation::LazyUpperBound => {
                        self.lazy_bound(db).map(|bound| {
                            bound
                                .materialize_impl(db, materialization_kind, visitor)
                                .into()
                        })
                    }
                    TypeVarBoundOrConstraintsEvaluation::LazyConstraints => {
                        self.lazy_constraints(db).map(|constraints| {
                            constraints
                                .materialize_impl(db, materialization_kind, visitor)
                                .into()
                        })
                    }
                }),
            self.explicit_variance(db),
            self._default(db).and_then(|default| match default {
                TypeVarDefaultEvaluation::Eager(ty) => {
                    Some(ty.materialize(db, materialization_kind, visitor).into())
                }
                TypeVarDefaultEvaluation::Lazy => self
                    .lazy_default(db)
                    .map(|ty| ty.materialize(db, materialization_kind, visitor).into()),
            }),
        )
    }

    fn to_instance(self, db: &'db dyn Db) -> Option<Self> {
        let bound_or_constraints = match self.bound_or_constraints(db)? {
            TypeVarBoundOrConstraints::UpperBound(upper_bound) => {
                TypeVarBoundOrConstraints::UpperBound(upper_bound.to_instance(db)?)
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                TypeVarBoundOrConstraints::Constraints(constraints.to_instance(db)?)
            }
        };
        let identity = TypeVarIdentity::new(
            db,
            Name::new(format!("{}'instance", self.name(db))),
            None, // definition
            self.kind(db),
        );
        Some(Self::new(
            db,
            identity,
            Some(bound_or_constraints.into()),
            self.explicit_variance(db),
            None, // _default
        ))
    }

    #[salsa::tracked(
        cycle_fn=lazy_bound_or_constraints_cycle_recover,
        cycle_initial=lazy_bound_or_constraints_cycle_initial,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn lazy_bound(self, db: &'db dyn Db) -> Option<TypeVarBoundOrConstraints<'db>> {
        let definition = self.definition(db)?;
        let module = parsed_module(db, definition.file(db)).load(db);
        let ty = match definition.kind(db) {
            // PEP 695 typevar
            DefinitionKind::TypeVar(typevar) => {
                let typevar_node = typevar.node(&module);
                definition_expression_type(db, definition, typevar_node.bound.as_ref()?)
            }
            // legacy typevar
            DefinitionKind::Assignment(assignment) => {
                let call_expr = assignment.value(&module).as_call_expr()?;
                let expr = &call_expr.arguments.find_keyword("bound")?.value;
                definition_expression_type(db, definition, expr)
            }
            _ => return None,
        };
        Some(TypeVarBoundOrConstraints::UpperBound(ty))
    }

    #[salsa::tracked(
        cycle_fn=lazy_bound_or_constraints_cycle_recover,
        cycle_initial=lazy_bound_or_constraints_cycle_initial,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn lazy_constraints(self, db: &'db dyn Db) -> Option<TypeVarBoundOrConstraints<'db>> {
        let definition = self.definition(db)?;
        let module = parsed_module(db, definition.file(db)).load(db);
        let constraints = match definition.kind(db) {
            // PEP 695 typevar
            DefinitionKind::TypeVar(typevar) => {
                let typevar_node = typevar.node(&module);
                let bound =
                    definition_expression_type(db, definition, typevar_node.bound.as_ref()?);
                let constraints = if let Some(tuple) = bound
                    .as_nominal_instance()
                    .and_then(|instance| instance.tuple_spec(db))
                {
                    if let Tuple::Fixed(tuple) = tuple.into_owned() {
                        tuple.owned_elements()
                    } else {
                        vec![Type::unknown()].into_boxed_slice()
                    }
                } else {
                    vec![Type::unknown()].into_boxed_slice()
                };
                TypeVarConstraints::new(db, constraints)
            }
            // legacy typevar
            DefinitionKind::Assignment(assignment) => {
                let call_expr = assignment.value(&module).as_call_expr()?;
                TypeVarConstraints::new(
                    db,
                    call_expr
                        .arguments
                        .args
                        .iter()
                        .skip(1)
                        .map(|arg| definition_expression_type(db, definition, arg))
                        .collect::<Box<_>>(),
                )
            }
            _ => return None,
        };
        Some(TypeVarBoundOrConstraints::Constraints(constraints))
    }

    #[salsa::tracked(cycle_fn=lazy_default_cycle_recover, cycle_initial=lazy_default_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    fn lazy_default(self, db: &'db dyn Db) -> Option<Type<'db>> {
        fn convert_type_to_paramspec_value<'db>(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
            let parameters = match ty {
                Type::NominalInstance(nominal_instance)
                    if nominal_instance.has_known_class(db, KnownClass::EllipsisType) =>
                {
                    Parameters::gradual_form()
                }
                Type::NominalInstance(nominal_instance) => nominal_instance
                    .own_tuple_spec(db)
                    .map_or_else(Parameters::unknown, |tuple_spec| {
                        Parameters::new(
                            db,
                            tuple_spec.all_elements().map(|ty| {
                                Parameter::positional_only(None).with_annotated_type(*ty)
                            }),
                        )
                    }),
                Type::Dynamic(dynamic) => match dynamic {
                    DynamicType::Todo(_)
                    | DynamicType::TodoUnpack
                    | DynamicType::TodoStarredExpression => Parameters::todo(),
                    DynamicType::Any
                    | DynamicType::Unknown
                    | DynamicType::UnknownGeneric(_)
                    | DynamicType::Divergent(_) => Parameters::unknown(),
                },
                Type::TypeVar(typevar) if typevar.is_paramspec(db) => {
                    return ty;
                }
                Type::KnownInstance(KnownInstanceType::TypeVar(typevar))
                    if typevar.is_paramspec(db) =>
                {
                    return ty;
                }
                _ => Parameters::unknown(),
            };
            Type::paramspec_value_callable(db, parameters)
        }

        let definition = self.definition(db)?;
        let module = parsed_module(db, definition.file(db)).load(db);
        match definition.kind(db) {
            // PEP 695 typevar
            DefinitionKind::TypeVar(typevar) => {
                let typevar_node = typevar.node(&module);
                Some(definition_expression_type(
                    db,
                    definition,
                    typevar_node.default.as_ref()?,
                ))
            }
            // legacy typevar / ParamSpec
            DefinitionKind::Assignment(assignment) => {
                let call_expr = assignment.value(&module).as_call_expr()?;
                let func_ty = definition_expression_type(db, definition, &call_expr.func);
                let known_class = func_ty.as_class_literal().and_then(|cls| cls.known(db));
                let expr = &call_expr.arguments.find_keyword("default")?.value;
                let default_type = definition_expression_type(db, definition, expr);
                if known_class == Some(KnownClass::ParamSpec) {
                    Some(convert_type_to_paramspec_value(db, default_type))
                } else {
                    Some(default_type)
                }
            }
            // PEP 695 ParamSpec
            DefinitionKind::ParamSpec(paramspec) => {
                let paramspec_node = paramspec.node(&module);
                let default_ty =
                    definition_expression_type(db, definition, paramspec_node.default.as_ref()?);
                Some(convert_type_to_paramspec_value(db, default_ty))
            }
            _ => None,
        }
    }

    pub fn bind_pep695(self, db: &'db dyn Db) -> Option<BoundTypeVarInstance<'db>> {
        if !matches!(
            self.identity(db).kind(db),
            TypeVarKind::Pep695 | TypeVarKind::Pep695ParamSpec
        ) {
            return None;
        }
        let typevar_definition = self.definition(db)?;
        let index = semantic_index(db, typevar_definition.file(db));
        let (_, child) = index
            .child_scopes(typevar_definition.file_scope(db))
            .next()?;
        child
            .node()
            .generic_context(db, index)?
            .binds_typevar(db, self)
    }
}

fn lazy_bound_or_constraints_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: TypeVarInstance<'db>,
) -> Option<TypeVarBoundOrConstraints<'db>> {
    None
}

#[expect(clippy::ref_option)]
fn lazy_bound_or_constraints_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &Option<TypeVarBoundOrConstraints<'db>>,
    current: Option<TypeVarBoundOrConstraints<'db>>,
    _typevar: TypeVarInstance<'db>,
) -> Option<TypeVarBoundOrConstraints<'db>> {
    // Normalize the bounds/constraints to ensure cycle convergence.
    match (previous, current) {
        (Some(prev), Some(current)) => Some(current.cycle_normalized(db, *prev, cycle)),
        (None, Some(current)) => Some(current.recursive_type_normalized(db, cycle)),
        (_, None) => None,
    }
}

#[expect(clippy::ref_option)]
fn lazy_default_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_default: &Option<Type<'db>>,
    default: Option<Type<'db>>,
    _typevar: TypeVarInstance<'db>,
) -> Option<Type<'db>> {
    // Normalize the default to ensure cycle convergence.
    match (previous_default, default) {
        (Some(prev), Some(default)) => Some(default.cycle_normalized(db, *prev, cycle)),
        (None, Some(default)) => Some(default.recursive_type_normalized(db, cycle)),
        (_, None) => None,
    }
}

#[allow(clippy::unnecessary_wraps)]
fn lazy_default_cycle_initial<'db>(
    _db: &'db dyn Db,
    id: salsa::Id,
    _self: TypeVarInstance<'db>,
) -> Option<Type<'db>> {
    Some(Type::divergent(id))
}

/// Where a type variable is bound and usable.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, salsa::Update, get_size2::GetSize,
)]
pub enum BindingContext<'db> {
    /// The definition of the generic class, function, or type alias that binds this typevar.
    Definition(Definition<'db>),
    /// The typevar is synthesized internally, and is not associated with a particular definition
    /// in the source, but is still bound and eligible for specialization inference.
    Synthetic,
}

impl<'db> From<Definition<'db>> for BindingContext<'db> {
    fn from(definition: Definition<'db>) -> Self {
        BindingContext::Definition(definition)
    }
}

impl<'db> BindingContext<'db> {
    pub(crate) fn definition(self) -> Option<Definition<'db>> {
        match self {
            BindingContext::Definition(definition) => Some(definition),
            BindingContext::Synthetic => None,
        }
    }

    fn name(self, db: &'db dyn Db) -> Option<String> {
        self.definition().and_then(|definition| definition.name(db))
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, get_size2::GetSize)]
pub enum ParamSpecAttrKind {
    Args,
    Kwargs,
}

impl std::fmt::Display for ParamSpecAttrKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamSpecAttrKind::Args => f.write_str("args"),
            ParamSpecAttrKind::Kwargs => f.write_str("kwargs"),
        }
    }
}

/// The identity of a bound type variable.
///
/// This identifies a specific binding of a typevar to a context (e.g., `T@ClassC` vs `T@FunctionF`),
/// independent of the typevar's bounds or constraints. Two bound typevars have the same identity
/// if they represent the same logical typevar bound in the same context, even if their bounds
/// have been materialized differently.
#[derive(
    Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd, get_size2::GetSize, salsa::Update,
)]
pub struct BoundTypeVarIdentity<'db> {
    pub(crate) identity: TypeVarIdentity<'db>,
    pub(crate) binding_context: BindingContext<'db>,
    /// If [`Some`], this indicates that this type variable is the `args` or `kwargs` component
    /// of a `ParamSpec` i.e., `P.args` or `P.kwargs`.
    paramspec_attr: Option<ParamSpecAttrKind>,
}

/// A type variable that has been bound to a generic context, and which can be specialized to a
/// concrete type.
///
/// # Ordering
///
/// Ordering is based on the wrapped data's salsa-assigned id and not on its values.
/// The id may change between runs, or when e.g. a `BoundTypeVarInstance` was garbage-collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct BoundTypeVarInstance<'db> {
    pub typevar: TypeVarInstance<'db>,
    binding_context: BindingContext<'db>,
    /// If [`Some`], this indicates that this type variable is the `args` or `kwargs` component
    /// of a `ParamSpec` i.e., `P.args` or `P.kwargs`.
    paramspec_attr: Option<ParamSpecAttrKind>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BoundTypeVarInstance<'_> {}

impl<'db> BoundTypeVarInstance<'db> {
    /// Get the identity of this bound typevar.
    ///
    /// This is used for comparing whether two bound typevars represent the same logical typevar,
    /// regardless of e.g. differences in their bounds or constraints due to materialization.
    pub(crate) fn identity(self, db: &'db dyn Db) -> BoundTypeVarIdentity<'db> {
        BoundTypeVarIdentity {
            identity: self.typevar(db).identity(db),
            binding_context: self.binding_context(db),
            paramspec_attr: self.paramspec_attr(db),
        }
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db ast::name::Name {
        self.typevar(db).name(db)
    }

    pub(crate) fn kind(self, db: &'db dyn Db) -> TypeVarKind {
        self.typevar(db).kind(db)
    }

    pub(crate) fn is_paramspec(self, db: &'db dyn Db) -> bool {
        self.kind(db).is_paramspec()
    }

    /// Returns a new bound typevar instance with the given `ParamSpec` attribute set.
    ///
    /// This method will also set an appropriate upper bound on the typevar, based on the
    /// attribute kind. For `P.args`, the upper bound will be `tuple[object, ...]`, and for
    /// `P.kwargs`, the upper bound will be `Top[dict[str, Any]]`.
    ///
    /// It's the caller's responsibility to ensure that this method is only called on a `ParamSpec`
    /// type variable.
    pub(crate) fn with_paramspec_attr(self, db: &'db dyn Db, kind: ParamSpecAttrKind) -> Self {
        debug_assert!(
            self.is_paramspec(db),
            "Expected a ParamSpec, got {:?}",
            self.kind(db)
        );

        let upper_bound = TypeVarBoundOrConstraints::UpperBound(match kind {
            ParamSpecAttrKind::Args => Type::homogeneous_tuple(db, Type::object()),
            ParamSpecAttrKind::Kwargs => KnownClass::Dict
                .to_specialized_instance(db, [KnownClass::Str.to_instance(db), Type::any()])
                .top_materialization(db),
        });

        let typevar = TypeVarInstance::new(
            db,
            self.typevar(db).identity(db),
            Some(TypeVarBoundOrConstraintsEvaluation::Eager(upper_bound)),
            None, // ParamSpecs cannot have explicit variance
            None, // `P.args` and `P.kwargs` cannot have defaults even though `P` can
        );

        Self::new(db, typevar, self.binding_context(db), Some(kind))
    }

    /// Returns a new bound typevar instance without any `ParamSpec` attribute set.
    ///
    /// This method will also remove any upper bound that was set by `with_paramspec_attr`. This
    /// means that the returned typevar will have no upper bound or constraints.
    ///
    /// It's the caller's responsibility to ensure that this method is only called on a `ParamSpec`
    /// type variable.
    pub(crate) fn without_paramspec_attr(self, db: &'db dyn Db) -> Self {
        debug_assert!(
            self.is_paramspec(db),
            "Expected a ParamSpec, got {:?}",
            self.kind(db)
        );

        Self::new(
            db,
            TypeVarInstance::new(
                db,
                self.typevar(db).identity(db),
                None, // Remove the upper bound set by `with_paramspec_attr`
                None, // ParamSpecs cannot have explicit variance
                None, // `P.args` and `P.kwargs` cannot have defaults even though `P` can
            ),
            self.binding_context(db),
            None,
        )
    }

    /// Returns whether two bound typevars represent the same logical typevar, regardless of e.g.
    /// differences in their bounds or constraints due to materialization.
    pub(crate) fn is_same_typevar_as(self, db: &'db dyn Db, other: Self) -> bool {
        self.identity(db) == other.identity(db)
    }

    /// Create a new PEP 695 type variable that can be used in signatures
    /// of synthetic generic functions.
    pub(crate) fn synthetic(
        db: &'db dyn Db,
        name: &'static str,
        variance: TypeVarVariance,
    ) -> Self {
        let identity = TypeVarIdentity::new(
            db,
            Name::new_static(name),
            None, // definition
            TypeVarKind::Pep695,
        );
        let typevar = TypeVarInstance::new(
            db,
            identity,
            None, // _bound_or_constraints
            Some(variance),
            None, // _default
        );
        Self::new(db, typevar, BindingContext::Synthetic, None)
    }

    /// Create a new synthetic `Self` type variable with the given upper bound.
    pub(crate) fn synthetic_self(
        db: &'db dyn Db,
        upper_bound: Type<'db>,
        binding_context: BindingContext<'db>,
    ) -> Self {
        let identity = TypeVarIdentity::new(
            db,
            Name::new_static("Self"),
            None, // definition
            TypeVarKind::TypingSelf,
        );
        let typevar = TypeVarInstance::new(
            db,
            identity,
            Some(TypeVarBoundOrConstraints::UpperBound(upper_bound).into()),
            Some(TypeVarVariance::Invariant),
            None, // _default
        );
        Self::new(db, typevar, binding_context, None)
    }

    /// Returns an identical type variable with its `TypeVarBoundOrConstraints` mapped by the
    /// provided closure.
    pub(crate) fn map_bound_or_constraints(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(Option<TypeVarBoundOrConstraints<'db>>) -> Option<TypeVarBoundOrConstraints<'db>>,
    ) -> Self {
        let bound_or_constraints = f(self.typevar(db).bound_or_constraints(db));
        let typevar = TypeVarInstance::new(
            db,
            self.typevar(db).identity(db),
            bound_or_constraints.map(TypeVarBoundOrConstraintsEvaluation::Eager),
            self.typevar(db).explicit_variance(db),
            self.typevar(db)._default(db),
        );

        Self::new(
            db,
            typevar,
            self.binding_context(db),
            self.paramspec_attr(db),
        )
    }

    pub(crate) fn variance_with_polarity(
        self,
        db: &'db dyn Db,
        polarity: TypeVarVariance,
    ) -> TypeVarVariance {
        let _span = tracing::trace_span!("variance_with_polarity").entered();
        match self.typevar(db).explicit_variance(db) {
            Some(explicit_variance) => explicit_variance.compose(polarity),
            None => match self.binding_context(db) {
                BindingContext::Definition(definition) => binding_type(db, definition)
                    .with_polarity(polarity)
                    .variance_of(db, self),
                BindingContext::Synthetic => TypeVarVariance::Invariant,
            },
        }
    }

    pub fn variance(self, db: &'db dyn Db) -> TypeVarVariance {
        self.variance_with_polarity(db, TypeVarVariance::Covariant)
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Type<'db> {
        match type_mapping {
            TypeMapping::Specialization(specialization) => {
                let typevar = if self.is_paramspec(db) {
                    self.without_paramspec_attr(db)
                } else {
                    self
                };
                specialization
                    .get(db, typevar)
                    .map(|ty| {
                        if let Some(attr) = self.paramspec_attr(db)
                            && let Type::TypeVar(typevar) = ty
                            && typevar.is_paramspec(db)
                        {
                            return Type::TypeVar(typevar.with_paramspec_attr(db, attr));
                        }
                        ty
                    })
                    .unwrap_or(Type::TypeVar(self))
            }
            TypeMapping::PartialSpecialization(partial) => {
                let typevar = if self.is_paramspec(db) {
                    self.without_paramspec_attr(db)
                } else {
                    self
                };
                partial
                    .get(db, typevar)
                    .map(|ty| {
                        if let Some(attr) = self.paramspec_attr(db)
                            && let Type::TypeVar(typevar) = ty
                            && typevar.is_paramspec(db)
                        {
                            return Type::TypeVar(typevar.with_paramspec_attr(db, attr));
                        }
                        ty
                    })
                    .unwrap_or(Type::TypeVar(self))
            }
            TypeMapping::BindSelf {
                self_type,
                binding_context,
            } => {
                if self.typevar(db).is_self(db)
                    && binding_context.is_none_or(|context| self.binding_context(db) == context)
                {
                    *self_type
                } else {
                    Type::TypeVar(self)
                }
            }
            TypeMapping::ReplaceSelf { new_upper_bound } => {
                if self.typevar(db).is_self(db) {
                    Type::TypeVar(BoundTypeVarInstance::synthetic_self(
                        db,
                        *new_upper_bound,
                        self.binding_context(db),
                    ))
                } else {
                    Type::TypeVar(self)
                }
            }
            TypeMapping::PromoteLiterals(_)
            | TypeMapping::ReplaceParameterDefaults
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::EagerExpansion => Type::TypeVar(self),
            TypeMapping::Materialize(materialization_kind) => {
                Type::TypeVar(self.materialize_impl(db, *materialization_kind, visitor))
            }
        }
    }
}

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
    pub(crate) fn default_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let binding_context = self.binding_context(db);
        self.typevar(db).default_type(db).map(|ty| {
            ty.apply_type_mapping(
                db,
                &TypeMapping::BindLegacyTypevars(binding_context),
                TypeContext::default(),
            )
        })
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self::new(
            db,
            self.typevar(db).normalized_impl(db, visitor),
            self.binding_context(db),
            self.paramspec_attr(db),
        )
    }

    fn materialize_impl(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self::new(
            db,
            self.typevar(db)
                .materialize_impl(db, materialization_kind, visitor),
            self.binding_context(db),
            self.paramspec_attr(db),
        )
    }

    fn to_instance(self, db: &'db dyn Db) -> Option<Self> {
        Some(Self::new(
            db,
            self.typevar(db).to_instance(db)?,
            self.binding_context(db),
            self.paramspec_attr(db),
        ))
    }
}

/// Whether a typevar default is eagerly specified or lazily evaluated.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub enum TypeVarDefaultEvaluation<'db> {
    /// The default type is lazily evaluated.
    Lazy,
    /// The default type is eagerly specified.
    Eager(Type<'db>),
}

impl<'db> From<Type<'db>> for TypeVarDefaultEvaluation<'db> {
    fn from(value: Type<'db>) -> Self {
        TypeVarDefaultEvaluation::Eager(value)
    }
}

/// Whether a typevar bound/constraints is eagerly specified or lazily evaluated.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub enum TypeVarBoundOrConstraintsEvaluation<'db> {
    /// There is a lazily-evaluated upper bound.
    LazyUpperBound,
    /// There is a lazily-evaluated set of constraints.
    LazyConstraints,
    /// The upper bound/constraints are eagerly specified.
    Eager(TypeVarBoundOrConstraints<'db>),
}

impl<'db> From<TypeVarBoundOrConstraints<'db>> for TypeVarBoundOrConstraintsEvaluation<'db> {
    fn from(value: TypeVarBoundOrConstraints<'db>) -> Self {
        TypeVarBoundOrConstraintsEvaluation::Eager(value)
    }
}

/// Type variable constraints (e.g. `T: (int, str)`).
/// This is structurally identical to [`UnionType`], except that it does not perform simplification and preserves the element types.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeVarConstraints<'db> {
    #[returns(ref)]
    elements: Box<[Type<'db>]>,
}

impl get_size2::GetSize for TypeVarConstraints<'_> {}

fn walk_type_var_constraints<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    constraints: TypeVarConstraints<'db>,
    visitor: &V,
) {
    for ty in constraints.elements(db) {
        visitor.visit_type(db, *ty);
    }
}

impl<'db> TypeVarConstraints<'db> {
    fn as_type(self, db: &'db dyn Db) -> Type<'db> {
        let mut builder = UnionBuilder::new(db);
        for ty in self.elements(db) {
            builder = builder.add(*ty);
        }
        builder.build()
    }

    fn to_instance(self, db: &'db dyn Db) -> Option<TypeVarConstraints<'db>> {
        let mut instance_elements = Vec::new();
        for ty in self.elements(db) {
            instance_elements.push(ty.to_instance(db)?);
        }
        Some(TypeVarConstraints::new(
            db,
            instance_elements.into_boxed_slice(),
        ))
    }

    fn map(self, db: &'db dyn Db, transform_fn: impl FnMut(&Type<'db>) -> Type<'db>) -> Self {
        let mapped = self
            .elements(db)
            .iter()
            .map(transform_fn)
            .collect::<Box<_>>();
        TypeVarConstraints::new(db, mapped)
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
        let mut origin = TypeOrigin::Declared;
        for ty in self.elements(db) {
            let PlaceAndQualifiers {
                place: ty_member,
                qualifiers: new_qualifiers,
            } = transform_fn(ty);
            qualifiers |= new_qualifiers;
            match ty_member {
                Place::Undefined => {
                    possibly_unbound = true;
                }
                Place::Defined(ty_member, member_origin, member_boundness) => {
                    origin = origin.merge(member_origin);
                    if member_boundness == Definedness::PossiblyUndefined {
                        possibly_unbound = true;
                    }

                    all_unbound = false;
                    builder = builder.add(ty_member);
                }
            }
        }
        PlaceAndQualifiers {
            place: if all_unbound {
                Place::Undefined
            } else {
                Place::Defined(
                    builder.build(),
                    origin,
                    if possibly_unbound {
                        Definedness::PossiblyUndefined
                    } else {
                        Definedness::AlwaysDefined
                    },
                )
            },
            qualifiers,
        }
    }

    fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        let normalized = self
            .elements(db)
            .iter()
            .map(|ty| ty.normalized_impl(db, visitor))
            .collect::<Box<_>>();
        TypeVarConstraints::new(db, normalized)
    }

    fn materialize_impl(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let materialized = self
            .elements(db)
            .iter()
            .map(|ty| ty.materialize(db, materialization_kind, visitor))
            .collect::<Box<_>>();
        TypeVarConstraints::new(db, materialized)
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub enum TypeVarBoundOrConstraints<'db> {
    UpperBound(Type<'db>),
    Constraints(TypeVarConstraints<'db>),
}

fn walk_type_var_bounds<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    bounds: TypeVarBoundOrConstraints<'db>,
    visitor: &V,
) {
    match bounds {
        TypeVarBoundOrConstraints::UpperBound(bound) => visitor.visit_type(db, bound),
        TypeVarBoundOrConstraints::Constraints(constraints) => {
            walk_type_var_constraints(db, constraints, visitor);
        }
    }
}

impl<'db> TypeVarBoundOrConstraints<'db> {
    fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        match self {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                TypeVarBoundOrConstraints::UpperBound(bound.normalized_impl(db, visitor))
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                TypeVarBoundOrConstraints::Constraints(constraints.normalized_impl(db, visitor))
            }
        }
    }

    /// Normalize for cycle recovery by combining with the previous value and
    /// removing divergent types introduced by the cycle.
    ///
    /// See [`Type::cycle_normalized`] for more details on how this works.
    fn cycle_normalized(self, db: &'db dyn Db, previous: Self, cycle: &salsa::Cycle) -> Self {
        match (self, previous) {
            (
                TypeVarBoundOrConstraints::UpperBound(bound),
                TypeVarBoundOrConstraints::UpperBound(prev_bound),
            ) => {
                TypeVarBoundOrConstraints::UpperBound(bound.cycle_normalized(db, prev_bound, cycle))
            }
            (
                TypeVarBoundOrConstraints::Constraints(constraints),
                TypeVarBoundOrConstraints::Constraints(prev_constraints),
            ) => {
                // Normalize each constraint with its corresponding previous constraint
                let current_elements = constraints.elements(db);
                let prev_elements = prev_constraints.elements(db);
                TypeVarBoundOrConstraints::Constraints(TypeVarConstraints::new(
                    db,
                    current_elements
                        .iter()
                        .zip(prev_elements.iter())
                        .map(|(ty, prev_ty)| ty.cycle_normalized(db, *prev_ty, cycle))
                        .collect::<Box<_>>(),
                ))
            }
            // The choice of whether it's an upper bound or constraints is purely syntactic and
            // thus can never change in a cycle: `parsed_module` does not participate in cycles,
            // the AST will never change from one iteration to the next.
            _ => unreachable!(
                "TypeVar switched from bound to constraints (or vice versa) in fixpoint iteration"
            ),
        }
    }

    /// Normalize recursive types for cycle recovery when there's no previous value.
    ///
    /// See [`Type::recursive_type_normalized`] for more details.
    fn recursive_type_normalized(self, db: &'db dyn Db, cycle: &salsa::Cycle) -> Self {
        match self {
            TypeVarBoundOrConstraints::UpperBound(bound) => {
                TypeVarBoundOrConstraints::UpperBound(bound.recursive_type_normalized(db, cycle))
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                TypeVarBoundOrConstraints::Constraints(
                    constraints.map(db, |ty| ty.recursive_type_normalized(db, cycle)),
                )
            }
        }
    }

    fn materialize_impl(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self {
            TypeVarBoundOrConstraints::UpperBound(bound) => TypeVarBoundOrConstraints::UpperBound(
                bound.materialize(db, materialization_kind, visitor),
            ),
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                TypeVarBoundOrConstraints::Constraints(constraints.materialize_impl(
                    db,
                    materialization_kind,
                    visitor,
                ))
            }
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

/// Contains information about a `types.UnionType` instance built from a PEP 604
/// union or a legacy `typing.Union[…]` annotation in a value expression context,
/// e.g. `IntOrStr = int | str` or `IntOrStr = Union[int, str]`.
///
/// # Ordering
/// Ordering is based on the context's salsa-assigned id and not on its values.
/// The id may change between runs, or when the context was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct UnionTypeInstance<'db> {
    /// The types of the elements of this union, as they were inferred in a value
    /// expression context. For `int | str`, this would contain `<class 'int'>` and
    /// `<class 'str'>`. For `Union[int, str]`, this field is `None`, as we infer
    /// the elements as type expressions. Use `value_expression_types` to get the
    /// corresponding value expression types.
    #[expect(clippy::ref_option)]
    #[returns(ref)]
    _value_expr_types: Option<Box<[Type<'db>]>>,

    /// The type of the full union, which can be used when this `UnionType` instance
    /// is used in a type expression context. For `int | str`, this would contain
    /// `Ok(int | str)`. If any of the element types could not be converted, this
    /// contains the first encountered error.
    #[returns(ref)]
    union_type: Result<Type<'db>, InvalidTypeExpressionError<'db>>,
}

impl get_size2::GetSize for UnionTypeInstance<'_> {}

impl<'db> UnionTypeInstance<'db> {
    pub(crate) fn from_value_expression_types(
        db: &'db dyn Db,
        value_expr_types: impl IntoIterator<Item = Type<'db>>,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
    ) -> Type<'db> {
        let value_expr_types = value_expr_types.into_iter().collect::<Box<_>>();

        let mut builder = UnionBuilder::new(db);
        for ty in &value_expr_types {
            match ty.in_type_expression(db, scope_id, typevar_binding_context) {
                Ok(ty) => builder.add_in_place(ty),
                Err(error) => {
                    return Type::KnownInstance(KnownInstanceType::UnionType(
                        UnionTypeInstance::new(db, Some(value_expr_types), Err(error)),
                    ));
                }
            }
        }

        Type::KnownInstance(KnownInstanceType::UnionType(UnionTypeInstance::new(
            db,
            Some(value_expr_types),
            Ok(builder.build()),
        )))
    }

    /// Get the types of the elements of this union as they would appear in a value
    /// expression context. For a PEP 604 union, we return the actual types that were
    /// inferred when we encountered the union in a value expression context. For a
    /// legacy `typing.Union[…]` annotation, we turn the type-expression types into
    /// their corresponding value-expression types, i.e. we turn instances like `int`
    /// into class literals like `<class 'int'>`. This operation is potentially lossy.
    pub(crate) fn value_expression_types(
        self,
        db: &'db dyn Db,
    ) -> Result<impl Iterator<Item = Type<'db>> + 'db, InvalidTypeExpressionError<'db>> {
        let to_class_literal = |ty: Type<'db>| {
            ty.as_nominal_instance()
                .map(|instance| Type::ClassLiteral(instance.class(db).class_literal(db).0))
                .unwrap_or_else(Type::unknown)
        };

        if let Some(value_expr_types) = self._value_expr_types(db) {
            Ok(Either::Left(value_expr_types.iter().copied()))
        } else {
            match self.union_type(db).clone()? {
                Type::Union(union) => Ok(Either::Right(Either::Left(
                    union.elements(db).iter().copied().map(to_class_literal),
                ))),
                ty => Ok(Either::Right(Either::Right(std::iter::once(
                    to_class_literal(ty),
                )))),
            }
        }
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        let value_expr_types = self._value_expr_types(db).as_ref().map(|types| {
            types
                .iter()
                .map(|ty| ty.normalized_impl(db, visitor))
                .collect::<Box<_>>()
        });
        let union_type = self
            .union_type(db)
            .clone()
            .map(|ty| ty.normalized_impl(db, visitor));

        Self::new(db, value_expr_types, union_type)
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        // The `Divergent` elimination rules are different within union types.
        // See `UnionType::recursive_type_normalized_impl` for details.
        let value_expr_types = match self._value_expr_types(db).as_ref() {
            Some(types) if nested => Some(
                types
                    .iter()
                    .map(|ty| ty.recursive_type_normalized_impl(db, div, nested))
                    .collect::<Option<Box<_>>>()?,
            ),
            Some(types) => Some(
                types
                    .iter()
                    .map(|ty| {
                        ty.recursive_type_normalized_impl(db, div, nested)
                            .unwrap_or(div)
                    })
                    .collect::<Box<_>>(),
            ),
            None => None,
        };
        let union_type = match self.union_type(db).clone() {
            Ok(ty) if nested => Ok(ty.recursive_type_normalized_impl(db, div, nested)?),
            Ok(ty) => Ok(ty
                .recursive_type_normalized_impl(db, div, nested)
                .unwrap_or(div)),
            Err(err) => Err(err),
        };

        Some(Self::new(db, value_expr_types, union_type))
    }
}

/// A salsa-interned `Type`
///
/// # Ordering
/// Ordering is based on the context's salsa-assigned id and not on its values.
/// The id may change between runs, or when the context was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct InternedType<'db> {
    inner: Type<'db>,
}

impl get_size2::GetSize for InternedType<'_> {}

impl<'db> InternedType<'db> {
    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        InternedType::new(db, self.inner(db).normalized_impl(db, visitor))
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let inner = if nested {
            self.inner(db)
                .recursive_type_normalized_impl(db, div, nested)?
        } else {
            self.inner(db)
                .recursive_type_normalized_impl(db, div, nested)
                .unwrap_or(div)
        };
        Some(InternedType::new(db, inner))
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
                if let Some(definition) = bindings.callable_type().definition(db) {
                    if let Some(definition_range) = definition.focus_range(db) {
                        diag.annotate(
                            Annotation::secondary(definition_range.into())
                                .message("attribute defined here"),
                        );
                    }
                }
            }
            Self::Call(CallDunderError::PossiblyUnbound(bindings)) => {
                diag.info("`__await__` may be missing");
                if let Some(definition_spans) = bindings.callable_type().function_spans(db) {
                    diag.annotate(
                        Annotation::secondary(definition_spans.signature)
                            .message("method defined here"),
                    );
                }
            }
            Self::Call(CallDunderError::MethodNotAvailable) => {
                diag.info("`__await__` is missing");
                if let Some(type_definition) = context_expression_type.definition(db) {
                    if let Some(definition_range) = type_definition.focus_range(db) {
                        diag.annotate(
                            Annotation::secondary(definition_range.into())
                                .message("type defined here"),
                        );
                    }
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
                    format!("the method `{name}` may be missing")
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
                    format!("the methods `{name_a}` and `{name_b}` are possibly missing")
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

        let alt_enter = context_expression_type.try_call_dunder(
            db,
            alt_enter_method,
            CallArguments::none(),
            TypeContext::default(),
        );
        let alt_exit = context_expression_type.try_call_dunder(
            db,
            alt_exit_method,
            CallArguments::positional([Type::unknown(), Type::unknown(), Type::unknown()]),
            TypeContext::default(),
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
            } => dunder_error.return_type(db).and_then(|ty| {
                if mode.is_async() {
                    ty.try_await(db).ok()
                } else {
                    Some(ty)
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
                        TypeContext::default(),
                    ))
                    .and_then(|ty| ty.try_await(db).ok())
                } else {
                    return_type(dunder_iter_bindings.return_type(db).try_call_dunder(
                        db,
                        "__next__",
                        CallArguments::none(),
                        TypeContext::default(),
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
    /// because `__bool__` points to a type that has a possibly missing `__call__` method.
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
                    context.report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, context_expression_node)
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
                    context.report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, context_expression_node)
                {
                    builder.into_diagnostic(format_args!(
                        "Method `__init__` on type `{}` may be missing.",
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
                    context.report_lint(&POSSIBLY_MISSING_IMPLICIT_CALL, context_expression_node)
                {
                    builder.into_diagnostic(format_args!(
                        "Method `__new__` on type `{}` may be missing.",
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

/// A non-exhaustive enumeration of relations that can exist between types.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub(crate) enum TypeRelation<'db> {
    /// The "subtyping" relation.
    ///
    /// A [fully static] type `B` is a subtype of a fully static type `A` if and only if
    /// the set of possible runtime values represented by `B` is a subset of the set
    /// of possible runtime values represented by `A`.
    ///
    /// For a pair of types `C` and `D` that may or may not be fully static,
    /// `D` can be said to be a subtype of `C` if every possible fully static
    /// [materialization] of `D` is a subtype of every possible fully static
    /// materialization of `C`. Another way of saying this is that `D` will be a
    /// subtype of `C` if and only if the union of all possible sets of values
    /// represented by `D` (the "top materialization" of `D`) is a subtype of the
    /// intersection of all possible sets of values represented by `C` (the "bottom
    /// materialization" of `C`). More concisely: `D <: C` iff `Top[D] <: Bottom[C]`.
    ///
    /// For example, `list[Any]` can be said to be a subtype of `Sequence[object]`,
    /// because every possible fully static materialization of `list[Any]` (`list[int]`,
    /// `list[str]`, `list[bytes | bool]`, `list[SupportsIndex]`, etc.) would be
    /// considered a subtype of `Sequence[object]`.
    ///
    /// Note that this latter expansion of the subtyping relation to non-fully-static
    /// types is not described in the typing spec, but this expansion to gradual types is
    /// sound and consistent with the principles laid out in the spec. This definition
    /// does mean the subtyping relation is not reflexive for non-fully-static types
    /// (e.g. `Any` is not a subtype of `Any`).
    ///
    /// [fully static]: https://typing.python.org/en/latest/spec/glossary.html#term-fully-static-type
    /// [materialization]: https://typing.python.org/en/latest/spec/glossary.html#term-materialize
    Subtyping,

    /// The "assignability" relation.
    ///
    /// The assignability relation between two types `A` and `B` dictates whether a
    /// type checker should emit an error when a value of type `B` is assigned to a
    /// variable declared as having type `A`.
    ///
    /// For a pair of [fully static] types `A` and `B`, the assignability relation
    /// between `A` and `B` is the same as the subtyping relation.
    ///
    /// Between a pair of `C` and `D` where either `C` or `D` is not fully static, the
    /// assignability relation may be more permissive than the subtyping relation. `D`
    /// can be said to be assignable to `C` if *some* possible fully static [materialization]
    /// of `D` is a subtype of *some* possible fully static materialization of `C`.
    /// Another way of saying this is that `D` will be assignable to `C` if and only if the
    /// intersection of all possible sets of values represented by `D` (the "bottom
    /// materialization" of `D`) is a subtype of the union of all possible sets of values
    /// represented by `C` (the "top materialization" of `C`).
    /// More concisely: `D <: C` iff `Bottom[D] <: Top[C]`.
    ///
    /// For example, `Any` is not a subtype of `int`, because there are possible
    /// materializations of `Any` (e.g., `str`) that are not subtypes of `int`.
    /// `Any` is *assignable* to `int`, however, as there are *some* possible materializations
    /// of `Any` (such as `int` itself!) that *are* subtypes of `int`. `Any` cannot even
    /// be considered a subtype of itself, as two separate uses of `Any` in the same scope
    /// might materialize to different types between which there would exist no subtyping
    /// relation; nor is `Any` a subtype of `int | Any`, for the same reason. Nonetheless,
    /// `Any` is assignable to both `Any` and `int | Any`.
    ///
    /// While `Any` can materialize to anything, the presence of `Any` in a type does not
    /// necessarily make it assignable to everything. For example, `list[Any]` is not
    /// assignable to `int`, because there are no possible fully static types we could
    /// substitute for `Any` in this type that would make it a subtype of `int`. For the
    /// same reason, a union such as `str | Any` is not assignable to `int`.
    ///
    /// [fully static]: https://typing.python.org/en/latest/spec/glossary.html#term-fully-static-type
    /// [materialization]: https://typing.python.org/en/latest/spec/glossary.html#term-materialize
    Assignability,

    /// The "redundancy" relation.
    ///
    /// The redundancy relation dictates whether the union `A | B` can be safely simplified
    /// to the type `A` without downstream consequences on ty's inference of types elsewhere.
    ///
    /// For a pair of [fully static] types `A` and `B`, the redundancy relation between `A`
    /// and `B` is the same as the subtyping relation.
    ///
    /// Between a pair of `C` and `D` where either `C` or `D` is not fully static, the
    /// redundancy relation sits in between the subtyping relation and the assignability relation.
    /// `D` can be said to be redundant in a union with `C` if the top materialization of the type
    /// `C | D` is equivalent to the top materialization of `C`, *and* the bottom materialization
    /// of `C | D` is equivalent to the bottom materialization of `C`.
    /// More concisely: `D <: C` iff `Top[C | D] == Top[C]` AND `Bottom[C | D] == Bottom[C]`.
    ///
    /// Practically speaking, in most respects the redundancy relation is the same as the subtyping
    /// relation. It is redundant to add `bool` to a union that includes `int`, because `bool` is a
    /// subtype of `int`, so inference of attribute access or binary expressions on the union
    /// `int | bool` would always produce a type that represents the same set of possible sets of
    /// runtime values as if ty had inferred the attribute access or binary expression on `int`
    /// alone.
    ///
    /// Where the redundancy relation differs from the subtyping relation is that there are a
    /// number of simplifications that can be made when simplifying unions that are not
    /// strictly permitted by the subtyping relation. For example, it is safe to avoid adding
    /// `Any` to a union that already includes `Any`, because `Any` already represents an
    /// unknown set of possible sets of runtime values that can materialize to any type in a
    /// gradual, permissive way. Inferring attribute access or binary expressions over
    /// `Any | Any` could never conceivably yield a type that represents a different set of
    /// possible sets of runtime values to inferring the same expression over `Any` alone;
    /// although `Any` is not a subtype of `Any`, top materialization of both `Any` and
    /// `Any | Any` is `object`, and the bottom materialization of both types is `Never`.
    ///
    /// The same principle also applies to intersections that include `Any` being added to
    /// unions that include `Any`: for any type `A`, although naively distributing
    /// type-inference operations over `(Any & A) | Any` could produce types that have
    /// different displays to `Any`, `(Any & A) | Any` nonetheless has the same top
    /// materialization as `Any` and the same bottom materialization as `Any`, and thus it is
    /// redundant to add `Any & A` to a union that already includes `Any`.
    ///
    /// Union simplification cannot use the assignability relation, meanwhile, as it is
    /// trivial to produce examples of cases where adding a type `B` to a union that includes
    /// `A` would impact downstream type inference, even where `B` is assignable to `A`. For
    /// example, `int` is assignable to `Any`, but attribute access over the union `int | Any`
    /// will yield very different results to attribute access over `Any` alone. The top
    /// materialization of `Any` and `int | Any` may be the same type (`object`), but the
    /// two differ in their bottom materializations (`Never` and `int`, respectively).
    ///
    /// Despite the above principles, there is one exceptional type that should never be union-simplified: the `Divergent` type.
    /// This is a kind of dynamic type, but it acts as a marker to track recursive type structures.
    /// If this type is accidentally eliminated by simplification, the fixed-point iteration will not converge.
    ///
    /// [fully static]: https://typing.python.org/en/latest/spec/glossary.html#term-fully-static-type
    /// [materializations]: https://typing.python.org/en/latest/spec/glossary.html#term-materialize
    Redundancy,

    /// The "constraint implication" relationship, aka "implies subtype of".
    ///
    /// This relationship tests whether one type is a [subtype][Self::Subtyping] of another,
    /// assuming that the constraints in a particular constraint set hold.
    ///
    /// For concrete types (types that do not contain typevars), this relationship is the same as
    /// [subtyping][Self::Subtyping]. (Constraint sets place restrictions on typevars, so if you
    /// are not comparing typevars, the constraint set can have no effect on whether subtyping
    /// holds.)
    ///
    /// If you're comparing a typevar, we have to consider what restrictions the constraint set
    /// places on that typevar to determine if subtyping holds. For instance, if you want to check
    /// whether `T ≤ int`, then the answer will depend on what constraint set you are considering:
    ///
    /// ```text
    /// implies_subtype_of(T ≤ bool, T, int) ⇒ true
    /// implies_subtype_of(T ≤ int, T, int)  ⇒ true
    /// implies_subtype_of(T ≤ str, T, int)  ⇒ false
    /// ```
    ///
    /// In the first two cases, the constraint set ensures that `T` will always specialize to a
    /// type that is a subtype of `int`. In the final case, the constraint set requires `T` to
    /// specialize to a subtype of `str`, and there is no such type that is also a subtype of
    /// `int`.
    ///
    /// There are two constraint sets that deserve special consideration.
    ///
    /// - The "always true" constraint set does not place any restrictions on any typevar. In this
    ///   case, `implies_subtype_of` will return the same result as `when_subtype_of`, even if
    ///   you're comparing against a typevar.
    ///
    /// - The "always false" constraint set represents an impossible situation. In this case, every
    ///   subtype check will be vacuously true, even if you're comparing two concrete types that
    ///   are not actually subtypes of each other. (That is, `implies_subtype_of(false, int, str)`
    ///   will return true!)
    SubtypingAssuming(ConstraintSet<'db>),
}

impl TypeRelation<'_> {
    pub(crate) const fn is_assignability(self) -> bool {
        matches!(self, TypeRelation::Assignability)
    }

    pub(crate) const fn is_subtyping(self) -> bool {
        matches!(self, TypeRelation::Subtyping)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, get_size2::GetSize)]
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

fn into_callable_type_cycle_initial<'db>(
    db: &'db dyn Db,
    _id: salsa::Id,
    _self: BoundMethodType<'db>,
) -> CallableType<'db> {
    CallableType::bottom(db)
}

#[salsa::tracked]
impl<'db> BoundMethodType<'db> {
    /// Returns the type that replaces any `typing.Self` annotations in the bound method signature.
    /// This is normally the bound-instance type (the type of `self` or `cls`), but if the bound method is
    /// a `@classmethod`, then it should be an instance of that bound-instance type.
    pub(crate) fn typing_self_type(self, db: &'db dyn Db) -> Type<'db> {
        let mut self_instance = self.self_instance(db);
        if self.function(db).is_classmethod(db) {
            self_instance = self_instance.to_instance(db).unwrap_or_else(Type::unknown);
        }
        self_instance
    }

    pub(crate) fn map_self_type(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(Type<'db>) -> Type<'db>,
    ) -> Self {
        Self::new(db, self.function(db), f(self.self_instance(db)))
    }

    #[salsa::tracked(cycle_initial=into_callable_type_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn into_callable_type(self, db: &'db dyn Db) -> CallableType<'db> {
        let function = self.function(db);
        let self_instance = self.typing_self_type(db);

        CallableType::new(
            db,
            CallableSignature::from_overloads(
                function
                    .signature(db)
                    .overloads
                    .iter()
                    .map(|signature| signature.bind_self(db, Some(self_instance))),
            ),
            CallableTypeKind::FunctionLike,
        )
    }

    fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self::new(
            db,
            self.function(db).normalized_impl(db, visitor),
            self.self_instance(db).normalized_impl(db, visitor),
        )
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self::new(
            db,
            self.function(db)
                .recursive_type_normalized_impl(db, div, nested)?,
            self.self_instance(db)
                .recursive_type_normalized_impl(db, div, true)?,
        ))
    }

    fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        // A bound method is a typically a subtype of itself. However, we must explicitly verify
        // the subtyping of the underlying function signatures (since they might be specialized
        // differently), and of the bound self parameter (taking care that parameters, including a
        // bound self parameter, are contravariant.)
        self.function(db)
            .has_relation_to_impl(
                db,
                other.function(db),
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            )
            .and(db, || {
                other.self_instance(db).has_relation_to_impl(
                    db,
                    self.self_instance(db),
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            })
    }

    fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        self.function(db)
            .is_equivalent_to_impl(db, other.function(db), inferable, visitor)
            .and(db, || {
                other.self_instance(db).is_equivalent_to_impl(
                    db,
                    self.self_instance(db),
                    inferable,
                    visitor,
                )
            })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, get_size2::GetSize)]
pub enum CallableTypeKind {
    /// Represents regular callable objects.
    Regular,

    /// Represents function-like objects, like the synthesized methods of dataclasses or
    /// `NamedTuples`. These callables act like real functions when accessed as attributes on
    /// instances, i.e. they bind `self`.
    FunctionLike,

    /// Represents the value bound to a `typing.ParamSpec` type variable.
    ParamSpecValue,
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

    kind: CallableTypeKind,
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

impl<'db> Type<'db> {
    /// Create a callable type with a single non-overloaded signature.
    pub(crate) fn single_callable(db: &'db dyn Db, signature: Signature<'db>) -> Type<'db> {
        Type::Callable(CallableType::single(db, signature))
    }

    /// Create a non-overloaded, function-like callable type with a single signature.
    ///
    /// A function-like callable will bind `self` when accessed as an attribute on an instance.
    pub(crate) fn function_like_callable(db: &'db dyn Db, signature: Signature<'db>) -> Type<'db> {
        Type::Callable(CallableType::function_like(db, signature))
    }

    /// Create a non-overloaded callable type which represents the value bound to a `ParamSpec`
    /// type variable.
    pub(crate) fn paramspec_value_callable(
        db: &'db dyn Db,
        parameters: Parameters<'db>,
    ) -> Type<'db> {
        Type::Callable(CallableType::paramspec_value(db, parameters))
    }
}

impl<'db> CallableType<'db> {
    pub(crate) fn single(db: &'db dyn Db, signature: Signature<'db>) -> CallableType<'db> {
        CallableType::new(
            db,
            CallableSignature::single(signature),
            CallableTypeKind::Regular,
        )
    }

    pub(crate) fn function_like(db: &'db dyn Db, signature: Signature<'db>) -> CallableType<'db> {
        CallableType::new(
            db,
            CallableSignature::single(signature),
            CallableTypeKind::FunctionLike,
        )
    }

    pub(crate) fn paramspec_value(
        db: &'db dyn Db,
        parameters: Parameters<'db>,
    ) -> CallableType<'db> {
        CallableType::new(
            db,
            CallableSignature::single(Signature::new(parameters, None)),
            CallableTypeKind::ParamSpecValue,
        )
    }

    /// Create a callable type which accepts any parameters and returns an `Unknown` type.
    pub(crate) fn unknown(db: &'db dyn Db) -> CallableType<'db> {
        Self::single(db, Signature::unknown())
    }

    pub(crate) fn is_function_like(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), CallableTypeKind::FunctionLike)
    }

    pub(crate) fn bind_self(
        self,
        db: &'db dyn Db,
        self_type: Option<Type<'db>>,
    ) -> CallableType<'db> {
        CallableType::new(
            db,
            self.signatures(db).bind_self(db, self_type),
            self.kind(db),
        )
    }

    pub(crate) fn apply_self(self, db: &'db dyn Db, self_type: Type<'db>) -> CallableType<'db> {
        CallableType::new(
            db,
            self.signatures(db).apply_self(db, self_type),
            self.kind(db),
        )
    }

    /// Create a callable type which represents a fully-static "bottom" callable.
    ///
    /// Specifically, this represents a callable type with a single signature:
    /// `(*args: object, **kwargs: object) -> Never`.
    pub(crate) fn bottom(db: &'db dyn Db) -> CallableType<'db> {
        Self::new(db, CallableSignature::bottom(), CallableTypeKind::Regular)
    }

    /// Return a "normalized" version of this `Callable` type.
    ///
    /// See [`Type::normalized`] for more details.
    fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        CallableType::new(
            db,
            self.signatures(db).normalized_impl(db, visitor),
            self.kind(db),
        )
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(CallableType::new(
            db,
            self.signatures(db)
                .recursive_type_normalized_impl(db, div, nested)?,
            self.kind(db),
        ))
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        CallableType::new(
            db,
            self.signatures(db)
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            self.kind(db),
        )
    }

    fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        self.signatures(db)
            .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
    }

    /// Check whether this callable type has the given relation to another callable type.
    ///
    /// See [`Type::is_subtype_of`] and [`Type::is_assignable_to`] for more details.
    fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if other.is_function_like(db) && !self.is_function_like(db) {
            return ConstraintSet::from(false);
        }
        self.signatures(db).has_relation_to_impl(
            db,
            other.signatures(db),
            inferable,
            relation,
            relation_visitor,
            disjointness_visitor,
        )
    }

    /// Check whether this callable type is equivalent to another callable type.
    ///
    /// See [`Type::is_equivalent_to`] for more details.
    fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if self == other {
            return ConstraintSet::from(true);
        }

        ConstraintSet::from(self.is_function_like(db) == other.is_function_like(db)).and(db, || {
            self.signatures(db)
                .is_equivalent_to_impl(db, other.signatures(db), inferable, visitor)
        })
    }
}

/// Converting a type "into a callable" can possibly return a _union_ of callables. Eventually,
/// when coercing that result to a single type, you'll get a `UnionType`. But this lets you handle
/// that result as a list of `CallableType`s before merging them into a `UnionType` should that be
/// helpful.
///
/// Note that this type is guaranteed to contain at least one callable. If you need to support "no
/// callables" as a possibility, use `Option<CallableTypes>`.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) struct CallableTypes<'db>(SmallVec<[CallableType<'db>; 1]>);

impl<'db> CallableTypes<'db> {
    pub(crate) fn one(callable: CallableType<'db>) -> Self {
        CallableTypes(smallvec![callable])
    }

    pub(crate) fn from_elements(callables: impl IntoIterator<Item = CallableType<'db>>) -> Self {
        let callables: SmallVec<_> = callables.into_iter().collect();
        assert!(!callables.is_empty(), "CallableTypes should not be empty");
        CallableTypes(callables)
    }

    pub(crate) fn exactly_one(self) -> Option<CallableType<'db>> {
        match self.0.as_slice() {
            [single] => Some(*single),
            _ => None,
        }
    }

    fn into_inner(self) -> SmallVec<[CallableType<'db>; 1]> {
        self.0
    }

    pub(crate) fn into_type(self, db: &'db dyn Db) -> Type<'db> {
        match self.0.as_slice() {
            [] => unreachable!("CallableTypes should not be empty"),
            [single] => Type::Callable(*single),
            slice => UnionType::from_elements(db, slice.iter().copied().map(Type::Callable)),
        }
    }

    pub(crate) fn map(self, mut f: impl FnMut(CallableType<'db>) -> CallableType<'db>) -> Self {
        Self::from_elements(self.0.iter().map(|element| f(*element)))
    }

    pub(crate) fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        other: CallableType<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        self.0.iter().when_all(db, |element| {
            element.has_relation_to_impl(
                db,
                other,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            )
        })
    }
}

/// Represents a specific instance of a bound method type for a builtin class.
///
/// Unlike bound methods of user-defined classes, these are not generally instances
/// of `types.BoundMethodType` at runtime.
#[derive(
    Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, salsa::Update, get_size2::GetSize,
)]
pub enum KnownBoundMethodType<'db> {
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

    // ConstraintSet methods
    ConstraintSetRange,
    ConstraintSetAlways,
    ConstraintSetNever,
    ConstraintSetImpliesSubtypeOf(TrackedConstraintSet<'db>),
    ConstraintSetSatisfies(TrackedConstraintSet<'db>),
    ConstraintSetSatisfiedByAllTypeVars(TrackedConstraintSet<'db>),

    // GenericContext methods
    GenericContextSpecializeConstrained(GenericContext<'db>),
}

pub(super) fn walk_method_wrapper_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    method_wrapper: KnownBoundMethodType<'db>,
    visitor: &V,
) {
    match method_wrapper {
        KnownBoundMethodType::FunctionTypeDunderGet(function) => {
            visitor.visit_function_type(db, function);
        }
        KnownBoundMethodType::FunctionTypeDunderCall(function) => {
            visitor.visit_function_type(db, function);
        }
        KnownBoundMethodType::PropertyDunderGet(property) => {
            visitor.visit_property_instance_type(db, property);
        }
        KnownBoundMethodType::PropertyDunderSet(property) => {
            visitor.visit_property_instance_type(db, property);
        }
        KnownBoundMethodType::StrStartswith(string_literal) => {
            visitor.visit_type(db, Type::StringLiteral(string_literal));
        }
        KnownBoundMethodType::ConstraintSetRange
        | KnownBoundMethodType::ConstraintSetAlways
        | KnownBoundMethodType::ConstraintSetNever
        | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
        | KnownBoundMethodType::ConstraintSetSatisfies(_)
        | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
        | KnownBoundMethodType::GenericContextSpecializeConstrained(_) => {}
    }
}

impl<'db> KnownBoundMethodType<'db> {
    fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        match (self, other) {
            (
                KnownBoundMethodType::FunctionTypeDunderGet(self_function),
                KnownBoundMethodType::FunctionTypeDunderGet(other_function),
            ) => self_function.has_relation_to_impl(
                db,
                other_function,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            (
                KnownBoundMethodType::FunctionTypeDunderCall(self_function),
                KnownBoundMethodType::FunctionTypeDunderCall(other_function),
            ) => self_function.has_relation_to_impl(
                db,
                other_function,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            ),

            (
                KnownBoundMethodType::PropertyDunderGet(self_property),
                KnownBoundMethodType::PropertyDunderGet(other_property),
            )
            | (
                KnownBoundMethodType::PropertyDunderSet(self_property),
                KnownBoundMethodType::PropertyDunderSet(other_property),
            ) => self_property.when_equivalent_to(db, other_property, inferable),

            (KnownBoundMethodType::StrStartswith(_), KnownBoundMethodType::StrStartswith(_)) => {
                ConstraintSet::from(self == other)
            }

            (
                KnownBoundMethodType::ConstraintSetRange,
                KnownBoundMethodType::ConstraintSetRange,
            )
            | (
                KnownBoundMethodType::ConstraintSetAlways,
                KnownBoundMethodType::ConstraintSetAlways,
            )
            | (
                KnownBoundMethodType::ConstraintSetNever,
                KnownBoundMethodType::ConstraintSetNever,
            )
            | (
                KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_),
                KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_),
            )
            | (
                KnownBoundMethodType::ConstraintSetSatisfies(_),
                KnownBoundMethodType::ConstraintSetSatisfies(_),
            )
            | (
                KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_),
                KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_),
            )
            | (
                KnownBoundMethodType::GenericContextSpecializeConstrained(_),
                KnownBoundMethodType::GenericContextSpecializeConstrained(_),
            ) => ConstraintSet::from(true),

            (
                KnownBoundMethodType::FunctionTypeDunderGet(_)
                | KnownBoundMethodType::FunctionTypeDunderCall(_)
                | KnownBoundMethodType::PropertyDunderGet(_)
                | KnownBoundMethodType::PropertyDunderSet(_)
                | KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
                | KnownBoundMethodType::GenericContextSpecializeConstrained(_),
                KnownBoundMethodType::FunctionTypeDunderGet(_)
                | KnownBoundMethodType::FunctionTypeDunderCall(_)
                | KnownBoundMethodType::PropertyDunderGet(_)
                | KnownBoundMethodType::PropertyDunderSet(_)
                | KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
                | KnownBoundMethodType::GenericContextSpecializeConstrained(_),
            ) => ConstraintSet::from(false),
        }
    }

    fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        match (self, other) {
            (
                KnownBoundMethodType::FunctionTypeDunderGet(self_function),
                KnownBoundMethodType::FunctionTypeDunderGet(other_function),
            ) => self_function.is_equivalent_to_impl(db, other_function, inferable, visitor),

            (
                KnownBoundMethodType::FunctionTypeDunderCall(self_function),
                KnownBoundMethodType::FunctionTypeDunderCall(other_function),
            ) => self_function.is_equivalent_to_impl(db, other_function, inferable, visitor),

            (
                KnownBoundMethodType::PropertyDunderGet(self_property),
                KnownBoundMethodType::PropertyDunderGet(other_property),
            )
            | (
                KnownBoundMethodType::PropertyDunderSet(self_property),
                KnownBoundMethodType::PropertyDunderSet(other_property),
            ) => self_property.is_equivalent_to_impl(db, other_property, inferable, visitor),

            (KnownBoundMethodType::StrStartswith(_), KnownBoundMethodType::StrStartswith(_)) => {
                ConstraintSet::from(self == other)
            }

            (
                KnownBoundMethodType::ConstraintSetRange,
                KnownBoundMethodType::ConstraintSetRange,
            )
            | (
                KnownBoundMethodType::ConstraintSetAlways,
                KnownBoundMethodType::ConstraintSetAlways,
            )
            | (
                KnownBoundMethodType::ConstraintSetNever,
                KnownBoundMethodType::ConstraintSetNever,
            ) => ConstraintSet::from(true),

            (
                KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(left_constraints),
                KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(right_constraints),
            )
            | (
                KnownBoundMethodType::ConstraintSetSatisfies(left_constraints),
                KnownBoundMethodType::ConstraintSetSatisfies(right_constraints),
            )
            | (
                KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(left_constraints),
                KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(right_constraints),
            ) => left_constraints
                .constraints(db)
                .iff(db, right_constraints.constraints(db)),

            (
                KnownBoundMethodType::GenericContextSpecializeConstrained(left_generic_context),
                KnownBoundMethodType::GenericContextSpecializeConstrained(right_generic_context),
            ) => ConstraintSet::from(left_generic_context == right_generic_context),

            (
                KnownBoundMethodType::FunctionTypeDunderGet(_)
                | KnownBoundMethodType::FunctionTypeDunderCall(_)
                | KnownBoundMethodType::PropertyDunderGet(_)
                | KnownBoundMethodType::PropertyDunderSet(_)
                | KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
                | KnownBoundMethodType::GenericContextSpecializeConstrained(_),
                KnownBoundMethodType::FunctionTypeDunderGet(_)
                | KnownBoundMethodType::FunctionTypeDunderCall(_)
                | KnownBoundMethodType::PropertyDunderGet(_)
                | KnownBoundMethodType::PropertyDunderSet(_)
                | KnownBoundMethodType::StrStartswith(_)
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
                | KnownBoundMethodType::GenericContextSpecializeConstrained(_),
            ) => ConstraintSet::from(false),
        }
    }

    fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        match self {
            KnownBoundMethodType::FunctionTypeDunderGet(function) => {
                KnownBoundMethodType::FunctionTypeDunderGet(function.normalized_impl(db, visitor))
            }
            KnownBoundMethodType::FunctionTypeDunderCall(function) => {
                KnownBoundMethodType::FunctionTypeDunderCall(function.normalized_impl(db, visitor))
            }
            KnownBoundMethodType::PropertyDunderGet(property) => {
                KnownBoundMethodType::PropertyDunderGet(property.normalized_impl(db, visitor))
            }
            KnownBoundMethodType::PropertyDunderSet(property) => {
                KnownBoundMethodType::PropertyDunderSet(property.normalized_impl(db, visitor))
            }
            KnownBoundMethodType::StrStartswith(_)
            | KnownBoundMethodType::ConstraintSetRange
            | KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever
            | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
            | KnownBoundMethodType::ConstraintSetSatisfies(_)
            | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
            | KnownBoundMethodType::GenericContextSpecializeConstrained(_) => self,
        }
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            KnownBoundMethodType::FunctionTypeDunderGet(function) => {
                Some(KnownBoundMethodType::FunctionTypeDunderGet(
                    function.recursive_type_normalized_impl(db, div, nested)?,
                ))
            }
            KnownBoundMethodType::FunctionTypeDunderCall(function) => {
                Some(KnownBoundMethodType::FunctionTypeDunderCall(
                    function.recursive_type_normalized_impl(db, div, nested)?,
                ))
            }
            KnownBoundMethodType::PropertyDunderGet(property) => {
                Some(KnownBoundMethodType::PropertyDunderGet(
                    property.recursive_type_normalized_impl(db, div, nested)?,
                ))
            }
            KnownBoundMethodType::PropertyDunderSet(property) => {
                Some(KnownBoundMethodType::PropertyDunderSet(
                    property.recursive_type_normalized_impl(db, div, nested)?,
                ))
            }
            KnownBoundMethodType::StrStartswith(_)
            | KnownBoundMethodType::ConstraintSetRange
            | KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever
            | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
            | KnownBoundMethodType::ConstraintSetSatisfies(_)
            | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
            | KnownBoundMethodType::GenericContextSpecializeConstrained(_) => Some(self),
        }
    }

    /// Return the [`KnownClass`] that inhabitants of this type are instances of at runtime
    fn class(self) -> KnownClass {
        match self {
            KnownBoundMethodType::FunctionTypeDunderGet(_)
            | KnownBoundMethodType::FunctionTypeDunderCall(_)
            | KnownBoundMethodType::PropertyDunderGet(_)
            | KnownBoundMethodType::PropertyDunderSet(_) => KnownClass::MethodWrapperType,
            KnownBoundMethodType::StrStartswith(_) => KnownClass::BuiltinFunctionType,
            KnownBoundMethodType::ConstraintSetRange
            | KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever
            | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
            | KnownBoundMethodType::ConstraintSetSatisfies(_)
            | KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)
            | KnownBoundMethodType::GenericContextSpecializeConstrained(_) => {
                KnownClass::ConstraintSet
            }
        }
    }

    /// Return the signatures of this bound method type.
    ///
    /// If the bound method type is overloaded, it may have multiple signatures.
    fn signatures(self, db: &'db dyn Db) -> impl Iterator<Item = Signature<'db>> {
        match self {
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
            //
            // TODO: Consider merging these synthesized signatures with the ones in
            // [`WrapperDescriptorKind::signatures`], since this one is just that signature
            // with the `self` parameters removed.
            KnownBoundMethodType::FunctionTypeDunderGet(_)
            | KnownBoundMethodType::PropertyDunderGet(_) => Either::Left(Either::Left(
                [
                    Signature::new(
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_only(Some(Name::new_static("instance")))
                                    .with_annotated_type(Type::none(db)),
                                Parameter::positional_only(Some(Name::new_static("owner")))
                                    .with_annotated_type(KnownClass::Type.to_instance(db)),
                            ],
                        ),
                        None,
                    ),
                    Signature::new(
                        Parameters::new(
                            db,
                            [
                                Parameter::positional_only(Some(Name::new_static("instance")))
                                    .with_annotated_type(Type::object()),
                                Parameter::positional_only(Some(Name::new_static("owner")))
                                    .with_annotated_type(UnionType::from_elements(
                                        db,
                                        [KnownClass::Type.to_instance(db), Type::none(db)],
                                    ))
                                    .with_default_type(Type::none(db)),
                            ],
                        ),
                        None,
                    ),
                ]
                .into_iter(),
            )),
            KnownBoundMethodType::FunctionTypeDunderCall(function) => Either::Left(Either::Right(
                function.signature(db).overloads.iter().cloned(),
            )),
            KnownBoundMethodType::PropertyDunderSet(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(Type::object()),
                            Parameter::positional_only(Some(Name::new_static("value")))
                                .with_annotated_type(Type::object()),
                        ],
                    ),
                    None,
                )))
            }
            KnownBoundMethodType::StrStartswith(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("prefix")))
                                .with_annotated_type(UnionType::from_elements(
                                    db,
                                    [
                                        KnownClass::Str.to_instance(db),
                                        Type::homogeneous_tuple(
                                            db,
                                            KnownClass::Str.to_instance(db),
                                        ),
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
                        ],
                    ),
                    Some(KnownClass::Bool.to_instance(db)),
                )))
            }

            KnownBoundMethodType::ConstraintSetRange => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("lower_bound")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                            Parameter::positional_only(Some(Name::new_static("typevar")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                            Parameter::positional_only(Some(Name::new_static("upper_bound")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                        ],
                    ),
                    Some(KnownClass::ConstraintSet.to_instance(db)),
                )))
            }

            KnownBoundMethodType::ConstraintSetAlways
            | KnownBoundMethodType::ConstraintSetNever => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::empty(),
                    Some(KnownClass::ConstraintSet.to_instance(db)),
                )))
            }

            KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("ty")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                            Parameter::positional_only(Some(Name::new_static("of")))
                                .type_form()
                                .with_annotated_type(Type::any()),
                        ],
                    ),
                    Some(KnownClass::ConstraintSet.to_instance(db)),
                )))
            }

            KnownBoundMethodType::ConstraintSetSatisfies(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::positional_only(Some(Name::new_static("other")))
                            .with_annotated_type(KnownClass::ConstraintSet.to_instance(db))],
                    ),
                    Some(KnownClass::ConstraintSet.to_instance(db)),
                )))
            }

            KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [Parameter::keyword_only(Name::new_static("inferable"))
                            .type_form()
                            .with_annotated_type(UnionType::from_elements(
                                db,
                                [Type::homogeneous_tuple(db, Type::any()), Type::none(db)],
                            ))
                            .with_default_type(Type::none(db))],
                    ),
                    Some(KnownClass::Bool.to_instance(db)),
                )))
            }

            KnownBoundMethodType::GenericContextSpecializeConstrained(_) => {
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("constraints")))
                                .with_annotated_type(KnownClass::ConstraintSet.to_instance(db)),
                        ],
                    ),
                    Some(UnionType::from_elements(
                        db,
                        [KnownClass::Specialization.to_instance(db), Type::none(db)],
                    )),
                )))
            }
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

impl WrapperDescriptorKind {
    fn signatures(self, db: &dyn Db) -> impl Iterator<Item = Signature<'_>> {
        /// Similar to what we do in [`KnownBoundMethod::signatures`],
        /// here we also model `types.FunctionType.__get__` (or builtins.property.__get__),
        /// but now we consider a call to this as a function, i.e. we also expect the `self`
        /// argument to be passed in.
        ///
        /// TODO: Consider merging these synthesized signatures with the ones in
        /// [`KnownBoundMethod::signatures`], since that one is just this signature
        /// with the `self` parameters removed.
        fn dunder_get_signatures(db: &dyn Db, class: KnownClass) -> [Signature<'_>; 2] {
            let type_instance = KnownClass::Type.to_instance(db);
            let none = Type::none(db);
            let descriptor = class.to_instance(db);
            [
                Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("self")))
                                .with_annotated_type(descriptor),
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(none),
                            Parameter::positional_only(Some(Name::new_static("owner")))
                                .with_annotated_type(type_instance),
                        ],
                    ),
                    None,
                ),
                Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("self")))
                                .with_annotated_type(descriptor),
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(Type::object()),
                            Parameter::positional_only(Some(Name::new_static("owner")))
                                .with_annotated_type(UnionType::from_elements(
                                    db,
                                    [type_instance, none],
                                ))
                                .with_default_type(none),
                        ],
                    ),
                    None,
                ),
            ]
        }

        match self {
            WrapperDescriptorKind::FunctionTypeDunderGet => {
                Either::Left(dunder_get_signatures(db, KnownClass::FunctionType).into_iter())
            }
            WrapperDescriptorKind::PropertyDunderGet => {
                Either::Left(dunder_get_signatures(db, KnownClass::Property).into_iter())
            }
            WrapperDescriptorKind::PropertyDunderSet => {
                let object = Type::object();
                Either::Right(std::iter::once(Signature::new(
                    Parameters::new(
                        db,
                        [
                            Parameter::positional_only(Some(Name::new_static("self")))
                                .with_annotated_type(KnownClass::Property.to_instance(db)),
                            Parameter::positional_only(Some(Name::new_static("instance")))
                                .with_annotated_type(object),
                            Parameter::positional_only(Some(Name::new_static("value")))
                                .with_annotated_type(object),
                        ],
                    ),
                    None,
                )))
            }
        }
    }
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
    /// the current file (i.e. [`crate::semantic_index::definition::ImportFromDefinitionNodeRef`]).
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
        let submodule = resolve_module(db, importing_file, &absolute_submodule_name)?;
        Some(Type::module_literal(db, importing_file, submodule))
    }

    fn try_module_getattr(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        // For module literals, we want to try calling the module's own `__getattr__` function
        // if it exists. First, we need to look up the `__getattr__` function in the module's scope.
        if let Some(file) = self.module(db).file(db) {
            let getattr_symbol = imported_symbol(db, file, "__getattr__", None);
            if let Place::Defined(getattr_type, origin, boundness) = getattr_symbol.place {
                // If we found a __getattr__ function, try to call it with the name argument
                if let Ok(outcome) = getattr_type.try_call(
                    db,
                    &CallArguments::positional([Type::string_literal(db, name)]),
                ) {
                    return PlaceAndQualifiers {
                        place: Place::Defined(outcome.return_type(db), origin, boundness),
                        qualifiers: TypeQualifiers::FROM_MODULE_GETATTR,
                    };
                }
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
        if place_and_qualifiers.place.is_undefined() {
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

    specialization: Option<Specialization<'db>>,
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
        let type_alias_stmt_node = scope.node(db).expect_type_alias();
        semantic_index(db, scope.file(db)).expect_single_definition(type_alias_stmt_node)
    }

    /// The RHS type of a PEP-695 style type alias with specialization applied.
    pub(crate) fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        self.apply_function_specialization(db, self.raw_value_type(db))
    }

    /// The RHS type of a PEP-695 style type alias with *no* specialization applied.
    /// Returns `Divergent` if the type alias is defined cyclically.
    #[salsa::tracked(cycle_fn=value_type_cycle_recover, cycle_initial=value_type_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    fn raw_value_type(self, db: &'db dyn Db) -> Type<'db> {
        let scope = self.rhs_scope(db);
        let module = parsed_module(db, scope.file(db)).load(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();
        let definition = self.definition(db);

        definition_expression_type(db, definition, &type_alias_stmt_node.node(&module).value)
    }

    fn apply_function_specialization(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        if let Some(generic_context) = self.generic_context(db) {
            let specialization = self
                .specialization(db)
                .unwrap_or_else(|| generic_context.default_specialization(db, None));
            ty.apply_specialization(db, specialization)
        } else {
            ty
        }
    }

    pub(crate) fn apply_specialization(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> PEP695TypeAliasType<'db> {
        match self.generic_context(db) {
            None => self,

            Some(generic_context) => {
                // Note that at runtime, a specialized type alias is an instance of `typing.GenericAlias`.
                // However, the `GenericAlias` type in ty is heavily special cased to refer to specialized
                // class literals, so we instead represent specialized type aliases as instances of
                // `typing.TypeAliasType` internally, and pass the specialization through to the value type,
                // except when resolving to an instance of the type alias, or its display representation.
                let specialization = f(generic_context);
                PEP695TypeAliasType::new(
                    db,
                    self.name(db),
                    self.rhs_scope(db),
                    Some(specialization),
                )
            }
        }
    }

    pub(crate) fn is_specialized(self, db: &'db dyn Db) -> bool {
        self.specialization(db).is_some()
    }

    #[salsa::tracked(cycle_initial=generic_context_cycle_initial, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        let scope = self.rhs_scope(db);
        let file = scope.file(db);
        let parsed = parsed_module(db, file).load(db);
        let type_alias_stmt_node = scope.node(db).expect_type_alias();

        type_alias_stmt_node
            .node(&parsed)
            .type_params
            .as_ref()
            .map(|type_params| {
                let index = semantic_index(db, scope.file(db));
                let definition = index.expect_single_definition(type_alias_stmt_node);
                GenericContext::from_type_params(db, index, definition, type_params)
            })
    }

    fn normalized_impl(self, _db: &'db dyn Db, _visitor: &NormalizedVisitor<'db>) -> Self {
        self
    }
}

fn generic_context_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
    _self: PEP695TypeAliasType<'db>,
) -> Option<GenericContext<'db>> {
    None
}

fn value_type_cycle_initial<'db>(
    _db: &'db dyn Db,
    id: salsa::Id,
    _self: PEP695TypeAliasType<'db>,
) -> Type<'db> {
    Type::divergent(id)
}

fn value_type_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_value: &Type<'db>,
    value: Type<'db>,
    _self: PEP695TypeAliasType<'db>,
) -> Type<'db> {
    value.cycle_normalized(db, *previous_value, cycle)
}

/// A PEP 695 `types.TypeAliasType` created by manually calling the constructor.
///
/// # Ordering
/// Ordering is based on the type alias's salsa-assigned id and not on its values.
/// The id may change between runs, or when the alias was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct ManualPEP695TypeAliasType<'db> {
    #[returns(ref)]
    pub name: ast::name::Name,
    pub definition: Option<Definition<'db>>,
    pub value: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ManualPEP695TypeAliasType<'_> {}

fn walk_manual_pep_695_type_alias<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    type_alias: ManualPEP695TypeAliasType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, type_alias.value(db));
}

impl<'db> ManualPEP695TypeAliasType<'db> {
    fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self::new(
            db,
            self.name(db),
            self.definition(db),
            self.value(db).normalized_impl(db, visitor),
        )
    }

    // TODO: with full support for manual PEP-695 style type aliases, this method should become unnecessary.
    fn recursive_type_normalized_impl(self, db: &'db dyn Db, div: Type<'db>) -> Option<Self> {
        Some(Self::new(
            db,
            self.name(db),
            self.definition(db),
            self.value(db)
                .recursive_type_normalized_impl(db, div, true)?,
        ))
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, salsa::Update, get_size2::GetSize,
)]
pub enum TypeAliasType<'db> {
    /// A type alias defined using the PEP 695 `type` statement.
    PEP695(PEP695TypeAliasType<'db>),
    /// A type alias defined by manually instantiating the PEP 695 `types.TypeAliasType`.
    ManualPEP695(ManualPEP695TypeAliasType<'db>),
}

fn walk_type_alias_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    type_alias: TypeAliasType<'db>,
    visitor: &V,
) {
    if !visitor.should_visit_lazy_type_attributes() {
        return;
    }
    match type_alias {
        TypeAliasType::PEP695(type_alias) => {
            walk_pep_695_type_alias(db, type_alias, visitor);
        }
        TypeAliasType::ManualPEP695(type_alias) => {
            walk_manual_pep_695_type_alias(db, type_alias, visitor);
        }
    }
}

impl<'db> TypeAliasType<'db> {
    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        match self {
            TypeAliasType::PEP695(type_alias) => {
                TypeAliasType::PEP695(type_alias.normalized_impl(db, visitor))
            }
            TypeAliasType::ManualPEP695(type_alias) => {
                TypeAliasType::ManualPEP695(type_alias.normalized_impl(db, visitor))
            }
        }
    }

    fn recursive_type_normalized_impl(self, db: &'db dyn Db, div: Type<'db>) -> Option<Self> {
        match self {
            TypeAliasType::PEP695(type_alias) => Some(TypeAliasType::PEP695(type_alias)),
            TypeAliasType::ManualPEP695(type_alias) => Some(TypeAliasType::ManualPEP695(
                type_alias.recursive_type_normalized_impl(db, div)?,
            )),
        }
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db str {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.name(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.name(db),
        }
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => Some(type_alias.definition(db)),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.definition(db),
        }
    }

    pub fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.value_type(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.value(db),
        }
    }

    pub(crate) fn raw_value_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.raw_value_type(db),
            TypeAliasType::ManualPEP695(type_alias) => type_alias.value(db),
        }
    }

    pub(crate) fn as_pep_695_type_alias(self) -> Option<PEP695TypeAliasType<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => Some(type_alias),
            TypeAliasType::ManualPEP695(_) => None,
        }
    }

    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        // TODO: Add support for generic non-PEP695 type aliases.
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.generic_context(db),
            TypeAliasType::ManualPEP695(_) => None,
        }
    }

    pub(crate) fn specialization(self, db: &'db dyn Db) -> Option<Specialization<'db>> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.specialization(db),
            TypeAliasType::ManualPEP695(_) => None,
        }
    }

    fn apply_function_specialization(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        match self {
            TypeAliasType::PEP695(type_alias) => type_alias.apply_function_specialization(db, ty),
            TypeAliasType::ManualPEP695(_) => ty,
        }
    }

    pub(crate) fn apply_specialization(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> Self {
        match self {
            TypeAliasType::PEP695(type_alias) => {
                TypeAliasType::PEP695(type_alias.apply_specialization(db, f))
            }
            TypeAliasType::ManualPEP695(_) => self,
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
    /// Whether the value pointed to by this type is recursively defined.
    /// If `Yes`, union literal widening is performed early.
    recursively_defined: RecursivelyDefined,
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

    /// Create a union from a list of elements without unpacking type aliases.
    pub(crate) fn from_elements_leave_aliases<I, T>(db: &'db dyn Db, elements: I) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        elements
            .into_iter()
            .fold(
                UnionBuilder::new(db).unpack_aliases(false),
                |builder, element| builder.add(element.into()),
            )
            .build()
    }

    fn from_elements_cycle_recovery<I, T>(db: &'db dyn Db, elements: I) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        elements
            .into_iter()
            .fold(
                UnionBuilder::new(db).cycle_recovery(true),
                |builder, element| builder.add(element.into()),
            )
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
    pub(crate) fn map(
        self,
        db: &'db dyn Db,
        transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        self.elements(db)
            .iter()
            .map(transform_fn)
            .fold(UnionBuilder::new(db), |builder, element| {
                builder.add(element)
            })
            .recursively_defined(self.recursively_defined(db))
            .build()
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
        let mut builder = UnionBuilder::new(db);
        for element in self.elements(db).iter().map(transform_fn) {
            builder = builder.add(element?);
        }
        builder = builder.recursively_defined(self.recursively_defined(db));
        Some(builder.build())
    }

    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Option<Type<'db>> {
        self.try_map(db, |element| element.to_instance(db))
    }

    pub(crate) fn filter(
        self,
        db: &'db dyn Db,
        mut f: impl FnMut(&Type<'db>) -> bool,
    ) -> Type<'db> {
        self.elements(db)
            .iter()
            .filter(|ty| f(ty))
            .fold(UnionBuilder::new(db), |builder, element| {
                builder.add(*element)
            })
            .recursively_defined(self.recursively_defined(db))
            .build()
    }

    pub(crate) fn map_with_boundness(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Place<'db>,
    ) -> Place<'db> {
        let mut builder = UnionBuilder::new(db);

        let mut all_unbound = true;
        let mut possibly_unbound = false;
        let mut origin = TypeOrigin::Declared;
        for ty in self.elements(db) {
            let ty_member = transform_fn(ty);
            match ty_member {
                Place::Undefined => {
                    possibly_unbound = true;
                }
                Place::Defined(ty_member, member_origin, member_boundness) => {
                    origin = origin.merge(member_origin);
                    if member_boundness == Definedness::PossiblyUndefined {
                        possibly_unbound = true;
                    }

                    all_unbound = false;
                    builder = builder.add(ty_member);
                }
            }
        }

        if all_unbound {
            Place::Undefined
        } else {
            Place::Defined(
                builder
                    .recursively_defined(self.recursively_defined(db))
                    .build(),
                origin,
                if possibly_unbound {
                    Definedness::PossiblyUndefined
                } else {
                    Definedness::AlwaysDefined
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
        let mut origin = TypeOrigin::Declared;
        for ty in self.elements(db) {
            let PlaceAndQualifiers {
                place: ty_member,
                qualifiers: new_qualifiers,
            } = transform_fn(ty);
            qualifiers |= new_qualifiers;
            match ty_member {
                Place::Undefined => {
                    possibly_unbound = true;
                }
                Place::Defined(ty_member, member_origin, member_boundness) => {
                    origin = origin.merge(member_origin);
                    if member_boundness == Definedness::PossiblyUndefined {
                        possibly_unbound = true;
                    }

                    all_unbound = false;
                    builder = builder.add(ty_member);
                }
            }
        }
        PlaceAndQualifiers {
            place: if all_unbound {
                Place::Undefined
            } else {
                Place::Defined(
                    builder
                        .recursively_defined(self.recursively_defined(db))
                        .build(),
                    origin,
                    if possibly_unbound {
                        Definedness::PossiblyUndefined
                    } else {
                        Definedness::AlwaysDefined
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
    pub(crate) fn normalized(self, db: &'db dyn Db) -> Type<'db> {
        self.normalized_impl(db, &NormalizedVisitor::default())
    }

    pub(crate) fn normalized_impl(
        self,
        db: &'db dyn Db,
        visitor: &NormalizedVisitor<'db>,
    ) -> Type<'db> {
        self.elements(db)
            .iter()
            .map(|ty| ty.normalized_impl(db, visitor))
            .fold(
                UnionBuilder::new(db)
                    .order_elements(true)
                    .unpack_aliases(true),
                UnionBuilder::add,
            )
            .recursively_defined(self.recursively_defined(db))
            .build()
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Type<'db>> {
        let mut builder = UnionBuilder::new(db)
            .order_elements(false)
            .unpack_aliases(false)
            .cycle_recovery(true)
            .recursively_defined(self.recursively_defined(db));
        let mut empty = true;
        for ty in self.elements(db) {
            if nested {
                // list[T | Divergent] => list[Divergent]
                let ty = ty.recursive_type_normalized_impl(db, div, nested)?;
                if ty == div {
                    return Some(ty);
                }
                builder = builder.add(ty);
                empty = false;
            } else {
                // `Divergent` in a union type does not mean true divergence, so we skip it if not nested.
                // e.g. T | Divergent == T | (T | (T | (T | ...))) == T
                if ty == &div {
                    builder = builder.recursively_defined(RecursivelyDefined::Yes);
                    continue;
                }
                builder = builder.add(
                    ty.recursive_type_normalized_impl(db, div, nested)
                        .unwrap_or(div),
                );
                empty = false;
            }
        }
        if empty {
            builder = builder.add(div);
        }
        Some(builder.build())
    }

    pub(crate) fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        _inferable: InferableTypeVars<'_, 'db>,
        _visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if self == other {
            return ConstraintSet::from(true);
        }

        let self_elements = self.elements(db);
        let other_elements = other.elements(db);

        if self_elements.len() != other_elements.len() {
            return ConstraintSet::from(false);
        }

        let sorted_self = self.normalized(db);

        if sorted_self == Type::Union(other) {
            return ConstraintSet::from(true);
        }

        ConstraintSet::from(sorted_self == other.normalized(db))
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
    pub(crate) fn from_elements<I, T>(db: &'db dyn Db, elements: I) -> Type<'db>
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        IntersectionBuilder::new(db)
            .positive_elements(elements)
            .build()
    }

    /// Return a new `IntersectionType` instance with the positive and negative types sorted
    /// according to a canonical ordering, and other normalizations applied to each element as applicable.
    ///
    /// See [`Type::normalized`] for more details.
    #[must_use]
    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        self.normalized_impl(db, &NormalizedVisitor::default())
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        fn normalized_set<'db>(
            db: &'db dyn Db,
            elements: &FxOrderSet<Type<'db>>,
            visitor: &NormalizedVisitor<'db>,
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

    pub(crate) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        fn opt_normalized_set<'db>(
            db: &'db dyn Db,
            elements: &FxOrderSet<Type<'db>>,
            div: Type<'db>,
            nested: bool,
        ) -> Option<FxOrderSet<Type<'db>>> {
            elements
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, div, nested))
                .collect()
        }

        fn normalized_set<'db>(
            db: &'db dyn Db,
            elements: &FxOrderSet<Type<'db>>,
            div: Type<'db>,
            nested: bool,
        ) -> FxOrderSet<Type<'db>> {
            elements
                .iter()
                .map(|ty| {
                    ty.recursive_type_normalized_impl(db, div, nested)
                        .unwrap_or(div)
                })
                .collect()
        }

        let positive = if nested {
            opt_normalized_set(db, self.positive(db), div, nested)?
        } else {
            normalized_set(db, self.positive(db), div, nested)
        };
        let negative = if nested {
            opt_normalized_set(db, self.negative(db), div, nested)?
        } else {
            normalized_set(db, self.negative(db), div, nested)
        };

        Some(IntersectionType::new(db, positive, negative))
    }

    /// Return `true` if `self` represents exactly the same set of possible runtime objects as `other`
    pub(crate) fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        _inferable: InferableTypeVars<'_, 'db>,
        _visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if self == other {
            return ConstraintSet::from(true);
        }

        let self_positive = self.positive(db);
        let other_positive = other.positive(db);

        if self_positive.len() != other_positive.len() {
            return ConstraintSet::from(false);
        }

        let self_negative = self.negative(db);
        let other_negative = other.negative(db);

        if self_negative.len() != other_negative.len() {
            return ConstraintSet::from(false);
        }

        let sorted_self = self.normalized(db);

        if sorted_self == other {
            return ConstraintSet::from(true);
        }

        ConstraintSet::from(sorted_self == other.normalized(db))
    }

    /// Returns an iterator over the positive elements of the intersection. If
    /// there are no positive elements, returns a single `object` type.
    fn positive_elements_or_object(self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> {
        if self.positive(db).is_empty() {
            Either::Left(std::iter::once(Type::object()))
        } else {
            Either::Right(self.positive(db).iter().copied())
        }
    }

    /// Map a type transformation over all positive elements of the intersection. Leave the
    /// negative elements unchanged.
    pub(crate) fn map_positive(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        let mut builder = IntersectionBuilder::new(db);
        for ty in self.positive(db) {
            builder = builder.add_positive(transform_fn(ty));
        }
        for ty in self.negative(db) {
            builder = builder.add_negative(*ty);
        }
        builder.build()
    }

    pub(crate) fn map_with_boundness(
        self,
        db: &'db dyn Db,
        mut transform_fn: impl FnMut(&Type<'db>) -> Place<'db>,
    ) -> Place<'db> {
        let mut builder = IntersectionBuilder::new(db);

        let mut all_unbound = true;
        let mut any_definitely_bound = false;
        let mut origin = TypeOrigin::Declared;
        for ty in self.positive_elements_or_object(db) {
            let ty_member = transform_fn(&ty);
            match ty_member {
                Place::Undefined => {}
                Place::Defined(ty_member, member_origin, member_boundness) => {
                    origin = origin.merge(member_origin);
                    all_unbound = false;
                    if member_boundness == Definedness::AlwaysDefined {
                        any_definitely_bound = true;
                    }

                    builder = builder.add_positive(ty_member);
                }
            }
        }

        if all_unbound {
            Place::Undefined
        } else {
            Place::Defined(
                builder.build(),
                origin,
                if any_definitely_bound {
                    Definedness::AlwaysDefined
                } else {
                    Definedness::PossiblyUndefined
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

        let mut all_unbound = true;
        let mut any_definitely_bound = false;
        let mut origin = TypeOrigin::Declared;
        for ty in self.positive_elements_or_object(db) {
            let PlaceAndQualifiers {
                place: member,
                qualifiers: new_qualifiers,
            } = transform_fn(&ty);
            qualifiers |= new_qualifiers;
            match member {
                Place::Undefined => {}
                Place::Defined(ty_member, member_origin, member_boundness) => {
                    origin = origin.merge(member_origin);
                    all_unbound = false;
                    if member_boundness == Definedness::AlwaysDefined {
                        any_definitely_bound = true;
                    }

                    builder = builder.add_positive(ty_member);
                }
            }
        }

        PlaceAndQualifiers {
            place: if all_unbound {
                Place::Undefined
            } else {
                Place::Defined(
                    builder.build(),
                    origin,
                    if any_definitely_bound {
                        Definedness::AlwaysDefined
                    } else {
                        Definedness::PossiblyUndefined
                    },
                )
            },
            qualifiers,
        }
    }

    pub fn iter_positive(self, db: &'db dyn Db) -> impl Iterator<Item = Type<'db>> {
        self.positive(db).iter().copied()
    }

    pub(crate) fn has_one_element(self, db: &'db dyn Db) -> bool {
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
    value: CompactString,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for StringLiteralType<'_> {}

impl<'db> StringLiteralType<'db> {
    /// The length of the string, as would be returned by Python's `len()`.
    pub(crate) fn python_len(self, db: &'db dyn Db) -> usize {
        self.value(db).chars().count()
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
    pub(crate) fn enum_class_instance(self, db: &'db dyn Db) -> Type<'db> {
        self.enum_class(db).to_non_generic_instance(db)
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
    pub(crate) fn place_name(self, db: &'db dyn Db) -> Option<String> {
        let (scope, place) = self.place_info(db)?;
        let table = place_table(db, scope);

        Some(format!("{}", table.place(place)))
    }

    pub(crate) fn unbound(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeIs(Self::new(db, ty, None))
    }

    pub(crate) fn bound(
        db: &'db dyn Db,
        return_type: Type<'db>,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
    ) -> Type<'db> {
        Type::TypeIs(Self::new(db, return_type, Some((scope, place))))
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
        self.return_type(db)
            .with_polarity(TypeVarVariance::Invariant)
            .variance_of(db, typevar)
    }
}

/// Walk the MRO of this class and return the last class just before the specified known base.
/// This can be used to determine upper bounds for `Self` type variables on methods that are
/// being added to the given class.
pub(super) fn determine_upper_bound<'db>(
    db: &'db dyn Db,
    class_literal: ClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    is_known_base: impl Fn(ClassBase<'db>) -> bool,
) -> Type<'db> {
    let upper_bound = class_literal
        .iter_mro(db, specialization)
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::db::tests::{TestDbBuilder, setup_db};
    use crate::place::{typing_extensions_symbol, typing_symbol};
    use ruff_db::system::DbWithWritableSystem as _;
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
        // acknowledged limitation of the current implementation. We cannot
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

    #[test]
    fn divergent_type() {
        let db = setup_db();
        let div = Type::divergent(salsa::plumbing::Id::from_bits(1));

        // The `Divergent` type must not be eliminated in union with other dynamic types,
        // as this would prevent detection of divergent type inference using `Divergent`.
        let union = UnionType::from_elements(&db, [Type::unknown(), div]);
        assert_eq!(union.display(&db).to_string(), "Unknown | Divergent");

        let union = UnionType::from_elements(&db, [div, Type::unknown()]);
        assert_eq!(union.display(&db).to_string(), "Divergent | Unknown");

        let union = UnionType::from_elements(&db, [div, Type::unknown(), todo_type!("1")]);
        assert_eq!(union.display(&db).to_string(), "Divergent | Unknown");

        assert!(div.is_equivalent_to(&db, div));
        assert!(!div.is_equivalent_to(&db, Type::unknown()));
        assert!(!Type::unknown().is_equivalent_to(&db, div));
        assert!(!div.is_redundant_with(&db, Type::unknown()));
        assert!(!Type::unknown().is_redundant_with(&db, div));

        let truthy_div = IntersectionBuilder::new(&db)
            .add_positive(div)
            .add_negative(Type::AlwaysFalsy)
            .build();

        let union = UnionType::from_elements(&db, [Type::unknown(), truthy_div]);
        assert!(!truthy_div.is_redundant_with(&db, Type::unknown()));
        assert_eq!(
            union.display(&db).to_string(),
            "Unknown | (Divergent & ~AlwaysFalsy)"
        );

        let union = UnionType::from_elements(&db, [truthy_div, Type::unknown()]);
        assert!(!Type::unknown().is_redundant_with(&db, truthy_div));
        assert_eq!(
            union.display(&db).to_string(),
            "(Divergent & ~AlwaysFalsy) | Unknown"
        );

        // The `object` type has a good convergence property, that is, its union with all other types is `object`.
        // (e.g. `object | tuple[Divergent] == object`, `object | tuple[object] == object`)
        // So we can safely eliminate `Divergent`.
        let union = UnionType::from_elements(&db, [div, KnownClass::Object.to_instance(&db)]);
        assert_eq!(union.display(&db).to_string(), "object");

        let union = UnionType::from_elements(&db, [KnownClass::Object.to_instance(&db), div]);
        assert_eq!(union.display(&db).to_string(), "object");

        let recursive = UnionType::from_elements(
            &db,
            [
                KnownClass::List.to_specialized_instance(&db, [div]),
                Type::none(&db),
            ],
        );
        let nested_rec = KnownClass::List.to_specialized_instance(&db, [recursive]);
        assert_eq!(
            nested_rec.display(&db).to_string(),
            "list[list[Divergent] | None]"
        );
        let normalized = nested_rec
            .recursive_type_normalized_impl(&db, div, false)
            .unwrap();
        assert_eq!(normalized.display(&db).to_string(), "list[Divergent]");

        let union = UnionType::from_elements(&db, [div, KnownClass::Int.to_instance(&db)]);
        assert_eq!(union.display(&db).to_string(), "Divergent | int");
        let normalized = union
            .recursive_type_normalized_impl(&db, div, false)
            .unwrap();
        assert_eq!(normalized.display(&db).to_string(), "int");

        // The same can be said about intersections for the `Never` type.
        let intersection = IntersectionBuilder::new(&db)
            .add_positive(Type::Never)
            .add_positive(div)
            .build();
        assert_eq!(intersection.display(&db).to_string(), "Never");

        let intersection = IntersectionBuilder::new(&db)
            .add_positive(div)
            .add_positive(Type::Never)
            .build();
        assert_eq!(intersection.display(&db).to_string(), "Never");
    }

    #[test]
    fn type_alias_variance() {
        use crate::db::tests::TestDb;
        use crate::place::global_symbol;

        fn get_type_alias<'db>(db: &'db TestDb, name: &str) -> PEP695TypeAliasType<'db> {
            let module = ruff_db::files::system_path_to_file(db, "/src/a.py").unwrap();
            let ty = global_symbol(db, module, name).place.expect_type();
            let Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(
                type_alias,
            ))) = ty
            else {
                panic!("Expected `{name}` to be a type alias");
            };
            type_alias
        }
        fn get_bound_typevar<'db>(
            db: &'db TestDb,
            type_alias: PEP695TypeAliasType<'db>,
        ) -> BoundTypeVarInstance<'db> {
            let generic_context = type_alias.generic_context(db).unwrap();
            generic_context.variables(db).next().unwrap()
        }

        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
class Covariant[T]:
    def get(self) -> T:
        raise ValueError

class Contravariant[T]:
    def set(self, value: T):
        pass

class Invariant[T]:
    def get(self) -> T:
        raise ValueError
    def set(self, value: T):
        pass

class Bivariant[T]:
    pass

type CovariantAlias[T] = Covariant[T]
type ContravariantAlias[T] = Contravariant[T]
type InvariantAlias[T] = Invariant[T]
type BivariantAlias[T] = Bivariant[T]

type RecursiveAlias[T] = None | list[RecursiveAlias[T]]
type RecursiveAlias2[T] = None | list[T] | list[RecursiveAlias2[T]]
"#,
        )
        .unwrap();
        let covariant = get_type_alias(&db, "CovariantAlias");
        assert_eq!(
            KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(covariant))
                .variance_of(&db, get_bound_typevar(&db, covariant)),
            TypeVarVariance::Covariant
        );

        let contravariant = get_type_alias(&db, "ContravariantAlias");
        assert_eq!(
            KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(contravariant))
                .variance_of(&db, get_bound_typevar(&db, contravariant)),
            TypeVarVariance::Contravariant
        );

        let invariant = get_type_alias(&db, "InvariantAlias");
        assert_eq!(
            KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(invariant))
                .variance_of(&db, get_bound_typevar(&db, invariant)),
            TypeVarVariance::Invariant
        );

        let bivariant = get_type_alias(&db, "BivariantAlias");
        assert_eq!(
            KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(bivariant))
                .variance_of(&db, get_bound_typevar(&db, bivariant)),
            TypeVarVariance::Bivariant
        );

        let recursive = get_type_alias(&db, "RecursiveAlias");
        assert_eq!(
            KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(recursive))
                .variance_of(&db, get_bound_typevar(&db, recursive)),
            TypeVarVariance::Bivariant
        );

        let recursive2 = get_type_alias(&db, "RecursiveAlias2");
        assert_eq!(
            KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(recursive2))
                .variance_of(&db, get_bound_typevar(&db, recursive2)),
            TypeVarVariance::Invariant
        );
    }

    #[test]
    fn eager_expansion() {
        use crate::db::tests::TestDb;
        use crate::place::global_symbol;

        fn get_type_alias<'db>(db: &'db TestDb, name: &str) -> Type<'db> {
            let module = ruff_db::files::system_path_to_file(db, "/src/a.py").unwrap();
            let ty = global_symbol(db, module, name).place.expect_type();
            let Type::KnownInstance(KnownInstanceType::TypeAliasType(TypeAliasType::PEP695(
                type_alias,
            ))) = ty
            else {
                panic!("Expected `{name}` to be a type alias");
            };
            Type::TypeAlias(TypeAliasType::PEP695(type_alias))
        }

        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"

type IntStr = int | str
type ListIntStr = list[IntStr]
type RecursiveList[T] = list[T | RecursiveList[T]]
type RecursiveIntList = RecursiveList[int]
type Itself = Itself
type A = B
type B = A
type G[T] = H[T]
type H[T] = G[T]
"#,
        )
        .unwrap();

        let int_str = get_type_alias(&db, "IntStr");
        assert_eq!(
            int_str.expand_eagerly(&db).display(&db).to_string(),
            "int | str",
        );

        let list_int_str = get_type_alias(&db, "ListIntStr");
        assert_eq!(
            list_int_str.expand_eagerly(&db).display(&db).to_string(),
            "list[int | str]",
        );

        let rec_list = get_type_alias(&db, "RecursiveList");
        assert_eq!(
            rec_list.expand_eagerly(&db).display(&db).to_string(),
            "list[Divergent]",
        );

        let rec_int_list = get_type_alias(&db, "RecursiveIntList");
        assert_eq!(
            rec_int_list.expand_eagerly(&db).display(&db).to_string(),
            "list[Divergent]",
        );

        let itself = get_type_alias(&db, "Itself");
        assert_eq!(
            itself.expand_eagerly(&db).display(&db).to_string(),
            "Divergent",
        );

        let a = get_type_alias(&db, "A");
        assert_eq!(a.expand_eagerly(&db).display(&db).to_string(), "Divergent",);

        let b = get_type_alias(&db, "B");
        assert_eq!(b.expand_eagerly(&db).display(&db).to_string(), "Divergent",);

        let g = get_type_alias(&db, "G");
        assert_eq!(g.expand_eagerly(&db).display(&db).to_string(), "Divergent",);

        let h = get_type_alias(&db, "H");
        assert_eq!(h.expand_eagerly(&db).display(&db).to_string(), "Divergent",);
    }
}
