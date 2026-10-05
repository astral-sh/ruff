//! Source producers borrowing the canonical entry's query and admission capabilities.

mod abstract_methods;
mod annotated_assignment;
mod annotation_expression;
mod applicable_constraints;
mod assignment;
mod assignment_validation;
mod attribute;
mod bound_default;
mod builtin_lookup;
mod callable;
mod callable_guard;
mod constructor_binding_storage;
mod constructor_callable;
mod constructor_members;
mod constructor_matching;
mod constructor_preparation;
mod constructor_signature;
mod contextual_tuple;
mod chained_comparison;
mod class;
mod class_bases;
mod class_checks;
mod class_context;
mod class_finality;
mod class_mro;
mod class_object_entry;
mod class_object_member;
mod constructor_new;
mod class_selection;
pub(in crate::types::infer) use class_selection::FixedFieldCopy;
mod code_generator;
mod data_descriptor;
mod declaration;
mod declaration_binding;
mod default_specialization;
#[cfg(test)]
pub(in crate::types) use default_specialization::observations as legacy_callable_observations;
mod deferred;
mod deferred_assignment;
mod deferred_type_parameter;
mod descriptor;
mod dunder_callable;
mod dunder_all;
mod enum_class;
mod enum_inheritance;
mod environment;
mod equality;
mod expression;
mod file;
mod file_suppressions;
mod function;
mod type_parameter_shadow;
mod type_parameter;
mod function_application;
mod function_body;
mod function_decorators;
mod function_descriptor;
pub(in crate::types::infer) mod function_last_signature;
mod generic_intersection;
mod generics;
mod identity_specialization;
mod guarded_member;
mod implicit_globals;
mod implicit_names;
mod implicit_place;
mod inheritance_cycle;
mod inherited_context;
mod inherited_function_context;
mod inner_metaclass;
mod imports;
mod instance_flags;
mod instance_layout;
mod instance_member_source;
mod intersection;
mod invocation;
mod known_class;
mod known_class_bindings;
mod lazy_typevar_default;
mod legacy_context;
mod lint_diagnostic;
pub(in crate::types::infer::builder) mod lint_diagnostic_cost;
mod literal_sequence;
mod local_functions;
pub(in crate::types::infer) mod local_transfer;
mod materialization;
mod member_dunder;
mod member_finalization;
mod member_lookup;
mod member_normalization;
mod member_self_binding;
mod member_source;
mod member_storage;
mod metaclass;
mod metaclass_reconciliation;
mod mro_dispatch;
mod narrowing;
mod negate;
mod nominal_member;
mod normalization;
mod origin;
mod own_member;
mod override_variable_kind;
mod parameter;
mod place;
mod public_promotion;
mod postchecks;
mod property_deprecations;
mod return_typevar;
mod property_provenance;
mod reachability;
mod relations;
pub(in crate::types::infer) mod receiver_constraints;
mod r#return;
mod runtime_visibility;
mod scope;
mod scope_members;
mod return_context;
mod return_locations;
mod return_scoping;
mod signature;
mod self_binding;
mod statement;
pub(in crate::types::infer::builder) mod storage;
mod subclass_metaclass;
mod subscript;
mod subscript_specialization;
mod suite_checks;
mod truthiness;
mod tuple_class;
mod tuple_expression;
mod tuple_resize;
mod tuple_spec;
mod tuple_widening;
mod type_algebra;
mod type_comparison;
mod type_conversion;
mod typing_self;
mod type_search;
mod typevar_default;
mod typevar_set;
mod typevartuple;
mod union;

#[cfg(test)]
pub(in crate::types::infer) use declaration::observations as declaration_observations;

#[cfg(test)]
pub(in crate::types::infer) use tuple_widening::observations as tuple_widening_observations;

#[cfg(test)]
pub(in crate::types::infer) use crate::types::infer::builder::function::source_effects::FunctionDefinitionEffects;

use std::future::Future;
use std::pin::Pin;

use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
use ruff_db::source::SourceText;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{
    BorrowOrCopy, ExecutionWork, FieldReadProfile, FieldRequest, RunError, RunResult, TaskEndpoint,
};
use ty_module_resolver::{KnownModule, Module, ModuleName};
use ty_python_core::definition::{
    AnnotatedAssignmentDefinitionKind, AssignmentDefinitionKind, Definition, DefinitionKind,
};
use ty_python_core::expression::{Expression, ExpressionKind};
#[cfg(test)]
use ty_python_core::narrowing_constraints::ConstraintKey;
#[cfg(test)]
use ty_python_core::place::PlaceExpr;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::CallableAndCallExpr;
use ty_python_core::program::Program;
#[cfg(test)]
use ty_python_core::scope::FileScopeId;
use ty_python_core::scope::ScopeId;
use ty_python_core::{PlaceTable, ProgramFile, SemanticIndex, UseDefMap};

use super::{DefinitionEffects, SourceDefinitionEffect};
use crate::place::{ConsideredDefinitions, PlaceAndQualifiers, RequiresExplicitReExport};
use crate::suppression::Suppressions;
use crate::types::ModuleLiteralType;
use crate::types::abstract_methods::AbstractMethod;
use crate::types::callable::{CallableType, CallableTypeKind};
use crate::types::class::slots::InstanceLayout;
use crate::types::class::{
    ClassInstanceFlags, CodeGeneratorKind, KnownClass, KnownClassLookupError, StaticClassLiteral,
};
use crate::types::context::InferContext;
use crate::types::descriptor::{DescriptorRequest, DescriptorResult};
use crate::types::enums::{EnumClassLiteral, EnumMetadata};
use crate::types::function::{
    DataclassTransformerFlags, DataclassTransformerParams, FunctionLiteral, FunctionType,
    OverloadLiteral, UpdatedFunctionSignatures,
};
use crate::types::generics::Specialization;
use crate::types::infer::builder::annotated_assignment::infer_annotated_assignment_definition_with;
#[cfg(test)]
use crate::types::infer::builder::applicable_constraints::{
    ApplicableConstraintsFacts, narrow_expr_with_applicable_constraints_with,
};
use crate::types::infer::builder::class::source_effects::ClassIdentity;
use crate::types::infer::builder::function::source_effects::OverloadIdentity;
use crate::types::infer::builder::source_expression::SourceExpressionOperation;
use crate::types::infer::builder::type_expression::TypeExpressionMode;
use crate::types::infer::builder::{TypeContext, TypeInferenceBuilder, local};
use crate::types::infer::{
    DefinitionInference, ExpressionInference, FunctionDecoratorInference, InferenceRegion,
    ScopeInference,
};
use crate::types::method::{BoundMethodReceiver, BoundMethodType};
use crate::types::mro::{Mro, StaticMroError};
use crate::types::narrow::ExpressionNarrowingConstraints;
use crate::types::set_theoretic::NegativeIntersectionElements;
use crate::types::set_theoretic::builder::{IntersectionPolarity, IntersectionSimplification};
use crate::types::signatures::{CallableSignature, Signature};
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::typevar::{
    BoundTypeVarIdentity, BoundTypeVarInstance, TypeVarBoundOrConstraintsEvaluation,
    TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance, TypeVarKind,
};
use crate::types::{
    ClassType, DescriptorArgumentComparison, DescriptorDispatch, DescriptorDispatches, DescriptorGetCallContext, GenericAlias, GenericContext, IntersectionType,
    MaterializationKind, MemberLookupKey,
    MemberLookupPolicy, MemberLookupResult, PropertyAccessorDefinitions, PropertyInstanceClass,
    PropertyInstanceType, RecursivelyDefined, Truthiness, Type, TypeVarVariance, UnionType,
};
use crate::{Db, FxIndexMap, FxOrderMap, FxOrderSet, ProgramEnvironment};

/// Each variant names a source operation whose producer is still unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum SourceOperation {
    ImplicitName(crate::analysis::ImplicitNameOperation),
    StatementQuery,
    StatementConditionDiagnostic,
    StatementTry,
    StatementWith,
    StatementMatch,
    StatementAssignment,
    StatementAnnotatedAssignment,
    StatementAugmentedAssignment,
    StatementTypeAlias,
    StatementFor,
    StatementWhile,
    StatementImport,
    StatementImportFrom,
    StatementAssert,
    StatementRaise,
    StatementReturn,
    StatementDelete,
    StatementGlobal,
    ScopeBody,
    ParameterAnnotation,
    ParameterDefault,
    TypingSelfBodyScope,
    BindingOwner,
    BindingStorage,
    BindingDiagnostic,
    BindingMember,
    AssignmentValidation(crate::analysis::AssignmentValidationOperation),
    AssignmentUnpack,
    AssignmentCall,
    AssignmentNamedTuple,
    AssignmentTypedDict,
    AssignmentNewClass,
    AssignmentEnum,
    AssignmentParamSpec,
    AssignmentTypeVarTuple,
    AssignmentNewType,
    AssignmentBuiltinType,
    AssignmentTypeAliasType,
    AssignmentSentinel,
    AssignmentDesugaredDecorator,
    AssignmentDiagnostic,
    AnnotatedAssignment(
        crate::types::infer::builder::annotated_assignment::AnnotatedAssignmentOperation,
    ),
    LegacyTypeVarDiagnostic,
    LegacyTypeVarForwardedOwner,
    TypeVarDefault(crate::analysis::TypeVarDefaultOperation),
    BoundTypeVarDefaultCycleNormalization,
    BoundTypeVarDefaultRecursiveNormalization,
    LegacyTypeVariables(crate::types::legacy_typevars::LegacyTypeVarOperation),
    DefaultSpecialization(crate::types::generics::defaults::DefaultSpecializationOperation),
    FunctionParamSpec,
    FunctionUnpackedKwargs,
    FunctionReturnCheck,
    ReturnGeneratorType,
    ReturnNoneType,

    ScopeDeferred,
    ScopePostcheck,
    ClassCheck(crate::analysis::ClassCheckOperation),
    OwnMember(crate::analysis::OwnMemberOperation),
    Deferred(crate::analysis::DeferredInferenceOperation),
    Definition(SourceDefinitionEffect),
    Expression(SourceExpressionOperation),
    ExpressionScope,
    ContextualExpression,
    DataclassFieldSpecifiers,
    AnnotationString,
    QuotedAnnotation(crate::analysis::QuotedAnnotationOperation),
    AnnotationQualifier,
    AnnotationConditionalAlias,
    TypeExpressionLegacy,
    TypeExpressionUnionRuntimeValidation,
    TypeExpressionSubscript,
    TypeExpressionCallableUnpack,
    TypeExpressionRecursiveAlias,
    TypeExpressionParamSpecAttribute,
    TypeExpressionInvalid,
    TypeExpressionInitTypeVariableReport,
    TypeExpressionAliasTypeVariableReport,
    TypeExpressionUnboundTypeVariableReport,
    TypeExpressionSubclassArgument,
    TypeConversion(crate::types::TypeConversionOperation),
    SubscriptExpectedKeys,
    SubscriptImplicitAlias,
    SubscriptReceiver,
    SubscriptExpressionTypes,
    SubscriptDiagnostic,
    SubscriptLegacyArgumentTraversal,
    ExplicitSpecializationProtocolMember,
    ExplicitSpecializationParamSpec,
    ExplicitSpecializationVariadic,
    ExplicitSpecializationLazyUpperBound,
    ExplicitSpecializationLazyConstraints,
    ExplicitSpecializationBounds,
    ExplicitSpecializationBoundMapping,
    ExplicitSpecializationDiagnostic,
    ExplicitSpecializationTupleClass,
    ExplicitSpecializationTypeClass,
    FunctionDecorator,
    DecoratorApplication(crate::analysis::DecoratorApplicationOperation),
    FunctionMetadata,
    FunctionTypeParameterShadowDiagnostic,
    TypeParameterConstraintCountDiagnostic,
    ImportPolicy,
    ImportDiagnostic,
    ImportBinding,
    Submodule,
    ModuleGetattr,
    ModuleTypeMember,
    ImplicitPlace,
    PlaceScope,
    PlacePromotion,
    DiscardedBinding,
    LoopHeader,
    Reachability,
    ReachabilityPrefix,
    ReachabilityCheckpoint,
    ReachabilityPredicate,
    Truthiness(crate::types::TruthinessOperation),
    ChainedComparison(crate::types::ChainedComparisonOperation),
    KnownClassInstance(crate::types::class::KnownClassInstanceOperation),
    InstanceFlagsMetaclass,
    ExplicitAnyInstanceConstruction,
    TypeComparison(crate::types::TypeComparisonOperation),
    TupleSpec(crate::types::TupleSpecOperation),
    RecursiveNormalization(crate::types::RecursiveNormalizationOperation),
    GenericAliasCycleMerge,
    Equality(crate::types::EqualityOperation),
    Attribute(crate::types::AttributeOperation),
    MemberLookup(crate::types::member_lookup::general::GeneralMemberOperation),
    Descriptor(crate::types::descriptor::effects::DescriptorOperation),
    CallableConversion(crate::types::callable::CallableConversionOperation),
    TypeSearch(crate::types::visitor::SearchOperation),
    Narrowing,
    Union,
    FunctionComparison,
    Equivalence,
    MissingBinding,
    CanonicalMerge,
    ExpressionStorage,
    ExpressionCache,
    CallArguments,
    CallBindings,
    CallableGuard(crate::analysis::CallableGuardOperation),
    ConstructorPreparation(crate::analysis::ConstructorPreparationOperation),
    ConstructorStorage(crate::analysis::ConstructorStorageOperation),
    ConstructorSignature(crate::analysis::ConstructorSignatureOperation),
    BoundMethodPreparation(crate::analysis::BoundMethodPreparationOperation),
    CallParameterMatching,
    CallGenericFreshening,
    CallConstructorMatching,
    CallVariadicMatching,
    CallKeywordMatching,
    CallUnpackedMatching,
    ArgumentPreparation,
    ArgumentCandidates,
    ArgumentGenericContext,
    ArgumentNarrowing,
    ArgumentSpeculation,
    ArgumentTypeContext,
    ArgumentChecking,
    CheckerArgumentExpansion,
    CheckerGenericInference,
    CheckerKnownFunction,
    CheckerOverloadFiltering,
    CheckerParameterUnion,
    CheckerParamSpec,
    CheckerSpecialization,
    CheckerSplat,
    CheckerConstructor,
    CheckerDownstreamConstructor,
    CheckerEquivalent,
    CheckerClassInfo,
    CheckerTypeVarTuple,
    CheckerConstructorReceiver,
    CallConstructorReturn,
    CallDeprecationBoundMethodType,
    CallDeprecationBoundMethodFunction,
    CallDeprecationDownstreamConstructor,
    CallDeprecationDiagnostic,
    CallDiscardedExtraArguments,
    CallKnownFunctionCheck,
    CallKnownClassCheck,
    CallNeverReveal,
    CallReceiverConstraints,
    CallRangeInference,
    CallTypeGuardBinding,
    CallDiagnostic,
    CallCollectionReturn,
    CallReturnUnionBuilder,
    CallReturnIntersectionBuilder,
    Relation(crate::types::RelationOperation),
    Materialization(crate::types::MaterializationOperation),
    PublicPromotion(crate::types::MaterializationOperation),
    SelfMapping(crate::types::MaterializationOperation),
    FreshenBoundTypeVars(crate::types::MaterializationOperation),
    ReturnCallableMapping(crate::types::MaterializationOperation),
    Specialization(crate::types::MaterializationOperation),
    LegacyTypeVarBinding(crate::types::MaterializationOperation),
    TypeAliasResolution,
    TypeVarBounds,
    TypeVarBindingFunctionContext,
    TypeVarBindingCapturedParamSpec,
    TypeVarBindingAliasContext,
    TypeVarDomainTop,
    TypeVarConstraintUnion,
    NewTypeBase,
    EnumMemberValue,
    EnumComplementIntern,
    EnumComplementLiteralUnion,
    ClassSelection,
    GenericIntersection,
    RecursiveTypeUnfold,
    ContextualTypeFormPositive,
    ContextualTypeFormFallback,
    ContextualLiteralFilter,
    ContextualLiteralAssignability,
    CollectionConstraintStorage,
    CallMetadata,
    CallSpecial,
    SignatureTypeParameters,
    SignatureClassReceiver,
    SignatureAsyncReturn,
    SignatureParameterNormalization,
    SignatureGenericContext,
    SignatureReturnCallables,
    SuiteAwaitableNominal,
    SuiteAwaitableUnion,
    SuiteAwaitableIntersection,
    SuiteAwaitableKnownFunction,
    SuiteAwaitableDiagnostic,
    SuiteRedundantIf,
    SuiteRedundantAssert,
    SuiteRedundantWhile,
    SuiteRedundantMatch,
    PostCheckDecoratorCalls,
    PostCheckFunctionLegacyPositional,
    PostCheckFunctionPep695,
    PostCheckFunctionTypeVarDefaults,
    PostCheckFunctionTypeVarOrdering,
    PostCheckOverloads,
    PostCheckTypeGuard,
    PostCheckFinalValue,
    PostCheckDynamicClass,
    FileImplicitAlias,
    FileReadError,
    FileParseDiagnostic,
    FileUnsupportedSyntaxDiagnostic,
    FileSemanticSyntaxDiagnostic,
    FileDiagnosticSort,
    FileSuppressionSelection,
    FileSuppressionUnknownRule,
    FileSuppressionInvalid,
    FileSuppressionBlanket,
    FileSuppressionUnused,
    Finalization,
}

/// Real structural preparation, with an additional module pin retained by the root through drain.
pub(in crate::types::infer) struct PreparedSource<'db> {
    pub(in crate::types::infer) file: ProgramFile<'db>,
    pub(in crate::types::infer) module: ParsedModuleRef,
    pub(in crate::types::infer) index: &'db SemanticIndex<'db>,
}

/// Implemented by the one canonical source-access owner; no method starts an execution root.
///
/// Query selectors protect creation and await with `child_call`. Source access reuses the
/// prepared file's retained module and index, keeping the module pinned through driver drain.
/// `unavailable` records its reason in the existing session; it owns no second reason channel.
pub(in crate::types::infer) trait SourceAccess<'run, 'db: 'run>:
    Clone + 'run
{
    type Resources: crate::types::relation::source::resources::RelationResourceAccess<'run, 'db>
        + crate::types::mapping::source::MappingResourceAccess<'run, 'db>;
    fn resources(&self) -> Self::Resources;
    fn db(&self) -> &'db dyn Db;
    fn endpoint(&self) -> &TaskEndpoint<'run, 'db>;

    /// Quotes cloning this access and retiring its nonfinal shared owners as
    /// `(logical work, requested bytes)`.
    /// Includes clone calls, internal initialization/transfers, and aggregate retirement.
    /// The caller admits this quote before cloning and separately quotes the completed access's
    /// construction/return carriers and its transfer into any retained wrapper. Runtime providers
    /// and the driver must keep the underlying owners alive until the clone's children drain.
    fn retained_clone_quote() -> RunResult<(usize, usize)>;

    async fn should_check_file(&self, file: File) -> RunResult<bool>;
    async fn rule_selection(&self, file: File) -> RunResult<&'db crate::lint::RuleSelection>;
    async fn verbose(&self, file: File) -> RunResult<bool>;
    async fn analysis_settings(&self, file: File) -> RunResult<&'db crate::AnalysisSettings>;
    async fn source_text(&self, file: File) -> RunResult<SourceText>;
    async fn suppressions(&self, file: File) -> RunResult<&'db Suppressions>;
    async fn semantic_index(&self, file: ProgramFile<'db>) -> RunResult<&'db SemanticIndex<'db>>;
    async fn place_table(&self, scope: ScopeId<'db>) -> RunResult<&'db PlaceTable>;
    async fn use_def_map(&self, scope: ScopeId<'db>) -> RunResult<&'db UseDefMap<'db>>;
    async fn expression_narrowing_constraints(
        &self,
        expression: Expression<'db>,
    ) -> RunResult<&'db ExpressionNarrowingConstraints<'db>>;
    async fn parsed_module(&self, file: ProgramFile<'db>) -> RunResult<ParsedModuleRef>;
    async fn known_module(&self, file: ProgramFile<'db>) -> RunResult<Option<KnownModule>>;
    async fn file_module(&self, file: ProgramFile<'db>) -> RunResult<Option<Module<'db>>>;
    async fn global_scope(&self, file: ProgramFile<'db>) -> RunResult<ScopeId<'db>>;
    async fn module_type_body_scope(
        &self,
        program: Program<'db>,
    ) -> RunResult<Option<ScopeId<'db>>>;
    async fn effective_variable_kind(
        &self,
        class: ClassType<'db>,
        name: &Name,
    ) -> RunResult<Option<crate::types::overrides::VariableKind>>;
    async fn is_function_definition(
        &self,
        scope: ScopeId<'db>,
        symbol: ty_python_core::symbol::ScopedSymbolId,
    ) -> RunResult<bool>;
    async fn data_descriptor(
        &self,
        program: Program<'db>,
        ty: Type<'db>,
        any_of_union: bool,
    ) -> RunResult<bool>;
    async fn scope(
        &self,
        scope: ScopeId<'db>,
        context: TypeContext<'db>,
    ) -> RunResult<&'db ScopeInference<'db>>;
    async fn expression(
        &self,
        expression: Expression<'db>,
        context: TypeContext<'db>,
    ) -> RunResult<&'db ExpressionInference<'db>>;
    async fn definition(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionInference<'db>>;
    async fn deferred_definition(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionInference<'db>>;
    async fn function_known_decorators(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<&'db FunctionDecoratorInference<'db>>;
    async fn function_overloads(
        &self,
        last_definition: OverloadLiteral<'db>,
    ) -> RunResult<&'db (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>)>;
    async fn runtime_visibility(&self, definition: Definition<'db>) -> RunResult<bool>;
    async fn member_lookup_key(
        &self,
        ty: Type<'db>,
        name: Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupKey<'db>>;
    async fn member_lookup(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>>;
    async fn class_member_lookup(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>>;
    /// Fetches the canonical raw `__new__` member for constructor preparation.
    async fn constructor_new_member(
        &self,
        program: Program<'db>,
        ty: Type<'db>,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>>;
    async fn member_result(
        &self,
        parts: crate::types::LookupParts<'db>,
    ) -> RunResult<MemberLookupResult<'db>>;
    async fn descriptor_get(
        &self,
        request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>>;
    async fn place_by_id(
        &self,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        considered: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>>;
    async fn dunder_all_names(
        &self,
        file: ProgramFile<'db>,
    ) -> RunResult<&'db Option<FxHashSet<Name>>>;
    async fn overload_literal(
        &self,
        identity: OverloadIdentity<'_, 'db>,
    ) -> RunResult<OverloadLiteral<'db>>;
    async fn function_type(&self, literal: FunctionLiteral<'db>) -> RunResult<FunctionType<'db>>;

    /// Interns a function literal with owned signature updates and an optional descriptor override.
    /// The caller funds the updated-signature payload's construction, cloning, nested allocations
    /// and retirement, as well as this method's future, before calling.
    async fn intern_function_type(
        &self,
        literal: FunctionLiteral<'db>,
        updated: Option<Box<UpdatedFunctionSignatures<'db>>>,
        descriptor_kind: Option<CallableTypeKind>,
    ) -> RunResult<FunctionType<'db>>;

    async fn class_literal(
        &self,
        identity: ClassIdentity<'_, 'db>,
    ) -> RunResult<StaticClassLiteral<'db>>;
    async fn intern_typevar_identity(
        &self,
        name: &Name,
        definition: Option<Definition<'db>>,
        kind: TypeVarKind,
    ) -> RunResult<TypeVarIdentity<'db>>;
    async fn intern_typevar_instance(
        &self,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        variance: Option<TypeVarVariance>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> RunResult<TypeVarInstance<'db>>;
    async fn intern_bound_typevar(
        &self,
        typevar: TypeVarInstance<'db>,
        identity: BoundTypeVarIdentity<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>>;
    /// Interns ordered constraints whose buffer construction and retirement the caller has funded.
    async fn intern_typevar_constraints(
        &self,
        elements: Box<[Type<'db>]>,
    ) -> RunResult<crate::types::typevar::TypeVarConstraints<'db>>;
    async fn bound_typevar_default(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>;
    async fn lazy_typevar_default(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>>;
    async fn apply_specialization(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
    ) -> RunResult<Type<'db>>;
    async fn intern_generic_context(
        &self,
        program: Program<'db>,
        variables: FxOrderMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,
    ) -> RunResult<GenericContext<'db>>;
    async fn intern_typevar_set(
        &self,
        variables: crate::types::typevar::construction::TypeVarSetVariables<'db>,
    ) -> RunResult<crate::types::typevar::TypeVarSetInner<'db>>;
    async fn tuple_class(&self, tuple: TupleType<'db>) -> RunResult<ClassType<'db>>;
    async fn intern_specialization(
        &self,
        context: GenericContext<'db>,
        types: Box<[Type<'db>]>,
        materialization: Option<MaterializationKind>,
        tuple: Option<TupleType<'db>>,
    ) -> RunResult<Specialization<'db>>;
    async fn intern_explicit_any_class(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<crate::types::instance::ExplicitAnyInstanceClass<'db>>;
    async fn intern_generic_alias(
        &self,
        origin: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericAlias<'db>>;
    async fn known_class_lookup(
        &self,
        program: Program<'db>,
        class: KnownClass,
    ) -> RunResult<Result<Option<StaticClassLiteral<'db>>, KnownClassLookupError<'db>>>;
    async fn known_class_instance(
        &self,
        program: Program<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>>;
    async fn class_generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>;
    /// Requests the declaration-owned PEP 695 context using the class's canonical query key.
    async fn pep695_class_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>;
    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>>;
    async fn instance_layout(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db InstanceLayout>;
    async fn abstract_methods(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<&'db FxIndexMap<Name, AbstractMethod<'db>>>;
    async fn might_be_explicitly_abstract(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<bool>;
    async fn implicit_attribute_names(&self, scope: ScopeId<'db>) -> RunResult<&'db [Name]>;
    async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]>;
    async fn inheritance_cycle(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<crate::types::class::static_literal::InheritanceCycle>>;
    async fn inner_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<crate::types::class::metaclass_selection::MetaclassSelectionResult<'db>>;
    async fn class_decorators(&self, class: StaticClassLiteral<'db>)
    -> RunResult<&'db [Type<'db>]>;
    async fn inherited_instance_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassInstanceFlags>;
    async fn inherited_class_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>>;
    async fn enum_metadata(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<&'db EnumMetadata<'db>>>;
    async fn enum_class_metadata(
        &self,
        class: crate::types::ClassLiteral<'db>,
    ) -> RunResult<Option<&'db EnumMetadata<'db>>>;
    async fn enum_class_literal(
        &self,
        class: crate::types::ClassLiteral<'db>,
    ) -> RunResult<Option<EnumClassLiteral<'db>>>;
    async fn intern_enum_class(
        &self,
        class: crate::types::ClassLiteral<'db>,
        members: Box<[(Name, Type<'db>)]>,
        aliases: Box<[(Name, Name)]>,
        aliases_are_known: bool,
        members_are_exhaustive: bool,
    ) -> RunResult<EnumClassLiteral<'db>>;
    async fn class_mro_literals(
        &self,
        class: crate::types::ClassLiteral<'db>,
    ) -> RunResult<&'db [crate::types::ClassLiteral<'db>]>;
    async fn static_mro(

        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db Result<Mro<'db>, Box<StaticMroError<'db>>>>;
    async fn source_alias_mro(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<&'db Result<Mro<'db>, Box<StaticMroError<'db>>>>;
    async fn function_signature(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>>;
    /// Fetches the existing canonical query for the last definition's effective signature.
    /// The caller admits this method's future; the result borrows canonical memo storage.
    async fn function_last_definition_signature(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<&'db Signature<'db>>;
    async fn callable_type(
        &self,
        signatures: &'db CallableSignature<'db>,
        kind: CallableTypeKind,
    ) -> RunResult<CallableType<'db>>;
    async fn owned_callable_type(
        &self,
        signatures: CallableSignature<'db>,
        kind: CallableTypeKind,
        deprecated: Option<OverloadLiteral<'db>>,
    ) -> RunResult<CallableType<'db>>;
    async fn intern_bound_method(
        &self,
        func: Type<'db>,
        program: Program<'db>,
        class_method: bool,
        receiver: BoundMethodReceiver<'db>,
    ) -> RunResult<BoundMethodType<'db>>;
    async fn intern_descriptor_get_call_context(
        &self,
        descriptor_type: Type<'db>,
        callable_type: Type<'db>,
        instance: Option<Type<'db>>,
        owner: Type<'db>,
    ) -> RunResult<DescriptorGetCallContext<'db>>;
    async fn descriptor_dispatch(
        &self,
        signatures: CallableSignature<'db>,
        arguments: Box<[Type<'db>]>,
        comparisons: Box<[Box<[DescriptorArgumentComparison<'db>]>]>,
        selected_overloads: Box<[usize]>,
        failed: bool,
    ) -> RunResult<DescriptorDispatch<'db>>;
    async fn descriptor_dispatches(
        &self,
        elements: Box<[DescriptorDispatch<'db>]>,
    ) -> RunResult<DescriptorDispatches<'db>>;
    async fn intern_property(
        &self,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
        instance_class: PropertyInstanceClass<'db>,
        accessor_definitions: PropertyAccessorDefinitions<'db>,
    ) -> RunResult<PropertyInstanceType<'db>>;
    async fn intern_tuple(
        &self,
        program: Program<'db>,
        spec: TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>>;
    async fn string_literal(&self, value: &str) -> RunResult<Type<'db>>;
    async fn union_from_two_elements(
        &self,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Type<'db>>;
    async fn intersection_from_two_elements(
        &self,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Type<'db>>;
    async fn is_redundant_with(&self, first: Type<'db>, second: Type<'db>) -> RunResult<bool> {
        relations::is_redundant_with(self, first, second).await
    }
    async fn canonical_redundancy(&self, first: Type<'db>, second: Type<'db>) -> RunResult<bool>;
    /// Returns the cached owned lazy-assignability conditions for this ordered type pair.
    async fn canonical_owned_assignability(
        &self,
        program: Program<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<&'db crate::types::constraints::OwnedConstraintSet<'db>>;
    async fn cached_materialization(
        &self,
        program: Program<'db>,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> RunResult<Type<'db>>;
    async fn intern_typeform(&self, argument: Type<'db>) -> RunResult<Type<'db>>;
    /// Interns the supplied flags and ordered field specifiers in the canonical metadata table.
    async fn intern_dataclass_transformer_params(
        &self,
        flags: DataclassTransformerFlags,
        field_specifiers: Box<[Type<'db>]>,
    ) -> RunResult<DataclassTransformerParams<'db>>;
    async fn simplify_intersection_pair(
        &self,
        first: Type<'db>,
        second: Type<'db>,
        polarity: IntersectionPolarity,
    ) -> RunResult<IntersectionSimplification> {
        type_algebra::simplify_intersection_pair(self, first, second, polarity).await
    }
    async fn canonical_intersection_simplification(
        &self,
        first: Type<'db>,
        second: Type<'db>,
        polarity: IntersectionPolarity,
    ) -> RunResult<IntersectionSimplification>;
    async fn intern_union(
        &self,
        elements: Box<[Type<'db>]>,
        recursively_defined: RecursivelyDefined,
    ) -> RunResult<UnionType<'db>>;
    async fn intern_intersection(
        &self,
        positive: FxOrderSet<Type<'db>>,
        negative: NegativeIntersectionElements<'db>,
    ) -> RunResult<IntersectionType<'db>>;
    async fn non_terminal_call(&self, call: CallableAndCallExpr<'db>) -> RunResult<Truthiness>;
    async fn unavailable<T>(&self, operation: SourceOperation) -> RunResult<T>;
    async fn prepare_existing(&self, file: ProgramFile<'db>) -> RunResult<PreparedSource<'db>>;
    async fn prepare_file(
        &self,
        file: File,
        program: Program<'db>,
    ) -> RunResult<PreparedSource<'db>>;
    async fn resolve_module(
        &self,
        program: Program<'db>,
        name: &ModuleName,
        importing_file: Option<File>,
    ) -> RunResult<Option<Module<'db>>>;
    async fn module_literal(
        &self,
        module: Module<'db>,
        importing_file: Option<ProgramFile<'db>>,
    ) -> RunResult<ModuleLiteralType<'db>>;
}

pub(in crate::types::infer) struct SourceEffects<'access, 'run, 'db: 'run, A> {
    pub(in crate::types::infer::builder) access: &'access A,
    program: Program<'db>,
    lifetime: std::marker::PhantomData<&'run ()>,
}

impl<'access, 'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'access, 'run, 'db, A> {
    pub(in crate::types::infer) fn new(access: &'access A, program: Program<'db>) -> Self {
        Self {
            access,
            program,
            lifetime: std::marker::PhantomData,
        }
    }

    fn db(&self) -> &'db dyn Db {
        self.access.db()
    }

    fn check_program(&self, program: Program<'db>) -> RunResult<()> {
        if program == self.program {
            Ok(())
        } else {
            Err(RunError::Contract("source program is foreign"))
        }
    }

    pub(in crate::types::infer) async fn check_file_program(
        &self,
        file: ProgramFile<'db>,
    ) -> RunResult<()> {
        let request = file
            .read_fields(self.access.endpoint().field_request_context())
            .program();
        self.check_program(self.field_with_profile(request, &FixedFieldCopy).await?)
    }

    pub(in crate::types::infer::builder) async fn local<T>(
        &self,
        work: usize,
        requested_bytes: usize,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.local_quoted(Ok((work, requested_bytes)), action).await
    }

    pub(in crate::types::infer) async fn local_quoted<T>(
        &self,
        quote: RunResult<(usize, usize)>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let endpoint = self.access.endpoint();
        // Keep captured owners here while the driver drains the current invocation's pending
        // child tasks. Moving the factory into the callback would destroy those owners as soon
        // as an admission returns an error.
        let mut action = Some(action);
        Ok(endpoint
            .local_call(|| {
                let (work, requested_bytes) = quote?;
                endpoint.admit_work(work)?;
                if requested_bytes != 0 {
                    endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
                }
                endpoint.check_completion()?;
                let action = action
                    .take()
                    .ok_or(RunError::Contract("local factory was consumed"))?;
                Ok(action())
            })
            .await)
    }

    pub(in crate::types::infer::builder) async fn work(&self, units: usize) -> RunResult<()> {
        self.local(units, 0, || ()).await
    }

    pub(in crate::types::infer::builder) async fn string_literal_value(
        &self,
        value: &str,
    ) -> RunResult<Type<'db>> {
        self.access.string_literal(value).await
    }

    pub(in crate::types::infer::builder) async fn property_instance_value(
        &self,
        getter: Option<Type<'db>>,
        setter: Option<Type<'db>>,
        deleter: Option<Type<'db>>,
    ) -> RunResult<PropertyInstanceType<'db>> {
        self.access
            .intern_property(
                getter,
                setter,
                deleter,
                PropertyInstanceClass::Builtin,
                PropertyAccessorDefinitions::default(),
            )
            .await
    }

    pub(in crate::types::infer::builder) async fn field<R: FieldRequest<'db>>(
        &self,
        request: R,
    ) -> RunResult<R::Output> {
        Ok(self
            .access
            .endpoint()
            .read_field(request, &BorrowOrCopy)
            .await)
    }

    pub(in crate::types::infer::builder) async fn field_with_profile<
        R: FieldRequest<'db>,
        P: FieldReadProfile<R::Stored>,
    >(
        &self,
        request: R,
        profile: &P,
    ) -> RunResult<R::Output> {
        Ok(self.access.endpoint().read_field(request, profile).await)
    }

    /// Admits construction of a fixed representation stored inline in its caller.
    ///
    /// Each construction charges one work unit and `size_of::<T>()` requested bytes before
    /// invoking `make`, including when the caller reuses its storage. These bytes cover fixed
    /// representation initialization and copying. Additional factory computation, payload
    /// allocation and dynamic cleanup require separate admissions.
    pub(in crate::types::infer) async fn initialize_value<T>(
        &self,
        make: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.local(1, size_of::<T>(), make).await
    }

    /// Admits and boxes a continuation without storing it inline in the enclosing future.
    ///
    /// Charges one work unit and `size_of::<F>()` cumulative requested bytes before invoking
    /// `make`. The byte charge covers initialization and copying of the fixed future representation.
    /// Additional factory computation, transitive allocations, polling, and dynamic cleanup need
    /// separate admissions; the size of `F` does not bound those costs.
    pub(in crate::types::infer) async fn allocate_future<F: Future>(
        &self,
        make: impl FnOnce() -> F,
    ) -> RunResult<Pin<Box<F>>> {
        let bytes = size_of::<F>();
        self.local(1, bytes, || Box::pin(make())).await
    }

    pub(in crate::types::infer::builder) async fn unavailable<T>(
        &self,
        operation: SourceOperation,
    ) -> RunResult<T> {
        self.access.unavailable(operation).await
    }

    pub(in crate::types::infer::builder) fn checked(value: Option<usize>) -> RunResult<usize> {
        value.ok_or(RunError::Contract("source work quotation overflow"))
    }

    pub(in crate::types::infer::builder) async fn canonical_expression(
        &self,
        expression: Expression<'db>,
        context: TypeContext<'db>,
    ) -> RunResult<&'db ExpressionInference<'db>> {
        self.check_file_program(self.expression_file(expression).await?)
            .await?;
        self.access.expression(expression, context).await
    }

    pub(in crate::types::infer::builder) async fn function_signature(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        self.check_file_program(self.function_file(function).await?)
            .await?;
        self.access.function_signature(function).await
    }

    pub(in crate::types::infer) async fn infer_expression(
        &self,
        expression: Expression<'db>,
        context: TypeContext<'db>,
    ) -> RunResult<ExpressionInference<'db>> {
        let db = self.db();
        let scope = self.expression_scope(expression).await?;
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let source = self.access.prepare_existing(file).await?;
        self.check_file_program(source.file).await?;
        if source.file != file {
            return Err(RunError::Contract("prepared expression file is foreign"));
        }
        let env = ProgramEnvironment::from_file(source.file);
        let mut owner = self
            .empty_builder(
                &source,
                &env,
                InferenceRegion::Expression(expression, context),
            )
            .await?;
        let builder = &mut owner.builder;
        self.setup_dataclass_field_specifiers(builder, SourceOperation::DataclassFieldSpecifiers)
            .await?;
        let kind = self.field(expression.read_fields(db).kind()).await?;
        self.local(2, 0, || builder.setup_expression_region_flags(kind))
            .await?;
        let node_ref = self.field(expression.read_fields(db).node_ref()).await?;
        let node = node_ref.node(&source.module);
        match kind {
            ExpressionKind::TypeExpression => {
                local::source::type_expression(builder, node, TypeExpressionMode::Scoped, self)
                    .await?;
            }
            ExpressionKind::Normal | ExpressionKind::Callee => {
                local::source::expression(builder, node, context, self).await?;
            }
        }
        let quote = self.expression_finalization_quote(&owner.builder).await?;
        // Keep the owner outside the admission closure so refusal cannot retire it before drain.
        let mut owner = Some(owner);
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(quote.work)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: quote.bytes,
                })?;
                endpoint.check_completion()?;
                let owner = owner
                    .take()
                    .ok_or(RunError::Contract("expression owner already consumed"))?;
                Ok(owner.builder.into_expression_inference())
            })
            .await)
    }

    pub(in crate::types::infer) async fn infer_definition(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<DefinitionInference<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        #[cfg(test)]
        let _lifetime = crate::types::infer::source_runtime::tests::function_decorator_ingestion::OwnerLifetime::new(definition);
        let source = self.access.prepare_existing(file).await?;
        self.check_file_program(source.file).await?;
        if source.file != file {
            return Err(RunError::Contract("prepared definition file is foreign"));
        }
        let env = ProgramEnvironment::from_file(source.file);
        let mut owner = self
            .empty_builder(&source, &env, InferenceRegion::Definition(definition))
            .await?;
        owner
            .builder
            .infer_region_definition_with(self, definition)
            .await?;
        self.finish_definition_owner(owner, definition).await
    }

    async fn finish_definition_owner(
        &self,
        owner: UnpublishedBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> RunResult<DefinitionInference<'db>> {
        let builder = &owner.builder;
        // The common builder quotation funds map freezing, vector compaction and discarded caches.
        // The additional 64 units cover definition compaction's fixed empty/singleton checks
        // and representation dispatch.
        let storage = self.scope_finalization_quote(builder).await?;
        let entries = builder.expressions.len();
        let bindings = builder.bindings.0.len();
        let declarations = builder.declarations.0.len();
        let deferred = builder.deferred.0.len();
        let count = Self::checked(
            entries
                .checked_add(bindings)
                .and_then(|n| n.checked_add(declarations))
                .and_then(|n| n.checked_add(deferred)),
        )?;
        let work = Self::checked(storage.work.checked_add(64))?;
        let bytes = Self::checked(
            count
                .checked_add(1)
                .and_then(|n| n.checked_mul(8))
                .and_then(|n| n.checked_mul(size_of::<(Definition<'db>, Type<'db>)>()))
                .and_then(|n| {
                    n.checked_add(size_of::<crate::types::infer::DefinitionInferenceExtra<'db>>())
                })
                .and_then(|n| {
                    n.checked_add(size_of::<
                        crate::types::infer::OtherDefinitionInferenceExtra<'db>,
                    >())
                })
                .and_then(|n| n.checked_add(storage.bytes)),
        )?;
        // Payload-producing effects admit their owners when they grow. The original finalizer
        // chooses its compact representation and takes the owner only after admission succeeds.
        let mut owner = Some(owner);
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(work)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: bytes,
                })?;
                endpoint.check_completion()?;
                let owner = owner
                    .take()
                    .ok_or(RunError::Contract("definition owner already consumed"))?;
                Ok(owner.builder.finish_inferred_definition(definition))
            })
            .await)
    }

    async fn empty_builder<'ast>(
        &self,
        source: &'ast PreparedSource<'db>,
        env: &'ast ProgramEnvironment<'db>,
        region: InferenceRegion<'db>,
    ) -> RunResult<UnpublishedBuilder<'db, 'ast>> {
        let scope = match region {
            InferenceRegion::Scope(scope, _) => scope,
            InferenceRegion::Expression(expression, _) => self.expression_scope(expression).await?,
            InferenceRegion::Definition(definition)
            | InferenceRegion::Deferred(definition)
            | InferenceRegion::FunctionDecorators(definition)
            | InferenceRegion::FunctionDefaults(definition) => {
                self.definition_scope(definition).await?
            }
            InferenceRegion::Statement(_) => {
                return self.unavailable(SourceOperation::StatementQuery).await;
            }
        };
        let file = self.scope_file(scope).await?;
        if file != source.file {
            return Err(RunError::Contract(
                "builder scope belongs to another prepared file",
            ));
        }
        let physical_file = self.context_file(file, env).await?;
        self.initialize_value(|| {
            let context = InferContext::new_with_validated_source(
                self.db(),
                env,
                scope,
                physical_file,
                file,
                &source.module,
            );
            let mut builder = TypeInferenceBuilder::from_context(context, region, source.index);
            // This owner starts with empty diagnostics and accepts responsibility for discard.
            // Later payload-producing effects remain unavailable until they admit their cleanup.
            builder.context.defuse();
            #[cfg(test)]
            let lifetime = observations::BuilderLifetime::new(self.db(), region);
            UnpublishedBuilder {
                builder,
                #[cfg(test)]
                _lifetime: lifetime,
            }
        })
        .await
    }

    #[cfg(test)]
    pub(in crate::types::infer) async fn assigned_place_for_test<'ast>(
        &self,
        source: &'ast PreparedSource<'db>,
        env: &'ast ProgramEnvironment<'db>,
        expression: Expression<'db>,
        node: ast::ExprRef<'ast>,
        place: PlaceExpr,
    ) -> RunResult<(PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>)> {
        let owner = self
            .empty_builder(
                source,
                env,
                InferenceRegion::Expression(expression, TypeContext::default()),
            )
            .await?;
        owner.builder.infer_place_load_with(self, place, node).await
    }

    #[cfg(test)]
    pub(in crate::types::infer) async fn applicable_constraints_for_test<'ast>(
        &self,
        source: &'ast PreparedSource<'db>,
        env: &'ast ProgramEnvironment<'db>,
        expression: Expression<'db>,
        node: ast::ExprRef<'ast>,
        ty: Type<'db>,
        constraints: &[(FileScopeId, ConstraintKey)],
    ) -> RunResult<Type<'db>> {
        let owner = self
            .empty_builder(
                source,
                env,
                InferenceRegion::Expression(expression, TypeContext::default()),
            )
            .await?;
        narrow_expr_with_applicable_constraints_with(
            &owner.builder,
            node,
            ty,
            constraints,
            ApplicableConstraintsFacts,
            self,
        )
        .await
    }
}

#[cfg(test)]
impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(in crate::types::infer) fn ordinary_assigned_place_for_test(
        mut self,
        node: ast::ExprRef<'_>,
        place: PlaceExpr,
    ) -> (PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>) {
        self.context.defuse();
        self.infer_place_load(place, node)
    }

    pub(in crate::types::infer) fn ordinary_applicable_constraints_for_test(
        mut self,
        node: ast::ExprRef<'_>,
        ty: Type<'db>,
        constraints: &[(FileScopeId, ConstraintKey)],
    ) -> Type<'db> {
        self.context.defuse();
        self.narrow_expr_with_applicable_constraints(node, ty, constraints)
    }
}

struct UnpublishedBuilder<'db, 'ast> {
    builder: TypeInferenceBuilder<'db, 'ast>,
    #[cfg(test)]
    _lifetime: observations::BuilderLifetime,
}

#[cfg(test)]
pub(in crate::types::infer) mod observations {
    use salsa::plumbing::AsId;

    use crate::Db;
    use crate::reachability::ReachabilityCacheKey;
    use crate::types::call::{Bindings, CallArguments};
    use crate::types::infer::InferenceRegion;
    use std::cell::{Cell, RefCell};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::types::infer) enum Event {
        Created,
        Stored,
        DefinitionStored,
        AnnotationCompleted,
        AnnotatedDefinitionStored,
        BindingStored,
        PlaceStep,
        PlaceResolutionStep,
        PlacePrefixesReady,
        TupleShapeReady,
        ComparisonRetained,
        ExpressionMerged,
        FunctionSignatureReady,
        ArgumentsReady,
        ArgumentsChecked,
        CallStored,
        FileScopeMerged,
        OverrideMemberRetained,
        OverrideMemberLookupReady,
        ReachabilityAllocationBefore,
        ReachabilityAllocationAdmitted,
        ReachabilityCacheReady,
        ReachabilityCacheStored,
    }

    #[derive(Clone, Copy, Debug)]
    pub(in crate::types::infer) struct ReachabilityCacheObservation {
        pub key: ReachabilityCacheKey,
        pub storage: (usize, usize, usize, usize),
        pub remaining: Option<usize>,
    }

    type ArgumentsObserver = for<'db> fn(&'db dyn Db, &CallArguments<'_, 'db>, &Bindings<'db>);

    thread_local! {
        static LIVE: Cell<usize> = const { Cell::new(0) };
        static CREATED: Cell<usize> = const { Cell::new(0) };
        static STORED: Cell<usize> = const { Cell::new(0) };
        static STORED_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static DEFINITION_STORED_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static ANNOTATION_COMPLETED: Cell<usize> = const { Cell::new(0) };
        static FIRST_ANNOTATION_COMPLETED_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static ANNOTATED_DEFINITION_STORED: Cell<usize> = const { Cell::new(0) };
        static FIRST_ANNOTATED_DEFINITION_STORED_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static BINDING_STORED_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static PLACE_STEPS: Cell<usize> = const { Cell::new(0) };
        static FIRST_PLACE_STEP_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static PLACE_RESOLUTION_STEPS: Cell<usize> = const { Cell::new(0) };
        static FIRST_PLACE_RESOLUTION_STEP_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static PLACE_PREFIXES_READY: Cell<usize> = const { Cell::new(0) };
        static FIRST_PLACE_PREFIXES_READY_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static TUPLE_SHAPES_READY: Cell<usize> = const { Cell::new(0) };
        static FIRST_TUPLE_SHAPE_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static COMPARISONS_RETAINED: Cell<usize> = const { Cell::new(0) };
        static FIRST_COMPARISON_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static MERGED_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static MERGED_DIAGNOSTICS: Cell<usize> = const { Cell::new(0) };
        static SIGNATURE_READY: Cell<usize> = const { Cell::new(0) };
        static SIGNATURE_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static ARGUMENTS_READY: Cell<usize> = const { Cell::new(0) };
        static ARGUMENTS_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static ARGUMENTS_OBSERVER: Cell<Option<ArgumentsObserver>> = const { Cell::new(None) };
        static CHECKED_OBSERVER: Cell<Option<ArgumentsObserver>> = const { Cell::new(None) };
        static FILE_SCOPE_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static CALL_STORED: Cell<usize> = const { Cell::new(0) };
        static CALL_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static CANCEL: Cell<Option<Event>> = const { Cell::new(None) };
        static CANCEL_DEFINITION: Cell<Option<salsa::Id>> = const { Cell::new(None) };
        static OVERRIDE_MEMBERS: RefCell<Vec<(String, usize)>> = const { RefCell::new(Vec::new()) };
        static OVERRIDE_MEMBER_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static OVERRIDE_MEMBER_LOOKUPS: Cell<usize> = const { Cell::new(0) };
        static FIRST_OVERRIDE_MEMBER_LOOKUP_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static REACHABILITY_ALLOCATION_BEFORE: Cell<usize> = const { Cell::new(0) };
        static REACHABILITY_ALLOCATION_ADMITTED: Cell<usize> = const { Cell::new(0) };
        static FIRST_REACHABILITY_ALLOCATION_REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
        static REACHABILITY_CACHE_READY: Cell<Option<ReachabilityCacheObservation>> = const { Cell::new(None) };
        static REACHABILITY_CACHE_STORED: Cell<Option<ReachabilityCacheObservation>> = const { Cell::new(None) };
        static REACHABILITY_CACHE_INSERTIONS: Cell<usize> = const { Cell::new(0) };
    }

    pub(in crate::types::infer) fn reset(cancel: Option<Event>) {
        assert_eq!(LIVE.get(), 0);
        CREATED.set(0);
        STORED.set(0);
        STORED_REMAINING.set(None);
        DEFINITION_STORED_REMAINING.set(None);
        ANNOTATION_COMPLETED.set(0);
        FIRST_ANNOTATION_COMPLETED_REMAINING.set(None);
        ANNOTATED_DEFINITION_STORED.set(0);
        FIRST_ANNOTATED_DEFINITION_STORED_REMAINING.set(None);
        BINDING_STORED_REMAINING.set(None);
        PLACE_STEPS.set(0);
        FIRST_PLACE_STEP_REMAINING.set(None);
        PLACE_RESOLUTION_STEPS.set(0);
        FIRST_PLACE_RESOLUTION_STEP_REMAINING.set(None);
        PLACE_PREFIXES_READY.set(0);
        FIRST_PLACE_PREFIXES_READY_REMAINING.set(None);
        TUPLE_SHAPES_READY.set(0);
        FIRST_TUPLE_SHAPE_REMAINING.set(None);
        COMPARISONS_RETAINED.set(0);
        FIRST_COMPARISON_REMAINING.set(None);
        MERGED_REMAINING.set(None);
        MERGED_DIAGNOSTICS.set(0);
        SIGNATURE_READY.set(0);
        SIGNATURE_REMAINING.set(None);
        ARGUMENTS_READY.set(0);
        ARGUMENTS_REMAINING.set(None);
        ARGUMENTS_OBSERVER.set(None);
        CHECKED_OBSERVER.set(None);
        CALL_STORED.set(0);
        FILE_SCOPE_REMAINING.set(None);
        CALL_REMAINING.set(None);
        CANCEL.set(cancel);
        CANCEL_DEFINITION.set(None);
        OVERRIDE_MEMBERS.with_borrow_mut(Vec::clear);
        OVERRIDE_MEMBER_REMAINING.set(None);
        OVERRIDE_MEMBER_LOOKUPS.set(0);
        FIRST_OVERRIDE_MEMBER_LOOKUP_REMAINING.set(None);
        REACHABILITY_ALLOCATION_BEFORE.set(0);
        REACHABILITY_ALLOCATION_ADMITTED.set(0);
        FIRST_REACHABILITY_ALLOCATION_REMAINING.set(None);
        REACHABILITY_CACHE_READY.set(None);
        REACHABILITY_CACHE_STORED.set(None);
        REACHABILITY_CACHE_INSERTIONS.set(0);
    }

    pub(in crate::types::infer) fn counts() -> (usize, usize, usize) {
        (LIVE.get(), CREATED.get(), STORED.get())
    }

    pub(in crate::types::infer) fn reachability_allocation() -> (usize, usize, Option<usize>) {
        (
            REACHABILITY_ALLOCATION_BEFORE.get(),
            REACHABILITY_ALLOCATION_ADMITTED.get(),
            FIRST_REACHABILITY_ALLOCATION_REMAINING.get(),
        )
    }

    pub(in crate::types::infer) fn comparison_progress() -> (usize, Option<usize>) {
        (COMPARISONS_RETAINED.get(), FIRST_COMPARISON_REMAINING.get())
    }

    pub(in crate::types::infer) fn cache_ready() -> Option<ReachabilityCacheObservation> {
        REACHABILITY_CACHE_READY.get()
    }

    pub(in crate::types::infer) fn cache_stored() -> (usize, Option<ReachabilityCacheObservation>) {
        (
            REACHABILITY_CACHE_INSERTIONS.get(),
            REACHABILITY_CACHE_STORED.get(),
        )
    }

    pub(super) fn reachability_cache_ready(
        db: &dyn Db,
        key: ReachabilityCacheKey,
        storage: (usize, usize, usize, usize),
    ) {
        if REACHABILITY_CACHE_READY.get().is_none() {
            REACHABILITY_CACHE_READY.set(Some(ReachabilityCacheObservation {
                key,
                storage,
                remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
            }));
        }
        observe(db, Event::ReachabilityCacheReady);
    }

    pub(super) fn reachability_cache_stored(
        db: &dyn Db,
        key: ReachabilityCacheKey,
        storage: (usize, usize, usize, usize),
    ) {
        REACHABILITY_CACHE_INSERTIONS.set(REACHABILITY_CACHE_INSERTIONS.get() + 1);
        if REACHABILITY_CACHE_STORED.get().is_none() {
            REACHABILITY_CACHE_STORED.set(Some(ReachabilityCacheObservation {
                key,
                storage,
                remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
            }));
        }
        observe(db, Event::ReachabilityCacheStored);
    }

    pub(in crate::types::infer) fn stored_remaining() -> Option<usize> {
        STORED_REMAINING.get()
    }

    pub(in crate::types::infer) fn definition_stored_remaining() -> Option<usize> {
        DEFINITION_STORED_REMAINING.get()
    }

    pub(in crate::types::infer) fn annotation_completed() -> (usize, Option<usize>) {
        (
            ANNOTATION_COMPLETED.get(),
            FIRST_ANNOTATION_COMPLETED_REMAINING.get(),
        )
    }

    pub(in crate::types::infer) fn annotated_definition_stored() -> (usize, Option<usize>) {
        (
            ANNOTATED_DEFINITION_STORED.get(),
            FIRST_ANNOTATED_DEFINITION_STORED_REMAINING.get(),
        )
    }

    pub(in crate::types::infer) fn binding_stored_remaining() -> Option<usize> {
        BINDING_STORED_REMAINING.get()
    }

    pub(in crate::types::infer) fn place_progress() -> (usize, Option<usize>) {
        (PLACE_STEPS.get(), FIRST_PLACE_STEP_REMAINING.get())
    }

    pub(in crate::types::infer) fn place_resolution_progress() -> (usize, Option<usize>) {
        (
            PLACE_RESOLUTION_STEPS.get(),
            FIRST_PLACE_RESOLUTION_STEP_REMAINING.get(),
        )
    }

    pub(in crate::types::infer) fn place_prefix_progress() -> (usize, Option<usize>) {
        (
            PLACE_PREFIXES_READY.get(),
            FIRST_PLACE_PREFIXES_READY_REMAINING.get(),
        )
    }

    pub(in crate::types::infer) fn merged_remaining() -> Option<usize> {
        MERGED_REMAINING.get()
    }

    pub(in crate::types::infer) fn tuple_shape_progress() -> (usize, Option<usize>) {
        (TUPLE_SHAPES_READY.get(), FIRST_TUPLE_SHAPE_REMAINING.get())
    }

    pub(in crate::types::infer) fn merged_diagnostics() -> usize {
        MERGED_DIAGNOSTICS.get()
    }

    pub(in crate::types::infer) fn signature_ready() -> (usize, Option<usize>) {
        (SIGNATURE_READY.get(), SIGNATURE_REMAINING.get())
    }

    pub(in crate::types::infer) fn argument_progress() -> (usize, Option<usize>) {
        (ARGUMENTS_READY.get(), ARGUMENTS_REMAINING.get())
    }

    pub(in crate::types::infer) fn set_arguments_observer(observer: ArgumentsObserver) {
        ARGUMENTS_OBSERVER.set(Some(observer));
    }

    pub(in crate::types::infer) fn arguments_ready<'db>(
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
        bindings: &Bindings<'db>,
    ) {
        if let Some(observer) = ARGUMENTS_OBSERVER.get() {
            observer(db, arguments, bindings);
        }
        observe(db, Event::ArgumentsReady);
    }

    pub(in crate::types::infer) fn set_checked_observer(observer: ArgumentsObserver) {
        CHECKED_OBSERVER.set(Some(observer));
    }

    pub(in crate::types::infer) fn arguments_checked<'db>(
        db: &'db dyn Db,
        arguments: &CallArguments<'_, 'db>,
        bindings: &Bindings<'db>,
    ) {
        if let Some(observer) = CHECKED_OBSERVER.get() {
            observer(db, arguments, bindings);
        }
        observe(db, Event::ArgumentsChecked);
    }

    pub(in crate::types::infer) fn file_scope_remaining() -> Option<usize> {
        FILE_SCOPE_REMAINING.get()
    }

    pub(in crate::types::infer) fn override_members() -> (Vec<(String, usize)>, Option<usize>) {
        (
            OVERRIDE_MEMBERS.with_borrow(Clone::clone),
            OVERRIDE_MEMBER_REMAINING.get(),
        )
    }

    pub(super) fn override_member_retained(db: &dyn Db, name: &str, retained: usize) {
        OVERRIDE_MEMBERS.with_borrow_mut(|members| members.push((name.to_owned(), retained)));
        observe(db, Event::OverrideMemberRetained);
    }

    pub(in crate::types::infer) fn override_member_lookup_progress() -> (usize, Option<usize>) {
        (
            OVERRIDE_MEMBER_LOOKUPS.get(),
            FIRST_OVERRIDE_MEMBER_LOOKUP_REMAINING.get(),
        )
    }

    pub(in crate::types::infer) fn call_progress() -> (usize, Option<usize>) {
        (CALL_STORED.get(), CALL_REMAINING.get())
    }

    pub(super) fn observe_merge(db: &dyn Db, diagnostics: usize) {
        MERGED_REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
            db,
        ));
        MERGED_DIAGNOSTICS.set(diagnostics);
        observe(db, Event::ExpressionMerged);
    }

    pub(super) fn observe(db: &dyn Db, event: Event) {
        if matches!(event, Event::DefinitionStored) {
            DEFINITION_STORED_REMAINING.set(
                salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
            );
        }
        match event {
            Event::Created => CREATED.set(CREATED.get() + 1),
            Event::ReachabilityAllocationBefore => {
                REACHABILITY_ALLOCATION_BEFORE.set(REACHABILITY_ALLOCATION_BEFORE.get() + 1);
            }
            Event::ReachabilityAllocationAdmitted => {
                REACHABILITY_ALLOCATION_ADMITTED.set(REACHABILITY_ALLOCATION_ADMITTED.get() + 1);
                if REACHABILITY_ALLOCATION_ADMITTED.get() == 1 {
                    FIRST_REACHABILITY_ALLOCATION_REMAINING.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                    );
                }
            }
            Event::Stored | Event::DefinitionStored => {
                STORED.set(STORED.get() + 1);
                STORED_REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                    db,
                ));
            }
            Event::AnnotationCompleted => {
                ANNOTATION_COMPLETED.set(ANNOTATION_COMPLETED.get() + 1);
                if ANNOTATION_COMPLETED.get() == 1 {
                    FIRST_ANNOTATION_COMPLETED_REMAINING.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                    );
                }
            }
            Event::AnnotatedDefinitionStored => {
                ANNOTATED_DEFINITION_STORED.set(ANNOTATED_DEFINITION_STORED.get() + 1);
                if ANNOTATED_DEFINITION_STORED.get() == 1 {
                    FIRST_ANNOTATED_DEFINITION_STORED_REMAINING.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                    );
                }
            }
            Event::FileScopeMerged => {
                FILE_SCOPE_REMAINING.set(
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                );
            }
            Event::ExpressionMerged
            | Event::ReachabilityCacheReady
            | Event::ReachabilityCacheStored => {}
            Event::BindingStored => {
                BINDING_STORED_REMAINING.set(
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                );
            }
            Event::PlaceStep => {
                PLACE_STEPS.set(PLACE_STEPS.get() + 1);
                if FIRST_PLACE_STEP_REMAINING.get().is_none() {
                    FIRST_PLACE_STEP_REMAINING.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                    );
                }
            }
            Event::PlaceResolutionStep => {
                PLACE_RESOLUTION_STEPS.set(PLACE_RESOLUTION_STEPS.get() + 1);
                if FIRST_PLACE_RESOLUTION_STEP_REMAINING.get().is_none() {
                    FIRST_PLACE_RESOLUTION_STEP_REMAINING.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                    );
                }
            }
            Event::PlacePrefixesReady => {
                PLACE_PREFIXES_READY.set(PLACE_PREFIXES_READY.get() + 1);
                if FIRST_PLACE_PREFIXES_READY_REMAINING.get().is_none() {
                    FIRST_PLACE_PREFIXES_READY_REMAINING.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                    );
                }
            }
            Event::OverrideMemberRetained => {
                OVERRIDE_MEMBER_REMAINING.set(
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                );
            }
            Event::OverrideMemberLookupReady => {
                OVERRIDE_MEMBER_LOOKUPS.set(OVERRIDE_MEMBER_LOOKUPS.get() + 1);
                if FIRST_OVERRIDE_MEMBER_LOOKUP_REMAINING.get().is_none() {
                    FIRST_OVERRIDE_MEMBER_LOOKUP_REMAINING.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                    );
                }
            }
            Event::TupleShapeReady => {
                TUPLE_SHAPES_READY.set(TUPLE_SHAPES_READY.get() + 1);
                if FIRST_TUPLE_SHAPE_REMAINING.get().is_none() {
                    FIRST_TUPLE_SHAPE_REMAINING.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                    );
                }
            }
            Event::ComparisonRetained => {
                COMPARISONS_RETAINED.set(COMPARISONS_RETAINED.get() + 1);
                if FIRST_COMPARISON_REMAINING.get().is_none() {
                    FIRST_COMPARISON_REMAINING.set(
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
                    );
                }
            }
            Event::ArgumentsChecked => {}
            Event::CallStored => {
                CALL_STORED.set(CALL_STORED.get() + 1);
                CALL_REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                    db,
                ));
            }
            Event::FunctionSignatureReady => {
                SIGNATURE_READY.set(SIGNATURE_READY.get() + 1);
                SIGNATURE_REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                    db,
                ));
            }
            Event::ArgumentsReady => {
                ARGUMENTS_READY.set(ARGUMENTS_READY.get() + 1);
                ARGUMENTS_REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                    db,
                ));
            }
        }
        if CANCEL.get() == Some(event) {
            CANCEL.set(None);
            db.cancellation_token().cancel();
        }
    }

    pub(in crate::types::infer) fn cancel_definition_creation(definition: salsa::Id) {
        CANCEL_DEFINITION.set(Some(definition));
    }

    pub(super) struct BuilderLifetime {
        definition: Option<salsa::Id>,
    }
    impl BuilderLifetime {
        pub(super) fn new(db: &dyn Db, region: InferenceRegion<'_>) -> Self {
            LIVE.set(LIVE.get() + 1);
            let definition = match region {
                InferenceRegion::Definition(definition) => Some(definition.as_id()),
                _ => None,
            };
            let guard = Self { definition };
            observe(db, Event::Created);
            if definition.is_some() && CANCEL_DEFINITION.get() == definition {
                CANCEL_DEFINITION.set(None);
                db.cancellation_token().cancel();
            }
            guard
        }
    }
    impl Drop for BuilderLifetime {
        fn drop(&mut self) {
            LIVE.set(LIVE.get() - 1);
            crate::types::cyclic::guard_storage::observations::source_child_dropped(
                self.definition,
            );
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DefinitionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn definition_kind(
        &self,
        _db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionKind<'db>> {
        let fields = self.access.endpoint().field_request_context();
        self.field(definition.read_fields(fields).kind()).await
    }

    async fn assignment<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &AssignmentDefinitionKind<'db>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.allocate_future(|| {
            builder.infer_assignment_definition_with(self, assignment, definition)
        })
        .await?
        .await
    }
    async fn annotated_assignment<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        assignment: &'db AnnotatedAssignmentDefinitionKind,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.allocate_future(|| {
            infer_annotated_assignment_definition_with(builder, assignment, definition, self)
        })
        .await?
        .await
    }
    async fn parameter<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter: &'ast ast::ParameterWithDefault,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.infer_parameter_source(builder, parameter, definition)
            .await
    }
    async fn type_parameter(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        node: super::super::typevar::pep695::TypeParameterDefinitionNode<'_>,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.infer_type_parameter_source(builder, node, definition).await
    }
    async fn function(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        function: &ast::StmtFunctionDef,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        builder
            .infer_function_definition_with(self, function, definition)
            .await
    }
    async fn class(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        class: &ast::StmtClassDef,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        self.allocate_future(|| builder.infer_class_definition_with(self, class, definition))
            .await?
            .await
    }
    async fn import<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        builder
            .infer_import_definition_with(self, alias, definition)
            .await
    }
    async fn import_from<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        import: &ast::StmtImportFrom,
        alias: &'ast ast::Alias,
        definition: Definition<'db>,
    ) -> RunResult<()> {
        builder
            .infer_import_from_definition_with(self, import, alias, definition)
            .await
    }
    async fn legacy_operation<'ast>(
        &self,
        effect: SourceDefinitionEffect,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _body: impl FnOnce(&mut TypeInferenceBuilder<'db, 'ast>),
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Definition(effect)).await
    }
}
