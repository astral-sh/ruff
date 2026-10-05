use std::cell::RefCell;
use std::future::{Future, poll_fn};
use std::panic::AssertUnwindSafe;

use ruff_python_ast::name::Name;
use ruff_text_size::{Ranged, TextRange};
use salsa::execution_probe::FinalSourceMemo;
use rustc_hash::{FxHashMap, FxHashSet};

use super::*;
use crate::FxOrderSet;
use crate::analysis::DeferredInferenceOperation;
use crate::types::callable::{CallableType, CallableTypeKind};
use crate::types::class::KnownClassArgument;
use crate::types::constraints::{ConstraintSet, ConstraintSetBuilder, OwnedConstraintTypeCursor};
use crate::types::cyclic::guard_storage::observations as lifetime_observations;
use crate::types::function::source::FunctionSignatureEffects;
use crate::types::infer::builder::DeferredExpressionState;
use crate::types::infer::legacy_callable_observations as legacy_observations;
use crate::types::infer::legacy_callable_observations::{
    Boundary, Cancellation, Event as LegacyEvent, ScopeState,
};
use crate::types::infer::{InferenceFlags, TypeExpressionFlags, infer_deferred_types, infer_scope_types};
use crate::types::instance::ProtocolInterfaceSource;
use crate::types::legacy_typevars::{
    LegacyProtocolTypeStep, LegacyTypeVarOperation, LegacyVisitorScopes, Pending,
};
use crate::types::protocol_class::{
    LegacyProtocolTestMember, ProtocolInterface, ProtocolMemberData,
};
use crate::types::signatures::source::SignatureSourceEffects;
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::typevar::{
    TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation, TypeVarIdentity,
    TypeVarInstance, TypeVarKind, TypeVarNonce,
};
use crate::types::{
    BindingContext, BoundTypeVarInstance, FindLegacyTypeVarsVisitor, GenericContext,
    KnownInstanceType, ProtocolInstanceType, SubclassOfInner, SubclassOfType,
};

const CALL: &str =
    "from typing import Any\ndef choose(value: Any) -> Any:\n    pass\nchoose(True)\n";
const CALLABLE_CALL: &str = "from typing import Any, Callable\ndef choose(value: Callable[..., Any]) -> Any:\n    pass\nchoose(choose)\n";
const ANNOTATIONS: &str = "from typing import Any\ndef choose(first: Any = [], /, second: Any = [], *args: Any, flag: Any = [], **kwargs: Any) -> Any:\n    pass\n";

#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) struct State {
    pub(in crate::types::infer) binding: Option<salsa::Id>,
    pub(in crate::types::infer) flags: InferenceFlags,
    pub(in crate::types::infer) deferred: DeferredExpressionState,
    pub(in crate::types::infer) had_cache: bool,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        let deferred = match (self.deferred, other.deferred) {
            (DeferredExpressionState::None, DeferredExpressionState::None)
            | (DeferredExpressionState::Deferred, DeferredExpressionState::Deferred) => true,
            (
                DeferredExpressionState::InStringAnnotation(left),
                DeferredExpressionState::InStringAnnotation(right),
            ) => left == right,
            _ => false,
        };
        self.binding == other.binding
            && self.flags == other.flags
            && deferred
            && self.had_cache == other.had_cache
    }
}

impl Eq for State {}

#[derive(Clone, Copy, Debug)]
struct Annotation {
    range: TextRange,
    state: State,
    remaining: usize,
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    started: Vec<State>,
    completed: Vec<State>,
    restored: Vec<State>,
    entered: Vec<Annotation>,
    inferred: Vec<Annotation>,
    cancellation_check_returned: bool,
    type_parameter_pending: usize,
    pending_annotations: Vec<Annotation>,
    completed_after_guard_events: Vec<usize>,
    restored_after_guard_events: Vec<usize>,
    subclass_targets: Vec<salsa::Id>,
    subclass_inputs: Vec<SubclassInput>,
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static CANCEL: Cell<bool> = const { Cell::new(false) };
    static CANCEL_SUBCLASS: Cell<bool> = const { Cell::new(false) };
    static JOURNAL: RefCell<Snapshot> = RefCell::new(Snapshot::default());
}

struct Recording;

impl Recording {
    fn start(cancel: bool) -> Self {
        assert!(!ENABLED.replace(true));
        CANCEL.set(cancel);
        JOURNAL.with_borrow_mut(|journal| *journal = Snapshot::default());
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        ENABLED.set(false);
        CANCEL.set(false);
        CANCEL_SUBCLASS.set(false);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SubclassInput {
    Class,
    Protocol,
    Dynamic,
    TypeVar,
}

/// Records entry into the generic `type[C[T]]` target before specialization and input selection.
pub(in crate::types::infer) fn subclass_target_entered(class: StaticClassLiteral<'_>) {
    if ENABLED.get() {
        JOURNAL.with_borrow_mut(|journal| journal.subclass_targets.push(class.as_id()));
    }
}

/// Records the selected ClassSubclass input and optionally requests cancellation before the
/// `SubclassInstanceEffects::subclass` child is constructed. This hook requests cancellation;
/// the canonical query controls when that request can be delivered.
pub(in crate::types::infer) fn subclass_constructing(db: &dyn Db, inner: SubclassOfInner<'_>) {
    if ENABLED.get() {
        let input = match inner {
            SubclassOfInner::Class(_) => SubclassInput::Class,
            SubclassOfInner::Protocol(_) => SubclassInput::Protocol,
            SubclassOfInner::Dynamic(_) => SubclassInput::Dynamic,
            SubclassOfInner::TypeVar(_) => SubclassInput::TypeVar,
        };
        JOURNAL.with_borrow_mut(|journal| journal.subclass_inputs.push(input));
        if CANCEL_SUBCLASS.replace(false) {
            db.cancellation_token().cancel();
        }
    }
}

pub(in crate::types::infer) fn transaction_started(_db: &dyn Db, state: State) {
    if ENABLED.get() {
        JOURNAL.with_borrow_mut(|journal| journal.started.push(state));
    }
}

pub(in crate::types::infer) fn transaction_completed(_db: &dyn Db, state: State) {
    if ENABLED.get() {
        JOURNAL.with_borrow_mut(|journal| {
            journal.completed.push(state);
            journal
                .completed_after_guard_events
                .push(lifetime_observations::snapshot().count);
        });
    }
}

pub(in crate::types::infer) fn transaction_restored(_db: &dyn Db, state: State) {
    if ENABLED.get() {
        JOURNAL.with_borrow_mut(|journal| {
            journal.restored.push(state);
            journal
                .restored_after_guard_events
                .push(lifetime_observations::snapshot().count);
        });
    }
}

/// Records actual suspension of PEP 695 annotations inside their canonical scope provider.
/// Observing the outer request alone misses children polled by that independent provider.
pub(in crate::types::infer) async fn observe_type_parameter_polling<F: Future>(
    annotations: F,
) -> F::Output {
    let mut annotations = std::pin::pin!(annotations);
    poll_fn(|context| {
        let result = annotations.as_mut().poll(context);
        if ENABLED.get() && result.is_pending() {
            JOURNAL.with_borrow_mut(|journal| {
                journal.type_parameter_pending += 1;
                if let Some(annotation) = journal.entered.last() {
                    journal.pending_annotations.push(*annotation);
                }
            });
        }
        result
    })
    .await
}

pub(in crate::types::infer) fn annotation_entered(db: &dyn Db, state: State, range: TextRange) {
    if ENABLED.get() {
        let remaining = salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
        JOURNAL.with_borrow_mut(|journal| {
            journal.entered.push(Annotation {
                range,
                state,
                remaining,
            })
        });
    }
}

pub(in crate::types::infer) fn annotation_completed(db: &dyn Db, state: State, range: TextRange) {
    if ENABLED.get() {
        let remaining = salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
        JOURNAL.with_borrow_mut(|journal| {
            journal.inferred.push(Annotation {
                range,
                state,
                remaining,
            })
        });
        if CANCEL.replace(false) {
            db.cancellation_token().cancel();
            db.unwind_if_revision_cancelled();
            JOURNAL.with_borrow_mut(|journal| journal.cancellation_check_returned = true);
        }
    }
}

fn snapshot() -> Snapshot {
    JOURNAL.with_borrow(Clone::clone)
}

fn database(source: &str, version: PythonVersion, stub: bool) -> TestDb {
    let mut db = TestDbBuilder::new()
        .with_python_version(version)
        .build()
        .unwrap();
    db.write_file(if stub { "src/main.pyi" } else { "src/main.py" }, source)
        .unwrap();
    db
}

fn prepared(db: &TestDb, stub: bool) -> PreparedAnalysisFile<'_> {
    let file = system_path_to_file(db, if stub { "src/main.pyi" } else { "src/main.py" }).unwrap();
    prepare_file(db, file).unwrap()
}

fn function<'a>(prepared: &'a PreparedAnalysisFile<'_>) -> &'a ast::StmtFunctionDef {
    prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .find_map(|statement| {
            if let Stmt::FunctionDef(function) = statement {
                Some(function)
            } else {
                None
            }
        })
        .unwrap()
}

fn annotations(function: &ast::StmtFunctionDef) -> Vec<(&ast::Expr, InferenceFlags)> {
    let mut result = Vec::new();
    if let Some(returns) = function.returns.as_deref() {
        result.push((returns, InferenceFlags::IN_RETURN_TYPE));
    }
    for parameter in function.parameters.iter_non_variadic_params() {
        if let Some(annotation) = parameter.annotation() {
            result.push((annotation, InferenceFlags::IN_PARAMETER_ANNOTATION));
        }
    }
    if let Some(parameter) = &function.parameters.vararg
        && let Some(annotation) = parameter.annotation.as_deref()
    {
        result.push((
            annotation,
            InferenceFlags::IN_PARAMETER_ANNOTATION | InferenceFlags::IN_VARARG_ANNOTATION,
        ));
    }
    if let Some(parameter) = &function.parameters.kwarg
        && let Some(annotation) = parameter.annotation.as_deref()
    {
        result.push((
            annotation,
            InferenceFlags::IN_PARAMETER_ANNOTATION | InferenceFlags::IN_KWARG_ANNOTATION,
        ));
    }
    result
}

#[derive(Clone, Copy, Debug)]
enum Request<'ast, 'db> {
    Deferred,
    TypeParameter(Definition<'db>),
    Pep695Context,
    PublicSignature,
    ReturnLocations {
        parameters: &'ast Parameters<'db>,
        return_type: Type<'db>,
    },
    Parameter(&'ast ast::Expr),
    LegacyVariables(Type<'db>),
}

#[derive(Clone, Copy)]
enum Interruption {
    WorkLimit,
    LocalCancellation,
    ColdDependencyCancellation,
}

#[derive(Debug, Eq, PartialEq)]
enum Value<'db> {
    Deferred(&'db DefinitionInference<'db>),
    TypeParameter,
    Pep695Context(GenericContext<'db>),
    PublicSignature(&'db CallableSignature<'db>),
    ReturnLocations {
        outside: FxHashSet<BoundTypeVarInstance<'db>>,
        inside: FxHashMap<CallableType<'db>, FxOrderSet<BoundTypeVarInstance<'db>>>,
    },
    Parameter(Type<'db>, TypeExpressionFlags),
    LegacyVariables(FxOrderSet<BoundTypeVarInstance<'db>>),
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    request: Request<'_, 'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<Value<'db>>, AnalysisFailure> {
    let function_node = function(prepared);
    let type_parameters = function_node.type_params.as_deref();
    let definition = prepared
        .semantic_index()
        .expect_single_definition(function_node);
    with_analysis_session(prepared, policy, |session| {
        let environments = StableStorage::new();
        let builders = StableStorage::new();
        let owners = StableStorage::new();
        let default_arguments = StableStorage::new();
        let return_callables = crate::types::relation::source::resources::ReturnCallableMappingStorage::new();
        let mapping = StableStorage::new();
        let checkers = CheckerStorage::new();
        let resources = SourceResources::new(
            &environments,
            &builders,
            &owners,
            &mapping,
            &checkers,
            &default_arguments,
            &return_callables,
        );
        let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
        let (function, overload) = register_function_values(session.db(), &mut registry)?;
        let callable = register_callable_values(session.db(), &mut registry)?;
        let bound_method = register_bound_method_values(session.db(), &mut registry)?;
        let descriptor_get_call_context =
            register_descriptor_get_call_context_values(session.db(), &mut registry)?;
        let descriptor_dispatch = register_descriptor_dispatch_values(session.db(), &mut registry)?;
        let descriptor_dispatches =
            register_descriptor_dispatches_values(session.db(), &mut registry)?;
        let property = register_property_values(session.db(), &mut registry)?;
        let tuple = register_tuple_values(session.db(), &mut registry)?;
        let string_literal = registry.finite_interned_values_with_memos(
            StringLiteralType::ingredient(session.db().zalsa()),
            (),
        )?;
        let union = register_union_values(session.db(), &mut registry)?;
        let intersection = register_intersection_values(session.db(), &mut registry)?;
        let module = register_module_values(session.db(), &mut registry)?;
        let class = register_class_values(session.db(), &mut registry)?;
        let known_class = register_known_class_values(session.db(), &mut registry)?;
        let member = register_member_lookup_values(session.db(), &mut registry)?;
        let type_pair = register_source_type_pair_values(session.db(), &mut registry)?;
        let expression_context = register_expression_context_values(session.db(), &mut registry)?;
        let values = SourceValues {
            type_pair,
            expression_context,
            function,
            overload,
            callable,
            bound_method,
            descriptor_get_call_context,
            descriptor_dispatch,
            descriptor_dispatches,
            property,
            tuple,
            string_literal,
            union,
            intersection,
            module,
            class,
            known_class,
            member,
        };
        let (run, routes) = register(session, prepared, registry, &values, resources)?;
        let values = &values;
        run.run(|endpoint| async move {
            let access = SourceQueryAccess {
                session,
                endpoint,
                routes,
                values,
            };
            match request {
                Request::Deferred => access
                    .deferred_definition(definition)
                    .await
                    .map(Value::Deferred),
                Request::TypeParameter(parameter) => access
                    .definition(parameter)
                    .await
                    .map(|_| Value::TypeParameter),
                Request::PublicSignature => {
                    let effects = SourceEffects::new(&access, session.program());
                    let binding = FunctionSignatureEffects::binding_type(
                        &effects,
                        session.db(),
                        definition,
                    )
                    .await?;
                    let Type::FunctionLiteral(function) = binding else {
                        return Err(RunError::Contract("fixture function binding"));
                    };
                    access.function_signature(function).await.map(Value::PublicSignature)
                }
                Request::ReturnLocations { parameters, return_type } => {
                    let effects = SourceEffects::new(&access, session.program());
                    let locations = crate::types::generics::return_scoping::ReturnScopeEffects::locations(
                        &effects, parameters, return_type,
                    ).await?;
                    Ok(Value::ReturnLocations {
                        outside: locations.found_outside_callable_return,
                        inside: locations.found_inside_callable_return,
                    })
                }
                Request::Pep695Context => {
                    let effects = SourceEffects::new(&access, session.program());
                    FunctionSignatureEffects::pep695_context(
                        &effects,
                        session.db(),
                        prepared.semantic_index(),
                        definition,
                        type_parameters.unwrap(),
                    )
                    .await
                    .map(Value::Pep695Context)
                }
                Request::Parameter(expression) => {
                    let effects = SourceEffects::new(&access, session.program());
                    let (ty, flags) = SignatureSourceEffects::parameter_annotation(
                        &effects,
                        session.db(),
                        definition,
                        expression,
                    )
                    .await?;
                    Ok(Value::Parameter(ty, flags))
                }
                Request::LegacyVariables(ty) => {
                    let file = prepared.program_file();
                    let source = access.prepare_existing(file).await?;
                    let env = ProgramEnvironment::from_file(file);
                    let effects = SourceEffects::new(&access, session.program());
                    effects
                        .legacy_variables_for_test(
                            &source,
                            &env,
                            prepared
                                .semantic_index()
                                .expression(expression_key(prepared)),
                            ty,
                        )
                        .await
                        .map(Value::LegacyVariables)
                }
            }
        })
    })
}

fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn assert_completed_transaction(definition: Definition<'_>, function: &ast::StmtFunctionDef) {
    let journal = snapshot();
    assert_eq!(journal.started.len(), 1);
    assert_eq!(journal.completed, journal.started);
    assert!(journal.restored.is_empty());
    let expected = annotations(function);
    assert_eq!(journal.entered.len(), expected.len());
    assert_eq!(journal.inferred.len(), expected.len());
    let annotation_flags = InferenceFlags::IN_RETURN_TYPE
        | InferenceFlags::IN_PARAMETER_ANNOTATION
        | InferenceFlags::IN_VARARG_ANNOTATION
        | InferenceFlags::IN_KWARG_ANNOTATION;
    for ((entered, inferred), (expression, flags)) in
        journal.entered.iter().zip(&journal.inferred).zip(expected)
    {
        assert_eq!(entered.range, expression.range());
        assert_eq!(inferred.range, expression.range());
        assert_eq!(entered.state, inferred.state);
        assert_eq!(entered.state.binding, Some(definition.as_id()));
        assert_eq!(entered.state.flags & annotation_flags, flags);
    }
}

/// Cold ordinary and callable annotations publish reusable canonical signature and inference memos.
#[test_case::test_case(CALL; "any annotation")]
#[test_case::test_case(CALLABLE_CALL; "stored callable annotation")]
fn cold_annotated_call_reads_deferred_signature_results_and_reuses_canonical_memos(source: &str) {
    let db = database(source, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let function_node = function(&prepared);
    let definition = prepared
        .semantic_index()
        .expect_single_definition(function_node);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(false);
    let cold = capture(&db, || {
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
    })
    .unwrap();
    drop(recording);
    assert_eq!(cold.value, Ok(AnalysisOutcome::Complete(Type::any())));
    cold.check_root_reads().unwrap();
    assert_completed_transaction(definition, function_node);
    assert_cleanup();

    let Type::FunctionLiteral(function) =
        infer_definition_types(&db, definition).binding_type(definition)
    else {
        panic!("fixture function")
    };
    let signature_ingredient = function_literal_signature_ingredient(&db);
    let signature_key = signature_ingredient.database_key_index(function.as_id());
    let deferred_ingredient = deferred_definition_inference_ingredient(&db);
    let deferred_key = deferred_ingredient.database_key_index(definition.as_id());
    let reads: Vec<_> = cold
        .reads
        .iter()
        .filter(|read| read.parent == Some(signature_key))
        .filter_map(|read| {
            let name = db.ingredient_debug_name(read.key.ingredient_index());
            matches!(
                name.as_ref(),
                "infer_definition_types"
                    | "infer_deferred_types"
                    | "infer_function_default_types"
                    | "infer_scope_types_impl"
            )
            .then_some(name.into_owned())
        })
        .collect();
    assert_eq!(
        reads,
        [
            "infer_deferred_types",
            "infer_deferred_types",
            "infer_deferred_types"
        ]
    );
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, deferred_ingredient, definition.as_id()).is_ok()
    );
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, signature_ingredient, function.as_id()).is_ok()
    );
    let cold_signature = cold
        .reads
        .iter()
        .find(|read| read.key == signature_key)
        .unwrap();
    let cold_deferred = cold
        .reads
        .iter()
        .find(|read| read.key == deferred_key)
        .unwrap();

    let ordinary_db = database(source, PythonVersion::PY313, false);
    let ordinary_prepared = self::prepared(&ordinary_db, false);
    let expression = ordinary_prepared
        .semantic_index()
        .expression(expression_key(&ordinary_prepared));
    assert_eq!(
        infer_expression_types(&ordinary_db, expression, TypeContext::default())
            .expression_type(expression.node_ref(&ordinary_db)),
        Type::any()
    );

    let mut reader = db.clone();
    reader.take_salsa_events();
    let native = capture(&db, || {
        (
            function.signature(&db),
            infer_deferred_types(&db, definition),
        )
    })
    .unwrap();
    for (key, address) in [
        (signature_key, cold_signature.memo_address),
        (deferred_key, cold_deferred.memo_address),
    ] {
        assert!(
            native
                .reads
                .iter()
                .any(|read| read.key == key && read.memo_address == address)
        );
    }
    observations::reset(None);
    let recording = Recording::start(false);
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        cold.value
    );
    drop(recording);
    assert!(snapshot().started.is_empty());
    let events = reader.take_salsa_events();
    for query in [
        "function_literal_signature",
        "infer_deferred_types",
        "infer_expression_types_impl",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

/// Refusal before a callable-annotated signature completes leaves that memo unpublished and retryable.
#[test]
fn callable_annotation_refusal_preserves_completed_dependencies_and_retries() {
    let measured = database(CALLABLE_CALL, PythonVersion::PY313, false);
    let measured_prepared = prepared(&measured, false);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(
            &measured_prepared,
            expression_key(&measured_prepared),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(Type::any())),
    );
    let signature_work = funded().semantic_work_limit - observations::signature_ready().1.unwrap();
    assert_cleanup();

    let db = database(CALLABLE_CALL, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let definition = prepared
        .semantic_index()
        .expect_single_definition(function(&prepared));
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    assert_eq!(
        expression_type_with_policy(
            &prepared,
            expression_key(&prepared),
            &AnalysisPolicy {
                semantic_work_limit: signature_work - 1,
                ..funded()
            },
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        }),
    );
    assert_eq!(observations::signature_ready().0, 0);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .is_ok()
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            deferred_definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .is_ok()
    );
    let Type::FunctionLiteral(function) =
        infer_definition_types(&db, definition).binding_type(definition)
    else {
        panic!("fixture function")
    };
    assert!(matches!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            function_literal_signature_ingredient(&db),
            function.as_id()
        ),
        Err(FinalSourceError::MissingMemo),
    ));
    assert_cleanup();
    assert_eq!(
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded()),
        Ok(AnalysisOutcome::Complete(Type::any())),
    );
    assert_eq!(observations::signature_ready().0, 1);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            function_literal_signature_ingredient(&db),
            function.as_id(),
        )
        .is_ok()
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

/// Creates a legacy variable with stored identity fields, without a source inference dependency.
fn legacy_variable<'db>(
    db: &'db TestDb,
    name: &str,
    kind: TypeVarKind,
) -> BoundTypeVarInstance<'db> {
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(db, Name::new(name), None, kind),
            None,
            None,
            None,
        ),
        BindingContext::Synthetic(db.program_environment().program(db)),
        None,
        TypeVarNonce::NONE,
    )
}

/// Forces pending-stack spill and seen-table growth while nested receiver cursors remain owned.
///
/// Four callable levels leave sibling annotations, defaults, and overloads pending, exceeding the
/// stack's inline capacity. Twelve independent receiver constraints grow each seen table after its
/// first allocation. Only one bound per level contains the nested callable, keeping the walk linear.
#[derive(Debug)]
struct StoredCallable<'db> {
    callable: CallableType<'db>,
    declaration: BoundTypeVarInstance<'db>,
}

impl<'db> StoredCallable<'db> {
    fn new(db: &'db TestDb) -> Self {
        let env = db.program_environment();
        let annotation = legacy_variable(db, "Annotation", TypeVarKind::LegacyTypeVar);
        let default = legacy_variable(db, "Default", TypeVarKind::LegacyTypeVar);
        let returns = legacy_variable(db, "Return", TypeVarKind::LegacyTypeVar);
        let declaration = legacy_variable(db, "Declaration", TypeVarKind::LegacyTypeVar);
        let keys: Vec<_> = (0..12)
            .map(|index| {
                legacy_variable(db, &format!("Receiver{index}"), TypeVarKind::LegacyTypeVar)
            })
            .collect();
        let mut nested = Type::TypeVar(returns);
        for _ in 0..4 {
            let constraints = ConstraintSetBuilder::new().into_owned(|builder| {
                keys.iter().fold(
                    ConstraintSet::from_bool(builder, true),
                    |constraints, key| {
                        constraints.and(db, builder, || {
                            let lower = if *key == keys[0] {
                                nested
                            } else {
                                Type::TypeVar(returns)
                            };
                            ConstraintSet::constrain_typevar_lower_bound(
                                db, &env, builder, *key, lower,
                            )
                        })
                    },
                )
            });
            nested = Type::single_callable(
                db,
                Signature::new_generic(
                    Some(GenericContext::from_typevar_instances(
                        db,
                        &env,
                        [declaration],
                    )),
                    Parameters::standard([
                        Parameter::positional_only(None)
                            .with_annotated_type(Type::TypeVar(annotation))
                            .with_default_type(Type::TypeVar(default)),
                        Parameter::variadic(Name::new_static("args"))
                            .with_annotated_type(Type::TypeVar(annotation)),
                        Parameter::keyword_variadic(Name::new_static("kwargs"))
                            .with_annotated_type(Type::TypeVar(default)),
                    ]),
                    Type::TypeVar(returns),
                )
                .with_probe_receiver_constraints(constraints),
            );
        }
        let callable = CallableType::new(
            db,
            CallableSignature {
                overloads: [
                    Signature::new(
                        Parameters::standard([
                            Parameter::positional_only(None).with_annotated_type(nested),
                            Parameter::keyword_only(Name::new_static("flag"))
                                .with_annotated_type(Type::TypeVar(annotation)),
                        ]),
                        Type::TypeVar(returns),
                    ),
                    Signature::new(Parameters::gradual_form(), Type::TypeVar(default)),
                    Signature::new(Parameters::top(), Type::TypeVar(returns)),
                ]
                .into_iter()
                .collect(),
            },
            CallableTypeKind::FunctionLike,
        );
        Self {
            callable,
            declaration,
        }
    }
}

/// Checks one destructor entry per cursor before the collector and source builder owner retire.
/// These entry hooks do not prove that cursor backing deallocation has completed.
fn assert_legacy_retirement(snapshot: &legacy_observations::Snapshot) {
    let created: Vec<_> = snapshot
        .events
        .iter()
        .filter_map(|event| match event {
            LegacyEvent::CursorCreated(id) => Some(*id),
            _ => None,
        })
        .collect();
    let dropped: Vec<_> = snapshot
        .events
        .iter()
        .filter_map(|event| match event {
            LegacyEvent::CursorDrop { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(dropped.len(), created.len());
    let mut sorted = dropped;
    sorted.sort_unstable();
    assert_eq!(sorted, created);
    assert_eq!(snapshot.live_cursors, 0);
    let collector = snapshot
        .events
        .iter()
        .rposition(|event| matches!(event, LegacyEvent::CollectorDrop { live_cursors: 0 }))
        .unwrap();
    let owner = snapshot
        .events
        .iter()
        .rposition(|event| matches!(event, LegacyEvent::OwnerRetired { live_cursors: 0 }))
        .unwrap();
    assert!(collector < owner);
    assert!(
        snapshot.events[collector + 1..]
            .iter()
            .all(|event| !matches!(event, LegacyEvent::CursorDrop { .. }))
    );
    assert_cleanup();
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallableRepresentation {
    Instance,
    KnownValue,
}

/// Both callable representations preserve legacy-variable order through source effects and enter
/// each cursor destructor before the collector and source builder owner retire.
#[test_case::test_case(CallableRepresentation::Instance; "callable instance")]
#[test_case::test_case(CallableRepresentation::KnownValue; "known callable value")]
fn stored_callable_collection_preserves_order_and_retires_flat_owners(
    representation: CallableRepresentation,
) {
    let db = database(CALL, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let fixture = StoredCallable::new(&db);
    let ty = match representation {
        CallableRepresentation::Instance => Type::Callable(fixture.callable),
        CallableRepresentation::KnownValue => {
            Type::KnownInstance(KnownInstanceType::Callable(fixture.callable))
        }
    };
    let mut expected = FxOrderSet::default();
    ty.find_legacy_typevars(&db, &db.program_environment(), None, &mut expected);
    assert!(!expected.contains(&fixture.declaration));
    assert_eq!(expected.len(), 15);
    observations::reset(None);
    let recording = legacy_observations::Recording::start(Cancellation::Never);
    let result = controlled(&prepared, Request::LegacyVariables(ty), &funded());
    drop(recording);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Complete(Value::LegacyVariables(expected)))
    );
    let snapshot = legacy_observations::snapshot();
    assert!(snapshot.events.iter().any(|event| matches!(
        event,
        LegacyEvent::Accepted {
            boundary: Boundary::StackSpill,
            stack: Some(state),
            ..
        } if state.spilled
    )));
    assert!(snapshot.events.iter().any(|event| matches!(event, LegacyEvent::Accepted { boundary: Boundary::ReceiverStep, cursor: Some(state), .. } if state.seen_len > 1)));
    assert_legacy_retirement(&snapshot);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdmissionLimit {
    Work,
    Bytes,
}

/// Matches populated receiver growth, owning transfers, and the other requested admission kinds.
/// A `ReusedTransfer` target uses the enclosing `OwningEnqueue` observation because it supplies the
/// populated cursor state as well as the stack's spare capacity; both describe one stack admission.
fn boundary_matches(target: Boundary, admission: &legacy_observations::Admission) -> bool {
    match target {
        Boundary::ReceiverStep => {
            admission.boundary == target
                && admission.cursor.is_some_and(|state| {
                    state.seen_len > 0 && state.seen_len == state.seen_capacity
                })
        }
        Boundary::ReusedTransfer => {
            admission.boundary == Boundary::OwningEnqueue
                && admission.cursor.is_some_and(|state| state.seen_len > 0)
                && admission
                    .stack
                    .is_some_and(|state| state.len < state.capacity)
        }
        Boundary::Pop => {
            admission.boundary == target && admission.cursor.is_some_and(|state| state.seen_len > 0)
        }
        Boundary::CursorCreate | Boundary::OwningEnqueue | Boundary::StackSpill => {
            admission.boundary == target
        }
        Boundary::ScopeStorageCreate
        | Boundary::VisitorCreate
        | Boundary::ScopeEnter
        | Boundary::ScopeRestore
        | Boundary::ProtocolMember
        | Boundary::ProtocolTypes
        | Boundary::ProtocolSlot => false,
    }
}

/// Reports whether the selected production boundary completed, even if later work was refused.
fn boundary_accepted(snapshot: &legacy_observations::Snapshot, target: Boundary) -> bool {
    let mut selected = None;
    for event in &snapshot.events {
        match event {
            LegacyEvent::Before(admission) if boundary_matches(target, admission) => {
                selected = Some(admission.boundary);
            }
            LegacyEvent::Accepted { boundary, .. } if Some(*boundary) == selected => return true,
            _ => {}
        }
    }
    false
}

/// Runs collection under the supplied policy and returns scalar outcome and ownership observations.
fn run_stored_admission<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    ty: Type<'db>,
    policy: &AnalysisPolicy,
) -> (
    Result<AnalysisOutcome<()>, AnalysisFailure>,
    legacy_observations::Snapshot,
) {
    observations::reset(None);
    let recording = legacy_observations::Recording::start(Cancellation::Never);
    let result = controlled(prepared, Request::LegacyVariables(ty), policy);
    drop(recording);
    let result = result.map(|result| match result {
        AnalysisOutcome::Complete(Value::LegacyVariables(_)) => AnalysisOutcome::Complete(()),
        AnalysisOutcome::Complete(
            Value::Deferred(_)
            | Value::TypeParameter
            | Value::Pep695Context(_)
            | Value::PublicSignature(_)
            | Value::ReturnLocations { .. }
            | Value::Parameter(..),
        ) => {
            panic!("legacy collection result")
        }
        AnalysisOutcome::Incomplete { reason, completed } => {
            AnalysisOutcome::Incomplete { reason, completed }
        }
    });
    let snapshot = legacy_observations::snapshot();
    assert_cleanup();
    (result, snapshot)
}

/// Runs each limit measurement on fresh values so prior memo reuse cannot alter the boundary.
fn stored_admission_probe(
    policy: &AnalysisPolicy,
) -> (
    Result<AnalysisOutcome<()>, AnalysisFailure>,
    legacy_observations::Snapshot,
) {
    let db = database(CALL, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let fixture = StoredCallable::new(&db);
    run_stored_admission(&prepared, Type::Callable(fixture.callable), policy)
}

/// Work and byte refusal leave the selected owning boundary unmutated and retire all admitted cursors.
///
/// Receiver work refusal stops before its fixed result-carrier admission. Its byte probe reaches
/// the first populated seen-table growth. `receiver_constraint_steps_admit_growth` in
/// `constraints/type_analysis.rs` checks refusal at each `TddControl` admission within that growth.
#[test_case::test_case(Boundary::CursorCreate, AdmissionLimit::Work; "cursor work")]
#[test_case::test_case(Boundary::CursorCreate, AdmissionLimit::Bytes; "cursor bytes")]
#[test_case::test_case(Boundary::OwningEnqueue, AdmissionLimit::Work; "enqueue work")]
#[test_case::test_case(Boundary::OwningEnqueue, AdmissionLimit::Bytes; "enqueue bytes")]
#[test_case::test_case(Boundary::StackSpill, AdmissionLimit::Work; "spill work")]
#[test_case::test_case(Boundary::StackSpill, AdmissionLimit::Bytes; "spill bytes")]
#[test_case::test_case(Boundary::ReusedTransfer, AdmissionLimit::Work; "reused transfer work")]
#[test_case::test_case(Boundary::ReusedTransfer, AdmissionLimit::Bytes; "reused transfer bytes")]
#[test_case::test_case(Boundary::ReceiverStep, AdmissionLimit::Work; "populated receiver work")]
#[test_case::test_case(Boundary::ReceiverStep, AdmissionLimit::Bytes; "populated receiver bytes")]
#[test_case::test_case(Boundary::Pop, AdmissionLimit::Work; "pop work")]
#[test_case::test_case(Boundary::Pop, AdmissionLimit::Bytes; "pop bytes")]
fn stored_callable_admission_refusal_preserves_live_owners(
    target: Boundary,
    limit: AdmissionLimit,
) {
    let (result, measured) = stored_admission_probe(&funded());
    assert_eq!(result, Ok(AnalysisOutcome::Complete(())));
    let admission = measured
        .events
        .iter()
        .find_map(|event| match event {
            LegacyEvent::Before(admission) if boundary_matches(target, admission) => {
                Some(*admission)
            }
            _ => None,
        })
        .unwrap();
    assert!(admission.work > 0);
    let representation = match target {
        Boundary::CursorCreate => size_of::<OwnedConstraintTypeCursor<'_, '_>>(),
        Boundary::OwningEnqueue | Boundary::ReusedTransfer | Boundary::StackSpill => {
            size_of::<Pending<'_, '_>>()
        }
        Boundary::Pop => size_of::<Option<Pending<'_, '_>>>(),
        Boundary::ReceiverStep => size_of::<Option<Option<[Type<'_>; 2]>>>(),
        Boundary::ScopeStorageCreate => size_of::<LegacyVisitorScopes<'_>>(),
        Boundary::VisitorCreate | Boundary::ScopeEnter => {
            size_of::<FindLegacyTypeVarsVisitor<'_>>()
        }
        Boundary::ScopeRestore => size_of::<Option<FindLegacyTypeVarsVisitor<'_>>>(),
        Boundary::ProtocolMember => size_of::<Option<(&Name, &ProtocolMemberData<'_>)>>(),
        Boundary::ProtocolTypes => size_of::<[Option<Type<'_>>; 6]>(),
        Boundary::ProtocolSlot => size_of::<Option<LegacyProtocolTypeStep<'_>>>(),
    };
    assert!(admission.bytes >= representation);
    assert!(boundary_accepted(&measured, target));
    assert_legacy_retirement(&measured);

    let policy = match limit {
        AdmissionLimit::Work => AnalysisPolicy {
            semantic_work_limit: funded().semantic_work_limit - admission.remaining_work
                + admission.work
                - 1,
            ..funded()
        },
        AdmissionLimit::Bytes => {
            let mut lower = 0;
            let mut upper = funded().requested_bytes_limit;
            while lower < upper {
                let middle = lower + (upper - lower) / 2;
                let (_, snapshot) = stored_admission_probe(&AnalysisPolicy {
                    requested_bytes_limit: middle,
                    ..funded()
                });
                if boundary_accepted(&snapshot, target) {
                    upper = middle;
                } else {
                    lower = middle + 1;
                }
            }
            let (_, accepted) = stored_admission_probe(&AnalysisPolicy {
                requested_bytes_limit: upper,
                ..funded()
            });
            assert!(boundary_accepted(&accepted, target));
            AnalysisPolicy {
                requested_bytes_limit: upper - 1,
                ..funded()
            }
        }
    };
    let db = database(CALL, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let fixture = StoredCallable::new(&db);
    let ty = Type::Callable(fixture.callable);
    let revision = salsa::plumbing::current_revision(&db);
    let (result, refused) = run_stored_admission(&prepared, ty, &policy);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: match limit {
                AdmissionLimit::Work => AnalysisIncomplete::WorkLimit,
                AdmissionLimit::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
            },
            completed: (),
        })
    );
    assert!(!boundary_accepted(&refused, target));
    let (before_index, before) = refused
        .events
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, event)| match event {
            LegacyEvent::Before(admission) if boundary_matches(target, admission) => {
                Some((index, admission))
            }
            _ => None,
        })
        .unwrap();
    if let Some(cursor) = before.cursor {
        let dropped = refused.events[before_index + 1..]
            .iter()
            .find_map(|event| match event {
                LegacyEvent::CursorDrop { id, state } if Some(*id) == cursor.observation_id => {
                    Some(*state)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(dropped, cursor);
    }
    assert_legacy_retirement(&refused);
    let (retry, retried) = run_stored_admission(&prepared, ty, &funded());
    assert_eq!(retry, Ok(AnalysisOutcome::Complete(())));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_legacy_retirement(&retried);
}

/// Native cancellation with nested populated cursors retires pending work before the source builder owner.
#[test]
fn stored_callable_cancellation_retires_live_receiver_cursors_before_retry() {
    let db = database(CALL, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let fixture = StoredCallable::new(&db);
    let ty = Type::Callable(fixture.callable);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = legacy_observations::Recording::start(Cancellation::PopulatedReceiver);
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, Request::LegacyVariables(ty), &funded())
    }));
    drop(recording);
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    let snapshot = legacy_observations::snapshot();
    assert!(snapshot.events.iter().any(|event| matches!(event, LegacyEvent::CancellationRequested { live_cursors, pending } if *live_cursors > 1 && *pending > 0)));
    assert_legacy_retirement(&snapshot);

    observations::reset(None);
    let recording = legacy_observations::Recording::start(Cancellation::Never);
    let retry = controlled(&prepared, Request::LegacyVariables(ty), &funded());
    drop(recording);
    assert!(
        matches!(
            retry,
            Ok(AnalysisOutcome::Complete(Value::LegacyVariables(_)))
        ),
        "{retry:?}"
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_legacy_retirement(&legacy_observations::snapshot());
}

/// A nested function-typed parameter refuses before return normalization, with a receiver cursor still live.
#[test]
fn stored_callable_descendant_refusal_keeps_its_exact_operation_and_order() {
    let db = database(CALL, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let definition = prepared
        .semantic_index()
        .expect_single_definition(function(&prepared));
    let function = infer_definition_types(&db, definition).binding_type(definition);
    let paramspec = legacy_variable(&db, "P", TypeVarKind::LegacyParamSpec);
    let nested = Type::single_callable(
        &db,
        Signature::new(
            Parameters::standard([Parameter::positional_only(None).with_annotated_type(function)]),
            Type::TypeVar(paramspec),
        ),
    );
    let env = db.program_environment();
    let receiver = legacy_variable(&db, "Receiver", TypeVarKind::LegacyTypeVar);
    let constraints = ConstraintSetBuilder::new().into_owned(|builder| {
        ConstraintSet::constrain_typevar_lower_bound(&db, &env, builder, receiver, nested)
    });
    let ty = Type::single_callable(
        &db,
        Signature::new(Parameters::empty(), Type::any())
            .with_probe_receiver_constraints(constraints),
    );
    observations::reset(None);
    let recording = legacy_observations::Recording::start(Cancellation::Never);
    let result = controlled(&prepared, Request::LegacyVariables(ty), &funded());
    drop(recording);
    assert_eq!(
        result,
        Ok(unavailable(OperationId::LegacyTypeVariables(
            LegacyTypeVarOperation::Function
        )))
    );
    let snapshot = legacy_observations::snapshot();
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| matches!(event, LegacyEvent::CursorCreated(_)))
    );
    assert_legacy_retirement(&snapshot);
}

/// Creates five nested protocol levels whose property-read callables retain receiver cursors during descent.
/// Each property also contributes write and descriptor types, followed by a sibling attribute;
/// those raw types require separate visitor scopes after the nested callable finishes.
fn stored_protocol<'db>(db: &'db TestDb) -> ProtocolInstanceType<'db> {
    let env = db.program_environment();
    let annotation = legacy_variable(db, "Annotation", TypeVarKind::LegacyTypeVar);
    let default = legacy_variable(db, "Default", TypeVarKind::LegacyTypeVar);
    let returns = legacy_variable(db, "Return", TypeVarKind::LegacyTypeVar);
    let keys: Vec<_> = (0..12)
        .map(|index| legacy_variable(db, &format!("Receiver{index}"), TypeVarKind::LegacyTypeVar))
        .collect();
    let wrap = |nested| {
        let constraints = ConstraintSetBuilder::new().into_owned(|builder| {
            keys.iter().fold(
                ConstraintSet::from_bool(builder, true),
                |constraints, key| {
                    constraints.and(db, builder, || {
                        let lower = if *key == keys[0] {
                            nested
                        } else {
                            Type::TypeVar(returns)
                        };
                        ConstraintSet::constrain_typevar_lower_bound(db, &env, builder, *key, lower)
                    })
                },
            )
        });
        let callable = Type::single_callable(
            db,
            Signature::new(
                Parameters::standard([Parameter::positional_only(None)
                    .with_annotated_type(Type::TypeVar(annotation))
                    .with_default_type(Type::TypeVar(default))]),
                Type::TypeVar(returns),
            )
            .with_probe_receiver_constraints(constraints),
        );
        let interface = ProtocolInterface::for_legacy_test(
            db,
            &env,
            [
                (
                    Name::new_static("body"),
                    LegacyProtocolTestMember::Property {
                        read: Some(callable),
                        write: Some(Type::TypeVar(default)),
                        descriptor: Some(Type::TypeVar(annotation)),
                    },
                ),
                (
                    Name::new_static("tail"),
                    LegacyProtocolTestMember::Attribute(Type::TypeVar(returns)),
                ),
            ],
        );
        ProtocolInstanceType::from_interface_source_for_test(
            db,
            ProtocolInterfaceSource::Synthesized(interface),
        )
    };
    let mut protocol = wrap(Type::TypeVar(returns));
    for _ in 1..5 {
        protocol = wrap(Type::ProtocolInstance(protocol));
    }
    protocol
}

/// Checks accepted scope entry/restoration transitions and vector destruction before the collector retires.
/// The retirement event follows the vector's drop; it does not identify individual visitor drops.
/// Returns the vector's last observed state, or `None` if its construction was never observed.
fn assert_protocol_retirement(snapshot: &legacy_observations::Snapshot) -> Option<ScopeState> {
    let mut before_scope_change = None;
    for event in &snapshot.events {
        match event {
            LegacyEvent::Before(admission)
                if matches!(
                    admission.boundary,
                    Boundary::ScopeEnter | Boundary::ScopeRestore
                ) =>
            {
                before_scope_change = Some(*admission);
            }
            LegacyEvent::Accepted {
                boundary,
                scopes: Some(after),
                ..
            } if matches!(boundary, Boundary::ScopeEnter | Boundary::ScopeRestore) => {
                let before = before_scope_change.take().unwrap();
                assert_eq!(before.boundary, *boundary);
                let before = before.scopes.unwrap();
                if *boundary == Boundary::ScopeEnter {
                    assert_eq!(after.len, before.len + 1);
                    assert!(after.capacity >= after.len);
                } else {
                    assert_eq!(after.len + 1, before.len);
                    assert_eq!(after.capacity, before.capacity);
                }
            }
            _ => {}
        }
    }
    assert_eq!(snapshot.scopes, None);
    let retired = snapshot
        .events
        .iter()
        .enumerate()
        .find_map(|(index, event)| {
            if let LegacyEvent::ScopeStorageRetired { state } = event {
                Some((index, *state))
            } else {
                None
            }
        });
    if let Some((retirement, _)) = retired {
        let collector = snapshot
            .events
            .iter()
            .position(|event| matches!(event, LegacyEvent::CollectorDrop { .. }))
            .unwrap();
        assert!(retirement < collector);
        assert!(
            snapshot.events[retirement + 1..]
                .iter()
                .all(|event| { !matches!(event, LegacyEvent::ScopeStorageRetired { .. }) })
        );
    }
    assert_legacy_retirement(snapshot);
    retired.map(|(_, state)| state)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProtocolRepresentation {
    Instance,
    Subclass,
}

/// Both protocol entry forms preserve stored raw-type order, balance fresh scopes, and retire owners.
#[test_case::test_case(ProtocolRepresentation::Instance; "protocol instance")]
#[test_case::test_case(ProtocolRepresentation::Subclass; "protocol subclass")]
fn stored_protocol_collection_restores_nested_scopes(representation: ProtocolRepresentation) {
    let db = database(CALL, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let protocol = stored_protocol(&db);
    let ty = match representation {
        ProtocolRepresentation::Instance => Type::ProtocolInstance(protocol),
        ProtocolRepresentation::Subclass => SubclassOfType::from_protocol(protocol),
    };
    let mut expected = FxOrderSet::default();
    ty.find_legacy_typevars(&db, &db.program_environment(), None, &mut expected);
    assert_eq!(expected.len(), 15);
    observations::reset(None);
    let recording = legacy_observations::Recording::start(Cancellation::Never);
    let result = controlled(&prepared, Request::LegacyVariables(ty), &funded());
    drop(recording);
    let Ok(AnalysisOutcome::Complete(Value::LegacyVariables(actual))) = result else {
        panic!("stored protocol collection: {result:?}");
    };
    assert_eq!(
        actual.iter().copied().collect::<Vec<_>>(),
        expected.iter().copied().collect::<Vec<_>>()
    );
    let snapshot = legacy_observations::snapshot();
    let enters = snapshot
        .events
        .iter()
        .filter(|event| {
            matches!(
                event,
                LegacyEvent::Accepted {
                    boundary: Boundary::ScopeEnter,
                    ..
                }
            )
        })
        .count();
    let restores = snapshot
        .events
        .iter()
        .filter(|event| {
            matches!(
                event,
                LegacyEvent::Accepted {
                    boundary: Boundary::ScopeRestore,
                    ..
                }
            )
        })
        .count();
    assert_eq!(enters, 20);
    assert_eq!(restores, enters);
    assert!(snapshot.events.iter().any(|event| matches!(event,
        LegacyEvent::Accepted { boundary: Boundary::ScopeEnter, scopes: Some(state), .. }
            if state.len == 5
    )));
    assert!(snapshot.events.iter().any(|event| matches!(event,
        LegacyEvent::Accepted { boundary: Boundary::StackSpill, stack: Some(state), .. }
            if state.spilled
    )));
    assert_eq!(assert_protocol_retirement(&snapshot).unwrap().len, 0);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProtocolAdmission {
    ScopeStorageCreate,
    VisitorCreate,
    ScopeGrow,
    ScopeReuse,
    ScopeRestore,
    Member,
    Types,
    Slot,
}

impl ProtocolAdmission {
    const fn boundary(self) -> Boundary {
        match self {
            Self::ScopeStorageCreate => Boundary::ScopeStorageCreate,
            Self::VisitorCreate => Boundary::VisitorCreate,
            Self::ScopeGrow | Self::ScopeReuse => Boundary::ScopeEnter,
            Self::ScopeRestore => Boundary::ScopeRestore,
            Self::Member => Boundary::ProtocolMember,
            Self::Types => Boundary::ProtocolTypes,
            Self::Slot => Boundary::ProtocolSlot,
        }
    }

    /// Selects operations with enclosing scopes still live, except initial vector construction.
    fn matches(self, admission: &legacy_observations::Admission) -> bool {
        if admission.boundary != self.boundary() {
            return false;
        }
        match self {
            Self::ScopeStorageCreate => true,
            Self::ScopeGrow => admission
                .scopes
                .is_some_and(|state| state.len >= 2 && state.len == state.capacity),
            Self::ScopeReuse => admission
                .scopes
                .is_some_and(|state| state.len >= 2 && state.len < state.capacity),
            Self::VisitorCreate | Self::ScopeRestore | Self::Member | Self::Types | Self::Slot => {
                admission.scopes.is_some_and(|state| state.len >= 2)
            }
        }
    }
}

/// Reports whether the first matching protocol operation completed before any later refusal.
fn protocol_boundary_accepted(
    snapshot: &legacy_observations::Snapshot,
    target: ProtocolAdmission,
) -> bool {
    let mut selected = false;
    for event in &snapshot.events {
        match event {
            LegacyEvent::Before(admission) if target.matches(admission) => selected = true,
            LegacyEvent::Accepted { boundary, .. }
                if selected && *boundary == target.boundary() =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// Runs stored-protocol legacy-variable collection under `policy`, returning its outcome and observations.
/// Uses fresh values for each limit measurement so earlier memo reuse cannot move its boundary.
fn protocol_admission_probe(
    policy: &AnalysisPolicy,
) -> (
    Result<AnalysisOutcome<()>, AnalysisFailure>,
    legacy_observations::Snapshot,
) {
    let db = database(CALL, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    run_stored_admission(
        &prepared,
        Type::ProtocolInstance(stored_protocol(&db)),
        policy,
    )
}

/// Work and byte refusal precede protocol/scope mutation and permit a funded same-database retry.
/// Nested targets retain enclosing visitors. Before a rejected scope insertion, the incoming
/// visitor's construction is recorded; the scope vector then retires with its length unchanged.
#[test_case::test_case(ProtocolAdmission::ScopeStorageCreate, AdmissionLimit::Work; "scope storage work")]
#[test_case::test_case(ProtocolAdmission::ScopeStorageCreate, AdmissionLimit::Bytes; "scope storage bytes")]
#[test_case::test_case(ProtocolAdmission::VisitorCreate, AdmissionLimit::Work; "visitor work")]
#[test_case::test_case(ProtocolAdmission::VisitorCreate, AdmissionLimit::Bytes; "visitor bytes")]
#[test_case::test_case(ProtocolAdmission::ScopeGrow, AdmissionLimit::Work; "scope growth work")]
#[test_case::test_case(ProtocolAdmission::ScopeGrow, AdmissionLimit::Bytes; "scope growth bytes")]
#[test_case::test_case(ProtocolAdmission::ScopeReuse, AdmissionLimit::Work; "scope reuse work")]
#[test_case::test_case(ProtocolAdmission::ScopeReuse, AdmissionLimit::Bytes; "scope reuse bytes")]
#[test_case::test_case(ProtocolAdmission::ScopeRestore, AdmissionLimit::Work; "scope restoration work")]
#[test_case::test_case(ProtocolAdmission::ScopeRestore, AdmissionLimit::Bytes; "scope restoration bytes")]
#[test_case::test_case(ProtocolAdmission::Member, AdmissionLimit::Work; "member advance work")]
#[test_case::test_case(ProtocolAdmission::Member, AdmissionLimit::Bytes; "member advance bytes")]
#[test_case::test_case(ProtocolAdmission::Types, AdmissionLimit::Work; "raw types work")]
#[test_case::test_case(ProtocolAdmission::Types, AdmissionLimit::Bytes; "raw types bytes")]
#[test_case::test_case(ProtocolAdmission::Slot, AdmissionLimit::Work; "raw slot work")]
#[test_case::test_case(ProtocolAdmission::Slot, AdmissionLimit::Bytes; "raw slot bytes")]
fn stored_protocol_admission_refusal_preserves_scope_owners(
    target: ProtocolAdmission,
    limit: AdmissionLimit,
) {
    let (result, measured) = protocol_admission_probe(&funded());
    assert_eq!(result, Ok(AnalysisOutcome::Complete(())));
    let admission = measured
        .events
        .iter()
        .find_map(|event| match event {
            LegacyEvent::Before(admission) if target.matches(admission) => Some(*admission),
            _ => None,
        })
        .unwrap();
    assert!(admission.work > 0);
    let representation = match target {
        ProtocolAdmission::ScopeStorageCreate => size_of::<LegacyVisitorScopes<'_>>(),
        ProtocolAdmission::VisitorCreate
        | ProtocolAdmission::ScopeGrow
        | ProtocolAdmission::ScopeReuse => size_of::<FindLegacyTypeVarsVisitor<'_>>(),
        ProtocolAdmission::ScopeRestore => size_of::<Option<FindLegacyTypeVarsVisitor<'_>>>(),
        ProtocolAdmission::Member => size_of::<Option<(&Name, &ProtocolMemberData<'_>)>>(),
        ProtocolAdmission::Types => size_of::<[Option<Type<'_>>; 6]>(),
        ProtocolAdmission::Slot => size_of::<Option<LegacyProtocolTypeStep<'_>>>(),
    };
    assert!(admission.bytes >= representation);
    assert!(protocol_boundary_accepted(&measured, target));
    assert_eq!(assert_protocol_retirement(&measured).unwrap().len, 0);
    let policy = match limit {
        AdmissionLimit::Work => AnalysisPolicy {
            semantic_work_limit: funded().semantic_work_limit - admission.remaining_work
                + admission.work
                - 1,
            ..funded()
        },
        AdmissionLimit::Bytes => {
            let mut lower = 0;
            let mut upper = funded().requested_bytes_limit;
            while lower < upper {
                let middle = lower + (upper - lower) / 2;
                let (_, snapshot) = protocol_admission_probe(&AnalysisPolicy {
                    requested_bytes_limit: middle,
                    ..funded()
                });
                if protocol_boundary_accepted(&snapshot, target) {
                    upper = middle;
                } else {
                    lower = middle + 1;
                }
            }
            let (_, accepted) = protocol_admission_probe(&AnalysisPolicy {
                requested_bytes_limit: upper,
                ..funded()
            });
            assert!(protocol_boundary_accepted(&accepted, target));
            AnalysisPolicy {
                requested_bytes_limit: upper - 1,
                ..funded()
            }
        }
    };
    let db = database(CALL, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let ty = Type::ProtocolInstance(stored_protocol(&db));
    let revision = salsa::plumbing::current_revision(&db);
    let (result, refused) = run_stored_admission(&prepared, ty, &policy);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: match limit {
                AdmissionLimit::Work => AnalysisIncomplete::WorkLimit,
                AdmissionLimit::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
            },
            completed: (),
        })
    );
    assert!(!protocol_boundary_accepted(&refused, target));
    let (before_index, before) = refused
        .events
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, event)| match event {
            LegacyEvent::Before(admission) if target.matches(admission) => {
                Some((index, *admission))
            }
            _ => None,
        })
        .unwrap();
    let retired = assert_protocol_retirement(&refused);
    match target {
        ProtocolAdmission::ScopeStorageCreate => assert_eq!(retired, None),
        ProtocolAdmission::VisitorCreate
        | ProtocolAdmission::ScopeGrow
        | ProtocolAdmission::ScopeReuse
        | ProtocolAdmission::ScopeRestore
        | ProtocolAdmission::Member
        | ProtocolAdmission::Types
        | ProtocolAdmission::Slot => assert_eq!(retired, before.scopes),
    }
    if matches!(
        target,
        ProtocolAdmission::ScopeGrow | ProtocolAdmission::ScopeReuse
    ) {
        assert!(
            refused.events[..before_index]
                .iter()
                .rev()
                .find_map(|event| match event {
                    LegacyEvent::Accepted {
                        boundary: Boundary::VisitorCreate,
                        scopes,
                        ..
                    } => Some(*scopes),
                    _ => None,
                })
                .is_some_and(|scopes| scopes == before.scopes)
        );
    }
    let (retry, retried) = run_stored_admission(&prepared, ty, &funded());
    assert_eq!(retry, Ok(AnalysisOutcome::Complete(())));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_eq!(assert_protocol_retirement(&retried).unwrap().len, 0);
}

/// Native cancellation with nested visitors and a populated receiver cursor drops scopes and
/// pending cursors before the source builder owner; a funded retry completes on the same revision.
#[test]
fn stored_protocol_cancellation_retires_scopes_and_receiver_cursors_before_retry() {
    let db = database(CALL, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let ty = Type::ProtocolInstance(stored_protocol(&db));
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = legacy_observations::Recording::start(Cancellation::NestedProtocolReceiver);
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, Request::LegacyVariables(ty), &funded())
    }));
    drop(recording);
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    let snapshot = legacy_observations::snapshot();
    let cancelled_scope = snapshot
        .events
        .iter()
        .find_map(|event| match event {
            LegacyEvent::ProtocolCancellationRequested {
                scopes,
                cursor,
                pending,
            } if scopes.len >= 2 && cursor.seen_len > 0 && *pending > 0 => Some(*scopes),
            _ => None,
        })
        .unwrap();
    assert_eq!(assert_protocol_retirement(&snapshot), Some(cancelled_scope));
    let (retry, retried) = run_stored_admission(&prepared, ty, &funded());
    assert_eq!(retry, Ok(AnalysisOutcome::Complete(())));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_eq!(assert_protocol_retirement(&retried).unwrap().len, 0);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProtocolDescendant {
    Function,
    ParamSpec,
}

/// A stored protocol's function and ParamSpec descendants retain their exact refusal operation,
/// while enclosing scopes and a populated receiver cursor retire after the refusal.
#[test_case::test_case(ProtocolDescendant::Function; "guarded function")]
#[test_case::test_case(ProtocolDescendant::ParamSpec; "paramspec normalization")]
fn stored_protocol_descendant_refusal_keeps_its_exact_operation(descendant: ProtocolDescendant) {
    let db = database(CALL, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let env = db.program_environment();
    let (child, operation) = match descendant {
        ProtocolDescendant::Function => {
            let definition = prepared
                .semantic_index()
                .expect_single_definition(function(&prepared));
            (
                infer_definition_types(&db, definition).binding_type(definition),
                LegacyTypeVarOperation::Function,
            )
        }
        ProtocolDescendant::ParamSpec => (
            Type::TypeVar(legacy_variable(&db, "P", TypeVarKind::LegacyParamSpec)),
            LegacyTypeVarOperation::NormalizeParamSpec,
        ),
    };
    let wrap = |ty| {
        let interface = ProtocolInterface::for_legacy_test(
            &db,
            &env,
            [(
                Name::new_static("value"),
                LegacyProtocolTestMember::Attribute(ty),
            )],
        );
        Type::ProtocolInstance(ProtocolInstanceType::from_interface_source_for_test(
            &db,
            ProtocolInterfaceSource::Synthesized(interface),
        ))
    };
    let inner = wrap(child);
    let receiver = legacy_variable(&db, "Receiver", TypeVarKind::LegacyTypeVar);
    let constraints = ConstraintSetBuilder::new().into_owned(|builder| {
        ConstraintSet::constrain_typevar_lower_bound(&db, &env, builder, receiver, inner)
    });
    let ty = wrap(Type::single_callable(
        &db,
        Signature::new(Parameters::empty(), Type::any())
            .with_probe_receiver_constraints(constraints),
    ));
    observations::reset(None);
    let recording = legacy_observations::Recording::start(Cancellation::Never);
    let result = controlled(&prepared, Request::LegacyVariables(ty), &funded());
    drop(recording);
    assert_eq!(
        result,
        Ok(unavailable(OperationId::LegacyTypeVariables(operation)))
    );
    let snapshot = legacy_observations::snapshot();
    assert!(snapshot.events.iter().any(|event| matches!(event,
        LegacyEvent::CursorDrop { state, .. } if state.seen_len > 0
    )));
    assert_eq!(assert_protocol_retirement(&snapshot).unwrap().len, 2);
}

#[test]
fn deferred_signature_annotations_preserve_order_flags_and_leave_defaults_unevaluated() {
    for (version, stub, future) in [
        (PythonVersion::PY313, false, false),
        (PythonVersion::PY314, false, false),
        (PythonVersion::PY313, true, false),
        (PythonVersion::PY313, false, true),
    ] {
        let source = if future {
            format!("from __future__ import annotations\n{ANNOTATIONS}")
        } else {
            ANNOTATIONS.to_owned()
        };
        let db = database(&source, version, stub);
        let prepared = prepared(&db, stub);
        let function = function(&prepared);
        let definition = prepared.semantic_index().expect_single_definition(function);
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(false);
        let cold = capture(&db, || controlled(&prepared, Request::Deferred, &funded())).unwrap();
        drop(recording);
        let Ok(AnalysisOutcome::Complete(Value::Deferred(inference))) = cold.value else {
            panic!("{:?}", cold.value)
        };
        cold.check_root_reads().unwrap();
        assert_completed_transaction(definition, function);
        for (annotation, _) in annotations(function) {
            assert_eq!(inference.expression_type(annotation), Type::any());
            assert_eq!(
                inference.type_expression_flags(annotation),
                TypeExpressionFlags::empty()
            );
        }
        for parameter in function.parameters.iter_non_variadic_params() {
            if let Some(default) = parameter.default() {
                assert_eq!(inference.try_expression_type(default), None);
            }
        }
        let ordinary_db = database(&source, version, stub);
        let ordinary_prepared = self::prepared(&ordinary_db, stub);
        let ordinary_function = self::function(&ordinary_prepared);
        let ordinary_definition = ordinary_prepared
            .semantic_index()
            .expect_single_definition(ordinary_function);
        let ordinary = infer_deferred_types(&ordinary_db, ordinary_definition);
        for ((annotation, _), (ordinary_annotation, _)) in annotations(function)
            .into_iter()
            .zip(annotations(ordinary_function))
        {
            assert_eq!(
                inference.expression_type(annotation),
                ordinary.expression_type(ordinary_annotation)
            );
            assert_eq!(
                inference.type_expression_flags(annotation),
                ordinary.type_expression_flags(ordinary_annotation)
            );
        }
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                deferred_definition_inference_ingredient(&db),
                definition.as_id()
            )
            .is_ok()
        );
        let mut reader = db.clone();
        reader.take_salsa_events();
        assert_eq!(
            controlled(&prepared, Request::Deferred, &funded()),
            Ok(AnalysisOutcome::Complete(Value::Deferred(inference)))
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "infer_deferred_types",
            Some(definition.as_id()),
            &reader.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SubclassAnnotation {
    Object,
    Nominal,
    Protocol,
}

impl SubclassAnnotation {
    fn source(self) -> String {
        let base = match self {
            Self::Object => return "def choose(value: type[object]): ...\n".to_owned(),
            Self::Nominal => "Generic",
            Self::Protocol => "Protocol",
        };
        format!(
            "from typing import {base}, TypeVar\n\
             T = TypeVar(\"T\")\n\
             class Box({base}[T]): ...\n\
             class Leaf: ...\n\
             def choose(value: type[Box[Leaf]]): ...\n"
        )
    }

    /// Returns the generic target's expected input; object normalization has no such target.
    fn input(self) -> Option<SubclassInput> {
        match self {
            Self::Object => None,
            Self::Nominal => Some(SubclassInput::Class),
            Self::Protocol => Some(SubclassInput::Protocol),
        }
    }
}

/// Checks the outer representation and extracts the nominal or class-backed protocol alias.
/// Object normalization instead checks the `type` instance and returns `None`.
fn subclass_annotation_alias<'db>(
    db: &'db TestDb,
    kind: SubclassAnnotation,
    ty: Type<'db>,
) -> Option<GenericAlias<'db>> {
    match kind {
        SubclassAnnotation::Object => {
            let Type::NominalInstance(instance) = ty else {
                panic!("type[object] must normalize to a type instance: {ty:?}");
            };
            assert_eq!(instance.known_class(db), Some(KnownClass::Type));
            None
        }
        SubclassAnnotation::Nominal | SubclassAnnotation::Protocol => {
            let Type::SubclassOf(subclass) = ty else {
                panic!("specialized subclass annotation: {ty:?}");
            };
            let class = match (kind, subclass.subclass_of()) {
                (SubclassAnnotation::Nominal, SubclassOfInner::Class(class)) => class,
                (SubclassAnnotation::Protocol, SubclassOfInner::Protocol(protocol)) => {
                    let ProtocolInterfaceSource::Class(class) =
                        protocol.interface_source_with_fields(salsa::FieldReads::new(db))
                    else {
                        panic!("declared protocol must retain its class-backed interface");
                    };
                    *class
                }
                other => panic!("unexpected subclass annotation: {other:?}"),
            };
            let ClassType::Generic(alias) = class else {
                panic!("Box[Leaf] must retain its specialization: {class:?}");
            };
            Some(alias)
        }
    }
}

/// Reads a fixture class only after certifying its definition memo, so post-attempt assertions
/// cannot fill a missing definition result through ordinary inference.
fn subclass_annotation_class<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    name: &str,
) -> StaticClassLiteral<'db> {
    let class = prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .filter_map(Stmt::as_class_def_stmt)
        .find(|class| class.name.as_str() == name)
        .unwrap();
    let definition = prepared.semantic_index().expect_single_definition(class);
    assert!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            definition_inference_ingredient(db),
            definition.as_id(),
        )
        .is_ok()
    );
    let Some(ClassLiteral::Static(class)) =
        infer_definition_types(db, definition).original_class_type(definition)
    else {
        panic!("canonical fixture class {name}");
    };
    class
}

/// Checks the observed target's class identity and independently checks the result's canonical
/// specialization. The object case checks the canonical `type` instance and absence of that target.
fn assert_subclass_annotation_result<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    kind: SubclassAnnotation,
    ty: Type<'db>,
    journal: &Snapshot,
) {
    let env = ProgramEnvironment::from_file(prepared.program_file());
    if let Some(alias) = subclass_annotation_alias(db, kind, ty) {
        let origin = subclass_annotation_class(db, prepared, "Box");
        let leaf = subclass_annotation_class(db, prepared, "Leaf");
        assert_eq!(journal.subclass_targets, [origin.as_id()]);
        assert_eq!(journal.subclass_inputs.as_slice(), kind.input().as_slice());
        assert_eq!(alias.origin(db), origin);
        let context_ingredient = static_class_generic_context_ingredient(db);
        assert!(FinalSourceMemo::certify(db as &dyn Db, context_ingredient, origin.as_id()).is_ok());
        let context = salsa::attach(db, || {
            let db = db as &dyn Db;
            *context_ingredient.fetch(db, db.zalsa(), db.zalsa_local(), origin.as_id())
        })
        .unwrap();
        assert_eq!(context.variables(db).len(), 1);
        let specialization = alias.specialization(db);
        assert_eq!(specialization.generic_context(db), context);
        assert_eq!(specialization.materialization_kind(db), None);
        assert!(specialization.tuple(db).is_none());
        let [argument] = specialization.types(db) else {
            panic!("Box has one type argument");
        };
        let Type::NominalInstance(instance) = argument else {
            panic!("Box argument must be a Leaf instance: {argument:?}");
        };
        assert_eq!(instance.class_literal(db, &env), ClassLiteral::Static(leaf));
        assert_eq!(
            specialization,
            Specialization::new(db, context, vec![*argument].into_boxed_slice(), None, None),
        );
        assert_eq!(alias, GenericAlias::new(db, origin, specialization));
    } else {
        assert!(journal.subclass_targets.is_empty());
        assert!(journal.subclass_inputs.is_empty());
        let argument = KnownClassArgument::new(db, KnownClass::Type, env.program(db));
        let ingredient = known_class_to_instance_ingredient(db);
        assert!(FinalSourceMemo::certify(db as &dyn Db, ingredient, argument.as_id()).is_ok());
        let canonical = salsa::attach(db, || {
            let db = db as &dyn Db;
            *ingredient.fetch(db, db.zalsa(), db.zalsa_local(), argument.as_id())
        });
        assert_eq!(ty, canonical);
    }
}

/// Checks reuse of the completed deferred annotation memo and ordinary-result parity on a
/// separate database, keeping the comparison independent of the controlled database's memos.
fn assert_subclass_annotation_publication<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    kind: SubclassAnnotation,
    ty: Type<'db>,
    flags: TypeExpressionFlags,
) {
    let function = function(prepared);
    let definition = prepared.semantic_index().expect_single_definition(function);
    let annotation = function.parameters.args[0].annotation().unwrap();
    assert_eq!(flags, TypeExpressionFlags::empty());
    assert!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            deferred_definition_inference_ingredient(db),
            definition.as_id(),
        )
        .is_ok()
    );
    let mut reader = db.clone();
    reader.take_salsa_events();
    let canonical = infer_deferred_types(db, definition);
    assert_eq!(canonical.expression_type(annotation), ty);
    assert_eq!(canonical.type_expression_flags(annotation), flags);
    assert_function_query_was_not_run_by_name(
        db,
        "infer_deferred_types",
        Some(definition.as_id()),
        &reader.take_salsa_events(),
    );
    let ordinary_db = database(&kind.source(), PythonVersion::PY313, true);
    let ordinary_prepared = self::prepared(&ordinary_db, true);
    let ordinary_function = self::function(&ordinary_prepared);
    let ordinary_definition = ordinary_prepared
        .semantic_index()
        .expect_single_definition(ordinary_function);
    let ordinary_annotation = ordinary_function.parameters.args[0].annotation().unwrap();
    let ordinary = infer_deferred_types(&ordinary_db, ordinary_definition);
    let ordinary_ty = ordinary.expression_type(ordinary_annotation);
    let _ = subclass_annotation_alias(&ordinary_db, kind, ordinary_ty);
    assert_eq!(ordinary.type_expression_flags(ordinary_annotation), flags);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    assert_eq!(
        ty.display(db, &env).to_string(),
        ordinary_ty.display(&ordinary_db, &ordinary_env).to_string(),
    );
}

/// Checks that annotation attempts released their owners and allow a new preparation phase.
fn assert_subclass_annotation_cleanup(db: &TestDb) {
    assert_cleanup();
    let preparation = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        salsa::prepared_source_probe::try_with_preparation(db, || ())
    }));
    assert!(matches!(preparation, Ok(Ok(()))), "{preparation:?}");
}

/// Real parameter annotations complete cold and publish canonical subclass and object-normalization
/// results that one same-revision request reuses without rerunning annotation inference.
#[test_case::test_case(SubclassAnnotation::Object; "object normalization")]
#[test_case::test_case(SubclassAnnotation::Nominal; "nominal subclass")]
#[test_case::test_case(SubclassAnnotation::Protocol; "declared protocol subclass")]
fn subclass_parameter_annotations_publish_and_reuse_canonical_results(kind: SubclassAnnotation) {
    let db = database(&kind.source(), PythonVersion::PY313, true);
    let prepared = prepared(&db, true);
    let function = function(&prepared);
    let definition = prepared.semantic_index().expect_single_definition(function);
    let annotation = function.parameters.args[0].annotation().unwrap();
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(false);
    let cold = capture(&db, || controlled(&prepared, Request::Parameter(annotation), &funded()))
        .unwrap();
    drop(recording);
    cold.check_root_reads().unwrap();
    let cold_journal = snapshot();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_subclass_annotation_cleanup(&db);
    let Ok(AnalysisOutcome::Complete(Value::Parameter(cold_ty, cold_flags))) = cold.value else {
        panic!("{kind:?} first cold outcome: {:?}", cold.value);
    };
    eprintln!("{kind:?} first cold outcome: Complete");
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            deferred_definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .is_ok()
    );
    assert_subclass_annotation_result(&db, &prepared, kind, cold_ty, &cold_journal);

    let mut reader = db.clone();
    reader.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start(false);
    let retry = capture(&db, || controlled(&prepared, Request::Parameter(annotation), &funded()))
        .unwrap();
    drop(recording);
    retry.check_root_reads().unwrap();
    let retry_journal = snapshot();
    let events = reader.take_salsa_events();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_subclass_annotation_cleanup(&db);
    let Ok(AnalysisOutcome::Complete(Value::Parameter(ty, flags))) = retry.value else {
        panic!("{kind:?} single same-revision retry: {:?}", retry.value);
    };
    assert_eq!((ty, flags), (cold_ty, cold_flags));
    assert!(retry_journal.started.is_empty());
    assert!(retry_journal.subclass_targets.is_empty());
    assert!(retry_journal.subclass_inputs.is_empty());
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_deferred_types",
        Some(definition.as_id()),
        &events,
    );
    assert_subclass_annotation_publication(&db, &prepared, kind, ty, flags);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_subclass_annotation_cleanup(&db);
}

/// A cancellation request before ClassSubclass construction remains masked through deferred-query
/// publication. Cancellation is delivered afterward, and one same-revision retry reuses the result.
#[test_case::test_case(SubclassAnnotation::Nominal; "nominal subclass")]
#[test_case::test_case(SubclassAnnotation::Protocol; "declared protocol subclass")]
fn subclass_parameter_masked_cancellation_publishes_and_reuses_the_actual_target(
    kind: SubclassAnnotation,
) {
    let db = database(&kind.source(), PythonVersion::PY313, true);
    let prepared = prepared(&db, true);
    let function = function(&prepared);
    let definition = prepared.semantic_index().expect_single_definition(function);
    let annotation = function.parameters.args[0].annotation().unwrap();
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(false);
    CANCEL_SUBCLASS.set(true);
    let interrupted = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, Request::Parameter(annotation), &funded())
    }));
    let cancellation_requested = !CANCEL_SUBCLASS.get();
    drop(recording);
    let interrupted_journal = snapshot();
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_subclass_annotation_cleanup(&db);
    assert!(
        matches!(interrupted, Err(salsa::Cancelled::Local)),
        "{kind:?} first cold outcome: {interrupted:?}",
    );
    assert!(cancellation_requested);
    assert_eq!(
        interrupted_journal.subclass_inputs.as_slice(),
        kind.input().as_slice(),
    );
    assert_completed_transaction(definition, function);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            deferred_definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .is_ok()
    );
    let mut reader = db.clone();
    reader.take_salsa_events();
    let canonical = infer_deferred_types(&db, definition);
    let canonical_ty = canonical.expression_type(annotation);
    let canonical_flags = canonical.type_expression_flags(annotation);
    assert_eq!(canonical_flags, TypeExpressionFlags::empty());
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_deferred_types",
        Some(definition.as_id()),
        &reader.take_salsa_events(),
    );
    assert_subclass_annotation_result(&db, &prepared, kind, canonical_ty, &interrupted_journal);
    reader.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start(false);
    let retry = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, Request::Parameter(annotation), &funded())
    }));
    drop(recording);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_subclass_annotation_cleanup(&db);
    let Ok(Ok(AnalysisOutcome::Complete(Value::Parameter(ty, flags)))) = retry else {
        panic!("{kind:?} single same-revision retry: {retry:?}");
    };
    assert_eq!((ty, flags), (canonical_ty, canonical_flags));
    let retry_journal = snapshot();
    assert!(retry_journal.started.is_empty());
    assert!(retry_journal.subclass_targets.is_empty());
    assert!(retry_journal.subclass_inputs.is_empty());
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_deferred_types",
        Some(definition.as_id()),
        &reader.take_salsa_events(),
    );
    assert_subclass_annotation_publication(&db, &prepared, kind, ty, flags);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_subclass_annotation_cleanup(&db);
}

/// Missing deferred expression results retain unknown types and empty annotation flags.
#[test]
fn signature_selector_preserves_missing_types() {
    let db = database(ANNOTATIONS, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let function = function(&prepared);
    let parameter = &function.parameters.posonlyargs[0];
    let annotation = parameter.annotation().unwrap();
    assert_eq!(
        controlled(&prepared, Request::Parameter(annotation), &funded()),
        Ok(AnalysisOutcome::Complete(Value::Parameter(
            Type::any(),
            TypeExpressionFlags::empty()
        )))
    );
    // Deferred annotations leave default expressions absent from their canonical result.
    assert_eq!(
        controlled(
            &prepared,
            Request::Parameter(parameter.default().unwrap()),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(Value::Parameter(
            Type::unknown(),
            TypeExpressionFlags::empty()
        )))
    );
    assert_cleanup();
}

/// A cold PEP 695 parameter read publishes its complete scope and restores annotation state.
#[test]
fn cold_pep695_parameter_annotation_publishes_its_canonical_scope() {
    let db = database(
        "def choose[T](value: T) -> T:\n    pass\n",
        PythonVersion::PY313,
        false,
    );
    let prepared = self::prepared(&db, false);
    let function = self::function(&prepared);
    let definition = prepared.semantic_index().expect_single_definition(function);
    let annotation = function.parameters.args[0].annotation().unwrap();
    let scope = prepared.semantic_index().scope_id(
        prepared
            .semantic_index()
            .try_expression_scope_id(annotation)
            .unwrap(),
    );
    let mut reader = db.clone();
    reader.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start(false);
    let cold = capture(&db, || {
        controlled(&prepared, Request::Parameter(annotation), &funded())
    })
    .unwrap();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(Value::Parameter(ty, flags))) = cold.value else {
        panic!("{:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    let Type::TypeVar(variable) = ty else {
        panic!("{ty:?}");
    };
    assert_eq!(variable.name(&db).as_str(), "T");
    assert_eq!(
        variable.binding_context(&db),
        BindingContext::Definition(definition)
    );
    assert_eq!(flags, TypeExpressionFlags::empty());
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            scope_inference_ingredient(&db),
            scope.as_id()
        )
        .is_ok()
    );
    assert_completed_transaction(definition, function);
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_deferred_types",
        Some(definition.as_id()),
        &reader.take_salsa_events(),
    );
    assert_cleanup();
}

/// Looks up a prepared type-parameter definition without requesting its inferred declaration.
fn type_parameter_definition<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    parameter: &ast::TypeParam,
) -> Definition<'db> {
    match parameter {
        ast::TypeParam::TypeVar(node) => prepared.semantic_index().expect_single_definition(node),
        ast::TypeParam::ParamSpec(node) => prepared.semantic_index().expect_single_definition(node),
        ast::TypeParam::TypeVarTuple(node) => {
            prepared.semantic_index().expect_single_definition(node)
        }
    }
}

/// One cold context request completes all three canonical declaration kinds in source order.
#[test]
fn cold_pep695_context_uses_canonical_declarations_in_source_order() {
    let db = database(
        "def choose[T, *Ts, **P](): ...\n",
        PythonVersion::PY313,
        false,
    );
    let prepared = prepared(&db, false);
    let function = function(&prepared);
    let owner = prepared.semantic_index().expect_single_definition(function);
    let parameters = function.type_params.as_deref().unwrap();
    let mut reader = db.clone();
    reader.take_salsa_events();
    observations::reset(None);
    let cold = capture(&db, || {
        controlled(&prepared, Request::Pep695Context, &funded())
    })
    .unwrap();
    let Ok(AnalysisOutcome::Complete(Value::Pep695Context(context))) = cold.value else {
        panic!("{:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    let variables = context.variables(&db).collect::<Vec<_>>();
    assert_eq!(variables.len(), 3);
    let expected = [
        ("T", TypeVarKind::Pep695TypeVar),
        ("Ts", TypeVarKind::Pep695TypeVarTuple),
        ("P", TypeVarKind::Pep695ParamSpec),
    ];
    for ((parameter, variable), (name, kind)) in parameters.iter().zip(&variables).zip(expected) {
        let definition = type_parameter_definition(&prepared, parameter);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                definition_inference_ingredient(&db),
                definition.as_id()
            )
            .is_ok()
        );
        let inference = infer_definition_types(&db, definition);
        let declared = Type::KnownInstance(KnownInstanceType::TypeVar(variable.typevar(&db)));
        assert_eq!(inference.completed_binding(definition), Some(declared));
        let declaration = inference.completed_declaration(definition).unwrap();
        assert_eq!(declaration.inner_type(), declared);
        assert_eq!(declaration.origin(), crate::types::TypeOrigin::Inferred);
        assert!(declaration.qualifiers().is_empty());
        assert_eq!(inference.binding_scan_len(definition), 1);
        assert_eq!(inference.declaration_scan_len(definition), 1);
        assert!(inference.extra.is_none());
        assert_eq!(variable.name(&db).as_str(), name);
        assert_eq!(variable.kind(&db), kind);
        assert_eq!(
            variable.typevar(&db),
            TypeVarInstance::new(&db, variable.typevar(&db).identity(&db), None, None, None)
        );
        assert_eq!(variable.typevar(&db).definition(&db), Some(definition));
        assert_eq!(
            variable.binding_context(&db),
            BindingContext::Definition(owner)
        );
    }
    let events = reader.take_salsa_events();
    for query in [
        "infer_deferred_types",
        "infer_scope_types_impl",
        "infer_expression_types_impl",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    // Ordinary construction reads the completed canonical declarations; it adds no cold inference.
    assert_eq!(
        context,
        GenericContext::from_type_params(&db, prepared.semantic_index(), owner, parameters)
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_definition_types",
        None,
        &reader.take_salsa_events(),
    );
    assert_cleanup();
}

/// Identifies the lazy descriptor retained by the header fixtures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LazyHeader {
    UpperBound,
    Constraints,
    Default,
}

/// A completed header retains lazy metadata and its type-parameter definition as the deferred owner.
/// Requesting `value: None` requires completion of that annotation's containing scope, so its deferred
/// header work reaches the unavailable unresolved-reference diagnostic and leaves the scope unpublished.
#[test_case::test_case("T: Missing", TypeVarKind::Pep695TypeVar, LazyHeader::UpperBound; "upper bound")]
#[test_case::test_case("T: (MissingA, MissingB)", TypeVarKind::Pep695TypeVar, LazyHeader::Constraints; "constraints")]
#[test_case::test_case("T = Missing", TypeVarKind::Pep695TypeVar, LazyHeader::Default; "typevar default")]
#[test_case::test_case("**P = [Missing]", TypeVarKind::Pep695ParamSpec, LazyHeader::Default; "paramspec default")]
#[test_case::test_case("*Ts = *tuple[Missing]", TypeVarKind::Pep695TypeVarTuple, LazyHeader::Default; "typevartuple default")]
fn pep695_headers_defer_annotations_until_complete_scope_inference(
    parameter: &str,
    kind: TypeVarKind,
    lazy: LazyHeader,
) {
    let source = format!("def choose[{parameter}](value: None): ...\n");
    let db = database(&source, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let function = function(&prepared);
    let parameter = &function.type_params.as_deref().unwrap().type_params[0];
    let definition = type_parameter_definition(&prepared, parameter);
    let mut reader = db.clone();
    reader.take_salsa_events();
    observations::reset(None);
    let cold = capture(&db, || {
        controlled(&prepared, Request::Pep695Context, &funded())
    })
    .unwrap();
    let Ok(AnalysisOutcome::Complete(Value::Pep695Context(context))) = cold.value else {
        panic!("{:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    let variable = context.variables(&db).next().unwrap().typevar(&db);
    let (bound, default) = match lazy {
        LazyHeader::UpperBound => (
            Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
            None,
        ),
        LazyHeader::Constraints => (
            Some(TypeVarBoundOrConstraintsEvaluation::LazyConstraints),
            None,
        ),
        LazyHeader::Default => (None, Some(TypeVarDefaultEvaluation::Lazy)),
    };
    assert_eq!(variable.kind(&db), kind);
    assert_eq!(
        variable,
        TypeVarInstance::new(&db, variable.identity(&db), bound, None, default)
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .is_ok()
    );
    let declaration = infer_definition_types(&db, definition);
    let Some(DefinitionInferenceExtra::Deferred(deferred)) = declaration.extra.as_deref() else {
        panic!("{declaration:?}");
    };
    assert_eq!(&**deferred, &[definition]);
    let events = reader.take_salsa_events();
    for query in [
        "infer_deferred_types",
        "infer_scope_types_impl",
        "infer_expression_types_impl",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_cleanup();

    let annotation = function.parameters.args[0].annotation().unwrap();
    let scope = prepared.semantic_index().scope_id(
        prepared
            .semantic_index()
            .try_expression_scope_id(annotation)
            .unwrap(),
    );
    observations::reset(None);
    let recording = Recording::start(false);
    assert_eq!(
        controlled(&prepared, Request::Parameter(annotation), &funded()),
        Ok(unavailable(OperationId::UnresolvedReference))
    );
    drop(recording);
    assert_completed_transaction(
        prepared.semantic_index().expect_single_definition(function),
        function,
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            scope_inference_ingredient(&db),
            scope.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            deferred_definition_inference_ingredient(&db),
            definition.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_cleanup();
}

/// Invalid constraint counts refuse at their diagnostic before a declaration or context publishes.
#[test_case::test_case("()"; "zero constraints")]
#[test_case::test_case("(Missing,)"; "one constraint")]
#[test_case::test_case("(Missing,) = MissingDefault"; "invalid constraints with default")]
fn pep695_invalid_constraint_count_preserves_its_diagnostic_refusal(constraints: &str) {
    let source = format!("def choose[T: {constraints}](): ...\n");
    let db = database(&source, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let parameter = &function(&prepared)
        .type_params
        .as_deref()
        .unwrap()
        .type_params[0];
    let definition = type_parameter_definition(&prepared, parameter);
    observations::reset(None);
    assert_eq!(
        controlled(&prepared, Request::Pep695Context, &funded()),
        Ok(unavailable(
            OperationId::TypeParameterConstraintCountDiagnostic
        ))
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_cleanup();
}

/// Selects one independent cumulative resource limit without reducing the other allowance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Pep695Limit {
    Work,
    Bytes,
}

/// Selects the production entry whose cold admission is interrupted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Pep695Entry {
    Declaration,
    Context,
    Scope,
}

/// A tiny independent work or byte allowance refuses before publishing a cold PEP 695 result.
#[test_case::test_case(Pep695Entry::Declaration, Pep695Limit::Work; "declaration work")]
#[test_case::test_case(Pep695Entry::Declaration, Pep695Limit::Bytes; "declaration bytes")]
#[test_case::test_case(Pep695Entry::Context, Pep695Limit::Work; "context work")]
#[test_case::test_case(Pep695Entry::Context, Pep695Limit::Bytes; "context bytes")]
#[test_case::test_case(Pep695Entry::Scope, Pep695Limit::Work; "scope work")]
#[test_case::test_case(Pep695Entry::Scope, Pep695Limit::Bytes; "scope bytes")]
fn pep695_cold_limits_leave_canonical_results_unpublished(entry: Pep695Entry, limit: Pep695Limit) {
    let db = database(
        "def choose[T](value: T) -> T: ...\n",
        PythonVersion::PY313,
        false,
    );
    let prepared = prepared(&db, false);
    let function = function(&prepared);
    let parameter = &function.type_params.as_deref().unwrap().type_params[0];
    let definition = type_parameter_definition(&prepared, parameter);
    let annotation = function.parameters.args[0].annotation().unwrap();
    let scope = prepared.semantic_index().scope_id(
        prepared
            .semantic_index()
            .try_expression_scope_id(annotation)
            .unwrap(),
    );
    let request = match entry {
        Pep695Entry::Declaration => Request::TypeParameter(definition),
        Pep695Entry::Context => Request::Pep695Context,
        Pep695Entry::Scope => Request::Parameter(annotation),
    };
    let (policy, reason) = match limit {
        Pep695Limit::Work => (
            AnalysisPolicy {
                semantic_work_limit: 1,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
        ),
        Pep695Limit::Bytes => (
            AnalysisPolicy {
                requested_bytes_limit: 1,
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    };
    observations::reset(None);
    assert_eq!(
        controlled(&prepared, request, &policy),
        Ok(AnalysisOutcome::Incomplete {
            reason,
            completed: ()
        })
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            scope_inference_ingredient(&db),
            scope.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_cleanup();
}

/// Refusal in a later annotation restores binding, flags, deferred state, and incoming cache presence.
#[test]
fn pep695_annotation_refusal_restores_the_outer_transaction() {
    let db = database(
        "def choose[T](first: T, second: \"lambda: None\") -> T: ...\n",
        PythonVersion::PY313,
        false,
    );
    let prepared = prepared(&db, false);
    let function = function(&prepared);
    let definition = prepared.semantic_index().expect_single_definition(function);
    let annotation = function.parameters.args[1].annotation().unwrap();
    let scope = prepared.semantic_index().scope_id(
        prepared
            .semantic_index()
            .try_expression_scope_id(annotation)
            .unwrap(),
    );
    observations::reset(None);
    let recording = Recording::start(false);
    assert_eq!(
        controlled(&prepared, Request::Parameter(annotation), &funded()),
        Ok(unavailable(OperationId::TypeExpressionLegacy))
    );
    drop(recording);
    let journal = snapshot();
    assert_eq!(journal.started.len(), 1);
    assert_eq!(journal.restored, journal.started);
    assert!(journal.completed.is_empty());
    assert_eq!(journal.inferred.len(), 2);
    assert_eq!(journal.entered.last().unwrap().range, annotation.range());
    assert_eq!(
        journal.entered.last().unwrap().state.binding,
        Some(definition.as_id())
    );
    assert!(
        journal
            .entered
            .last()
            .unwrap()
            .state
            .flags
            .contains(InferenceFlags::IN_PARAMETER_ANNOTATION)
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            scope_inference_ingredient(&db),
            scope.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_cleanup();
}

/// A refused scope keeps its completed header reusable on one explicit funded retry at the same revision.
#[test]
fn pep695_scope_work_refusal_reuses_its_canonical_header_on_retry() {
    let source = "def choose[T](value: T) -> T: ...\n";
    let measured = database(source, PythonVersion::PY313, false);
    let measured_prepared = prepared(&measured, false);
    let measured_annotation = function(&measured_prepared).parameters.args[0]
        .annotation()
        .unwrap();
    observations::reset(None);
    let recording = Recording::start(false);
    assert!(matches!(
        controlled(
            &measured_prepared,
            Request::Parameter(measured_annotation),
            &funded()
        ),
        Ok(AnalysisOutcome::Complete(Value::Parameter(..)))
    ));
    drop(recording);
    // The return annotation `T` precedes the parameter annotation; resolving it completes
    // T's canonical declaration. A separate measurement database preserves cold dependencies
    // in the later limited attempt while placing its refusal after that declaration completes.
    //
    let policy = AnalysisPolicy {
        semantic_work_limit: funded().semantic_work_limit - snapshot().inferred[0].remaining,
        ..funded()
    };
    assert_cleanup();

    let db = database(source, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let function = function(&prepared);
    let definition = prepared.semantic_index().expect_single_definition(function);
    let parameter = type_parameter_definition(
        &prepared,
        &function.type_params.as_deref().unwrap().type_params[0],
    );
    let annotation = function.parameters.args[0].annotation().unwrap();
    let scope = prepared.semantic_index().scope_id(
        prepared
            .semantic_index()
            .try_expression_scope_id(annotation)
            .unwrap(),
    );
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = Recording::start(false);
    assert_eq!(
        controlled(&prepared, Request::Parameter(annotation), &policy),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        })
    );
    drop(recording);
    let journal = snapshot();
    assert_eq!(journal.started.len(), 1);
    assert_eq!(journal.restored, journal.started);
    assert!(journal.completed.is_empty());
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            scope_inference_ingredient(&db),
            scope.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            parameter.as_id()
        )
        .is_ok()
    );
    assert_cleanup();

    let mut reader = db.clone();
    reader.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start(false);
    assert!(matches!(
        controlled(&prepared, Request::Parameter(annotation), &funded()),
        Ok(AnalysisOutcome::Complete(Value::Parameter(
            Type::TypeVar(_),
            _
        )))
    ));
    drop(recording);
    assert_completed_transaction(definition, function);
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_definition_types",
        Some(parameter.as_id()),
        &reader.take_salsa_events(),
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            scope_inference_ingredient(&db),
            scope.as_id()
        )
        .is_ok()
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

/// Masked cancellation preserves a completed annotation scope for one same-revision retry.
/// The annotation child suspends, and its drop marker precedes the transaction-completion marker.
/// Salsa delivers cancellation after publication, and the reused scope matches ordinary inference.
#[test]
fn pep695_pending_annotation_cancellation_reuses_completed_scope() {
    let source = "class Leaf: pass\ndef choose[T](first: T, second: Leaf) -> T: ...\n";
    let db = database(source, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let function = function(&prepared);
    let definition = prepared.semantic_index().expect_single_definition(function);
    let annotation = function.parameters.args[1].annotation().unwrap();
    let scope = prepared.semantic_index().scope_id(
        prepared
            .semantic_index()
            .try_expression_scope_id(annotation)
            .unwrap(),
    );
    let Stmt::ClassDef(class) = &prepared.parsed_module().syntax().body[0] else {
        panic!("fixture annotation class");
    };
    let child = prepared.semantic_index().expect_single_definition(class);
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    observations::cancel_definition_creation(child.as_id());
    lifetime_observations::reset();
    let recording = Recording::start(false);
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled(&prepared, Request::Parameter(annotation), &funded())
    }));
    drop(recording);
    lifetime_observations::stop();
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    let journal = snapshot();
    assert_completed_transaction(definition, function);
    assert_eq!(journal.completed_after_guard_events.len(), 1);
    assert!(journal.type_parameter_pending > 0);
    assert!(journal.pending_annotations.iter().any(|pending| {
        pending.range == annotation.range()
            && pending.state.binding == Some(definition.as_id())
            && pending
                .state
                .flags
                .contains(InferenceFlags::IN_PARAMETER_ANNOTATION)
    }));
    let lifetimes = lifetime_observations::snapshot();
    assert!(!lifetimes.overflowed, "{lifetimes:?}");
    assert_eq!(lifetimes.active_scopes, 0, "{lifetimes:?}");
    let events = &lifetimes.events[..lifetimes.count];
    let child_dropped = events.iter().position(|event| matches!(event, Some(lifetime_observations::Event::SourceChildDropped { definition: Some(actual) }) if *actual == child.as_id())).unwrap();
    assert!(
        child_dropped < journal.completed_after_guard_events[0],
        "{journal:?}; {lifetimes:?}"
    );
    assert!(
        events[journal.completed_after_guard_events[0]..]
            .iter()
            .any(|event| matches!(
                event,
                Some(lifetime_observations::Event::SourceChildDropped { definition: None })
            ))
    );
    for event in events {
        if let Some(lifetime_observations::Event::StorageDropped {
            outstanding_removal_weights,
            ..
        }) = event
        {
            assert_eq!(*outstanding_removal_weights, [0; 3]);
        }
    }
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            scope_inference_ingredient(&db),
            scope.as_id()
        )
        .is_ok()
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            child.as_id(),
        )
        .is_ok()
    );
    assert_cleanup();

    let preparation = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        salsa::prepared_source_probe::try_with_preparation(&db, || ())
    }));
    assert!(matches!(preparation, Ok(Ok(()))), "{preparation:?}");
    let mut reader = db.clone();
    reader.take_salsa_events();
    observations::reset(None);
    let recording = Recording::start(false);
    assert!(matches!(
        controlled(&prepared, Request::Parameter(annotation), &funded()),
        Ok(AnalysisOutcome::Complete(Value::Parameter(..)))
    ));
    drop(recording);
    assert!(snapshot().started.is_empty());
    let events = reader.take_salsa_events();
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_scope_types_impl",
        Some(scope.as_id()),
        &events,
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_definition_types",
        Some(child.as_id()),
        &events,
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            scope_inference_ingredient(&db),
            scope.as_id()
        )
        .is_ok()
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();

    // Compare the completed memo with the ordinary producer after retry, so the
    // ordinary computation cannot supply the result used by the interrupted attempt.
    //
    let canonical = infer_scope_types(&db, scope, TypeContext::default());
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = TypeInferenceBuilder::new(
        &db,
        &env,
        InferenceRegion::Scope(scope, TypeContext::default()),
        prepared.program_file().file(&db),
        prepared.program_file(),
        prepared.semantic_index(),
        prepared.parsed_module(),
    )
    .finish_scope();
    assert_eq!(canonical, &ordinary);
}

#[test]
fn unsupported_return_annotation_restores_the_outer_transaction_before_retry() {
    let source = "from typing import Any\ndef choose(value: Any) -> \"lambda: None\":\n    pass\n";
    let db = database(source, PythonVersion::PY313, false);
    let prepared = prepared(&db, false);
    let function = function(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        observations::reset(None);
        let recording = Recording::start(false);
        assert_eq!(
            controlled(&prepared, Request::Deferred, &funded()),
            Ok(unavailable(OperationId::TypeExpressionLegacy))
        );
        drop(recording);
        let journal = snapshot();
        assert_eq!(journal.started.len(), 1);
        assert_eq!(journal.restored, journal.started);
        assert!(journal.completed.is_empty());
        assert_eq!(journal.entered.len(), 1);
        assert_eq!(
            journal.entered[0].range,
            function.returns.as_ref().unwrap().range()
        );
        assert!(journal.inferred.is_empty());
        assert_cleanup();
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

#[test]
fn annotation_interruption_restores_outer_state_and_retries_at_the_same_revision() {
    let measured = database(ANNOTATIONS, PythonVersion::PY313, false);
    let measured_prepared = prepared(&measured, false);
    observations::reset(None);
    let recording = Recording::start(false);
    assert!(matches!(
        controlled(&measured_prepared, Request::Deferred, &funded()),
        Ok(AnalysisOutcome::Complete(Value::Deferred(_)))
    ));
    drop(recording);
    let limited = AnalysisPolicy {
        semantic_work_limit: funded().semantic_work_limit - snapshot().inferred[0].remaining,
        ..funded()
    };
    assert_cleanup();
    for interruption in [
        Interruption::WorkLimit,
        Interruption::LocalCancellation,
        Interruption::ColdDependencyCancellation,
    ] {
        let cancel = !matches!(interruption, Interruption::WorkLimit);
        let cold_dependency = matches!(interruption, Interruption::ColdDependencyCancellation);
        let source = if cold_dependency {
            format!(
                "from leaf import Leaf\n{}",
                ANNOTATIONS.replacen("first: Any", "first: Leaf", 1)
            )
        } else {
            ANNOTATIONS.to_owned()
        };
        let mut db = database(&source, PythonVersion::PY313, false);
        if cold_dependency {
            db.write_file("src/leaf.py", "class Leaf:\n    pass\n")
                .unwrap();
        }
        let prepared = prepared(&db, false);
        let definition = prepared
            .semantic_index()
            .expect_single_definition(function(&prepared));
        let revision = salsa::plumbing::current_revision(&db);
        observations::reset(None);
        let recording = Recording::start(cancel);
        let policy = if cancel { funded() } else { limited };
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, Request::Deferred, &policy)
        }));
        drop(recording);
        match result {
            Err(salsa::Cancelled::Local) if cancel => {}
            Ok(result) if !cancel => assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            ),
            other => panic!("{other:?}"),
        }
        let journal = snapshot();
        assert_eq!(journal.started.len(), 1);
        assert_eq!(journal.cancellation_check_returned, cancel);
        if matches!(interruption, Interruption::LocalCancellation) {
            assert_eq!(journal.completed, journal.started);
            assert!(journal.restored.is_empty());
        } else {
            assert_eq!(journal.restored, journal.started);
            assert!(journal.completed.is_empty());
            assert_eq!(journal.inferred.len(), 1);
        }
        assert_cleanup();
        let preparation = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            salsa::prepared_source_probe::try_with_preparation(&db, || ())
        }));
        assert!(matches!(preparation, Ok(Ok(()))), "{preparation:?}");
        observations::reset(None);
        let retry = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled(&prepared, Request::Deferred, &funded())
        }));
        assert!(
            matches!(retry, Ok(Ok(AnalysisOutcome::Complete(Value::Deferred(_))))),
            "{retry:?}"
        );
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                deferred_definition_inference_ingredient(&db),
                definition.as_id()
            )
            .is_ok()
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_cleanup();
    }
}

/// Records completed return-scoping stages without changing admission or polling behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum ReturnScopeStage {
    Locations,
    MovedVariable,
    RetainedRenamings,
    RetainedReplacements,
}

thread_local! {
    static RETURN_SCOPE_RECORDING: Cell<bool> = const { Cell::new(false) };
    static RETURN_SCOPE_REMAINING: Cell<[Option<usize>; 4]> = const { Cell::new([None; 4]) };
}

/// Records the first completed occurrence of `stage`, while its newly retained state is live.
pub(in crate::types::infer) fn return_scope_stage(db: &dyn Db, stage: ReturnScopeStage) {
    if RETURN_SCOPE_RECORDING.get() {
        let mut remaining = RETURN_SCOPE_REMAINING.get();
        let slot = &mut remaining[stage as usize];
        if slot.is_none() {
            *slot = salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
        }
        RETURN_SCOPE_REMAINING.set(remaining);
    }
}

/// Enables one fixed-size observation record for a controlled signature request.
#[derive(Debug)]
struct ReturnScopeRecording;

impl ReturnScopeRecording {
    fn start() -> Self {
        assert!(!RETURN_SCOPE_RECORDING.replace(true));
        RETURN_SCOPE_REMAINING.set([None; 4]);
        Self
    }
}

impl Drop for ReturnScopeRecording {
    fn drop(&mut self) {
        RETURN_SCOPE_RECORDING.set(false);
    }
}

/// Selects whether a function type variable occurs outside, within, or across returned callables.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReturnScopeFixture {
    Returned,
    OutsideParameter,
    OutsideAfter,
    OutsideBefore,
    Repeated,
    Nested,
    Unknown,
}

impl ReturnScopeFixture {
    /// Supplies source annotations whose canonical inference creates the types under inspection.
    const fn source(self) -> &'static str {
        match self {
            Self::Returned => {
                "from typing import Callable\ndef choose[T]() -> Callable[[T], T]: ...\n"
            }
            Self::OutsideParameter => {
                "from typing import Callable\ndef choose[T](value: T) -> Callable[[T], T]: ...\n"
            }
            Self::OutsideAfter => {
                "from typing import Callable\ndef choose[T]() -> tuple[Callable[[], T], T]: ...\n"
            }
            Self::OutsideBefore => {
                "from typing import Callable\ndef choose[T]() -> tuple[T, Callable[[], T]]: ...\n"
            }
            Self::Repeated => {
                "from typing import Callable\ndef choose[T]() -> tuple[Callable[[T], T], Callable[[T], T]]: ...\n"
            }
            Self::Nested => {
                "from typing import Callable\ndef choose[T]() -> Callable[[Callable[[T], T]], T]: ...\n"
            }
            Self::Unknown => "def choose[T](): ...\n",
        }
    }
}

/// Extracts the fixture's sole overload, so assertions do not silently ignore additional signatures.
fn return_scope_signature<'a, 'db>(signatures: &'a CallableSignature<'db>) -> &'a Signature<'db> {
    let [signature] = signatures.overloads.as_slice() else {
        panic!("fixture must have one overload: {signatures:?}");
    };
    signature
}

/// Extracts the fixture's callable annotation without invoking callable conversion or inference.
fn return_scope_callable(ty: Type<'_>) -> CallableType<'_> {
    let Type::Callable(callable) = ty else {
        panic!("fixture callable annotation: {ty:?}");
    };
    callable
}

/// Extracts the two stored tuple elements after the controlled request has ended.
fn return_scope_pair<'db>(db: &'db TestDb, ty: Type<'db>) -> [Type<'db>; 2] {
    let Some(tuple) = ty.exact_tuple_instance_spec(db) else {
        panic!("fixture tuple annotation: {ty:?}");
    };
    let mut elements = tuple.fixed_elements();
    assert_eq!(elements.len(), 2);
    let Some(first) = elements.next() else {
        panic!("fixture first tuple element: {ty:?}");
    };
    let Some(second) = elements.next() else {
        panic!("fixture second tuple element: {ty:?}");
    };
    [*first, *second]
}

/// Checks that a context contains exactly the expected variable bound by the fixture function.
fn return_scope_variable<'db>(
    db: &'db TestDb,
    signature: &Signature<'db>,
    definition: Definition<'db>,
    name: &str,
) -> BoundTypeVarInstance<'db> {
    let Some(context) = signature.generic_context else {
        panic!("expected generic fixture signature: {signature:?}");
    };
    let mut variables = context.variables(db);
    assert_eq!(variables.len(), 1);
    let Some(variable) = variables.next() else {
        panic!("expected one generic variable: {signature:?}");
    };
    assert_eq!(variable.name(db).as_str(), name);
    assert_eq!(variable.binding_context(db).definition(), Some(definition));
    variable
}

/// Checks rescoping by inspecting the canonical variable handles in the returned annotations.
fn assert_return_scope_result<'db>(
    db: &'db TestDb,
    definition: Definition<'db>,
    signature: &Signature<'db>,
    fixture: ReturnScopeFixture,
) {
    match fixture {
        ReturnScopeFixture::Unknown => {
            return_scope_variable(db, signature, definition, "T");
            assert_eq!(signature.return_ty, Type::unknown());
        }
        ReturnScopeFixture::OutsideParameter => {
            let variable = return_scope_variable(db, signature, definition, "T");
            let returned = return_scope_callable(signature.return_ty);
            let returned = return_scope_signature(returned.signatures(db));
            assert_eq!(returned.generic_context, None);
            assert_eq!(returned.return_ty, Type::TypeVar(variable));
            assert_eq!(
                signature.parameters()[0].annotated_type(),
                Type::TypeVar(variable)
            );
        }
        ReturnScopeFixture::OutsideAfter | ReturnScopeFixture::OutsideBefore => {
            let variable = return_scope_variable(db, signature, definition, "T");
            let pair = return_scope_pair(db, signature.return_ty);
            let [callable, outside] = if fixture == ReturnScopeFixture::OutsideBefore {
                [pair[1], pair[0]]
            } else {
                pair
            };
            assert_eq!(outside, Type::TypeVar(variable));
            let callable = return_scope_callable(callable);
            let callable = return_scope_signature(callable.signatures(db));
            assert_eq!(callable.generic_context, None);
            assert_eq!(callable.return_ty, Type::TypeVar(variable));
        }
        ReturnScopeFixture::Returned | ReturnScopeFixture::Nested => {
            assert_eq!(signature.generic_context, None);
            let returned = return_scope_callable(signature.return_ty);
            let returned = return_scope_signature(returned.signatures(db));
            let variable = return_scope_variable(db, returned, definition, "T'return");
            assert_eq!(returned.return_ty, Type::TypeVar(variable));
            if fixture == ReturnScopeFixture::Nested {
                let nested = return_scope_callable(returned.parameters()[0].annotated_type());
                let nested = return_scope_signature(nested.signatures(db));
                assert_eq!(nested.generic_context, None);
                assert_eq!(nested.return_ty, Type::TypeVar(variable));
                assert_eq!(
                    nested.parameters()[0].annotated_type(),
                    Type::TypeVar(variable)
                );
            } else {
                assert_eq!(
                    returned.parameters()[0].annotated_type(),
                    Type::TypeVar(variable)
                );
            }
        }
        ReturnScopeFixture::Repeated => {
            assert_eq!(signature.generic_context, None);
            let [left, right] = return_scope_pair(db, signature.return_ty);
            let left = return_scope_callable(left);
            let right = return_scope_callable(right);
            assert_eq!(left, right);
            let left = return_scope_signature(left.signatures(db));
            let right = return_scope_signature(right.signatures(db));
            let left_variable = return_scope_variable(db, left, definition, "T'return");
            let right_variable = return_scope_variable(db, right, definition, "T'return");
            assert_eq!(left.return_ty, Type::TypeVar(left_variable));
            assert_eq!(right.return_ty, Type::TypeVar(right_variable));
            assert_eq!(
                right.parameters()[0].annotated_type(),
                Type::TypeVar(right_variable)
            );
        }
    }
}

/// One cold public-signature request publishes the canonical result. Returned-only variables move
/// into the outermost returned callable; parameter or outside-return occurrences keep the function
/// context. Nested callables use their outermost callable's renamed variable.
/// The repeated tuple case also checks that its input annotations share one callable handle.
#[test_case::test_case(ReturnScopeFixture::Returned; "return only")]
#[test_case::test_case(ReturnScopeFixture::OutsideParameter; "parameter occurrence")]
#[test_case::test_case(ReturnScopeFixture::OutsideAfter; "callable before outside occurrence")]
#[test_case::test_case(ReturnScopeFixture::OutsideBefore; "outside occurrence before callable")]
#[test_case::test_case(ReturnScopeFixture::Repeated; "repeated outermost callable handle")]
#[test_case::test_case(ReturnScopeFixture::Nested; "nested callable inherits outermost context")]
#[test_case::test_case(ReturnScopeFixture::Unknown; "unknown return retains function context")]
fn cold_public_signature_rescopes_return_callable_contexts(fixture: ReturnScopeFixture) {
    let db = database(fixture.source(), PythonVersion::PY313, true);
    let prepared = prepared(&db, true);
    let node = function(&prepared);
    let definition = prepared.semantic_index().expect_single_definition(node);
    observations::reset(None);
    let cold = capture(&db, || {
        controlled(&prepared, Request::PublicSignature, &funded())
    })
    .unwrap();
    let Ok(AnalysisOutcome::Complete(Value::PublicSignature(signatures))) = cold.value else {
        panic!("cold public signature: {:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    assert_cleanup();
    assert_return_scope_result(&db, definition, return_scope_signature(signatures), fixture);

    let Type::FunctionLiteral(function) =
        infer_definition_types(&db, definition).binding_type(definition)
    else {
        panic!("fixture function binding");
    };
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            function_literal_signature_ingredient(&db),
            function.as_id(),
        )
        .is_ok()
    );
    let mut reader = db.clone();
    reader.take_salsa_events();
    assert!(std::ptr::eq(function.signature(&db), signatures));
    assert_function_query_was_not_run_by_name(
        &db,
        "function_literal_signature",
        Some(function.as_id()),
        &reader.take_salsa_events(),
    );
    if fixture == ReturnScopeFixture::Repeated {
        let annotation = node.returns.as_deref().unwrap();
        let original = crate::types::signatures::function_signature_expression_type(
            &db, definition, annotation,
        );
        let [left, right] = return_scope_pair(&db, original);
        assert_eq!(left, right);
        assert_eq!(
            return_scope_signature(return_scope_callable(left).signatures(&db)).generic_context,
            None
        );
        assert_eq!(
            return_scope_signature(return_scope_callable(right).signatures(&db)).generic_context,
            None
        );
    }
}

/// Work refusal after each populated return-scoping stage leaves its public signature unpublished.
/// A separate measurement database locates the boundary; one same-revision retry completes it.
#[test_case::test_case(ReturnScopeStage::Locations; "locations retained")]
#[test_case::test_case(ReturnScopeStage::MovedVariable; "original variable retained")]
#[test_case::test_case(ReturnScopeStage::RetainedRenamings; "renaming map retained")]
#[test_case::test_case(ReturnScopeStage::RetainedReplacements; "replacement map retained")]
fn return_scope_work_refusal_keeps_signature_unpublished_before_retry(stage: ReturnScopeStage) {
    let fixture = ReturnScopeFixture::Returned;
    let measured_db = database(fixture.source(), PythonVersion::PY313, true);
    let measured_prepared = prepared(&measured_db, true);
    observations::reset(None);
    let recording = ReturnScopeRecording::start();
    let measured = controlled(&measured_prepared, Request::PublicSignature, &funded());
    drop(recording);
    assert!(
        matches!(
            measured,
            Ok(AnalysisOutcome::Complete(Value::PublicSignature(_)))
        ),
        "{measured:?}"
    );
    let remaining = RETURN_SCOPE_REMAINING.get()[stage as usize].unwrap();
    let policy = AnalysisPolicy {
        semantic_work_limit: funded().semantic_work_limit - remaining,
        ..funded()
    };
    assert_cleanup();

    let db = database(fixture.source(), PythonVersion::PY313, true);
    let prepared = prepared(&db, true);
    let definition = prepared
        .semantic_index()
        .expect_single_definition(function(&prepared));
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    let recording = ReturnScopeRecording::start();
    assert_eq!(
        controlled(&prepared, Request::PublicSignature, &policy),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        })
    );
    drop(recording);
    assert_eq!(RETURN_SCOPE_REMAINING.get()[stage as usize], Some(0));
    assert_cleanup();
    // Binding completed before return scoping. Certify that child before reading its stored
    // function handle, so this inspection cannot silently warm the explicit retry.
    //
    assert!(FinalSourceMemo::certify(
        &db as &dyn Db,
        definition_inference_ingredient(&db),
        definition.as_id(),
    ).is_ok());
    let Type::FunctionLiteral(function) =
        infer_definition_types(&db, definition).binding_type(definition)
    else {
        panic!("fixture function binding");
    };
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            function_literal_signature_ingredient(&db),
            function.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );

    observations::reset(None);
    let retry = controlled(&prepared, Request::PublicSignature, &funded());
    let Ok(AnalysisOutcome::Complete(Value::PublicSignature(signatures))) = retry else {
        panic!("one funded retry: {retry:?}");
    };
    assert_return_scope_result(&db, definition, return_scope_signature(signatures), fixture);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            function_literal_signature_ingredient(&db),
            function.as_id(),
        )
        .is_ok()
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

/// A byte-only refusal at the cold public-signature entry remains retryable at the same revision.
/// This exercises the independent allocation limit.
#[test]
fn return_scope_byte_entry_refusal_allows_one_funded_retry() {
    let fixture = ReturnScopeFixture::Returned;
    let db = database(fixture.source(), PythonVersion::PY313, true);
    let prepared = prepared(&db, true);
    let definition = prepared
        .semantic_index()
        .expect_single_definition(function(&prepared));
    let revision = salsa::plumbing::current_revision(&db);
    observations::reset(None);
    assert_eq!(
        controlled(
            &prepared,
            Request::PublicSignature,
            &AnalysisPolicy {
                requested_bytes_limit: 1,
                ..funded()
            }
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: (),
        })
    );
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
    assert_cleanup();

    observations::reset(None);
    let retry = controlled(&prepared, Request::PublicSignature, &funded());
    let Ok(AnalysisOutcome::Complete(Value::PublicSignature(signatures))) = retry else {
        panic!("one funded retry: {retry:?}");
    };
    assert_return_scope_result(&db, definition, return_scope_signature(signatures), fixture);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}

/// Selects whether the stored generic callable is a parameter annotation or a return annotation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeclarationLocation {
    Parameter,
    Return,
}

/// Generic-context declarations contribute their bound variable, without visiting its bound or default.
/// Stored values isolate this visitor rule; no source declaration is inferred to create the fixture.
#[test_case::test_case(DeclarationLocation::Parameter; "declaration outside returned callable")]
#[test_case::test_case(DeclarationLocation::Return; "declaration within returned callable")]
fn return_locations_record_declarations_without_their_bounds_or_defaults(
    location: DeclarationLocation,
) {
    let db = database("def choose(): ...\n", PythonVersion::PY313, true);
    let prepared = prepared(&db, true);
    let env = db.program_environment();
    let bound = legacy_variable(&db, "Bound", TypeVarKind::LegacyTypeVar);
    let default = legacy_variable(&db, "Default", TypeVarKind::LegacyTypeVar);
    let declaration = BoundTypeVarInstance::new(
        &db,
        TypeVarInstance::new(
            &db,
            TypeVarIdentity::new(
                &db,
                Name::new_static("Declaration"),
                None,
                TypeVarKind::LegacyTypeVar,
            ),
            Some(TypeVarBoundOrConstraintsEvaluation::Eager(
                crate::types::typevar::TypeVarBoundOrConstraints::UpperBound(Type::TypeVar(bound)),
            )),
            None,
            Some(TypeVarDefaultEvaluation::Eager(Type::TypeVar(default))),
        ),
        BindingContext::Synthetic(env.program(&db)),
        None,
        TypeVarNonce::NONE,
    );
    let context = GenericContext::from_typevar_instances(&db, &env, [declaration]);
    let ty = Type::single_callable(
        &db,
        Signature::new_generic(Some(context), Parameters::empty(), Type::unknown()),
    );
    let callable = return_scope_callable(ty);
    let (parameters, return_type) = match location {
        DeclarationLocation::Parameter => (
            Parameters::standard([Parameter::positional_only(None).with_annotated_type(ty)]),
            Type::unknown(),
        ),
        DeclarationLocation::Return => (Parameters::empty(), ty),
    };
    let definition = prepared
        .semantic_index()
        .expect_single_definition(function(&prepared));
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );

    observations::reset(None);
    let controlled = controlled(
        &prepared,
        Request::ReturnLocations {
            parameters: &parameters,
            return_type,
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(Value::ReturnLocations { outside, inside })) = controlled
    else {
        panic!("controlled declaration occurrences: {controlled:?}");
    };
    assert_cleanup();
    let ordinary = crate::types::generics::return_locations::collect_locations_sync(
        &parameters,
        return_type,
        &crate::types::generics::return_locations::OrdinaryLocationEffects { db: &db, env: &env },
    )
    .unwrap();
    assert_eq!(outside, ordinary.found_outside_callable_return);
    assert_eq!(inside, ordinary.found_inside_callable_return);
    match location {
        DeclarationLocation::Parameter => {
            assert_eq!(outside.len(), 1);
            assert!(outside.contains(&declaration));
            assert!(inside.is_empty());
        }
        DeclarationLocation::Return => {
            assert!(outside.is_empty());
            assert_eq!(inside.len(), 1);
            let variables = &inside[&callable];
            assert_eq!(variables.len(), 1);
            assert!(variables.contains(&declaration));
        }
    }
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            definition.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo)
    );
}
/// A shared variable handle is recorded once under each distinct outermost returned callable.
/// This checks occurrence collection only; it does not choose either callable's final generic context.
#[test]
fn return_locations_distinguish_outermost_callable_handles() {
    let db = database("def choose(): ...\n", PythonVersion::PY313, true);
    let prepared = prepared(&db, true);
    let env = db.program_environment();
    let variable = legacy_variable(&db, "Shared", TypeVarKind::LegacyTypeVar);
    let left = Type::single_callable(
        &db,
        Signature::new(Parameters::empty(), Type::TypeVar(variable)),
    );
    let right = Type::single_callable(
        &db,
        Signature::new(
            Parameters::standard([
                Parameter::positional_only(None).with_annotated_type(Type::TypeVar(variable))
            ]),
            Type::TypeVar(variable),
        ),
    );
    let left_callable = return_scope_callable(left);
    let right_callable = return_scope_callable(right);
    assert_ne!(left_callable, right_callable);
    let return_type = Type::tuple(crate::types::tuple::TupleType::heterogeneous(
        &db,
        &env,
        [left, right],
    ));
    let parameters = Parameters::empty();
    observations::reset(None);
    let controlled = controlled(
        &prepared,
        Request::ReturnLocations {
            parameters: &parameters,
            return_type,
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(Value::ReturnLocations { outside, inside })) = controlled
    else {
        panic!("controlled distinct-callable occurrences: {controlled:?}");
    };
    assert_cleanup();
    let ordinary = crate::types::generics::return_locations::collect_locations_sync(
        &parameters,
        return_type,
        &crate::types::generics::return_locations::OrdinaryLocationEffects { db: &db, env: &env },
    )
    .unwrap();
    assert_eq!(outside, ordinary.found_outside_callable_return);
    assert_eq!(inside, ordinary.found_inside_callable_return);
    assert!(outside.is_empty());
    assert_eq!(inside.len(), 2);
    assert_eq!(inside[&left_callable].len(), 1);
    assert!(inside[&left_callable].contains(&variable));
    assert_eq!(inside[&right_callable].len(), 1);
    assert!(inside[&right_callable].contains(&variable));
}
