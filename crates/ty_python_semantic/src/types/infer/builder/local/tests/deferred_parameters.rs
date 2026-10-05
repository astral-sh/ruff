//! Deferred parameter controls observe nested restoration in the registered source runtime.

use std::cell::RefCell;
use std::future::{Future, poll_fn};
use std::panic::AssertUnwindSafe;
use std::rc::Rc;

use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_db::testing::assert_function_query_was_not_run_by_name;
use ruff_text_size::{Ranged, TextRange};
use salsa::execution_probe::{FinalSourceError, FinalSourceMemo, RunResult};
use salsa::plumbing::AsId;
use salsa::prepared_source_probe::assert_no_active_attempt;
use ty_python_core::Program;
use ty_python_core::definition::DefinitionKind;

use super::super::*;
use crate::analysis::{AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, PreparedAnalysisFile, prepare_file};
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::types::cyclic::guard_storage::observations as lifetime_observations;
use crate::types::infer::builder::deferred::DeferredEffects;
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceEffects, observations};
use crate::types::infer::builder::typevar::pep695::TypeParameterDefinitionNode;
use crate::types::infer::source_runtime::tests::nominal_members::{MemberOperation, controlled_member_operation};
use crate::types::infer::source_runtime::tests::signature_annotations::State;
use crate::types::infer::{DefinitionInference, deferred_definition_inference_ingredient, definition_inference_ingredient, infer_deferred_types};

/// Identifies the declaration checkpoint and the nested default-helper checkpoints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer::builder) enum TransactionKind {
    Declaration,
    ParamSpec,
    TypeVarTuple,
}

/// Distinguishes checkpoints that borrow the same builder at different nesting depths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer::builder) struct TransactionId(usize);

/// Identifies one type-expression request made by the selected deferred parameter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer::builder) struct ChildId(usize);

#[derive(Clone, Copy, Debug)]
struct Checkpoint {
    state: State,
    remaining: Option<usize>,
    lifetime_events: usize,
}

impl Checkpoint {
    /// Copies builder state and the position of preceding physical child-drop events.
    fn new(builder: &TypeInferenceBuilder<'_, '_>) -> Self {
        Self {
            state: function_annotation_state(builder),
            remaining: salsa::attempt_probe::remaining_allowance_for_diagnostics(builder.db()),
            lifetime_events: lifetime_observations::snapshot().count,
        }
    }
}

#[derive(Debug)]
struct Transaction {
    kind: TransactionKind,
    builder: usize,
    started: Checkpoint,
    completed: Option<Checkpoint>,
    restored: Option<Checkpoint>,
}

#[derive(Debug)]
struct Child {
    transaction: TransactionId,
    range: TextRange,
    entered: Checkpoint,
    pending: usize,
    completed: Option<Checkpoint>,
}

#[derive(Debug)]
struct Journal {
    definition: salsa::Id,
    transactions: Vec<Transaction>,
    active: Vec<TransactionId>,
    restored: Vec<TransactionId>,
    children: Vec<Child>,
}

thread_local! {
    static RECORDING: RefCell<Option<Rc<RefCell<Journal>>>> = const { RefCell::new(None) };
}

#[derive(Debug)]
struct Recording(Rc<RefCell<Journal>>);

impl Recording {
    /// Starts collecting checkpoint and child observations for one deferred definition.
    fn new(definition: Definition<'_>) -> Self {
        observations::reset(None);
        lifetime_observations::reset();
        let journal = Rc::new(RefCell::new(Journal {
            definition: definition.as_id(),
            transactions: Vec::new(),
            active: Vec::new(),
            restored: Vec::new(),
            children: Vec::new(),
        }));
        RECORDING.with_borrow_mut(|recording| {
            assert!(recording.replace(Rc::clone(&journal)).is_none());
        });
        Self(journal)
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.with_borrow_mut(|recording| *recording = None);
        lifetime_observations::stop();
    }
}

/// Records a checkpoint only when its builder belongs to the selected deferred definition.
pub(in crate::types::infer::builder) fn transaction_started(
    builder: &TypeInferenceBuilder<'_, '_>,
    kind: TransactionKind,
) -> Option<TransactionId> {
    RECORDING.with_borrow(|recording| {
        let Some(recording) = recording else { return None; };
        let InferenceRegion::Deferred(definition) = builder.region else { return None; };
        let mut journal = recording.borrow_mut();
        if definition.as_id() != journal.definition { return None; }
        let id = TransactionId(journal.transactions.len());
        journal.transactions.push(Transaction {
            kind,
            builder: builder as *const _ as usize,
            started: Checkpoint::new(builder),
            completed: None,
            restored: None,
        });
        journal.active.push(id);
        Some(id)
    })
}

/// Records normal completion before the checkpoint is disarmed.
pub(in crate::types::infer::builder) fn transaction_completed(
    id: TransactionId,
    builder: &TypeInferenceBuilder<'_, '_>,
) {
    RECORDING.with_borrow(|recording| {
        if let Some(recording) = recording {
            let mut journal = recording.borrow_mut();
            assert_eq!(journal.active.pop(), Some(id));
            journal.transactions[id.0].completed = Some(Checkpoint::new(builder));
        }
    });
}

/// Records restored state after the checkpoint returns the caller's flags and context.
pub(in crate::types::infer::builder) fn transaction_restored(
    id: TransactionId,
    builder: &TypeInferenceBuilder<'_, '_>,
) {
    RECORDING.with_borrow(|recording| {
        if let Some(recording) = recording {
            let mut journal = recording.borrow_mut();
            assert_eq!(journal.active.pop(), Some(id));
            journal.transactions[id.0].restored = Some(Checkpoint::new(builder));
            journal.restored.push(id);
        }
    });
}

/// Associates a real annotation child with its innermost selected checkpoint.
pub(in crate::types::infer::builder) fn child_entered(
    builder: &TypeInferenceBuilder<'_, '_>,
    expression: &ast::Expr,
) -> Option<ChildId> {
    RECORDING.with_borrow(|recording| {
        let Some(recording) = recording else { return None; };
        let mut journal = recording.borrow_mut();
        let transaction = *journal.active.last()?;
        if journal.transactions[transaction.0].builder != builder as *const _ as usize { return None; }
        let id = ChildId(journal.children.len());
        journal.children.push(Child {
            transaction,
            range: expression.range(),
            entered: Checkpoint::new(builder),
            pending: 0,
            completed: None,
        });
        Some(id)
    })
}

/// Counts actual suspension without changing the child's poll result or cancellation behavior.
pub(in crate::types::infer::builder) async fn observe_child_polling<F: Future>(
    child: Option<ChildId>,
    future: F,
) -> F::Output {
    let mut future = std::pin::pin!(future);
    poll_fn(|context| {
        let result = future.as_mut().poll(context);
        if result.is_pending() && let Some(child) = child {
            RECORDING.with_borrow(|recording| {
                if let Some(recording) = recording {
                    recording.borrow_mut().children[child.0].pending += 1;
                }
            });
        }
        result
    }).await
}

/// Records the child's completed state before the adapter starts its next admitted operation.
pub(in crate::types::infer::builder) fn child_completed(
    child: Option<ChildId>,
    builder: &TypeInferenceBuilder<'_, '_>,
) {
    if let Some(child) = child {
        RECORDING.with_borrow(|recording| {
            if let Some(recording) = recording {
                recording.borrow_mut().children[child.0].completed = Some(Checkpoint::new(builder));
            }
        });
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Parameter {
    TypeVar,
    ParamSpec,
    TypeVarTuple,
}

impl Parameter {
    const fn source(self) -> &'static str {
        match self {
            Self::TypeVar => "class Leaf: pass\nclass C[T = Leaf]: pass\n",
            Self::ParamSpec => "class Leaf: pass\nclass C[**P = [int, Leaf]]: pass\n",
            Self::TypeVarTuple => "class Leaf: pass\nclass C[*Ts = *tuple[int, Leaf]]: pass\n",
        }
    }

    const fn kinds(self) -> &'static [TransactionKind] {
        match self {
            Self::TypeVar => &[TransactionKind::Declaration],
            Self::ParamSpec => &[TransactionKind::Declaration, TransactionKind::ParamSpec],
            Self::TypeVarTuple => &[TransactionKind::Declaration, TransactionKind::TypeVarTuple],
        }
    }
}

fn funded() -> AnalysisPolicy {
    AnalysisPolicy { semantic_work_limit: 1_000_000, requested_bytes_limit: 16 * 1024 * 1024 }
}

fn fixture(parameter: Parameter) -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new().with_python_version(PythonVersion::PY313).build()?;
    db.write_file("src/main.py", parameter.source())?;
    Ok(db)
}

fn prepare(db: &TestDb) -> anyhow::Result<PreparedAnalysisFile<'_>> {
    prepare_file(db, system_path_to_file(db, "src/main.py")?).map_err(|error| anyhow::anyhow!("{error:?}"))
}

#[derive(Debug)]
struct Nodes<'db, 'ast> {
    definition: Definition<'db>,
    leaf: Definition<'db>,
    node: TypeParameterDefinitionNode<'db>,
    default: &'ast ast::Expr,
    selected: &'ast ast::Expr,
}

/// Selects the parameter, its default, and the cold Leaf definition from the fixture.
/// The selected ParamSpec child is its last element, so its first element completes beforehand.
fn nodes<'db, 'ast>(db: &'db TestDb, prepared: &'ast PreparedAnalysisFile<'db>) -> anyhow::Result<Nodes<'db, 'ast>> {
    let [ast::Stmt::ClassDef(leaf), ast::Stmt::ClassDef(class)] = prepared.parsed_module().suite().as_slice() else {
        anyhow::bail!("fixture must contain Leaf and C");
    };
    let Some(parameters) = &class.type_params else { anyhow::bail!("C must have a type parameter"); };
    let [parameter] = parameters.type_params.as_slice() else { anyhow::bail!("C must have exactly one type parameter"); };
    let index = prepared.semantic_index();
    let definition = match parameter {
        ast::TypeParam::TypeVar(node) => index.expect_single_definition(node),
        ast::TypeParam::ParamSpec(node) => index.expect_single_definition(node),
        ast::TypeParam::TypeVarTuple(node) => index.expect_single_definition(node),
    };
    let node = match definition.kind(db) {
        DefinitionKind::TypeVar(node) => TypeParameterDefinitionNode::TypeVar(node),
        DefinitionKind::ParamSpec(node) => TypeParameterDefinitionNode::ParamSpec(node),
        DefinitionKind::TypeVarTuple(node) => TypeParameterDefinitionNode::TypeVarTuple(node),
        _ => anyhow::bail!("fixture definition must be a type parameter"),
    };
    let Some(default) = parameter.default() else { anyhow::bail!("fixture parameter must have a default"); };
    let selected = match default {
        ast::Expr::List(list) => list.elts.last().ok_or_else(|| anyhow::anyhow!("ParamSpec default must be nonempty"))?,
        _ => default,
    };
    Ok(Nodes { definition, leaf: index.expect_single_definition(leaf), node, default, selected })
}

#[derive(Debug)]
struct CanonicalRequest<'db>(Definition<'db>);

impl<'db> MemberOperation<'db> for CanonicalRequest<'db> {
    type Output = &'db DefinitionInference<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(self, access: &A, _program: Program<'db>) -> RunResult<Self::Output>
    where 'db: 'run {
        access.deferred_definition(self.0).await
    }
}

struct DirectRequest<'builder, 'db, 'ast> {
    builder: &'builder mut TypeInferenceBuilder<'db, 'ast>,
    node: TypeParameterDefinitionNode<'db>,
}

impl<'db> MemberOperation<'db> for DirectRequest<'_, 'db, '_> {
    type Output = ();

    async fn run<'run, A: SourceAccess<'run, 'db>>(self, access: &A, program: Program<'db>) -> RunResult<()>
    where 'db: 'run {
        DeferredEffects::type_parameter(&SourceEffects::new(access, program), self.builder, self.node).await
    }
}

fn canonical<'db>(prepared: &PreparedAnalysisFile<'db>, definition: Definition<'db>, policy: &AnalysisPolicy) -> anyhow::Result<AnalysisOutcome<&'db DefinitionInference<'db>>> {
    controlled_member_operation(prepared, CanonicalRequest(definition), policy).map_err(|error| anyhow::anyhow!("{error:?}"))
}

/// Checks that unpublished builders have retired and no analysis attempt remains active.
fn cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

fn assert_missing(db: &TestDb, definition: Definition<'_>) {
    assert_eq!(FinalSourceMemo::certify(db as &dyn Db, deferred_definition_inference_ingredient(db), definition.as_id()).map(|_| ()), Err(FinalSourceError::MissingMemo));
}

/// Certifies the completed Leaf child without invoking ordinary inference to produce it.
fn assert_leaf_ready(db: &TestDb, leaf: Definition<'_>) {
    assert!(FinalSourceMemo::certify(db as &dyn Db, definition_inference_ingredient(db), leaf.as_id()).is_ok());
}

/// Certifies that the funded canonical retry published the selected deferred definition.
fn assert_deferred_ready(db: &TestDb, definition: Definition<'_>) {
    assert!(FinalSourceMemo::certify(db as &dyn Db, deferred_definition_inference_ingredient(db), definition.as_id()).is_ok());
}

/// Checks normal completion of every nested checkpoint with its original state restored.
fn assert_completed(journal: &Journal, parameter: Parameter) {
    assert!(journal.active.is_empty());
    assert!(journal.restored.is_empty());
    assert_eq!(journal.transactions.iter().map(|transaction| transaction.kind).collect::<Vec<_>>(), parameter.kinds());
    for transaction in &journal.transactions {
        assert!(transaction.restored.is_none());
        assert_eq!(transaction.completed.map(|checkpoint| checkpoint.state), Some(transaction.started.state));
    }
}

/// Checks nested restoration, which precedes destruction of the borrowed root builder.
fn assert_restored(journal: &Journal, parameter: Parameter) {
    assert!(journal.active.is_empty());
    assert_eq!(journal.transactions.iter().map(|transaction| transaction.kind).collect::<Vec<_>>(), parameter.kinds());
    assert_eq!(journal.restored, (0..journal.transactions.len()).rev().map(TransactionId).collect::<Vec<_>>());
    for transaction in &journal.transactions {
        assert!(transaction.completed.is_none());
        assert_eq!(transaction.restored.map(|checkpoint| checkpoint.state), Some(transaction.started.state));
        match transaction.kind {
            TransactionKind::Declaration => {}
            TransactionKind::ParamSpec | TransactionKind::TypeVarTuple => {
                assert!(matches!(transaction.started.state.deferred, DeferredExpressionState::Deferred));
            }
        }
    }
}

fn selected_child<'a>(journal: &'a Journal, nodes: &Nodes<'_, '_>) -> anyhow::Result<&'a Child> {
    journal.children.iter().find(|child| child.range == nodes.selected.range()).ok_or_else(|| anyhow::anyhow!("selected default child was not entered"))
}

/// Checks default-helper state and completion of the preceding ParamSpec element.
fn assert_child_state(journal: &Journal, nodes: &Nodes<'_, '_>, parameter: Parameter) -> anyhow::Result<()> {
    let child = selected_child(journal, nodes)?;
    let transaction = &journal.transactions[child.transaction.0];
    assert_eq!(transaction.kind, *parameter.kinds().last().ok_or_else(|| anyhow::anyhow!("missing checkpoint kind"))?);
    assert!(matches!(child.entered.state.deferred, DeferredExpressionState::Deferred));
    match parameter {
        Parameter::TypeVar => {}
        Parameter::ParamSpec => {
            assert!(!child.entered.state.flags.contains(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR));
            let Some(first) = journal.children.first() else { anyhow::bail!("missing first ParamSpec element"); };
            assert_ne!(first.range, child.range);
            assert!(first.completed.is_some());
        }
        Parameter::TypeVarTuple => assert!(child.entered.state.flags.contains(InferenceFlags::IN_VALID_UNPACK_CONTEXT)),
    }
    Ok(())
}

/// Measures cumulative attempt work through selected child completion in an independent database.
fn completed_child_work(parameter: Parameter) -> anyhow::Result<usize> {
    let db = fixture(parameter)?;
    let prepared = prepare(&db)?;
    let nodes = nodes(&db, &prepared)?;
    let recording = Recording::new(nodes.definition);
    assert!(matches!(canonical(&prepared, nodes.definition, &funded())?, AnalysisOutcome::Complete(_)));
    let journal = recording.0.borrow();
    let completed = selected_child(&journal, &nodes)?.completed.ok_or_else(|| anyhow::anyhow!("selected child did not complete"))?;
    let remaining = completed.remaining.ok_or_else(|| anyhow::anyhow!("missing child work allowance"))?;
    cleanup();
    Ok(funded().semantic_work_limit - remaining)
}

/// Compares the completed default with ordinary inference in an independent database.
fn assert_ordinary<'db>(parameter: Parameter, db: &'db TestDb, prepared: &PreparedAnalysisFile<'db>, nodes: &Nodes<'db, '_>, inference: &DefinitionInference<'db>) -> anyhow::Result<()> {
    let ordinary_db = fixture(parameter)?;
    let ordinary_prepared = prepare(&ordinary_db)?;
    let ordinary_nodes = self::nodes(&ordinary_db, &ordinary_prepared)?;
    let ordinary = infer_deferred_types(&ordinary_db, ordinary_nodes.definition);
    assert_eq!(inference.expression_type(nodes.default).display(db, &ProgramEnvironment::from_file(prepared.program_file())).to_string(), ordinary.expression_type(ordinary_nodes.default).display(&ordinary_db, &ProgramEnvironment::from_file(ordinary_prepared.program_file())).to_string());
    assert_eq!(inference.expressions.iter().len(), ordinary.expressions.iter().len());
    assert_eq!(inference.extra.is_none(), ordinary.extra.is_none());
    Ok(())
}

/// A work refusal after the selected child restores every checkpoint and leaves the deferred memo
/// unpublished; one funded retry in the same revision matches independent ordinary inference.
#[test_case::test_case(Parameter::TypeVar; "typevar")]
#[test_case::test_case(Parameter::ParamSpec; "paramspec")]
#[test_case::test_case(Parameter::TypeVarTuple; "typevartuple")]
fn deferred_parameter_work_refusal_restores_and_retries(parameter: Parameter) -> anyhow::Result<()> {
    let work = completed_child_work(parameter)?;
    let db = fixture(parameter)?;
    let prepared = prepare(&db)?;
    let nodes = nodes(&db, &prepared)?;
    let revision = salsa::plumbing::current_revision(&db);
    assert_missing(&db, nodes.definition);
    let recording = Recording::new(nodes.definition);
    assert_eq!(canonical(&prepared, nodes.definition, &AnalysisPolicy { semantic_work_limit: work, ..funded() })?, AnalysisOutcome::Incomplete { reason: AnalysisIncomplete::WorkLimit, completed: () });
    {
        let journal = recording.0.borrow();
        assert_restored(&journal, parameter);
        assert_child_state(&journal, &nodes, parameter)?;
        assert!(selected_child(&journal, &nodes)?.completed.is_some());
        assert!(matches!(journal.transactions[0].started.state.deferred, DeferredExpressionState::None));
    }
    assert_missing(&db, nodes.definition);
    assert_leaf_ready(&db, nodes.leaf);
    cleanup();
    drop(recording);
    let mut reader = db.clone();
    reader.take_salsa_events();
    let recording = Recording::new(nodes.definition);
    let AnalysisOutcome::Complete(inference) = canonical(&prepared, nodes.definition, &funded())? else { anyhow::bail!("funded retry did not complete"); };
    assert_completed(&recording.0.borrow(), parameter);
    assert_deferred_ready(&db, nodes.definition);
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", Some(nodes.leaf.as_id()), &reader.take_salsa_events());
    drop(recording);
    assert_ordinary(parameter, &db, &prepared, &nodes, inference)?;
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    cleanup();
    Ok(())
}

/// Reports whether the selected child completes at the given byte allowance on fresh cold input.
fn reaches_completed_child(parameter: Parameter, bytes: usize) -> anyhow::Result<bool> {
    let db = fixture(parameter)?;
    let prepared = prepare(&db)?;
    let nodes = nodes(&db, &prepared)?;
    let recording = Recording::new(nodes.definition);
    let result = canonical(&prepared, nodes.definition, &AnalysisPolicy { requested_bytes_limit: bytes, ..funded() })?;
    assert!(matches!(result, AnalysisOutcome::Complete(_) | AnalysisOutcome::Incomplete { reason: AnalysisIncomplete::RequestedAllocationLimit, .. }), "{result:?}");
    let reached = recording.0.borrow().children.iter().any(|child| child.range == nodes.selected.range() && child.completed.is_some());
    cleanup();
    Ok(reached)
}

/// A byte refusal after child inference restores nested default state and leaves no deferred memo;
/// the unchanged definition succeeds with one funded retry in the same revision.
#[test_case::test_case(Parameter::TypeVar; "typevar")]
#[test_case::test_case(Parameter::ParamSpec; "paramspec")]
#[test_case::test_case(Parameter::TypeVarTuple; "typevartuple")]
fn deferred_parameter_byte_refusal_restores_and_retries(parameter: Parameter) -> anyhow::Result<()> {
    let mut low = 0;
    let mut high = funded().requested_bytes_limit;
    assert!(reaches_completed_child(parameter, high)?);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if reaches_completed_child(parameter, middle)? { high = middle; } else { low = middle; }
    }
    let db = fixture(parameter)?;
    let prepared = prepare(&db)?;
    let nodes = nodes(&db, &prepared)?;
    let revision = salsa::plumbing::current_revision(&db);
    assert_missing(&db, nodes.definition);
    let recording = Recording::new(nodes.definition);
    assert_eq!(canonical(&prepared, nodes.definition, &AnalysisPolicy { requested_bytes_limit: high, ..funded() })?, AnalysisOutcome::Incomplete { reason: AnalysisIncomplete::RequestedAllocationLimit, completed: () });
    {
        let journal = recording.0.borrow();
        assert_restored(&journal, parameter);
        assert_child_state(&journal, &nodes, parameter)?;
        assert!(selected_child(&journal, &nodes)?.completed.is_some());
    }
    assert_missing(&db, nodes.definition);
    assert_leaf_ready(&db, nodes.leaf);
    cleanup();
    drop(recording);
    let mut reader = db.clone();
    reader.take_salsa_events();
    let recording = Recording::new(nodes.definition);
    let AnalysisOutcome::Complete(inference) = canonical(&prepared, nodes.definition, &funded())? else { anyhow::bail!("funded retry did not complete"); };
    assert_completed(&recording.0.borrow(), parameter);
    assert_deferred_ready(&db, nodes.definition);
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", Some(nodes.leaf.as_id()), &reader.take_salsa_events());
    drop(recording);
    assert_ordinary(parameter, &db, &prepared, &nodes, inference)?;
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    cleanup();
    Ok(())
}

/// Cancellation requested inside the cold Leaf definition is delivered after that canonical child
/// completes and publishes. The adapter restores its caller-owned state; one retry with a fresh
/// builder in the same revision reuses Leaf's completed memo.
#[test_case::test_case(Parameter::TypeVar; "typevar")]
#[test_case::test_case(Parameter::ParamSpec; "paramspec")]
#[test_case::test_case(Parameter::TypeVarTuple; "typevartuple")]
fn deferred_parameter_cold_child_cancellation_restores_and_retries(parameter: Parameter) -> anyhow::Result<()> {
    let db = fixture(parameter)?;
    let prepared = prepare(&db)?;
    let nodes = nodes(&db, &prepared)?;
    let file = prepared.program_file();
    let env = ProgramEnvironment::from_file(file);
    let revision = salsa::plumbing::current_revision(&db);
    let make_builder = || {
        let mut builder = TypeInferenceBuilder::new(&db, &env, InferenceRegion::Deferred(nodes.definition), file.file(&db), file, prepared.semantic_index(), prepared.parsed_module());
        builder.context.defuse();
        builder.typevar_binding_context = Some(nodes.leaf);
        builder.context.inference_flags.insert(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR);
        builder.context.inference_flags.remove(InferenceFlags::IN_VALID_UNPACK_CONTEXT);
        builder.setup_expression_cache();
        builder
    };
    let mut builder = make_builder();
    let initial = function_annotation_state(&builder);
    let cache = builder.expression_cache.as_ref().map(|cache| (Rc::as_ptr(cache), Rc::strong_count(cache)));
    let recording = Recording::new(nodes.definition);
    observations::cancel_definition_creation(nodes.leaf.as_id());
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| controlled_member_operation(&prepared, DirectRequest { builder: &mut builder, node: nodes.node }, &funded())));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(function_annotation_state(&builder), initial);
    assert_eq!(builder.expression_cache.as_ref().map(|cache| (Rc::as_ptr(cache), Rc::strong_count(cache))), cache);
    {
        let journal = recording.0.borrow();
        assert_restored(&journal, parameter);
        assert_child_state(&journal, &nodes, parameter)?;
        let child = selected_child(&journal, &nodes)?;
        assert!(child.pending > 0);
        let lifetimes = lifetime_observations::snapshot();
        assert!(!lifetimes.overflowed);
        let dropped = lifetimes.events[..lifetimes.count].iter().position(|event| matches!(event, Some(lifetime_observations::Event::SourceChildDropped { definition: Some(definition) }) if *definition == nodes.leaf.as_id())).ok_or_else(|| anyhow::anyhow!("cold Leaf builder did not retire"))?;
        let restored = journal.transactions[child.transaction.0].restored.ok_or_else(|| anyhow::anyhow!("child's checkpoint was not restored"))?;
        assert!(dropped < restored.lifetime_events);
    }
    assert_missing(&db, nodes.definition);
    assert_leaf_ready(&db, nodes.leaf);
    cleanup();
    drop(builder);
    drop(recording);
    let mut reader = db.clone();
    reader.take_salsa_events();
    let recording = Recording::new(nodes.definition);
    let mut retry = make_builder();
    assert_eq!(controlled_member_operation(&prepared, DirectRequest { builder: &mut retry, node: nodes.node }, &funded()).map_err(|error| anyhow::anyhow!("{error:?}"))?, AnalysisOutcome::Complete(()));
    assert_eq!(function_annotation_state(&retry), initial);
    assert_completed(&recording.0.borrow(), parameter);
    assert_function_query_was_not_run_by_name(&db, "infer_definition_types", Some(nodes.leaf.as_id()), &reader.take_salsa_events());
    drop(recording);
    let Some(actual) = retry.try_expression_type(nodes.default) else { anyhow::bail!("retry did not store the default"); };
    let ordinary_db = fixture(parameter)?;
    let ordinary_prepared = prepare(&ordinary_db)?;
    let ordinary_nodes = self::nodes(&ordinary_db, &ordinary_prepared)?;
    let ordinary = infer_deferred_types(&ordinary_db, ordinary_nodes.definition);
    assert_eq!(actual.display(&db, &env).to_string(), ordinary.expression_type(ordinary_nodes.default).display(&ordinary_db, &ProgramEnvironment::from_file(ordinary_prepared.program_file())).to_string());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    drop(retry);
    cleanup();
    Ok(())
}
