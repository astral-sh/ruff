//! Explicit, experimental semantic execution with one budget per request.
//!
//! Structural preparation establishes the source identity before semantic execution. Its work and
//! allocations are outside these limits. An incomplete result contains no candidate expression type.

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::rc::Rc;

use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::File;
use ruff_db::parsed::{ParsedModule, ParsedModuleRef, parsed_module};
use ruff_db::source::{SourceText, source_text};
use rustc_hash::FxHashMap;
use salsa::attempt_probe::{AttemptOutcome, Incomplete, StartError, report_incomplete};
use salsa::execution_probe::{
    BorrowOrCopy, ExecutionBudget, ExecutionLimits, ExecutionUsage, ExecutionWork,
    PreparedSourceMemo, RunError, RunResult, TaskEndpoint, try_with_metered_execution_budget,
    with_explicit_reads,
};
pub use salsa::prepared_source_probe::PreparationError;
use salsa::prepared_source_probe::{Stamp, try_with_preparation};
use ty_module_resolver::{
    ImportingFile, KnownModule, Module, ModuleName,
    PreparedModuleResolution as ResolverPreparedModuleResolution, file_to_module, resolve_module,
    resolve_module_confident,
};
use ty_python_core::program::Program;
use ty_python_core::scope::ScopeId;
use ty_python_core::{ExpressionNodeKey, ProgramFile, SemanticIndex, global_scope, semantic_index};

use crate::Db;
use crate::prepared_host::PreparedHostFileReads;
use crate::suppression::{Suppressions, suppressions};
pub use crate::types::{
    AnnotatedAssignmentOperation, AttributeOperation, CallableConversionOperation,
    BoundMethodPreparationOperation, CallableGuardOperation, ChainedComparisonOperation,
    ConstructorSignatureOperation, ConstructorStorageOperation, DecoratorApplicationOperation,
    DefaultSpecializationOperation, DescriptorOperation, EqualityOperation, GeneralMemberOperation,
    KnownClassInstanceOperation, LegacyTypeVarOperation, MappingOperation,
    MaterializationOperation, RecursiveNormalizationOperation, RelationOperation, SearchOperation,
    SourceDefinitionEffect, SourceExpressionOperation, TruthinessOperation, TupleSpecOperation,
    TypeComparisonOperation, TypeConversionOperation,
};
use crate::types::local_transfer::local_with_fixed_transfers_at;
use crate::types::{Type, run_source_expression, run_source_file};

#[cfg(test)]
mod source_read_tests;

#[cfg(feature = "testing")]
pub mod testing;

/// Limits shared by all semantic execution in one explicitly requested root.
///
/// Interpret the limits together: requested bytes also cover initialization and copying of
/// fixed value representations, including each admitted inline construction. Semantic work alone
/// is not a CPU-work bound.
/// Native source preparation and allocator internals are outside these execution limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisPolicy {
    /// Admitted semantic and control operations, excluding representation copying charged as bytes.
    pub semantic_work_limit: usize,
    /// Cumulative conservative storage quotations, not live memory or allocator backing.
    /// Accepted charges are not refunded when storage is released.
    pub requested_bytes_limit: usize,
}

/// A completed result or the completed portion of an interrupted analysis.
#[derive(Debug, Eq, PartialEq)]
#[must_use]
pub enum AnalysisOutcome<T, P = ()> {
    Complete(T),
    Incomplete {
        reason: AnalysisIncomplete,
        completed: P,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisIncomplete {
    WorkLimit,
    RequestedAllocationLimit,
    UnavailableOperation(OperationId),
}

/// A class-validation phase whose controlled dependencies are not implemented yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassCheckOperation {
    InheritanceCycle,
    InheritanceDiagnostic,
    Slots,
    SlotDataclassConflict,
    SlotDefinition,
    InstanceLayout,
    SlotNamespace,
    GenericEnum,
    EnumSubtype,
    EnumMetadata,
    FinalDecorator,
    SubclassConstruction,
    Kind,
    CodeGenerator,
    NamedTuple,
    Protocol,
    DisjointBaseDecorator,
    DataclassApplication,
    ExplicitBases,
    ExplicitBaseTuple,
    ExplicitBaseCycleNormalization,
    Mro,
    MroObject,
    MroBaseConversion,
    MroSingleBaseCollection,
    MroBaseCollection,
    MroSpecialization,
    MroMerge,
    MroErrorDetails,
    MroDynamic,
    TotalOrdering,
    Metaclass,
    Arguments,
    GenericContext,
    Pep695GenericContext,
    InheritedGenericContext,
    InheritedTypeVariables,
    InheritedContextConstruction,
    GenericDefaults,
    GenericAliasConstruction,
    DataclassFields,
    Overrides,
    Namespace,
    AbstractMethods,
    FinalValues,
    ProtocolVariance,
    TypedDict,
    Members,
}

/// A class's own-member lookup dependency without a controlled implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnMemberOperation {
    Dynamic,
    DynamicNamedTuple,
    DynamicTypedDict,
    DynamicEnum,
    TupleLen,
    TupleGetitem,
    TupleNew,
    TupleRuntimeSpecialization,
    DunderParamSpec,
    ConstructorContext,
    SlotDescriptor,
    TotalOrdering,
    FrozenSubclass,
    GeneratedMember,
    ClassDunderCallable,
    AugmentedBindings,
    TypedDictMro,
    GenericClassMro,
}

/// Deferred inference that has no controlled implementation yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeferredInferenceOperation {
    TypeVar,
    ParamSpec,
    TypeVarTuple,
    AssignmentNamedTuple,
    AssignmentNewType,
    AssignmentTypeAliasType,
    AssignmentBuiltinType,
    AssignmentTypedDict,
    AssignmentNewClass,
    AssignmentConstraints,
    AssignmentConstraintDiagnostic,
    AssignmentBoundDiagnostic,
    AssignmentBoundedDefault,
    AssignmentParamSpecDefault,
    AssignmentTypeVarTupleDefault,
    TypeParameterOuterScopeDefault,
    ParamSpecDefaultDiagnostic,
    ParamSpecDefaultClass,
    TypeVarTupleDefaultDiagnostic,
    ExtraItemsClassification,
    ExtraItemsAnnotation,
    FunctionDefaults,
    ExpressionScope,
}

/// An implicit-name lookup dependency without a controlled implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImplicitNameOperation {
    ClassBody,
    SpecialModuleGlobal,
    ModuleGlobalDocstring,
    ModuleGlobalMember,
    RuntimeVisibilityMetadata,
    RuntimeVisibilityAlias,
    RuntimeVisibilitySubscript,
    RuntimeVisibilityCycle,
}

/// A TypeVar-default dependency without a controlled implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeVarDefaultOperation {
    AliasGenericContext,
    AliasIdentity,
    RecursiveIdentity,
    AliasValue,
    AliasRawValue,
    RecursiveUnfold,
    ParamSpecConversion,
    CycleNormalization,
    RecursiveNormalization,
}

/// An assignment-validation dependency without a controlled implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssignmentValidationOperation {
    Legacy,
    FullyStatic,
    DataclassBody,
    PureRedundancy,
    InvalidDiagnostic,
    UnsoundDiagnostic,
}

/// A constructor metadata dependency without a controlled implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConstructorPreparationOperation {
    DynamicCodeGenerator,
}

/// Diagnostic work required by a rejected quoted type annotation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotedAnnotationOperation {
    RawStringDiagnostic,
    ConcatenatedStringDiagnostic,
    EscapeDiagnostic,
    SyntaxDiagnostic,
}

/// A source operation that has no controlled implementation yet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationId {
    ImplicitName(ImplicitNameOperation),
    StatementQuery,
    StatementClass,
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
    AssignmentValidation(AssignmentValidationOperation),
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
    AnnotatedAssignment(AnnotatedAssignmentOperation),
    LegacyTypeVarDiagnostic,
    LegacyTypeVarForwardedOwner,
    TypeVarDefault(TypeVarDefaultOperation),
    BoundTypeVarDefaultCycleNormalization,
    BoundTypeVarDefaultRecursiveNormalization,
    GenericAliasCycleMerge,
    LegacyTypeVariables(LegacyTypeVarOperation),
    DefaultSpecialization(DefaultSpecializationOperation),
    FunctionParamSpec,
    FunctionUnpackedKwargs,
    FunctionReturnCheck,
    ReturnGeneratorType,
    ReturnNoneType,

    ScopeDeferred,
    ScopePostcheck,
    ClassCheck(ClassCheckOperation),
    OwnMember(OwnMemberOperation),
    Deferred(DeferredInferenceOperation),
    ExpressionBody,
    ScopeCycleInitial,
    ExpressionCycleInitial,
    ScopeCycleRecovery,
    ExpressionCycleRecovery,
    ContextualScopeKey,
    ContextualExpressionKey,
    DefinitionBody(SourceDefinitionEffect),
    DefinitionCycleInitial,
    DefinitionCycleRecovery,
    DeferredDefinitionCycleInitial,
    DeferredDefinitionCycleRecovery,
    FunctionSignatureCycleInitial,
    FunctionSignatureCycleRecovery,
    ModuleLiteral,
    ExpressionKind,
    StringLiteralExpectedType,
    StringTypeAlias,
    RevealType,
    UnresolvedReference,
    ExpressionScope,
    ContextualExpression,
    DataclassFieldSpecifiers,
    TypeExpression,
    AnnotationString,
    QuotedAnnotation(QuotedAnnotationOperation),
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
    TypeConversion(TypeConversionOperation),
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
    DecoratorApplication(DecoratorApplicationOperation),
    FunctionMetadata,
    FunctionTypeParameterShadowDiagnostic,
    TypeParameterConstraintCountDiagnostic,
    ClassDefinition,
    ImportPolicy,
    RelativeImport,
    ImportDiagnostic,
    ImportBinding,
    Submodule,
    ModuleGetattr,
    ModuleTypeMember,
    ImplicitPlace,
    PlaceScope,
    PlacePromotion,
    ModuleFallback,
    ReExport,
    DiscardedBinding,
    LoopHeader,
    Reachability,
    ReachabilityPrefix,
    ReachabilityCheckpoint,
    ReachabilityPredicate,
    Truthiness(TruthinessOperation),
    ChainedComparison(ChainedComparisonOperation),
    KnownClassInstance(KnownClassInstanceOperation),
    InstanceFlagsMetaclass,
    ExplicitAnyInstanceConstruction,
    TypeComparison(TypeComparisonOperation),
    TupleSpec(TupleSpecOperation),
    RecursiveNormalization(RecursiveNormalizationOperation),
    Equality(EqualityOperation),
    Attribute(AttributeOperation),
    MemberLookup(GeneralMemberOperation),
    Descriptor(DescriptorOperation),
    CallableConversion(CallableConversionOperation),
    TypeSearch(SearchOperation),
    Narrowing,
    Union,
    FunctionComparison,
    Equivalence,
    MissingBinding,
    Deprecation,
    CanonicalMerge,
    ExpressionStorage,
    ExpressionCache,
    CallArguments,
    CallBindings,
    CallableGuard(CallableGuardOperation),
    ConstructorPreparation(ConstructorPreparationOperation),
    ConstructorStorage(ConstructorStorageOperation),
    ConstructorSignature(ConstructorSignatureOperation),
    BoundMethodPreparation(BoundMethodPreparationOperation),
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
    ClassSelection,
    GenericIntersection,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisFailure {
    Preparation(PreparationError),
    /// The prepared file has no independently inferable expression for this node key.
    InvalidExpressionKey,
    Start(StartError),
    Execution(RunError),
}

/// Structural source state retained for semantic requests in one database revision.
///
/// Keeping the database borrowed preserves the index's lifetime. The parsed module remains pinned
/// until the handle is dropped, including while interrupted semantic owners are being drained.
/// Clones share structural preparation discovered by semantic requests, so same-revision retries
/// can reuse it. Semantic reads still validate and record the canonical dependencies.
#[derive(Clone)]
pub struct PreparedAnalysisFile<'db> {
    root: Rc<PreparedSource<'db>>,
    sources: Rc<RefCell<Rc<PreparedSources<'db>>>>,
}

impl<'db> PreparedAnalysisFile<'db> {
    /// The retained syntax tree from which expression node keys can be selected.
    pub fn parsed_module(&self) -> &ParsedModuleRef {
        self.root.parsed_module()
    }

    pub(crate) fn belongs_to(&self, db: &dyn Db) -> bool {
        self.root.belongs_to(db)
    }

    pub(crate) fn program_file(&self) -> ProgramFile<'db> {
        self.root.program_file()
    }

    pub(crate) fn semantic_index(&self) -> &'db SemanticIndex<'db> {
        self.root.semantic_index()
    }
}

pub(crate) struct PreparedSource<'db> {
    db: &'db dyn Db,
    stamp: Stamp,
    file: ProgramFile<'db>,
    module: ParsedModuleRef,
    index: &'db SemanticIndex<'db>,
    parsed_memo: PreparedSourceMemo<'db, ParsedModule>,
    index_memo: PreparedSourceMemo<'db, SemanticIndex<'db>>,
    host_reads: Rc<dyn PreparedHostFileReads<'db> + 'db>,
    source_memo: PreparedSourceMemo<'db, SourceText>,
    suppressions_memo: PreparedSourceMemo<'db, Suppressions>,
    file_module_memo: PreparedSourceMemo<'db, Option<Module<'db>>>,
    global_scope_memo: PreparedSourceMemo<'db, ScopeId<'db>>,
}

impl<'db> PreparedSource<'db> {
    pub(crate) fn parsed_module(&self) -> &ParsedModuleRef {
        &self.module
    }

    pub(crate) fn program_file(&self) -> ProgramFile<'db> {
        self.file
    }

    pub(crate) fn host_reads(&self) -> Rc<dyn PreparedHostFileReads<'db> + 'db> {
        Rc::clone(&self.host_reads)
    }

    pub(crate) fn belongs_to(&self, db: &dyn Db) -> bool {
        std::ptr::addr_eq(db, self.db) && self.stamp.belongs_to(db)
    }

    pub(crate) fn semantic_index(&self) -> &'db SemanticIndex<'db> {
        self.index
    }

    fn check_current(&self) -> Result<(), PreparationError> {
        self.db.unwind_if_revision_cancelled();
        if !self.stamp.belongs_to(self.db) {
            return Err(PreparationError::ChangedDatabaseStamp);
        }
        self.parsed_memo
            .check_current()
            .and_then(|()| self.index_memo.check_current())
            .and_then(|()| self.source_memo.check_current())
            .and_then(|()| self.suppressions_memo.check_current())
            .and_then(|()| self.file_module_memo.check_current())
            .and_then(|()| self.global_scope_memo.check_current())
            .map_err(|_| PreparationError::InvalidDependency)?;
        self.host_reads.check_current()
    }

    pub(crate) async fn read_source_text(&self, endpoint: &TaskEndpoint<'_, 'db>) -> SourceText {
        let source = endpoint
            .read_prepared_source(source_text::prepared_read(&self.source_memo))
            .await;
        endpoint
            .local_call(|| {
                // The canonical memo retains the backing; this clone and its eventual drop
                // only adjust the Arc count, including when a semantic owner is interrupted.
                endpoint.admit_work(8)?;
                endpoint.check_completion()?;
                Ok(source.clone())
            })
            .await
    }

    pub(crate) async fn read_suppressions(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
    ) -> &'db Suppressions {
        endpoint
            .read_prepared_source(suppressions::prepared_read(&self.suppressions_memo))
            .await
    }

    pub(crate) async fn read_semantic_index(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
    ) -> &'db SemanticIndex<'db> {
        endpoint
            .read_prepared_source(semantic_index::prepared_read(&self.index_memo))
            .await
    }

    pub(crate) async fn read_parsed_module(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
    ) -> &'db ParsedModule {
        endpoint
            .read_prepared_source(parsed_module::prepared_read(&self.parsed_memo))
            .await
    }

    pub(crate) async fn read_source(&self, endpoint: &TaskEndpoint<'_, 'db>) {
        let module = self.read_parsed_module(endpoint).await;
        let index = self.read_semantic_index(endpoint).await;
        endpoint
            .local_call(|| {
                endpoint.admit_work(4)?;
                endpoint.check_completion()?;
                if module != self.module.module() || !std::ptr::eq(index, self.index) {
                    return Err(RunError::Contract("prepared source identity changed"));
                }
                Ok(())
            })
            .await;
    }
}

struct PreparedSourceParts<'db> {
    db: &'db dyn Db,
    stamp: Stamp,
    file: ProgramFile<'db>,
    parsed: &'db ParsedModule,
    module: ParsedModuleRef,
    index: &'db SemanticIndex<'db>,
    suppressions: &'db Suppressions,
}

impl<'db> PreparedSourceParts<'db> {
    fn certify(self) -> Result<Rc<PreparedSource<'db>>, PreparationError> {
        self.db.unwind_if_revision_cancelled();
        if !self.stamp.belongs_to(self.db) {
            return Err(PreparationError::ChangedDatabaseStamp);
        }
        let parsed_memo = parsed_module::prepare_memo(self.db, self.file.python_file(self.db))
            .map_err(|_| PreparationError::InvalidDependency)?;
        let index_memo = semantic_index::prepare_memo(self.db, self.file)
            .map_err(|_| PreparationError::InvalidDependency)?;
        let source_memo = source_text::prepare_memo(self.db, self.file.file(self.db))
            .map_err(|_| PreparationError::InvalidDependency)?;
        let suppressions_memo = suppressions::prepare_memo(self.db, self.file.python_file(self.db))
            .map_err(|_| PreparationError::InvalidDependency)?;
        let file_module_memo =
            file_to_module::prepare_memo(self.db, self.file.resolver_file(self.db))
                .map_err(|_| PreparationError::InvalidDependency)?;
        let global_scope_memo = global_scope::prepare_memo(self.db, self.file)
            .map_err(|_| PreparationError::InvalidDependency)?;
        let host_reads = self
            .db
            .prepare_analysis_host_reads(self.file.file(self.db))?;
        let parsed = parsed_memo
            .value()
            .map_err(|_| PreparationError::InvalidDependency)?;
        let index = index_memo
            .value()
            .map_err(|_| PreparationError::InvalidDependency)?;
        let suppressions = suppressions_memo
            .value()
            .map_err(|_| PreparationError::InvalidDependency)?;
        if !std::ptr::eq(parsed, self.parsed)
            || parsed != self.module.module()
            || !std::ptr::eq(index, self.index)
            || !std::ptr::eq(suppressions, self.suppressions)
        {
            return Err(PreparationError::InvalidDependency);
        }
        let prepared = PreparedSource {
            db: self.db,
            stamp: self.stamp,
            file: self.file,
            module: self.module,
            index: self.index,
            parsed_memo,
            index_memo,
            host_reads,
            source_memo,
            suppressions_memo,
            file_module_memo,
            global_scope_memo,
        };
        prepared.check_current()?;
        Ok(Rc::new(prepared))
    }
}

/// Prepare structural source state without executing semantic inference.
///
/// This operation resolves the already applied environment, parses and indexes the file, and loads
/// suppression structure. Preparation is outside semantic quotas and requires an idle database.
pub fn prepare_file<'db>(
    db: &'db dyn Db,
    file: File,
) -> Result<PreparedAnalysisFile<'db>, PreparationError> {
    let parts = try_with_preparation(db, || {
        let stamp = Stamp::current(db);
        let file = db.prepare_analysis_environment(file);
        prepare_program_file(db, stamp, file)
    })?;
    let root = parts.certify()?;
    let sources = PreparedSources {
        program: root.file.program(db),
        entries: RefCell::new(PreparedSourceEntries {
            files: FxHashMap::from_iter([(root.file.file(db), Rc::clone(&root))]),
            modules: Vec::new(),
        }),
    };
    Ok(PreparedAnalysisFile {
        root,
        sources: Rc::new(RefCell::new(Rc::new(sources))),
    })
}

fn prepare_program_file<'db>(
    db: &'db dyn Db,
    stamp: Stamp,
    file: ProgramFile<'db>,
) -> PreparedSourceParts<'db> {
    let python_file = file.python_file(db);
    let parsed = parsed_module(db, python_file);
    let module = parsed.load(db);
    let index = semantic_index(db, file);
    global_scope(db, file);
    file_to_module(db, file.resolver_file(db));
    let suppressions = suppressions(db, python_file);
    source_text(db, file.file(db));
    PreparedSourceParts {
        db,
        stamp,
        file,
        parsed,
        module,
        index,
        suppressions,
    }
}

/// Infer a type through the canonical expression query with an explicit, nonrenewable budget.
///
/// Select `expression_key` from the syntax retained by [`PreparedAnalysisFile::parsed_module`]. Node
/// keys identify nodes within that file; they do not identify a source file or database themselves.
///
/// Existing inference entry points retain their ordinary behavior. Enabling this module does not
/// select this execution mode for other calls. Native database cancellation still unwinds.
pub fn expression_type_with_policy<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    expression_key: ExpressionNodeKey,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Type<'db>>, AnalysisFailure> {
    if !prepared.root.stamp.belongs_to(prepared.root.db) {
        return Err(AnalysisFailure::Preparation(
            PreparationError::ChangedDatabaseStamp,
        ));
    }
    let expression = prepared
        .semantic_index()
        .try_expression(expression_key)
        .ok_or(AnalysisFailure::InvalidExpressionKey)?;
    with_analysis_session(prepared, policy, |session| {
        run_source_expression(session, prepared, expression, expression_key)
    })
}

/// Check a prepared file through the ordinary typing and diagnostic phases under one budget.
///
/// Completed scope queries remain available after an interrupted request. The incomplete outcome
/// contains no partial file diagnostics; callers can retry in the same revision with the same
/// preparation. A completed result has the same diagnostic and read-error shape as [`crate::check_file`].
/// Native database cancellation still unwinds after execution owners have been cleaned up.
pub fn check_file_with_policy(
    prepared: &PreparedAnalysisFile<'_>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Result<Box<[Diagnostic]>, Diagnostic>>, AnalysisFailure> {
    if !prepared.root.stamp.belongs_to(prepared.root.db) {
        return Err(AnalysisFailure::Preparation(
            PreparationError::ChangedDatabaseStamp,
        ));
    }
    with_analysis_session(prepared, policy, |session| {
        run_source_file(session, prepared)
    })
}

pub(crate) struct AnalysisSession<'session, 'db> {
    db: &'db dyn Db,
    stamp: Stamp,
    budget: ExecutionBudget<'session>,
    sources: Rc<PreparedSources<'db>>,
    stop: RefCell<Option<StopReason>>,
}

enum PreparationRequest<'db> {
    File {
        file: File,
        program: Program<'db>,
    },
    Module {
        program: Program<'db>,
        name: ModuleName,
        importing_file: Option<File>,
    },
}

enum StopReason {
    PreparationFailed(AnalysisFailure),
    Unavailable(OperationId),
}

struct PreparedModuleResolution<'db> {
    name: ModuleName,
    importing_file: Option<File>,
    resolution: ResolverPreparedModuleResolution<'db>,
}

enum PreparedModuleDemand<'db> {
    Ready(ResolverPreparedModuleResolution<'db>),
    Missing(PreparationRequest<'db>),
}

struct PreparedSources<'db> {
    program: Program<'db>,
    entries: RefCell<PreparedSourceEntries<'db>>,
}

impl<'db> PreparedSources<'db> {
    fn entries(&self) -> RunResult<Ref<'_, PreparedSourceEntries<'db>>> {
        self.entries
            .try_borrow()
            .map_err(|_| RunError::Contract("prepared source catalogue is mutably borrowed"))
    }

    fn entries_mut(&self) -> RunResult<RefMut<'_, PreparedSourceEntries<'db>>> {
        self.entries.try_borrow_mut().map_err(|_| {
            RunError::Contract("prepared source catalogue is borrowed during publication")
        })
    }

    fn file(&self, file: File) -> RunResult<Option<Rc<PreparedSource<'db>>>> {
        Ok(self.entries()?.files.get(&file).cloned())
    }

    fn module(
        &self,
        name: &ModuleName,
        importing_file: Option<File>,
    ) -> RunResult<Option<ResolverPreparedModuleResolution<'db>>> {
        Ok(self.entries()?.module(name, importing_file).cloned())
    }

    fn prepare(
        &self,
        db: &'db dyn Db,
        stamp: Stamp,
        request: PreparationRequest<'db>,
    ) -> Result<(), AnalysisFailure> {
        let (program, already_prepared) = match &request {
            PreparationRequest::File { file, program } => (
                *program,
                self.file(*file).map_err(AnalysisFailure::Execution)?.is_some(),
            ),
            PreparationRequest::Module {
                program,
                name,
                importing_file,
            } => (
                *program,
                self.module(name, *importing_file)
                    .map_err(AnalysisFailure::Execution)?
                    .is_some(),
            ),
        };
        if program != self.program || already_prepared {
            return Err(AnalysisFailure::Execution(RunError::Contract(
                "structural demand did not identify an unprepared input in the root program",
            )));
        }
        match request {
            PreparationRequest::File { file, .. } => self.prepare_source(db, stamp, file),
            PreparationRequest::Module {
                name,
                importing_file,
                ..
            } => {
                let (existing, parts) = try_with_preparation(db, || {
                    let module = resolve_requested_module(db, self.program, &name, importing_file);
                    let file = module.and_then(|module| module.file(db));
                    let existing = file.map(|file| self.file(file)).transpose()?.flatten();
                    let parts = file.filter(|_| existing.is_none()).map(|file| {
                        db.prepare_analysis_file_settings(file);
                        prepare_program_file(db, stamp, self.program.program_file(db, file))
                    });
                    Ok((existing, parts))
                })
                .map_err(AnalysisFailure::Preparation)?
                .map_err(AnalysisFailure::Execution)?;
                let resolution = ResolverPreparedModuleResolution::prepare(
                    db,
                    self.program.resolver_environment(db),
                    &name,
                    importing_file,
                )
                .map_err(AnalysisFailure::Preparation)?;
                let source = parts
                    .map(PreparedSourceParts::certify)
                    .transpose()
                    .map_err(AnalysisFailure::Preparation)?
                    .map(|source| (source.file.file(db), source));
                if let Some(existing) = existing {
                    existing.check_current().map_err(AnalysisFailure::Preparation)?;
                }
                resolution.check_current().map_err(AnalysisFailure::Preparation)?;
                let mut entries = self.entries_mut().map_err(AnalysisFailure::Execution)?;
                if entries.module(&name, importing_file).is_some()
                    || source.as_ref().is_some_and(|(file, _)| entries.files.contains_key(file))
                {
                    return Err(AnalysisFailure::Execution(RunError::Contract(
                        "structural preparation would replace a published source or module",
                    )));
                }
                if let Some((file, source)) = source {
                    entries.files.insert(file, source);
                }
                entries.modules.push(PreparedModuleResolution {
                    name,
                    importing_file,
                    resolution,
                });
                Ok(())
            }
        }
    }

    fn prepare_source(
        &self,
        db: &'db dyn Db,
        stamp: Stamp,
        file: File,
    ) -> Result<(), AnalysisFailure> {
        let parts = try_with_preparation(db, || {
            db.prepare_analysis_file_settings(file);
            prepare_program_file(db, stamp, self.program.program_file(db, file))
        }).map_err(AnalysisFailure::Preparation)?;
        let source = parts.certify().map_err(AnalysisFailure::Preparation)?;
        let mut entries = self.entries_mut().map_err(AnalysisFailure::Execution)?;
        if entries.files.contains_key(&file) {
            return Err(AnalysisFailure::Execution(RunError::Contract(
                "structural preparation would replace a published source",
            )));
        }
        entries.files.insert(file, source);
        Ok(())
    }
}

struct PreparedSourceEntries<'db> {
    files: FxHashMap<File, Rc<PreparedSource<'db>>>,
    modules: Vec<PreparedModuleResolution<'db>>,
}

impl<'db> PreparedSourceEntries<'db> {
    fn module(
        &self,
        name: &ModuleName,
        importing_file: Option<File>,
    ) -> Option<&ResolverPreparedModuleResolution<'db>> {
        self.modules.iter().find_map(|entry| {
            (entry.importing_file == importing_file && entry.name == *name)
                .then_some(&entry.resolution)
        })
    }
}

fn resolve_requested_module<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    name: &ModuleName,
    importing_file: Option<File>,
) -> Option<Module<'db>> {
    let environment = program.resolver_environment(db);
    match importing_file {
        Some(file) => resolve_module(db, ImportingFile::File(file, environment), name),
        None => resolve_module_confident(db, environment, name),
    }
}

impl<'session, 'db> AnalysisSession<'session, 'db> {
    pub(crate) fn db(&self) -> &'db dyn Db {
        self.db
    }

    pub(crate) fn program(&self) -> Program<'db> {
        self.sources.program
    }

    pub(crate) async fn read_semantic_index(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
        file: ProgramFile<'db>,
    ) -> RunResult<&'db SemanticIndex<'db>> {
        let fields = file.read_fields(endpoint.field_request_context());
        let python_file = endpoint
            .read_field(fields.python_file(), &BorrowOrCopy)
            .await;
        let file = endpoint
            .read_field(
                python_file
                    .read_fields(endpoint.field_request_context())
                    .file(),
                &BorrowOrCopy,
            )
            .await;
        let program = endpoint.read_field(fields.program(), &BorrowOrCopy).await;
        let source = self.source(endpoint, file, program).await?;
        Ok(source.read_semantic_index(endpoint).await)
    }

    pub(crate) async fn read_known_module(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
        file: ProgramFile<'db>,
    ) -> RunResult<Option<KnownModule>> {
        match self.read_file_module(endpoint, file).await? {
            Some(module) => module.known_with(endpoint).await,
            None => Ok(None),
        }
    }

    pub(crate) async fn read_file_module(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
        file: ProgramFile<'db>,
    ) -> RunResult<Option<Module<'db>>> {
        let context = endpoint.field_request_context();
        let python_file = endpoint
            .read_field(file.read_fields(context).python_file(), &BorrowOrCopy)
            .await;
        let physical_file = endpoint
            .read_field(python_file.read_fields(context).file(), &BorrowOrCopy)
            .await;
        let program = endpoint
            .read_field(file.read_fields(context).program(), &BorrowOrCopy)
            .await;
        let _environment = endpoint
            .read_field(
                program.field_requests(context).resolver_environment(),
                &BorrowOrCopy,
            )
            .await;
        let source = self.source(endpoint, physical_file, program).await?;
        let module = endpoint
            .read_prepared_source(file_to_module::prepared_read(&source.file_module_memo))
            .await;
        local_with_fixed_transfers_at(endpoint, 1, size_of::<Option<Module<'db>>>(), || *module)
            .await
    }

    pub(crate) async fn read_global_scope(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
        file: ProgramFile<'db>,
    ) -> RunResult<ScopeId<'db>> {
        let context = endpoint.field_request_context();
        let python_file = endpoint
            .read_field(file.read_fields(context).python_file(), &BorrowOrCopy)
            .await;
        let physical_file = endpoint
            .read_field(python_file.read_fields(context).file(), &BorrowOrCopy)
            .await;
        let program = endpoint
            .read_field(file.read_fields(context).program(), &BorrowOrCopy)
            .await;
        let source = self.source(endpoint, physical_file, program).await?;
        let scope = endpoint
            .read_prepared_source(global_scope::prepared_read(&source.global_scope_memo))
            .await;
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<ScopeId<'db>>() + 1)?;
                endpoint.check_completion()?;
                Ok(*scope)
            })
            .await)
    }

    pub(crate) fn budget(&self) -> &ExecutionBudget<'session> {
        &self.budget
    }

    /// Call only within the endpoint's protected local operation.
    pub(crate) fn unavailable<T>(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
        operation: OperationId,
    ) -> RunResult<T> {
        self.interrupt(endpoint, StopReason::Unavailable(operation))
    }

    /// Obtain pinned structural inputs through protected operations. Missing inputs are prepared
    /// while the executor retains the semantic owners and their canonical query claims.
    pub(crate) async fn source(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
        file: File,
        program: Program<'db>,
    ) -> RunResult<Rc<PreparedSource<'db>>> {
        if let Some(source) = endpoint
            .local_call(|| self.lookup_source(endpoint, file, program))
            .await
        {
            return Ok(source);
        }
        self.prepare_sources(endpoint, PreparationRequest::File { file, program })
            .await;
        Ok(endpoint
            .local_call(|| {
                self.lookup_source(endpoint, file, program)?
                    .ok_or(RunError::Contract(
                        "structural preparation did not publish the requested source",
                    ))
            })
            .await)
    }

    fn lookup_source(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
        file: File,
        program: Program<'db>,
    ) -> RunResult<Option<Rc<PreparedSource<'db>>>> {
        endpoint.admit_work(5)?;
        // The catalogue retains the backing through cleanup; this handle only changes Rc counts.
        endpoint.admit_work(8)?;
        endpoint.check_completion()?;
        if program != self.sources.program {
            return Err(RunError::Contract(
                "source request belongs to another program",
            ));
        }
        if let Some(source) = self.sources.file(file)? {
            if !source.belongs_to(self.db) {
                return Err(RunError::Contract("prepared source database stamp changed"));
            }
            endpoint.check_completion()?;
            return Ok(Some(source));
        }
        Ok(None)
    }

    /// Prepare missing module resolutions while retaining semantic execution. Replaying the prepared
    /// canonical query records its dependency, including when resolution found no module.
    pub(crate) async fn resolve_module(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
        program: Program<'db>,
        name: &ModuleName,
        importing_file: Option<File>,
    ) -> RunResult<Option<Module<'db>>> {
        let demand = endpoint
            .local_call(|| {
                if let Some(resolution) =
                    self.lookup_module(endpoint, program, name, importing_file)?
                {
                    return Ok(PreparedModuleDemand::Ready(resolution));
                }
                // Compact names can allocate more than their text length. Reserve fixed headroom for the
                // string's minimum allocation and metadata, as well as the retained request itself.
                let requested_bytes = name
                    .as_str()
                    .len()
                    .checked_add(4 * size_of::<ModuleName>())
                    .and_then(|bytes| bytes.checked_add(size_of::<PreparationRequest<'db>>()))
                    .ok_or(RunError::Contract("module request byte quotation overflow"))?;
                endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
                endpoint.check_completion()?;
                Ok(PreparedModuleDemand::Missing(PreparationRequest::Module {
                        program,
                        name: name.clone(),
                        importing_file,
                    }))
            })
            .await;
        let resolution = match demand {
            PreparedModuleDemand::Ready(resolution) => resolution,
            PreparedModuleDemand::Missing(request) => {
                self.prepare_sources(endpoint, request).await;
                endpoint
                    .local_call(|| {
                        self.lookup_module(endpoint, program, name, importing_file)?
                            .ok_or(RunError::Contract(
                                "structural preparation did not publish the requested module",
                            ))
                    })
                    .await
            }
        };
        let _environment = endpoint
            .read_field(
                program
                    .field_requests(endpoint.field_request_context())
                    .resolver_environment(),
                &BorrowOrCopy,
            )
            .await;
        Ok(resolution.read(endpoint).await)
    }

    fn lookup_module(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
        program: Program<'db>,
        name: &ModuleName,
        importing_file: Option<File>,
    ) -> RunResult<Option<ResolverPreparedModuleResolution<'db>>> {
        // The linear lookup bounds comparisons without allocating a temporary owned key. The
        // additional scans cover canonical-key hashing, a possible name copy, and its disposal.
        let entries = self.sources.entries()?.modules.len();
        // PreparedSourceMemo::clone copies eight fixed fields. The primary and optional fallback
        // require at most two clones; 32 operations cover those copies and their enclosing carriers.
        // The certificates borrow their memo backing; cloning them never clones the module value.
        let work = entries
            .checked_add(8)
            .and_then(|entries| name.as_str().len().checked_add(1)?.checked_mul(entries))
            .and_then(|work| work.checked_add(8))
            .and_then(|work| work.checked_add(32))
            .ok_or(RunError::Contract("module request work quotation overflow"))?;
        let requested_bytes = size_of::<ResolverPreparedModuleResolution<'db>>()
            .checked_mul(4)
            .and_then(|bytes| {
                bytes.checked_add(
                    size_of::<Option<ResolverPreparedModuleResolution<'db>>>().checked_mul(3)?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    size_of::<RunResult<Option<ResolverPreparedModuleResolution<'db>>>>()
                        .checked_mul(2)?,
                )
            })
            .and_then(|bytes| bytes.checked_add(size_of::<RunResult<PreparedModuleDemand<'db>>>()))
            .and_then(|bytes| {
                bytes.checked_add(size_of::<RunResult<ResolverPreparedModuleResolution<'db>>>())
            })
            .ok_or(RunError::Contract("module request byte quotation overflow"))?;
        endpoint.admit_work(work)?;
        endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
        endpoint.check_completion()?;
        if program != self.sources.program {
            return Err(RunError::Contract(
                "module request belongs to another program",
            ));
        }
        let resolution = self.sources.module(name, importing_file)?;
        endpoint.check_completion()?;
        Ok(resolution)
    }

    async fn prepare_sources(
        &self,
        endpoint: &TaskEndpoint<'_, 'db>,
        request: PreparationRequest<'db>,
    ) {
        endpoint
            .prepare_structural(|| {
                if let Err(error) = self.sources.prepare(self.db, self.stamp, request) {
                    let mut stop = self.stop.try_borrow_mut().map_err(|_| {
                        RunError::Contract("preparation failure has a borrowed stop reason")
                    })?;
                    if stop.is_some() {
                        return Err(RunError::Contract(
                            "preparation failure follows an earlier stop",
                        ));
                    }
                    *stop = Some(StopReason::PreparationFailed(error));
                    return Err(RunError::Refused(Incomplete::Interrupted));
                }
                Ok(())
            })
            .await;
    }

    fn interrupt<T>(&self, endpoint: &TaskEndpoint<'_, 'db>, stop: StopReason) -> RunResult<T> {
        endpoint.check_completion()?;
        let reason = report_incomplete(self.db, Incomplete::Interrupted);
        if reason == Incomplete::Interrupted && self.stop.borrow().is_none() {
            *self.stop.borrow_mut() = Some(stop);
        }
        Err(RunError::Refused(reason))
    }
}

pub(crate) fn with_analysis_session<'db, T>(
    prepared: &PreparedAnalysisFile<'db>,
    policy: &AnalysisPolicy,
    body: impl for<'session> FnOnce(&AnalysisSession<'session, 'db>) -> RunResult<T>,
) -> Result<AnalysisOutcome<T>, AnalysisFailure> {
    let db = prepared.root.db;
    // The outer attachment clears local cancellation on exit. Retain it through the final
    // cancellation check, including when a child completed before its parent refused work.
    salsa::attach(db, || {
        if !prepared.belongs_to(db) {
            return Err(AnalysisFailure::Preparation(
                PreparationError::ChangedDatabaseStamp,
            ));
        }
        // Keep the catalogue borrowed through structural preparation as well as semantic execution.
        // Clones cannot start a second request while this invocation is preparing structural inputs.
        let sources = prepared
            .sources
            .try_borrow_mut()
            .map_err(|_| AnalysisFailure::Start(StartError::NestedAttempt))?;
        let limits = ExecutionLimits {
            semantic_work: policy.semantic_work_limit,
            requested_bytes: policy.requested_bytes_limit,
        };
        let stop = Cell::new(None);
        let actual_error = Cell::new(None);
        let receipt = try_with_metered_execution_budget(db, limits, |budget| {
            let session = AnalysisSession {
                db,
                stamp: prepared.root.stamp,
                budget,
                sources: Rc::clone(&sources),
                stop: RefCell::new(None),
            };
            let result = with_explicit_reads(db, &session.budget, || body(&session));
            actual_error.set(result.as_ref().err().copied());
            stop.set(session.stop.take());
            result
        })
        .map_err(AnalysisFailure::Start)?;

        // Driver and provider owners have drained. Check real cancellation outside query-cycle masking.
        db.unwind_if_revision_cancelled();
        remaining_after(limits, receipt.usage)?;
        finish(receipt.outcome, actual_error.get(), stop.take())
    })
}

fn remaining_after(
    limits: ExecutionLimits,
    usage: ExecutionUsage,
) -> Result<ExecutionLimits, AnalysisFailure> {
    let contract = || {
        AnalysisFailure::Execution(RunError::Contract(
            "execution receipt exceeds remaining root budget",
        ))
    };
    Ok(ExecutionLimits {
        semantic_work: limits
            .semantic_work
            .checked_sub(usage.semantic_work)
            .ok_or_else(contract)?,
        requested_bytes: limits
            .requested_bytes
            .checked_sub(usage.requested_bytes)
            .ok_or_else(contract)?,
    })
}

fn finish<T>(
    outcome: AttemptOutcome<RunResult<T>>,
    actual_error: Option<RunError>,
    stop: Option<StopReason>,
) -> Result<AnalysisOutcome<T>, AnalysisFailure> {
    if let Some(error @ (RunError::Contract(_) | RunError::RequiresFetch)) = actual_error {
        return Err(AnalysisFailure::Execution(error));
    }
    let reason = match outcome {
        AttemptOutcome::Complete(Ok(value)) if actual_error.is_none() => {
            return Ok(AnalysisOutcome::Complete(value));
        }
        AttemptOutcome::Complete(Err(RunError::Preparation(error))) => {
            return Err(AnalysisFailure::Preparation(error));
        }
        AttemptOutcome::Complete(Err(error)) => return Err(AnalysisFailure::Execution(error)),
        AttemptOutcome::Complete(Ok(_)) => {
            return Err(AnalysisFailure::Execution(RunError::Contract(
                "completed analysis retained an execution error",
            )));
        }
        AttemptOutcome::Incomplete(Incomplete::Allowance) => AnalysisIncomplete::WorkLimit,
        AttemptOutcome::Incomplete(Incomplete::RequestedAllocation) => {
            AnalysisIncomplete::RequestedAllocationLimit
        }
        AttemptOutcome::Incomplete(Incomplete::Interrupted) => {
            if let Some(RunError::Preparation(error)) = actual_error {
                return Err(AnalysisFailure::Preparation(error));
            }
            match stop {
                Some(StopReason::Unavailable(operation))
                    if actual_error == Some(RunError::Refused(Incomplete::Interrupted)) =>
                {
                    AnalysisIncomplete::UnavailableOperation(operation)
                }
                Some(StopReason::PreparationFailed(error))
                    if actual_error == Some(RunError::Refused(Incomplete::Interrupted)) =>
                {
                    return Err(error);
                }
                _ => {
                    return Err(AnalysisFailure::Execution(
                        actual_error.unwrap_or(RunError::Refused(Incomplete::Interrupted)),
                    ));
                }
            }
        }
    };
    Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_record_cannot_replace_a_hard_error() {
        for error in [RunError::Contract("root error"), RunError::RequiresFetch] {
            assert_eq!(
                finish::<()>(
                    AttemptOutcome::Incomplete(Incomplete::Interrupted),
                    Some(error),
                    Some(StopReason::Unavailable(OperationId::ExpressionBody))
                ),
                Err(AnalysisFailure::Execution(error)),
            );
        }
    }

    #[test]
    fn unavailable_record_requires_the_exact_protected_refusal() {
        assert_eq!(
            finish::<()>(
                AttemptOutcome::Incomplete(Incomplete::Interrupted),
                None,
                Some(StopReason::Unavailable(OperationId::ExpressionBody))
            ),
            Err(AnalysisFailure::Execution(RunError::Refused(
                Incomplete::Interrupted
            ))),
        );
        assert_eq!(
            finish::<()>(
                AttemptOutcome::Incomplete(Incomplete::Interrupted),
                Some(RunError::Refused(Incomplete::Interrupted)),
                None
            ),
            Err(AnalysisFailure::Execution(RunError::Refused(
                Incomplete::Interrupted
            ))),
        );
    }

    #[test]
    fn preparation_failure_preserves_provenance_after_the_protected_refusal() {
        for failure in [
            AnalysisFailure::Preparation(PreparationError::UnsupportedDependency),
            AnalysisFailure::Execution(RunError::Contract("preparation error")),
        ] {
            assert_eq!(
                finish::<()>(
                    AttemptOutcome::Incomplete(Incomplete::Interrupted),
                    Some(RunError::Refused(Incomplete::Interrupted)),
                    Some(StopReason::PreparationFailed(failure)),
                ),
                Err(failure),
            );
            assert_eq!(
                finish::<()>(
                    AttemptOutcome::Incomplete(Incomplete::Interrupted),
                    None,
                    Some(StopReason::PreparationFailed(failure)),
                ),
                Err(AnalysisFailure::Execution(RunError::Refused(
                    Incomplete::Interrupted,
                ))),
            );
            for error in [RunError::Contract("root error"), RunError::RequiresFetch] {
                assert_eq!(
                    finish::<()>(
                        AttemptOutcome::Incomplete(Incomplete::Interrupted),
                        Some(error),
                        Some(StopReason::PreparationFailed(failure)),
                    ),
                    Err(AnalysisFailure::Execution(error)),
                );
            }
        }
    }

    #[test]
    fn runtime_preparation_errors_preserve_prior_budget_refusals() {
        let preparation = PreparationError::InvalidDependency;
        let error = RunError::Preparation(preparation);
        for outcome in [
            AttemptOutcome::Complete(Err(error)),
            AttemptOutcome::Incomplete(Incomplete::Interrupted),
        ] {
            assert_eq!(
                finish::<()>(
                    outcome,
                    Some(error),
                    Some(StopReason::Unavailable(OperationId::ExpressionBody)),
                ),
                Err(AnalysisFailure::Preparation(preparation)),
            );
        }
        for (reason, expected) in [
            (Incomplete::Allowance, AnalysisIncomplete::WorkLimit),
            (
                Incomplete::RequestedAllocation,
                AnalysisIncomplete::RequestedAllocationLimit,
            ),
        ] {
            assert_eq!(
                finish::<()>(AttemptOutcome::Incomplete(reason), Some(error), None),
                Ok(AnalysisOutcome::Incomplete {
                    reason: expected,
                    completed: (),
                }),
            );
        }
    }
}
