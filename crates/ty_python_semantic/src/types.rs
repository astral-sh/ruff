use compact_str::{CompactString, ToCompactString};
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
pub use context::ProgramEnvironment;
use ruff_db::Instant;
use ruff_db::diagnostic::{Annotation, Diagnostic, Span};
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_text_size::Ranged;
use smallvec::smallvec_inline;
use ty_module_resolver::{
    ImportingFile, KnownModule, Module, ModuleName, file_to_module, resolve_module,
};

pub(crate) use self::callable::UpcastPolicy;
use self::class::ClassInstanceFlags;
pub use self::cyclic::CycleDetector;
pub(crate) use self::cyclic::TypeTransformer;
use self::cyclic::{ActiveRecursionDetector, TypeIdentity};
pub use self::dedicated::pytest::{
    FixtureBinding, FixtureExposure, FixtureNameSource, fixture_bindings_for_parameter,
    fixture_exposures_for_definition, pytest_global_plugin_files,
};
pub(crate) use self::diagnostic::TypeCheckDiagnostics;
pub(crate) use self::diagnostic::register_lints;
pub use self::diagnostic::{UNDEFINED_REVEAL, UNRESOLVED_REFERENCE};
use self::infer::infer_function_default_types;
pub(crate) use self::infer::{
    InferredDeclaration, TypeContext, infer_complete_scope_types, infer_deferred_types,
    infer_definition_types, infer_expression_type, infer_expression_types,
    infer_same_file_expression_type, infer_scope_types, is_discarded_dict_key_assignment,
};
pub(crate) use self::iteration::extract_fixed_length_iterable_element_types;
pub use self::known_instance::KnownInstanceType;
pub(crate) use self::match_pattern::{
    ClassPatternPositionalSource, callable_pattern_type, class_pattern_positional_sources,
    definite_match_pattern_type, definite_match_pattern_type_for_subject,
    exact_sequence_pattern_type, mapping_pattern_type, pattern_binding_fallthrough_type,
    sequence_pattern_type_builder, singleton_pattern_type, starred_sequence_pattern_type,
    typed_dict_matches_class_pattern,
};
pub(crate) use self::relation_error::{ErrorContext, ErrorContextTree, ParameterDescription};
use self::set_theoretic::KnownUnion;
use self::set_theoretic::NegativeIntersectionElements;
pub(crate) use self::set_theoretic::builder::{
    IntersectionBuilder, UnionAccumulator, UnionBuilder,
};
pub use self::set_theoretic::{IntersectionType, UnionType};
pub(crate) use self::signatures::Signature;
pub use self::signatures::{ParameterDefault, ParameterKind};
pub(crate) use self::subclass_of::{SubclassOfInner, SubclassOfType};
pub(crate) use self::type_expansion::expand_type;
pub(crate) use crate::diagnostic::add_inferred_python_version_hint_to_diagnostic;
use crate::place::{
    DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, TypeOrigin,
    builtins_module_scope, imported_symbol, known_module_symbol, place_from_bindings,
};
use crate::suppression::check_suppressions;
use crate::types::bound_super::BoundSuperType;
use crate::types::call::bind::ConstructorCallableKind;
use crate::types::call::{Binding, Bindings, CallArguments, CallableBinding};
pub(crate) use crate::types::callable::{CallableType, CallableTypes};
pub(crate) use crate::types::class_base::ClassBase;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::context::{LintDiagnosticGuard, LintDiagnosticGuardBuilder};
use crate::types::diagnostic::{
    AttributeAccessMethod, INVALID_AWAIT, INVALID_TYPE_FORM, report_bad_attribute_access_call,
    report_bad_dunder_get_call, report_bad_import_call,
};
pub use crate::types::display::{DisplaySettings, TypeDetail, TypeDisplayDetails};
pub(crate) use crate::types::enums::{EnumClassLiteral, EnumComplementType, enum_metadata};
pub(crate) use crate::types::equality::{ComparisonSoundnessPolicy, equality_truthiness};
use crate::types::function::{
    DataclassTransformerFlags, DataclassTransformerParams, FunctionDecorators, FunctionSpans,
    FunctionType, KnownFunction,
};
pub(crate) use crate::types::generics::GenericContext;
use crate::types::generics::{ApplySpecialization, Specialization, bind_typevar};
use crate::types::infer::InferenceFlags;
use crate::types::known_instance::{
    InternedConstraintSet, InternedType, SentinelInstance, UnionTypeInstance,
};
pub use crate::types::method::{BoundMethodType, KnownBoundMethodType, WrapperDescriptorKind};
use crate::types::mro::{MroIterator, StaticMroError};
pub(crate) use crate::types::narrow::{NarrowingConstraint, infer_narrowing_constraints};
use crate::types::newtype::NewType;
use crate::types::signatures::{ConcatenateTail, walk_signature};
pub(crate) use crate::types::signatures::{Parameter, Parameters};
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
use crate::types::variance::VarianceInferable;
use crate::types::visitor::{
    any_over_type, any_over_type_including_alias_arguments, dynamic_content,
};
use crate::{Db, FxOrderSet, HasType, NameKind, Program, SemanticModel};
pub(crate) use class::{ClassLiteral, ClassType, GenericAlias, StaticClassLiteral};
pub use class::{KnownClass, MethodDecorator, SlotDescriptorType};
use instance::Protocol;
pub use instance::{NominalInstanceType, ProtocolInstanceType};
pub(crate) use literal::{
    BytesLiteralType, EnumLiteralType, LiteralValueType, LiteralValueTypeKind, StringLiteralType,
};
pub use special_form::SpecialFormType;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::ScopeId;
use ty_python_core::{ProgramFile, Truthiness, place_table, semantic_index, use_def_map};

mod attribute_write;
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
mod dedicated;
mod diagnostic;
mod display;
mod enums;
mod equality;
mod function;
mod generics;
pub mod ide_support;
mod infer;
mod instance;
mod iteration;
mod known_instance;
pub mod list_members;
mod literal;
mod match_pattern;
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
mod type_expansion;
mod type_form;
mod typed_dict;
mod typevar;
mod unpacker;
mod variance;
mod visitor;

mod definition;
#[cfg(test)]
mod property_tests;
mod subscript;

pub fn check_types(db: &dyn Db, file: ProgramFile<'_>) -> Vec<Diagnostic> {
    let source_file = file.file(db);
    let _span = tracing::trace_span!("check_types", ?source_file).entered();
    tracing::debug!("Checking file '{path}'", path = source_file.path(db));

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
            .map(|error| Diagnostic::invalid_syntax(source_file, error, error)),
    );

    let diagnostics = check_suppressions(db, file.python_file(db), diagnostics);

    let elapsed = start.elapsed();
    if elapsed >= Duration::from_millis(100) {
        tracing::info!(
            "Checking file `{path}` took more than 100ms ({elapsed:?})",
            path = source_file.path(db)
        );
    }

    diagnostics
}

/// Infer the type of a binding.
pub(crate) fn binding_type<'db>(db: &'db dyn Db, definition: Definition<'db>) -> Type<'db> {
    let inference = infer_definition_types(db, definition);
    inference.binding_type(definition)
}

/// Returns whether a definition represents a value that exists at runtime.
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
#[salsa::tracked(returns(copy))]
pub(crate) fn exists_at_runtime<'db>(db: &'db dyn Db, definition: Definition<'db>) -> bool {
    let file = definition.program_file(db);
    let inference = infer_definition_types(db, definition);
    let ty = inference.binding_type(definition);

    // A class or function decorated with `@type_check_only` never exists at runtime.
    if ty.is_type_check_only(db)
        || inference
            .undecorated_type()
            .is_some_and(|ty| ty.is_type_check_only(db))
    {
        return false;
    }

    let parsed = parsed_module(db, file.python_file(db));
    let module = parsed.load(db);

    // Definitions inside an `if TYPE_CHECKING` block are never available at runtime.
    if semantic_index(db, file).is_in_type_checking_block(
        definition.file_scope(db),
        definition.full_range(db, &module).range(),
    ) {
        return false;
    }

    // The remaining heuristics only apply to stub definitions.
    if !file.file(db).is_stub(db) {
        return true;
    }

    let is_private = definition.place(db).as_symbol().is_some_and(|symbol| {
        matches!(
            NameKind::classify(place_table(db, definition.scope(db)).symbol(symbol).name()),
            NameKind::Sunder
        )
    });

    if !is_private {
        return true;
    }

    // Private type variables, parameter specifications, and type-variable tuples in stubs are
    // implementation details rather than runtime values.
    if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = ty
        && typevar.definition(db) == Some(definition)
    {
        return false;
    }

    // Explicit PEP 613 and PEP 695 type aliases in stubs are also typing-only helpers.
    let model = SemanticModel::new(db, file);
    if model.is_type_alias_definition(definition) {
        return false;
    }

    let DefinitionKind::Assignment(assignment) = definition.kind(db) else {
        return true;
    };

    // Treat only unambiguous union, `Literal`, and `Annotated` expressions as implicit aliases.
    // Other expressions may also be aliases, but a false negative is preferable to incorrectly
    // hiding a value that exists at runtime.
    match (ty, assignment.value(&module)) {
        (
            Type::KnownInstance(KnownInstanceType::UnionType(_)),
            ast::Expr::BinOp(ast::ExprBinOp {
                op: ast::Operator::BitOr,
                ..
            }),
        ) => false,
        (
            Type::KnownInstance(KnownInstanceType::Literal(_) | KnownInstanceType::Annotated(_)),
            ast::Expr::Subscript(subscript),
        ) => !matches!(
            subscript.value.inferred_type(&model),
            Some(Type::SpecialForm(_) | Type::ClassLiteral(_) | Type::GenericAlias(_))
        ),
        _ => true,
    }
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
    let file = definition.program_file(db);
    let index = semantic_index(db, file);
    let file_scope = index.expression_scope_id(expression);
    let scope = file_scope.to_scope_id(db, file);
    if scope == definition.scope(db) {
        // expression is in the definition scope
        let inference = infer_definition_types(db, definition);
        if let Some(ty) = inference.try_expression_type(expression) {
            ty
        } else if let Some(ty) =
            infer_deferred_types(db, definition).try_expression_type(expression)
        {
            ty
        } else if matches!(definition.kind(db), DefinitionKind::Function(_)) {
            infer_function_default_types(db, definition).expression_type(expression)
        } else {
            Type::unknown()
        }
    } else {
        // expression is in a type-params sub-scope
        infer_complete_scope_types(db, scope).expression_type(expression)
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

struct ApplyTypeMappingTag;
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
        let type_transformer = match type_mapping {
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
        };
        type_transformer
            .get_or_init(Box::default)
            .visit_type(db, ty, func)
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
type SpecializationVisitor<'db> = CycleDetector<'db, VisitSpecialization, Type<'db>, (), 3>;
struct VisitSpecialization;

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
            Type::Union(union) => {
                let mut elements = union.elements(db).iter();
                let Type::SpecialForm(SpecialFormType::TypedDict(module)) = elements.next()? else {
                    return None;
                };
                elements.try_fold(*module, |module, element| {
                    let Type::SpecialForm(SpecialFormType::TypedDict(element_module)) = element
                    else {
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
            _ => None,
        }
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

/// An interned description of an invalid implicit `__get__` call.
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

/// The type and descriptor kind produced by an implicit `__get__` call.
#[derive(Clone, Debug, Copy, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct DescriptorGetResult<'db> {
    pub(crate) return_type: Type<'db>,
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
    kind: AttributeKind,
    error: Option<DescriptorGetCallContext<'db>>,
) -> Result<Option<DescriptorGetResult<'db>>, DescriptorGetError<'db>> {
    let result = DescriptorGetResult { return_type, kind };
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
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct MemberLookupError<'db> {
    #[returns(copy)]
    fallback_member: PlaceAndQualifiers<'db>,
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
type MemberLookupResult<'db> = Result<PlaceAndQualifiers<'db>, MemberLookupError<'db>>;

fn member_lookup_result<'db>(
    db: &'db dyn Db,
    member: PlaceAndQualifiers<'db>,
    error: Option<MemberLookupErrorKind<'db>>,
) -> MemberLookupResult<'db> {
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
        Ok(member) => Ok(member.map_type(f)),
        Err(error) => Err(MemberLookupError::new(
            db,
            error.fallback_member(db).map_type(f),
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
            let member = constraints.map_with_boundness_and_qualifiers(db, env, |constraint| {
                let result = constraint.member_lookup_with_policy_and_receiver(
                    db,
                    env,
                    name,
                    policy,
                    Some(*constraint),
                );
                let result =
                    map_member_lookup_type(db, result, |ty| match ty {
                        Type::BoundMethod(method) => Type::BoundMethod(
                            method.with_signature_receiver(db, symbolic_receiver, *constraint),
                        ),
                        _ => ty,
                    });
                error = error.or_else(|| result.err().map(|error| error.kind(db)));
                result.unwrap_or_else(|error| error.fallback_member(db))
            });
            member_lookup_result(db, member, error)
        }
    }
}

fn member_lookup_or_fall_back_to<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    result: MemberLookupResult<'db>,
    fallback_fn: impl FnOnce() -> MemberLookupResult<'db>,
) -> MemberLookupResult<'db> {
    let member = result.unwrap_or_else(|error| error.fallback_member(db));
    match member.place {
        Place::Undefined => fallback_fn(),
        Place::Defined(DefinedPlace {
            definedness: Definedness::AlwaysDefined,
            ..
        }) => result,
        Place::Defined(DefinedPlace {
            definedness: Definedness::PossiblyUndefined,
            ..
        }) => {
            let fallback = fallback_fn();
            let fallback_member = fallback.unwrap_or_else(|error| error.fallback_member(db));
            member_lookup_result(
                db,
                member.or_fall_back_to(db, env, || fallback_member),
                result
                    .err()
                    .map(|error| error.kind(db))
                    .or_else(|| fallback.err().map(|error| error.kind(db))),
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
    let error = result
        .err()
        .map(|error| error.kind(db))
        .filter(|_| cycle.iteration() <= crate::TAINTED_CYCLES || previous.is_err());
    let member = result.unwrap_or_else(|error| error.fallback_member(db));
    let previous = previous.unwrap_or_else(|error| error.fallback_member(db));
    member_lookup_result(db, member.cycle_normalized(db, env, previous, cycle), error)
}

impl<'db> From<PlaceAndQualifiers<'db>> for MemberLookupResult<'db> {
    fn from(member: PlaceAndQualifiers<'db>) -> Self {
        Ok(member)
    }
}

impl<'db> From<Place<'db>> for MemberLookupResult<'db> {
    fn from(place: Place<'db>) -> Self {
        Ok(place.into())
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

        /// Ignore members that are only available through a dynamic type.
        ///
        /// This is used when detecting descriptors. An `Any` or `Unknown` base can provide any
        /// member, but that does not mean that every subclass should be treated as a descriptor.
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
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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
        match self {
            Self::Builtin => KnownClass::Property.to_class_literal(db, env),
            Self::Enum => KnownClass::EnumProperty.to_class_literal(db, env),
            Self::Subclass(class) => class.into(),
        }
    }

    fn to_instance(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            Self::Builtin => KnownClass::Property.to_instance(db, env),
            Self::Enum => KnownClass::EnumProperty.to_instance(db, env),
            Self::Subclass(class) => Type::instance(db, env, class),
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
    let wrapper = match name {
        "__get__" => WrapperDescriptorKind::PropertyDunderGet,
        "__set__" => WrapperDescriptorKind::PropertyDunderSet,
        "__delete__" => WrapperDescriptorKind::PropertyDunderDelete,
        _ => return member,
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

/// Represents a property with known accessors and the standard descriptor behavior.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct PropertyInstanceType<'db> {
    #[returns(copy)]
    pub getter: Option<Type<'db>>,
    #[returns(copy)]
    pub setter: Option<Type<'db>>,
    #[returns(copy)]
    pub deleter: Option<Type<'db>>,
    #[returns(copy)]
    instance_class: PropertyInstanceClass<'db>,
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

// The Salsa heap is tracked separately.
impl get_size2::GetSize for PropertyInstanceType<'_> {}

impl<'db> PropertyInstanceType<'db> {
    fn new(
        db: &'db dyn Db,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
    ) -> Self {
        Self::new_internal(db, getter, setter, deleter, PropertyInstanceClass::Builtin)
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
        )
    }

    fn with_accessors(
        self,
        db: &'db dyn Db,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
    ) -> Self {
        Self::new_internal(db, getter, setter, deleter, self.instance_class(db))
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
        Self::new_internal(db, getter, setter, deleter, instance_class)
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
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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
    pub(crate) const fn any() -> Self {
        Self::Dynamic(DynamicType::Any)
    }

    pub const fn unknown() -> Self {
        Self::Dynamic(DynamicType::Unknown)
    }

    pub(crate) fn divergent(id: salsa::Id) -> Self {
        Self::Divergent(DivergentType::new(id))
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

        Some(match divergent.materialization_kind() {
            Some(materialization_kind) => {
                Type::Divergent(divergent.materialized(materialization_kind.flip()))
            }
            None => Type::Divergent(divergent),
        })
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
        if let Type::NominalInstance(instance) = self
            && !instance.is_definition_generic(db)
        {
            return false;
        }

        // Type alias bodies cannot declare `Self`, but their explicit type arguments can
        // contain the `Self` from an enclosing method or class.
        any_over_type_including_alias_arguments(db, env, self, |ty| {
            ty.as_typevar().is_some_and(|tv| tv.typevar(db).is_self(db))
        })
    }

    /// Returns `true` if this type supports eager `Self` binding via `bind_self_typevars`.
    ///
    /// `FunctionLiteral`, `BoundMethod`, and function-like `Callable` types return `false`
    /// because their `Self` binding is deferred to call time via the signature binding path.
    fn supports_self_binding(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        match self {
            Type::FunctionLiteral(_) | Type::BoundMethod(_) | Type::KnownBoundMethod(_) => false,
            Type::Callable(callable) if callable.is_function_like(db) => false,
            _ => self.contains_self(db, env),
        }
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
        if !self.supports_self_binding(db, env) {
            return self;
        }

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
        if cycle.iteration() <= crate::TAINTED_CYCLES {
            let self_degraded_by_overload =
                any_over_type(db, env, self, false, |ty| {
                    matches!(ty, Type::Dynamic(DynamicType::AmbiguousOverload))
                }) && !any_over_type(db, env, self, false, |ty| ty.is_divergent())
                    && any_over_type(db, env, previous, false, |ty| ty.is_divergent());
            // Generally, the precision of type inference improves with each iteration.
            // However, overload is an exception; as iterations progress, overload matching may become ambiguous, and a reversal of precision can occur.
            // This kind of precision degradation can be determined by whether the type contains `DynamicType::AmbiguousOverload`.
            if self_degraded_by_overload {
                UnionType::from_elements_cycle_recovery(db, env, [previous, self])
            } else {
                self
            }
        } else if let (Type::GenericAlias(current), Type::GenericAlias(previous)) = (self, previous)
            && let Some(merged) = current.merge_cycle_recovery(db, previous)
        {
            Type::GenericAlias(merged)
        } else {
            // The current type is unioned to the previous type. Unioning in the reverse order can
            // cause the fixed-point iterations to converge slowly or even fail. Consider the case
            // where the order of union types is different between the previous and current cycle.
            // We should use the previous union type as the base and only add new element types in
            // this cycle, if any.
            UnionType::from_elements_cycle_recovery(db, env, [previous, self])
        }
        .recursive_type_normalized_impl_with_cycle(db, env, cycle)
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

    /// Returns `true` if this type is an awaitable that should be awaited before being discarded.
    ///
    /// Currently checks for instances of `types.CoroutineType` (returned by `async def` calls).
    /// Unions are considered awaitable only if every element is awaitable.
    /// Intersections are considered awaitable if any positive element is awaitable.
    fn is_awaitable(self, db: &'db dyn Db) -> bool {
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
    fn known_specialization(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        known_class: KnownClass,
    ) -> Option<Specialization<'db>> {
        let class_literal = known_class.try_to_class_literal(db, env)?;
        self.specialization_of(db, env, class_literal)
    }

    /// If the type is a specialized instance of the given class, returns the specialization.
    fn specialization_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        expected_class: StaticClassLiteral<'_>,
    ) -> Option<Specialization<'db>> {
        self.class_specialization(db, env)
            .filter(|(class_literal, _)| *class_literal == expected_class)
            .map(|(_, specialization)| specialization)
    }

    /// If this type is a class instance or class-backed `TypedDict`, returns its specialization.
    fn class_specialization(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<(StaticClassLiteral<'db>, Specialization<'db>)> {
        let class = match self {
            Type::TypedDict(typed_dict) => typed_dict.defining_class()?,
            _ => self.nominal_class(db, env)?,
        };

        class
            .static_class_literal(db)
            .and_then(|(class_literal, specialization)| Some((class_literal, specialization?)))
    }

    /// If this type is a class instance, returns its class.
    fn nominal_class(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<ClassType<'db>> {
        match self {
            Type::NominalInstance(instance) => Some(instance.class(db, env)),
            Type::ProtocolInstance(instance) => instance.class_origin(db).map(|class| *class),
            Type::TypeAlias(alias) => alias.value_type(db).nominal_class(db, env),
            Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db).nominal_class(db, env),
            Type::TypeVar(typevar) => {
                let TypeVarBoundOrConstraints::UpperBound(bound) =
                    typevar.typevar(db).bound_or_constraints(db, env)?
                else {
                    return None;
                };
                bound.nominal_class(db, env)
            }
            Type::LiteralValue(literal) => {
                literal.fallback_instance(db, env).nominal_class(db, env)
            }
            Type::PropertyInstance(property) => {
                property.instance_fallback(db, env).nominal_class(db, env)
            }
            Type::SlotDescriptor(_) => KnownClass::MemberDescriptorType
                .to_instance(db, env)
                .nominal_class(db, env),
            _ => None,
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
        (*self).cached_materialization(db, env.program(db), MaterializationKind::Top)
    }

    /// Returns the bottom materialization (or lower bound materialization) of this type, which is
    /// the most specific form of the type that is fully static.
    #[must_use]
    fn bottom_materialization(&self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        (*self).cached_materialization(db, env.program(db), MaterializationKind::Bottom)
    }

    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, id, _, _, materialization_kind| {
            Type::Divergent(DivergentType::new(id).materialized(materialization_kind))
        },
        cycle_fn=|db, cycle, previous: &Type<'db>, value: Type<'db>, _, program, _| {
            value.cycle_normalized_impl(db, &ProgramEnvironment::from_program(program), *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    fn cached_materialization(
        self,
        db: &'db dyn Db,
        program: Program<'db>,
        materialization_kind: MaterializationKind,
    ) -> Type<'db> {
        let env = &ProgramEnvironment::from_program(program);
        self.materialize(db, materialization_kind, &ApplyTypeMappingVisitor::new(env))
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
        self.as_nominal_instance()
            .and_then(|instance| instance.tuple_spec(db, env))
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

    const fn as_special_form(self) -> Option<SpecialFormType> {
        match self {
            Type::SpecialForm(special_form) => Some(special_form),
            _ => None,
        }
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

    /// If this type is a `Type::TypeAlias`, recursively resolves it to its
    /// underlying value type. Otherwise, returns `self` unchanged.
    fn resolve_type_alias(self, db: &'db dyn Db) -> Type<'db> {
        let mut ty = self;
        while let Type::TypeAlias(alias) = ty {
            ty = alias.value_type(db);
        }
        ty
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
        match self.resolve_type_alias(db) {
            Type::Union(union) => Some(union),
            Type::NewTypeInstance(newtype) => newtype.concrete_base_type(db).as_union_like(db),
            _ => None,
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
        match self {
            Type::ClassLiteral(class_literal) => Some(class_literal.default_specialization(db)),
            Type::GenericAlias(alias) => Some(ClassType::Generic(alias)),
            _ => None,
        }
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
        // Avoid invoking the `IntersectionBuilder` for negations that are trivial.
        //
        // We verify that this always produces the same result as
        // `IntersectionBuilder::new(db, env).add_negative(*self).build()` via the
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
            | Type::TypeForm(_)
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
            | Type::SlotDescriptor(_)
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

            Type::Union(_) | Type::Intersection(_) | Type::EnumComplement(_) => {
                IntersectionBuilder::new(db, env)
                    .add_negative(*self)
                    .build()
            }
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
            Type::TypeVar(_) | Type::TypeAlias(_) | Type::SubclassOf(_) => true,
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
            Type::NominalInstance(_)
            | Type::NewTypeInstance(_)
            | Type::LiteralValue(_)
            | Type::TypeAlias(_) => true,

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
        mut f: impl FnMut(&Type<'db>) -> bool,
    ) -> Type<'db> {
        let Type::Union(union) = self.resolve_type_alias(db) else {
            return self;
        };
        let union = if union.has_aliases(db) {
            match union.expand_aliases(db, env) {
                Type::Union(expanded) => expanded,
                // Expanding collapsed the union to a single type, leaving nothing to filter
                // between, so apply the predicate to it directly.
                expanded => return if f(&expanded) { expanded } else { Type::Never },
            }
        } else {
            union
        };
        union.filter(db, f)
    }

    /// If the type is a union, removes union elements that are disjoint from `target`.
    ///
    /// Otherwise, returns the type unchanged.
    fn filter_disjoint_elements(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Type<'db> {
        let constraints = ConstraintSetBuilder::new();
        self.filter_union(db, env, |elem| {
            !elem
                .when_disjoint_from(db, env, target, &constraints, inferable)
                .is_always_satisfied(db, env)
        })
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
        match self {
            Type::ModuleLiteral(_) => Some(KnownClass::ModuleType.to_instance(db, env)),
            Type::FunctionLiteral(_) => Some(KnownClass::FunctionType.to_instance(db, env)),
            Type::LiteralValue(literal) => Some(literal.fallback_instance(db, env)),
            _ => None,
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
        match self {
            Type::LiteralValue(literal) if literal.is_promotable() => {
                literal.fallback_instance(db, env)
            }
            Type::FunctionLiteral(literal) => Type::Callable(literal.into_callable_type(db)),
            _ => self,
        }
    }

    /// Like [`Type::promote_singletons_recursively`], but does not recurse into nested types.
    fn promote_singletons_impl(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            Type::NominalInstance(instance) if instance.is_singleton(db) => {
                UnionType::from_two_elements(db, env, self, Type::unknown())
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
        cycle.head_ids().fold(self, |ty, id| {
            ty.recursive_type_normalized_impl(db, env, Type::divergent(id), false)
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
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        if nested && self.same_divergent_marker(div) {
            return None;
        }
        match self {
            Type::Union(union) => union.recursive_type_normalized_impl(db, env, div, nested),
            Type::Intersection(intersection) => intersection
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::Intersection),
            Type::EnumComplement(complement) => complement
                .to_intersection(db, env)
                .recursive_type_normalized_impl(db, env, div, nested),
            Type::Callable(callable) => callable
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::Callable),
            Type::ProtocolInstance(protocol) => protocol
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::ProtocolInstance),
            Type::NominalInstance(instance) => instance
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::NominalInstance),
            Type::FunctionLiteral(function) => function
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::FunctionLiteral),
            Type::PropertyInstance(property) => property
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::PropertyInstance),
            Type::SlotDescriptor(descriptor) => descriptor
                .value_type(db)
                .recursive_type_normalized_impl(db, env, div, true)
                .map(|value_type| Type::SlotDescriptor(SlotDescriptorType::new(db, value_type))),
            Type::KnownBoundMethod(method_kind) => method_kind
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::KnownBoundMethod),
            Type::BoundMethod(method) => method
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::BoundMethod),
            Type::BoundSuper(bound_super) => bound_super
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::BoundSuper),
            Type::GenericAlias(generic) => generic
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::GenericAlias),
            Type::ClassLiteral(class) => class
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::ClassLiteral),
            Type::SubclassOf(subclass_of) => subclass_of
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::SubclassOf),
            Type::TypeVar(_) => Some(self),
            Type::KnownInstance(known_instance) => known_instance
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::KnownInstance),
            Type::TypeIs(type_is) => {
                recursive_type_normalize_type_guard_like(db, env, type_is, div, nested)
            }
            Type::TypeGuard(type_guard) => {
                recursive_type_normalize_type_guard_like(db, env, type_guard, div, nested)
            }
            Type::TypeForm(typeform) => typeform
                .type_argument(db)
                .recursive_type_normalized_impl(db, env, div, true)
                .map(|ty| TypeFormType::from_type_expression(db, ty)),
            Type::Divergent(_) => Some(self),
            Type::Dynamic(dynamic) => Some(Type::Dynamic(dynamic.recursive_type_normalized())),
            Type::TypedDict(_) => {
                // TODO: Normalize TypedDicts
                Some(self)
            }
            Type::TypeAlias(_) => Some(self),
            Type::NewTypeInstance(newtype) => newtype
                .recursive_type_normalized_impl(db, env, div, nested)
                .map(Type::NewTypeInstance),
            Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::Never
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::SpecialForm(_)
            | Type::LiteralValue(_) => Some(self),
        }
    }

    /// Recursively visit the specialization of a generic class instance.
    ///
    /// The provided closure will be called on any nested types, along with their variance with
    /// respect to the outermost type.
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
                }
                Type::TypeAlias(alias) => visitor.visit(db, self, || {
                    alias
                        .value_type(db)
                        .visit_specialization_impl(db, env, polarity, f, visitor);
                }),
                Type::Callable(callable) => {
                    for signature in callable.signatures(db) {
                        for parameter in signature.parameters() {
                            let variance = TypeVarVariance::Contravariant.compose(polarity);

                            f(parameter.annotated_type(), variance);

                            visitor.visit(db, parameter.annotated_type(), || {
                                parameter
                                    .annotated_type()
                                    .visit_specialization_impl(db, env, variance, f, visitor);
                            });
                        }

                        visitor.visit(db, signature.return_ty, || {
                            signature
                                .return_ty
                                .visit_specialization_impl(db, env, polarity, f, visitor);
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

            visitor.visit(db, *ty, || {
                ty.visit_specialization_impl(db, env, variance, f, visitor);
            });
        }
    }

    /// Return true if there is just a single inhabitant for this type.
    ///
    /// Note: This function aims to have no false positives, but might return `false`
    /// for more complicated types that are actually singletons.
    fn is_singleton(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
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
        if let Some(fallback) = (*self).materialized_divergent_fallback() {
            return fallback.find_name_in_mro_with_policy(db, env, name, policy);
        }

        match self {
            Type::Union(union) => {
                Some(union.map_with_boundness_and_qualifiers(db, env, |elem| {
                    elem.find_name_in_mro_with_policy(db, env, name, policy)
                        // If some elements are classes, and some are not, we simply fall back to `Unbound` for the non-class
                        // elements instead of short-circuiting the whole result to `None`. We would need a more detailed
                        // return type otherwise, and since `find_name_in_mro` is usually called via `class_member`, this is
                        // not a problem.
                        .unwrap_or_default()
                }))
            }
            Type::Intersection(inter) => {
                Some(inter.map_with_boundness_and_qualifiers(db, env, |elem| {
                    elem.find_name_in_mro_with_policy(db, env, name, policy)
                        // Fall back to Unbound, similar to the union case (see above).
                        .unwrap_or_default()
                }))
            }

            Type::Dynamic(_) if policy.require_concrete() => Some(Place::Undefined.into()),

            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => Some(Place::bound(self).into()),

            Type::ClassLiteral(class) if class.is_typed_dict(db) => {
                Some(class.typed_dict_member(db, env, None, name, policy))
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
                    (Some(KnownClass::Property | KnownClass::EnumProperty), "__get__") => Some(
                        Place::bound(Type::WrapperDescriptor(
                            WrapperDescriptorKind::PropertyDunderGet,
                        ))
                        .into(),
                    ),
                    (Some(KnownClass::Property | KnownClass::EnumProperty), "__set__") => Some(
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

                    _ => Some(
                        class
                            .class_member(db, env, name, policy)
                            .map_type(|member| property_wrapper_descriptor(db, env, name, member)),
                    ),
                }
            }

            Type::GenericAlias(alias) if alias.is_typed_dict(db) => {
                Some(alias.origin(db).typed_dict_member(
                    db,
                    env,
                    (name == "__init__").then_some(alias.specialization(db)),
                    name,
                    policy,
                ))
            }

            Type::GenericAlias(alias) => Some(
                ClassType::from(*alias)
                    .class_member(db, env, name, policy)
                    .map_type(|member| property_wrapper_descriptor(db, env, name, member)),
            ),

            Type::SubclassOf(subclass_of_ty) => {
                subclass_of_ty.find_name_in_mro_with_policy(db, env, name, policy)
            }

            // Note: `super(pivot, owner).__class__` is `builtins.super`, not the owner's class.
            // `BoundSuper` should look up the name in the MRO of `builtins.super`.
            Type::BoundSuper(_) => KnownClass::Super
                .to_class_literal(db, env)
                .find_name_in_mro_with_policy(db, env, name, policy),

            // We eagerly normalize type[object], i.e. Type::SubclassOf(object) to `type`,
            // i.e. Type::NominalInstance(type). So looking up a name in the MRO of
            // `Type::NominalInstance(type)` is equivalent to looking up the name in the
            // MRO of the class `object`.
            Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Type) => {
                if policy.mro_no_object_fallback() {
                    Some(Place::Undefined.into())
                } else {
                    KnownClass::Object
                        .to_class_literal(db, env)
                        .find_name_in_mro_with_policy(db, env, name, policy)
                }
            }

            Type::TypeAlias(alias) => alias
                .value_type(db)
                .find_name_in_mro_with_policy(db, env, name, policy),

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
            | Type::SlotDescriptor(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_)
            | Type::EnumComplement(_)
            | Type::NewTypeInstance(_) => None,
        }
    }

    fn lookup_dunder_new(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<PlaceAndQualifiers<'db>> {
        #[salsa::tracked(returns(copy), cycle_initial=|_, _, _, _| None, heap_size=ruff_memory_usage::heap_size)]
        fn lookup_dunder_new_inner<'db>(
            db: &'db dyn Db,
            program: Program<'db>,
            ty: Type<'db>,
        ) -> Option<PlaceAndQualifiers<'db>> {
            let env = &ProgramEnvironment::from_program(program);
            let mut flags = MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK;
            if !ty.is_subtype_of(db, env, KnownClass::Type.to_instance(db, env)) {
                flags |= MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK;
            }
            ty.find_name_in_mro_with_policy(db, env, "__new__", flags)
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

    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, id, _| Place::bound(Type::divergent(id)).into(),
        cycle_fn=|db, cycle, previous: &PlaceAndQualifiers<'db>, member: PlaceAndQualifiers<'db>, key: MemberLookupKey<'db>| {
            member.cycle_normalized(db, &ProgramEnvironment::from_program(key.program(db)), *previous, cycle)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    fn class_member_with_policy_inner(
        db: &'db dyn Db,
        key: MemberLookupKey<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let ty = key.ty(db);
        let name = key.name(db);
        let policy = key.policy(db);
        let program = key.program(db);
        let env = &ProgramEnvironment::from_program(program);

        tracing::trace!("class_member: {}.{}", ty.display(db, env), name);
        if let Some(fallback) = ty.materialized_divergent_fallback() {
            return fallback.class_member_with_policy(db, env, name, policy);
        }
        if let Type::ProtocolInstance(protocol) = ty
            && let Some(origin) = protocol.materialized_origin(db)
        {
            let interface = protocol.interface(db);
            return if interface.includes_member(db, name) {
                interface.instance_member(db, env, name)
            } else {
                Type::instance(db, env, *origin).class_member_with_policy(db, env, name, policy)
            };
        }

        match ty {
            Type::Union(union) => union.map_with_boundness_and_qualifiers(db, env, |elem| {
                elem.class_member_with_policy(db, env, name, policy)
            }),
            Type::Intersection(inter) => inter.map_with_boundness_and_qualifiers(db, env, |elem| {
                elem.class_member_with_policy(db, env, name, policy)
            }),
            Type::TypedDict(TypedDictType::Synthesized(synthesized)) => {
                class::synthesized_typed_dict_class_member(db, env, synthesized, policy, name)
            }
            // TODO: Remove this once synthesized protocols have a precise meta-type.
            Type::ProtocolInstance(protocol) if protocol.class_origin(db).is_none() => {
                ty.instance_member(db, env, name)
            }

            Type::LiteralValue(literal)
                if name == "__len__"
                    && let Some(length) = match literal.kind() {
                        LiteralValueTypeKind::Bytes(bytes) => Some(bytes.python_len(db)),
                        LiteralValueTypeKind::String(string) => Some(string.python_len(db)),
                        _ => None,
                    }
                    && let Ok(length) = i64::try_from(length) =>
            {
                let parameters = Parameters::standard([Parameter::positional_only(Some(
                    Name::new_static("self"),
                ))
                .with_annotated_type(ty)]);
                Place::bound(Type::function_like_callable(
                    db,
                    Signature::new(parameters, Type::int_literal(length)),
                ))
                .into()
            }

            // `type[Any]` (or `type[Unknown]`, etc.) has an unknown metaclass, but all
            // metaclasses inherit from `type`. Check `type`'s class-level attributes
            // first so that data descriptors like `__mro__` and `__bases__` resolve to
            // their correct types instead of collapsing to `Any`/`Unknown`.
            Type::SubclassOf(subclass_of) if subclass_of.is_dynamic() => {
                let type_result = KnownClass::Type
                    .to_class_literal(db, env)
                    .find_name_in_mro_with_policy(db, env, name, policy)
                    .expect("`find_name_in_mro` should return `Some` for a class literal");
                if !type_result.place.is_undefined() {
                    type_result
                } else {
                    ty.to_meta_type(db, env)
                        .find_name_in_mro_with_policy(db, env, name, policy)
                        .expect(
                            "`Type::find_name_in_mro()` should return `Some()` \
                            when called on a meta-type",
                        )
                }
            }

            Type::NominalInstance(instance) => ty.to_meta_type(db, env).class_namespace_member(
                db,
                env,
                instance.class(db, env),
                name,
                policy,
            ),

            Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_) => ty
                .to_meta_type(db, env)
                .class_object_member(db, env, name, policy),

            _ => ty
                .to_meta_type(db, env)
                .find_name_in_mro_with_policy(db, env, name, policy)
                .expect(
                    "`Type::find_name_in_mro()` should return `Some()` \
                    when called on a meta-type",
                ),
        }
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
    ) -> PlaceAndQualifiers<'db> {
        let ty = key.ty(db);

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
        let class_attr = self
            .find_name_in_mro_with_policy(db, env, name, policy)
            .expect(
                "Calling `class_object_member` on class literals and subclass-of types \
                should always find an MRO",
            );

        let own_class = match self {
            Type::SubclassOf(subclass_of) => match subclass_of.subclass_of() {
                SubclassOfInner::Protocol(protocol) => {
                    protocol.class_origin(db).map(|origin| *origin)
                }
                subclass_of => subclass_of.into_class(db, env),
            },
            _ => self.to_class_type(db),
        };
        let own_class_attr =
            own_class.map(|class| class.own_class_member(db, env, None, name).inner);

        // A definitely-declared attribute in this class's own namespace is the contract for
        // values populated by metaclass initialization, analogous to a declared instance
        // attribute initialized in `__init__`. An inherited declaration does not mask a value
        // that the metaclass stores directly on the newly constructed subclass.
        let own_declaration_definedness = match own_class_attr {
            Some(PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        origin: TypeOrigin::Declared,
                        definedness,
                        ..
                    }),
                ..
            }) => Some(definedness),
            _ => None,
        };
        if own_declaration_definedness == Some(Definedness::AlwaysDefined) {
            return class_attr;
        }

        let Some(metaclass_instance) = self
            .to_meta_type(db, env)
            .to_instance_approximation(db, env)
        else {
            return class_attr;
        };
        let metaclass_attr = metaclass_instance.instance_member(db, env, name);

        if own_declaration_definedness.is_some() {
            // A conditionally-declared attribute is a contract only on paths where that
            // declaration is present; the metaclass value is the fallback on other paths.
            class_attr.or_fall_back_to(db, env, || metaclass_attr)
        } else {
            metaclass_attr.or_fall_back_to(db, env, || class_attr)
        }
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
        let class_attr = self
            .find_name_in_mro_with_policy(db, env, name, policy)
            .expect("The meta-type of an instance-like type should always have an MRO");
        let Some(metaclass) = class
            .inferred_metaclass(db)
            .for_inheritance(db, env)
            .to_instance_approximation(db, env)
            .and_then(|metaclass| metaclass.nominal_class(db, env))
        else {
            return class_attr;
        };
        let metaclass_member = metaclass.instance_member(db, env, name);
        if metaclass_member.is_undefined() {
            return class_attr;
        }
        let metaclass_member_is_implicit = metaclass_member
            .qualifiers
            .contains(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE);

        let own_class_member = class.class_literal(db).class_member_from_mro(
            db,
            env,
            name,
            policy,
            class.iter_mro(db).take(1),
        );
        // A non-ClassVar declaration-only member describes instance storage but does not add a
        // value to the class namespace.
        let own_class_member = if !own_class_member.is_class_var()
            && class.static_class_literal(db).is_some_and(|(class, _)| {
                let scope = class.body_scope(db);
                place_table(db, scope)
                    .symbol_id(name)
                    .is_some_and(|symbol| {
                        place_from_bindings(
                            db,
                            env,
                            use_def_map(db, scope).end_of_scope_symbol_bindings(symbol),
                        )
                        .place
                        .is_undefined()
                    })
            }) {
            PlaceAndQualifiers::default()
        } else {
            own_class_member
        };
        let inherited_class_member = class.class_literal(db).class_member_from_mro(
            db,
            env,
            name,
            policy,
            class.iter_mro(db).skip(1),
        );

        let metaclass_member = if metaclass_member_is_implicit {
            Self::with_definedness(metaclass_member, Definedness::PossiblyUndefined)
        } else {
            metaclass_member
        };
        let class_member = own_class_member
            .or_fall_back_to(db, env, || metaclass_member)
            .or_fall_back_to(db, env, || inherited_class_member);
        let class_member = if metaclass_member_is_implicit {
            // Preserve the existing convention that an inferred instance member is assumed to be
            // available even when no lower-precedence fallback exists.
            Self::with_definedness(class_member, Definedness::AlwaysDefined)
        } else {
            class_member
        };
        if policy.no_instance_fallback() || policy.require_concrete() {
            return class_member;
        }
        let Some(dynamic_instance_type) = class.iter_mro(db).find_map(|base| match base {
            ClassBase::Any | ClassBase::Dynamic(_) | ClassBase::Divergent(_) => {
                Some(Type::from(base))
            }
            _ => None,
        }) else {
            return class_member;
        };
        let dynamic_instance_fallback = Place::bound(dynamic_instance_type).into();

        // A dynamic base can provide arbitrary instance storage that shadows non-data class
        // attributes. Preserve only the data-descriptor alternatives before falling back to the
        // actual dynamic type.
        let Some(class_member_ty) = class_member.ignore_possibly_undefined() else {
            return dynamic_instance_fallback;
        };
        if !class_member_ty.may_be_data_descriptor(db, env) {
            return dynamic_instance_fallback;
        }
        let PlaceAndQualifiers {
            place: Place::Defined(declaration),
            qualifiers,
        } = class_member
        else {
            return dynamic_instance_fallback;
        };
        let mut all_arms_are_possible_data_descriptors = true;
        let descriptor_ty = declaration.ty.filter_union(db, env, |ty| {
            let is_possible_data_descriptor = ty.may_be_data_descriptor(db, env);
            all_arms_are_possible_data_descriptors &= is_possible_data_descriptor;
            is_possible_data_descriptor
        });
        Place::Defined(DefinedPlace {
            ty: descriptor_ty,
            definedness: if all_arms_are_possible_data_descriptors {
                declaration.definedness
            } else {
                Definedness::PossiblyUndefined
            },
            ..declaration
        })
        .with_qualifiers(qualifiers)
        .or_fall_back_to(db, env, || dynamic_instance_fallback)
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

            Type::Dynamic(_) | Type::Divergent(_) | Type::Never => Place::bound(self).into(),

            Type::NominalInstance(instance) => {
                instance.class(db, env).instance_member(db, env, name)
            }
            Type::NewTypeInstance(newtype) => newtype
                .concrete_base_type(db)
                .instance_member(db, env, name),

            Type::ProtocolInstance(protocol) => protocol.instance_member(db, env, name),

            Type::FunctionLiteral(_) => KnownClass::FunctionType
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
                match bound_typevar.typevar(db).bound_or_constraints(db, env) {
                    None => Type::object().instance_member(db, env, name),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.instance_member(db, env, name)
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
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
                .map_or(Place::Undefined, |member| member.place)
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
        #[salsa::tracked(returns(copy), cycle_initial=|_, _, _, _, _, _| Ok(None), heap_size=ruff_memory_usage::heap_size)]
        fn try_call_dunder_get_inner<'db>(
            db: &'db dyn Db,
            program: Program<'db>,
            ty: Type<'db>,
            instance: Option<Type<'db>>,
            owner: Type<'db>,
        ) -> Result<Option<DescriptorGetResult<'db>>, DescriptorGetError<'db>> {
            let env = &ProgramEnvironment::from_program(program);
            if let Some(fallback) = ty.materialized_divergent_fallback() {
                return fallback.try_call_dunder_get(db, env, instance, owner);
            }

            if let Some(dynamic) = ty.dynamic_descriptor_type() {
                return Ok(Some(DescriptorGetResult {
                    return_type: dynamic,
                    kind: AttributeKind::DataDescriptor,
                }));
            }

            if let Some(union) = ty.as_union_like(db) {
                let mut return_types = UnionBuilder::new(db, env);
                let mut error = None;
                let mut any_descriptor = false;
                let mut all_data_descriptors = true;

                for alternative in union.elements(db) {
                    let result = alternative
                        .try_call_dunder_get(db, env, instance, owner)
                        .unwrap_or_else(|failure| {
                            error = error.or(Some(failure.context));
                            Some(failure.fallback())
                        });
                    if let Some(DescriptorGetResult { return_type, kind }) = result {
                        any_descriptor = true;
                        all_data_descriptors &= kind.is_data();
                        return_types = return_types.add(return_type);
                    } else {
                        all_data_descriptors = false;
                        return_types = return_types.add(*alternative);
                    }
                }

                return if any_descriptor {
                    descriptor_get_result(
                        return_types.build(),
                        if all_data_descriptors {
                            AttributeKind::DataDescriptor
                        } else {
                            AttributeKind::NormalOrNonDataDescriptor
                        },
                        error,
                    )
                } else {
                    Ok(None)
                };
            }

            match ty {
                Type::Callable(callable) if callable.is_staticmethod_like(db) => {
                    // For "staticmethod-like" callables, model the behavior of `staticmethod.__get__`.
                    // The underlying function is returned as-is, without binding self.
                    return Ok(Some(DescriptorGetResult {
                        return_type: ty,
                        kind: AttributeKind::NormalOrNonDataDescriptor,
                    }));
                }
                Type::Callable(callable)
                    if let is_function_like = callable.is_function_like(db)
                        && (is_function_like || callable.is_classmethod_like(db)) =>
                {
                    // For "function-like" or "classmethod-like" callables, model the behavior of
                    // `FunctionType.__get__` or `classmethod.__get__`.
                    //
                    // It is a shortcut to model this in `try_call_dunder_get`. If we
                    // want to be really precise, we should instead return a new method-wrapper
                    // type variant for the synthesized `__get__` method of these synthesized
                    // functions. The method-wrapper would then be returned from
                    // `find_name_in_mro` when called on function-like `Callable`s. This would
                    // allow us to correctly model the behavior of *explicit*
                    // `SomeDataclass.__init__.__get__` calls.
                    let return_type = if instance.is_none() && is_function_like {
                        ty
                    } else {
                        let self_type = instance.unwrap_or_else(|| {
                            // For classmethod-like callables, bind to the owner class.
                            owner.to_instance_approximation(db, env).unwrap_or(owner)
                        });

                        Type::Callable(callable.bind_self(db, env, Some(self_type)))
                    };

                    return Ok(Some(DescriptorGetResult {
                        return_type,
                        kind: AttributeKind::NormalOrNonDataDescriptor,
                    }));
                }
                _ => {}
            }

            let Place::Defined(DefinedPlace {
                ty: concrete_descr_get,
                ..
            }) = ty
                .class_member_with_policy(db, env, "__get__", MemberLookupPolicy::REQUIRE_CONCRETE)
                .place
            else {
                return Ok(None);
            };

            // A recursive member lookup can yield the internal cycle marker. It does not
            // represent a concrete descriptor method and must not escape through the access.
            if concrete_descr_get.is_divergent() {
                return Ok(None);
            }

            // Descriptor special-method lookup checks the descriptor's type, so instance storage
            // cannot shadow `__get__`. Dynamic MRO entries still participate in the lookup.
            let Place::Defined(DefinedPlace {
                ty: descr_get,
                definedness: descr_get_boundness,
                ..
            }) = ty
                .class_member_with_policy(
                    db,
                    env,
                    "__get__",
                    MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place
            else {
                return Ok(None);
            };

            let instance_ty = instance.unwrap_or_else(|| Type::none(db, env));
            let kind = if ty.is_data_descriptor(db, env) {
                AttributeKind::DataDescriptor
            } else {
                AttributeKind::NormalOrNonDataDescriptor
            };
            let (return_type, error) = match descr_get.try_call(
                db,
                env,
                &CallArguments::positional([ty, instance_ty, owner]),
            ) {
                Ok(bindings) => (bindings.return_type(db, env), None),
                Err(error) => (
                    error.return_type(db, env),
                    Some(DescriptorGetCallContext::new(
                        db, ty, descr_get, instance, owner,
                    )),
                ),
            };
            let return_type = if descr_get_boundness == Definedness::AlwaysDefined {
                return_type
            } else {
                UnionType::from_two_elements(db, env, return_type, ty)
            };

            descriptor_get_result(return_type, kind, error)
        }

        tracing::trace!(
            "try_call_dunder_get: {}, {}, {}",
            self.display(db, env),
            instance
                .unwrap_or_else(|| Type::none(db, env))
                .display(db, env),
            owner.display(db, env)
        );

        // Function descriptors have fixed binding behavior, so avoid retaining a tracked query
        // for every function and access context.
        if let Type::FunctionLiteral(function) = self {
            let return_type = if function.is_classmethod(db) {
                Type::BoundMethod(BoundMethodType::new(db, function, owner, owner))
            } else if let Some(instance) = instance
                && !function.is_staticmethod(db)
            {
                Type::BoundMethod(BoundMethodType::new(db, function, instance, instance))
            } else {
                self
            };

            return Ok(Some(DescriptorGetResult {
                return_type,
                kind: AttributeKind::NormalOrNonDataDescriptor,
            }));
        }

        // The interpreter returns the descriptor itself on class access and its stored value on
        // instance access; no Python property accessors participate in either operation.
        if let Type::SlotDescriptor(descriptor) = self {
            return Ok(Some(DescriptorGetResult {
                return_type: instance.map_or(self, |_| descriptor.value_type(db)),
                kind: AttributeKind::DataDescriptor,
            }));
        }

        try_call_dunder_get_inner(db, env.program(db), self, instance, owner)
    }

    /// Look up `__get__` on the meta-type of `attribute`, and call it with `attribute`, `instance`,
    /// and `owner` as arguments. This method exists as a separate step as we need to handle unions
    /// and intersections explicitly.
    fn try_call_dunder_get_on_attribute(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        attribute: PlaceAndQualifiers<'db>,
        instance: Option<Type<'db>>,
        owner: Type<'db>,
    ) -> (
        PlaceAndQualifiers<'db>,
        AttributeKind,
        Option<DescriptorGetCallContext<'db>>,
    ) {
        if let PlaceAndQualifiers {
            place:
                Place::Defined(DefinedPlace {
                    ty,
                    origin,
                    definedness,
                    public_type_policy,
                    provenance,
                }),
            qualifiers,
        } = attribute
            && let Some(fallback) = ty.materialized_divergent_fallback()
        {
            return Self::try_call_dunder_get_on_attribute(
                db,
                env,
                Place::Defined(DefinedPlace {
                    ty: fallback,
                    origin,
                    definedness,
                    public_type_policy,
                    provenance,
                })
                .with_qualifiers(qualifiers),
                instance,
                owner,
            );
        }

        let (member, kind, error) = match attribute {
            // A directly dynamic attribute could be a data descriptor even though we cannot see
            // its methods. Preserve that uncertainty, along with the existing bottom and cycle
            // behavior, without performing member lookups that cannot add information.
            PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        ty: Type::Dynamic(_) | Type::Divergent(_) | Type::Never,
                        ..
                    }),
                qualifiers: _,
            } => (attribute, AttributeKind::DataDescriptor, None),

            PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        ty: Type::Union(union),
                        origin,
                        definedness: boundness,
                        public_type_policy,
                        provenance: attribute_provenance,
                    }),
                qualifiers,
            } => {
                let mut all_data_descriptors = true;
                let mut error = None;
                let place = union
                    .map_with_boundness(db, env, |elem| {
                        let result = elem
                            .try_call_dunder_get(db, env, instance, owner)
                            .unwrap_or_else(|failure| {
                                error = error.or(Some(failure.context));
                                Some(failure.fallback())
                            });
                        let ty = match result {
                            Some(DescriptorGetResult { return_type, kind }) => {
                                all_data_descriptors &= kind.is_data();
                                return_type
                            }
                            None => {
                                all_data_descriptors = false;
                                *elem
                            }
                        };

                        Place::Defined(DefinedPlace {
                            ty,
                            origin,
                            definedness: boundness,
                            public_type_policy,
                            provenance: attribute_provenance,
                        })
                    })
                    .with_qualifiers(qualifiers);

                let kind = if all_data_descriptors {
                    AttributeKind::DataDescriptor
                } else {
                    AttributeKind::NormalOrNonDataDescriptor
                };

                (place, kind, error)
            }

            attribute @ PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        ty: Type::Intersection(intersection),
                        origin,
                        definedness,
                        public_type_policy,
                        provenance: attribute_provenance,
                    }),
                qualifiers,
            } => {
                let mut error = None;
                let place = if intersection.positive(db).is_empty() {
                    attribute
                } else {
                    intersection
                        .map_with_boundness(db, env, |elem| {
                            let ty = elem
                                .try_call_dunder_get(db, env, instance, owner)
                                .unwrap_or_else(|failure| {
                                    error = error.or(Some(failure.context));
                                    Some(failure.fallback())
                                })
                                .map_or(*elem, |result| result.return_type);
                            Place::Defined(DefinedPlace {
                                ty,
                                origin,
                                definedness,
                                public_type_policy,
                                provenance: attribute_provenance,
                            })
                        })
                        .with_qualifiers(qualifiers)
                };
                (
                    place,
                    // TODO: Discover data descriptors in intersections without decomposing the
                    // descriptor return type into an unsound intersection.
                    AttributeKind::NormalOrNonDataDescriptor,
                    error,
                )
            }

            PlaceAndQualifiers {
                place:
                    Place::Defined(DefinedPlace {
                        ty: attribute_ty,
                        origin,
                        definedness: boundness,
                        public_type_policy,
                        provenance,
                    }),
                qualifiers: _,
            } => {
                let mut error = None;
                let result = attribute_ty
                    .try_call_dunder_get(db, env, instance, owner)
                    .unwrap_or_else(|failure| {
                        error = Some(failure.context);
                        Some(failure.fallback())
                    });
                if let Some(DescriptorGetResult { return_type, kind }) = result {
                    (
                        Place::Defined(DefinedPlace {
                            ty: return_type,
                            origin,
                            definedness: boundness,
                            public_type_policy,
                            provenance,
                        })
                        .into(),
                        kind,
                        error,
                    )
                } else {
                    (attribute, AttributeKind::NormalOrNonDataDescriptor, None)
                }
            }

            _ => (attribute, AttributeKind::NormalOrNonDataDescriptor, None),
        };

        (member, kind, error)
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
            Type::NominalInstance(instance) if instance.has_known_class(db, KnownClass::Type) => {
                false
            }
            Type::TypeForm(_) | Type::SubclassOf(_) => false,
            _ => !self.may_be_data_descriptor(db, env),
        }
    }

    // Definite data descriptors use an all-of union fold; possible data descriptors use any-of.
    // Seed recursive aliases with the corresponding identity value.
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, _, _, any_of_union: bool| !any_of_union,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn is_data_descriptor_impl(
        self,
        db: &'db dyn Db,
        program: Program<'db>,
        any_of_union: bool,
    ) -> bool {
        let env = &ProgramEnvironment::from_program(program);
        match self {
            Type::Dynamic(_) => !any_of_union,
            Type::SubclassOf(_) if self.dynamic_descriptor_type().is_some() => true,
            Type::Never | Type::PropertyInstance(_) | Type::SlotDescriptor(_) => true,
            Type::Union(union) if any_of_union => union
                .elements(db)
                .iter()
                .any(|ty| ty.is_data_descriptor_impl(db, program, any_of_union)),
            Type::Union(union) => union
                .elements(db)
                .iter()
                .all(|ty| ty.is_data_descriptor_impl(db, program, any_of_union)),
            Type::Intersection(intersection) => intersection
                .iter_positive(db)
                .any(|ty| ty.is_data_descriptor_impl(db, program, any_of_union)),
            Type::TypeAlias(alias) => {
                alias
                    .value_type(db)
                    .is_data_descriptor_impl(db, program, any_of_union)
            }
            _ => {
                !self
                    .class_member_with_policy(
                        db,
                        env,
                        "__set__",
                        MemberLookupPolicy::REQUIRE_CONCRETE,
                    )
                    .place
                    .is_undefined()
                    || !self
                        .class_member_with_policy(
                            db,
                            env,
                            "__delete__",
                            MemberLookupPolicy::REQUIRE_CONCRETE,
                        )
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
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
        fallback: MemberLookupResult<'db>,
        policy: InstanceFallbackShadowsNonDataDescriptor,
    ) -> MemberLookupResult<'db> {
        let meta_attr_plain = Self::instance_lookup_class_member_with_policy(db, env, key);
        let meta_attr_ty = meta_attr_plain.place.ignore_possibly_undefined();
        // Preserve the receiver's type variables and all its narrowed class constraints.
        let owner = receiver.to_meta_type(db, env);
        let (
            PlaceAndQualifiers {
                place: meta_attr,
                qualifiers: meta_attr_qualifiers,
            },
            meta_attr_kind,
            meta_attr_error,
        ) = Self::try_call_dunder_get_on_attribute(db, env, meta_attr_plain, Some(receiver), owner);

        let meta_attr_error = meta_attr_error.map(MemberLookupErrorKind::DescriptorGet);
        let fallback_error = fallback.err().map(|error| error.kind(db));
        let fallback_member = fallback.unwrap_or_else(|error| error.fallback_member(db));

        // A slot stores the same instance attribute described by the receiver's declarations.
        // Unlike an arbitrary data descriptor, its inherited getter must not hide a more precise
        // declaration established by the receiver's class.
        if matches!(meta_attr, Place::Defined(_))
            && matches!(meta_attr_ty, Some(Type::SlotDescriptor(_)))
            && !fallback_member.place.is_undefined()
        {
            return member_lookup_result(db, fallback_member, fallback_error);
        }

        let PlaceAndQualifiers {
            place: fallback,
            qualifiers: fallback_qualifiers,
        } = fallback_member;

        match (meta_attr, meta_attr_kind, fallback) {
            // The fallback type is unbound, so we can just return `meta_attr` unconditionally,
            // no matter if it's data descriptor, a non-data descriptor, or a normal attribute.
            (meta_attr @ Place::Defined(_), _, Place::Undefined) => member_lookup_result(
                db,
                meta_attr.with_qualifiers(meta_attr_qualifiers),
                meta_attr_error,
            ),

            // `meta_attr` is the return type of a data descriptor and definitely bound, so we
            // return it.
            (
                meta_attr @ Place::Defined(DefinedPlace {
                    definedness: Definedness::AlwaysDefined,
                    ..
                }),
                AttributeKind::DataDescriptor,
                _,
            ) => member_lookup_result(
                db,
                meta_attr.with_qualifiers(meta_attr_qualifiers),
                meta_attr_error,
            ),

            // `meta_attr` is the return type of a data descriptor, but the attribute on the
            // meta-type is possibly-unbound. This means that we "fall through" to the next
            // stage of the descriptor protocol and union with the fallback type.
            (
                Place::Defined(DefinedPlace {
                    ty: meta_attr_ty,
                    origin: meta_origin,
                    definedness: Definedness::PossiblyUndefined,
                    provenance: meta_attr_provenance,
                    ..
                }),
                AttributeKind::DataDescriptor,
                Place::Defined(DefinedPlace {
                    ty: fallback_ty,
                    origin: fallback_origin,
                    definedness: fallback_boundness,
                    public_type_policy: fallback_public_type_policy,
                    provenance: fallback_provenance,
                }),
            ) => member_lookup_result(
                db,
                Place::Defined(DefinedPlace {
                    ty: UnionType::from_two_elements(db, env, meta_attr_ty, fallback_ty),
                    origin: meta_origin.merge(fallback_origin),
                    definedness: fallback_boundness,
                    public_type_policy: fallback_public_type_policy,
                    provenance: fallback_provenance.or(meta_attr_provenance),
                })
                .with_qualifiers(meta_attr_qualifiers.union(fallback_qualifiers)),
                meta_attr_error.or(fallback_error),
            ),

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
            ) if policy == InstanceFallbackShadowsNonDataDescriptor::Yes => member_lookup_result(
                db,
                fallback.with_qualifiers(fallback_qualifiers),
                fallback_error,
            ),

            // `meta_attr` is *not* a data descriptor. The `fallback` symbol is either possibly
            // unbound or the policy argument is `No`. In both cases, the `fallback` type does
            // not completely shadow the non-data descriptor, so we build a union of the two.
            (
                Place::Defined(DefinedPlace {
                    ty: meta_attr_ty,
                    origin: meta_origin,
                    definedness: meta_attr_boundness,
                    provenance: meta_attr_provenance,
                    ..
                }),
                AttributeKind::NormalOrNonDataDescriptor,
                Place::Defined(DefinedPlace {
                    ty: fallback_ty,
                    origin: fallback_origin,
                    definedness: fallback_boundness,
                    public_type_policy: fallback_public_type_policy,
                    provenance: fallback_provenance,
                }),
            ) => member_lookup_result(
                db,
                Place::Defined(DefinedPlace {
                    ty: UnionType::from_two_elements(db, env, meta_attr_ty, fallback_ty),
                    origin: meta_origin.merge(fallback_origin),
                    definedness: meta_attr_boundness.max(fallback_boundness),
                    public_type_policy: fallback_public_type_policy,
                    provenance: fallback_provenance.or(meta_attr_provenance),
                })
                .with_qualifiers(meta_attr_qualifiers.union(fallback_qualifiers)),
                meta_attr_error.or(fallback_error),
            ),

            // If the attribute is not found on the meta-type, we simply return the fallback.
            (Place::Undefined, _, fallback) => member_lookup_result(
                db,
                fallback.with_qualifiers(fallback_qualifiers),
                fallback_error,
            ),
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
        #[salsa::tracked(
            returns(copy),
            cycle_initial=|_, id, _| Ok(Place::bound(Type::divergent(id)).into()),
            cycle_fn=|db, cycle, previous: &MemberLookupResult<'db>, member: MemberLookupResult<'db>, key: MemberLookupKey<'db>| {
                cycle_normalized_member_lookup(db, &ProgramEnvironment::from_program(key.program(db)), member, *previous, cycle)
            },
            heap_size=ruff_memory_usage::heap_size
        )]
        fn member_lookup_with_policy_inner<'db>(
            db: &'db dyn Db,
            key: MemberLookupKey<'db>,
        ) -> MemberLookupResult<'db> {
            member_lookup_with_policy_impl(db, key, None)
        }

        #[salsa::tracked(
            returns(copy),
            cycle_initial=|_, id, _, _| Ok(Place::bound(Type::divergent(id)).into()),
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
            member_lookup_with_policy_impl(db, key, Some(receiver))
        }

        fn member_lookup_with_policy_impl<'db>(
            db: &'db dyn Db,
            key: MemberLookupKey<'db>,
            receiver: Option<Type<'db>>,
        ) -> MemberLookupResult<'db> {
            fn promote_inferred_attribute_class_literals<'db>(
                db: &'db dyn Db,
                env: &ProgramEnvironment<'db>,
                result: MemberLookupResult<'db>,
            ) -> MemberLookupResult<'db> {
                let member = result.unwrap_or_else(|error| error.fallback_member(db));
                let should_promote = matches!(
                    member.place,
                    Place::Defined(DefinedPlace {
                        origin: TypeOrigin::Inferred,
                        ..
                    })
                ) && !member.qualifiers.contains(TypeQualifiers::FINAL);

                if should_promote {
                    map_member_lookup_type(db, result, |ty| ty.promote_class_literals(db, env))
                } else {
                    result
                }
            }

            fn instance_like_member_lookup<'db>(
                db: &'db dyn Db,
                env: &ProgramEnvironment<'db>,
                key: MemberLookupKey<'db>,
                receiver: Type<'db>,
            ) -> MemberLookupResult<'db> {
                let this = key.ty(db);
                let name = key.name(db);
                let name_str = name.as_str();

                // Enum members can be accessed through enum instances and other enum members,
                // e.g. `answer.YES` or `Answer.YES.NO`.
                if let Some(enum_class) = match this {
                    Type::LiteralValue(literal) => literal
                        .as_enum()
                        .map(|enum_literal| enum_literal.enum_class_literal(db)),
                    _ => this
                        .nominal_class(db, env)
                        .map(|class| class.class_literal(db))
                        .and_then(|class| class.into_enum_class(db)),
                } && let Some(resolved_name) = enum_class.resolve_member(db, name)
                {
                    return Place::bound(Type::enum_literal(EnumLiteralType::new(
                        db,
                        enum_class,
                        resolved_name,
                    )))
                    .into();
                }

                let fallback = this.instance_member(db, env, name_str);

                let result = Type::invoke_descriptor_protocol(
                    db,
                    env,
                    key,
                    receiver,
                    fallback.into(),
                    InstanceFallbackShadowsNonDataDescriptor::No,
                );

                if result
                    .unwrap_or_else(|error| error.fallback_member(db))
                    .is_class_var()
                    && this.is_typed_dict()
                {
                    // `ClassVar`s on `TypedDictFallback` cannot be accessed on inhabitants of `SomeTypedDict`.
                    // They can only be accessed on `SomeTypedDict` directly.
                    return Place::Undefined.into();
                }

                let result = this.fallback_to_getattr(db, env, name, result, key.policy(db));
                // An inferred attribute accessed through an instance can resolve to an override
                // on a subclass, so an exact class object is not a safe public type here.
                let result = map_member_lookup_type(db, result, |ty| {
                    ty.bind_self_typevars(db, env, receiver)
                });
                promote_inferred_attribute_class_literals(db, env, result)
            }

            let program = key.program(db);
            let env = &ProgramEnvironment::from_program(program);
            let this = key.ty(db);
            let name = key.name(db);
            let name_str = name.as_str();
            let policy = key.policy(db);

            tracing::trace!(
                "member_lookup_with_policy: {}.{}",
                this.display(db, env),
                name
            );
            if let Some(fallback) = this.materialized_divergent_fallback() {
                return fallback
                    .member_lookup_with_policy_and_receiver(db, env, name_str, policy, receiver);
            }

            match this {
                Type::Union(union) => {
                    let mut error = None;
                    let member = union.map_with_boundness_and_qualifiers(db, env, |elem| {
                        let result = elem.member_lookup_with_policy_and_receiver(
                            db, env, name_str, policy, receiver,
                        );
                        error = error.or_else(|| result.err().map(|error| error.kind(db)));
                        result.unwrap_or_else(|error| error.fallback_member(db))
                    });
                    member_lookup_result(db, member, error)
                }

                Type::Intersection(intersection) => {
                    if let Some(complement) = intersection.enum_complement(db, env) {
                        enums::member_lookup_for_enum_complement(
                            db, env, complement, name_str, policy,
                        )
                        .into()
                    } else {
                        let receiver = Some(receiver.unwrap_or(this));
                        let mut error = None;
                        let member =
                            intersection.map_with_boundness_and_qualifiers(db, env, |elem| {
                                let result = elem.member_lookup_with_policy_and_receiver(
                                    db, env, name_str, policy, receiver,
                                );
                                error = error.or_else(|| result.err().map(|error| error.kind(db)));
                                result.unwrap_or_else(|error| error.fallback_member(db))
                            });
                        member_lookup_result(db, member, error)
                    }
                }

                Type::EnumComplement(complement) => {
                    enums::member_lookup_for_enum_complement(db, env, complement, name_str, policy)
                        .into()
                }

                Type::Dynamic(..) | Type::Divergent(_) | Type::Never => Place::bound(this).into(),

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

                Type::LiteralValue(literal)
                    if name == "startswith"
                        && let Some(string_literal) = literal.as_string() =>
                {
                    Place::bound(Type::KnownBoundMethod(KnownBoundMethodType::StrStartswith(
                        string_literal,
                    )))
                    .into()
                }

                Type::ClassLiteral(class)
                    if name == "lower_bound" && class.is_known(db, KnownClass::ConstraintSet) =>
                {
                    Place::bound(Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetLowerBound,
                    ))
                    .into()
                }
                Type::ClassLiteral(class)
                    if name == "upper_bound" && class.is_known(db, KnownClass::ConstraintSet) =>
                {
                    Place::bound(Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetUpperBound,
                    ))
                    .into()
                }
                Type::ClassLiteral(class)
                    if name == "equality" && class.is_known(db, KnownClass::ConstraintSet) =>
                {
                    Place::bound(Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetEquality,
                    ))
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
                    if name == "exists" =>
                {
                    Place::bound(Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetExists(tracked),
                    ))
                    .into()
                }
                Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked))
                    if name == "for_all" =>
                {
                    Place::bound(Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetForAll(tracked),
                    ))
                    .into()
                }
                Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked))
                    if name == "solutions_for" =>
                {
                    Place::bound(Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetSolutionsFor(tracked),
                    ))
                    .into()
                }
                Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked))
                    if name == "solutions" =>
                {
                    Place::bound(Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetSolutions(tracked),
                    ))
                    .into()
                }
                Type::KnownInstance(KnownInstanceType::ConstraintSet(tracked))
                    if name == "with_detailed_display" =>
                {
                    Place::bound(Type::KnownBoundMethod(
                        KnownBoundMethodType::ConstraintSetWithDetailedDisplay(tracked),
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
                Type::ClassLiteral(_) | Type::GenericAlias(_)
                    if matches!(name_str, "__get__" | "__set__" | "__delete__")
                        && let Some(wrapper @ Type::WrapperDescriptor(_)) = this
                            .find_name_in_mro_with_policy(db, env, name_str, policy)
                            .and_then(|member| member.place.ignore_possibly_undefined()) =>
                {
                    Place::bound(wrapper).into()
                }
                Type::BoundMethod(bound_method) => match name_str {
                    "__self__" => Place::bound(bound_method.self_instance(db)).into(),
                    "__func__" => {
                        Place::bound(Type::FunctionLiteral(bound_method.function(db))).into()
                    }
                    _ => {
                        let result = KnownClass::MethodType
                            .to_instance(db, env)
                            .member_lookup_with_policy_and_receiver(
                                db, env, name_str, policy, receiver,
                            );
                        member_lookup_or_fall_back_to(db, env, result, || {
                            // If an attribute is not available on the bound method object,
                            // it will be looked up on the underlying function object. This
                            // changes the lookup object, so do not forward the bound-method
                            // receiver.
                            Type::FunctionLiteral(bound_method.function(db))
                                .member_lookup_with_policy_and_receiver(
                                    db, env, name_str, policy, None,
                                )
                        })
                    }
                },
                Type::KnownBoundMethod(method) => method
                    .class()
                    .to_instance(db, env)
                    .member_lookup_with_policy_and_receiver(db, env, name_str, policy, receiver),
                Type::WrapperDescriptor(_) => KnownClass::WrapperDescriptorType
                    .to_instance(db, env)
                    .member_lookup_with_policy_and_receiver(db, env, name_str, policy, receiver),
                Type::DataclassDecorator(_) => KnownClass::FunctionType
                    .to_instance(db, env)
                    .member_lookup_with_policy_and_receiver(db, env, name_str, policy, receiver),

                Type::Callable(_) | Type::DataclassTransformer(_) if name_str == "__call__" => {
                    Place::bound(this).into()
                }

                Type::Callable(callable) if callable.is_function_like(db) => {
                    KnownClass::FunctionType
                        .to_instance(db, env)
                        .member_lookup_with_policy_and_receiver(db, env, name_str, policy, receiver)
                }

                Type::Callable(_) | Type::DataclassTransformer(_) => Type::object()
                    .member_lookup_with_policy_and_receiver(db, env, name_str, policy, receiver),

                Type::NominalInstance(instance)
                    if matches!(name_str, "major" | "minor") && instance.is_sys_version_info() =>
                {
                    let python_version = env.python_version(db);
                    let segment = if name == "major" {
                        python_version.major
                    } else {
                        python_version.minor
                    };
                    Place::bound(Type::int_literal(segment.into())).into()
                }

                Type::PropertyInstance(property) if name == "fget" => {
                    Place::bound(property.getter(db).unwrap_or(Type::none(db, env))).into()
                }
                Type::PropertyInstance(property) if name == "fset" => {
                    Place::bound(property.setter(db).unwrap_or(Type::none(db, env))).into()
                }
                Type::PropertyInstance(property) if name == "fdel" => {
                    Place::bound(property.deleter(db).unwrap_or(Type::none(db, env))).into()
                }

                Type::LiteralValue(literal)
                    if literal.is_int() && matches!(name_str, "real" | "numerator") =>
                {
                    Place::bound(this).into()
                }

                Type::LiteralValue(literal)
                    if matches!(name_str, "real" | "numerator")
                        && let Some(bool_value) = literal.as_bool() =>
                {
                    Place::bound(Type::int_literal(i64::from(bool_value))).into()
                }

                Type::ModuleLiteral(module) => module.static_member(db, env, name_str),

                // If a protocol does not include a member and the policy disables falling back to
                // `object`, we return `Place::Undefined` here. This short-circuits attribute lookup
                // before we find the "fallback to attribute access on `object`" logic later on
                // (otherwise we would infer that all synthesized protocols have `__getattribute__`
                // methods, and therefore that all synthesized protocols have all possible attributes.)
                //
                // Note that we could do this for *all* protocols, but it's only *necessary* for synthesized
                // ones, and the standard logic is *probably* more performant for class-based protocols?
                Type::ProtocolInstance(protocol)
                    if protocol.class_origin(db).is_none()
                        && policy.mro_no_object_fallback()
                        && !protocol.interface(db).includes_member(db, name_str) =>
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
                Type::NewTypeInstance(new_type_instance) if this.as_union_like(db).is_some() => {
                    new_type_instance
                        .concrete_base_type(db)
                        .member_lookup_with_policy_and_receiver(db, env, name_str, policy, None)
                }

                Type::TypeAlias(alias) => alias
                    .value_type(db)
                    .member_lookup_with_policy_and_receiver(db, env, name_str, policy, receiver),

                _ if policy.no_instance_fallback() => {
                    let receiver = receiver.unwrap_or(this);
                    let result = Type::invoke_descriptor_protocol(
                        db,
                        env,
                        key,
                        receiver,
                        Place::Undefined.into(),
                        InstanceFallbackShadowsNonDataDescriptor::No,
                    );
                    map_member_lookup_type(db, result, |ty| {
                        ty.bind_self_typevars(db, env, receiver)
                    })
                }

                Type::LiteralValue(literal)
                    if matches!(name_str, "name" | "_name_" | "value" | "_value_")
                        && let Some(enum_literal) = literal.as_enum()
                        && !enums::class_defines_property(
                            db,
                            env,
                            enum_literal.enum_class(db),
                            name_str,
                        ) =>
                {
                    let enum_class = enum_literal.enum_class_literal(db);
                    let is_enum_subclass = Type::ClassLiteral(enum_class.class_literal(db))
                        .is_subtype_of(db, env, KnownClass::Enum.to_subclass_of(db, env));

                    let ty = match name_str {
                        "name" if is_enum_subclass => {
                            enum_class.name_type(db, enum_literal.name(db))
                        }
                        "_name_" => enum_class.name_type(db, enum_literal.name(db)),
                        "value" if is_enum_subclass => {
                            enum_class.value_type(db, enum_literal.name(db))
                        }
                        "_value_" => enum_class.value_type(db, enum_literal.name(db)),
                        _ => None,
                    };

                    ty.map(Place::bound).unwrap_or_default().into()
                }

                Type::TypeVar(typevar)
                    if typevar.is_paramspec(db)
                        && let Some(attr) = ParamSpecAttrKind::from_name(name_str) =>
                {
                    Place::declared(Type::TypeVar(typevar.with_paramspec_attr(db, attr))).into()
                }
                Type::TypeVar(typevar) => {
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
                        instance_like_member_lookup(db, env, key, receiver)
                    }
                }

                Type::NominalInstance(instance)
                    if matches!(name_str, "name" | "_name_" | "value" | "_value_")
                        && let class_literal = instance.class_literal(db, env)
                        && let Some(metadata) = enum_metadata(db, class_literal)
                        && !enums::class_defines_property(db, env, class_literal, name_str) =>
                {
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

                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(partial))
                    if name_str == "__call__" =>
                {
                    Place::bound(Type::KnownInstance(
                        KnownInstanceType::FunctoolsPartialCall(partial),
                    ))
                    .into()
                }

                Type::KnownInstance(KnownInstanceType::FunctoolsPartialCall(_))
                    if name_str == "__call__" =>
                {
                    Place::bound(this).into()
                }

                Type::KnownInstance(KnownInstanceType::FunctoolsPartial(partial)) => {
                    let wrapped = partial.wrapped(db).inner(db);
                    let nominal_lookup = partial
                        .partial(db)
                        .into_functools_partial_instance(db, env)
                        .member_lookup_with_policy_and_receiver(
                            db, env, name_str, policy, receiver,
                        );
                    if name_str == "func" {
                        match nominal_lookup
                            .unwrap_or_else(|error| error.fallback_member(db))
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
                | Type::TypedDict(_) => {
                    let receiver = receiver.unwrap_or(this);
                    instance_like_member_lookup(db, env, key, receiver)
                }

                Type::ClassLiteral(..) | Type::GenericAlias(..) | Type::SubclassOf(..) => {
                    // A class-object lookup can originate from a TypeVar bound such as `type[A]`.
                    // Retain that TypeVar as the receiver so `Self` binds to `T'instance`, not `A`,
                    // unless its constraints also include non-class-object types.
                    let receiver = receiver
                        .filter(|receiver| receiver.to_instance_approximation(db, env).is_some())
                        .unwrap_or(this);
                    let enum_class = match this {
                        Type::ClassLiteral(literal) => literal.into_enum_class(db),
                        Type::SubclassOf(subclass_of) => subclass_of
                            .subclass_of()
                            .into_class(db, env)
                            .and_then(|class| class.class_literal(db).into_enum_class(db)),
                        _ => None,
                    };
                    if let Some(enum_class) = enum_class
                        && let Some(resolved_name) = enum_class.resolve_member(db, name)
                    {
                        return Place::bound(Type::enum_literal(EnumLiteralType::new(
                            db,
                            enum_class,
                            resolved_name,
                        )))
                        .into();
                    }

                    let class_attr_plain = this.class_object_member(db, env, name_str, policy);

                    let self_instance = receiver.to_instance_approximation(db, env).expect(
                        "The receiver for a class-object lookup should always be instantiable",
                    );
                    let class_attr_plain = class_attr_plain
                        .map_type(|ty| ty.bind_self_typevars(db, env, self_instance));

                    let (class_attr_fallback, _, class_attr_error) =
                        Type::try_call_dunder_get_on_attribute(
                            db,
                            env,
                            class_attr_plain,
                            None,
                            receiver,
                        );

                    let result = Type::invoke_descriptor_protocol(
                        db,
                        env,
                        key,
                        receiver,
                        member_lookup_result(
                            db,
                            class_attr_fallback,
                            class_attr_error.map(MemberLookupErrorKind::DescriptorGet),
                        ),
                        InstanceFallbackShadowsNonDataDescriptor::Yes,
                    );

                    // A class is an instance of its metaclass. If attribute lookup on the class
                    // fails, Python falls back to `type(cls).__getattr__` and
                    // `type(cls).__getattribute__` on the metaclass, analogous to how instance
                    // attribute access falls back to `__getattr__`/`__getattribute__` on the
                    // class. `try_call_dunder` adds `NO_INSTANCE_FALLBACK`, which causes the
                    // lookup to hit the catch-all that only checks the meta-type (the metaclass).
                    let result = this.fallback_to_getattr(db, env, name, result, policy);
                    // Unlike a specific class literal, `type[C]` can represent any subclass of
                    // `C`, unless a `TypeVar` upper bound normalizes to a final class.
                    let result = if let Type::SubclassOf(subclass_of) = this
                        && subclass_of.exact_typevar_upper_bound(db, env).is_none()
                    {
                        promote_inferred_attribute_class_literals(db, env, result)
                    } else {
                        result
                    };

                    // `type[Any]`/`type[Unknown]` are gradual forms with an unknown metaclass
                    // (which is at least `type`). Attributes resolved via `type`'s descriptors
                    // are intersected with the dynamic type to reflect uncertainty about
                    // whether the unknown metaclass overrides them.
                    if let Type::SubclassOf(subclass_of) = this
                        && let SubclassOfInner::Dynamic(dynamic) = subclass_of.subclass_of()
                    {
                        map_member_lookup_type(db, result, |ty| {
                            if ty.is_dynamic() {
                                ty
                            } else {
                                IntersectionType::from_two_elements(
                                    db,
                                    env,
                                    ty,
                                    Type::Dynamic(dynamic),
                                )
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
                    let owner_attr =
                        bound_super.find_name_in_mro_after_pivot(db, env, name_str, policy);

                    bound_super
                        .try_call_dunder_get_on_attribute(db, env, owner_attr)
                        .unwrap_or_else(|| owner_attr.into())
                }
            }
        }

        if self.materialized_divergent_fallback().is_none() {
            if name == "__class__" {
                return Place::bound(self.dunder_class(db, env)).into();
            }

            if matches!(self, Type::Dynamic(_) | Type::Divergent(_) | Type::Never) {
                return Place::bound(self).into();
            }
        }

        let key = MemberLookupKey::new(db, env.program(db), self, name, policy);
        match receiver {
            Some(receiver) => member_lookup_with_policy_and_receiver_inner(db, key, receiver),
            None => member_lookup_with_policy_inner(db, key),
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
        self.bindings_impl(db, env, &ActiveRecursionDetector::default())
    }

    fn bindings_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        recursion_guard: &ActiveRecursionDetector<Type<'db>>,
    ) -> Bindings<'db> {
        if let Some(fallback) = self.materialized_divergent_fallback() {
            return fallback.bindings_impl(db, env, recursion_guard);
        }

        match self {
            Type::Callable(callable) => {
                CallableBinding::from_overloads(self, callable.signatures(db).iter().cloned())
                    .into()
            }

            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db, env) {
                    None => CallableBinding::not_callable(self).into(),
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.bindings_impl(db, env, recursion_guard)
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        Bindings::from_union(
                            self,
                            constraints
                                .elements(db)
                                .iter()
                                .map(|ty| ty.bindings_impl(db, env, recursion_guard)),
                        )
                    }
                }
            }

            Type::BoundMethod(bound_method) => {
                let signature = bound_method.function(db).signature(db);
                let self_instance = bound_method.self_instance(db);
                let signature_receiver = bound_method.signature_receiver(db);
                // Class-based protocol member lookup has already specialized the method for this
                // receiver. Bake an implicit positional receiver into the signature instead of
                // checking it structurally again during call inference.
                let protocol_receiver_is_specialized = self_instance
                    .as_protocol_instance()
                    .is_some_and(|protocol| protocol.class_origin(db).is_some())
                    && signature
                        .overloads
                        .iter()
                        .all(Signature::has_implicit_positional_receiver_annotation);
                if protocol_receiver_is_specialized || signature_receiver != self_instance {
                    let mut binding =
                        CallableBinding::from_overloads(self, signature.overloads.iter().cloned())
                            .with_bound_type(signature_receiver);
                    binding.bake_bound_type_into_overloads(db, env);
                    binding.into()
                } else {
                    // Solve exact receiver constraints before checking the other arguments, but
                    // retain the receiver itself for call inference and receiver diagnostics.
                    let overloads = signature.overloads.iter().flat_map(|overload| {
                        if overload.has_receiver_determined_method_typevar(db, env)
                            && let Some(specialized) = overload.specialize_for_bound_receiver(
                                db,
                                env,
                                self_instance,
                                bound_method.typing_self_type(db),
                            )
                        {
                            specialized.overloads
                        } else {
                            smallvec_inline![overload.clone()]
                        }
                    });

                    CallableBinding::from_overloads(self, overloads)
                        .with_bound_type(self_instance)
                        .into()
                }
            }

            Type::KnownBoundMethod(method) => {
                CallableBinding::from_overloads(self, method.signatures(db, env)).into()
            }

            Type::WrapperDescriptor(wrapper_descriptor) => {
                CallableBinding::from_overloads(self, wrapper_descriptor.signatures(db, env)).into()
            }

            // TODO: We should probably also check the original return type of the function
            // that was decorated with `@dataclass_transform`, to see if it is consistent with
            // with what we configure here.
            Type::DataclassTransformer(_) => Binding::single(
                self,
                Signature::new(
                    Parameters::standard([Parameter::positional_only(Some(Name::new_static(
                        "func",
                    )))
                    .with_annotated_type(Type::object())]),
                    Type::unknown(),
                ),
            )
            .into(),

            Type::FunctionLiteral(function_type) => match function_type.known(db) {
                Some(KnownFunction::AssertType) => {
                    let val_ty = BoundTypeVarInstance::synthetic(
                        db,
                        env,
                        Name::new_static("T"),
                        TypeVarVariance::Invariant,
                    );

                    Binding::single(
                        self,
                        Signature::new_generic(
                            Some(GenericContext::from_typevar_instances(db, env, [val_ty])),
                            Parameters::standard([
                                Parameter::positional_only(Some(Name::new_static("value")))
                                    .with_annotated_type(Type::TypeVar(val_ty)),
                                Parameter::positional_only(Some(Name::new_static("type")))
                                    .with_annotated_type(object_type_form(db)),
                            ]),
                            Type::TypeVar(val_ty),
                        ),
                    )
                    .into()
                }

                Some(KnownFunction::AssertNever) => {
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::standard([Parameter::positional_only(Some(
                                Name::new_static("arg"),
                            ))
                            // We need to set the type to `Any` here (instead of `Never`),
                            // in order for every `assert_never` call to pass the argument
                            // check. If we set it to `Never`, we'll get invalid-argument-type
                            // errors instead of `type-assertion-failure` errors.
                            .with_annotated_type(Type::any())]),
                            Type::Never,
                        ),
                    )
                    .into()
                }

                Some(KnownFunction::Cast) => Binding::single(
                    self,
                    Signature::new(
                        Parameters::standard([
                            Parameter::positional_or_keyword(Name::new_static("typ"))
                                .with_annotated_type(object_type_form(db)),
                            Parameter::positional_or_keyword(Name::new_static("val"))
                                .with_annotated_type(Type::any()),
                        ]),
                        Type::any(),
                    ),
                )
                .into(),

                Some(KnownFunction::Dataclass) => {
                    let python_version = env.python_version(db);
                    let bool_parameter = |name: &'static str, default: bool| {
                        Parameter::keyword_only(Name::new_static(name))
                            .with_annotated_type(KnownClass::Bool.to_instance(db, env))
                            .with_default_type(Type::bool_literal(default))
                    };

                    let mut decorator_factory_parameters = vec![
                        bool_parameter("init", true),
                        bool_parameter("repr", true),
                        bool_parameter("eq", true),
                        bool_parameter("order", false),
                        bool_parameter("unsafe_hash", false),
                        bool_parameter("frozen", false),
                    ];

                    if python_version >= ast::PythonVersion::PY310 {
                        decorator_factory_parameters.extend([
                            bool_parameter("match_args", true),
                            bool_parameter("kw_only", false),
                            bool_parameter("slots", false),
                        ]);
                    }

                    if python_version >= ast::PythonVersion::PY311 {
                        decorator_factory_parameters.push(bool_parameter("weakref_slot", false));
                    }

                    let parameters_with_cls = |cls_ty| {
                        let mut parameters =
                            Vec::with_capacity(decorator_factory_parameters.len() + 1);
                        parameters.push(
                            Parameter::positional_only(Some(Name::new_static("cls")))
                                .with_annotated_type(cls_ty),
                        );
                        parameters.extend_from_slice(&decorator_factory_parameters);
                        parameters
                    };

                    CallableBinding::from_overloads(
                        self,
                        [
                            // def dataclass(cls: None, /, *, ...) -> Callable[[type[_T]], type[_T]]: ...
                            Signature::new(
                                Parameters::standard(parameters_with_cls(Type::none(db, env))),
                                Type::unknown(),
                            ),
                            // def dataclass(cls: type[_T], /, *, ...) -> type[_T]: ...
                            Signature::new(
                                Parameters::standard(parameters_with_cls(
                                    KnownClass::Type.to_instance(db, env),
                                )),
                                Type::unknown(),
                            ),
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
                                Parameters::standard(decorator_factory_parameters),
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
                .known_class_literal_bindings(db, env, class)
                .unwrap_or_else(|| {
                    self.constructor_bindings(
                        db,
                        env,
                        ClassType::NonGeneric(class),
                        recursion_guard,
                    )
                }),

            Type::GenericAlias(alias) => {
                self.constructor_bindings(db, env, ClassType::Generic(alias), recursion_guard)
            }

            Type::SubclassOf(subclass_of_type) => match subclass_of_type.subclass_of() {
                SubclassOfInner::Dynamic(dynamic_type) => {
                    Binding::single(self, Signature::dynamic(Type::Dynamic(dynamic_type))).into()
                }
                SubclassOfInner::Class(class) => {
                    self.constructor_bindings(db, env, class, recursion_guard)
                }
                SubclassOfInner::Protocol(protocol) => protocol.class_origin(db).map_or_else(
                    || Binding::single(self, Signature::dynamic(Type::unknown())).into(),
                    |origin| {
                        let bindings = self.constructor_bindings(db, env, *origin, recursion_guard);
                        if protocol.materialization_kind(db).is_some() {
                            bindings.with_constructed_instance_type(
                                db,
                                Type::ProtocolInstance(protocol),
                            )
                        } else {
                            bindings
                        }
                    },
                ),
                SubclassOfInner::TypeVar(tvar) => {
                    let constructor_instance_type = Type::TypeVar(tvar);
                    let bindings = match tvar.typevar(db).require_bound_or_constraints(db, env) {
                        TypeVarBoundOrConstraints::UpperBound(bound) => {
                            let constructor = bound.constructor_for_typevar_bound(db, env);
                            if let Type::ClassLiteral(class) = constructor
                                && let Some(bindings) =
                                    self.known_class_literal_bindings(db, env, class)
                            {
                                bindings
                            } else {
                                constructor.bindings_impl(db, env, recursion_guard)
                            }
                        }
                        TypeVarBoundOrConstraints::Constraints(constraints) => {
                            Bindings::from_union(
                                self,
                                constraints.elements(db).iter().map(|ty| {
                                    ty.to_meta_type(db, env)
                                        .bindings_impl(db, env, recursion_guard)
                                }),
                            )
                        }
                    };
                    // Some built-in constructors, including `object`, are special-cased as regular
                    // callable bindings. Wrap them so that every bound or constrained call has
                    // constructor context and constructs `T`; existing constructor bindings keep
                    // their original kind.
                    bindings
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
                let signature = Signature::new(Parameters::standard([parameter]), Type::any());
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
                        env,
                        "__call__",
                        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    )
                    .place
                {
                    Place::Defined(DefinedPlace {
                        ty: dunder_callable,
                        definedness: boundness,
                        ..
                    }) => {
                        let mut bindings = dunder_callable.bindings_impl(db, env, recursion_guard);
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
                    .map(|element| element.bindings_impl(db, env, recursion_guard)),
            ),

            // A narrowed `type[T: Base] & type[Child]` still needs to construct `T & Child`,
            // but its constructor must come from `Child`, not from `Base` as an independent,
            // competing alternative. Flattening the projected instance lets intersection
            // simplification select that constructor without discarding unrelated providers.
            Type::Intersection(intersection)
                if intersection.positive(db).iter().all(|element| {
                    // A metaclass instance also has an instance-space projection, but it can
                    // provide an independent `__call__`. Only simplify actual class-object
                    // variants so `type[Base] & Meta` retains both callable candidates.
                    matches!(
                        element.resolve_type_alias(db),
                        Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)
                    )
                }) && let Some(instance_type) = self.to_instance_approximation(db, env)
                    && let Type::NominalInstance(lookup_instance) =
                        instance_type.flatten_typevars(db, env)
                    && let Some(bindings) = {
                        let bindings = lookup_instance.to_meta_type(db, env).bindings_impl(
                            db,
                            env,
                            recursion_guard,
                        );
                        bindings.has_only_constructor_items().then_some(bindings)
                    } =>
            {
                bindings
                    .with_constructed_instance_type(db, instance_type)
                    .with_callable_type(self)
            }

            Type::Intersection(intersection) => Bindings::from_intersection(
                self,
                intersection
                    .positive_elements_or_object(db)
                    .map(|element| element.bindings_impl(db, env, recursion_guard)),
            ),

            Type::EnumComplement(complement) => {
                complement
                    .to_intersection(db, env)
                    .bindings_impl(db, env, recursion_guard)
            }

            Type::DataclassDecorator(_) => {
                let typevar = BoundTypeVarInstance::synthetic(
                    db,
                    env,
                    Name::new_static("T"),
                    TypeVarVariance::Invariant,
                );
                let typevar_meta = SubclassOfType::from(db, env, typevar);
                let context = GenericContext::from_typevar_instances(db, env, [typevar]);
                let parameters = [Parameter::positional_only(Some(Name::new_static("cls")))
                    .with_annotated_type(typevar_meta)];
                // Intersect with `Any` for the return type to reflect the fact that the `dataclass()`
                // decorator adds methods to the class
                let returns =
                    IntersectionType::from_two_elements(db, env, typevar_meta, Type::any());
                let signature = Signature::new_generic(
                    Some(context),
                    Parameters::standard(parameters),
                    returns,
                );
                Binding::single(self, signature).into()
            }

            // TODO: some `SpecialForm`s are callable (e.g. TypedDicts)
            Type::SpecialForm(_) => CallableBinding::not_callable(self).into(),

            Type::LiteralValue(literal) => match literal.kind() {
                LiteralValueTypeKind::Enum(enum_literal) => enum_literal
                    .enum_class_instance(db, env)
                    .bindings_impl(db, env, recursion_guard),
                _ => CallableBinding::not_callable(self).into(),
            },

            Type::KnownInstance(KnownInstanceType::NewType(newtype)) => Binding::single(
                self,
                Signature::new(
                    Parameters::standard([Parameter::positional_only(None)
                        .with_annotated_type(newtype.base(db).instance_type(db, env))]),
                    Type::NewTypeInstance(newtype),
                ),
            )
            .into(),

            Type::KnownInstance(
                KnownInstanceType::FunctoolsPartial(partial)
                | KnownInstanceType::FunctoolsPartialCall(partial),
            ) => Type::Callable(partial.partial(db)).bindings_impl(db, env, recursion_guard),

            Type::KnownInstance(known_instance) => known_instance
                .instance_fallback(db, env)
                .bindings_impl(db, env, recursion_guard),

            Type::TypeAlias(alias) => alias.value_type(db).bindings_impl(db, env, recursion_guard),

            Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::BoundSuper(_)
            | Type::ModuleLiteral(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_) => CallableBinding::not_callable(self).into(),
        }
    }

    fn known_class_literal_bindings(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
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
                            Parameters::standard([Parameter::positional_only(Some(
                                Name::new_static("o"),
                            ))
                            .with_annotated_type(Type::any())
                            .with_default_type(Type::bool_literal(false))]),
                            KnownClass::Bool.to_instance(db, env),
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
                                Parameters::standard([
                                    Parameter::positional_only(Some(Name::new_static("t")))
                                        .with_annotated_type(Type::any()),
                                    Parameter::positional_only(Some(Name::new_static("obj")))
                                        .with_annotated_type(Type::any()),
                                ]),
                                KnownClass::Super.to_instance(db, env),
                            ),
                            Signature::new(
                                Parameters::standard([Parameter::positional_only(Some(
                                    Name::new_static("t"),
                                ))
                                .with_annotated_type(Type::any())]),
                                KnownClass::Super.to_instance(db, env),
                            ),
                            Signature::new(
                                Parameters::empty(),
                                KnownClass::Super.to_instance(db, env),
                            ),
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
                let warning_class_type = KnownClass::Warning.to_subclass_of(db, env);

                Some(
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::standard([
                                Parameter::positional_only(Some(Name::new_static("message")))
                                    .with_annotated_type(Type::literal_string()),
                                Parameter::keyword_only(Name::new_static("category"))
                                    .with_annotated_type(UnionType::from_two_elements(
                                        db,
                                        env,
                                        warning_class_type,
                                        Type::none(db, env),
                                    ))
                                    .with_default_type(warning_class_type),
                                Parameter::keyword_only(Name::new_static("stacklevel"))
                                    .with_annotated_type(KnownClass::Int.to_instance(db, env))
                                    .with_default_type(Type::int_literal(1)),
                            ]),
                            KnownClass::Deprecated.to_instance(db, env),
                        ),
                    )
                    .into(),
                )
            }

            KnownClass::TypeAliasType | KnownClass::ExtensionsTypeAliasType => {
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
                            Parameters::standard([
                                Parameter::positional_or_keyword(Name::new_static("name"))
                                    .with_annotated_type(KnownClass::Str.to_instance(db, env)),
                                Parameter::positional_or_keyword(Name::new_static("value"))
                                    .with_annotated_type(object_type_form(db)),
                                Parameter::keyword_only(Name::new_static("type_params"))
                                    .with_annotated_type(Type::homogeneous_tuple(
                                        db,
                                        env,
                                        UnionType::from_elements(
                                            db,
                                            env,
                                            [
                                                KnownClass::TypeVar.to_instance(db, env),
                                                KnownClass::ParamSpec.to_instance(db, env),
                                                KnownClass::TypeVarTuple.to_instance(db, env),
                                            ],
                                        ),
                                    ))
                                    .with_default_type(Type::empty_tuple(db, env)),
                            ]),
                            Type::unknown(),
                        ),
                    )
                    .into(),
                )
            }

            KnownClass::Property => {
                let getter_signature = Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(None).with_annotated_type(Type::any())
                    ]),
                    Type::any(),
                );
                let setter_signature = Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(None).with_annotated_type(Type::any()),
                        Parameter::positional_only(None).with_annotated_type(Type::any()),
                    ]),
                    Type::none(db, env),
                );
                let deleter_signature = Signature::new(
                    Parameters::standard([
                        Parameter::positional_only(None).with_annotated_type(Type::any())
                    ]),
                    Type::any(),
                );

                Some(
                    Binding::single(
                        self,
                        Signature::new(
                            Parameters::standard([
                                Parameter::positional_or_keyword(Name::new_static("fget"))
                                    .with_annotated_type(UnionType::from_two_elements(
                                        db,
                                        env,
                                        Type::single_callable(db, getter_signature),
                                        Type::none(db, env),
                                    ))
                                    .with_default_type(Type::none(db, env)),
                                Parameter::positional_or_keyword(Name::new_static("fset"))
                                    .with_annotated_type(UnionType::from_two_elements(
                                        db,
                                        env,
                                        Type::single_callable(db, setter_signature),
                                        Type::none(db, env),
                                    ))
                                    .with_default_type(Type::none(db, env)),
                                Parameter::positional_or_keyword(Name::new_static("fdel"))
                                    .with_annotated_type(UnionType::from_two_elements(
                                        db,
                                        env,
                                        Type::single_callable(db, deleter_signature),
                                        Type::none(db, env),
                                    ))
                                    .with_default_type(Type::none(db, env)),
                                Parameter::positional_or_keyword(Name::new_static("doc"))
                                    .with_annotated_type(UnionType::from_two_elements(
                                        db,
                                        env,
                                        KnownClass::Str.to_instance(db, env),
                                        Type::none(db, env),
                                    ))
                                    .with_default_type(Type::none(db, env)),
                            ]),
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
                    env,
                    Name::new_static("_T"),
                    TypeVarVariance::Covariant,
                );

                Some(
                    Binding::single(
                        self,
                        Signature::new_generic(
                            Some(GenericContext::from_typevar_instances(db, env, [return_ty])),
                            Parameters::concatenate(
                                db,
                                vec![
                                    Parameter::positional_only(Some(Name::new_static("func")))
                                        .with_annotated_type(Type::single_callable(
                                            db,
                                            Signature::new(
                                                Parameters::gradual_form(),
                                                Type::TypeVar(return_ty),
                                            ),
                                        )),
                                ],
                                ConcatenateTail::Gradual,
                            ),
                            KnownClass::FunctoolsPartial.to_specialized_instance(
                                db,
                                env,
                                &[Type::TypeVar(return_ty)],
                            ),
                        ),
                    )
                    .into(),
                )
            }

            KnownClass::Tuple => {
                let element_ty = BoundTypeVarInstance::synthetic(
                    db,
                    env,
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
                            Signature::new(Parameters::empty(), Type::empty_tuple(db, env)),
                            Signature::new_generic(
                                Some(GenericContext::from_typevar_instances(
                                    db,
                                    env,
                                    [element_ty],
                                )),
                                Parameters::standard([Parameter::positional_only(Some(
                                    Name::new_static("iterable"),
                                ))
                                .with_annotated_type(
                                    KnownClass::Iterable.to_specialized_instance(
                                        db,
                                        env,
                                        &[Type::TypeVar(element_ty)],
                                    ),
                                )]),
                                Type::homogeneous_tuple(db, env, Type::TypeVar(element_ty)),
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
    fn constructor_bindings(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        recursion_guard: &ActiveRecursionDetector<Type<'db>>,
    ) -> Bindings<'db> {
        fn resolve_dunder_new_callable<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
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
            match place.try_call_dunder_get(db, env, owner) {
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
            env: &ProgramEnvironment<'db>,
            bindings: Bindings<'db>,
            self_type: Type<'db>,
        ) -> Bindings<'db> {
            bindings.map(|binding| {
                let mut binding = binding;
                // If descriptor binding produced a bound callable, bake that into the signature
                // first, then bind `cls` for constructor-call semantics (the call site omits `cls`).
                // Note: This intentionally preserves `type.__call__` behavior for `@classmethod __new__`,
                // which receives an extra implicit `cls` and errors at call sites.
                binding.bake_bound_type_into_overloads(db, env);
                binding.bound_type = Some(self_type);
                binding
            })
        }

        let class_literal = class.class_literal(db);
        let class_generic_context = class_literal.generic_context(db);

        // Keep bespoke constructor behavior for cases that don't map cleanly to `__new__`/`__init__`.
        let fallback_bindings = || {
            let return_type = self
                .to_instance_approximation(db, env)
                .unwrap_or(Type::unknown());
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

        // Specialized and non-generic TypedDict constructors use their dedicated validation.
        // An unspecialized generic constructor also needs its real `__init__` signature so
        // ordinary call inference can solve the class type variables.
        if (class_literal.is_typed_dict(db)
            || class::CodeGeneratorKind::TypedDict.matches(db, class_literal))
            && (!matches!(self, Type::ClassLiteral(_)) || class_generic_context.is_none())
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
                    | KnownClass::ExtensionsTypeAliasType
                    | KnownClass::Deprecated
            )
        ) {
            return fallback_bindings();
        }

        // Temporary special-casing for all subclasses of `enum.Enum` until we support the
        // functional syntax for creating enum classes. TODO we should ideally check e.g.
        // `MyEnum(1)` to make sure `1` is a valid value for `MyEnum`.
        if KnownClass::Enum
            .to_class_literal(db, env)
            .to_class_type(db)
            .is_some_and(|enum_class| class.is_subclass_of(db, env, enum_class))
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

        let on_cycle = || {
            // Leave the return type unknown so the enclosing constructor supplies its own
            // instance type, rather than the class where the cycle happened to be detected.
            Binding::single(self_type, Signature::dynamic(Type::unknown())).into()
        };
        // Key recursion by the full receiver type. Descriptor overloads can distinguish `C` from
        // `type[C]`, and different specializations need separate expansion even if one contains
        // the other, because a constructor may ignore its nested type arguments.
        recursion_guard.visit(&self_type, on_cycle, || {
            // Check for a custom `__call__` on the metaclass (excluding `type.__call__`).
            // We preserve its full overload set here and defer constructor branching decisions
            // until call-time overload resolution.
            let metaclass_dunder_call = self_type.member_lookup_with_policy(
                db,
                env,
                "__call__",
                MemberLookupPolicy::NO_INSTANCE_FALLBACK
                    | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
            );

            let Some(constructor_instance_ty) = self_type.to_instance_approximation(db, env) else {
                return fallback_bindings();
            };

            // TypedDict classes inherit `dict.__new__`, whose gradual `**kwargs` signature cannot
            // constrain their type variables. Their synthesized `__init__` contains the actual field
            // types, including generic extra items, so constructor inference should start there.
            let new_method = if class_literal.is_typed_dict(db) {
                None
            } else {
                self_type.lookup_dunder_new(db, env)
            };

            let init_method_no_object = constructor_instance_ty.member_lookup_with_policy(
                db,
                env,
                "__init__",
                MemberLookupPolicy::NO_INSTANCE_FALLBACK
                    | MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
            );

            let (new_bindings, has_any_new) = match new_method.as_ref().map(|method| method.place) {
                Some(place) => match resolve_dunder_new_callable(db, env, self_type, place) {
                    Some((new_callable, definedness)) => {
                        let bindings = new_callable.bindings_impl(db, env, recursion_guard);
                        let mut bindings = bind_constructor_new(db, env, bindings, self_type)
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
                        .bindings_impl(db, env, recursion_guard)
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
                    let init_method_with_object = constructor_instance_ty
                        .member_lookup_with_policy(
                            db,
                            env,
                            "__init__",
                            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                        );
                    match init_method_with_object.place {
                        Place::Defined(DefinedPlace {
                            ty: init_method,
                            definedness,
                            ..
                        }) => {
                            let mut bindings = init_method
                                .bindings_impl(db, env, recursion_guard)
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
                    .bindings_impl(db, env, recursion_guard)
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
        let constraints = ConstraintSetBuilder::new();
        self.bindings(db, env)
            .match_parameters(db, env, argument_types)
            .check_types(
                db,
                env,
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
        if let Type::Intersection(intersection) = self {
            return intersection.try_call_dunder_with_policy(
                db,
                env,
                name,
                argument_types,
                tcx,
                policy,
            );
        }

        if let Type::Union(union) = self {
            return union.try_call_dunder_with_policy(db, env, name, argument_types, tcx, policy);
        }

        // Implicit calls to dunder methods never access instance members, so we pass
        // `NO_INSTANCE_FALLBACK` here in addition to other policies:
        let policy = policy | MemberLookupPolicy::NO_INSTANCE_FALLBACK;
        match self.member_lookup_with_policy(db, env, name, policy).place {
            Place::Defined(DefinedPlace {
                ty: dunder_callable,
                definedness: boundness,
                provenance,
                ..
            }) => {
                let constraints = ConstraintSetBuilder::new();
                let bindings = dunder_callable
                    .bindings(db, env)
                    .match_parameters(db, env, argument_types)
                    .check_types(db, env, &constraints, argument_types, tcx, &[]);

                let bindings = match bindings {
                    Ok(bindings) => bindings,
                    Err(CallError(kind, bindings)) => {
                        return Err(CallDunderError::CallError(kind, bindings, provenance));
                    }
                };

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
        env: &ProgramEnvironment<'db>,
        name: &str,
        argument_types: &CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        match self.member(db, env, name).place {
            Place::Defined(DefinedPlace {
                ty: dunder_callable,
                definedness: boundness,
                provenance,
                ..
            }) => {
                let constraints = ConstraintSetBuilder::new();
                let bindings = dunder_callable
                    .bindings(db, env)
                    .match_parameters(db, env, argument_types)
                    .check_types(db, env, &constraints, argument_types, tcx, &[]);

                let bindings = match bindings {
                    Ok(bindings) => bindings,
                    Err(CallError(kind, bindings)) => {
                        return Err(CallDunderError::CallError(kind, bindings, provenance));
                    }
                };

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
        let Some(class) = self.nominal_class(db, env).or_else(|| {
            self.to_meta_type(db, env)
                .to_instance_approximation(db, env)
                .and_then(|instance| instance.nominal_class(db, env))
        }) else {
            return true;
        };

        let class = class.class_literal(db);
        if class.as_static().is_none() {
            return true;
        }

        let flags = class.instance_flags(db);
        if flags.contains(ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE) {
            return true;
        }

        if !flags.contains(ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE) {
            return false;
        }

        !matches!(
            result,
            Ok(PlaceAndQualifiers {
                place: Place::Defined(place),
                ..
            }) if place.is_definitely_defined()
        )
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
        let custom_getattr_result = || {
            if policy.no_getattr_lookup() {
                return MemberLookupResult::from(Place::Undefined);
            }

            let name_type = Type::string_literal(db, name);
            match self.try_call_dunder(
                db,
                env,
                "__getattr__",
                CallArguments::positional([name_type]),
                TypeContext::default(),
            ) {
                Ok(outcome) => Place::bound(outcome.return_type(db, env)).into(),
                Err(CallDunderError::CallError(_, bindings, _)) => member_lookup_result(
                    db,
                    Place::bound(bindings.return_type(db, env)).into(),
                    Some(MemberLookupErrorKind::GetAttr {
                        receiver: self,
                        name: name_type,
                    }),
                ),
                Err(
                    CallDunderError::PossiblyUnbound { .. } | CallDunderError::MethodNotAvailable,
                ) => Place::Undefined.into(),
            }
        };

        let getattribute_policy = MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
            | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK;
        if !self.custom_getattribute_may_affect_lookup(db, env, result)
            || self
                .class_member_with_policy(db, env, "__getattribute__", getattribute_policy)
                .place
                .is_undefined()
        {
            return member_lookup_or_fall_back_to(db, env, result, custom_getattr_result);
        }

        let name_type = Type::string_literal(db, name);
        let custom_getattribute = match self.try_call_dunder_with_policy(
            db,
            env,
            "__getattribute__",
            &mut CallArguments::positional([name_type]),
            TypeContext::default(),
            getattribute_policy,
        ) {
            Ok(bindings) => Place::bound(bindings.return_type(db, env)).into(),
            Err(CallDunderError::CallError(_, bindings, _)) => member_lookup_result(
                db,
                Place::bound(bindings.return_type(db, env)).into(),
                Some(MemberLookupErrorKind::GetAttribute {
                    receiver: self,
                    name: name_type,
                }),
            ),
            Err(CallDunderError::PossiblyUnbound { .. }) => Place::Undefined.into(),
            Err(CallDunderError::MethodNotAvailable) => {
                return member_lookup_or_fall_back_to(db, env, result, custom_getattr_result);
            }
        };

        if let Err(error) = custom_getattribute {
            let member = result.unwrap_or_else(|error| error.fallback_member(db));
            return Err(MemberLookupError::new(
                db,
                member.or_fall_back_to(db, env, || error.fallback_member(db)),
                error.kind(db),
            ));
        }

        // A custom override runs before the descriptor and might return without invoking it.
        let result = if matches!(
            result.err().map(|error| error.kind(db)),
            Some(MemberLookupErrorKind::DescriptorGet(_))
        ) {
            Ok(result.unwrap_or_else(|error| error.fallback_member(db)))
        } else {
            result
        };

        let result = member_lookup_or_fall_back_to(db, env, result, || custom_getattribute);
        member_lookup_or_fall_back_to(db, env, result, custom_getattr_result)
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
            Type::TypeVar(tvar) => {
                match tvar.typevar(db).bound_or_constraints(db, env) {
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        bound.flatten_typevars(db, env)
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        constraints.as_type(db, env).flatten_typevars(db, env)
                    }
                    // Unbounded typevar is effectively `object`.
                    None => Type::object(),
                }
            }
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

    fn in_type_expression_impl(
        &self,
        db: &'db dyn Db,
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
        inference_flags: InferenceFlags,
    ) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        let env = &ProgramEnvironment::from_scope(scope_id);
        match self {
            // Special cases for `float` and `complex`
            // https://typing.python.org/en/latest/spec/special-types.html#special-cases-for-float-and-complex
            Type::ClassLiteral(class) => {
                let ty = match class.known(db) {
                    Some(KnownClass::Complex) => KnownUnion::Complex.to_type(db, env),
                    Some(KnownClass::Float)
                        if !inference_flags
                            .contains(InferenceFlags::DISABLE_INT_FLOAT_SPECIAL_CASE) =>
                    {
                        KnownUnion::Float.to_type(db, env)
                    }
                    _ => Type::instance(db, env, class.default_specialization(db)),
                };
                Ok(ty)
            }
            Type::GenericAlias(alias) => Ok(Type::instance(db, env, ClassType::from(*alias))),

            Type::SubclassOf(_)
            | Type::EnumComplement(_)
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
            | Type::SlotDescriptor(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
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
                    if !inference_flags.contains(InferenceFlags::IN_UNPACK_TYPE_ARGUMENT)
                        && typevar.is_typevartuple(db)
                    {
                        return Err(InvalidTypeExpressionError {
                            invalid_expressions: smallvec_inline![
                                InvalidTypeExpression::InvalidBareTypeVarTuple(*typevar)
                            ],
                            fallback_type: Type::unknown(),
                        });
                    }
                    let index = semantic_index(db, scope_id.program_file(db));
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
                KnownInstanceType::ConstraintSetSolution(__call__) => {
                    Err(InvalidTypeExpressionError {
                        invalid_expressions: smallvec_inline![
                            InvalidTypeExpression::ConstraintSetSolution
                        ],
                        fallback_type: Type::unknown(),
                    })
                }
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

                    Ok(instance.inner(db).to_meta_type(db, env))
                }
                KnownInstanceType::Callable(callable) => Ok(Type::Callable(*callable)),
                KnownInstanceType::LiteralStringAlias(ty) => Ok(ty.inner(db)),
                KnownInstanceType::Sentinel(sentinel) => {
                    Ok(Type::KnownInstance(KnownInstanceType::Sentinel(*sentinel)))
                }
                KnownInstanceType::FunctoolsPartial(_)
                | KnownInstanceType::FunctoolsPartialCall(_)
                | KnownInstanceType::Range { .. } => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::InvalidType(
                        *self, scope_id
                    )],
                    fallback_type: Type::unknown(),
                }),
            },

            Type::SpecialForm(special_form) => special_form
                .in_type_expression(db, scope_id, typevar_binding_context, inference_flags)
                .map_err(|err| {
                    let fallback_type = match err {
                        InvalidTypeExpression::Concatenate
                        | InvalidTypeExpression::RequiresTwoArguments(
                            SpecialFormType::Concatenate,
                        ) => Type::Dynamic(DynamicType::InvalidConcatenateUnknown),
                        InvalidTypeExpression::TypingSelfWithIncompatibleReceiver(typing_self) => {
                            Type::TypeVar(typing_self)
                        }
                        _ => Type::unknown(),
                    };

                    InvalidTypeExpressionError {
                        fallback_type,
                        invalid_expressions: smallvec_inline![err],
                    }
                }),

            Type::Union(union) => {
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

            Type::Dynamic(_) | Type::Divergent(_) => Ok(*self),

            Type::NominalInstance(instance) => match instance.known_class(db) {
                Some(KnownClass::NoneType) => Ok(Type::none(db, env)),
                // TODO: Emit an invalid-type-form diagnostic and recover to `Unknown` for
                // unrecognized `TypeVar` and `TypeVarTuple` instances.
                Some(KnownClass::TypeVar) => Ok(todo_type!(
                    "unrecognized `typing.TypeVar` instances should be invalid type expressions"
                )),
                Some(KnownClass::TypeVarTuple | KnownClass::ExtensionsTypeVarTuple) => {
                    Ok(todo_type!(
                        "unrecognized `typing.TypeVarTuple` instances \
                        should be invalid type expressions"
                    ))
                }
                _ => Err(InvalidTypeExpressionError {
                    invalid_expressions: smallvec_inline![InvalidTypeExpression::InvalidType(
                        *self, scope_id
                    )],
                    fallback_type: Type::unknown(),
                }),
            },

            Type::Intersection(_) => Ok(todo_type!("Type::Intersection.in_type_expression")),

            Type::TypeAlias(alias) => alias.value_type(db).in_type_expression_impl(
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
        #[derive(Default)]
        struct MetaTypeVisitor<'db> {
            active_aliases: ActiveRecursionDetector<TypeAliasType<'db>>,
            active_identities: ActiveRecursionDetector<TypeIdentity<'db>>,
        }

        fn to_meta_type_inner<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
            visitor: &MetaTypeVisitor<'db>,
        ) -> Type<'db> {
            match ty {
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
                Type::Union(union) => {
                    union.map(db, env, |ty| to_meta_type_inner(db, env, *ty, visitor))
                }
                Type::TypeIs(_) | Type::TypeGuard(_) => KnownClass::Bool.to_class_literal(db, env),
                Type::TypeForm(_) => to_meta_type_inner(db, env, Type::object(), visitor),
                Type::LiteralValue(literal) => match literal.kind() {
                    LiteralValueTypeKind::Bool(_) => KnownClass::Bool.to_class_literal(db, env),
                    LiteralValueTypeKind::Bytes(_) => KnownClass::Bytes.to_class_literal(db, env),
                    LiteralValueTypeKind::Int(_) => KnownClass::Int.to_class_literal(db, env),
                    LiteralValueTypeKind::Enum(enum_literal) => {
                        Type::ClassLiteral(enum_literal.enum_class(db))
                    }
                    LiteralValueTypeKind::String(_) | LiteralValueTypeKind::LiteralString => {
                        KnownClass::Str.to_class_literal(db, env)
                    }
                },
                Type::FunctionLiteral(_) => KnownClass::FunctionType.to_class_literal(db, env),
                Type::BoundMethod(_) => KnownClass::MethodType.to_class_literal(db, env),
                Type::KnownBoundMethod(method) => method.class().to_class_literal(db, env),
                Type::WrapperDescriptor(_) => {
                    KnownClass::WrapperDescriptorType.to_class_literal(db, env)
                }
                Type::DataclassDecorator(_) => KnownClass::FunctionType.to_class_literal(db, env),
                Type::Callable(callable) if callable.is_function_like(db) => {
                    KnownClass::FunctionType.to_class_literal(db, env)
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
                Type::SubclassOf(subclass_of_ty) => subclass_of_ty.to_meta_type(db, env),
                Type::Dynamic(dynamic) => {
                    SubclassOfType::from(db, env, SubclassOfInner::Dynamic(dynamic))
                }
                Type::Divergent(_) => ty,
                Type::Intersection(intersection) => {
                    if let Some(alternatives) = intersection.finite_alternative_union(db, env) {
                        to_meta_type_inner(db, env, alternatives, visitor)
                    } else {
                        // Negative constraints do not generally constrain classes: `int & ~Literal[0]`
                        // still has meta-type `type[int]`. Pure negations are bounded by `object`.
                        let mut builder = IntersectionBuilder::new(db, env);
                        for positive in intersection.positive_elements_or_object(db) {
                            builder.add_positive_in_place(to_meta_type_inner(
                                db, env, positive, visitor,
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
                    visitor.active_aliases.visit(
                        &alias,
                        || Type::Never,
                        || {
                            visitor.active_identities.visit(
                                &Type::TypeAlias(alias).to_type_identity(db),
                                || KnownClass::Type.to_instance(db, env),
                                || to_meta_type_inner(db, env, alias.value_type(db), visitor),
                            )
                        },
                    )
                }
                Type::NewTypeInstance(newtype) => {
                    to_meta_type_inner(db, env, newtype.concrete_base_type(db), visitor)
                }
            }
        }

        to_meta_type_inner(db, env, self, &MetaTypeVisitor::default())
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
        if matches!(
            self,
            Type::Dynamic(_)
                | Type::Divergent(_)
                | Type::Never
                | Type::WrapperDescriptor(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::ModuleLiteral(_)
                | Type::ClassLiteral(_)
                | Type::SpecialForm(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::LiteralValue(_)
                | Type::BoundSuper(_)
                | Type::KnownInstance(
                    KnownInstanceType::SubscriptedProtocol(_)
                        | KnownInstanceType::SubscriptedGeneric(_)
                        | KnownInstanceType::TypeAliasType(_)
                        | KnownInstanceType::Deprecated(_)
                        | KnownInstanceType::Field(_)
                        | KnownInstanceType::ConstraintSet(_)
                        | KnownInstanceType::ConstraintSetSolution(_)
                        | KnownInstanceType::GenericContext(_)
                        | KnownInstanceType::Specialization(_)
                        | KnownInstanceType::Literal(_)
                        | KnownInstanceType::NewType(_)
                        | KnownInstanceType::Sentinel(_)
                        | KnownInstanceType::NamedTupleSpec(_),
                )
                | Type::KnownBoundMethod(
                    KnownBoundMethodType::StrStartswith(_)
                        | KnownBoundMethodType::ConstraintSetLowerBound
                        | KnownBoundMethodType::ConstraintSetUpperBound
                        | KnownBoundMethodType::ConstraintSetEquality
                        | KnownBoundMethodType::ConstraintSetRange
                        | KnownBoundMethodType::ConstraintSetAlways
                        | KnownBoundMethodType::ConstraintSetNever
                        | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                        | KnownBoundMethodType::ConstraintSetSatisfies(_)
                        | KnownBoundMethodType::ConstraintSetExists(_)
                        | KnownBoundMethodType::ConstraintSetForAll(_)
                        | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
                        | KnownBoundMethodType::ConstraintSetSolutions(_)
                        | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_)
                )
        ) {
            return self;
        }

        self.apply_specialization_inner(db, specialization, specialize_self_domain)
    }

    #[salsa::tracked(
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
    fn apply_specialization_inner(
        self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
    ) -> Type<'db> {
        let env = &ProgramEnvironment::from_program(specialization.generic_context(db).program(db));
        let apply_specialization = ApplySpecialization::Specialization {
            specialization,
            specialize_self_domain,
        };
        let type_mapping = match specialization.materialization_kind(db) {
            None => TypeMapping::ApplySpecialization(apply_specialization),
            Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
                specialization: apply_specialization,
                materialization_kind,
            },
        };

        self.apply_type_mapping(db, env, &type_mapping, TypeContext::default())
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

        if let Type::ClassLiteral(class) = self
            && matches!(
                type_mapping,
                TypeMapping::Promote(PromotionMode::On, PromotionKind::ClassLiteralsOnly)
            )
        {
            return SubclassOfType::from(db, visitor.env, class.default_specialization(db));
        }

        match self {
            Type::TypeVar(bound_typevar) => {
                bound_typevar.apply_type_mapping_impl(db, type_mapping, visitor)
            }
            Type::KnownInstance(known_instance) => {
                known_instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            }

            Type::FunctionLiteral(function) => visitor.visit(db, self, type_mapping, || {
                match type_mapping {
                    // Promote the types within the signature before promoting the signature to its
                    // callable form.
                    TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular) => {
                        Type::FunctionLiteral(function.apply_type_mapping_impl(
                            db,
                            type_mapping,
                            tcx,
                            visitor,
                        ))
                        .promote_impl(db, visitor.env)
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
                method
                    .function(db)
                    .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                method
                    .self_instance(db)
                    .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                method.signature_receiver(db).apply_type_mapping_impl(
                    db,
                    type_mapping,
                    tcx,
                    visitor,
                ),
            )),

            Type::NominalInstance(instance)
                if matches!(
                    type_mapping,
                    TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular)
                ) =>
            {
                match instance.known_class(db) {
                    Some(KnownClass::Complex) => KnownUnion::Complex.to_type(db, visitor.env),
                    Some(KnownClass::Float) => KnownUnion::Float.to_type(db, visitor.env),
                    _ => instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                }
            }

            Type::NominalInstance(instance)
                if matches!(
                    type_mapping,
                    TypeMapping::Promote(PromotionMode::On, PromotionKind::SingletonsOnly)
                ) =>
            {
                if instance.is_singleton(db) {
                    self.promote_singletons_impl(db, visitor.env)
                } else {
                    instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                }
            }

            Type::NominalInstance(instance) => {
                instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            }

            Type::NewTypeInstance(newtype) => visitor.visit(db, self, type_mapping, || {
                Type::NewTypeInstance(newtype.map_base_class_type(db, |class_type| {
                    class_type.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                }))
            }),

            Type::ProtocolInstance(instance) => Type::ProtocolInstance(
                instance.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            ),

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

            Type::SubclassOf(subclass_of) => {
                subclass_of.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            }

            Type::PropertyInstance(property) => Type::PropertyInstance(
                property.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            ),

            Type::SlotDescriptor(descriptor) => Type::SlotDescriptor(SlotDescriptorType::new(
                db,
                descriptor
                    .value_type(db)
                    .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            )),

            Type::Union(union) => union.map_leave_aliases(db, visitor.env, |element| {
                element.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            }),
            Type::Intersection(intersection) => {
                let mut builder = IntersectionBuilder::new(db, visitor.env);
                for positive in intersection.positive(db) {
                    builder.add_positive_in_place(positive.apply_type_mapping_impl(
                        db,
                        type_mapping,
                        tcx,
                        visitor,
                    ));
                }
                // Regular promotion should remove negative contributions from intersections,
                // so we don't preserve them here when regular promotion is enabled.
                if !matches!(
                    type_mapping,
                    TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular)
                ) {
                    for negative in intersection.negative(db) {
                        builder.add_negative_in_place(negative.apply_type_mapping_impl(
                            db,
                            &type_mapping.flip(),
                            tcx,
                            visitor,
                        ));
                    }
                }
                builder.build()
            }

            Type::EnumComplement(complement) => complement
                .to_intersection(db, visitor.env)
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),

            Type::TypeIs(type_is) => visitor.visit(db, self, type_mapping, || {
                type_is.with_type(
                    db,
                    type_is.type_argument(db).apply_type_mapping_impl(
                        db,
                        type_mapping,
                        tcx,
                        visitor,
                    ),
                )
            }),

            Type::TypeGuard(type_guard) => visitor.visit(db, self, type_mapping, || {
                type_guard.with_type(
                    db,
                    type_guard.return_type(db).apply_type_mapping_impl(
                        db,
                        type_mapping,
                        tcx,
                        visitor,
                    ),
                )
            }),

            Type::TypeForm(typeform) => visitor.visit(db, self, type_mapping, || {
                TypeFormType::from_type_expression(
                    db,
                    typeform.type_argument(db).apply_type_mapping_impl(
                        db,
                        type_mapping,
                        tcx,
                        visitor,
                    ),
                )
            }),

            Type::TypeAlias(alias) => {
                match type_mapping {
                    TypeMapping::Materialize(_) if alias.materialization_kind(db).is_some() => self,
                    TypeMapping::EagerExpansion if alias.materialization_kind(db).is_some() => {
                        alias.value_type(db).expand_eagerly(db, visitor.env)
                    }
                    // For EagerExpansion, expand the raw value type. This path relies on Salsa's cycle
                    // detection rather than the visitor's cycle detection, because the visitor tracks
                    // Type values and `RecursiveList` is different from `RecursiveList[T]`.
                    TypeMapping::EagerExpansion => {
                        alias.raw_value_type(db).expand_eagerly(db, visitor.env)
                    }
                    // When specializing a generic type alias, instead of specializing the expanded type, the type alias itself is specialized.
                    // Without this special handling, recursive type aliases would result in cycles, returning an unspecialized fallback type.
                    TypeMapping::ApplySpecialization(specialization)
                    | TypeMapping::ApplySpecializationWithMaterialization {
                        specialization, ..
                    } if matches!(
                        specialization,
                        ApplySpecialization::Specialization { .. }
                            | ApplySpecialization::TypeAlias(_)
                            | ApplySpecialization::Partial { .. }
                    ) =>
                    {
                        let mut current_specialization =
                            specialization.as_specialization(db).unwrap();
                        if let TypeMapping::ApplySpecializationWithMaterialization {
                            materialization_kind,
                            ..
                        } = type_mapping
                        {
                            current_specialization = current_specialization
                                .with_materialization_kind(db, Some(*materialization_kind));
                        }
                        Type::TypeAlias(alias.apply_specialization(db, |generic_context| {
                            alias
                                .specialization(db)
                                .unwrap_or_else(|| generic_context.default_specialization(db, None))
                                .apply_specialization(db, current_specialization)
                        }))
                    }
                    _ => {
                        // IMPORTANT: All processing must happen inside a single visitor.visit() call so that if we encounter
                        // this same TypeAlias again (e.g., in `type RecursiveT = int | tuple[RecursiveT, ...]`), the visitor
                        // will detect the cycle and return the fallback value.
                        let mapped = visitor.visit(db, self, type_mapping, || {
                            alias.value_type(db).apply_type_mapping_impl(
                                db,
                                type_mapping,
                                tcx,
                                visitor,
                            )
                        });

                        // If the type mapping does not result in any change to this type alias, keep the
                        // alias node instead of eagerly expanding it. A recursive backedge also returns
                        // the alias itself, and fully static aliases must retain their original identity.
                        if mapped == self || alias.value_type(db) == mapped {
                            self
                        } else if let TypeMapping::Materialize(materialization_kind) = type_mapping
                            && alias.is_recursive(db)
                        {
                            Type::TypeAlias(
                                alias.with_materialization_kind(db, Some(*materialization_kind)),
                            )
                        } else {
                            mapped
                        }
                    }
                }
            }

            Type::LiteralValue(_) => match type_mapping {
                TypeMapping::ApplySpecialization(_)
                | TypeMapping::ApplySpecializationWithMaterialization { .. }
                | TypeMapping::BindLegacyTypevars(_)
                | TypeMapping::FreshenBoundTypeVars { .. }
                | TypeMapping::BindSelf { .. }
                | TypeMapping::ReplaceSelf { .. }
                | TypeMapping::Materialize(_)
                | TypeMapping::ReplaceParameterDefaults
                | TypeMapping::EagerExpansion
                | TypeMapping::RescopeReturnCallables(_)
                | TypeMapping::Promote(PromotionMode::Off, _)
                | TypeMapping::Promote(
                    PromotionMode::On,
                    PromotionKind::ClassLiteralsOnly | PromotionKind::SingletonsOnly,
                ) => self,
                TypeMapping::Promote(PromotionMode::On, PromotionKind::Regular) => {
                    self.promote_impl(db, visitor.env)
                }
            },

            Type::Dynamic(_) => match type_mapping {
                TypeMapping::ApplySpecialization(_)
                | TypeMapping::ApplySpecializationWithMaterialization { .. }
                | TypeMapping::BindLegacyTypevars(_)
                | TypeMapping::FreshenBoundTypeVars { .. }
                | TypeMapping::BindSelf(..)
                | TypeMapping::ReplaceSelf { .. }
                | TypeMapping::Promote(..)
                | TypeMapping::ReplaceParameterDefaults
                | TypeMapping::EagerExpansion
                | TypeMapping::RescopeReturnCallables(_) => self,
                TypeMapping::Materialize(materialization_kind) => match materialization_kind {
                    MaterializationKind::Top => Type::object(),
                    MaterializationKind::Bottom => Type::Never,
                },
            },
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
                | KnownBoundMethodType::ConstraintSetLowerBound
                | KnownBoundMethodType::ConstraintSetUpperBound
                | KnownBoundMethodType::ConstraintSetEquality
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetExists(_)
                | KnownBoundMethodType::ConstraintSetForAll(_)
                | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
                | KnownBoundMethodType::ConstraintSetSolutions(_)
                | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_),
            )
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::BoundSuper(_)
            | Type::SpecialForm(_) => self,

            // A non-generic class never needs to be specialized. A generic class is specialized
            // explicitly (via a subscript expression) or implicitly (via a call), and not because
            // some other generic context's specialization is applied to it.
            Type::ClassLiteral(_) => self,
        }
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
        self.find_legacy_typevars_impl(
            db,
            env,
            binding_context,
            typevars,
            &FindLegacyTypeVarsVisitor::default(),
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
        let matching_typevar = |bound_typevar: &BoundTypeVarInstance<'db>| {
            match bound_typevar.typevar(db).kind(db) {
                TypeVarKind::LegacyTypeVar | TypeVarKind::Pep613Alias | TypeVarKind::TypingSelf
                    if binding_context.is_none_or(|binding_context| {
                        bound_typevar.binding_context(db)
                            == BindingContext::Definition(binding_context)
                    }) =>
                {
                    Some(*bound_typevar)
                }
                TypeVarKind::LegacyTypeVarTuple
                    if binding_context.is_none_or(|binding_context| {
                        bound_typevar.binding_context(db)
                            == BindingContext::Definition(binding_context)
                    }) =>
                {
                    Some(*bound_typevar)
                }
                TypeVarKind::LegacyParamSpec
                    if binding_context.is_none_or(|binding_context| {
                        bound_typevar.binding_context(db)
                            == BindingContext::Definition(binding_context)
                    }) =>
                {
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
                visitor.visit(db, self, || {
                    function.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
                });
            }

            Type::BoundMethod(method) => visitor.visit(db, self, || {
                method.self_instance(db).find_legacy_typevars_impl(
                    db,
                    env,
                    binding_context,
                    typevars,
                    visitor,
                );
                method.function(db).find_legacy_typevars_impl(
                    db,
                    env,
                    binding_context,
                    typevars,
                    visitor,
                );
            }),

            Type::KnownBoundMethod(
                KnownBoundMethodType::FunctionTypeDunderGet(function)
                | KnownBoundMethodType::FunctionTypeDunderCall(function),
            ) => visitor.visit(db, self, || {
                function.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
            }),

            Type::KnownBoundMethod(
                KnownBoundMethodType::PropertyDunderGet(property)
                | KnownBoundMethodType::PropertyDunderSet(property)
                | KnownBoundMethodType::PropertyDunderDelete(property),
            ) => visitor.visit(db, self, || {
                property.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
            }),

            Type::Callable(callable) => {
                callable.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
            }

            Type::PropertyInstance(property) => visitor.visit(db, self, || {
                property.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
            }),

            Type::SlotDescriptor(descriptor) => visitor.visit(db, self, || {
                descriptor.value_type(db).find_legacy_typevars_impl(
                    db,
                    env,
                    binding_context,
                    typevars,
                    visitor,
                );
            }),

            Type::Union(union) => {
                for element in union.elements(db) {
                    element.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
                }
            }
            Type::Intersection(intersection) => {
                for positive in intersection.positive(db) {
                    positive.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
                }
                for negative in intersection.negative(db) {
                    negative.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
                }
            }
            Type::EnumComplement(complement) => {
                for rest in complement.rest(db) {
                    rest.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
                }
            }

            Type::GenericAlias(alias) => {
                alias.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
            }

            Type::NominalInstance(instance) => {
                instance.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
            }

            Type::ProtocolInstance(instance) => {
                instance.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
            }

            Type::TypedDict(TypedDictType::Class(class)) => {
                class.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
            }

            // Synthesized schemas can contain type variables, but their internal narrowing and
            // update constraints inherit those variables from an existing generic context.
            Type::TypedDict(TypedDictType::Synthesized(_)) => {}

            Type::NewTypeInstance(_) => {
                // A newtype can never be constructed from an unspecialized generic class, so it is
                // impossible that we could ever find any legacy typevars in a newtype instance or
                // its underlying class.
            }

            Type::SubclassOf(subclass_of) => {
                subclass_of.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
            }

            Type::TypeIs(type_is) => {
                type_is.type_argument(db).find_legacy_typevars_impl(
                    db,
                    env,
                    binding_context,
                    typevars,
                    visitor,
                );
            }

            Type::TypeGuard(type_guard) => {
                type_guard.return_type(db).find_legacy_typevars_impl(
                    db,
                    env,
                    binding_context,
                    typevars,
                    visitor,
                );
            }

            Type::TypeForm(typeform) => {
                typeform.type_argument(db).find_legacy_typevars_impl(
                    db,
                    env,
                    binding_context,
                    typevars,
                    visitor,
                );
            }

            Type::TypeAlias(alias) => {
                visitor.visit(db, self, || {
                    alias.value_type(db).find_legacy_typevars_impl(
                        db,
                        env,
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
                            env,
                            binding_context,
                            typevars,
                            visitor,
                        );
                    }
                }
                KnownInstanceType::Annotated(ty) => {
                    ty.inner(db).find_legacy_typevars_impl(
                        db,
                        env,
                        binding_context,
                        typevars,
                        visitor,
                    );
                }
                KnownInstanceType::Callable(callable_type) => {
                    callable_type.find_legacy_typevars_impl(
                        db,
                        env,
                        binding_context,
                        typevars,
                        visitor,
                    );
                }
                KnownInstanceType::TypeGenericAlias(ty)
                | KnownInstanceType::LiteralStringAlias(ty) => {
                    ty.inner(db).find_legacy_typevars_impl(
                        db,
                        env,
                        binding_context,
                        typevars,
                        visitor,
                    );
                }
                KnownInstanceType::SubscriptedProtocol(_)
                | KnownInstanceType::SubscriptedGeneric(_)
                | KnownInstanceType::TypeVar(_)
                | KnownInstanceType::TypeAliasType(_)
                | KnownInstanceType::Deprecated(_)
                | KnownInstanceType::Field(_)
                | KnownInstanceType::ConstraintSet(_)
                | KnownInstanceType::ConstraintSetSolution(_)
                | KnownInstanceType::GenericContext(_)
                | KnownInstanceType::Specialization(_)
                | KnownInstanceType::Literal(_)
                | KnownInstanceType::NamedTupleSpec(_)
                | KnownInstanceType::NewType(_)
                | KnownInstanceType::Sentinel(_)
                | KnownInstanceType::Range { .. }
                | KnownInstanceType::FunctoolsPartial(_)
                | KnownInstanceType::FunctoolsPartialCall(_) => {
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
                | KnownBoundMethodType::ConstraintSetLowerBound
                | KnownBoundMethodType::ConstraintSetUpperBound
                | KnownBoundMethodType::ConstraintSetEquality
                | KnownBoundMethodType::ConstraintSetRange
                | KnownBoundMethodType::ConstraintSetAlways
                | KnownBoundMethodType::ConstraintSetNever
                | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                | KnownBoundMethodType::ConstraintSetSatisfies(_)
                | KnownBoundMethodType::ConstraintSetExists(_)
                | KnownBoundMethodType::ConstraintSetForAll(_)
                | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
                | KnownBoundMethodType::ConstraintSetSolutions(_)
                | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_),
            )
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::ModuleLiteral(_)
            | Type::ClassLiteral(_)
            | Type::LiteralValue(_)
            | Type::BoundSuper(_)
            | Type::SpecialForm(_) => {}
        }
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

    #[salsa::tracked(
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
            Self::BoundMethod(method) => {
                Some(TypeDefinition::Function(method.function(db).definition(db)))
            }
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
        let mut variables = FxOrderSet::default();
        self.find_legacy_typevars(db, env, None, &mut variables);
        let generic_context = GenericContext::from_typevar_instances(db, env, variables);
        self.apply_specialization(db, generic_context.default_specialization(db, None))
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

    // Calls the dunder on each element separately and combines the results.
    // This avoids intersecting bound methods (which often collapses to Never)
    // and instead intersects the return types.
    //
    // TODO: we might be able to remove this after fixing
    // https://github.com/astral-sh/ty/issues/2428.
    fn try_call_dunder_with_policy(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        argument_types: &mut CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        if let Some(alternatives) = self.finite_alternative_union(db, env) {
            return alternatives.try_call_dunder_with_policy(
                db,
                env,
                name,
                argument_types,
                tcx,
                policy,
            );
        }

        // Using `positive()` rather than `positive_elements_or_object()` is safe
        // here because `object` does not define any of the dunders that are called
        // through this path without `MRO_NO_OBJECT_FALLBACK` (e.g. `__await__`,
        // `__iter__`, `__enter__`, `__bool__`).
        let positive = self.positive(db);
        let mut successful_bindings = Vec::with_capacity(positive.len());
        let mut last_error = None;
        let mut error_provenance = Provenance::Unknown;

        for element in positive {
            match element.try_call_dunder_with_policy(db, env, name, argument_types, tcx, policy) {
                Ok(bindings) => successful_bindings.push(bindings),
                Err(err) => {
                    error_provenance = error_provenance.or(err.provenance());
                    last_error = Some(err);
                }
            }
        }

        if successful_bindings.is_empty() {
            // TODO we are only showing one of the errors here; should we aggregate
            // them somehow or show all of them?
            return Err(last_error
                .unwrap_or(CallDunderError::MethodNotAvailable)
                .with_provenance(error_provenance));
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
        env: &ProgramEnvironment<'db>,
        name: &str,
        argument_types: &mut CallArguments<'_, 'db>,
        tcx: TypeContext<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<Bindings<'db>, CallDunderError<'db>> {
        let elements = self.elements(db);
        let mut builder = UnionBuilder::new(db, env);
        let mut unbound_on: Vec<Type<'db>> = Vec::new();
        let mut any_defined = false;
        let mut possibly_undefined = false;
        let mut provenance = Provenance::Unknown;

        for element in elements {
            match element
                .member_lookup_with_policy(
                    db,
                    env,
                    name,
                    policy | MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place
            {
                Place::Defined(DefinedPlace {
                    ty,
                    definedness: Definedness::PossiblyUndefined,
                    provenance: member_provenance,
                    ..
                }) => {
                    builder = builder.add(ty);
                    any_defined = true;
                    possibly_undefined = true;
                    provenance = provenance.or(member_provenance);
                }
                Place::Defined(DefinedPlace {
                    ty,
                    provenance: member_provenance,
                    ..
                }) => {
                    builder = builder.add(ty);
                    any_defined = true;
                    provenance = provenance.or(member_provenance);
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
        let bindings = match dunder_callable
            .bindings(db, env)
            .match_parameters(db, env, argument_types)
            .check_types(db, env, &constraints, argument_types, tcx, &[])
        {
            Ok(bindings) => bindings,
            Err(CallError(kind, bindings)) => {
                return Err(CallDunderError::CallError(kind, bindings, provenance));
            }
        };

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
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> TypeVarVariance {
        tracing::trace!(
            "Checking variance of '{tvar}' in `{ty:?}`",
            tvar = typevar.identity.name(db),
            ty = self.display(db, env),
        );

        let v = match self {
            Type::ClassLiteral(class_literal) => class_literal.variance_of(db, env, typevar),

            Type::FunctionLiteral(function_type) => {
                // TODO: do we need to replace self?
                function_type.variance_of(db, typevar)
            }

            Type::BoundMethod(method_type) => {
                // TODO: do we need to replace self?
                method_type.function(db).variance_of(db, typevar)
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
                TypeVarVariance::Covariant
            }
            Type::ProtocolInstance(protocol_instance_type) => {
                protocol_instance_type.variance_of(db, env, typevar)
            }
            Type::TypedDict(typed_dict) => typed_dict.variance_of(db, env, typevar),
            // unions are covariant in their disjuncts
            Type::Union(union_type) => union_type
                .elements(db)
                .iter()
                .map(|ty| ty.variance_of(db, env, typevar))
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
                .map(|ty| ty.variance_of(db, env, typevar))
                .chain(intersection_type.negative(db).iter().map(|ty| {
                    ty.with_polarity(TypeVarVariance::Contravariant)
                        .variance_of(db, env, typevar)
                }))
                .collect(),
            Type::EnumComplement(complement) => complement
                .to_intersection(db, env)
                .variance_of(db, env, typevar),
            Type::PropertyInstance(property_instance_type) => [
                Some(property_instance_type.instance_fallback(db, env)),
                property_instance_type.getter(db),
                property_instance_type.setter(db),
                property_instance_type.deleter(db),
            ]
            .into_iter()
            .flatten()
            .map(|ty| ty.variance_of(db, env, typevar))
            .collect(),
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
            | Type::NewTypeInstance(_) => TypeVarVariance::Bivariant,
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

#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
fn class_mro_literals<'db>(
    db: &'db dyn Db,
    class_literal: ClassLiteral<'db>,
) -> Box<[ClassLiteral<'db>]> {
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
    fn self_type(&self) -> Type<'db> {
        self.ty
    }

    fn binding_context(&self) -> Option<BindingContext<'db>> {
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
        let class_literal = match self_type {
            Type::TypeVar(typevar) if typevar.typevar(db).is_self(db) => {
                self_typevar_owner_class_literal(db, env, typevar)
            }
            _ => self_type
                .nominal_class(db, env)
                .map(|class| class.class_literal(db)),
        };

        Self {
            ty: self_type,
            class_literal,
            binding_context,
        }
    }

    /// Returns whether `bound_typevar` should be replaced by this binding's concrete self type.
    fn should_bind(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bound_typevar: BoundTypeVarInstance<'db>,
    ) -> bool {
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
            self_typevar_owner_class_literal(db, env, bound_typevar)
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
    RescopeReturnCallables(&'a FxHashMap<CallableType<'db>, CallableType<'db>>),
}

impl<'db> TypeMapping<'_, 'db> {
    /// Update the generic context of a [`Signature`] according to the current type mapping
    fn update_signature_generic_context(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        context: GenericContext<'db>,
    ) -> GenericContext<'db> {
        match self {
            TypeMapping::FreshenBoundTypeVars { .. } => GenericContext::from_typevar_instances(
                db,
                env,
                context.variables(db).map(|bound_typevar| {
                    Type::TypeVar(bound_typevar)
                        .apply_type_mapping(db, env, self, TypeContext::default())
                        .as_typevar()
                        .unwrap_or(bound_typevar)
                }),
            ),
            TypeMapping::ApplySpecialization(specialization)
            | TypeMapping::ApplySpecializationWithMaterialization { specialization, .. } => {
                // Filter out type variables that are already specialized
                // (i.e., mapped to a non-TypeVar type)
                let kept = context.variables(db).filter(|bound_typevar| {
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
                });
                if specialization.specialize_self_domain() {
                    let kept = kept.filter_map(|bound_typevar| {
                        Type::TypeVar(bound_typevar)
                            .apply_type_mapping(
                                db,
                                env,
                                &TypeMapping::ApplySpecialization(*specialization),
                                TypeContext::default(),
                            )
                            .as_typevar()
                    });
                    GenericContext::from_typevar_instances(db, env, kept)
                } else {
                    GenericContext::from_typevar_instances(db, env, kept)
                }
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
                env,
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
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::FreshenBoundTypeVars { .. }
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
/// (e.g. `Divergent` is assignable to `@Todo`, but `@Todo | Divergent` must not be reduced to `@Todo`).
/// Otherwise, type inference cannot converge properly.
/// For detailed properties of this type, see the unit test at the end of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
}

impl std::fmt::Display for DynamicType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DynamicType::Any => f.write_str("Any"),
            DynamicType::Unknown
            | DynamicType::UnknownGeneric(_)
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
    /// Type qualifiers that appear in an annotation expression.
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

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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
            .filter_map(|submodule_name| submodule_name.relative_to(self.module(db).name(db)))
            .filter_map(|relative_submodule| relative_submodule.components().next().map(Name::from))
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
        let module = self.module(db);
        // `__dict__` is a very special member that is never overridden by module globals;
        // we should always look it up directly as an attribute on `types.ModuleType`,
        // never in the global scope of the module.
        if name == "__dict__" {
            return KnownClass::ModuleType
                .to_instance(db, env)
                .member(db, env, "__dict__")
                .into();
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

        let file = module
            .file(db)
            .map(|file| ProgramFile::new(db, file, env.program(db)));
        let place_and_qualifiers = imported_symbol(db, env, file, name, None);

        // If the normal lookup failed, try to call the module's `__getattr__` function
        if place_and_qualifiers.place.is_undefined() {
            return self.try_module_getattr(db, env, name);
        }

        // typeshed re-exports some special forms across modules (e.g. `collections.abc.Callable`
        // is `from typing import Callable as Callable`). The resolved type still carries the
        // definition-site variant (`SpecialFormType::TypingCallable`), so we recover the
        // import-path identity here while it's still observable.
        if let Place::Defined(defined) = place_and_qualifiers.place
            && let Type::SpecialForm(special) = defined.ty
            && let Some(import_module) = self.module(db).known(db)
        {
            let rewrapped = special.rewrap_for_import_module(name, import_module);
            if rewrapped != special {
                return PlaceAndQualifiers {
                    place: Place::Defined(DefinedPlace {
                        ty: Type::SpecialForm(rewrapped),
                        ..defined
                    }),
                    qualifiers: place_and_qualifiers.qualifiers,
                }
                .into();
            }
        }

        place_and_qualifiers.into()
    }
}

/// Either the explicit `metaclass=` keyword of the class, or the inferred metaclass of one of its base classes.
#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
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

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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
    ) -> TypeVarVariance {
        self.type_argument(db)
            .with_polarity(TypeVarVariance::Invariant)
            .variance_of(db, env, typevar)
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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
    ) -> TypeVarVariance {
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
pub(super) fn determine_upper_bound<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class_literal: ClassLiteral<'db>,
    is_known_base: impl Fn(ClassBase<'db>) -> bool,
) -> Type<'db> {
    let upper_bound = class_literal
        .iter_mro(db)
        .take_while(|base| !is_known_base(*base))
        .filter_map(ClassBase::into_class)
        .last()
        .unwrap_or_else(|| class_literal.unknown_specialization(db));
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
