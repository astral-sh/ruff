use self::mapping::{MappingStart, TypeMappingContinuation};
use compact_str::{CompactString, ToCompactString};
use itertools::Itertools;
use ruff_diagnostics::{Edit, Fix};
use rustc_hash::FxHashSet;

use smallvec::SmallVec;
use std::borrow::Cow;
use std::cell::OnceCell;
use std::future::Future;
use std::iter;
use std::rc::Rc;

use bitflags::bitflags;
use call::{CallDunderError, CallError, CallErrorKind};
use context::InferContext;
pub use context::ProgramEnvironment;
use ruff_db::diagnostic::{Annotation, Diagnostic, Span};
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_text_size::Ranged;
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::execution_probe::{
    FixedQueryKeyProfile as CopyMemoProfile, PassiveMemoGroup, RegistryBuilder, RunResult,
};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::function::{Configuration, IngredientImpl};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::interned::FiniteInternedConfiguration;
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::{QuoteError, QuoteFuel};
use smallvec::smallvec_inline;
use ty_module_resolver::{
    ImportingFile, KnownModule, Module, ModuleName, file_to_module, resolve_module,
};

pub(crate) use self::callable::UpcastPolicy;
use self::class::ClassInstanceFlags;
use self::class::namespace::{
    InlineNamespaceLookupEffects, NamespaceLookupRequest, namespace_lookup_sync,
};
pub use self::cyclic::CycleDetector;
pub(crate) use self::cyclic::TypeTransformer;
pub use self::cyclic::entry::CallableGuardOperation;
pub use self::call::bind::constructor_preparation::ConstructorStorageOperation;
pub use self::call::preparation::bound_method::BoundMethodPreparationOperation;
pub use self::signatures::constructor_preparation::ConstructorSignatureOperation;
use self::cyclic::{
    ActiveRecursionDetector, CallableExpansion, CallableRecursionGuard, HasIdentity, TypeIdentity,
};
pub use self::dedicated::pytest::{
    FixtureBinding, FixtureExposure, FixtureNameSource, PytestTest, fixture_bindings_for_parameter,
    fixture_exposures_for_definition, pytest_global_plugin_files, pytest_tests_in_file,
};
pub(crate) use self::diagnostic::TypeCheckDiagnostics;
pub(crate) use self::diagnostic::register_lints;
pub use self::diagnostic::{UNDEFINED_REVEAL, UNRESOLVED_REFERENCE};
pub(crate) use self::infer::{
    InferredDeclaration, TypeContext, infer_complete_scope_types, infer_deferred_types,
    infer_definition_types, infer_expression_type, infer_expression_types,
    infer_same_file_expression_type, infer_scope_types, is_discarded_dict_key_assignment,
};
pub(crate) use self::iteration::extract_fixed_length_iterable_element_types;
pub use self::known_instance::KnownInstanceType;
use self::mapping::effects::{
    InlineMappingEffects, MappingEffects, SynchronousMappingEffects, inline_mapping_result,
};
pub(crate) use self::match_pattern::{
    ClassPatternPositionalSource, class_pattern_positional_sources, definite_match_pattern_type,
    definite_match_pattern_type_for_subject, exact_sequence_pattern_type, mapping_pattern_type,
    pattern_binding_fallthrough_type, sequence_pattern_type_builder, singleton_pattern_type,
    starred_sequence_pattern_type, typed_dict_matches_class_pattern,
};
pub use self::normalization::RecursiveNormalizationOperation;
use self::normalization::{
    NormalizationFacts, OrdinaryNormalizationEffects, RecursiveNormalizationFacts,
    RecursiveNormalizationRequest, cycle_normalized_sync, recursive_normalize_sync,
    recursive_type_normalized_with_cycle_sync,
};
use self::promotion::{
    InlinePublicPromotionEffects, PublicPromotionEffects, PublicPromotionWork,
    inline_public_promotion_result,
};
pub(crate) use self::relation_error::{ErrorContext, ErrorContextTree, ParameterDescription};
use self::set_theoretic::NegativeIntersectionElements;
pub(crate) use self::set_theoretic::builder::{
    IntersectionBuilder, UnionAccumulator, UnionBuilder,
};
pub use self::set_theoretic::{IntersectionType, UnionType};
use self::set_theoretic::{KnownUnion, RecursivelyDefined};
pub(crate) use self::signatures::Signature;
pub(crate) use self::signatures::effects::legacy_inline;
#[cfg(test)]
pub(crate) use self::signatures::effects::try_poll_immediate;
pub use self::signatures::{ParameterDefault, ParameterKind};
pub(crate) use self::subclass_of::{SubclassOfInner, SubclassOfType};
pub(crate) use self::type_expansion::expand_type;
pub(crate) use crate::diagnostic::add_inferred_python_version_hint_to_diagnostic;
use crate::place::{
    DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, TypeOrigin,
    builtins_module_scope, imported_symbol, known_module_symbol,
};
use crate::types::bound_super::BoundSuperType;
use crate::types::call::bind::ConstructorCallableKind;
use crate::types::call::{Binding, Bindings, CallArguments, CallableBinding};
pub(crate) use crate::types::callable::{CallableType, CallableTypes};
pub(crate) use crate::types::class_base::ClassBase;
use crate::types::constraints::ConstraintSetBuilder;
#[cfg(any(test, feature = "experimental-analysis"))]
use crate::types::constraints::{OwnedConstraintSet, OwnedConstraintSetProfile};
use crate::types::context::{LintDiagnosticGuard, LintDiagnosticGuardBuilder};
use crate::types::diagnostic::{
    AttributeAccessMethod, INVALID_AWAIT, INVALID_TYPE_FORM, report_bad_attribute_access_call,
    report_bad_dunder_get_call, report_bad_import_call,
};
pub use crate::types::display::{DisplaySettings, TypeDetail, TypeDisplayDetails};
pub(crate) use crate::types::enums::{EnumClassLiteral, EnumComplementType, enum_metadata};
pub(crate) use crate::types::equality::{ComparisonSoundnessPolicy, equality_truthiness};
pub(crate) use crate::types::function::FunctionType;
use crate::types::function::{
    DataclassTransformerFlags, DataclassTransformerParams, FunctionDecorators, FunctionSpans,
    KnownFunction, OverloadLiteral,
};
pub(crate) use crate::types::generics::GenericContext;
use crate::types::generics::{ApplySpecialization, Specialization};
use crate::types::infer::InferenceFlags;
use crate::types::known_instance::{
    InternedConstraintSet, InternedType, SentinelInstance, UnionTypeInstance,
};
pub use crate::types::method::{BoundMethodType, KnownBoundMethodType, WrapperDescriptorKind};
use crate::types::mro::{MroIterator, StaticMroError};
pub(crate) use crate::types::narrow::{NarrowingConstraint, infer_narrowing_constraints};
use crate::types::newtype::NewType;
pub(crate) use crate::types::signatures::{CallableSignature, Parameter, Parameters};
use crate::types::signatures::{ConcatenateTail, walk_signature};
use crate::types::special_form::TypeQualifier;
use crate::types::tuple::TupleSpec;
pub use crate::types::type_alias::TypeAliasType;
pub use crate::types::type_form::TypeFormType;
pub(crate) use crate::types::typed_dict::TypedDictType;
pub(crate) use crate::types::typevar::{
    BindingContext, BoundTypeVarIdentity, ParamSpecAttrKind, TypeVarBoundOrConstraints,
    TypeVarNonce,
};
pub use crate::types::typevar::{BoundTypeVarInstance, TypeVarKind};
use crate::types::typevar::{TypeVarInstance, TypeVarSet};
pub use crate::types::variance::TypeVarVariance;
use crate::types::variance::{VarianceInferable, VarianceTerm};
use crate::types::visitor::{
    any_over_type, any_over_type_including_alias_arguments, dynamic_content,
};
use crate::{Db, FxOrderSet, Program};
#[cfg(feature = "experimental-analysis")]
pub use bool::TruthinessOperation;
#[cfg(feature = "experimental-analysis")]
pub use callable::CallableConversionOperation;
#[cfg(feature = "experimental-analysis")]
pub use class::KnownClassInstanceOperation;
pub(crate) use class::{ClassLiteral, ClassType, GenericAlias, StaticClassLiteral};
pub use class::{KnownClass, MethodDecorator, SlotDescriptorType};
#[cfg(any(test, feature = "experimental-analysis"))]
pub use descriptor::effects::DescriptorOperation;
#[cfg(feature = "experimental-analysis")]
pub use equality::source::EqualityOperation;
pub use generics::defaults::DefaultSpecializationOperation;
#[cfg(feature = "experimental-analysis")]
pub use infer::AnnotatedAssignmentOperation;
#[cfg(feature = "experimental-analysis")]
pub use infer::TypeComparisonOperation;
#[cfg(feature = "experimental-analysis")]
pub use infer::{
    AttributeOperation, ChainedComparisonOperation, DecoratorApplicationOperation,
    SourceDefinitionEffect, SourceExpressionOperation,
};
use instance::Protocol;
#[cfg(feature = "experimental-analysis")]
pub use instance::tuple_spec::TupleSpecOperation;
pub use instance::{NominalInstanceType, ProtocolInstanceType};
pub use legacy_typevars::LegacyTypeVarOperation;
pub(crate) use literal::{
    BytesLiteralType, EnumLiteralType, LiteralValueType, LiteralValueTypeKind, StringLiteralType,
};
pub use mapping::MaterializationOperation;
pub use mapping::effects::MappingOperation;
#[cfg(feature = "experimental-analysis")]
pub use member_lookup::general::GeneralMemberOperation;
#[cfg(feature = "experimental-analysis")]
pub use relation::source_operations::RelationOperation;
pub use special_form::SpecialFormType;
use ty_python_core::definition::Definition;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::ScopeId;
use ty_python_core::{ProgramFile, Truthiness, place_table, semantic_index};
pub use type_expression_conversion::TypeConversionOperation;
#[cfg(feature = "experimental-analysis")]
pub use visitor::SearchOperation;

mod abstract_methods;
mod attribute_write;
mod bool;
mod bound_super;
mod call;
mod callable;
pub(crate) mod check;
mod class;
mod class_base;
mod class_selection;
mod constraints;
mod constructor;
mod context;
mod context_manager;
mod cyclic;
mod data_descriptor;
mod dedicated;
mod definition_expression;
mod descriptor;
mod diagnostic;
mod display;
mod enums;
mod equality;
mod fallible_sort;
mod function;
mod generic_attribute;
mod generics;
pub mod ide_support;
mod infer;
#[cfg(feature = "experimental-analysis")]
pub(crate) use infer::{run_source_expression, run_source_file};
mod instance;
mod iteration;
mod known_instance;
mod legacy_typevars;
pub mod list_members;
mod literal;
pub(crate) mod local_transfer;
mod mapping;
mod match_pattern;
mod member;
mod member_lookup;
mod method;
mod module_member_effects;
mod mro;
pub(crate) mod narrow;
mod negation;
mod newtype;
mod normalization;
mod overrides;
pub(crate) mod promotion;
mod property_deprecations;
mod property_provenance;
mod protocol_class;
mod recursive;
pub(crate) use recursive::RecursiveMapping;
pub use recursive::{RecursiveType, RecursiveVar, UnfoldResult};
pub(crate) mod relation;
mod relation_error;
mod runtime_visibility;
mod set_theoretic;
mod signatures;
mod source_read;
mod special_form;
mod storage_quote;
mod string_annotation;
mod subclass_of;
#[cfg(test)]
pub(crate) mod tests;
mod tuple;
mod type_alias;
mod type_expansion;
pub(in crate::types) mod type_expression_conversion;
mod type_form;
mod typed_dict;
mod typevar;
mod unpacker;
mod variance;
mod visitor;

mod definition;
pub(crate) mod definition_resolution;
#[cfg(test)]
mod property_tests;
mod subscript;

#[cfg(test)]
pub(crate) fn observe_constructor_probe_event(event: &salsa::EventKind) {
    constructor::expansion_probe::observe_salsa_event(event);
}

pub fn check_types(db: &dyn Db, file: ProgramFile<'_>) -> Vec<Diagnostic> {
    check::check_types(db, file)
}

/// Infer the type of a binding.
pub(crate) fn binding_type<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Type<'db> {
    let inference = infer_definition_types(db, definition);
    inference.binding_type(definition)
}

/// Returns whether a definition may represent a value that exists at runtime.
///
/// Type-checking-only decorators and guards never represent runtime values. Private type-variable
/// declarations, explicit aliases, and unambiguous typing aliases in stub files are also
/// typing-only, while public aliases and genuine runtime values remain visible.
///
/// ```python
/// _T = TypeVar("_T")  # Typing-only helper.
/// _Alias: TypeAlias = list[int]  # Typing-only alias.
/// _runtime_typevar = make_typevar()  # Runtime value.
/// _runtime_callback = callbacks[0]  # Runtime value.
/// ```
#[salsa::tracked(configuration = (pub(in crate::types) MayExistAtRuntimeConfiguration), attempt = ReturnOnly, returns(copy))]
pub(crate) fn may_exist_at_runtime<'db>(db: &'db dyn Db, definition: Definition<'db>) -> bool {
    match runtime_visibility::runtime_visibility_sync(
        definition,
        &runtime_visibility::InlineRuntimeVisibilityEffects(db),
    ) {
        Ok(visible) => visible,
        Err(never) => match never {},
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(crate) fn runtime_visibility_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<MayExistAtRuntimeConfiguration> {
    may_exist_at_runtime::fn_ingredient_(db, db.zalsa())
}

/// Infer the type of a declaration, returning `Rejected` if it is not valid.
pub(crate) fn inferred_declaration<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> InferredDeclaration<'db> {
    let inference = infer_definition_types(db, definition);
    inference.inferred_declaration(definition)
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
    match definition_expression::definition_expression_type_sync(
        definition,
        expression,
        &definition_expression::InlineDefinitionExpressionEffects(db),
    ) {
        Ok(ty) => ty,
        Err(never) => match never {},
    }
}

/// Infer the type and qualifiers of a deferred annotation expression that is a sub-expression of
/// a [`Definition`].
///
/// Supports expressions that are evaluated within a type-params sub-scope.
fn definition_expression_annotation<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    expression: &ast::Expr,
) -> TypeAndQualifiers<'db> {
    let file = definition.program_file(db);
    let index = semantic_index(db, file);
    let file_scope = index.expression_scope_id(expression);
    let scope = file_scope.to_scope_id(db, file);
    if scope == definition.scope(db) {
        let inference = infer_deferred_types(db, definition);
        TypeAndQualifiers::new(
            inference.expression_type(expression),
            TypeOrigin::Declared,
            inference.qualifiers(expression),
        )
    } else {
        let inference = infer_complete_scope_types(db, scope);
        TypeAndQualifiers::new(
            inference.expression_type(expression),
            TypeOrigin::Declared,
            inference.qualifiers(expression),
        )
    }
}

/// Active recursion state shared across nested type operations.
///
/// A transformation cache belongs to one mapping, but recursion can span specialization,
/// materialization, and meta-type projection. Preserve this context when starting a new mapping
/// visitor. Each operation keeps its own guards because its recursion keys and cycle fallbacks
/// differ.
#[derive(Default)]
struct TypeRecursionContext<'db> {
    meta_type: MetaTypeRecursion<'db>,
}

/// Guards shared by meta-type projections and the specializations they trigger.
///
/// Each projection also tracks direct alias recursion locally: those cycles add no new classes,
/// whereas re-entering through another projection can introduce metaclasses.
#[derive(Default)]
struct MetaTypeRecursion<'db> {
    aliases: ActiveRecursionDetector<(Program<'db>, TypeAliasType<'db>)>,
    growing_aliases: ActiveRecursionDetector<(Program<'db>, Definition<'db>)>,
    typevars: ActiveRecursionDetector<(Program<'db>, BoundTypeVarIdentity<'db>)>,
}

impl MetaTypeRecursion<'_> {
    fn is_active(&self) -> bool {
        !self.aliases.is_empty() || !self.growing_aliases.is_empty() || !self.typevars.is_empty()
    }
}

pub(crate) struct ApplyTypeMappingTag;
struct ApplyMaterializationEquivalence;

type MaterializationEquivalenceVisitor<'db> =
    Rc<CycleDetector<'db, ApplyMaterializationEquivalence, (Type<'db>, Type<'db>), bool, 1>>;

/// A [`TypeTransformer`] that is used in `apply_type_mapping` methods.
///
/// Some recursive transformations visit the same type under more than one mapping mode within a
/// single call chain. Keep separate cycle caches for those modes so one transformation cannot
/// reuse the result of another.
pub(crate) struct ApplyTypeMappingVisitor<'env, 'db> {
    env: &'env ProgramEnvironment<'db>,
    recursion_context: Option<&'env TypeRecursionContext<'db>>,
    /// Whether materialization also transforms type-variable bounds and defaults.
    materialize_typevar_bounds_and_defaults: bool,
    default: OnceCell<Box<TypeTransformer<'db, ApplyTypeMappingTag>>>,
    top_materialization: OnceCell<Box<TypeTransformer<'db, ApplyTypeMappingTag>>>,
    bottom_materialization: OnceCell<Box<TypeTransformer<'db, ApplyTypeMappingTag>>>,
    top_specialization_materialization: OnceCell<Box<TypeTransformer<'db, ApplyTypeMappingTag>>>,
    bottom_specialization_materialization: OnceCell<Box<TypeTransformer<'db, ApplyTypeMappingTag>>>,
    promotion: OnceCell<Box<TypeTransformer<'db, ApplyTypeMappingTag>>>,
    skip_promotion: OnceCell<Box<TypeTransformer<'db, ApplyTypeMappingTag>>>,
    materialization_equivalence: OnceCell<MaterializationEquivalenceVisitor<'db>>,
}

impl<'env, 'db> ApplyTypeMappingVisitor<'env, 'db> {
    fn new(env: &'env ProgramEnvironment<'db>) -> Self {
        Self {
            env,
            recursion_context: None,
            materialize_typevar_bounds_and_defaults: true,
            default: OnceCell::default(),
            top_materialization: OnceCell::default(),
            bottom_materialization: OnceCell::default(),
            top_specialization_materialization: OnceCell::default(),
            bottom_specialization_materialization: OnceCell::default(),
            promotion: OnceCell::default(),
            skip_promotion: OnceCell::default(),
            materialization_equivalence: OnceCell::default(),
        }
    }

    fn with_recursion_context(mut self, context: Option<&'env TypeRecursionContext<'db>>) -> Self {
        self.recursion_context = context;
        self
    }

    fn project_meta_type(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        match self.recursion_context {
            Some(context) => ty.to_meta_type_with_recursion(db, self.env, context),
            None => ty.to_meta_type(db, self.env),
        }
    }

    fn materialization_equivalence(&self) -> &MaterializationEquivalenceVisitor<'db> {
        self.materialization_equivalence
            .get_or_init(|| Rc::new(CycleDetector::new(true)))
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        func: impl FnOnce() -> Type<'db>,
    ) -> Type<'db> {
        self.transformer(type_mapping).visit_type(db, ty, func)
    }

    fn transformer_cell(
        &self,
        type_mapping: &TypeMapping<'_, 'db>,
    ) -> &OnceCell<Box<TypeTransformer<'db, ApplyTypeMappingTag>>> {
        match type_mapping {
            TypeMapping::Materialize(MaterializationKind::Top) => &self.top_materialization,
            TypeMapping::Materialize(MaterializationKind::Bottom) => &self.bottom_materialization,
            TypeMapping::ApplySpecializationWithMaterialization {
                materialization_kind: MaterializationKind::Top,
                ..
            } => &self.top_specialization_materialization,
            TypeMapping::ApplySpecializationWithMaterialization {
                materialization_kind: MaterializationKind::Bottom,
                ..
            } => &self.bottom_specialization_materialization,
            TypeMapping::Promote(PromotionMode::On, _) => &self.promotion,
            TypeMapping::Promote(PromotionMode::Off, _) => &self.skip_promotion,
            _ => &self.default,
        }
    }

    fn transformer(
        &self,
        type_mapping: &TypeMapping<'_, 'db>,
    ) -> &TypeTransformer<'db, ApplyTypeMappingTag> {
        self.transformer_cell(type_mapping)
            .get_or_init(Box::default)
    }

    fn is_equivalent_to_materialization(
        &self,
        db: &'db dyn Db,
        left: Type<'db>,
        right: Type<'db>,
    ) -> bool {
        self.materialization_equivalence()
            .visit(db, (left, right), || {
                left.is_equivalent_to_with_materialization_visitor(db, right, self)
            })
    }

    fn for_new_materialization_root(&self) -> Self {
        let materialization_equivalence = OnceCell::new();
        let was_empty =
            materialization_equivalence.set(Rc::clone(self.materialization_equivalence()));
        debug_assert!(was_empty.is_ok());

        Self {
            materialization_equivalence,
            recursion_context: self.recursion_context,
            materialize_typevar_bounds_and_defaults: self.materialize_typevar_bounds_and_defaults,
            ..Self::new(self.env)
        }
    }
}

/// A [`CycleDetector`] that is used in `find_legacy_typevars` methods.
pub(crate) type FindLegacyTypeVarsVisitor<'db> =
    CycleDetector<'db, FindLegacyTypeVars, Type<'db>, (), 3>;

#[derive(Debug)]
pub(crate) struct FindLegacyTypeVars;

/// A [`CycleDetector`] that is used in `visit_specialization` methods.
type SpecializationVisitor<'db> =
    CycleDetector<'db, VisitSpecialization, (Type<'db>, TypeVarVariance), (), 3>;
struct VisitSpecialization;

impl<'db> HasIdentity<'db> for (Type<'db>, TypeVarVariance) {
    type Id = (TypeIdentity<'db>, TypeVarVariance);

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        let (self_ty, self_variance) = self;
        let (other_ty, other_variance) = other;
        self_variance == other_variance && self_ty.may_share_type_identity(db, *other_ty)
    }

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        let (ty, variance) = self;
        (ty.to_type_identity(db), *variance)
    }
}

/// The standard-library `typing` module or its `typing_extensions` backport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum TypingModule {
    /// The standard-library `typing` module.
    Typing,
    /// The `typing_extensions` backport.
    TypingExtensions,
}

impl TypingModule {
    /// Return the module for a `TypedDict` special form, including a union of the special forms
    /// exported by `typing` and `typing_extensions`.
    fn from_typed_dict_type<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<Self> {
        match ty {
            Type::SpecialForm(SpecialFormType::TypedDict(module)) => Some(module),
            Type::Union(union) => Self::from_typed_dict_elements(union.elements(db)),
            _ => None,
        }
    }

    pub(in crate::types) fn from_typed_dict_elements(elements: &[Type<'_>]) -> Option<Self> {
        let mut elements = elements.iter();
        let Type::SpecialForm(SpecialFormType::TypedDict(module)) = elements.next()? else {
            return None;
        };
        elements.try_fold(*module, |module, element| {
            let Type::SpecialForm(SpecialFormType::TypedDict(element_module)) = element else {
                return None;
            };
            // `typing_extensions.TypedDict` always offers strictly more functionality than `typing.TypedDict`.
            // If any element is from `typing`, we therefore infer that the type is a `typing.TypedDict`,
            // since an operation on a union is only valid if the operation is valid on all elements in the
            // union.
            Some(match (module, element_module) {
                (Self::TypingExtensions, Self::TypingExtensions) => Self::TypingExtensions,
                _ => Self::Typing,
            })
        })
    }

    const fn from_type_alias_class(class: KnownClass) -> Option<Self> {
        match class {
            KnownClass::TypeAliasType => Some(Self::Typing),
            KnownClass::ExtensionsTypeAliasType => Some(Self::TypingExtensions),
            _ => None,
        }
    }

    const fn type_alias_class(self) -> KnownClass {
        match self {
            Self::Typing => KnownClass::TypeAliasType,
            Self::TypingExtensions => KnownClass::ExtensionsTypeAliasType,
        }
    }
}

/// Whether a type represents the upper or lower bound of a gradual type.
///
/// For generic specializations, this matters only if there is at least one invariant or constrained
/// type parameter. For example, we represent `Top[list[Any]]` as a `GenericAlias` with
/// `MaterializationKind` set to Top, which we denote as `Top[list[Any]]`.
/// A type `Top[list[T]]` includes all fully static list types `list[U]` where `U` is
/// a supertype of `Bottom[T]` and a subtype of `Top[T]`.
///
/// Similarly, there is `Bottom[list[Any]]`.
/// This type is harder to make sense of in a set-theoretic framework, but
/// it is a subtype of all materializations of `list[Any]`.
///
/// Recursive type aliases also retain their materialization kind so that materializing the alias
/// body preserves stable recursive references.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub enum MaterializationKind {
    Top,
    Bottom,
}

impl MaterializationKind {
    /// Flip the materialization type: `Top` becomes `Bottom` and vice versa.
    #[must_use]
    const fn flip(self) -> Self {
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
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum AttributeKind {
    DataDescriptor,
    NormalOrNonDataDescriptor,
}

impl AttributeKind {
    const fn is_data(self) -> bool {
        matches!(self, Self::DataDescriptor)
    }
}

/// An interned description of an implicit `__get__` call.
///
/// Member lookup carries this compact context through unions and fallbacks. Expression inference
/// reconstructs the concrete [`CallError`] if the invalid access remains after applying lookup
/// fallbacks and local assignment information.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct DescriptorGetCallContext<'db> {
    #[returns(copy)]
    descriptor_type: Type<'db>,
    #[returns(copy)]
    callable_type: Type<'db>,
    #[returns(copy)]
    instance: Option<Type<'db>>,
    #[returns(copy)]
    owner: Type<'db>,
}

impl get_size2::GetSize for DescriptorGetCallContext<'_> {}

impl<'db> DescriptorGetCallContext<'db> {
    /// Reconstructs the implicit call and returns its error if the call is still invalid.
    fn into_error(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<CallError<'db>> {
        let descriptor_type = self.descriptor_type(db);
        let instance = self.instance(db).unwrap_or_else(|| Type::none(db, env));
        let owner = self.owner(db);
        self.callable_type(db)
            .try_call(
                db,
                env,
                &CallArguments::positional([descriptor_type, instance, owner]),
            )
            .err()
    }
}

/// A call that determines a descriptor's result, including calls delegated to property getters.
/// Candidate signatures and actual arguments retain dispatch information even when a function
/// has been converted to `Callable`, or when the callable has no source declaration.
#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
struct DescriptorDispatch<'db> {
    #[returns(ref)]
    signatures: CallableSignature<'db>,
    #[returns(ref)]
    arguments: Box<[Type<'db>]>,
    #[returns(ref)]
    comparisons: Box<[Box<[DescriptorArgumentComparison<'db>]>]>,
    #[returns(ref)]
    selected_overloads: Box<[usize]>,
    failed: bool,
}

impl get_size2::GetSize for DescriptorDispatch<'_> {}

/// The effective obligation for a matched descriptor argument, after unpacked parameter matching.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
struct DescriptorArgumentComparison<'db> {
    argument_index: usize,
    argument_type: Type<'db>,
    parameter_type: Type<'db>,
}

#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
struct DescriptorDispatches<'db> {
    #[returns(ref)]
    elements: Box<[DescriptorDispatch<'db>]>,
}

impl get_size2::GetSize for DescriptorDispatches<'_> {}

/// Descriptor calls that produced a member.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue,
)]
struct DescriptorOrigin<'db> {
    dispatches: Option<DescriptorDispatches<'db>>,
    /// A failed call without callable candidates has no dispatch to observe.
    incomplete: bool,
    /// Whether an `Unknown` alternative in the returned value came from recursion recovery.
    /// This does not describe concrete callables with an unknown return annotation.
    return_contains_recursive_recovery: bool,
}

impl<'db> DescriptorOrigin<'db> {
    fn merge(self, db: &'db dyn Db, other: Self) -> Self {
        legacy_inline(self.merge_with(db, other, &call::bind::origin::InlineOriginEffects))
    }

    /// Discards recovery provenance when the effective result no longer includes `Unknown`.
    fn restrict_to_return_type(
        mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        return_type: Type<'db>,
    ) -> Self {
        if self.return_contains_recursive_recovery {
            let return_type = return_type.resolve_type_alias(db);
            let return_type = return_type
                .as_union_like(db)
                .map_or(return_type, |union| union.expand_aliases(db, env));
            self.return_contains_recursive_recovery = match return_type {
                Type::Union(union) => union.elements(db).iter().any(Type::is_unknown),
                ty => ty.is_unknown(),
            };
        }
        self
    }
}

/// The type and descriptor kind produced by an implicit `__get__` call.
#[derive(Clone, Debug, Copy, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct DescriptorGetResult<'db> {
    pub(crate) return_type: Type<'db>,
    origin: DescriptorOrigin<'db>,
    kind: AttributeKind,
}

/// A failed implicit descriptor call together with its recovery value.
#[derive(Clone, Debug, Copy, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct DescriptorGetError<'db> {
    fallback: DescriptorGetResult<'db>,
    context: DescriptorGetCallContext<'db>,
}

impl<'db> DescriptorGetError<'db> {
    /// Returns the descriptor's declared return type and kind despite the invalid call.
    pub(crate) const fn fallback(self) -> DescriptorGetResult<'db> {
        self.fallback
    }
}

fn descriptor_get_result<'db>(
    return_type: Type<'db>,
    origin: DescriptorOrigin<'db>,
    kind: AttributeKind,
    error: Option<DescriptorGetCallContext<'db>>,
) -> Result<Option<DescriptorGetResult<'db>>, DescriptorGetError<'db>> {
    let result = DescriptorGetResult {
        return_type,
        origin,
        kind,
    };
    match error {
        Some(context) => Err(DescriptorGetError {
            fallback: result,
            context,
        }),
        None => Ok(Some(result)),
    }
}

/// An operation that failed while resolving an attribute.
#[derive(Clone, Debug, Copy, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
enum MemberLookupErrorKind<'db> {
    DescriptorGet(DescriptorGetCallContext<'db>),

    /// An invalid fallback call, represented by its receiver and requested attribute name.
    ///
    /// Retaining only these arguments avoids storing call bindings in cached lookup results.
    GetAttr {
        receiver: Type<'db>,
        name: Type<'db>,
    },

    /// An invalid module-level `__getattr__` call, stored without its call bindings.
    ModuleGetAttr {
        callable: Type<'db>,
        name: Type<'db>,
    },

    /// An invalid attribute-interception call, represented by its receiver and attribute name.
    GetAttribute {
        receiver: Type<'db>,
        name: Type<'db>,
    },
}

/// A failed member lookup together with the member used to recover from the error.
#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
struct MemberLookupError<'db> {
    #[returns(copy)]
    fallback_member: ResolvedMember<'db>,
    #[returns(copy)]
    kind: MemberLookupErrorKind<'db>,
}

impl get_size2::GetSize for MemberLookupError<'_> {}

impl<'db> MemberLookupError<'db> {
    /// Reports the failed implicit call unless the lookup is shadowed or used for deletion.
    fn report_diagnostic(
        self,
        context: &InferContext<'db, '_>,
        object_type: Type<'db>,
        target: &ast::ExprAttribute,
        assigned_type: Option<Type<'db>>,
    ) {
        if matches!(target.ctx, ast::ExprContext::Del) {
            return;
        }

        let db = context.db();
        let env = context.program_environment();

        match self.kind(db) {
            MemberLookupErrorKind::DescriptorGet(call_context)
                if (assigned_type.is_none()
                    || call_context.descriptor_type(db).is_data_descriptor(db, env))
                    && let Some(failure) = call_context.into_error(db, env) =>
            {
                report_bad_dunder_get_call(
                    context,
                    &failure,
                    object_type,
                    call_context.descriptor_type(db),
                    target,
                );
            }
            kind @ (MemberLookupErrorKind::GetAttr { receiver, name }
            | MemberLookupErrorKind::GetAttribute { receiver, name }) => {
                let method = if matches!(kind, MemberLookupErrorKind::GetAttr { .. }) {
                    AttributeAccessMethod::GetAttr
                } else {
                    AttributeAccessMethod::GetAttribute
                };

                if method == AttributeAccessMethod::GetAttr && assigned_type.is_some() {
                    return;
                }

                if let Err(CallDunderError::CallError(kind, bindings, _)) = receiver
                    .try_call_dunder(
                        db,
                        env,
                        method.as_str(),
                        CallArguments::positional([name]),
                        TypeContext::default(),
                    )
                {
                    let failure = CallError(kind, bindings);
                    report_bad_attribute_access_call(
                        context,
                        &failure,
                        object_type,
                        target,
                        method,
                    );
                }
            }
            MemberLookupErrorKind::ModuleGetAttr { .. }
                if assigned_type.is_none()
                    && let Some(failure) = self.module_getattr_call_failure(db, env) =>
            {
                report_bad_attribute_access_call(
                    context,
                    &failure,
                    object_type,
                    target,
                    AttributeAccessMethod::GetAttr,
                );
            }
            MemberLookupErrorKind::DescriptorGet(_)
            | MemberLookupErrorKind::ModuleGetAttr { .. } => {}
        }
    }

    /// Reports a failed module `__getattr__` call on a `from` import.
    ///
    /// Imports defer this diagnostic until they have ruled out a real submodule:
    ///
    /// ```python
    /// from package import missing  # Calls package.__getattr__("missing").
    /// ```
    fn report_module_getattr_import_diagnostic(
        self,
        context: &InferContext<'db, '_>,
        module: ModuleLiteralType<'db>,
        target: &ast::Alias,
        name: &str,
    ) {
        if let Some(failure) =
            self.module_getattr_call_failure(context.db(), context.program_environment())
        {
            report_bad_import_call(context, &failure, module, target, name);
        }
    }

    /// Recreates a failed module `__getattr__` call without caching its call bindings.
    fn module_getattr_call_failure(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<CallError<'db>> {
        let MemberLookupErrorKind::ModuleGetAttr { callable, name } = self.kind(db) else {
            return None;
        };

        callable
            .try_call(db, env, &CallArguments::positional([name]))
            .err()
    }
}

/// A resolved member or an implicit-call error that retains its recovery value.
///
/// Unlike [`crate::place::LookupResult`], errors here describe failed attribute-access operations,
/// not undefined or possibly undefined places.
type MemberLookupResult<'db> = Result<ResolvedMember<'db>, MemberLookupError<'db>>;

/// A resolved member, retaining descriptor dispatch and deprecated accessors when present.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
enum ResolvedMember<'db> {
    /// A member requiring no additional metadata.
    Plain(PlaceAndQualifiers<'db>),
    /// Metadata is stored separately to keep ordinary lookups compact.
    WithMetadata(MemberMetadata<'db>),
}

/// Additional information about the descriptor operations that resolved a member.
#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
struct MemberMetadata<'db> {
    #[returns(copy)]
    member: PlaceAndQualifiers<'db>,
    #[returns(copy)]
    properties: Option<PropertyDeprecations<'db>>,
    #[returns(copy)]
    descriptor: DescriptorOrigin<'db>,
}

impl get_size2::GetSize for MemberMetadata<'_> {}

/// Deprecated property accessors retained independently of descriptor types. Distinct property
/// objects are disjoint types, but either can implement an attribute on an intersection.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct PropertyDeprecations<'db> {
    #[returns(ref)]
    getters: Box<[OverloadLiteral<'db>]>,
    #[returns(ref)]
    setters: Box<[OverloadLiteral<'db>]>,
    #[returns(ref)]
    deleters: Box<[OverloadLiteral<'db>]>,
}

impl get_size2::GetSize for PropertyDeprecations<'_> {}

impl<'db> PropertyDeprecations<'db> {
    fn functions(self, db: &'db dyn Db, access: ast::ExprContext) -> &'db [OverloadLiteral<'db>] {
        match access {
            ast::ExprContext::Load => self.getters(db),
            ast::ExprContext::Store => self.setters(db),
            ast::ExprContext::Del => self.deleters(db),
            ast::ExprContext::Invalid => &[],
        }
    }

    fn getters_only(self, db: &'db dyn Db) -> Self {
        Self::new(db, self.getters(db), [].as_slice(), [].as_slice())
    }

    /// Retain either alternative's deprecations: a union can invoke either accessor.
    fn union(self, db: &'db dyn Db, other: Self) -> Self {
        self.combine(db, other, false)
    }

    /// Retain deprecations only for access kinds deprecated in both alternatives. A
    /// non-deprecated getter can suppress read warnings without suppressing write warnings.
    fn intersection(self, db: &'db dyn Db, other: Self) -> Self {
        self.combine(db, other, true)
    }

    fn combine(self, db: &'db dyn Db, other: Self, intersection: bool) -> Self {
        let combine = |left: &[OverloadLiteral<'db>], right: &[OverloadLiteral<'db>]| {
            if intersection && (left.is_empty() || right.is_empty()) {
                Box::<[_]>::default()
            } else {
                left.iter().chain(right).copied().unique().collect()
            }
        };
        Self::new(
            db,
            combine(self.getters(db), other.getters(db)),
            combine(self.setters(db), other.setters(db)),
            combine(self.deleters(db), other.deleters(db)),
        )
    }
}

impl<'db> ResolvedMember<'db> {
    fn member(self, db: &'db dyn Db) -> PlaceAndQualifiers<'db> {
        match self {
            Self::Plain(member) => member,
            Self::WithMetadata(member) => member.member(db),
        }
    }

    fn deprecated_properties(self, db: &'db dyn Db) -> Option<PropertyDeprecations<'db>> {
        match self {
            Self::WithMetadata(member) => member.properties(db),
            Self::Plain(_) => None,
        }
    }

    fn with_metadata(
        db: &'db dyn Db,
        member: PlaceAndQualifiers<'db>,
        properties: Option<PropertyDeprecations<'db>>,
        descriptor: DescriptorOrigin<'db>,
    ) -> Self {
        if properties.is_none() && descriptor == DescriptorOrigin::default() {
            Self::Plain(member)
        } else {
            Self::WithMetadata(MemberMetadata::new(db, member, properties, descriptor))
        }
    }

    fn descriptor_origin(self, db: &'db dyn Db) -> DescriptorOrigin<'db> {
        match self {
            Self::Plain(_) => DescriptorOrigin::default(),
            Self::WithMetadata(member) => member.descriptor(db),
        }
    }

    /// Transform the member's value type without changing its property accessor deprecations.
    fn map_type(self, db: &'db dyn Db, f: impl FnOnce(Type<'db>) -> Type<'db>) -> Self {
        Self::with_metadata(
            db,
            self.member(db).map_type(f),
            self.deprecated_properties(db),
            self.descriptor_origin(db),
        )
    }
}

/// Combine accessor deprecations from alternative lookup paths. A non-deprecated path (`None`)
/// does not suppress deprecations from another possible path.
fn union_deprecated_properties<'db>(
    db: &'db dyn Db,
    left: Option<PropertyDeprecations<'db>>,
    right: Option<PropertyDeprecations<'db>>,
) -> Option<PropertyDeprecations<'db>> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.union(db, right)),
        _ => left.or(right),
    }
}

fn member_lookup_result<'db>(
    db: &'db dyn Db,
    member: PlaceAndQualifiers<'db>,
    error: Option<MemberLookupErrorKind<'db>>,
    properties: Option<PropertyDeprecations<'db>>,
) -> MemberLookupResult<'db> {
    member_lookup_result_with_origin(db, member, error, properties, DescriptorOrigin::default())
}

fn member_lookup_result_with_origin<'db>(
    db: &'db dyn Db,
    member: PlaceAndQualifiers<'db>,
    error: Option<MemberLookupErrorKind<'db>>,
    properties: Option<PropertyDeprecations<'db>>,
    descriptor: DescriptorOrigin<'db>,
) -> MemberLookupResult<'db> {
    let member = ResolvedMember::with_metadata(db, member, properties, descriptor);
    match error {
        Some(kind) => Err(MemberLookupError::new(db, member, kind)),
        None => Ok(member),
    }
}

fn map_member_lookup_type<'db>(
    db: &'db dyn Db,
    result: MemberLookupResult<'db>,
    f: impl FnOnce(Type<'db>) -> Type<'db>,
) -> MemberLookupResult<'db> {
    match result {
        Ok(member) => Ok(member.map_type(db, f)),
        Err(error) => Err(MemberLookupError::new(
            db,
            error.fallback_member(db).map_type(db, f),
            error.kind(db),
        )),
    }
}

fn distribute_member_lookup_over_bound_or_constraints<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    bound_or_constraints: TypeVarBoundOrConstraints<'db>,
    symbolic_receiver: Type<'db>,
    name: &str,
    policy: MemberLookupPolicy,
) -> MemberLookupResult<'db> {
    match bound_or_constraints {
        TypeVarBoundOrConstraints::UpperBound(bound) => bound
            .member_lookup_with_policy_and_receiver(db, env, name, policy, Some(symbolic_receiver)),
        TypeVarBoundOrConstraints::Constraints(constraints) => {
            let mut error = None;
            let mut properties = None;
            let mut descriptor = DescriptorOrigin::default();
            let member = constraints.map_with_boundness_and_qualifiers(db, env, |constraint| {
                let result = constraint.member_lookup_with_policy_and_receiver(
                    db,
                    env,
                    name,
                    policy,
                    Some(*constraint),
                );
                let result = map_member_lookup_type(db, result, |ty| match ty {
                    Type::BoundMethod(method) => Type::BoundMethod(
                        method.with_constrained_receiver(db, symbolic_receiver, *constraint),
                    ),
                    _ => ty,
                });
                error = error.or_else(|| result.err().map(|error| error.kind(db)));
                let member = result.unwrap_or_else(|error| error.fallback_member(db));
                properties =
                    union_deprecated_properties(db, properties, member.deprecated_properties(db));
                let origin = member.descriptor_origin(db);
                descriptor = descriptor.merge(db, origin);
                member.member(db)
            });
            member_lookup_result_with_origin(db, member, error, properties, descriptor)
        }
    }
}

fn member_lookup_or_fall_back_to<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    result: MemberLookupResult<'db>,
    fallback_fn: impl FnOnce() -> MemberLookupResult<'db>,
) -> MemberLookupResult<'db> {
    let resolved = result.unwrap_or_else(|error| error.fallback_member(db));
    let member = resolved.member(db);
    match member_fallback_decision(member.place) {
        MemberFallbackDecision::Missing => fallback_fn(),
        MemberFallbackDecision::Defined => result,
        MemberFallbackDecision::PossiblyUndefined => {
            let fallback = fallback_fn();
            let fallback_member = fallback.unwrap_or_else(|error| error.fallback_member(db));
            member_lookup_result_with_origin(
                db,
                member.or_fall_back_to(db, env, || fallback_member.member(db)),
                result
                    .err()
                    .map(|error| error.kind(db))
                    .or_else(|| fallback.err().map(|error| error.kind(db))),
                union_deprecated_properties(
                    db,
                    resolved.deprecated_properties(db),
                    fallback_member.deprecated_properties(db),
                ),
                resolved
                    .descriptor_origin(db)
                    .merge(db, fallback_member.descriptor_origin(db)),
            )
        }
    }
}

fn cycle_normalized_member_lookup<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    result: MemberLookupResult<'db>,
    previous: MemberLookupResult<'db>,
    cycle: &salsa::Cycle,
) -> MemberLookupResult<'db> {
    match member_lookup::normalization::member_cycle_normalized_sync(
        result,
        env,
        previous,
        cycle,
        member_lookup::normalization::MemberNormalizationFacts,
        &member_lookup::normalization::OrdinaryMemberNormalizationEffects { db },
    ) {
        Ok(result) => result,
        Err(error) => match error {},
    }
}

impl<'db> From<PlaceAndQualifiers<'db>> for MemberLookupResult<'db> {
    fn from(member: PlaceAndQualifiers<'db>) -> Self {
        Ok(ResolvedMember::Plain(member))
    }
}

impl<'db> From<Place<'db>> for MemberLookupResult<'db> {
    fn from(place: Place<'db>) -> Self {
        Ok(ResolvedMember::Plain(place.into()))
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

        /// Ignore members that are only available through a dynamic type or a divergent marker.
        ///
        /// This is used when detecting descriptors. An `Any` or `Unknown` base can provide any
        /// member, but that does not mean that every subclass should be treated as a descriptor.
        /// Likewise, a divergent marker from cyclic inference does not establish a concrete member.
        const REQUIRE_CONCRETE = 1 << 5;
    }
}

impl get_size2::GetSize for MemberLookupPolicy {}

impl MemberLookupPolicy {
    /// Only look up the attribute on the meta-type.
    ///
    /// If false - Look up the attribute on the meta-type, but fall back to attributes on the instance
    /// if the meta-type attribute is not found or if the meta-type attribute is not a data
    /// descriptor.
    const fn no_instance_fallback(self) -> bool {
        self.contains(Self::NO_INSTANCE_FALLBACK)
    }

    /// Exclude attributes defined on `object` when looking up attributes.
    const fn mro_no_object_fallback(self) -> bool {
        self.contains(Self::MRO_NO_OBJECT_FALLBACK)
    }

    /// Exclude attributes defined on `type` when looking up meta-class-attributes.
    const fn meta_class_no_type_fallback(self) -> bool {
        self.contains(Self::META_CLASS_NO_TYPE_FALLBACK)
    }

    /// Exclude attributes defined on `int` or `str` when looking up attributes.
    const fn mro_no_int_or_str_fallback(self) -> bool {
        self.contains(Self::MRO_NO_INT_OR_STR_LOOKUP)
    }

    /// Do not call `__getattr__` during member lookup.
    const fn no_getattr_lookup(self) -> bool {
        self.contains(Self::NO_GETATTR_LOOKUP)
    }

    /// Ignore members that are only available through a dynamic type.
    const fn require_concrete(self) -> bool {
        self.contains(Self::REQUIRE_CONCRETE)
    }
}

impl Default for MemberLookupPolicy {
    fn default() -> Self {
        Self::empty()
    }
}

/// The common key for class-member and instance-member lookup.
#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
struct MemberLookupKey<'db> {
    #[returns(copy)]
    program: Program<'db>,
    #[returns(copy)]
    ty: Type<'db>,
    #[returns(ref)]
    name: Name,
    #[returns(copy)]
    policy: MemberLookupPolicy,
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn class_member_lookup_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<ClassMemberWithPolicyInnerConfiguration> {
    class_member_with_policy_inner::fn_ingredient_(db, db.zalsa())
}

/// Exposes the query nested in [`Type::lookup_dunder_new`] to the source registry.
#[cfg(any(test, feature = "experimental-analysis"))]
#[derive(Debug)]
pub(in crate::types) struct LookupDunderNewQuery;

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) type LookupDunderNewConfiguration =
    <LookupDunderNewQuery as salsa::plumbing::TrackedFunctionConfiguration>::Configuration;

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn member_lookup_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<MemberLookupWithPolicyInnerConfiguration> {
    member_lookup_with_policy_inner::fn_ingredient_(db, db.zalsa())
}

#[salsa::tracked(configuration = (pub(in crate::types) ClassMemberWithPolicyInnerConfiguration), attempt = ReturnOnly,
    self_ty = Type<'db>,
    returns(copy),
    cycle_initial=|_, id, _| Place::bound(Type::divergent(id)).into(),
    cycle_fn=|db, cycle, previous: &PlaceAndQualifiers<'db>, member: PlaceAndQualifiers<'db>, key: MemberLookupKey<'db>| {
        member.cycle_normalized(db, &ProgramEnvironment::from_program(key.program(db)), *previous, cycle)
    },
    heap_size=ruff_memory_usage::heap_size
)]
fn class_member_with_policy_inner<'db>(
    db: &'db dyn Db,
    key: MemberLookupKey<'db>,
) -> PlaceAndQualifiers<'db> {
    let ty = key.ty(db);
    let name = key.name(db);
    let policy = key.policy(db);
    let program = key.program(db);
    let env = &ProgramEnvironment::from_program(program);

    tracing::trace!("class_member: {}.{}", ty.display(db, env), name);
    match member_lookup::class_dispatch::class_member_dispatch_sync(
        ty,
        name,
        policy,
        member_lookup::class_dispatch::ClassMemberDispatchFacts,
        &member_lookup::class_dispatch::OrdinaryClassMemberDispatch { db, env },
    ) {
        Ok(member) => member,
        Err(never) => match never {},
    }
}

#[salsa::tracked(configuration = (pub(in crate::types) MemberLookupWithPolicyInnerConfiguration), attempt = ReturnOnly,
    returns(copy),
    cycle_initial=|_, id, _| Place::bound(Type::divergent(id)).into(),
    cycle_fn=|db, cycle, previous: &MemberLookupResult<'db>, member: MemberLookupResult<'db>, key: MemberLookupKey<'db>| {
        cycle_normalized_member_lookup(db, &ProgramEnvironment::from_program(key.program(db)), member, *previous, cycle)
    },
    heap_size=ruff_memory_usage::heap_size
)]
fn member_lookup_with_policy_inner<'db>(
    db: &'db dyn Db,
    key: MemberLookupKey<'db>,
) -> MemberLookupResult<'db> {
    member_lookup_with_policy_impl(db, key, None, None)
}

fn member_lookup_with_policy_impl<'db>(
    db: &'db dyn Db,
    key: MemberLookupKey<'db>,
    receiver: Option<Type<'db>>,
    recursion_guard: Option<&CallableRecursionGuard<'db>>,
) -> MemberLookupResult<'db> {
    let env = ProgramEnvironment::from_program(key.program(db));
    tracing::trace!(
        "member_lookup_with_policy: {}.{}",
        key.ty(db).display(db, &env),
        key.name(db)
    );
    match member_lookup::general::member_lookup_dispatch_sync(
        key,
        receiver,
        member_lookup::general::GeneralMemberFacts,
        &member_lookup::general::InlineGeneralMemberEffects {
            db,
            env: &env,
            recursion_guard,
            receiver_lookup: None,
        },
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}

/// Meta data for `Type::Todo`, which represents a known limitation in ty.
#[cfg(debug_assertions)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct TodoType(&'static str);

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

/// The role a function definition plays in a property's descriptor protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyAccessorRole {
    /// `@property def x(self)` — runs on read.
    Getter,
    /// `@x.setter def x(self, value)` — runs on write.
    Setter,
    /// `@x.deleter def x(self)` — runs on `del`.
    Deleter,
}

/// The nominal class of a precise property. Known classes remain lazy so synthesized properties
/// do not need to resolve typeshed just to record their class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum PropertyInstanceClass<'db> {
    Builtin,
    Enum,
    Subclass(ClassType<'db>),
}

impl<'db> PropertyInstanceClass<'db> {
    fn from_class(db: &'db dyn Db, class: ClassType<'db>) -> Self {
        match class.known(db) {
            Some(KnownClass::Property) => Self::Builtin,
            Some(KnownClass::EnumProperty) => Self::Enum,
            _ => Self::Subclass(class),
        }
    }

    fn to_class_literal(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match member_lookup::class_dispatch::property_class_literal_sync(
            self,
            &member_lookup::class_dispatch::OrdinaryClassMemberDispatch { db, env },
        ) {
            Ok(class) => class,
            Err(never) => match never {},
        }
    }

    fn to_instance(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match member_lookup::class_dispatch::property_class_instance_sync(
            self,
            &member_lookup::class_dispatch::OrdinaryClassMemberDispatch { db, env },
        ) {
            Ok(instance) => instance,
            Err(never) => match never {},
        }
    }
}

/// Identifies the actual implementation, rather than a method with the same name on a subclass.
fn is_property_method<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    function: FunctionType<'db>,
) -> bool {
    let class = match file_to_module(db, function.program_file(db).resolver_file(db))
        .and_then(|module| module.known(db))
    {
        Some(KnownModule::Builtins) => KnownClass::Property,
        Some(KnownModule::Enum | KnownModule::Types) => KnownClass::EnumProperty,
        _ => return false,
    };

    class
        .try_to_class_literal(db, env)
        .and_then(|class| {
            ClassLiteral::Static(class)
                .class_member(db, env, function.name(db), MemberLookupPolicy::default())
                .place
                .ignore_possibly_undefined()
        })
        .and_then(Type::as_function_literal)
        // Comparing literals avoids the cross-module AST dependency of `FunctionType::definition`.
        .is_some_and(|original| original.literal(db) == function.literal(db))
}

/// Recognizes inherited property descriptor methods without replacing subclass overrides.
fn property_wrapper_descriptor<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    name: &str,
    member: Type<'db>,
) -> Type<'db> {
    let Some(wrapper) = property_wrapper_kind(name) else {
        return member;
    };
    if member
        .as_function_literal()
        .is_some_and(|function| is_property_method(db, env, function))
    {
        Type::WrapperDescriptor(wrapper)
    } else {
        member
    }
}

/// Source methods for property accessors, including accessors replaced by decorators.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue,
)]
pub struct PropertyAccessorDefinitions<'db> {
    getter: Option<Definition<'db>>,
    setter: Option<Definition<'db>>,
    deleter: Option<Definition<'db>>,
}

/// Represents a property with known accessors and the standard descriptor behavior.
#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct PropertyInstanceType<'db> {
    #[returns(copy)]
    pub getter: Option<Type<'db>>,
    #[returns(copy)]
    pub setter: Option<Type<'db>>,
    #[returns(copy)]
    pub deleter: Option<Type<'db>>,
    #[returns(copy)]
    instance_class: PropertyInstanceClass<'db>,
    /// Source definitions survive decorators that replace accessors with callable objects.
    #[returns(copy)]
    accessor_definitions: PropertyAccessorDefinitions<'db>,
}

fn walk_property_instance_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    property: PropertyInstanceType<'db>,
    visitor: &V,
) {
    if let PropertyInstanceClass::Subclass(class) = property.instance_class(db) {
        visitor.visit_type(db, class.into());
    }
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

#[cfg(feature = "experimental-analysis")]
impl FiniteInternedConfiguration for PropertyInstanceType<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // The fixed charge covers three accessor types, the class identity, and three
        // source-definition identities. Accessor types can also contain inline payload
        // bytes, which hashing and equality must visit.
        let mut work = 7usize;
        for accessor in [fields.0, fields.1, fields.2].into_iter().flatten() {
            work = work.checked_add(accessor.inline_payload_bytes())?;
        }
        Some(work)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for PropertyInstanceType<'_> {}

impl<'db> PropertyInstanceType<'db> {
    fn new(
        db: &'db dyn Db,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
    ) -> Self {
        Self::new_internal(
            db,
            getter,
            setter,
            deleter,
            PropertyInstanceClass::Builtin,
            PropertyAccessorDefinitions::default(),
        )
    }

    fn new_with_class(
        db: &'db dyn Db,
        class: ClassType<'db>,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
    ) -> Self {
        Self::new_internal(
            db,
            getter,
            setter,
            deleter,
            PropertyInstanceClass::from_class(db, class),
            PropertyAccessorDefinitions::default(),
        )
    }

    fn with_accessors(
        self,
        db: &'db dyn Db,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
    ) -> Self {
        let previous = self.accessor_definitions(db);
        let definitions = PropertyAccessorDefinitions {
            getter: previous.getter.filter(|_| getter == self.getter(db)),
            setter: previous.setter.filter(|_| setter == self.setter(db)),
            deleter: previous.deleter.filter(|_| deleter == self.deleter(db)),
        };
        Self::new_internal(
            db,
            getter,
            setter,
            deleter,
            self.instance_class(db),
            definitions,
        )
    }

    /// Records the source of an accessor supplied by a method decorator.
    fn with_accessor_definition(
        self,
        db: &'db dyn Db,
        decorator: Type<'db>,
        accessor: Type<'db>,
        definition: Definition<'db>,
    ) -> Self {
        property_provenance::with_accessor_definition_sync(
            self,
            decorator,
            accessor,
            definition,
            property_provenance::PropertyProvenanceFacts,
            &property_provenance::OrdinaryPropertyProvenanceEffects { db },
        )
        .unwrap_or_else(|never| match never {})
    }

    /// Pairs retained accessor types with their source methods, independently of decorators.
    fn accessors_with_functions(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = (Type<'db>, FunctionType<'db>)> {
        let definitions = self.accessor_definitions(db);
        [
            (self.getter(db), definitions.getter),
            (self.setter(db), definitions.setter),
            (self.deleter(db), definitions.deleter),
        ]
        .into_iter()
        .filter_map(move |(accessor, definition)| {
            let accessor = accessor?;
            let function = accessor.as_function_literal().or_else(|| {
                definition.and_then(|definition| {
                    infer_definition_types(db, definition).function_type(definition)
                })
            })?;
            Some((accessor, function))
        })
    }

    fn instance_fallback(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        self.instance_class(db).to_instance(db, env)
    }

    /// Returns the [`PropertyAccessorRole`] that `def` plays in this property, or `None` when
    /// `def` is not one of this property's accessors.
    ///
    /// Each accessor slot is a function-literal `Type`; an accessor may be overloaded, so a
    /// definition is matched against every overload signature and the implementation, not just
    /// the implementation's definition.
    pub fn accessor_role(
        self,
        db: &'db dyn Db,
        def: Definition<'db>,
    ) -> Option<PropertyAccessorRole> {
        let slot_matches = |accessor: Option<Type<'db>>| -> bool {
            accessor
                .and_then(Type::as_function_literal)
                .into_iter()
                .flat_map(|function| function.iter_overloads_and_implementation(db))
                .filter_map(|overload| overload.signature(db).definition())
                .any(|accessor_def| accessor_def == def)
        };

        if slot_matches(self.getter(db)) {
            Some(PropertyAccessorRole::Getter)
        } else if slot_matches(self.setter(db)) {
            Some(PropertyAccessorRole::Setter)
        } else if slot_matches(self.deleter(db)) {
            Some(PropertyAccessorRole::Deleter)
        } else {
            None
        }
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
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
        let instance_class = match self.instance_class(db) {
            PropertyInstanceClass::Subclass(class) => PropertyInstanceClass::Subclass(
                class.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            ),
            class => class,
        };
        Self::new_internal(
            db,
            getter,
            setter,
            deleter,
            instance_class,
            self.accessor_definitions(db),
        )
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let getter = match self.getter(db) {
            Some(ty) if nested => Some(ty.recursive_type_normalized_impl(db, env, div, true)?),
            Some(ty) => Some(
                ty.recursive_type_normalized_impl(db, env, div, true)
                    .unwrap_or(div),
            ),
            None => None,
        };
        let setter = match self.setter(db) {
            Some(ty) if nested => Some(ty.recursive_type_normalized_impl(db, env, div, true)?),
            Some(ty) => Some(
                ty.recursive_type_normalized_impl(db, env, div, true)
                    .unwrap_or(div),
            ),
            None => None,
        };
        let deleter = match self.deleter(db) {
            Some(ty) if nested => Some(ty.recursive_type_normalized_impl(db, env, div, true)?),
            Some(ty) => Some(
                ty.recursive_type_normalized_impl(db, env, div, true)
                    .unwrap_or(div),
            ),
            None => None,
        };
        let instance_class = match self.instance_class(db) {
            PropertyInstanceClass::Subclass(class) => PropertyInstanceClass::Subclass(
                class.recursive_type_normalized_impl(db, env, div, nested)?,
            ),
            class => class,
        };
        Some(Self::new_internal(
            db,
            getter,
            setter,
            deleter,
            instance_class,
            self.accessor_definitions(db),
        ))
    }

    fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        if let PropertyInstanceClass::Subclass(class) = self.instance_class(db) {
            class.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
        }
        if let Some(ty) = self.getter(db) {
            ty.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
        }
        if let Some(ty) = self.setter(db) {
            ty.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
        }
        if let Some(ty) = self.deleter(db) {
            ty.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
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
#[salsa::interned(debug, field_view = read_fields, field_requests = field_requests, heap_size=ruff_memory_usage::heap_size)]
pub struct DataclassParams<'db> {
    #[returns(copy)]
    flags: DataclassFlags,

    #[returns(deref)]
    field_specifiers: Box<[Type<'db>]>,
}

impl get_size2::GetSize for DataclassParams<'_> {}

impl<'db> DataclassParams<'db> {
    fn default_params(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Self {
        Self::from_flags(db, env, DataclassFlags::default())
    }

    fn from_flags(db: &'db dyn Db, env: &ProgramEnvironment<'db>, flags: DataclassFlags) -> Self {
        let dataclasses_field = known_module_symbol(db, env, KnownModule::Dataclasses, "field")
            .place
            .ignore_possibly_undefined()
            .unwrap_or_else(Type::unknown);

        Self::new(db, flags, [dataclasses_field].as_slice())
    }

    fn from_transformer_params(db: &'db dyn Db, params: DataclassTransformerParams<'db>) -> Self {
        Self::new(
            db,
            DataclassFlags::from(params.flags(db)),
            params.field_specifiers(db),
        )
    }

    fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let field_specifiers = self
            .field_specifiers(db)
            .iter()
            .map(|ty| {
                let ty = ty.recursive_type_normalized_impl(db, env, div, true);
                if nested { ty } else { Some(ty.unwrap_or(div)) }
            })
            .collect::<Option<Box<_>>>()?;

        Some(Self::new(db, self.flags(db), field_specifiers))
    }
}

/// Representation of a type: a set of possible values at runtime.
///
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum Type<'db> {
    /// The dynamic type: a statically unknown set of values
    Dynamic(DynamicType<'db>),
    /// A cycle marker used during recursive type inference.
    Divergent(DivergentType),
    /// A recursive type whose references are bound by its body.
    /// See the module documentation in `recursive.rs` for details.
    Recursive(RecursiveType<'db>),
    /// A variable in a recursive type body, with no standalone type semantics.
    ///
    /// This is syntax, not a dynamic type or an inference variable. It must remain
    /// under its binder during structural transformations. Semantic operations,
    /// including assignability, must only receive types with no unbound references.
    RecursiveVar(RecursiveVar<'db>),
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
    /// A Python property with specialized getter, setter, and deleter types.
    PropertyInstance(PropertyInstanceType<'db>),
    /// An interpreter-created descriptor for an instance slot.
    SlotDescriptor(SlotDescriptorType<'db>),
    /// The set of objects in any of the types in the union
    Union(UnionType<'db>),
    /// The set of objects in all of the types in the intersection
    Intersection(IntersectionType<'db>),
    /// An enum instance with one or more canonical enum members excluded.
    EnumComplement(EnumComplementType<'db>),
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
    /// The set of type-form objects that represent a type assignable to the argument.
    TypeForm(TypeFormType<'db>),
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

/// The result of discarding disjoint elements from a union.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum DiscardDisjointUnionElementsResult<'db> {
    /// The remaining type, or the unchanged input if it is not a union.
    Retained(Type<'db>),
    /// Every union element is disjoint from the target.
    AllDisjoint,
}

impl<'db> DiscardDisjointUnionElementsResult<'db> {
    /// Returns the retained type, or `Never` if every union element was disjoint.
    fn or_never(self) -> Type<'db> {
        match self {
            Self::Retained(ty) => ty,
            Self::AllDisjoint => Type::Never,
        }
    }

    /// Returns the retained type, or `original` if every union element was disjoint.
    fn unless_all_disjoint(self, original: Type<'db>) -> Type<'db> {
        match self {
            Self::Retained(ty) => ty,
            Self::AllDisjoint => original,
        }
    }
}

/// The result of projecting class-object types into the corresponding instance types.
///
/// An exact projection preserves all class-object constraints relevant to a `type[T]` relation;
/// where `to_meta_type` is a faithful inverse, it round-trips semantically. An over-approximation
/// may discard class-object constraints and cannot establish a subtype relation in target
/// position.
///
/// For example, given these Python classes:
///
/// ```py
/// class Base: ...
/// class Child(Base): ...
/// ```
///
/// `type[Base]` projects to `Base` exactly: both admit `Child`. In contrast,
/// `TypeOf[Base]` (the type of the expression `Base`) admits only the `Base` class object, but
/// also projects to `Base`, which admits `Child` instances. That projection is an
/// over-approximation.
#[derive(Copy, Clone, Debug)]
pub(crate) enum InstanceProjection<T> {
    Exact(T),
    OverApproximation(T),
}

impl<T> InstanceProjection<T> {
    const fn is_exact(&self) -> bool {
        matches!(self, Self::Exact(_))
    }

    fn into_inner(self) -> T {
        match self {
            Self::Exact(value) | Self::OverApproximation(value) => value,
        }
    }

    fn map<U>(self, transform: impl FnOnce(T) -> U) -> InstanceProjection<U> {
        match self {
            Self::Exact(value) => InstanceProjection::Exact(transform(value)),
            Self::OverApproximation(value) => {
                InstanceProjection::OverApproximation(transform(value))
            }
        }
    }

    const fn new(value: T, is_exact: bool) -> Self {
        if is_exact {
            Self::Exact(value)
        } else {
            Self::OverApproximation(value)
        }
    }
}

/// An ordered pair of types and their Python version shared by type-relation and set-theoretic
/// queries.
#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
struct TypePair<'db> {
    #[returns(copy)]
    program: Program<'db>,
    #[returns(copy)]
    first: Type<'db>,
    #[returns(copy)]
    second: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypePair<'_> {}

// Hashing and equality inspect only the interned identity.
#[cfg(any(test, feature = "experimental-analysis"))]
impl salsa::plumbing::function::FixedQueryFields for TypePair<'_> {}

#[cfg(any(test, feature = "experimental-analysis"))]
impl FiniteInternedConfiguration for TypePair<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // Both types contain only scalar handles and inline payload bytes. Hashing and equality
        // do not resolve their referenced semantic data.
        3usize
            .checked_add(fields.1.inline_payload_bytes())?
            .checked_add(fields.2.inline_payload_bytes())
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) type TypePairMemoSchema<'db, A, E, R, P, U, I> =
    salsa::execution_probe::PassiveMemoGroup<
        (
            salsa::execution_probe::PassiveMemo<
                'db,
                crate::types::TypePair<'static>,
                A,
                crate::types::constraints::OwnedConstraintSetProfile,
            >,
            salsa::execution_probe::PassiveMemo<
                'db,
                crate::types::TypePair<'static>,
                E,
                crate::types::constraints::OwnedConstraintSetProfile,
            >,
        ),
        salsa::execution_probe::PassiveMemoGroup<
            (
                salsa::execution_probe::PassiveMemo<
                    'db,
                    crate::types::TypePair<'static>,
                    R,
                    salsa::execution_probe::FixedQueryKeyProfile,
                >,
                salsa::execution_probe::PassiveMemo<
                    'db,
                    crate::types::TypePair<'static>,
                    P,
                    salsa::execution_probe::FixedQueryKeyProfile,
                >,
            ),
            (
                salsa::execution_probe::PassiveMemo<
                    'db,
                    crate::types::TypePair<'static>,
                    U,
                    salsa::execution_probe::FixedQueryKeyProfile,
                >,
                salsa::execution_probe::PassiveMemo<
                    'db,
                    crate::types::TypePair<'static>,
                    I,
                    salsa::execution_probe::FixedQueryKeyProfile,
                >,
            ),
        >,
    >;

#[cfg(any(test, feature = "experimental-analysis"))]
fn register_type_pair_values<'run, 'db: 'run, A, E, R, P, U, I>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
    assignability: &'db IngredientImpl<A>,
    equivalence: &'db IngredientImpl<E>,
    redundancy: &'db IngredientImpl<R>,
    possible_assignability: &'db IngredientImpl<P>,
    union: &'db IngredientImpl<U>,
    intersection: &'db IngredientImpl<I>,
) -> RunResult<
    salsa::execution_probe::InternedValues<
        'db,
        crate::types::TypePair<'static>,
        TypePairMemoSchema<'db, A, E, R, P, U, I>,
    >,
>
where
    A: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypePair<'a>,
            SalsaStruct<'a> = TypePair<'a>,
            Output<'a> = OwnedConstraintSet<'a>,
        >,
    E: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypePair<'a>,
            SalsaStruct<'a> = TypePair<'a>,
            Output<'a> = OwnedConstraintSet<'a>,
        >,
    R: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypePair<'a>,
            SalsaStruct<'a> = TypePair<'a>,
            Output<'a> = bool,
        >,
    P: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypePair<'a>,
            SalsaStruct<'a> = TypePair<'a>,
            Output<'a> = bool,
        >,
    U: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypePair<'a>,
            SalsaStruct<'a> = TypePair<'a>,
            Output<'a> = Type<'a>,
        >,
    I: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = TypePair<'a>,
            SalsaStruct<'a> = TypePair<'a>,
            Output<'a> = Type<'a>,
        >,
{
    let owner = TypePair::ingredient(db.zalsa());
    let assignability =
        registry.passive_memo::<_, _, OwnedConstraintSetProfile>(owner, assignability)?;
    let equivalence =
        registry.passive_memo::<_, _, OwnedConstraintSetProfile>(owner, equivalence)?;
    let redundancy = registry.passive_memo::<_, _, CopyMemoProfile>(owner, redundancy)?;
    let possible_assignability =
        registry.passive_memo::<_, _, CopyMemoProfile>(owner, possible_assignability)?;
    let union = registry.passive_memo::<_, _, CopyMemoProfile>(owner, union)?;
    let intersection = registry.passive_memo::<_, _, CopyMemoProfile>(owner, intersection)?;
    registry.finite_interned_values_with_memos(
        owner,
        PassiveMemoGroup::new(
            (assignability, equivalence),
            PassiveMemoGroup::new((redundancy, possible_assignability), (union, intersection)),
        ),
    )
}

#[cfg(test)]
mod type_pair_registration_tests {
    use salsa::attempt_probe::{AttemptOutcome, try_with_attempt};
    use salsa::execution_probe::{ExecutionAdmission, ExecutionWork, RunError};
    use salsa::plumbing::ZalsaDatabase;

    use super::*;
    use crate::db::tests::setup_db;

    struct Admission;

    impl ExecutionAdmission for Admission {
        fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
            Ok(())
        }
    }

    #[test]
    fn type_pair_schema_covers_six_real_memos_without_running_them() -> anyhow::Result<()> {
        let db = setup_db();
        let mut reader = db.clone();
        let program = db.program_environment().program(&db);
        let owner = TypePair::ingredient(db.zalsa());
        let assignability = relation::owned_assignability_ingredient(&db);
        let equivalence = relation::owned_equivalence_ingredient(&db);
        let redundancy = relation::redundancy_ingredient(&db);
        let possible = constraints::possible_assignability_ingredient(&db);
        let union = set_theoretic::union_from_two_elements_ingredient(&db);
        let intersection = set_theoretic::intersection_from_two_elements_ingredient(&db);
        let admission = Admission;
        reader.clear_salsa_events();
        let outcome = try_with_attempt(&db, 100_000, || {
            let mut registry = RegistryBuilder::new(&db, &admission)?;
            let missing =
                registry.passive_memo::<_, _, OwnedConstraintSetProfile>(owner, assignability)?;
            assert!(matches!(
                registry.finite_interned_values_with_memos(owner, (missing,)),
                Err(RunError::Contract(
                    "finite interned value memo mapping is unsupported"
                ))
            ));
            let values = register_type_pair_values(
                &db,
                &mut registry,
                assignability,
                equivalence,
                redundancy,
                possible,
                union,
                intersection,
            )?;
            registry.seal()?.run(|endpoint| async move {
                let first = endpoint
                    .intern_value(&values, (program, Type::Never, Type::unknown()))
                    .await;
                let equal = endpoint
                    .intern_value(&values, (program, Type::Never, Type::unknown()))
                    .await;
                let reversed = endpoint
                    .intern_value(&values, (program, Type::unknown(), Type::Never))
                    .await;
                Ok((first, equal, reversed))
            })
        });
        let Ok(AttemptOutcome::Complete(Ok((first, equal, reversed)))) = outcome else {
            anyhow::bail!("complete TypePair schema should admit canonical interning: {outcome:?}");
        };
        assert_eq!(first, equal);
        assert_ne!(first, reversed);
        assert!(
            reader
                .take_salsa_events()
                .iter()
                .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
        );
        assert_eq!(
            first,
            TypePair::new(&db, program, Type::Never, Type::unknown())
        );
        assert_eq!(
            reversed,
            TypePair::new(&db, program, Type::unknown(), Type::Never)
        );
        let fields = first.read_fields(salsa::FieldReads::new(&db));
        assert_eq!(*fields.program(), program);
        assert_eq!(*fields.first(), Type::Never);
        assert_eq!(*fields.second(), Type::unknown());
        Ok(())
    }
}

/// Helper for `recursive_type_normalized_impl` for `TypeGuardLike` types.
fn recursive_type_normalize_type_guard_like<'db, T: TypeGuardLike<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    guard: T,
    div: Type<'db>,
    nested: bool,
) -> Option<Type<'db>> {
    let ty = if nested {
        guard
            .type_argument(db)
            .recursive_type_normalized_impl(db, env, div, true)?
    } else {
        guard
            .type_argument(db)
            .recursive_type_normalized_impl(db, env, div, true)
            .unwrap_or(div)
    };
    Some(guard.with_type(db, ty))
}

/// Whether generator-type extraction supplies defaults for iterator annotations.
///
/// `Iterator[T]` and `AsyncIterator[T]` constrain yielded values but do not declare
/// send or return types. Defaults used to check a generator body do not describe
/// an arbitrary iterator's termination value or establish a send requirement.
#[derive(Clone, Copy)]
enum GeneratorTypeMode {
    /// Extract parameters exposed by `Generator` or `AsyncGenerator`, without
    /// supplying defaults for plain iterators.
    ///
    /// Use this when inferring a delegated iterator's `yield from` result or
    /// determining whether an outer generator annotation declares a send type.
    /// An `Iterator[T]` can terminate with `StopIteration(42)`, so its annotation
    /// does not imply that the `yield from` result is `None`.
    GeneratorOnly,
    /// Also recognize `Iterator[T]` and `AsyncIterator[T]`, using `T` as the yield
    /// type and `None` as both the send and return types.
    ///
    /// These defaults support inference of `yield` expressions and validation of
    /// `yield` and `return` statements in generator bodies. Return-type extraction
    /// also uses this mode, including when inferring `await` expressions.
    IteratorDefaults,
}

#[derive(Debug, Clone, Copy)]
#[expect(clippy::struct_field_names)]
struct GeneratorTypes<'db> {
    yield_ty: Option<Type<'db>>,
    send_ty: Option<Type<'db>>,
    return_ty: Option<Type<'db>>,
}

impl<'db> GeneratorTypes<'db> {
    /// Apply a generator's materialization with the variance of each operation.
    fn materialize(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        kind: MaterializationKind,
    ) -> Self {
        let visitor = ApplyTypeMappingVisitor::new(env);
        Self {
            yield_ty: self.yield_ty.map(|ty| ty.materialize(db, kind, &visitor)),
            send_ty: self
                .send_ty
                .map(|ty| ty.materialize(db, kind.flip(), &visitor)),
            return_ty: self.return_ty.map(|ty| ty.materialize(db, kind, &visitor)),
        }
    }
}

fn object_type_form(db: &dyn Db) -> Type<'_> {
    TypeFormType::from_type_expression(db, Type::object())
}

#[salsa::tracked]
impl<'db> Type<'db> {
    /// Bytes visited by hashing or comparing this Type's inline variable-size payload.
    /// Interned handles do not traverse their referenced fields at this boundary.
    #[cfg(debug_assertions)]
    pub(crate) fn inline_payload_bytes(self) -> usize {
        match self {
            Type::Dynamic(DynamicType::Todo(todo)) => todo.0.len(),
            Type::SubclassOf(class) => match class.subclass_of() {
                SubclassOfInner::Dynamic(DynamicType::Todo(todo)) => todo.0.len(),
                _ => 0,
            },
            _ => 0,
        }
    }

    #[cfg(not(debug_assertions))]
    pub(crate) const fn inline_payload_bytes(self) -> usize {
        0
    }

    pub(crate) const fn any() -> Self {
        Self::Dynamic(DynamicType::Any)
    }

    pub const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub(crate) fn divergent(id: salsa::Id) -> Self {
        Self::Divergent(DivergentType::new(id))
    }

    /// Returns a divergent marker for a cycle in type alias inference.
    fn divergent_alias(id: salsa::Id) -> Self {
        Self::Divergent(DivergentType {
            flags: DivergentFlags::FROM_TYPE_ALIAS,
            ..DivergentType::new(id)
        })
    }

    const fn is_divergent(&self) -> bool {
        matches!(self, Type::Divergent(_))
    }

    const fn as_divergent(self) -> Option<DivergentType> {
        match self {
            Type::Divergent(divergent) => Some(divergent),
            _ => None,
        }
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

        Some(negation::NegationFacts.negated_divergent(divergent))
    }

    fn is_fully_static(self, db: &'db dyn Db, env: &ProgramEnvironment) -> bool {
        dynamic_content(db, env, self).is_absent()
    }

    const fn as_intersection(self) -> Option<IntersectionType<'db>> {
        match self {
            Type::Intersection(intersection) => Some(intersection),
            _ => None,
        }
    }

    pub const fn is_unknown(&self) -> bool {
        matches!(
            self,
            Type::Dynamic(
                DynamicType::Unknown
                    | DynamicType::UnknownGeneric(_)
                    | DynamicType::UnknownLambdaParameter
                    | DynamicType::AmbiguousOverload
            )
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
    fn contains_self(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        match self.try_contains_self(db, env, &mut visitor::Unrestricted) {
            Ok(found) => found,
            Err(error) => match error {},
        }
    }

    fn try_contains_self<C: visitor::SearchControl>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        control: &mut C,
    ) -> Result<bool, C::Error> {
        member_lookup::self_binding::contains_self_sync(
            self,
            &mut member_lookup::self_binding::InlineMemberSelfBindingEffects::new(db, env, control),
        )
    }

    /// Returns `true` if this type supports eager `Self` binding via `bind_self_typevars`.
    ///
    /// `FunctionLiteral`, `BoundMethod`, and function-like `Callable` types return `false`
    /// because their `Self` binding is deferred to call time via the signature binding path.
    fn supports_self_binding<C: visitor::SearchControl>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        control: &mut C,
    ) -> Result<bool, C::Error> {
        member_lookup::self_binding::supports_self_binding_sync(
            *self,
            &mut member_lookup::self_binding::InlineMemberSelfBindingEffects::new(db, env, control),
        )
    }

    /// Bind `Self` type variables in this type to a concrete self type.
    ///
    /// Uses MRO-based matching: a `Self` typevar is only bound if its owner class
    /// is in the MRO of the self type's class.
    ///
    /// Types that defer `Self` binding to call time (functions, bound methods, function-like
    /// callables) are skipped; see `supports_self_binding`.
    fn bind_self_typevars(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        self_type: Type<'db>,
    ) -> Self {
        match self.try_bind_self_typevars(
            db,
            env,
            self_type,
            &mut visitor::Unrestricted,
            |ty, receiver| Ok(ty.bind_self_typevars_after_search(db, env, receiver)),
        ) {
            Ok(bound) => bound,
            Err(error) => match error {},
        }
    }

    fn try_bind_self_typevars<C: visitor::SearchControl>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        self_type: Type<'db>,
        control: &mut C,
        map: impl FnOnce(Self, Self) -> Result<Self, C::Error>,
    ) -> Result<Self, C::Error> {
        if !self.supports_self_binding(db, env, control)? {
            return Ok(self);
        }
        map(self, self_type)
    }

    fn bind_self_typevars_after_search(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        self_type: Type<'db>,
    ) -> Self {
        self.apply_type_mapping(
            db,
            env,
            &TypeMapping::BindSelf(SelfBinding::new(db, env, self_type, None)),
            TypeContext::default(),
        )
    }

    /// Returns `true` if `self` is [`Type::Callable`].
    const fn is_callable_type(&self) -> bool {
        matches!(self, Type::Callable(..))
    }

    /// Returns `true` if `self` is [`Type::ProtocolInstance`].
    const fn is_protocol_instance(&self) -> bool {
        matches!(self, Type::ProtocolInstance(..))
    }

    pub(crate) fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        self.cycle_normalized_impl(db, env, previous, cycle)
    }

    fn cycle_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        match cycle_normalized_sync(
            self,
            env,
            previous,
            cycle,
            NormalizationFacts,
            &OrdinaryNormalizationEffects { db },
        ) {
            Ok(normalized) => normalized,
            Err(error) => match error {},
        }
    }

    pub fn is_none(&self, db: &'db dyn Db) -> bool {
        self.is_instance_of(db, KnownClass::NoneType)
    }

    fn is_bool(&self, db: &'db dyn Db) -> bool {
        self.is_instance_of(db, KnownClass::Bool)
    }

    fn is_enum(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        self.as_nominal_instance()
            .is_some_and(|instance| enum_metadata(db, instance.class_literal(db, env)).is_some())
    }

    fn is_typealias_special_form(&self) -> bool {
        matches!(self, Type::SpecialForm(SpecialFormType::TypeAlias))
    }

    /// Whether this type wraps an alias body that can be unfolded.
    const fn is_alias_like(self) -> bool {
        matches!(self, Type::TypeAlias(_) | Type::Recursive(_))
    }

    pub fn is_notimplemented(&self, db: &'db dyn Db) -> bool {
        self.is_instance_of(db, KnownClass::NotImplementedType)
    }

    fn is_todo(&self) -> bool {
        self.as_dynamic().is_some_and(|dynamic| match dynamic {
            DynamicType::Any
            | DynamicType::Unknown
            | DynamicType::InvalidConcatenateUnknown
            | DynamicType::UnknownGeneric(_)
            | DynamicType::UnspecializedTypeVar
            | DynamicType::UnknownLambdaParameter
            | DynamicType::AmbiguousOverload => false,
            DynamicType::Todo(_) => true,
        })
    }

    pub const fn is_generic_alias(&self) -> bool {
        matches!(self, Type::GenericAlias(_))
    }

    /// Returns whether this type represents a specialization of a generic type.
    ///
    /// For example, whereas `<class 'list'>` is a generic type, `<class 'list[int]'>`
    /// is a specialization of that type.
    fn is_specialized_generic(self, db: &'db dyn Db) -> bool {
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
            Type::NominalInstance(instance_type) => instance_type.is_definition_generic(db),
            Type::ProtocolInstance(protocol) => protocol
                .class_origin(db)
                .is_some_and(|class| class.is_generic()),
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

    const fn is_dynamic(&self) -> bool {
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
            Type::Callable(callable) => callable.deprecated(db).is_some(),
            Type::ClassLiteral(c) => c.deprecated(db).is_some(),
            _ => false,
        }
    }

    /// If the type is a specialized instance of the given `KnownClass`, returns the specialization.
    fn known_specialization(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        known_class: KnownClass,
    ) -> Option<Specialization<'db>> {
        match infer::type_context::known_specialization_sync(
            *self,
            known_class,
            &infer::type_context::OrdinaryTypeContextEffects { db, env },
        ) {
            Ok(specialization) => specialization,
            Err(never) => match never {},
        }
    }

    /// If the type is a specialized instance of the given class, returns the specialization.
    fn specialization_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        expected_class: StaticClassLiteral<'_>,
    ) -> Option<Specialization<'db>> {
        match infer::type_context::specialization_of_sync(
            self,
            expected_class,
            infer::type_context::TypeContextFacts,
            &infer::type_context::OrdinaryTypeContextEffects { db, env },
        ) {
            Ok(specialization) => specialization,
            Err(never) => match never {},
        }
    }

    /// If this type is a class instance or class-backed `TypedDict`, returns its specialization.
    fn class_specialization(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<(StaticClassLiteral<'db>, Specialization<'db>)> {
        match class_selection::class_specialization_sync(
            self,
            &class_selection::OrdinaryNominalSelection { db, env },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// If this type is a class instance, returns its class.
    fn nominal_class(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<ClassType<'db>> {
        match class_selection::nominal_class_sync(
            self,
            &class_selection::OrdinaryNominalSelection { db, env },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Returns `true` if this type may contain preferred type mappings when provided as type context
    /// during generic call inference.
    ///
    /// This is the case for any type which may contain types in non-covariant position within it,
    /// e.g., nominal instances of a generic class, or callables.
    fn may_prefer_declared_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        self.class_specialization(db, env).is_some()
            || self.expand_eagerly(db, env).is_callable_type()
    }

    /// Returns the top materialization (or upper bound materialization) of this type, which is the
    /// most general form of the type that is fully static.
    #[must_use]
    fn top_materialization(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        (*self).materialization(db, env, MaterializationKind::Top)
    }

    /// Returns the bottom materialization (or lower bound materialization) of this type, which is
    /// the most specific form of the type that is fully static.
    #[must_use]
    fn bottom_materialization(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        (*self).materialization(db, env, MaterializationKind::Bottom)
    }

    fn materialization(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        materialization_kind: MaterializationKind,
    ) -> Type<'db> {
        inline_mapping_result(mapping::effects::materialization_sync(
            db,
            self,
            materialization_kind,
            &mapping::effects::InlineMaterializationEffects { env },
            mapping::effects::MappingDispatchFacts,
        ))
    }

    /// If this type is an instance type where the class has a tuple spec, returns the tuple spec.
    ///
    /// I.e., for the type `tuple[int, str]`, this will return the tuple spec `[int, str]`.
    /// For a subclass of `tuple[int, str]`, it will return the same tuple spec.
    fn tuple_instance_spec(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Cow<'db, TupleSpec<'db>>> {
        match instance::tuple_spec::tuple_instance_spec_sync(
            *self,
            env,
            instance::tuple_spec::TupleSpecFacts,
            &instance::tuple_spec::OrdinaryTupleSpecEffects { db },
        ) {
            Ok(tuple) => tuple,
            Err(never) => match never {},
        }
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
    /// - In covariant position, it's replaced with `object`, or the type variable's upper bound
    ///   when the dynamic type is a bounded generic argument
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
    fn materialize(
        &self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        self.apply_type_mapping_impl(
            db,
            &TypeMapping::Materialize(materialization_kind),
            TypeContext::default(),
            visitor,
        )
    }

    fn has_dynamic(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        any_over_type(db, env, self, false, |ty| ty.is_dynamic())
    }

    /// Returns the specialized Python property represented by this type.
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

    const fn as_type_alias(self) -> Option<TypeAliasType<'db>> {
        match self {
            Type::KnownInstance(KnownInstanceType::TypeAliasType(type_alias)) => Some(type_alias),
            _ => None,
        }
    }

    /// Resolve aliases and recursive binders at the outermost level.
    fn resolve_type_alias(self, db: &'db dyn Db) -> Type<'db> {
        let mut ty = self;
        let mut seen = SmallVec::<[Type<'db>; 4]>::new();
        loop {
            if seen.contains(&ty) {
                return ty;
            }
            match ty.alias_resolution_step() {
                type_alias::AliasResolutionStep::Alias(alias) => {
                    seen.push(ty);
                    ty = alias.value_type(db);
                }
                type_alias::AliasResolutionStep::Recursive(recursive) => {
                    seen.push(ty);
                    ty = recursive.unfold(db, &recursive.environment(db)).into_type();
                }
                type_alias::AliasResolutionStep::UnboundRecursiveVariable => {
                    unreachable!("semantic operation on an unbound recursive variable")
                }
                type_alias::AliasResolutionStep::Resolved(ty) => return ty,
            }
        }
    }

    /// Selects the constructor used for a type variable's upper bound.
    ///
    /// The meta-type of `object` simplifies to permissive bare `type`, so retain the exact class
    /// object instead. Resolve aliases first so an alias of `object` cannot bypass that behavior.
    fn constructor_for_typevar_bound(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        let bound = self.resolve_type_alias(db);
        if bound.is_object() {
            KnownClass::Object.to_class_literal(db, env)
        } else {
            bound.to_meta_type(db, env)
        }
    }

    /// Returns `Some(UnionType)` if this type behaves like a union. Apart from explicit unions,
    /// this returns `Some` for `TypeAlias`es of unions and `NewType`s of `float` and `complex`.
    fn as_union_like(self, db: &'db dyn Db) -> Option<UnionType<'db>> {
        match union_like_sync(self, &InlineTypeDispatch { db }) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    const fn as_dynamic(self) -> Option<DynamicType<'db>> {
        match self {
            Type::Dynamic(dynamic_type) => Some(dynamic_type),
            _ => None,
        }
    }

    const fn as_callable(self) -> Option<CallableType<'db>> {
        match self {
            Type::Callable(callable_type) => Some(callable_type),
            _ => None,
        }
    }

    const fn expect_dynamic(self) -> DynamicType<'db> {
        self.as_dynamic().expect("Expected a Type::Dynamic variant")
    }

    const fn as_protocol_instance(self) -> Option<ProtocolInstanceType<'db>> {
        match self {
            Type::ProtocolInstance(instance) => Some(instance),
            _ => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    const fn expect_class_literal(self) -> ClassLiteral<'db> {
        self.as_class_literal()
            .expect("Expected a Type::ClassLiteral variant")
    }

    pub const fn is_subclass_of(&self) -> bool {
        matches!(self, Type::SubclassOf(..))
    }

    pub const fn is_class_literal(&self) -> bool {
        matches!(self, Type::ClassLiteral(..))
    }

    const fn as_literal_value(self) -> Option<LiteralValueType<'db>> {
        match self {
            Type::LiteralValue(literal) => Some(literal),
            _ => None,
        }
    }

    fn as_literal_value_kind(self) -> Option<LiteralValueTypeKind<'db>> {
        match self {
            Type::LiteralValue(literal) => Some(literal.kind()),
            _ => None,
        }
    }

    const fn is_typed_dict(&self) -> bool {
        matches!(self, Type::TypedDict(..))
    }

    const fn as_typed_dict(self) -> Option<TypedDictType<'db>> {
        match self {
            Type::TypedDict(typed_dict) => Some(typed_dict),
            _ => None,
        }
    }

    /// Turn a class literal (`Type::ClassLiteral` or `Type::GenericAlias`) into a `ClassType`.
    /// Since a `ClassType` must be specialized, apply the default specialization to any
    /// unspecialized generic class literal.
    fn to_class_type(self, db: &'db dyn Db) -> Option<ClassType<'db>> {
        class::type_conversion::type_to_class_type(db, self)
    }

    const fn is_property_instance(&self) -> bool {
        matches!(self, Type::PropertyInstance(..))
    }

    pub(crate) fn module_literal(
        db: &'db dyn Db,
        importing_file: ProgramFile<'db>,
        submodule: Module<'db>,
    ) -> Self {
        Self::ModuleLiteral(ModuleLiteralType::new(
            db,
            submodule,
            submodule.kind(db).is_package().then_some(importing_file),
        ))
    }

    const fn is_union(self) -> bool {
        matches!(self, Type::Union(_))
    }

    pub const fn as_union(self) -> Option<UnionType<'db>> {
        match self {
            Type::Union(union_type) => Some(union_type),
            _ => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    const fn expect_union(self) -> UnionType<'db> {
        self.as_union().expect("Expected a Type::Union variant")
    }

    const fn is_intersection(self) -> bool {
        matches!(self, Type::Intersection(_))
    }

    /// Returns whether this is a "real" intersection type. (Negated types are represented by an
    /// intersection containing a single negative branch, which this method does _not_ consider a
    /// "real" intersection.)
    fn is_nontrivial_intersection(self, db: &'db dyn Db) -> bool {
        match self {
            Type::Intersection(intersection) => !intersection.is_simple_negation(db),
            _ => false,
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
    fn expect_function_literal(self) -> FunctionType<'db> {
        self.as_function_literal()
            .expect("Expected a Type::FunctionLiteral variant")
    }

    pub(crate) const fn is_function_literal(&self) -> bool {
        matches!(self, Type::FunctionLiteral(..))
    }

    fn as_string_literal(self) -> Option<StringLiteralType<'db>> {
        match self {
            Type::LiteralValue(literal) => literal.as_string(),
            _ => None,
        }
    }

    fn is_int_literal(&self) -> bool {
        self.as_literal_value()
            .is_some_and(LiteralValueType::is_int)
    }

    fn as_int_literal(self) -> Option<i64> {
        match self {
            Type::LiteralValue(literal) => literal.as_int(),
            _ => None,
        }
    }

    fn as_int_like_literal(self) -> Option<i64> {
        match self.as_literal_value_kind() {
            Some(LiteralValueTypeKind::Int(value)) => Some(value.as_i64()),
            Some(LiteralValueTypeKind::Bool(value)) => Some(i64::from(value)),
            _ => None,
        }
    }

    const fn as_bool_literal(self) -> Option<bool> {
        match self {
            Type::LiteralValue(literal) => literal.as_bool(),
            _ => None,
        }
    }

    const fn is_bool_literal(&self) -> bool {
        self.as_bool_literal().is_some()
    }

    pub(crate) fn as_enum_literal(self) -> Option<EnumLiteralType<'db>> {
        match self {
            Type::LiteralValue(literal) => literal.as_enum(),
            _ => None,
        }
    }

    #[cfg(test)]
    #[track_caller]
    fn expect_enum_literal(self) -> EnumLiteralType<'db> {
        match self.as_literal_value_kind() {
            Some(LiteralValueTypeKind::Enum(e)) => e,
            _ => panic!("Expected a `LiteralValueTypeKind::Enum` variant"),
        }
    }

    fn is_string_literal(&self) -> bool {
        self.as_literal_value()
            .is_some_and(literal::LiteralValueType::is_string)
    }

    /// Detects types which are valid to appear inside a `Literal[…]` type annotation.
    fn is_literal_or_union_of_literals(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
        match self {
            Type::Union(union) => union
                .elements(db)
                .iter()
                .all(|ty| ty.is_literal_or_union_of_literals(db, env)),
            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::String(_)
                | LiteralValueTypeKind::Bytes(_)
                | LiteralValueTypeKind::Int(_)
                | LiteralValueTypeKind::Bool(_)
                | LiteralValueTypeKind::Enum(_) => true,
                LiteralValueTypeKind::LiteralString => false,
            },
            Type::NominalInstance(_) => {
                self.is_none(db) || self.is_bool(db) || self.is_enum(db, env)
            }
            _ => false,
        }
    }

    /// Create a promotable string literal.
    pub(crate) fn string_literal<T>(db: &'db dyn Db, string: T) -> Self
    where
        T: salsa::Lookup<CompactString> + std::hash::Hash,
        CompactString: salsa::HashEqLike<T>,
    {
        Self::LiteralValue(LiteralValueType::promotable(StringLiteralType::new(
            db, string,
        )))
    }

    /// Create a promotable enum literal.
    fn enum_literal(value: EnumLiteralType<'db>) -> Self {
        Self::LiteralValue(LiteralValueType::promotable(value))
    }

    /// Create a promotable integer literal.
    pub(crate) fn int_literal(int: i64) -> Self {
        Self::LiteralValue(LiteralValueType::promotable(int))
    }

    /// Create a promotable single-character string literal.
    fn single_char_string_literal(db: &'db dyn Db, c: char) -> Self {
        Self::LiteralValue(LiteralValueType::promotable(StringLiteralType::new(
            db,
            c.to_compact_string(),
        )))
    }

    /// Create a promotable bytes literal.
    fn bytes_literal(db: &'db dyn Db, bytes: &[u8]) -> Self {
        Self::LiteralValue(LiteralValueType::promotable(BytesLiteralType::new(
            db, bytes,
        )))
    }

    /// Create a promotable boolean literal.
    pub fn bool_literal(value: bool) -> Self {
        Self::LiteralValue(LiteralValueType::promotable(value))
    }

    /// Create a `LiteralString`.
    fn literal_string() -> Self {
        // Note that `LiteralString`s are never implicitly inferred, and so are always unpromotable.
        Self::LiteralValue(LiteralValueType::unpromotable(
            LiteralValueTypeKind::LiteralString,
        ))
    }

    fn typed_dict(defining_class: impl Into<ClassType<'db>>) -> Self {
        Self::TypedDict(TypedDictType::new(defining_class.into()))
    }

    #[must_use]
    fn negate(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match negation::negate_sync(
            *self,
            negation::NegationFacts,
            &negation::OrdinaryNegationEffects { db, env },
        ) {
            Ok(negated) => negated,
            Err(never) => match never {},
        }
    }

    #[must_use]
    fn negate_if(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, yes: bool) -> Type<'db> {
        if yes { self.negate(db, env) } else { *self }
    }

    /// Return `true` if it is possible to spell an equivalent type to this one
    /// in user annotations without nonstandard extensions to the type system
    fn is_spellable(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            Type::LiteralValue(_)
            | Type::Never
            | Type::NewTypeInstance(_)
            | Type::NominalInstance(_) => true,
            // `TypedDict` and `Protocol` can be synthesized,
            // but it's always possible to create an equivalent type using a class definition.
            Type::TypedDict(_) | Type::ProtocolInstance(_) => true,
            // Not all `Callable` types are spellable using the `Callable` type form,
            // but they are all spellable using callback protocols.
            Type::Callable(_) => true,
            // `Unknown` and `@Todo` are nonstandard extensions,
            // but they are both exactly equivalent to `Any`
            Type::Dynamic(_) => true,
            Type::TypeVar(_) | Type::SubclassOf(_) => true,
            // `Recursive` currently only represents implicit type aliases with declared names.
            // Revisit this and `is_hintable` when general recursive type inference can produce
            // types without a declared alias.
            Type::TypeAlias(_) | Type::Recursive(_) => true,
            Type::TypeForm(typeform) => typeform.type_argument(db).is_spellable(db),
            Type::Intersection(_) => false,
            Type::EnumComplement(complement) => complement.is_spellable(db),
            Type::Divergent(_)
            | Type::SpecialForm(_)
            | Type::BoundSuper(_)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
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
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            Type::NominalInstance(_)
            | Type::NewTypeInstance(_)
            | Type::LiteralValue(_)
            | Type::TypeAlias(_)
            | Type::Recursive(_) => true,

            Type::Intersection(_)
            | Type::EnumComplement(_)
            | Type::Divergent(_)
            | Type::SpecialForm(_)
            | Type::BoundSuper(_)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
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
                SubclassOfInner::Protocol(_) => true,
                SubclassOfInner::Dynamic(dynamic) => Type::Dynamic(dynamic).is_hintable(db),
                SubclassOfInner::TypeVar(tvar) => Type::TypeVar(tvar).is_hintable(db),
            },

            Type::TypeVar(tvar) => tvar.typevar(db).definition(db).is_some(),

            Type::Union(union) => union.elements(db).iter().all(|ty| ty.is_hintable(db)),

            Type::TypedDict(td) => td.defining_class().is_some(),

            Type::ProtocolInstance(protocol) => protocol.class_origin(db).is_some(),

            Type::Dynamic(dynamic) => match dynamic {
                DynamicType::Any => true,
                DynamicType::Unknown
                | DynamicType::UnknownGeneric(_)
                | DynamicType::UnspecializedTypeVar
                | DynamicType::UnknownLambdaParameter
                | DynamicType::Todo(_)
                | DynamicType::InvalidConcatenateUnknown
                | DynamicType::AmbiguousOverload => false,
            },
        }
    }

    /// If the type is a union (or a type alias that resolves to a union), filters union elements
    /// based on the provided predicate.
    ///
    /// Aliases among the elements are expanded first. An element may itself be an alias for a
    /// union, which is otherwise left unexpanded so diagnostics can name it, but filtering is a
    /// set operation and has to see the members rather than the name.
    ///
    /// Otherwise, returns the type unchanged.
    fn filter_union(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        f: impl FnMut(&Type<'db>) -> bool,
    ) -> Type<'db> {
        let union = match infer::type_context::union_for_filter_sync(
            self,
            &infer::type_context::OrdinaryTypeContextEffects { db, env },
        ) {
            Ok(union) => union,
            Err(never) => match never {},
        };
        let Some(union) = union else {
            return self;
        };
        union.filter_expanding_aliases(db, env, f)
    }

    /// If the type is a union, removes union elements that are disjoint from `target`.
    ///
    /// Returns [`DiscardDisjointUnionElementsResult::AllDisjoint`] if every union element is removed.
    /// Non-union inputs, including `Never`, are returned unchanged as
    /// [`DiscardDisjointUnionElementsResult::Retained`].
    fn discard_disjoint_union_elements(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
    ) -> DiscardDisjointUnionElementsResult<'db> {
        match infer::type_context::discard_disjoint_sync(
            self,
            target,
            inferable,
            infer::type_context::TypeContextFacts,
            &infer::type_context::OrdinaryTypeContextEffects { db, env },
        ) {
            Ok(filtered) => filtered,
            Err(never) => match never {},
        }
    }

    /// Returns the fallback instance type that a literal is an instance of, or `None` if the type
    /// is not a literal.
    fn literal_fallback_instance(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        // There are other literal types that could conceivable be included here: class literals
        // falling back to `type[X]`, for instance. For now, there is not much rigorous thought put
        // into what's included vs not; this is just an empirical choice that makes our ecosystem
        // report look better until we have proper bidirectional type inference.
        match class_selection::literal_fallback_instance_sync(
            self,
            env,
            class_selection::LiteralFallbackFacts,
            &class_selection::OrdinaryNominalSelection { db, env },
        ) {
            Ok(instance) => instance,
            Err(error) => match error {},
        }
    }

    /// Promote (possibly nested) literals to types that these literals are instances of.
    ///
    /// Note that this function tries to promote literals to a more user-friendly form than their
    /// fallback instance type. For example, `def _() -> int` is promoted to `Callable[[], int]`,
    /// as opposed to `FunctionType`.
    pub(crate) fn promote(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        self.apply_type_mapping(
            db,
            env,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular),
            TypeContext::default(),
        )
    }

    #[ty_mapping_probe_macros::dual_public_promotion]
    pub(crate) async fn promote_public_with<E: PublicPromotionEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects.checkpoint(PublicPromotionWork::Admission).await?;
        let mapped = effects.regular(db, env, self).await?;
        mapped.promote_singletons_impl_with(db, env, effects).await
    }

    /// Finalizes the element type of a mutable collection after combining its element evidence.
    /// Literal types supplied by explicit annotations remain unpromotable. Without contextual
    /// constraints, singleton types also widen: `[None]` permits later mutation, as does the list
    /// created by `*rest, = (None,)`.
    /// Evidence from later collection uses also passes through this helper, since those types
    /// have not necessarily undergone the promotion applied to literal elements during inference.
    fn promote_collection_element_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        allow_tuple_size_promotion: bool,
        unconstrained: bool,
    ) -> Type<'db> {
        let ty = if unconstrained {
            self.promote(db, env)
        } else {
            self
        };
        let ty = if allow_tuple_size_promotion {
            ty.promote_tuple_size_in_union(db, env)
        } else {
            ty
        };
        if unconstrained {
            ty.promote_singletons_recursively(db, env)
        } else {
            ty
        }
    }

    /// Promote a top-level singleton type (like `None`, `EllipsisType`) to `T | Unknown`.
    pub(crate) fn promote_singletons(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        self.promote_singletons_impl(db, env)
    }

    /// Promote class literals to the class objects represented by `type[...]`.
    ///
    /// This is intentionally separate from regular promotion. Applying it during collection
    /// inference would lose useful precision for local and module-level collections of class
    /// objects.
    fn promote_class_literals(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        self.apply_type_mapping(
            db,
            env,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::ClassLiteralsOnly),
            TypeContext::default(),
        )
    }

    /// Recursively promote singleton types (like `None`, `EllipsisType`) to
    /// `T | Unknown` within nominal type parameters, without recursing into unions.
    /// Used for collection literal inference so that `[None]` is inferred as
    /// `list[None | Unknown]` rather than `list[None]`.
    fn promote_singletons_recursively(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        self.apply_type_mapping(
            db,
            env,
            &TypeMapping::Promote(PromotionMode::On, PromotionKind::SingletonsOnly),
            TypeContext::default(),
        )
    }

    /// Like [`Type::promote`], but does not recurse into nested types.
    fn promote_impl(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        inline_mapping_result(self.promote_impl_sync(db, env, &InlineMappingEffects))
    }

    async fn promote_impl_with<E: MappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        promotion::leaf::promote_leaf_with(
            self,
            env,
            promotion::leaf::PromotionLeafFacts,
            &promotion::leaf::MappingPromotionEffects { db, effects },
        )
        .await
    }

    fn promote_impl_sync<E: SynchronousMappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        promotion::leaf::promote_leaf_sync(
            self,
            env,
            promotion::leaf::PromotionLeafFacts,
            &promotion::leaf::MappingPromotionEffects { db, effects },
        )
    }

    /// Like [`Type::promote_singletons_recursively`], but does not recurse into nested types.
    fn promote_singletons_impl(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        inline_public_promotion_result(self.promote_singletons_impl_sync(
            db,
            env,
            &InlinePublicPromotionEffects,
        ))
    }

    #[ty_mapping_probe_macros::dual_public_promotion]
    async fn promote_singletons_impl_with<E: PublicPromotionEffects<'db>>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        effects
            .checkpoint(PublicPromotionWork::SingletonDispatch)
            .await?;
        let Type::NominalInstance(instance) = self else {
            return Ok(self);
        };
        effects
            .checkpoint(PublicPromotionWork::SingletonClassification)
            .await?;
        if effects.is_singleton(db, instance).await? {
            effects.union_two(db, env, self, Type::unknown()).await
        } else {
            Ok(self)
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
    pub(crate) fn recursive_type_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle,
    ) -> Self {
        self.recursive_type_normalized_impl_with_cycle(db, env, cycle)
    }

    fn recursive_type_normalized_impl_with_cycle(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle,
    ) -> Self {
        match recursive_type_normalized_with_cycle_sync(
            self,
            env,
            cycle,
            NormalizationFacts,
            &OrdinaryNormalizationEffects { db },
        ) {
            Ok(normalized) => normalized,
            Err(error) => match error {},
        }
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
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match recursive_normalize_sync(
            RecursiveNormalizationRequest {
                ty: self,
                divergent: div,
                nested,
            },
            env,
            &OrdinaryNormalizationEffects { db },
            RecursiveNormalizationFacts,
        ) {
            Ok(normalized) => normalized,
            Err(error) => match error {},
        }
    }

    /// Recursively visit a type and its specializations.
    ///
    /// The provided closure will be called on the type itself and its nested types, along with
    /// their variance with respect to the outermost type. Repeated types with the same variance
    /// may be skipped.
    fn visit_specialization<F>(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, mut f: F)
    where
        F: FnMut(Type<'db>, TypeVarVariance),
    {
        self.visit_specialization_impl(
            db,
            env,
            TypeVarVariance::Covariant,
            &mut f,
            &SpecializationVisitor::default(),
        );
    }

    fn visit_specialization_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        polarity: TypeVarVariance,
        f: &mut dyn FnMut(Type<'db>, TypeVarVariance),
        visitor: &SpecializationVisitor<'db>,
    ) {
        f(self, polarity);

        visitor.visit(db, (self, polarity), || {
            let Some((_, specialization)) = self.class_specialization(db, env) else {
                match self {
                    Type::Union(union) => {
                        for element in union.elements(db) {
                            element.visit_specialization_impl(db, env, polarity, f, visitor);
                        }
                    }
                    Type::Intersection(intersection) => {
                        for element in intersection.positive(db) {
                            element.visit_specialization_impl(db, env, polarity, f, visitor);
                        }
                        for element in intersection.negative(db) {
                            element.visit_specialization_impl(db, env, polarity.flip(), f, visitor);
                        }
                    }
                    Type::TypeAlias(alias) => alias
                        .value_type(db)
                        .visit_specialization_impl(db, env, polarity, f, visitor),
                    Type::Recursive(recursive) => {
                        if let UnfoldResult::Unfolded(unfolded) = recursive.unfold(db, env) {
                            unfolded.visit_specialization_impl(db, env, polarity, f, visitor);
                        }
                    }
                    Type::Callable(callable) => {
                        for signature in callable.signatures(db) {
                            for parameter in signature.parameters() {
                                parameter.annotated_type().visit_specialization_impl(
                                    db,
                                    env,
                                    polarity.flip(),
                                    f,
                                    visitor,
                                );
                            }

                            signature
                                .return_ty
                                .visit_specialization_impl(db, env, polarity, f, visitor);
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
                ty.visit_specialization_impl(db, env, variance, f, visitor);
            }
        });
    }

    /// Return true if there is just a single inhabitant for this type.
    ///
    /// Note: This function aims to have no false positives, but might return `false`
    /// for more complicated types that are actually singletons.
    fn is_singleton(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        match self {
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => false,
            Type::Recursive(recursive) => recursive
                .unfold(db, env)
                .is_unfolded_and(|unfolded| unfolded.is_singleton(db, env)),

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
                match bound_typevar.typevar(db).bound_or_constraints(db, env) {
                    None => false,
                    Some(TypeVarBoundOrConstraints::UpperBound(_)) => false,
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                        .elements(db)
                        .iter()
                        .all(|constraint| constraint.is_singleton(db, env)),
                }
            }

            // We eagerly transform `SubclassOf` to `ClassLiteral` for final types, so `SubclassOf` is never a singleton.
            Type::SubclassOf(..) => false,
            Type::BoundSuper(..) => false,
            Type::GenericAlias(..) => false,
            Type::FunctionLiteral(..)
            | Type::WrapperDescriptor(..)
            | Type::ClassLiteral(..)
            | Type::ModuleLiteral(..) => true,
            Type::SpecialForm(special_form) => special_form.is_guaranteed_singleton(),
            Type::KnownInstance(KnownInstanceType::Sentinel(_)) => true,
            Type::KnownInstance(_) => false,
            Type::Callable(_) => {
                // A callable type is never a singleton because for any given signature,
                // there could be any number of distinct objects that are all callable with that
                // signature.
                false
            }
            Type::BoundMethod(..) => {
                // `BoundMethod` types are not singleton types:
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
            Type::PropertyInstance(_) | Type::SlotDescriptor(_) => false,
            Type::Union(..) => {
                // A single-element union, where the sole element was a singleton, would itself
                // be a singleton type. However, unions with length < 2 should never appear in
                // our model due to [`UnionBuilder::build`].
                false
            }
            Type::Intersection(intersection) => intersection
                .enum_complement(db, env)
                .is_some_and(|complement| complement.is_singleton(db)),
            Type::EnumComplement(complement) => complement.is_singleton(db),
            Type::AlwaysTruthy | Type::AlwaysFalsy => false,
            Type::TypeIs(type_is) => type_is.is_bound(db),
            Type::TypeGuard(type_guard) => type_guard.is_bound(db),
            Type::TypeForm(_) => false,
            Type::TypedDict(_) => false,
            Type::TypeAlias(alias) => alias.value_type(db).is_singleton(db, env),
            Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db).is_singleton(db, env),
        }
    }

    /// This function is roughly equivalent to `find_name_in_mro` as defined in the [descriptor guide] or
    /// [`_PyType_Lookup`] in CPython's `Objects/typeobject.c`. It should typically be called through
    /// [`Type::class_member`], unless it is known that `self` is a class-like type. This function returns
    /// `None` if called on an instance-like type.
    ///
    /// [descriptor guide]: https://docs.python.org/3/howto/descriptor.html#invocation-from-an-instance
    /// [`_PyType_Lookup`]: https://github.com/python/cpython/blob/e285232c76606e3be7bf216efb1be1e742423e4b/Objects/typeobject.c#L5223
    fn find_name_in_mro(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Option<PlaceAndQualifiers<'db>> {
        self.find_name_in_mro_with_policy(db, env, name, MemberLookupPolicy::default())
    }

    fn find_name_in_mro_with_policy(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Option<PlaceAndQualifiers<'db>> {
        match member_lookup::mro_dispatch::find_name_in_mro_sync(
            *self,
            env,
            name,
            policy,
            member_lookup::mro_dispatch::MroLookupFacts,
            &member_lookup::mro_dispatch::OrdinaryMroLookupEffects { db },
        ) {
            Ok(member) => member,
            Err(never) => match never {},
        }
    }

    fn lookup_dunder_new(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<PlaceAndQualifiers<'db>> {
        #[salsa::tracked(configuration = (pub(in crate::types) LookupDunderNewInnerConfiguration), attempt = ReturnOnly, returns(copy), cycle_initial=|_, _, _, _| None, heap_size=ruff_memory_usage::heap_size)]
        fn lookup_dunder_new_inner<'db>(
            db: &'db dyn Db,
            program: Program<'db>,
            ty: Type<'db>,
        ) -> Option<PlaceAndQualifiers<'db>> {
            let env = &ProgramEnvironment::from_program(program);
            match constructor::new_lookup::lookup_dunder_new_sync(
                ty,
                env,
                constructor::new_lookup::NewLookupFacts,
                &constructor::new_lookup::OrdinaryNewLookupEffects { db },
            ) {
                Ok(member) => member,
                Err(never) => match never {},
            }
        }

        #[cfg(any(test, feature = "experimental-analysis"))]
        #[expect(non_local_definitions, reason = "The implementation must name the existing nested query configuration without moving its identity")]
        impl salsa::plumbing::TrackedFunctionConfiguration for LookupDunderNewQuery {
            type Configuration = LookupDunderNewInnerConfiguration;
        }

        #[cfg(any(test, feature = "experimental-analysis"))]
        #[expect(non_local_definitions, reason = "The accessor must name the existing nested query ingredient without moving its identity")]
        impl LookupDunderNewQuery {
            /// Returns the existing constructor lookup ingredient without evaluating it.
            pub(in crate::types) fn ingredient(
                db: &dyn Db,
            ) -> &IngredientImpl<LookupDunderNewConfiguration> {
                lookup_dunder_new_inner::fn_ingredient_(db, db.zalsa())
            }
        }

        lookup_dunder_new_inner(db, env.program(db), self)
    }

    /// Look up an attribute in the MRO of the meta-type of `self`. This returns class-level attributes
    /// when called on an instance-like type, and metaclass attributes when called on a class-like type.
    ///
    /// Basically corresponds to `self.to_meta_type().find_name_in_mro(name)`, except for the handling
    /// of union and intersection types.
    fn class_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        self.class_member_with_policy(db, env, name, MemberLookupPolicy::default())
    }

    fn class_member_with_policy(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        Self::class_member_with_policy_inner(
            db,
            MemberLookupKey::new(db, env.program(db), self, name, policy),
        )
    }

    fn class_member_with_policy_inner(
        db: &'db dyn Db,
        key: MemberLookupKey<'db>,
    ) -> PlaceAndQualifiers<'db> {
        class_member_with_policy_inner(db, key)
    }

    /// Look up the class member that participates in descriptor access through an instance.
    ///
    /// The meta-type of a type variable preserves method binding to that type variable, but it does
    /// not carry attributes stored in a nominal upper-bound class's namespace by its metaclass.
    /// Add those attributes using the same lookup as a concrete nominal instance.
    fn instance_lookup_class_member_with_policy(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let ty = key.ty(db);

        // `object.__dict__` is a typeshed approximation: a concrete slotted instance without
        // dictionary storage does not inherit that attribute at runtime. Keep normal lookup for
        // `Self` and other type variables because their subclasses can introduce a dictionary.
        let key = if key.name(db) == "__dict__"
            && let Type::NominalInstance(instance) = receiver
            && let Some((class, _)) = instance.class(db, env).static_class_literal(db)
            && class.lacks_instance_storage(db, "__dict__")
        {
            MemberLookupKey::new(
                db,
                key.program(db),
                ty,
                key.name(db).as_str(),
                key.policy(db) | MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
            )
        } else {
            key
        };

        if let Type::TypeVar(_) = ty {
            if let Some(class) = ty.nominal_class(db, env) {
                let name = key.name(db);
                let policy = key.policy(db);

                return ty
                    .to_meta_type(db, env)
                    .class_namespace_member(db, env, class, name, policy);
            }
        }

        Self::class_member_with_policy_inner(db, key)
    }

    /// Look up attributes stored in the namespace of a class object.
    ///
    /// Besides attributes present in the class MRO, this includes attributes assigned to
    /// instances of its metaclass. For example, `cls.x = ...` in `Meta.__init__` stores `x`
    /// on each class object constructed by `Meta`.
    fn class_object_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        crate::reachability::source::infallible(
            member_lookup::class_object::class_object_member_sync(
                self,
                name,
                policy,
                member_lookup::class_object::ClassObjectFacts,
                &member_lookup::class_object::OrdinaryClassObjectEffects { db, env },
            ),
        )
    }

    fn with_definedness(
        member: PlaceAndQualifiers<'db>,
        definedness: Definedness,
    ) -> PlaceAndQualifiers<'db> {
        match member {
            PlaceAndQualifiers {
                place: Place::Defined(member),
                qualifiers,
            } => Place::Defined(member.with_definedness(definedness)).with_qualifiers(qualifiers),
            member => member,
        }
    }

    /// Look up metaclass instance members in a constructed class's namespace.
    ///
    /// A class object is an instance of its metaclass, and its instance storage is also the class
    /// namespace consulted when looking up attributes through instances of that class.
    ///
    /// ```python
    /// class Meta(type):
    ///     generated: int
    ///
    /// class C(metaclass=Meta): ...
    ///
    /// reveal_type(C().generated)  # int
    /// ```
    ///
    /// An own class binding or `ClassVar` contract shadows a normal generated attribute. During
    /// instance lookup, the result participates in the existing descriptor and instance-fallback
    /// logic.
    ///
    /// Metaclass instance members participate, including inherited declarations and attributes
    /// inferred from instance methods. Class-body-only bindings remain attributes of the
    /// metaclass itself and are excluded.
    fn class_namespace_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        match namespace_lookup_sync(
            self,
            NamespaceLookupRequest {
                class,
                name,
                policy,
            },
            &InlineNamespaceLookupEffects::new(db, env),
        ) {
            Ok(member) => member,
            Err(never) => match never {},
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
    fn instance_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        match self {
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            Type::Union(union) => union.map_with_boundness_and_qualifiers(db, env, |elem| {
                elem.instance_member(db, env, name)
            }),

            Type::Intersection(intersection) => {
                if let Some(complement) = intersection.enum_complement(db, env) {
                    enums::instance_member_for_enum_complement(db, env, complement, name)
                } else {
                    intersection.map_with_boundness_and_qualifiers(db, env, |elem| {
                        elem.instance_member(db, env, name)
                    })
                }
            }

            Type::EnumComplement(complement) => {
                enums::instance_member_for_enum_complement(db, env, *complement, name)
            }

            Type::Recursive(recursive) => recursive
                .unfold(db, env)
                .map(|unfolded| unfolded.instance_member(db, env, name))
                .unwrap_or(Place::bound(self).into()),

            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => Place::bound(self).into(),

            Type::NominalInstance(instance) => {
                instance.class(db, env).instance_member(db, env, name)
            }
            Type::NewTypeInstance(newtype) => newtype
                .concrete_base_type(db)
                .instance_member(db, env, name),

            Type::ProtocolInstance(protocol) => protocol.instance_member(db, env, name),

            Type::FunctionLiteral(function) => function
                .runtime_class(db)
                .to_instance(db, env)
                .instance_member(db, env, name),

            Type::BoundMethod(_) => KnownClass::MethodType
                .to_instance(db, env)
                .instance_member(db, env, name),
            Type::KnownBoundMethod(method) => method
                .class()
                .to_instance(db, env)
                .instance_member(db, env, name),
            Type::WrapperDescriptor(_) => KnownClass::WrapperDescriptorType
                .to_instance(db, env)
                .instance_member(db, env, name),
            Type::DataclassDecorator(_) => KnownClass::FunctionType
                .to_instance(db, env)
                .instance_member(db, env, name),
            Type::Callable(_) | Type::DataclassTransformer(_) => {
                Type::object().instance_member(db, env, name)
            }

            Type::TypeVar(bound_typevar) => {
                match bound_typevar.require_bound_or_constraints(db, env) {
                    TypeVarBoundOrConstraints::UpperBound(bound) => {
                        bound.instance_member(db, env, name)
                    }
                    TypeVarBoundOrConstraints::Constraints(constraints) => constraints
                        .map_with_boundness_and_qualifiers(db, env, |constraint| {
                            constraint.instance_member(db, env, name)
                        }),
                }
            }

            Type::TypeIs(_) | Type::TypeGuard(_) => KnownClass::Bool
                .to_instance(db, env)
                .instance_member(db, env, name),

            Type::LiteralValue(literal) => literal
                .fallback_instance(db, env)
                .instance_member(db, env, name),

            Type::AlwaysTruthy | Type::AlwaysFalsy | Type::TypeForm(_) => {
                Type::object().instance_member(db, env, name)
            }
            Type::ModuleLiteral(_) => KnownClass::ModuleType
                .to_instance(db, env)
                .instance_member(db, env, name),

            Type::SpecialForm(_) | Type::KnownInstance(_) => Place::Undefined.into(),

            Type::PropertyInstance(property) => property
                .instance_class(db)
                .to_instance(db, env)
                .instance_member(db, env, name),

            Type::SlotDescriptor(_) => KnownClass::MemberDescriptorType
                .to_instance(db, env)
                .instance_member(db, env, name),

            // Note: `super(pivot, owner).__dict__` refers to the `__dict__` of the `builtins.super` instance,
            // not that of the owner.
            // This means we should only look up instance members defined on the `builtins.super()` instance itself.
            // If you want to look up a member in the MRO of the `super`'s owner,
            // refer to [`Type::member`] instead.
            Type::BoundSuper(_) => KnownClass::Super
                .to_instance(db, env)
                .instance_member(db, env, name),

            // TODO: we currently don't model the fact that class literals and subclass-of types have
            // a `__dict__` that is filled with class level attributes. Modeling this is currently not
            // required, as `instance_member` is only called for instance-like types through `member`,
            // but we might want to add this in the future.
            Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_) => {
                Place::Undefined.into()
            }

            Type::TypedDict(_) => Place::Undefined.into(),

            Type::TypeAlias(alias) => alias.value_type(db).instance_member(db, env, name),
        }
    }

    /// Access an attribute of this type without invoking the descriptor protocol. This
    /// method corresponds to `inspect.getattr_static(<object of type 'self'>, name)`.
    ///
    /// See also: [`Type::member`]
    fn static_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Place<'db> {
        if let Type::ModuleLiteral(module) = self {
            module
                .static_member(db, env, name)
                .map_or(Place::Undefined, |member| member.member(db).place)
        } else if let place @ Place::Defined(_) = self.class_member(db, env, name).place {
            place
        } else if let Some(place @ Place::Defined(_)) = self
            .find_name_in_mro(db, env, name)
            .map(|inner| inner.place)
        {
            place
        } else {
            self.instance_member(db, env, name).place
        }
    }

    /// Collect deprecated accessor implementations without inferring their signatures or
    /// intersecting their function or descriptor types. Retain the declarations so callers can
    /// report deprecations after descriptor lookup replaces the property with its value type:
    ///
    /// ```python
    /// from typing_extensions import deprecated
    ///
    /// class C:
    ///     @property
    ///     @deprecated("old getter")
    ///     def value(self) -> int: ...
    ///
    /// C().value  # Warn about the getter, even though the attribute has type `int`.
    /// ```
    ///
    /// Overload deprecations require a resolved call and do not apply to accessor references.
    fn property_deprecations(self, db: &'db dyn Db) -> Option<PropertyDeprecations<'db>> {
        match property_metadata_sync(self, &InlineTypeDispatch { db }) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    fn collect_property_deprecations(self, db: &'db dyn Db) -> Option<PropertyDeprecations<'db>> {
        legacy_inline(property_deprecations::collect_with(
            db,
            self,
            &function::LegacyFunctionIdentityEffects,
        ))
    }

    /// Returns the descriptor result type for directly dynamic values and gradual class-object
    /// values.
    fn dynamic_descriptor_type(self) -> Option<Type<'db>> {
        match self {
            Type::Dynamic(_) => Some(self),
            Type::SubclassOf(subclass_of) => {
                subclass_of.subclass_of().into_dynamic().map(Type::Dynamic)
            }
            _ => None,
        }
    }

    /// Looks up `__get__` on the meta-type of `self` and calls it with `self`, `instance`, and
    /// `owner`. Unlike other dunder methods, `__get__` is not itself looked up using the
    /// descriptor protocol.
    ///
    /// Returns the resulting type and descriptor kind, or an error retaining the recovery value
    /// when the implicit call is invalid. Returns `Ok(None)` when `__get__` is not defined.
    ///
    /// For example, accessing `C().value` below implicitly supplies the descriptor value, the
    /// `C` instance, and `C`, so the declared method is missing two parameters:
    ///
    /// ```python
    /// class Descriptor:
    ///     def __get__(self): ...
    ///
    /// class C:
    ///     value = Descriptor()
    ///
    /// C().value
    /// ```
    pub(crate) fn try_call_dunder_get(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        instance: Option<Type<'db>>,
        owner: Type<'db>,
    ) -> Result<Option<DescriptorGetResult<'db>>, DescriptorGetError<'db>> {
        self.try_call_dunder_get_with_recursion_guard(db, env, instance, owner, None)
    }

    fn try_call_dunder_get_with_recursion_guard(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        instance: Option<Type<'db>>,
        owner: Type<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Result<Option<DescriptorGetResult<'db>>, DescriptorGetError<'db>> {
        tracing::trace!(
            "try_call_dunder_get: {}, {}, {}",
            self.display(db, env),
            instance
                .unwrap_or_else(|| Type::none(db, env))
                .display(db, env),
            owner.display(db, env)
        );

        descriptor::evaluate_entry(
            db,
            env,
            descriptor::DescriptorRequest {
                ty: self,
                instance,
                owner,
            },
            recursion_guard,
        )
    }

    /// Applies the descriptor protocol while preserving the attribute's place metadata.
    fn try_call_dunder_get_on_attribute(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        attribute: PlaceAndQualifiers<'db>,
        instance: Option<Type<'db>>,
        owner: Type<'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> (
        PlaceAndQualifiers<'db>,
        AttributeKind,
        Option<DescriptorGetCallContext<'db>>,
        DescriptorOrigin<'db>,
    ) {
        let result = crate::reachability::source::infallible(attribute_descriptor_sync(
            attribute,
            instance,
            owner,
            LookupFacts,
            &InlineAttributeDescriptor { db, env, recursion_guard },
        ));
        (result.member, result.kind, result.error, result.origin)
    }


    /// Returns whether this type is a data descriptor, i.e. defines `__set__` or `__delete__`.
    /// If this type is a union, requires all elements of union to be data descriptors.
    /// A directly dynamic type is treated as a data descriptor because it could inhabit one.
    fn is_data_descriptor(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        self.is_data_descriptor_impl(db, env.program(db), false)
    }

    /// Returns whether this type should be considered a possible data descriptor.
    /// If this type is a union, returns true if _any_ element is a data descriptor.
    /// This is used to determine whether an attribute assignment is valid for narrowing.
    /// For practical convenience, dynamic union elements are not considered possible data
    /// descriptors here, because doing so would disable narrowing too frequently.
    fn may_be_data_descriptor(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        self.is_data_descriptor_impl(db, env.program(db), true)
    }

    /// Returns whether this type is known not to be a data descriptor.
    ///
    /// Descriptor uncertainty propagates through outer unions, intersections, and aliases.
    /// `TypeForm` values and inexact `type[...]` values are also uncertain because their bounds
    /// describe the represented instance types, not the runtime values whose metaclasses determine
    /// descriptor behavior.
    fn is_definitely_non_data_descriptor(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
        self.is_definitely_non_data_descriptor_impl(db, env.program(db))
    }

    // Recursive aliases use `true`, the identity for the all-of classifications above.
    #[salsa::tracked(
        attempt = ReturnOnly,
        returns(copy),
        cycle_initial=|_, _, _, _| true,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn is_definitely_non_data_descriptor_impl(
        self,
        db: &'db dyn Db,
        program: Program<'db>,
    ) -> bool {
        let env = &ProgramEnvironment::from_program(program);
        match self {
            Type::Dynamic(_) | Type::Divergent(_) | Type::TypeVar(_) => false,
            Type::Union(union) => union
                .elements(db)
                .iter()
                .all(|ty| ty.is_definitely_non_data_descriptor_impl(db, program)),
            Type::Intersection(intersection) => intersection
                .iter_positive(db)
                .all(|ty| ty.is_definitely_non_data_descriptor_impl(db, program)),
            Type::TypeAlias(alias) => alias
                .value_type(db)
                .is_definitely_non_data_descriptor_impl(db, program),
            Type::Recursive(recursive) => recursive.unfold(db, env).is_unchanged_or(|unfolded| {
                unfolded.is_definitely_non_data_descriptor_impl(db, program)
            }),
            Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Type) => {
                false
            }
            Type::TypeForm(_) | Type::SubclassOf(_) => false,
            _ => !self.may_be_data_descriptor(db, env),
        }
    }

    fn is_data_descriptor_impl(
        self,
        db: &'db dyn Db,
        program: Program<'db>,
        any_of_union: bool,
    ) -> bool {
        is_data_descriptor_impl_(db, self, program, any_of_union)
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
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
        fallback: MemberLookupResult<'db>,
        policy: InstanceFallbackShadowsNonDataDescriptor,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> MemberLookupResult<'db> {
        match invoke_lookup_descriptor_sync(
            key,
            receiver,
            fallback,
            policy,
            LookupFacts,
            &InlineLookupDescriptor {
                db,
                env,
                recursion_guard,
            },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Access an attribute of this type, potentially invoking the descriptor protocol.
    /// Corresponds to `getattr(<object of type 'self'>, name)`.
    ///
    /// See also: [`Type::static_member`]
    ///
    #[must_use]
    fn member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        self.try_member_lookup(db, env, name)
            .unwrap_or_else(|error| error.fallback_member(db))
            .member(db)
    }

    /// Performs member lookup while retaining errors from implicit attribute-access methods.
    fn try_member_lookup(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> MemberLookupResult<'db> {
        self.member_lookup_with_policy_and_receiver(
            db,
            env,
            name,
            MemberLookupPolicy::default(),
            None,
        )
    }

    /// Whether class access exposes an instance attribute whose type depends on the class's
    /// type parameters. Specializing a class does not give it separate attribute storage:
    /// `Box[int].value` and `Box[str].value` both refer to `Box.value` at runtime.
    /// A `type[Box[int]]` receiver can refer to a concrete subclass with its own attributes,
    /// so this restriction only applies to class literals and generic aliases.
    fn has_generic_instance_attribute(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> bool {
        match generic_attribute::has_generic_instance_attribute_sync(
            self,
            name,
            &generic_attribute::OrdinaryGenericAttributeEffects::new(db, env),
        ) {
            Ok(has_attribute) => has_attribute,
            Err(never) => match never {},
        }
    }

    /// Similar to [`Type::member`], but allows the caller to specify what policy should be used
    /// when looking up attributes. See [`MemberLookupPolicy`] for more information.
    pub(crate) fn member_lookup_with_policy(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        self.member_lookup_with_policy_and_receiver(db, env, name, policy, None)
            .unwrap_or_else(|error| error.fallback_member(db))
            .member(db)
    }

    /// Perform member lookup while optionally binding descriptors and `Self` to a more precise
    /// receiver than the type whose members are being searched.
    ///
    /// Intersection member lookup searches each positive element separately, but the resulting
    /// attribute is still bound to the full intersection.
    fn member_lookup_with_policy_and_receiver(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> MemberLookupResult<'db> {
        self.member_lookup_with_recursion_guard(db, env, name, policy, receiver, None)
    }

    fn member_lookup_with_recursion_guard(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> MemberLookupResult<'db> {
        #[salsa::tracked(attempt = ReturnOnly,
            returns(copy),
            cycle_initial=|_, id, _, _| Place::bound(Type::divergent(id)).into(),
            cycle_fn=|db, cycle, previous: &MemberLookupResult<'db>, member: MemberLookupResult<'db>, key: MemberLookupKey<'db>, _| {
                cycle_normalized_member_lookup(db, &ProgramEnvironment::from_program(key.program(db)), member, *previous, cycle)
            },
            heap_size=ruff_memory_usage::heap_size
        )]
        fn member_lookup_with_policy_and_receiver_inner<'db>(
            db: &'db dyn Db,
            key: MemberLookupKey<'db>,
            receiver: Type<'db>,
        ) -> MemberLookupResult<'db> {
            member_lookup_with_policy_impl(db, key, Some(receiver), None)
        }

        match member_lookup::general::member_lookup_entry_sync(
            self,
            member_lookup::general::GeneralMemberName::Text(name),
            policy,
            receiver,
            member_lookup::general::GeneralMemberFacts,
            &member_lookup::general::InlineGeneralMemberEffects {
                db,
                env,
                recursion_guard,
                receiver_lookup: Some(member_lookup_with_policy_and_receiver_inner),
            },
        ) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }

    /// Return the type of `len()` on a type if it is known more precisely than `int`,
    /// or `None` otherwise.
    ///
    /// In the second case, the return type of `len()` in `typeshed` (`int`)
    /// is used as a fallback.
    fn len(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Type<'db>> {
        fn non_negative_int_literal<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
        ) -> Option<Type<'db>> {
            match ty {
                // TODO: Emit diagnostic for non-integers and negative integers
                Type::LiteralValue(literal) => match literal.kind() {
                    LiteralValueTypeKind::Int(value) => (value.as_i64() >= 0).then_some(ty),
                    LiteralValueTypeKind::Bool(value) => Some(Type::int_literal(i64::from(value))),
                    _ => None,
                },
                Type::Union(union) => union.try_map(db, env, |element| {
                    non_negative_int_literal(db, env, *element)
                }),
                _ => None,
            }
        }

        // Eagerly distribute over unions as a fast path to avoid building a large union of bound methods.
        if let Type::Union(union) = self {
            return union.try_map(db, env, |element| element.len(db, env));
        }

        if let Type::LiteralValue(literal) = self
            && let Some(length) = match literal.kind() {
                LiteralValueTypeKind::String(string) => Some(string.python_len(db)),
                LiteralValueTypeKind::Bytes(bytes) => Some(bytes.python_len(db)),
                _ => None,
            }
        {
            return i64::try_from(length).ok().map(Type::int_literal);
        }

        let return_ty = match self.try_call_dunder(
            db,
            env,
            "__len__",
            CallArguments::none(),
            TypeContext::default(),
        ) {
            Ok(bindings) => bindings.return_type(db, env),
            Err(CallDunderError::PossiblyUnbound { bindings, .. }) => bindings.return_type(db, env),

            // TODO: emit a diagnostic
            Err(CallDunderError::MethodNotAvailable) => return None,
            Err(CallDunderError::CallError(_, bindings, _)) => bindings.return_type(db, env),
        };

        non_negative_int_literal(db, env, return_ty)
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
    fn getitem_dunder_call(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        key: Option<&str>,
    ) -> Option<Type<'db>> {
        let key = key
            .map(|key| Type::string_literal(db, key))
            .unwrap_or(Type::unknown());

        match self
            .member_lookup_with_policy(
                db,
                env,
                "__getitem__",
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .place
        {
            Place::Defined(DefinedPlace {
                ty: getitem_method,
                definedness: Definedness::AlwaysDefined,
                ..
            }) => getitem_method
                .try_call(db, env, &CallArguments::positional([key]))
                .ok()
                .map(|bindings| bindings.return_type(db, env)),

            _ => None,
        }
    }

    /// Returns the key and value types of this object if it was unpacked using `**`,
    /// or `None` if the object does not support unpacking.
    fn unpack_keys_and_items(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<(Type<'db>, Type<'db>)> {
        let key_ty = match self
            .member_lookup_with_policy(db, env, "keys", MemberLookupPolicy::NO_INSTANCE_FALLBACK)
            .place
        {
            Place::Defined(DefinedPlace {
                ty: keys_method,
                definedness: Definedness::AlwaysDefined,
                ..
            }) => keys_method
                .try_call(db, env, &CallArguments::none())
                .ok()
                .and_then(|bindings| {
                    Some(
                        bindings
                            .return_type(db, env)
                            .try_iterate(db, env)
                            .ok()?
                            .homogeneous_element_type(db, env),
                    )
                })?,

            _ => return None,
        };

        let value_ty = self
            .getitem_dunder_call(db, env, None)
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
    fn bindings(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Bindings<'db> {
        self.bindings_impl(db, env, &CallableRecursionGuard::new())
    }

    fn bindings_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
    ) -> Bindings<'db> {
        self.bindings_with_recovery(db, env, recursion_guard, false)
    }

    fn bindings_from_descriptor(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> Bindings<'db> {
        let mut bindings = self.bindings_with_recovery(
            db,
            env,
            recursion_guard,
            origin.return_contains_recursive_recovery,
        );
        bindings.add_descriptor_origin(db, origin);
        bindings
    }

    fn bindings_with_recovery(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
        unknown_is_recovery: bool,
    ) -> Bindings<'db> {
        #[cfg(test)]
        if constructor::expansion_probe::stopped(db) {
            return Binding::single(self, Signature::unknown()).into();
        }
        match call::preparation::bindings_sync(
            db,
            env,
            self,
            unknown_is_recovery,
            call::preparation::BindingPreparationFacts,
            &call::preparation::OrdinaryBindingPreparationEffects { recursion_guard },
        ) {
            Ok(bindings) => bindings,
            Err(never) => match never {},
        }
    }

    fn known_class_literal_bindings(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassLiteral<'db>,
    ) -> Option<Bindings<'db>> {
        call::preparation::known_class::known_class_bindings(db, env, self, class)
    }

    // Build bindings for constructor calls by combining `__new__`/`__init__` signatures.
    // Returns fallback bindings for cases that intentionally keep bespoke call behavior.
    fn constructor_bindings(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        recursion_guard: &CallableRecursionGuard<'db>,
    ) -> Bindings<'db> {
        #[cfg(test)]
        if constructor::expansion_probe::active() {
            return constructor::expansion_probe::bindings(db, env, self, class)
                .unwrap_or_else(|_| Binding::single(self, Signature::unknown()).into());
        }
        constructor::effects::inline_result(constructor::bindings::constructor_bindings_with(
            db,
            env,
            self,
            class,
            recursion_guard,
            &call::bindings::InlineBindingsEffects { recursion_guard },
        ))
        .unwrap_or_else(|error| match error {
            #[cfg(test)]
            constructor::effects::ConstructorError::Incomplete(_) => {
                Binding::single(self, Signature::unknown()).into()
            }
        })
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
        env: &ProgramEnvironment<'db>,
        argument_types: &CallArguments<'_, 'db>,
    ) -> Result<Bindings<'db>, CallError<'db>> {
        self.try_call_with_recursion_guard(db, env, argument_types, None)
    }

    fn try_call_with_recursion_guard(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        argument_types: &CallArguments<'_, 'db>,
        recursion_guard: Option<&CallableRecursionGuard<'db>>,
    ) -> Result<Bindings<'db>, CallError<'db>> {
        let Ok(result) = call::invocation::invoke_sync(
            self,
            call::invocation::InvocationContext {
                db,
                env,
                arguments: argument_types,
            },
            recursion_guard,
            &call::invocation::OrdinaryInvocationEffects,
        );
        result
    }

    /// Look up a dunder method on the meta-type of `self` and call it.
    ///
    /// Returns an `Err` if the dunder method can't be called,
    /// or the given arguments are not valid.
    fn try_call_dunder(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        mut argument_types: CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        self.try_call_dunder_with_policy(
            db,
            env,
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
        env: &ProgramEnvironment<'db>,
        name: &str,
        argument_types: &mut CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        call::dunder::DunderCallRequest::implicit(self, name, tcx, policy).evaluate(
            db,
            env,
            argument_types,
        )
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
        env: &ProgramEnvironment<'db>,
        name: &str,
        argument_types: &CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        call::dunder::DunderCallRequest::on_class(self, name, tcx).evaluate(db, env, argument_types)
    }

    /// Return whether a custom `__getattribute__` could affect this lookup.
    ///
    /// Reusing the receiver class's existing MRO classification avoids interning a member-lookup
    /// key just to determine whether an override exists. Class objects use their metaclass instead.
    /// An unknown base can intercept a missing attribute or bypass a failing descriptor, but cannot
    /// invalidate a definitely defined member.
    fn custom_getattribute_may_affect_lookup(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        result: MemberLookupResult<'db>,
    ) -> bool {
        match member_lookup::finalization::custom_getattribute_affects_sync(
            self,
            result,
            member_lookup::finalization::MemberFinalizationFacts,
            &member_lookup::finalization::OrdinaryMemberFinalization { db, env },
        ) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }

    /// Apply `__getattr__` / `__getattribute__` fallback to an attribute-lookup result.
    ///
    /// A custom `__getattribute__` can intercept even an always-defined normal lookup result.
    /// Otherwise, an undefined or possibly-undefined result falls back to `__getattribute__` and
    /// then `__getattr__` on the meta-type of `self`.
    fn fallback_to_getattr(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &Name,
        result: MemberLookupResult<'db>,
        policy: MemberLookupPolicy,
    ) -> MemberLookupResult<'db> {
        match member_lookup::finalization::fallback_to_getattr_sync(
            self,
            name,
            result,
            policy,
            member_lookup::finalization::MemberFinalizationFacts,
            &member_lookup::finalization::OrdinaryMemberFinalization { db, env },
        ) {
            Ok(value) => value,
            Err(never) => match never {},
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
    fn flatten_typevars(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            Type::TypeVar(tvar) => match tvar.require_bound_or_constraints(db, env) {
                TypeVarBoundOrConstraints::UpperBound(bound) => bound.flatten_typevars(db, env),
                TypeVarBoundOrConstraints::Constraints(constraints) => {
                    constraints.as_type(db, env).flatten_typevars(db, env)
                }
            },
            Type::Union(union) => {
                // Flatten each element and rebuild through the union builder.
                UnionType::from_elements(
                    db,
                    env,
                    union
                        .elements(db)
                        .iter()
                        .map(|e| e.flatten_typevars(db, env)),
                )
            }
            Type::Intersection(intersection) => {
                // Flatten each positive element and rebuild through the intersection builder.
                let mut builder = IntersectionBuilder::new(db, env);
                for pos in intersection.positive(db) {
                    builder.add_positive_in_place(pos.flatten_typevars(db, env));
                }
                for neg in intersection.negative(db) {
                    builder.add_negative_in_place(neg.flatten_typevars(db, env));
                }
                builder.build()
            }
            // Don't descend into other types; only flatten top-level typevars.
            _ => self,
        }
    }

    /// Resolve the type of an `await …` expression where `self` is the type of the awaitable.
    fn try_await(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Type<'db>, AwaitError<'db>> {
        let await_result = self.try_call_dunder(
            db,
            env,
            "__await__",
            CallArguments::none(),
            TypeContext::default(),
        );
        match await_result {
            Ok(bindings) => {
                let return_type = bindings.return_type(db, env);
                Ok(return_type.generator_return_type(db, env).ok_or_else(|| {
                    AwaitError::InvalidReturnType(return_type, Box::new(bindings))
                })?)
            }
            Err(call_error) => Err(AwaitError::Call(call_error)),
        }
    }

    /// Extract the yield, send, and return types of a generator.
    fn generator_types(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mode: GeneratorTypeMode,
    ) -> Option<GeneratorTypes<'db>> {
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
            } else if matches!(mode, GeneratorTypeMode::IteratorDefaults)
                && (class.is_known(db, KnownClass::Iterator)
                    || class.is_known(db, KnownClass::AsyncIterator))
                && let [yield_ty] = specialization.types(db)
            {
                let none = Type::none(db, env);
                Some(GeneratorTypes {
                    yield_ty: Some(*yield_ty),
                    send_ty: Some(none),
                    return_ty: Some(none),
                })
            } else {
                None
            }
        };

        match self {
            Type::NominalInstance(instance) => instance
                .class(db, env)
                .iter_mro(db)
                .find_map(from_class_base),
            Type::ProtocolInstance(protocol) => protocol
                .class_origin(db)
                .and_then(|class| class.iter_mro(db).find_map(from_class_base))
                .map(|types| {
                    protocol
                        .materialization_kind(db)
                        .map_or(types, |kind| types.materialize(db, env, kind))
                }),
            Type::TypeAlias(alias) => alias.value_type(db).generator_types(db, env, mode),
            // A provisional recursive body may unfold to itself without exposing a generator.
            Type::Recursive(recursive) => recursive
                .unfold(db, env)
                .into_unfolded()?
                .generator_types(db, env, mode),
            Type::Union(union) => {
                let mut yield_builder = Some(UnionBuilder::new(db, env));
                let mut send_builder = Some(UnionBuilder::new(db, env));
                let mut return_builder = Some(UnionBuilder::new(db, env));

                for ty in union.elements(db) {
                    let gt = ty.generator_types(db, env, mode)?;
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
                let mut yield_builder = Some(IntersectionBuilder::new(db, env));
                let mut send_builder = Some(IntersectionBuilder::new(db, env));
                let mut return_builder = Some(IntersectionBuilder::new(db, env));
                let mut any_success = false;

                for ty in intersection.positive(db) {
                    let Some(gt) = ty.generator_types(db, env, mode) else {
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

    /// Extract explicit send constraints from a generator function's return annotation.
    ///
    /// An iterator annotation does not expose `send`, but its presence in a union must not
    /// discard the send constraints from other generator alternatives.
    fn generator_annotation_send_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        if let Some(union) = self.as_union_like(db) {
            let mut send_types = union
                .elements(db)
                .iter()
                .filter_map(|ty| ty.generator_annotation_send_type(db, env));
            let first = send_types.next()?;
            return Some(
                send_types
                    .fold(UnionBuilder::new(db, env).add(first), UnionBuilder::add)
                    .build(),
            );
        }

        self.generator_types(db, env, GeneratorTypeMode::GeneratorOnly)
            .and_then(|types| types.send_ty)
    }

    fn generator_return_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        self.generator_types(db, env, GeneratorTypeMode::IteratorDefaults)
            .and_then(|generator_types| generator_types.return_ty)
    }

    /// Find a delegated generator's send type that cannot accept `send_ty`.
    ///
    /// Check union members independently to preserve gradual assignability. Intersecting
    /// `list[int]` and `list[str]` would give `Never`, incorrectly rejecting `list[Any]`.
    fn incompatible_yield_from_send_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        send_ty: Type<'db>,
    ) -> Option<Type<'db>> {
        if let Some(union) = self.as_union_like(db) {
            return union
                .elements(db)
                .iter()
                .find_map(|ty| ty.incompatible_yield_from_send_type(db, env, send_ty));
        }

        let inner_send_ty = self
            .generator_types(db, env, GeneratorTypeMode::GeneratorOnly)
            .and_then(|generator_types| generator_types.send_ty)
            .unwrap_or_else(|| Type::none(db, env));
        (!send_ty.is_assignable_to(db, env, inner_send_ty)).then_some(inner_send_ty)
    }

    /// Return the instance approximation, discarding whether the projection is exact.
    ///
    /// Use this only when an over-approximation is sound, such as constructor inference or a
    /// source-side relation. Target-side subtype checks must use [`Self::to_instance`].
    #[must_use]
    fn to_instance_approximation(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        self.to_instance(db, env)
            .map(InstanceProjection::into_inner)
    }

    /// Project this class-object type into its instance type while preserving projection quality.
    #[must_use]
    fn to_instance(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<InstanceProjection<Type<'db>>> {
        match self {
            Type::Recursive(recursive) => recursive
                .unfold(db, env)
                .map(|unfolded| unfolded.to_instance(db, env))
                .unwrap_or(Some(InstanceProjection::Exact(self))),
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => {
                Some(InstanceProjection::Exact(self))
            }
            Type::ClassLiteral(class) => Some(InstanceProjection::OverApproximation(
                Type::instance(db, env, class.default_specialization(db)),
            )),
            Type::GenericAlias(alias) => Some(InstanceProjection::OverApproximation(
                Type::instance(db, env, ClassType::from(alias)),
            )),
            Type::SubclassOf(subclass_of_ty) => Some(InstanceProjection::Exact(
                subclass_of_ty.to_instance(db, env),
            )),
            Type::KnownInstance(KnownInstanceType::NewType(newtype)) => Some(
                InstanceProjection::OverApproximation(Type::NewTypeInstance(newtype)),
            ),
            Type::Union(union) => union.to_instance(db, env),
            // If there is no bound or constraints on a typevar `T`, `T: object` implicitly, which
            // has no instance type. Otherwise, synthesize a typevar with bound or constraints
            // mapped through `to_instance`.
            Type::TypeVar(bound_typevar) => bound_typevar
                .to_instance(db, env)
                .map(|projection| projection.map(Type::TypeVar)),
            Type::TypeAlias(alias) => alias.value_type(db).to_instance(db, env),
            Type::Intersection(intersection) => intersection.to_instance(db, env),
            // An instance of class `C` may itself have instances if `C` is a subclass of `type`.
            Type::NominalInstance(instance) => KnownClass::Type
                .to_class_literal(db, env)
                .to_class_type(db)
                .is_some_and(|type_class| {
                    instance.class(db, env).is_subclass_of(db, env, type_class)
                })
                .then_some(InstanceProjection::OverApproximation(Type::object())),
            Type::FunctionLiteral(_)
            | Type::Callable(..)
            | Type::KnownBoundMethod(_)
            | Type::BoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ProtocolInstance(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::ModuleLiteral(_)
            | Type::LiteralValue(_)
            | Type::BoundSuper(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_)
            | Type::EnumComplement(_)
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
    fn in_type_expression(
        &self,
        db: &'db dyn Db,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
        inference_flags: InferenceFlags,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        self.in_type_expression_impl(db, scope_id, typevar_binding_context, inference_flags)
    }

    pub(in crate::types) async fn in_type_expression_with<
        E: type_expression_conversion::TypeExpressionConversionEffects<'db>,
    >(
        &self,
        db: &'db dyn Db,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
        inference_flags: InferenceFlags,
        effects: &E,
    ) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, E::Error> {
        let _ = db;
        type_expression_conversion::in_type_expression_with(
            *self,
            scope_id,
            typevar_binding_context,
            inference_flags,
            type_expression_conversion::ConversionFacts,
            effects,
        )
        .await
    }

    fn in_type_expression_impl(
        &self,
        db: &'db dyn Db,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
        inference_flags: InferenceFlags,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        match type_expression_conversion::in_type_expression_sync(
            *self,
            scope_id,
            typevar_binding_context,
            inference_flags,
            type_expression_conversion::ConversionFacts,
            &type_expression_conversion::InlineConversion { db },
        ) {
            Ok(result) => result,
            Err(error) => match error {},
        }
    }

    fn in_type_expression_recursive(
        db: &'db dyn Db,
        recursive: RecursiveType<'db>,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
        inference_flags: InferenceFlags,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        let env = &ProgramEnvironment::from_scope(scope_id);
        recursive
            .unfold(db, env)
            .map(|unfolded| {
                unfolded.in_type_expression_impl(
                    db,
                    scope_id,
                    typevar_binding_context,
                    inference_flags,
                )
            })
            .unwrap_or(Ok(Type::Recursive(recursive)))
    }

    fn in_type_expression_union(
        db: &'db dyn Db,
        union: &UnionType<'db>,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
        inference_flags: InferenceFlags,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        let env = &ProgramEnvironment::from_scope(scope_id);
        let mut builder = UnionBuilder::new(db, env);
        let mut invalid_expressions = smallvec::SmallVec::default();
        for element in union.elements(db) {
            match element.in_type_expression_impl(
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

    /// The type `NoneType` / `None`
    pub fn none(db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        KnownClass::NoneType.to_instance(db, env)
    }

    /// Given a type that is assumed to represent an instance of a class,
    /// return a type that represents that class itself.
    ///
    /// Note: the return type of `type(obj)` is subtly different from this.
    /// See `Self::dunder_class` for more details.
    #[must_use]
    fn to_meta_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        self.to_meta_type_with_recursion(db, env, &TypeRecursionContext::default())
    }

    fn to_meta_type_with_recursion(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        context: &TypeRecursionContext<'db>,
    ) -> Type<'db> {
        fn to_meta_type_inner<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
            context: &TypeRecursionContext<'db>,
            visitor: &ActiveRecursionDetector<TypeAliasType<'db>>,
        ) -> Type<'db> {
            match ty {
                Type::Recursive(recursive) => recursive
                    .unfold(db, env)
                    .map(|unfolded| to_meta_type_inner(db, env, unfolded, context, visitor))
                    .into_type(),
                Type::RecursiveVar(_) => {
                    unreachable!("semantic operation on an unbound recursive variable")
                }
                Type::Never => Type::Never,
                Type::NominalInstance(instance) => instance.to_meta_type(db, env),
                Type::KnownInstance(known_instance) => known_instance.to_meta_type(db, env),
                Type::SpecialForm(special_form) => special_form.to_meta_type(db, env),
                Type::PropertyInstance(property) => {
                    property.instance_class(db).to_class_literal(db, env)
                }
                Type::SlotDescriptor(_) => {
                    KnownClass::MemberDescriptorType.to_class_literal(db, env)
                }
                Type::Union(union) => union.map(db, env, |ty| {
                    to_meta_type_inner(db, env, *ty, context, visitor)
                }),
                Type::TypeIs(_) | Type::TypeGuard(_) => KnownClass::Bool.to_class_literal(db, env),
                Type::TypeForm(_) => to_meta_type_inner(db, env, Type::object(), context, visitor),
                Type::LiteralValue(literal) => match class_selection::literal_meta_type_sync(
                    literal,
                    class_selection::LiteralFallbackFacts,
                    &class_selection::OrdinaryNominalSelection { db, env },
                ) {
                    Ok(meta_type) => meta_type,
                    Err(error) => match error {},
                },
                Type::FunctionLiteral(function) => {
                    function.runtime_class(db).to_class_literal(db, env)
                }
                Type::BoundMethod(_) => KnownClass::MethodType.to_class_literal(db, env),
                Type::KnownBoundMethod(method) => method.class().to_class_literal(db, env),
                Type::WrapperDescriptor(_) => {
                    KnownClass::WrapperDescriptorType.to_class_literal(db, env)
                }
                Type::DataclassDecorator(_) => KnownClass::FunctionType.to_class_literal(db, env),
                Type::Callable(callable) if let Some(class) = callable.runtime_class(db) => {
                    class.to_class_literal(db, env)
                }
                Type::Callable(_) | Type::DataclassTransformer(_) => {
                    KnownClass::Type.to_instance(db, env)
                }
                Type::ModuleLiteral(_) => KnownClass::ModuleType.to_class_literal(db, env),
                Type::TypeVar(bound_typevar) => {
                    SubclassOfType::from(db, env, SubclassOfInner::TypeVar(bound_typevar))
                }
                Type::ClassLiteral(class) => class.metaclass(db),
                Type::GenericAlias(alias) => ClassType::from(alias).metaclass(db),
                Type::SubclassOf(subclass_of_ty)
                    if let SubclassOfInner::TypeVar(typevar) = subclass_of_ty.subclass_of() =>
                {
                    // Transposition changes a type variable's bounds but preserves its bound
                    // identity. Guard by that identity so newly transposed instances still match.
                    context.meta_type.typevars.visit(
                        &(env.program(db), typevar.identity(db)),
                        || KnownClass::Type.to_instance(db, env),
                        || subclass_of_ty.to_meta_type_with_recursion(db, env, context),
                    )
                }
                Type::SubclassOf(subclass_of_ty) => {
                    subclass_of_ty.to_meta_type_with_recursion(db, env, context)
                }
                Type::Dynamic(dynamic) => {
                    SubclassOfType::from(db, env, SubclassOfInner::Dynamic(dynamic))
                }
                Type::Divergent(_) => ty,
                Type::Intersection(intersection) => {
                    if let Some(alternatives) = intersection.finite_alternative_union(db, env) {
                        to_meta_type_inner(db, env, alternatives, context, visitor)
                    } else {
                        // Negative constraints do not generally constrain classes: `int & ~Literal[0]`
                        // still has meta-type `type[int]`. Pure negations are bounded by `object`.
                        let mut builder = IntersectionBuilder::new(db, env);
                        for positive in intersection.positive_elements_or_object(db) {
                            builder.add_positive_in_place(to_meta_type_inner(
                                db, env, positive, context, visitor,
                            ));
                        }

                        // An exclusion can narrow a type variable's union bound to a definite class:
                        // `(T: C | None) & ~None` has meta-type `type[T] & type[C]`.
                        // If the remaining bound is a class object, retain its metaclass instead.
                        // Structural bounds need separate runtime-class handling (see `dunder_class`).
                        if !intersection.negative(db).is_empty()
                            && intersection
                                .iter_positive(db)
                                .any(|positive| matches!(positive, Type::TypeVar(_)))
                            && let Some(narrowed_bound) =
                                match intersection.with_expanded_typevars_and_newtypes(db, env) {
                                    bound @ (Type::NominalInstance(_)
                                    | Type::ClassLiteral(_)
                                    | Type::GenericAlias(_)) => Some(bound),
                                    bound @ Type::SubclassOf(subclass_of)
                                        if let SubclassOfInner::Class(_) =
                                            subclass_of.subclass_of() =>
                                    {
                                        Some(bound)
                                    }
                                    _ => None,
                                }
                        {
                            builder.add_positive_in_place(to_meta_type_inner(
                                db,
                                env,
                                narrowed_bound,
                                context,
                                visitor,
                            ));
                        }

                        builder.build()
                    }
                }
                Type::EnumComplement(complement) => to_meta_type_inner(
                    db,
                    env,
                    complement.remaining_literal_union(db, env),
                    context,
                    visitor,
                ),
                Type::AlwaysTruthy | Type::AlwaysFalsy => KnownClass::Type.to_instance(db, env),
                Type::BoundSuper(_) => KnownClass::Super.to_class_literal(db, env),
                // Class-member lookup on a protocol instance must use the protocol's nominal class.
                // The structural `type[Protocol]` view is exposed by `dunder_class` and explicit
                // `type[Protocol]` annotations instead.
                Type::ProtocolInstance(protocol) => protocol.to_nominal_meta_type(db, env),
                // `TypedDict` instances are instances of `dict` at runtime, but its important that we
                // understand a more specific meta type in order to correctly handle `__getitem__`.
                Type::TypedDict(typed_dict) => match typed_dict {
                    TypedDictType::Class(class) => SubclassOfType::from(db, env, class),
                    TypedDictType::Synthesized(_) => SubclassOfType::from(
                        db,
                        env,
                        todo_type!("TypedDict synthesized meta-type").expect_dynamic(),
                    ),
                },
                Type::TypeAlias(alias) => {
                    // A repeated specialization adds no new classes to a recursive union. Changing
                    // type arguments can introduce other classes, so use an unconstrained metatype.
                    // Do not cache results: a projection made while another alias is active can omit
                    // classes that are only encountered later in that alias's union.
                    visitor.visit(
                        &alias,
                        || Type::Never,
                        || {
                            context.meta_type.aliases.visit(
                                &(env.program(db), alias),
                                || KnownClass::Type.to_instance(db, env),
                                || {
                                    let project_alias = || {
                                        to_meta_type_inner(
                                            db,
                                            env,
                                            alias.value_type_with_recursion(db, Some(context)),
                                            context,
                                            visitor,
                                        )
                                    };
                                    // Identity analysis can itself expand aliases, so establish the
                                    // exact-alias guard before checking for growing specializations.
                                    if let TypeIdentity::GrowingTypeAlias(definition) =
                                        Type::TypeAlias(alias).to_type_identity(db)
                                    {
                                        context.meta_type.growing_aliases.visit(
                                            &(env.program(db), definition),
                                            || KnownClass::Type.to_instance(db, env),
                                            project_alias,
                                        )
                                    } else {
                                        project_alias()
                                    }
                                },
                            )
                        },
                    )
                }
                Type::NewTypeInstance(newtype) => {
                    to_meta_type_inner(db, env, newtype.concrete_base_type(db), context, visitor)
                }
            }
        }

        to_meta_type_inner(db, env, self, context, &ActiveRecursionDetector::default())
    }

    /// Get the type of the `__class__` attribute of this type.
    ///
    /// For most types, this is equivalent to the meta type of this type. `TypedDict` types return
    /// `type[dict[str, object]]`, because their inhabitants are instances of `dict` at runtime.
    /// Class-backed protocols return their structural `type[Protocol]` view.
    #[must_use]
    fn dunder_class(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            Type::Union(union) => union.map(db, env, |element| element.dunder_class(db, env)),
            Type::Intersection(intersection) => intersection
                .try_dunder_class(db, env)
                .unwrap_or_else(|| self.to_meta_type(db, env)),
            Type::ProtocolInstance(protocol) => protocol.to_meta_type(db, env),
            Type::TypedDict(_) => KnownClass::Dict
                .to_specialized_class_type(
                    db,
                    env,
                    &[KnownClass::Str.to_instance(db, env), Type::object()],
                )
                .map(Type::from)
                // Guard against user-customized typesheds with a broken `dict` class
                .unwrap_or_else(Type::unknown),
            _ => self.to_meta_type(db, env),
        }
    }

    #[must_use]
    fn apply_optional_specialization(
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

    /// Projects a member from its generic owner, applying the owner's specialization to both
    /// ordinary occurrences and the domain of any retained synthetic `Self` variable.
    ///
    /// Rewriting the `Self` domain is specific to this projection boundary. Inference and other
    /// ordinary specializations must preserve that domain as fixed evidence.
    fn apply_optional_owner_specialization_to_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Type<'db> {
        if let Some(specialization) = specialization {
            self.apply_specialization_impl(db, specialization, true)
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
    fn apply_specialization(
        self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Type<'db> {
        self.apply_specialization_impl(db, specialization, false)
    }

    /// Applies either an ordinary specialization or an enclosing-owner specialization.
    ///
    /// Both modes share the same leaf fast paths. They differ only in whether a retained synthetic
    /// `Self` domain is part of the substitution.
    fn apply_specialization_impl(
        self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
    ) -> Type<'db> {
        match inline_mapping_result(self.specialization_start_sync(
            db,
            specialization,
            specialize_self_domain,
            &InlineMappingEffects,
        )) {
            Some(ty) => ty,
            None => self.apply_specialization_inner(db, specialization, specialize_self_domain),
        }
    }

    /// Applies specialization's root fast paths; `None` requires the recursive mapping body.
    pub(crate) async fn specialization_start_with<
        E: mapping::specialization_start::SpecializationStartEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        mapping::specialization_start::specialization_start_with(
            db,
            self,
            specialization,
            specialize_self_domain,
            mapping::specialization_start::SpecializationStartFacts,
            effects,
        )
        .await
    }

    fn specialization_start_sync<
        E: mapping::specialization_start::SynchronousSpecializationStartEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        mapping::specialization_start::specialization_start_sync(
            db,
            self,
            specialization,
            specialize_self_domain,
            mapping::specialization_start::SpecializationStartFacts,
            effects,
        )
    }

    fn apply_specialization_inner(
        self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
    ) -> Type<'db> {
        apply_specialization_inner(db, self, specialization, specialize_self_domain)
    }

    fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        self.apply_type_mapping_impl(db, type_mapping, tcx, &ApplyTypeMappingVisitor::new(env))
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        inline_mapping_result(self.apply_type_mapping_sync(
            db,
            type_mapping,
            tcx,
            visitor,
            &InlineMappingEffects,
        ))
    }

    pub(crate) async fn apply_type_mapping_with<
        'a,
        E: mapping::effects::SharedMappingEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Failure> {
        mapping::effects::apply_type_mapping_with(&mapping::effects::MappingDriver {
            db,
            ty: self,
            mapping: type_mapping,
            tcx,
            visitor,
            effects,
        })
        .await
    }

    pub(crate) fn mapping_start_with<
        'a,
        'v,
        E: mapping::effects::SharedMappingStartEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &'v ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> impl Future<
        Output = Result<MappingStart<Type<'db>, TypeMappingContinuation<'v, 'db>>, E::Failure>,
    > {
        mapping::effects::start_type_mapping_with(
            db,
            self,
            type_mapping,
            tcx,
            visitor,
            effects,
            mapping::effects::MappingDispatchFacts,
        )
    }

    fn complete_mapping_with<E: mapping::effects::SharedMappingStartEffects<'db>>(
        self,
        effects: &E,
    ) -> impl Future<Output = Result<Self, E::Failure>> {
        mapping::effects::complete_mapping_with(self, effects)
    }

    pub(crate) fn apply_type_mapping_sync<
        'a,
        E: mapping::effects::SynchronousSharedMappingEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Failure> {
        mapping::effects::apply_type_mapping_sync(&mapping::effects::MappingDriver {
            db,
            ty: self,
            mapping: type_mapping,
            tcx,
            visitor,
            effects,
        })
    }

    pub(crate) fn mapping_start_sync<
        'a,
        'v,
        E: mapping::effects::SynchronousSharedMappingStartEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &'v ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<MappingStart<Type<'db>, TypeMappingContinuation<'v, 'db>>, E::Failure> {
        mapping::effects::start_type_mapping_sync(
            db,
            self,
            type_mapping,
            tcx,
            visitor,
            effects,
            mapping::effects::MappingDispatchFacts,
        )
    }

    fn complete_mapping_sync<E: mapping::effects::SynchronousSharedMappingStartEffects<'db>>(
        self,
        effects: &E,
    ) -> Result<Self, E::Failure> {
        mapping::effects::complete_mapping_sync(self, effects)
    }

    fn expand_union_paramspecs(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Type<'db>> {
        // Expand union-valued `ParamSpec`s before specializing a given callable.
        if let TypeMapping::ApplySpecialization(specialization)
        | TypeMapping::ApplySpecializationWithMaterialization { specialization, .. } =
            type_mapping
        {
            let effects = function::mapping::InlineFunctionMappingEffects;
            let mut candidates = legacy_inline(function::mapping::union_paramspec_candidates_with(
                db, self, specialization, &effects,
            ));
            let mut seen = FxHashSet::default();
            let mut union_paramspecs = Vec::new();
            while let Some((typevar, union)) = legacy_inline(candidates.next_with(db, specialization, &effects)) {
                if seen.insert(typevar.identity(db)) {
                    union_paramspecs.push((typevar, union));
                }
            }

            if !union_paramspecs.is_empty() {
                // Independent union-valued `ParamSpec`s produce a Cartesian product. Bound
                // the expansion to avoid exponential blowup.
                const MAX_PARAMSPEC_EXPANSION: usize = 64;

                let mut expanded_callables = UnionBuilder::new(db, visitor.env);
                let mut expansion_size = 1usize;
                for (_, union) in &union_paramspecs {
                    expansion_size = expansion_size.saturating_mul(union.elements(db).len());
                    if expansion_size > MAX_PARAMSPEC_EXPANSION {
                        return Some(Type::unknown());
                    }

                    if union.recursively_defined(db).is_yes() {
                        expanded_callables =
                            expanded_callables.or_recursively_defined(RecursivelyDefined::Yes);
                    }
                }

                return Some(visitor.visit(db, self, type_mapping, || {
                    let expanded_paramspecs = union_paramspecs
                        .iter()
                        .map(|(typevar, union)| {
                            union.elements(db).iter().map(move |ty| (*typevar, *ty))
                        })
                        .multi_cartesian_product();

                    for bindings in expanded_paramspecs {
                        // Override the specialization with a specific parameter-list assigned to
                        // each `ParamSpec` from the union expansion.
                        let specialization = ApplySpecialization::WithBindings {
                            specialization,
                            bindings: &bindings,
                        };

                        let mapping = match type_mapping {
                            TypeMapping::ApplySpecializationWithMaterialization {
                                materialization_kind,
                                ..
                            } => TypeMapping::ApplySpecializationWithMaterialization {
                                specialization,
                                materialization_kind: *materialization_kind,
                            },
                            _ => TypeMapping::ApplySpecialization(specialization),
                        };

                        // Use a fresh visitor, as the visitor cache does not distinguish
                        // between these specialization bindings.
                        let callable = self.apply_type_mapping(db, visitor.env, &mapping, tcx);
                        expanded_callables.add_in_place(callable);
                    }

                    expanded_callables.build()
                }));
            }
        }

        None
    }

    /// Locates any legacy `TypeVar`s in this type, and adds them to a set. This is used to build
    /// up a generic context from any legacy `TypeVar`s that appear in a function parameter list or
    /// `Generic` specialization.
    fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        let Ok(()) = legacy_typevars::find_legacy_typevars_with(
            db,
            env,
            self,
            binding_context,
            typevars,
            &legacy_typevars::InlineLegacyTypeVarEffects,
        );
    }

    fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        let Ok(()) = legacy_typevars::collect_with_visitor(
            db,
            env,
            self,
            binding_context,
            typevars,
            visitor,
            &legacy_typevars::InlineLegacyTypeVarEffects,
        );
    }

    /// Bind all unbound legacy type variables to the given context and then
    /// add all legacy typevars to the provided set.
    fn bind_and_find_all_legacy_typevars(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        self.apply_type_mapping(
            db,
            env,
            &TypeMapping::BindLegacyTypevars(
                binding_context
                    .map(BindingContext::Definition)
                    .unwrap_or(BindingContext::Synthetic(env.program(db))),
            ),
            TypeContext::default(),
        )
        .find_legacy_typevars(db, env, None, variables);
    }

    /// Replace default types in parameters of callables with `Unknown`.
    fn replace_parameter_defaults(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        self.apply_type_mapping(
            db,
            env,
            &TypeMapping::ReplaceParameterDefaults,
            TypeContext::default(),
        )
    }

    /// Returns the eagerly expanded type.
    /// In the case of recursive type aliases, this will diverge, so that part will be replaced with `Divergent`.
    fn expand_eagerly(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        self.expand_eagerly_(db, env.program(db))
    }

    #[salsa::tracked(attempt = ReturnOnly,
        returns(copy),
        cycle_initial=|_, id, _, _| Type::divergent(id),
        cycle_fn=|db, cycle, previous: &Type<'db>, value: Type<'db>, _, program| {
            value.cycle_normalized_impl(db, &ProgramEnvironment::from_program(program), *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    fn expand_eagerly_(self, db: &'db dyn Db, program: Program<'db>) -> Type<'db> {
        let env = &ProgramEnvironment::from_program(program);
        self.apply_type_mapping(
            db,
            env,
            &TypeMapping::EagerExpansion,
            TypeContext::default(),
        )
    }

    /// Return the string representation of this type when converted to string as it would be
    /// provided by the `__str__` method.
    ///
    /// When not available, this should fall back to the value of `[Type::repr]`.
    /// Note: this method is used in the builtins `format`, `print`, `str.format` and `f-strings`.
    #[must_use]
    fn str(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Int(_) | LiteralValueTypeKind::Bool(_) => self.repr(db, env),
                LiteralValueTypeKind::String(_) | LiteralValueTypeKind::LiteralString => *self,
                LiteralValueTypeKind::Enum(enum_literal) => Type::string_literal(
                    db,
                    compact_str::format_compact!(
                        "{enum_class}.{name}",
                        enum_class = enum_literal.enum_class(db).name(db),
                        name = enum_literal.name(db)
                    ),
                ),
                LiteralValueTypeKind::Bytes(_) => KnownClass::Str.to_instance(db, env),
            },
            Type::SpecialForm(special_form) => {
                Type::string_literal(db, special_form.to_compact_string())
            }
            Type::KnownInstance(known_instance) => {
                Type::string_literal(db, known_instance.repr(db, env).to_compact_string())
            }
            ty if ty.is_subtype_of(db, env, Type::literal_string()) => Type::literal_string(),
            Type::Intersection(intersection) => {
                if let Some(alternatives) = intersection.finite_alternative_union(db, env) {
                    alternatives.str(db, env)
                } else {
                    KnownClass::Str.to_instance(db, env)
                }
            }
            Type::EnumComplement(complement) => {
                complement.remaining_literal_union(db, env).str(db, env)
            }
            // TODO: handle more complex types
            _ => KnownClass::Str.to_instance(db, env),
        }
    }

    /// Return the string representation of this type as it would be provided by the  `__repr__`
    /// method at runtime.
    #[must_use]
    fn repr(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Int(number) => {
                    Type::string_literal(db, number.to_compact_string())
                }
                LiteralValueTypeKind::Bool(true) => Type::string_literal(db, "True"),
                LiteralValueTypeKind::Bool(false) => Type::string_literal(db, "False"),
                LiteralValueTypeKind::String(literal) => Type::string_literal(
                    db,
                    compact_str::format_compact!("'{}'", literal.value(db).escape_default()),
                ),
                LiteralValueTypeKind::LiteralString => Type::literal_string(),
                _ => KnownClass::Str.to_instance(db, env),
            },
            Type::SpecialForm(special_form) => Type::string_literal(db, &*special_form.to_string()),
            Type::KnownInstance(known_instance) => {
                Type::string_literal(db, known_instance.repr(db, env).to_compact_string())
            }
            // TODO: handle more complex types
            _ => KnownClass::Str.to_instance(db, env),
        }
    }

    /// Returns where this type is defined.
    ///
    /// It's the foundation for the editor's "Go to type definition" feature
    /// where the user clicks on a value and it takes them to where the value's type is defined.
    ///
    /// This method returns `None` for unions and most intersections because how these
    /// should be handled, especially when some variants don't have definitions, is
    /// specific to the call site. Exact singleton finite intersections delegate to
    /// their only alternative, since there is no ambiguity to preserve there.
    pub fn definition(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<TypeDefinition<'db>> {
        match self {
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            Self::BoundMethod(method) => method.func(db).definition(db, env),
            Self::FunctionLiteral(function) => {
                Some(TypeDefinition::Function(function.definition(db)))
            }
            Self::ModuleLiteral(module) => Some(TypeDefinition::Module(module.module(db))),
            Self::ClassLiteral(class_literal) => class_literal.type_definition(db),
            Self::GenericAlias(alias) => Some(TypeDefinition::StaticClass(alias.definition(db))),
            Self::NominalInstance(instance) => instance.class(db, env).type_definition(db),
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
                SubclassOfInner::Protocol(protocol) => {
                    protocol.class_origin(db)?.type_definition(db)
                }
                SubclassOfInner::TypeVar(bound_typevar) => Some(TypeDefinition::TypeVar(
                    bound_typevar.typevar(db).definition(db)?,
                )),
            },

            Self::TypeAlias(alias) => alias.value_type(db).definition(db, env),
            Self::Recursive(recursive) => recursive
                .unfold(db, env)
                .into_unfolded()?
                .definition(db, env),
            Self::NewTypeInstance(newtype) => Some(TypeDefinition::NewType(newtype.definition(db))),

            Self::PropertyInstance(property) => property
                .getter(db)
                .and_then(|getter| getter.definition(db, env))
                .or_else(|| {
                    property
                        .setter(db)
                        .and_then(|setter| setter.definition(db, env))
                })
                .or_else(|| {
                    property
                        .deleter(db)
                        .and_then(|deleter| deleter.definition(db, env))
                }),

            // Navigating to the type of `Slotted.value` should open the `MemberDescriptorType`
            // class in typeshed, rather than the slot's instance-value annotation.
            Self::SlotDescriptor(_) => KnownClass::MemberDescriptorType
                .to_instance(db, env)
                .definition(db, env),

            Self::LiteralValue(literal) => literal
                .as_enum()
                .and_then(|enum_lit| enum_lit.definition(db))
                .map(TypeDefinition::EnumMember)
                .or_else(|| self.to_meta_type(db, env).definition(db, env)),

            Self::KnownBoundMethod(_)
            | Self::WrapperDescriptor(_)
            | Self::DataclassDecorator(_)
            | Self::DataclassTransformer(_)
            | Self::BoundSuper(_) => self.to_meta_type(db, env).definition(db, env),

            Self::TypeVar(bound_typevar) => Some(TypeDefinition::TypeVar(
                bound_typevar.typevar(db).definition(db)?,
            )),

            Self::ProtocolInstance(protocol) => protocol
                .class_origin(db)
                .and_then(|class| class.type_definition(db)),

            Self::TypedDict(typed_dict) => typed_dict.type_definition(db),

            Self::Union(_) => None,
            Self::Intersection(intersection) => {
                let alternatives = intersection.finite_alternatives(db, env)?;
                let [alternative] = alternatives.as_slice() else {
                    return None;
                };
                alternative.definition(db, env)
            }
            Self::EnumComplement(complement) => {
                let alternatives = complement.remaining_literal_types(db, env);
                let [alternative] = alternatives.as_slice() else {
                    return None;
                };
                alternative.definition(db, env)
            }

            Self::SpecialForm(special_form) => special_form.definition(db, env),
            Self::Never => Type::SpecialForm(SpecialFormType::Never).definition(db, env),
            Self::Dynamic(DynamicType::Any) => {
                Type::SpecialForm(SpecialFormType::Any).definition(db, env)
            }
            Self::Dynamic(
                DynamicType::Unknown
                | DynamicType::UnknownGeneric(_)
                | DynamicType::UnknownLambdaParameter
                | DynamicType::AmbiguousOverload,
            ) => Type::SpecialForm(SpecialFormType::Unknown).definition(db, env),
            Self::Divergent(_) => Type::SpecialForm(SpecialFormType::Divergent).definition(db, env),
            Self::Dynamic(DynamicType::Todo(_)) => {
                Type::SpecialForm(SpecialFormType::Todo).definition(db, env)
            }
            Self::AlwaysTruthy => {
                Type::SpecialForm(SpecialFormType::AlwaysTruthy).definition(db, env)
            }
            Self::AlwaysFalsy => {
                Type::SpecialForm(SpecialFormType::AlwaysFalsy).definition(db, env)
            }

            // These types have no definition
            Self::Dynamic(
                DynamicType::InvalidConcatenateUnknown | DynamicType::UnspecializedTypeVar,
            )
            | Self::Callable(_)
            | Self::TypeIs(_)
            | Self::TypeGuard(_)
            | Self::TypeForm(_) => None,
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
                    .function(db)?
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
            Type::BoundMethod(bound_method) => Some(bound_method.function(db)?.spans(db)),
            _ => None,
        }
    }

    fn generic_origin(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<StaticClassLiteral<'db>> {
        match self {
            Type::GenericAlias(generic) => Some(generic.origin(db)),
            Type::NominalInstance(instance)
                if let ClassType::Generic(generic) = instance.class(db, env) =>
            {
                Some(generic.origin(db))
            }
            _ => None,
        }
    }

    /// Default-specialize all legacy typevars in this type.
    ///
    /// This is used when an implicit type alias is referenced without explicitly specializing it.
    fn default_specialize(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match type_expression_conversion::default_specialize_sync(
            self,
            env,
            &type_expression_conversion::InlineConversion { db },
        ) {
            Ok(ty) => ty,
            Err(error) => match error {},
        }
    }

    pub(in crate::types) async fn default_specialize_with<
        E: type_expression_conversion::DefaultTypeSpecializationEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let _ = db;
        type_expression_conversion::default_specialize_with(self, env, effects).await
    }

    fn from_truthiness(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        truthiness: Truthiness,
    ) -> Self {
        match truthiness {
            Truthiness::AlwaysTrue => Type::bool_literal(true),
            Truthiness::AlwaysFalse => Type::bool_literal(false),
            Truthiness::Ambiguous => KnownClass::Bool.to_instance(db, env),
        }
    }

    /// Return whether the negation of this type is a subtype of `target`, reusing `negated_cache`
    /// for type shapes whose negation must still be materialized.
    fn negation_is_subtype_of_cached(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        negated_cache: &mut Option<Type<'db>>,
    ) -> bool {
        match self {
            Type::Intersection(intersection) => {
                intersection.negation_is_subtype_of(db, env, target)
            }
            _ => {
                let negated = negated_cache.get_or_insert_with(|| self.negate(db, env));
                negated.is_subtype_of(db, env, target)
            }
        }
    }
}

impl<'db> IntersectionType<'db> {
    /// Return whether the negation of this intersection is a subtype of `target`.
    ///
    /// Applying De Morgan's law to an intersection produces a union. Checking each branch
    /// directly avoids constructing and simplifying that temporary union, which can be costly
    /// for the large intersections produced by repeated narrowing.
    fn negation_is_subtype_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
    ) -> bool {
        self.positive(db)
            .iter()
            .all(|positive| positive.negate(db, env).is_subtype_of(db, env, target))
            && self
                .negative(db)
                .iter()
                .all(|negative| negative.is_subtype_of(db, env, target))
    }
}

impl<'db> From<&Type<'db>> for Type<'db> {
    fn from(value: &Type<'db>) -> Self {
        *value
    }
}

impl<'db> VarianceInferable<'db> for Type<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        tracing::trace!(
            "Checking variance of '{tvar}' in `{ty:?}`",
            tvar = typevar.identity.name(db),
            ty = self.display(db, env),
        );

        let v = match self {
            Type::RecursiveVar(_) => {
                unreachable!("semantic operation on an unbound recursive variable")
            }
            Type::ClassLiteral(class_literal) => class_literal.variance_of(db, env, typevar),
            Type::Recursive(recursive) => recursive.variance_of(db, env, typevar),

            Type::FunctionLiteral(function_type) => {
                // TODO: do we need to replace self?
                function_type.variance_of(db, typevar)
            }

            Type::BoundMethod(method_type) => {
                // Function-backed methods contribute their bound signatures. Callable instances
                // also expose attributes through `__func__`, so preserve their full variance.
                if matches!(
                    method_type.func(db),
                    Type::FunctionLiteral(_) | Type::Callable(_)
                ) && let Some(signatures) = method_type.bound_signatures(db)
                {
                    signatures.variance_of(db, env, typevar)
                } else {
                    // A callable object's type does not include the additional receiver bound
                    // by classmethod, which is also exposed through `__self__`.
                    VarianceTerm::join(
                        db,
                        [
                            method_type.func(db).variance_of(db, env, typevar),
                            method_type.self_instance(db).variance_of(db, env, typevar),
                        ],
                    )
                }
            }

            Type::NominalInstance(nominal_instance_type) => {
                nominal_instance_type.variance_of(db, env, typevar)
            }
            Type::GenericAlias(generic_alias) => generic_alias.variance_of(db, env, typevar),
            Type::Callable(callable_type) => {
                callable_type.signatures(db).variance_of(db, env, typevar)
            }
            // A type variable is always covariant in itself.
            Type::TypeVar(other_typevar) if other_typevar.identity(db) == typevar => {
                // type variables are covariant in themselves
                TypeVarVariance::Covariant.into()
            }
            Type::ProtocolInstance(protocol_instance_type) => {
                protocol_instance_type.variance_of(db, env, typevar)
            }
            Type::TypedDict(typed_dict) => typed_dict.variance_of(db, env, typevar),
            // unions are covariant in their disjuncts
            Type::Union(union_type) => VarianceTerm::join(
                db,
                union_type
                    .elements(db)
                    .iter()
                    .map(|ty| ty.variance_of(db, env, typevar)),
            ),

            // Products are covariant in their conjuncts. For negative
            // conjuncts, they're contravariant. To see this, suppose we have
            // `B` a subtype of `A`. A value of type `~B` could be some non-`B`
            // `A`, and so is not assignable to `~A`. On the other hand, a value
            // of type `~A` excludes all `A`s, and thus all `B`s, and so _is_
            // assignable to `~B`.
            Type::Intersection(intersection_type) => VarianceTerm::join(
                db,
                intersection_type
                    .positive(db)
                    .iter()
                    .map(|ty| ty.variance_of(db, env, typevar))
                    .chain(intersection_type.negative(db).iter().map(|ty| {
                        ty.with_polarity(TypeVarVariance::Contravariant)
                            .variance_of(db, env, typevar)
                    })),
            ),
            Type::EnumComplement(complement) => complement
                .to_intersection(db, env)
                .variance_of(db, env, typevar),
            Type::PropertyInstance(property_instance_type) => VarianceTerm::join(
                db,
                [
                    Some(property_instance_type.instance_fallback(db, env)),
                    property_instance_type.getter(db),
                    property_instance_type.setter(db),
                    property_instance_type.deleter(db),
                ]
                .into_iter()
                .flatten()
                .map(|ty| ty.variance_of(db, env, typevar)),
            ),
            // A generic class can store another class's slot descriptor directly:
            //
            //     class Owner[T]:
            //         descriptor = Slotted[T].value
            //
            // The descriptor's value can be both read and written, so `Owner` is invariant in T.
            Type::SlotDescriptor(descriptor) => descriptor
                .value_type(db)
                .with_polarity(TypeVarVariance::Invariant)
                .variance_of(db, env, typevar),
            Type::SubclassOf(subclass_of_type) => subclass_of_type.variance_of(db, env, typevar),
            Type::TypeIs(type_is_type) => type_is_type.variance_of(db, env, typevar),
            Type::TypeGuard(type_guard_type) => type_guard_type.variance_of(db, env, typevar),
            Type::TypeForm(typeform_type) => typeform_type.variance_of(db, env, typevar),
            Type::KnownInstance(known_instance) => known_instance.variance_of(db, env, typevar),
            Type::TypeAlias(alias) => alias.variance_of(db, env, typevar),
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
            | Type::NewTypeInstance(_) => VarianceTerm::BIVARIANT,
        };

        tracing::trace!(
            "Result of variance of '{tvar}' in `{ty:?}` is `{v:?}`",
            tvar = typevar.identity.name(db),
            ty = self.display(db, env),
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
    /// Promote class literals recursively without promoting other literal types.
    ClassLiteralsOnly,
    /// Singleton-only promotion recursively descends through nominal instances
    /// without recursing into unions or non-nominal types.
    SingletonsOnly,
}

/// Returns the [`ClassLiteral`] that "owns" a `Self` typevar (i.e., the class from its upper bound).
fn self_typevar_owner_class_literal<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    bound_typevar: BoundTypeVarInstance<'db>,
) -> Option<ClassLiteral<'db>> {
    bound_typevar
        .typevar(db)
        .upper_bound(db, env)
        .and_then(|ty| ty.nominal_class(db, env))
        .map(|class| class.class_literal(db))
}

#[salsa::tracked(configuration = (pub(in crate::types) ClassMroLiteralsConfiguration), attempt = ReturnOnly, returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn class_mro_literals<'db>(
    db: &'db dyn Db,
    class_literal: ClassLiteral<'db>,
) -> Box<[ClassLiteral<'db>]> {
    #[cfg(test)]
    {
        if salsa::attempt_probe::is_incomplete(db) {
            return Box::default();
        }
        if constructor::expansion_probe::mro_effects_enabled() {
            return mro::collection::collect_class_literals_sync(
                db,
                class_literal,
                &mro::attempt::AttemptMroEffects::new(db),
            )
            .unwrap_or_default();
        }
    }
    match mro::collection::collect_class_literals_sync(
        db,
        class_literal,
        &mro::root::InlineMroRootEffects::new(db),
    ) {
        Ok(literals) => literals,
        Err(never) => match never {},
    }
}

/// Information needed to bind `Self` typevars to a concrete type.
///
/// A matching binding context permits replacement directly. Otherwise, a `Self` typevar is
/// bound if its owner class is in the MRO of the self type's class; an unresolved owner imposes
/// no restriction once the self type's class is known.
///
/// A supplied binding context also identifies Self variables to remove from a mapped signature's
/// generic context. With no binding context, matching uses class owners and the signature's generic
/// context is preserved.
#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct SelfBinding<'db> {
    ty: Type<'db>,
    class_literal: Option<ClassLiteral<'db>>,
    binding_context: Option<BindingContext<'db>>,
}

impl<'db> SelfBinding<'db> {
    fn self_type(&self) -> Type<'db> {
        self.ty
    }

    const fn binding_context(&self) -> Option<BindingContext<'db>> {
        self.binding_context
    }
}

impl<'db> SelfBinding<'db> {
    fn new(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        self_type: Type<'db>,
        binding_context: Option<BindingContext<'db>>,
    ) -> Self {
        legacy_inline(mapping::self_binding::prepare_with(
            db,
            env,
            self_type,
            binding_context,
            &mapping::self_binding::InlineSelfBindingEffects,
        ))
    }

    /// Returns whether `bound_typevar` should be replaced by this binding's concrete self type.
    fn should_bind(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bound_typevar: BoundTypeVarInstance<'db>,
    ) -> bool {
        legacy_inline(mapping::self_binding::should_bind_with(
            db,
            env,
            self,
            bound_typevar,
            &mapping::self_binding::InlineSelfBindingEffects,
        ))
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
    /// A structural substitution constructed only by the recursive-type binder.
    ApplyRecursiveSubstitution(RecursiveMapping<'db>),
    /// Replaces any literal types with their corresponding promoted type form (e.g. `Literal["string"]`
    /// to `str`, or `def _() -> int` to `Callable[[], int]`).
    Promote(PromotionMode, PromotionKind),
    /// Binds a legacy typevar with the generic context (class, function, type alias) that it is
    /// being used in.
    BindLegacyTypevars(BindingContext<'db>),
    /// Freshens typevars bound by a generic context occurrence by adding a shared delta.
    FreshenBoundTypeVars {
        generic_context: GenericContext<'db>,
        delta: u32,
    },
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
    RescopeReturnCallables(mapping::return_callables::ReturnCallableReplacements<'a, 'db>),
}

/// The generic-context operation required when mapping a function signature.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum SignatureGenericContextMapping<'a, 'db> {
    Preserve,
    Freshen { generic_context: GenericContext<'db>, delta: u32 },
    Specialize(ApplySpecialization<'a, 'db>),
    RemoveSelf(BindingContext<'db>),
    ReplaceSelf(Type<'db>),
}

impl<'db> TypeMapping<'_, 'db> {
    #[cfg(test)]
    fn for_specialization(
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
    ) -> Self {
        mapping::OwnedTypeMapping::Specialization {
            specialization,
            specialize_self_domain,
            materialization_kind: specialization.materialization_kind(db),
        }
        .into_mapping()
    }

    /// Selects the generic-context operation without reading or rebuilding its variables.
    pub(in crate::types) const fn signature_generic_context_mapping(
        &self,
    ) -> SignatureGenericContextMapping<'_, 'db> {
        match self {
            Self::FreshenBoundTypeVars { generic_context, delta } => SignatureGenericContextMapping::Freshen { generic_context: *generic_context, delta: *delta },
            Self::ApplySpecialization(specialization)
            | Self::ApplySpecializationWithMaterialization { specialization, .. } => {
                SignatureGenericContextMapping::Specialize(*specialization)
            }
            Self::BindSelf(binding) => match binding.binding_context() {
                Some(context) => SignatureGenericContextMapping::RemoveSelf(context),
                None => SignatureGenericContextMapping::Preserve,
            },
            Self::ReplaceSelf { new_upper_bound } => {
                SignatureGenericContextMapping::ReplaceSelf(*new_upper_bound)
            }
            Self::Promote(..)
            | Self::ApplyRecursiveSubstitution(_)
            | Self::BindLegacyTypevars(_)
            | Self::Materialize(_)
            | Self::ReplaceParameterDefaults
            | Self::EagerExpansion
            | Self::RescopeReturnCallables(_) => SignatureGenericContextMapping::Preserve,
        }
    }

    /// Update the generic context of a [`Signature`] according to the current type mapping
    fn update_signature_generic_context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        context: GenericContext<'db>,
    ) -> GenericContext<'db> {
        match self.signature_generic_context_mapping() {
            SignatureGenericContextMapping::Freshen { generic_context, delta } => {
                signatures::effects::legacy_inline(generics::signature_freshening::freshen_signature_context_with(
                    db, env, context, generic_context, delta, &generics::signature_freshening::InlineSignatureFreshening,
                ))
            }
            SignatureGenericContextMapping::Specialize(ApplySpecialization::ReturnCallables(replacements)) => {
                signatures::effects::legacy_inline(generics::return_callable_context::map_return_callable_context_with(
                    db, env, context, replacements, &generics::return_callable_context::InlineReturnCallableContext,
                ))
            }
            SignatureGenericContextMapping::Specialize(specialization) => {
                generics::signature_context::specialize_signature_context(db, env, context, specialization)
            }
            SignatureGenericContextMapping::Preserve => context,
            SignatureGenericContextMapping::RemoveSelf(binding_context) => {
                context.remove_self(db, Some(binding_context))
            }
            SignatureGenericContextMapping::ReplaceSelf(new_upper_bound) => GenericContext::from_typevar_instances(
                db,
                env,
                context.variables(db).map(|typevar| {
                    if typevar.typevar(db).is_self(db) {
                        BoundTypeVarInstance::synthetic_self(
                            db,
                            new_upper_bound,
                            typevar.binding_context(db),
                        )
                    } else {
                        typevar
                    }
                }),
            ),
        }
    }

    /// Whether this mapping changes when applied in contravariant positions.
    const fn is_polarity_sensitive(&self) -> bool {
        match self {
            TypeMapping::Materialize(_)
            | TypeMapping::ApplySpecializationWithMaterialization { .. }
            | TypeMapping::Promote(..) => true,
            TypeMapping::ApplySpecialization(_)
            | TypeMapping::ApplyRecursiveSubstitution(_)
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::FreshenBoundTypeVars { .. }
            | TypeMapping::BindSelf(..)
            | TypeMapping::ReplaceSelf { .. }
            | TypeMapping::ReplaceParameterDefaults
            | TypeMapping::EagerExpansion
            | TypeMapping::RescopeReturnCallables(_) => false,
        }
    }

    /// Returns a new `TypeMapping` that should be applied in contravariant positions.
    fn flip(&self) -> Self {
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
            | TypeMapping::ApplyRecursiveSubstitution(_)
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::FreshenBoundTypeVars { .. }
            | TypeMapping::BindSelf(..)
            | TypeMapping::ReplaceSelf { .. }
            | TypeMapping::ReplaceParameterDefaults
            | TypeMapping::EagerExpansion
            | TypeMapping::RescopeReturnCallables(_) => self.clone(),
        }
    }

    /// Whether this mapping rewrites type structure without semantic normalization.
    ///
    /// Binding and unfolding may traverse open recursive bodies, so neither inference queries
    /// nor semantic operations may run on the intermediate types.
    const fn is_structural(&self) -> bool {
        matches!(self, TypeMapping::ApplyRecursiveSubstitution(_))
    }
}

bitflags! {
    /// Metadata retained when recursive inference recovers to a divergent marker.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct DivergentFlags: u8 {
        /// The cycle comes from type alias inference. Value inference can also diverge,
        /// for example when an assignment feeds into the next iteration of a loop.
        const FROM_TYPE_ALIAS = 1 << 0;
    }
}

impl get_size2::GetSize for DivergentFlags {}

/// A type that is determined to be divergent during recursive type inference.
/// This type must never be eliminated by dynamic type reduction
/// (e.g. `Divergent` is assignable to `@Todo`, but `@Todo | Divergent` must not be reduced to `@Todo`).
/// Otherwise, type inference cannot converge properly.
/// For detailed properties of this type, see the unit test at the end of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DivergentType {
    /// The query ID that caused the cycle.
    id: salsa::Id,
    flags: DivergentFlags,
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
            flags: DivergentFlags::empty(),
            materialization: None,
        }
    }

    fn same_marker(self, other: Self) -> bool {
        self.id == other.id
    }

    const fn materialized(self, kind: MaterializationKind) -> Self {
        Self {
            materialization: Some(kind),
            ..self
        }
    }

    const fn materialization_kind(self) -> Option<MaterializationKind> {
        self.materialization
    }
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
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
    /// A provisional marker inferred for a lambda parameter before access to its declared type.
    UnknownLambdaParameter,
    /// A special variant that represents that `Unknown` was inferred due to an invalid use of
    /// `Concatenate` in a type expression.
    ///
    /// TODO: this is a bit of a hack. `infer_type_expression` should really return a `Result`;
    /// if it did, this variant wouldn't be necessary.
    InvalidConcatenateUnknown,
    /// A special variant that indicates the result of overload matching is ambiguous.
    /// Ref: <https://typing.python.org/en/latest/spec/overload.html#step-5>
    AmbiguousOverload,
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
}

impl DynamicType<'_> {
    fn recursive_type_normalized(self) -> Self {
        self
    }

    fn is_todo(&self) -> bool {
        matches!(self, Self::Todo(_))
    }

    const fn is_provisional_marker(self) -> bool {
        matches!(
            self,
            Self::UnspecializedTypeVar | Self::UnknownLambdaParameter
        )
    }
}

impl std::fmt::Display for DynamicType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DynamicType::Any => f.write_str("Any"),
            DynamicType::Unknown
            | DynamicType::UnknownGeneric(_)
            | DynamicType::UnknownLambdaParameter
            | DynamicType::InvalidConcatenateUnknown
            | DynamicType::AmbiguousOverload => f.write_str("Unknown"),
            DynamicType::UnspecializedTypeVar => f.write_str("UnspecializedTypeVar"),
            // `DynamicType::Todo`'s display should be explicit that is not a valid display of
            // any other type
            DynamicType::Todo(todo) => write!(f, "@Todo{todo}"),
        }
    }
}

bitflags! {
    /// Type qualifiers from annotations or synthesized member metadata.
    #[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Hash)]
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
        /// `typing_extensions.ReadOnly`, or a synthesized read-only class attribute.
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
                    "Only a single bit should be set \
                    when calling `TypeQualifiers::name` (got {self:?})"
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
#[derive(Clone, Debug, Copy, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct TypeAndQualifiers<'db> {
    inner: Type<'db>,
    origin: TypeOrigin,
    qualifiers: TypeQualifiers,
    provenance: Provenance<'db>,
}

impl<'db> TypeAndQualifiers<'db> {
    pub(crate) fn new(inner: Type<'db>, origin: TypeOrigin, qualifiers: TypeQualifiers) -> Self {
        Self {
            inner,
            origin,
            qualifiers,
            provenance: Provenance::Unknown,
        }
    }

    fn declared(inner: Type<'db>) -> Self {
        Self {
            inner,
            origin: TypeOrigin::Declared,
            qualifiers: TypeQualifiers::empty(),
            provenance: Provenance::Unknown,
        }
    }

    pub(crate) fn with_provenance(mut self, provenance: Provenance<'db>) -> Self {
        self.provenance = provenance;
        self
    }

    pub(crate) fn provenance(&self) -> Provenance<'db> {
        self.provenance
    }

    /// Forget about type qualifiers and only return the inner type.
    pub(crate) fn inner_type(&self) -> Type<'db> {
        self.inner
    }

    pub(crate) fn origin(&self) -> TypeOrigin {
        self.origin
    }

    /// Return `self` with an additional qualifier added to the set of qualifiers.
    fn with_qualifier(mut self, qualifier: TypeQualifiers) -> Self {
        self.qualifiers |= qualifier;
        self
    }

    /// Return the set of type qualifiers.
    pub(crate) fn qualifiers(&self) -> TypeQualifiers {
        self.qualifiers
    }

    fn map_type(&self, f: impl FnOnce(Type<'db>) -> Type<'db>) -> TypeAndQualifiers<'db> {
        TypeAndQualifiers {
            inner: f(self.inner),
            origin: self.origin,
            qualifiers: self.qualifiers,
            provenance: self.provenance,
        }
    }
}

/// Error struct providing information on type(s) that were deemed to be invalid
/// in a type expression context, and the type we should therefore fallback to
/// for the problematic type expression.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
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
        let db = context.db();
        let InvalidTypeExpressionError {
            fallback_type,
            invalid_expressions,
        } = self;
        let env = context.program_environment();
        for error in invalid_expressions {
            let Some(builder) = context.report_lint(&INVALID_TYPE_FORM, node) else {
                continue;
            };
            let diagnostic = builder.into_diagnostic(error.reason(db, env, flags));
            error.add_subdiagnostics(db, env, diagnostic, node);
        }
        fallback_type
    }
}

/// Enumeration of various types that are invalid in type-expression contexts
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
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
    /// Same for `ty_extensions._internal.ConstraintSet`
    ConstraintSet,
    /// Same for `ty_extensions._internal.ConstraintSetSolution`
    ConstraintSetSolution,
    /// Same for `ty_extensions._internal.GenericContext`
    GenericContext,
    /// Same for `ty_extensions._internal.Specialization`
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
    /// `typing.Self` cannot be used in type aliases.
    TypingSelfInTypeAlias,
    /// `typing.Self` cannot be used in metaclass definitions.
    TypingSelfInMetaclass,
    /// `typing.Self` cannot be used with an incompatible explicit method receiver.
    TypingSelfWithIncompatibleReceiver(BoundTypeVarInstance<'db>),
    /// Some types are always invalid in type expressions
    InvalidType(Type<'db>, ScopeId<'db>),
    InvalidBareParamSpec(TypeVarInstance<'db>),
    InvalidBareTypeVarTuple(TypeVarInstance<'db>),
}

impl<'db> InvalidTypeExpression<'db> {
    fn reason(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        flags: InferenceFlags,
    ) -> impl std::fmt::Display + 'db {
        struct Display<'db> {
            error: InvalidTypeExpression<'db>,
            db: &'db dyn Db,
            env: ProgramEnvironment<'db>,
            flags: InferenceFlags,
        }

        impl std::fmt::Display for Display<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let db = self.db;
                let location = self.flags.type_expression_context();

                match self.error {
                    InvalidTypeExpression::RequiresOneArgument(special_form) => write!(
                        f,
                        "`{special_form}` requires exactly one argument \
                        when used in a {location}",
                    ),
                    InvalidTypeExpression::RequiresArguments(special_form) => write!(
                        f,
                        "`{special_form}` requires at least one argument \
                        when used in a {location}",
                    ),
                    InvalidTypeExpression::RequiresTwoArguments(special_form) => write!(
                        f,
                        "`{special_form}` requires at least two arguments \
                        when used in a {location}",
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
                        "`ty_extensions._internal.ConstraintSet` \
                        is not allowed in {location}s",
                    ),
                    InvalidTypeExpression::ConstraintSetSolution => write!(
                        f,
                        "`ty_extensions._internal.ConstraintSetSolution` is not allowed \
                        in {location}s",
                    ),
                    InvalidTypeExpression::GenericContext => {
                        write!(
                            f,
                            "`ty_extensions._internal.GenericContext` is not allowed \
                            in {location}s"
                        )
                    }
                    InvalidTypeExpression::Specialization => write!(
                        f,
                        "`ty_extensions._internal.Specialization` \
                        is not allowed in {location}s",
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
                                "Type qualifier `{qualifier}` is not allowed \
                                in type expressions (only in annotation expressions, \
                                and only with exactly one argument)",
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
                    InvalidTypeExpression::TypingSelfInTypeAlias => {
                        f.write_str("`Self` cannot be used in a type alias")
                    }
                    InvalidTypeExpression::TypingSelfInMetaclass => {
                        f.write_str("`Self` cannot be used in a metaclass")
                    }
                    InvalidTypeExpression::TypingSelfWithIncompatibleReceiver(_) => f.write_str(
                        "`Self` requires `self: Self` \
                        or `cls: type[Self]` for annotated receivers",
                    ),
                    InvalidTypeExpression::InvalidType(Type::FunctionLiteral(function), _) => {
                        write!(
                            f,
                            "Function `{function}` is not valid in a {location}",
                            function = function.name(db)
                        )
                    }
                    InvalidTypeExpression::InvalidType(Type::ModuleLiteral(module), _) => write!(
                        f,
                        "Module `{module}` is not valid in a {location}",
                        module = module.module(db).name(db)
                    ),
                    InvalidTypeExpression::InvalidType(ty, _) => write!(
                        f,
                        "Variable of type `{ty}` is not allowed in a {location}",
                        ty = ty.display(db, &self.env)
                    ),
                    InvalidTypeExpression::InvalidBareParamSpec(paramspec) => write!(
                        f,
                        "Bare ParamSpec `{}` is not valid \
                        in this context in a {location}",
                        paramspec.name(db)
                    ),
                    InvalidTypeExpression::InvalidBareTypeVarTuple(typevartuple) => write!(
                        f,
                        "Bare TypeVarTuple `{}` is not valid \
                        in this context in a {location}",
                        typevartuple.name(db)
                    ),
                    InvalidTypeExpression::Concatenate => write!(
                        f,
                        "`typing.Concatenate` is not allowed \
                        in this context in a {location}",
                    ),
                }
            }
        }

        Display {
            error: self,
            db,
            env: env.clone(),
            flags,
        }
    }

    fn add_subdiagnostics(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
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
            let module_name_final_part = module.name(db).last_component();
            let Some(module_member_with_same_name) = ty
                .member(db, env, module_name_final_part)
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
            diagnostic.set_primary_annotation_message(format_args!(
                "Did you mean to use the module's member \
                `{module_name_final_part}.{module_name_final_part}`?"
            ));
            diagnostic.help(format_args!(
                "Replace with `{module_name_final_part}.{module_name_final_part}`"
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
                .map(|parent| parent.to_scope_id(db, function_body_scope.program_file(db)))
                == builtins_module_scope(db, env)
        {
            diagnostic.set_primary_annotation_message("Did you mean `collections.abc.Callable`?");
        } else if matches!(self, InvalidTypeExpression::InvalidBareParamSpec(_)) {
            diagnostic.info("A bare ParamSpec is only valid:");
            diagnostic.info(" - as the first argument to `Callable`");
            diagnostic.info(" - as the last argument to `Concatenate`");
            diagnostic.info(" - as the default type for another ParamSpec");
            diagnostic.info(" - as part of a type parameter list when defining a generic class");
            diagnostic.info(" - or as part of an argument list when specializing a generic class");
        } else if matches!(self, InvalidTypeExpression::InvalidBareTypeVarTuple(_)) {
            diagnostic.info("A TypeVarTuple must be unpacked with `*` or `Unpack[]`.");
        } else if matches!(self, InvalidTypeExpression::Concatenate) {
            diagnostic.info("`typing.Concatenate` is only valid:");
            diagnostic.info(" - as the first argument to `Callable`");
            diagnostic.info(" - as a type argument for a `ParamSpec` parameter");
        }
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
        let env = context.program_environment();

        let mut diag = builder.into_diagnostic(
            format_args!("`{type}` is not awaitable", type = context_expression_type.display(db, env)),
        );
        match self {
            Self::Call(CallDunderError::CallError(CallErrorKind::BindingError, bindings, _)) => {
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
                _,
                attribute_provenance,
            )) => {
                let possibly = if matches!(kind, CallErrorKind::PossiblyNotCallable) {
                    " possibly"
                } else {
                    ""
                };
                diag.info(format_args!("`__await__` is{possibly} not callable"));
                if let Some(definition) = attribute_provenance.definition() {
                    let module = parsed_module(db, definition.python_file(db)).load(db);
                    diag.annotate(
                        Annotation::secondary(definition.focus_range(db, &module).into())
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
                            ty.display(db, env)
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
                if let Some(type_definition) = context_expression_type.definition(db, env)
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
                    return_type = return_type.display(db, env)
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

#[salsa::interned(debug, field_requests=field_requests, heap_size=ruff_memory_usage::heap_size)]
pub struct ModuleLiteralType<'db> {
    /// The imported module.
    #[returns(copy)]
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
    #[returns(copy)]
    _importing_file: Option<ProgramFile<'db>>,
}

#[cfg(feature = "experimental-analysis")]
impl FiniteInternedConfiguration for ModuleLiteralType<'static> {
    fn field_work(_fields: &Self::Fields<'_>) -> Option<usize> {
        Some(3)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ModuleLiteralType<'_> {}

impl<'db> ModuleLiteralType<'db> {
    fn importing_file(self, db: &'db dyn Db) -> Option<ProgramFile<'db>> {
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
            .filter_map(|submodule_name| self.imported_submodule_attribute(db, submodule_name))
    }

    fn imported_submodule_attribute(self, db: &'db dyn Db, imported: &ModuleName) -> Option<Name> {
        Self::imported_submodule_attribute_from_name(self.module(db).name(db), imported)
    }

    fn imported_submodule_attribute_from_name(
        module: &ModuleName,
        imported: &ModuleName,
    ) -> Option<Name> {
        imported
            .relative_to(module)
            .and_then(|relative| relative.components().next().map(Name::from))
    }

    fn resolve_submodule(self, db: &'db dyn Db, name: &str) -> Option<Type<'db>> {
        let importing_file = self.importing_file(db)?;
        let relative_submodule_name = ModuleName::new(name)?;
        let mut absolute_submodule_name = self.module(db).name(db).clone();
        absolute_submodule_name.extend(&relative_submodule_name);
        let submodule = resolve_module(
            db,
            ImportingFile::File(
                importing_file.file(db),
                importing_file.resolver_environment(db),
            ),
            &absolute_submodule_name,
        )?;
        Some(Type::module_literal(db, importing_file, submodule))
    }

    /// Resolves a missing member through the module's `__getattr__` function.
    ///
    /// Invalid calls retain their declared return type for recovery while deferring the diagnostic
    /// until the caller determines whether the fallback actually takes precedence.
    ///
    /// ```python
    /// # example.py
    /// def __getattr__() -> str: ...
    ///
    /// # Another module:
    /// import example
    /// example.missing  # Invalid call; the recovery type is str.
    /// ```
    fn try_module_getattr(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> MemberLookupResult<'db> {
        if let Some(file) = self
            .module(db)
            .file(db)
            .map(|file| ProgramFile::new(db, file, env.program(db)))
            && let Place::Defined(place) =
                imported_symbol(db, env, Some(file), "__getattr__", None).place
        {
            let name_type = Type::string_literal(db, name);
            let (return_type, error) =
                match place
                    .ty
                    .try_call(db, env, &CallArguments::positional([name_type]))
                {
                    Ok(outcome) => (outcome.return_type(db, env), None),
                    Err(CallError(_, bindings)) => (
                        bindings.return_type(db, env),
                        Some(MemberLookupErrorKind::ModuleGetAttr {
                            callable: place.ty,
                            name: name_type,
                        }),
                    ),
                };

            return member_lookup_result(
                db,
                PlaceAndQualifiers {
                    place: Place::Defined(DefinedPlace {
                        ty: return_type,
                        provenance: Provenance::Unknown,
                        ..place
                    }),
                    qualifiers: TypeQualifiers::FROM_MODULE_GETATTR,
                },
                error,
                None,
            );
        }

        Place::Undefined.into()
    }

    /// Looks up a module member while preserving failed module-level `__getattr__` calls.
    ///
    /// The failed call and its recovery type are retained so direct attribute access and `from`
    /// imports can report the error after resolving lookup precedence.
    fn static_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> MemberLookupResult<'db> {
        legacy_inline(self.static_member_with(
            db,
            env,
            &module_member_effects::LegacyInlineEffects,
            name,
        ))
    }

    pub(in crate::types) async fn static_member_with<
        E: module_member_effects::ModuleMemberEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        effects: &E,
        name: &str,
    ) -> Result<MemberLookupResult<'db>, E::Error> {
        effects.checkpoint(db, env, self, name).await?;
        let module = effects.module(db, self).await?;
        // `__dict__` is a very special member that is never overridden by module globals;
        // we should always look it up directly as an attribute on `types.ModuleType`,
        // never in the global scope of the module.
        if name == "__dict__" {
            return effects.module_type_member(db, env, "__dict__").await;
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
        if effects.has_submodule_attribute(db, self, name).await?
            && let Some(submodule) = effects.resolve_submodule(db, self, name).await?
        {
            return Ok(Place::bound(submodule).into());
        }

        let file = match effects.module_file(db, module).await? {
            Some(file) => Some(effects.source_file(db, env, file).await?),
            None => None,
        };
        let place_and_qualifiers = effects.imported_symbol(db, env, file, name).await?;

        // If the normal lookup failed, try to call the module's `__getattr__` function
        if place_and_qualifiers.place.is_undefined() {
            return effects.module_getattr(db, env, self, name).await;
        }

        // typeshed re-exports some special forms across modules (e.g. `collections.abc.Callable`
        // is `from typing import Callable as Callable`). The resolved type still carries the
        // definition-site variant (`SpecialFormType::TypingCallable`), so we recover the
        // import-path identity here while it's still observable.
        if let Place::Defined(defined) = place_and_qualifiers.place
            && let Type::SpecialForm(special) = defined.ty
            && let Some(import_module) = effects
                .known_module(db, effects.module(db, self).await?)
                .await?
        {
            let rewrapped = special.rewrap_for_import_module(name, import_module);
            if rewrapped != special {
                return Ok(PlaceAndQualifiers {
                    place: Place::Defined(DefinedPlace {
                        ty: Type::SpecialForm(rewrapped),
                        ..defined
                    }),
                    qualifiers: place_and_qualifiers.qualifiers,
                }
                .into());
            }
        }

        Ok(place_and_qualifiers.into())
    }
}

/// Either the explicit `metaclass=` keyword of the class, or the inferred metaclass of one of its base classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct MetaclassCandidate<'db> {
    metaclass: ClassType<'db>,
    /// The base that supplied this candidate, including the `Protocol` pseudo-base,
    /// or `None` for the class's own metaclass.
    base: Option<ClassBase<'db>>,
}

/// Information about a `@dataclass_transform`-decorated metaclass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct MetaclassTransformInfo<'db> {
    params: DataclassTransformerParams<'db>,

    /// Whether the metaclass providing these parameters was declared on the class itself
    /// (via an explicit `metaclass=` keyword) rather than inherited from a base class.
    from_explicit_metaclass: bool,
}

#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeIsType<'db> {
    #[returns(copy)]
    type_argument: Type<'db>,
    /// The ID of the scope to which the place belongs
    /// and the ID of the place itself within that scope.
    #[returns(copy)]
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
    fn place_name(self, db: &'db dyn Db) -> Option<String> {
        let (scope, place) = self.place_info(db)?;
        let table = place_table(db, scope);

        Some(format!("{}", table.place(place)))
    }

    /// Construct an unbound `TypeIs` return type from the user-written type expression.
    ///
    /// ```python
    /// from typing import TypeIs
    ///
    /// def is_tuple(value: object) -> TypeIs[tuple[int, ...]]:
    ///     return isinstance(value, tuple)
    /// ```
    fn from_type_expression(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeIs(Self::new(db, ty, None))
    }

    fn return_type(self, db: &'db dyn Db) -> Type<'db> {
        self.type_argument(db)
    }

    #[must_use]
    fn bind(self, db: &'db dyn Db, scope: ScopeId<'db>, place: ScopedPlaceId) -> Type<'db> {
        Type::TypeIs(Self::new(db, self.type_argument(db), Some((scope, place))))
    }

    #[must_use]
    fn with_type(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeIs(Self::new(db, ty, self.place_info(db)))
    }

    fn is_bound(self, db: &'db dyn Db) -> bool {
        self.place_info(db).is_some()
    }
}

impl<'db> VarianceInferable<'db> for TypeIsType<'db> {
    // See the [typing spec] on why `TypeIs` is invariant in its type.
    // [typing spec]: https://typing.python.org/en/latest/spec/narrowing.html#typeis
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        self.type_argument(db)
            .with_polarity(TypeVarVariance::Invariant)
            .variance_of(db, env, typevar)
    }
}

#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeGuardType<'db> {
    #[returns(copy)]
    return_type: Type<'db>,
    /// The ID of the scope to which the place belongs
    /// and the ID of the place itself within that scope.
    #[returns(copy)]
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
    fn place_name(self, db: &'db dyn Db) -> Option<String> {
        let (scope, place) = self.place_info(db)?;
        let table = place_table(db, scope);

        Some(format!("{}", table.place(place)))
    }

    fn unbound(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeGuard(Self::new(db, ty, None))
    }

    fn bound(
        db: &'db dyn Db,
        return_type: Type<'db>,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
    ) -> Type<'db> {
        Type::TypeGuard(Self::new(db, return_type, Some((scope, place))))
    }

    #[must_use]
    fn bind(self, db: &'db dyn Db, scope: ScopeId<'db>, place: ScopedPlaceId) -> Type<'db> {
        Self::bound(db, self.return_type(db), scope, place)
    }

    #[must_use]
    fn with_type(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        Type::TypeGuard(Self::new(db, ty, self.place_info(db)))
    }

    fn is_bound(self, db: &'db dyn Db) -> bool {
        self.place_info(db).is_some()
    }
}

impl<'db> VarianceInferable<'db> for TypeGuardType<'db> {
    // `TypeGuard` is covariant in its type parameter. See the `TypeGuard`
    // section of mdtest/generics/pep695/variance.md for details.
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        self.return_type(db).variance_of(db, env, typevar)
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
///
/// Preserve the class's specialization so that a method on `Child[int]` has a bound such as
/// `Base[int]`, rather than retaining the type variable in `Base[T@Child]`.
pub(super) fn determine_upper_bound<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
    is_known_base: impl Fn(ClassBase<'db>) -> bool,
) -> Type<'db> {
    let upper_bound = class
        .iter_mro(db)
        .take_while(|base| !is_known_base(*base))
        .filter_map(ClassBase::into_class)
        .last()
        .unwrap_or(class);
    Type::instance(db, env, upper_bound)
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

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousTypeDispatchEffects)]
pub(in crate::types) trait TypeDispatchEffects<'db> {
 type Error;
 #[operation(checkpoint)]
 async fn checkpoint(&self) -> Result<(), Self::Error>;
 #[operation(child)]
 async fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
 #[operation(child)]
 async fn newtype_union(&self, newtype: NewType<'db>) -> Result<Option<UnionType<'db>>, Self::Error>;
 #[operation(child)]
 async fn collect_properties(&self, ty: Type<'db>) -> Result<Option<PropertyDeprecations<'db>>, Self::Error>;
}
#[synchronous(union_like_sync)]
#[capabilities(effects = TypeDispatchEffects)]
#[passive_values(Type::Union, Type::NewTypeInstance)]
pub(in crate::types) async fn union_like_with<'db, E: TypeDispatchEffects<'db>>(ty: Type<'db>, effects: &E) -> Result<Option<UnionType<'db>>, E::Error> {
 effects.checkpoint().await?;
 let ty = effects.resolve_alias(ty).await?;
 match ty {
  Type::Union(union) => Ok(Some(union)),
  Type::NewTypeInstance(newtype) => effects.newtype_union(newtype).await,
  _ => Ok(None),
 }
}
#[synchronous(property_metadata_sync)]
#[capabilities(effects = TypeDispatchEffects)]
#[passive_values(Type::PropertyInstance, Type::Union, Type::Intersection)]
pub(in crate::types) async fn property_metadata_with<'db, E: TypeDispatchEffects<'db>>(ty: Type<'db>, effects: &E) -> Result<Option<PropertyDeprecations<'db>>, E::Error> {
 effects.checkpoint().await?;
 match ty {
  Type::PropertyInstance(_) | Type::Union(_) | Type::Intersection(_) => effects.collect_properties(ty).await,
  _ => Ok(None),
 }
}
}
struct InlineTypeDispatch<'db> {
    db: &'db dyn Db,
}
impl<'db> SynchronousTypeDispatchEffects<'db> for InlineTypeDispatch<'db> {
    type Error = std::convert::Infallible;
    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.resolve_type_alias(self.db))
    }
    fn newtype_union(&self, newtype: NewType<'db>) -> Result<Option<UnionType<'db>>, Self::Error> {
        Ok(newtype.concrete_base_type(self.db).as_union_like(self.db))
    }
    fn collect_properties(
        &self,
        ty: Type<'db>,
    ) -> Result<Option<PropertyDeprecations<'db>>, Self::Error> {
        Ok(ty.collect_property_deprecations(self.db))
    }
}

/// A descriptor-applied attribute, retaining the original place metadata and any call recovery.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct AttributeDescriptorResult<'db> {
    pub(in crate::types) member: PlaceAndQualifiers<'db>,
    pub(in crate::types) kind: AttributeKind,
    pub(in crate::types) error: Option<DescriptorGetCallContext<'db>>,
    pub(in crate::types) origin: DescriptorOrigin<'db>,
}

ty_mapping_probe_macros::shared_semantic_family! {
/// Supplies descriptor evaluation while the shared wrapper preserves attribute metadata.
#[synchronous(SynchronousAttributeDescriptorEffects)]
pub(in crate::types) trait AttributeDescriptorEffects<'db> {
    type Error;
    #[operation(checkpoint)]
    async fn checkpoint(&self) -> Result<(), Self::Error>;
    #[operation(child)]
    async fn descriptor(&self, request: descriptor::DescriptorRequest<'db>) -> Result<descriptor::DescriptorResult<'db>, Self::Error>;
}
#[finite_capability]
impl LookupFacts {
    fn attribute_raw_type<'db>(&self, member: PlaceAndQualifiers<'db>) -> Option<Type<'db>> { member.place.ignore_possibly_undefined() }
    fn attribute_request<'db>(&self, ty: Type<'db>, instance: Option<Type<'db>>, owner: Type<'db>) -> descriptor::DescriptorRequest<'db> {
        descriptor::DescriptorRequest { ty, instance, owner }
    }
    fn unchanged_attribute<'db>(&self, member: PlaceAndQualifiers<'db>) -> AttributeDescriptorResult<'db> {
        AttributeDescriptorResult { member, kind: AttributeKind::NormalOrNonDataDescriptor, error: None, origin: DescriptorOrigin::default() }
    }
    fn applied_attribute<'db>(&self, member: PlaceAndQualifiers<'db>, result: descriptor::DescriptorResult<'db>) -> AttributeDescriptorResult<'db> {
        let (result, error) = match result {
            Ok(result) => (result, None),
            Err(failure) => (Some(failure.fallback()), Some(failure.context)),
        };
        match result {
            None => self.unchanged_attribute(member),
            Some(result) => AttributeDescriptorResult {
                member: member.map_type(|_| result.return_type),
                kind: result.kind,
                error,
                origin: result.origin,
            },
        }
    }
}
/// Applies a descriptor to a defined attribute and retains place metadata and call recovery.
///
/// Undefined attributes do not invoke a descriptor. A missing descriptor keeps the attribute;
/// a failed descriptor call contributes its recovery value and error context together.
#[synchronous(attribute_descriptor_sync)]
#[capabilities(effects = AttributeDescriptorEffects, facts = LookupFacts)]
#[passive_values()]
pub(in crate::types) async fn attribute_descriptor_with<'db, E: AttributeDescriptorEffects<'db>>(
    attribute: PlaceAndQualifiers<'db>, instance: Option<Type<'db>>, owner: Type<'db>, facts: LookupFacts, effects: &E,
) -> Result<AttributeDescriptorResult<'db>, E::Error> {
    effects.checkpoint().await?;
    let Some(ty) = facts.attribute_raw_type(attribute) else { return Ok(facts.unchanged_attribute(attribute)); };
    let result = effects.descriptor(facts.attribute_request(ty, instance, owner)).await?;
    Ok(facts.applied_attribute(attribute, result))
}
}

/// Ordinary effects adapter for [`Type::try_call_dunder_get_with_recursion_guard`],
/// retaining the caller's environment and optional callable guard.
struct InlineAttributeDescriptor<'env, 'guard, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
    recursion_guard: Option<&'guard CallableRecursionGuard<'db>>,
}

impl<'db> SynchronousAttributeDescriptorEffects<'db> for InlineAttributeDescriptor<'_, '_, 'db> {
    type Error = std::convert::Infallible;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn descriptor(
        &self,
        request: descriptor::DescriptorRequest<'db>,
    ) -> Result<descriptor::DescriptorResult<'db>, Self::Error> {
        Ok(request.ty.try_call_dunder_get_with_recursion_guard(
            self.db,
            self.env,
            request.instance,
            request.owner,
            self.recursion_guard,
        ))
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct LookupParts<'db> {
    member: PlaceAndQualifiers<'db>,
    error: Option<MemberLookupErrorKind<'db>>,
    properties: Option<PropertyDeprecations<'db>>,
    descriptor: DescriptorOrigin<'db>,
}

#[derive(Clone, Copy)]
pub(in crate::types) struct LookupFacts;

pub(in crate::types) enum DescriptorSelection<'db> {
    OriginalFallback,
    Selected(LookupParts<'db>),
    Union {
        meta: LookupParts<'db>,
        fallback: LookupParts<'db>,
        definedness: Definedness,
    },
}

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousLookupDescriptorEffects)]
pub(in crate::types) trait LookupDescriptorEffects<'db> {
    type Error;
    #[operation(checkpoint)]
    async fn checkpoint(&self) -> Result<(), Self::Error>;
    #[operation(local)]
    async fn fallback_parts(&self, result: MemberLookupResult<'db>) -> Result<LookupParts<'db>, Self::Error>;
    #[operation(child)]
    async fn class_attribute(&self, key: MemberLookupKey<'db>, receiver: Type<'db>) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    #[operation(child)]
    async fn owner(&self, receiver: Type<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(child)]
    async fn descriptor(&self, request: descriptor::DescriptorRequest<'db>) -> Result<descriptor::DescriptorResult<'db>, Self::Error>;
    #[operation(child)]
    async fn properties(&self, ty: Type<'db>) -> Result<Option<PropertyDeprecations<'db>>, Self::Error>;
    #[operation(child)]
    async fn union(&self, first: Type<'db>, second: Type<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(child)]
    async fn merge_properties(&self, first: Option<PropertyDeprecations<'db>>, second: Option<PropertyDeprecations<'db>>) -> Result<Option<PropertyDeprecations<'db>>, Self::Error>;
    #[operation(child)]
    async fn merge_origins(&self, first: DescriptorOrigin<'db>, second: DescriptorOrigin<'db>) -> Result<DescriptorOrigin<'db>, Self::Error>;
    #[operation(local)]
    async fn result(&self, parts: LookupParts<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
}
#[finite_capability]
impl LookupFacts {

    fn raw_type<'db>(&self, member: PlaceAndQualifiers<'db>) -> Option<Type<'db>> { member.place.ignore_possibly_undefined() }
    fn request<'db>(&self, ty: Type<'db>, receiver: Type<'db>, owner: Type<'db>) -> descriptor::DescriptorRequest<'db> { descriptor::DescriptorRequest { ty, instance: Some(receiver), owner } }
    fn attribute<'db>(&self, member: PlaceAndQualifiers<'db>, result: descriptor::DescriptorResult<'db>, properties: Option<PropertyDeprecations<'db>>) -> (LookupParts<'db>, AttributeKind) {
        let (result, error) = match result {
            Ok(result) => (result, None),
            Err(failure) => (Some(failure.fallback()), Some(MemberLookupErrorKind::DescriptorGet(failure.context))),
        };
        match result {
            None => (LookupParts { member, error: None, properties, descriptor: DescriptorOrigin::default() }, AttributeKind::NormalOrNonDataDescriptor),
            Some(result) => (LookupParts { member: member.map_type(|_| result.return_type), error, properties, descriptor: result.origin }, result.kind),
        }
    }
    fn absent_attribute<'db>(&self, member: PlaceAndQualifiers<'db>) -> (LookupParts<'db>, AttributeKind) {
        (LookupParts { member, error: None, properties: None, descriptor: DescriptorOrigin::default() }, AttributeKind::NormalOrNonDataDescriptor)
    }
    // A slot exposes the receiver's existing storage, whose declaration can be more precise.
    fn selection<'db>(&self, meta: LookupParts<'db>, kind: AttributeKind, meta_type: Option<Type<'db>>, fallback: LookupParts<'db>, policy: InstanceFallbackShadowsNonDataDescriptor) -> DescriptorSelection<'db> {
        if matches!(meta.member.place, Place::Defined(_)) && matches!(meta_type, Some(Type::SlotDescriptor(_))) && !fallback.member.place.is_undefined() {
            return DescriptorSelection::OriginalFallback;
        }
        match (meta.member.place, kind, fallback.member.place) {
            // Without instance storage, the class attribute determines the result.
            (Place::Defined(_), _, Place::Undefined) => DescriptorSelection::Selected(meta),
            // A definitely defined data descriptor takes precedence over instance storage.
            (Place::Defined(DefinedPlace { definedness: Definedness::AlwaysDefined, .. }), AttributeKind::DataDescriptor, _) => DescriptorSelection::Selected(meta),
            // A possibly undefined data descriptor can fall through to instance storage.
            (Place::Defined(DefinedPlace { definedness: Definedness::PossiblyUndefined, .. }), AttributeKind::DataDescriptor, Place::Defined(fallback_member)) => DescriptorSelection::Union { meta, fallback, definedness: fallback_member.definedness },
            // Class attributes can shadow non-data metaclass descriptors. For instances, keep
            // both alternatives because declaration presence does not prove storage was assigned.
            (Place::Defined(_), AttributeKind::NormalOrNonDataDescriptor, Place::Defined(DefinedPlace { definedness: Definedness::AlwaysDefined, .. })) if policy == InstanceFallbackShadowsNonDataDescriptor::Yes => DescriptorSelection::Selected(fallback),
            (Place::Defined(meta_member), AttributeKind::NormalOrNonDataDescriptor, Place::Defined(fallback_member)) => DescriptorSelection::Union { meta, fallback, definedness: meta_member.definedness.max(fallback_member.definedness) },
            (Place::Undefined, _, _) => DescriptorSelection::Selected(fallback),
        }
    }
    fn union_types<'db>(&self, meta: LookupParts<'db>, fallback: LookupParts<'db>) -> Option<(Type<'db>, Type<'db>)> {
        Some((meta.member.place.raw_type()?, fallback.member.place.raw_type()?))
    }
    fn merged<'db>(&self, meta: LookupParts<'db>, fallback: LookupParts<'db>, definedness: Definedness, ty: Type<'db>, properties: Option<PropertyDeprecations<'db>>, descriptor: DescriptorOrigin<'db>) -> LookupParts<'db> {
        match (meta.member.place, fallback.member.place) {
            (Place::Defined(meta_member), Place::Defined(fallback_member)) => LookupParts {
                member: Place::Defined(DefinedPlace { ty, origin: meta_member.origin.merge(fallback_member.origin), definedness, public_type_policy: fallback_member.public_type_policy, provenance: fallback_member.provenance.or(meta_member.provenance) }).with_qualifiers(meta.member.qualifiers.union(fallback.member.qualifiers)),
                error: meta.error.or(fallback.error), properties, descriptor,
            },
            _ => fallback,
        }
    }
}
#[synchronous(invoke_lookup_descriptor_sync)]
#[capabilities(effects = LookupDescriptorEffects, facts = LookupFacts)]
#[passive_values(DescriptorSelection::OriginalFallback, DescriptorSelection::Selected, DescriptorSelection::Union)]
pub(in crate::types) async fn invoke_lookup_descriptor_with<'db, E: LookupDescriptorEffects<'db>>(
    key: MemberLookupKey<'db>, receiver: Type<'db>, fallback: MemberLookupResult<'db>, policy: InstanceFallbackShadowsNonDataDescriptor, facts: LookupFacts, effects: &E,
) -> Result<MemberLookupResult<'db>, E::Error> {
    effects.checkpoint().await?;
    let attribute = effects.class_attribute(key, receiver).await?;
    effects.checkpoint().await?;
    let meta_type = facts.raw_type(attribute);
    let owner = effects.owner(receiver).await?;
    let (meta, kind) = match meta_type {
        Some(ty) => {
            let result = effects.descriptor(facts.request(ty, receiver, owner)).await?;
            let properties = effects.properties(ty).await?;
            effects.checkpoint().await?;
            facts.attribute(attribute, result, properties)
        }
        None => facts.absent_attribute(attribute),
    };
    effects.checkpoint().await?;
    let fallback_parts = effects.fallback_parts(fallback).await?;
    match facts.selection(meta, kind, meta_type, fallback_parts, policy) {
        DescriptorSelection::OriginalFallback => Ok(fallback),
        DescriptorSelection::Selected(parts) => effects.result(parts).await,
        DescriptorSelection::Union { meta, fallback: fallback_parts, definedness } => {
            match facts.union_types(meta, fallback_parts) {
                Some((first, second)) => {
                    let ty = effects.union(first, second).await?;
                    let properties = effects.merge_properties(meta.properties, fallback_parts.properties).await?;
                    let descriptor = effects.merge_origins(meta.descriptor, fallback_parts.descriptor).await?;
                    effects.checkpoint().await?;
                    effects.result(facts.merged(meta, fallback_parts, definedness, ty, properties, descriptor)).await
                }
                None => effects.result(fallback_parts).await,
            }
        }
    }
}
}

struct InlineLookupDescriptor<'env, 'guard, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
    recursion_guard: Option<&'guard CallableRecursionGuard<'db>>,
}
impl<'db> SynchronousLookupDescriptorEffects<'db> for InlineLookupDescriptor<'_, '_, 'db> {
    type Error = std::convert::Infallible;
    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn fallback_parts(
        &self,
        result: MemberLookupResult<'db>,
    ) -> Result<LookupParts<'db>, Self::Error> {
        Ok(LookupFacts.parts(salsa::FieldReads::new(self.db), result))
    }
    fn class_attribute(
        &self,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(Type::instance_lookup_class_member_with_policy(
            self.db, self.env, key, receiver,
        ))
    }
    fn owner(&self, receiver: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(receiver.to_meta_type(self.db, self.env))
    }
    fn descriptor(
        &self,
        request: descriptor::DescriptorRequest<'db>,
    ) -> Result<descriptor::DescriptorResult<'db>, Self::Error> {
        Ok(descriptor::evaluate_entry(
            self.db,
            self.env,
            request,
            self.recursion_guard,
        ))
    }
    fn properties(&self, ty: Type<'db>) -> Result<Option<PropertyDeprecations<'db>>, Self::Error> {
        Ok(ty.property_deprecations(self.db))
    }
    fn union(&self, first: Type<'db>, second: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(UnionType::from_two_elements(
            self.db, self.env, first, second,
        ))
    }
    fn merge_properties(
        &self,
        first: Option<PropertyDeprecations<'db>>,
        second: Option<PropertyDeprecations<'db>>,
    ) -> Result<Option<PropertyDeprecations<'db>>, Self::Error> {
        Ok(union_deprecated_properties(self.db, first, second))
    }
    fn merge_origins(
        &self,
        first: DescriptorOrigin<'db>,
        second: DescriptorOrigin<'db>,
    ) -> Result<DescriptorOrigin<'db>, Self::Error> {
        Ok(first.merge(self.db, second))
    }
    fn result(&self, parts: LookupParts<'db>) -> Result<MemberLookupResult<'db>, Self::Error> {
        Ok(member_lookup_result_with_origin(
            self.db,
            parts.member,
            parts.error,
            parts.properties,
            parts.descriptor,
        ))
    }
}

impl LookupFacts {
    fn parts<'db>(
        &self,
        fields: salsa::FieldReads<'db>,
        result: MemberLookupResult<'db>,
    ) -> LookupParts<'db> {
        let (member, error) = match result {
            Ok(member) => (member, None),
            Err(error) => (
                *error.read_fields(fields).fallback_member(),
                Some(*error.read_fields(fields).kind()),
            ),
        };
        match member {
            ResolvedMember::Plain(member) => LookupParts {
                member,
                error,
                properties: None,
                descriptor: DescriptorOrigin::default(),
            },
            ResolvedMember::WithMetadata(metadata) => {
                let fields = metadata.read_fields(fields);
                LookupParts {
                    member: *fields.member(),
                    error,
                    properties: *fields.properties(),
                    descriptor: *fields.descriptor(),
                }
            }
        }
    }
    fn key_type<'db>(
        &self,
        fields: salsa::FieldReads<'db>,
        key: MemberLookupKey<'db>,
    ) -> Type<'db> {
        *key.read_fields(fields).ty()
    }
    fn key_name<'db>(
        &self,
        fields: salsa::FieldReads<'db>,
        key: MemberLookupKey<'db>,
    ) -> &'db Name {
        key.read_fields(fields).name()
    }
    fn key_policy<'db>(
        &self,
        fields: salsa::FieldReads<'db>,
        key: MemberLookupKey<'db>,
    ) -> MemberLookupPolicy {
        *key.read_fields(fields).policy()
    }
    fn suppress_typed_dict_classvar<'db>(
        &self,
        fields: salsa::FieldReads<'db>,
        ty: Type<'db>,
        result: MemberLookupResult<'db>,
    ) -> bool {
        self.parts(fields, result).member.is_class_var() && ty.is_typed_dict()
    }
}

fn promote_inferred_attribute_class_literals<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    result: MemberLookupResult<'db>,
) -> MemberLookupResult<'db> {
    match member_lookup::finalization::promote_inferred_member_sync(
        result,
        member_lookup::finalization::MemberFinalizationFacts,
        &member_lookup::finalization::OrdinaryMemberFinalization { db, env },
    ) {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
#[synchronous(SynchronousMemberEntryEffects)]
pub(in crate::types) trait MemberEntryEffects<'db> {
    type Error;
    #[operation(checkpoint)]
    async fn checkpoint(&self) -> Result<(), Self::Error>;
    #[operation(local)]
    async fn key_parts(&self, key: MemberLookupKey<'db>) -> Result<(Type<'db>, &'db Name, MemberLookupPolicy), Self::Error>;
    #[operation(local)]
    async fn suppress_typed_dict_classvar(&self, ty: Type<'db>, result: MemberLookupResult<'db>) -> Result<bool, Self::Error>;
    #[operation(child)]
    async fn meta_type(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
    #[operation(child)]
    async fn nominal_class(&self, instance: NominalInstanceType<'db>) -> Result<ClassType<'db>, Self::Error>;
    #[operation(child)]
    async fn namespace(&self, ty: Type<'db>, class: ClassType<'db>, name: &str, policy: MemberLookupPolicy) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    #[operation(child)]
    async fn enum_member(&self, ty: Type<'db>, name: &Name) -> Result<Option<MemberLookupResult<'db>>, Self::Error>;
    #[operation(child)]
    async fn instance_storage(&self, ty: Type<'db>, name: &str) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
    #[operation(child)]
    async fn invoke_descriptor(&self, key: MemberLookupKey<'db>, receiver: Type<'db>, fallback: MemberLookupResult<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
    #[operation(child)]
    async fn fallback(&self, ty: Type<'db>, name: &Name, result: MemberLookupResult<'db>, policy: MemberLookupPolicy) -> Result<MemberLookupResult<'db>, Self::Error>;
    #[operation(child)]
    async fn bind_self(&self, result: MemberLookupResult<'db>, receiver: Type<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
    #[operation(child)]
    async fn promote(&self, result: MemberLookupResult<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
}
#[finite_capability]
impl LookupFacts {



    fn name_str<'a>(&self, name: &'a Name) -> &'a str { name.as_str() }
    fn undefined<'db>(&self) -> MemberLookupResult<'db> { Place::Undefined.into() }
    fn from_place<'db>(&self, place: PlaceAndQualifiers<'db>) -> MemberLookupResult<'db> { place.into() }

}
#[synchronous(nominal_class_member_sync)]
#[capabilities(effects = MemberEntryEffects)]
#[passive_values()]
pub(in crate::types) async fn nominal_class_member_with<'db, E: MemberEntryEffects<'db>>(
    ty: Type<'db>, instance: NominalInstanceType<'db>, name: &str, policy: MemberLookupPolicy, effects: &E,
) -> Result<PlaceAndQualifiers<'db>, E::Error> {
    effects.checkpoint().await?;
    let meta_type = effects.meta_type(ty).await?;
    let class = effects.nominal_class(instance).await?;
    effects.namespace(meta_type, class, name, policy).await
}
#[synchronous(restricted_member_entry_sync)]
#[capabilities(effects = MemberEntryEffects, facts = LookupFacts)]
#[passive_values()]
pub(in crate::types) async fn restricted_member_entry_with<'db, E: MemberEntryEffects<'db>>(
    key: MemberLookupKey<'db>, receiver: Type<'db>, facts: LookupFacts, effects: &E,
) -> Result<MemberLookupResult<'db>, E::Error> {
    effects.checkpoint().await?;
    let result = effects.invoke_descriptor(key, receiver, facts.undefined()).await?;
    effects.bind_self(result, receiver).await
}
#[synchronous(instance_member_entry_sync)]
#[capabilities(effects = MemberEntryEffects, facts = LookupFacts)]
#[passive_values()]
pub(in crate::types) async fn instance_member_entry_with<'db, E: MemberEntryEffects<'db>>(
    key: MemberLookupKey<'db>, receiver: Type<'db>, facts: LookupFacts, effects: &E,
) -> Result<MemberLookupResult<'db>, E::Error> {
    effects.checkpoint().await?;
    let (ty, name, policy) = effects.key_parts(key).await?;
    if let Some(result) = effects.enum_member(ty, name).await? { return Ok(result); }
    let fallback = effects.instance_storage(ty, facts.name_str(name)).await?;
    let result = effects.invoke_descriptor(key, receiver, facts.from_place(fallback)).await?;
    effects.checkpoint().await?;
    if effects.suppress_typed_dict_classvar(ty, result).await? { return Ok(facts.undefined()); }
    let result = effects.fallback(ty, name, result, policy).await?;
    let result = effects.bind_self(result, receiver).await?;
    effects.promote(result).await
}
}

struct InlineMemberEntry<'env, 'guard, 'db> {
    db: &'db dyn Db,
    env: &'env ProgramEnvironment<'db>,
    recursion_guard: Option<&'guard CallableRecursionGuard<'db>>,
}
impl<'db> SynchronousMemberEntryEffects<'db> for InlineMemberEntry<'_, '_, 'db> {
    type Error = std::convert::Infallible;
    fn checkpoint(&self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn key_parts(
        &self,
        key: MemberLookupKey<'db>,
    ) -> Result<(Type<'db>, &'db Name, MemberLookupPolicy), Self::Error> {
        let fields = salsa::FieldReads::new(self.db);
        Ok((
            LookupFacts.key_type(fields, key),
            LookupFacts.key_name(fields, key),
            LookupFacts.key_policy(fields, key),
        ))
    }
    fn suppress_typed_dict_classvar(
        &self,
        ty: Type<'db>,
        result: MemberLookupResult<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(LookupFacts.suppress_typed_dict_classvar(salsa::FieldReads::new(self.db), ty, result))
    }
    fn meta_type(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.to_meta_type(self.db, self.env))
    }
    fn nominal_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        Ok(instance.class(self.db, self.env))
    }
    fn namespace(
        &self,
        ty: Type<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(ty.class_namespace_member(self.db, self.env, class, name, policy))
    }
    fn enum_member(
        &self,
        ty: Type<'db>,
        name: &Name,
    ) -> Result<Option<MemberLookupResult<'db>>, Self::Error> {
        let enum_class = match ty {
            Type::LiteralValue(literal) => literal
                .as_enum()
                .map(|enum_literal| enum_literal.enum_class_literal(self.db)),
            _ => ty
                .nominal_class(self.db, self.env)
                .map(|class| class.class_literal(self.db))
                .and_then(|class| class.into_enum_class(self.db)),
        };
        Ok(enum_class.and_then(|enum_class| {
            enum_class
                .resolve_member(self.db, name)
                .map(|resolved_name| {
                    Place::bound(Type::enum_literal(EnumLiteralType::new(
                        self.db,
                        enum_class,
                        resolved_name,
                    )))
                    .into()
                })
        }))
    }
    fn instance_storage(
        &self,
        ty: Type<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        Ok(ty.instance_member(self.db, self.env, name))
    }
    fn invoke_descriptor(
        &self,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
        fallback: MemberLookupResult<'db>,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        Ok(Type::invoke_descriptor_protocol(
            self.db,
            self.env,
            key,
            receiver,
            fallback,
            InstanceFallbackShadowsNonDataDescriptor::No,
            self.recursion_guard,
        ))
    }
    fn fallback(
        &self,
        ty: Type<'db>,
        name: &Name,
        result: MemberLookupResult<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        Ok(ty.fallback_to_getattr(self.db, self.env, name, result, policy))
    }
    fn bind_self(
        &self,
        result: MemberLookupResult<'db>,
        receiver: Type<'db>,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        member_lookup::finalization::map_member_type_sync(
            result,
            member_lookup::finalization::MemberTypeMapping::BindSelf(receiver),
            member_lookup::finalization::MemberFinalizationFacts,
            &member_lookup::finalization::OrdinaryMemberFinalization {
                db: self.db,
                env: self.env,
            },
        )
    }
    fn promote(
        &self,
        result: MemberLookupResult<'db>,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        Ok(promote_inferred_attribute_class_literals(
            self.db, self.env, result,
        ))
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) enum MemberFallbackDecision {
    Missing,
    Defined,
    PossiblyUndefined,
}

pub(in crate::types) fn member_fallback_decision(place: Place<'_>) -> MemberFallbackDecision {
    match place {
        Place::Undefined => MemberFallbackDecision::Missing,
        Place::Defined(DefinedPlace {
            definedness: Definedness::AlwaysDefined,
            ..
        }) => MemberFallbackDecision::Defined,
        Place::Defined(DefinedPlace {
            definedness: Definedness::PossiblyUndefined,
            ..
        }) => MemberFallbackDecision::PossiblyUndefined,
    }
}

impl LookupFacts {
    pub(in crate::types) fn getattribute_flags_affect_lookup<'db>(
        &self,
        fields: salsa::FieldReads<'db>,
        flags: ClassInstanceFlags,
        result: MemberLookupResult<'db>,
    ) -> bool {
        if flags.contains(ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE) {
            return true;
        }
        flags.contains(ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE)
            && !(result.is_ok()
                && matches!(self.parts(fields, result).member.place, Place::Defined(place) if place.is_definitely_defined()))
    }
}

pub(in crate::types) fn property_wrapper_kind(name: &str) -> Option<WrapperDescriptorKind> {
    match name {
        "__get__" => Some(WrapperDescriptorKind::PropertyDunderGet),
        "__set__" => Some(WrapperDescriptorKind::PropertyDunderSet),
        "__delete__" => Some(WrapperDescriptorKind::PropertyDunderDelete),
        _ => None,
    }
}

pub(in crate::types) fn native_class_mro_attribute<'db>(
    known: Option<KnownClass>,
    name: &str,
) -> Option<PlaceAndQualifiers<'db>> {
    let wrapper = match (known, name) {
        (Some(KnownClass::FunctionType), "__get__") => WrapperDescriptorKind::FunctionTypeDunderGet,
        (Some(KnownClass::FunctionType), "__set__" | "__delete__") => {
            // These frequently requested descriptor methods are absent on FunctionType.
            return Some(Place::Undefined.into());
        }
        (Some(KnownClass::Property | KnownClass::EnumProperty), "__get__") => {
            WrapperDescriptorKind::PropertyDunderGet
        }
        (Some(KnownClass::Property | KnownClass::EnumProperty), "__set__") => {
            WrapperDescriptorKind::PropertyDunderSet
        }
        (Some(KnownClass::Property), "__delete__") => WrapperDescriptorKind::PropertyDunderDelete,
        _ => return None,
    };
    Some(Place::bound(Type::WrapperDescriptor(wrapper)).into())
}

#[salsa::tracked(configuration = (pub(in crate::types) CachedMaterializationConfiguration), attempt = ReturnOnly,
    returns(copy),
    cycle_initial=|_, id, _, _, materialization_kind| {
        Type::Divergent(DivergentType::new(id).materialized(materialization_kind))
    },
    cycle_fn=|db, cycle, previous: &Type<'db>, value: Type<'db>, _, program, _| {
        value.cycle_normalized_impl(db, &ProgramEnvironment::from_program(program), *previous, cycle)
    },
    heap_size=ruff_memory_usage::heap_size
)]
fn cached_materialization<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    program: Program<'db>,
    materialization_kind: MaterializationKind,
) -> Type<'db> {
    let env = &ProgramEnvironment::from_program(program);
    ty.materialize(db, materialization_kind, &ApplyTypeMappingVisitor::new(env))
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn apply_specialization_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<ApplySpecializationInnerConfiguration> {
    apply_specialization_inner::fn_ingredient_(db, db.zalsa())
}

#[salsa::tracked(configuration = (pub(in crate::types) ApplySpecializationInnerConfiguration), attempt = ReturnOnly,
    self_ty = Type<'db>,
    returns(copy),
    cycle_initial=|_, id, _, _, _| Type::divergent(id),
    cycle_fn=|db, cycle, previous: &Type<'db>, value: Type<'db>, _, specialization: Specialization<'db>, _| {
        let env = ProgramEnvironment::from_program(
            specialization.generic_context(db).program(db),
        );
        value.cycle_normalized_impl(db, &env, *previous, cycle)
    },
    heap_size=ruff_memory_usage::heap_size
)]
fn apply_specialization_inner<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    specialization: Specialization<'db>,
    specialize_self_domain: bool,
) -> Type<'db> {
    inline_mapping_result(mapping::specialization::shared_specialization_sync(
        db,
        ty,
        specialization,
        specialize_self_domain,
        &mapping::specialization::InlineSpecializationEffects,
    ))
}

#[cfg(any(test, feature = "experimental-analysis"))]
fn cached_materialization_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<CachedMaterializationConfiguration> {
    cached_materialization::fn_ingredient_(db, db.zalsa())
}

#[cfg(any(test, feature = "experimental-analysis"))]
fn data_descriptor_ingredient(db: &dyn Db) -> &IngredientImpl<IsDataDescriptorImplConfiguration> {
    is_data_descriptor_impl_::fn_ingredient_(db, db.zalsa())
}

// Definite data descriptors use an all-of union fold; possible data descriptors use any-of.
// Seed recursive aliases with the corresponding identity value.
#[salsa::tracked(configuration = (pub(in crate::types) IsDataDescriptorImplConfiguration), attempt = ReturnOnly,
    self_ty = Type<'db>,
    returns(copy),
    cycle_initial=|_, _, _, _, any_of_union: bool| !any_of_union,
    heap_size=ruff_memory_usage::heap_size
)]
fn is_data_descriptor_impl_<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    program: Program<'db>,
    any_of_union: bool,
) -> bool {
    match data_descriptor::classify_data_descriptor_sync(
        ty,
        any_of_union,
        data_descriptor::DataDescriptorFacts,
        &data_descriptor::OrdinaryDataDescriptorEffects { db, program },
    ) {
        Ok(result) => result,
        Err(never) => match never {},
    }
}
