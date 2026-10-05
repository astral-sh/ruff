//! Observes tuple preparation, delivered child contexts, and retained owners in the source runtime.
//!
//! These controls need Rust access to admission, canonical memo identity, and owner retirement;
//! the typing behavior remains covered by tuple and bidirectional-inference mdtests.

use std::future::{Future, poll_fn};
use std::panic::AssertUnwindSafe;
use std::pin::pin;

use ruff_text_size::{Ranged, TextRange};
use test_case::test_case;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::tuple::{TupleSpec, TupleType, VariableLengthTuple, VariableSegment};

const CAPACITY: usize = 16;
const STAGES: usize = 8;

/// Identifies an actual source-runtime boundary at which a retained tuple can be interrupted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum Stage {
    Prepared,
    FirstElement,
    BeforeResize,
    AfterResize,
    BeforeAnnotations,
    AfterAnnotations,
    BeforeTupleTransfer,
    AfterTupleTransfer,
}

/// Records the fixture annotations without extending a database value's lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Annotation {
    Absent,
    Any,
    Unknown,
    Tuple,
    Other,
}

impl Annotation {
    fn of(ty: Option<Type<'_>>) -> Self {
        match ty {
            None => Self::Absent,
            Some(ty) if ty == Type::any() => Self::Any,
            Some(ty) if ty == Type::unknown() => Self::Unknown,
            Some(Type::NominalInstance(instance)) if instance.exact_tuple().is_some() => {
                Self::Tuple
            }
            Some(_) => Self::Other,
        }
    }
}

/// Stores fixed-size observation data; the journal itself does not allocate during inference.
#[derive(Clone, Copy, Debug)]
struct Snapshot {
    remaining: [Option<usize>; STAGES],
    boundaries: [usize; STAGES],
    prepared: usize,
    specifications: usize,
    annotation_counts: [Option<usize>; CAPACITY],
    can_use: [Option<bool>; CAPACITY],
    elements: [Option<(TextRange, Annotation)>; CAPACITY],
    element_count: usize,
    created: usize,
    retired: usize,
    live: usize,
    peak_live: usize,
    pending_with_owner: usize,
    pending_after_preparation: usize,
    overflowed: bool,
}

impl Snapshot {
    const fn new() -> Self {
        Self {
            remaining: [None; STAGES],
            boundaries: [0; STAGES],
            prepared: 0,
            specifications: 0,
            annotation_counts: [None; CAPACITY],
            can_use: [None; CAPACITY],
            elements: [None; CAPACITY],
            element_count: 0,
            created: 0,
            retired: 0,
            live: 0,
            peak_live: 0,
            pending_with_owner: 0,
            pending_after_preparation: 0,
            overflowed: false,
        }
    }
}

thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static CANCEL: Cell<Option<Stage>> = const { Cell::new(None) };
    static SNAPSHOT: Cell<Snapshot> = const { Cell::new(Snapshot::new()) };
}

/// Restricts observations and cancellation to the controlled operation in this scope.
#[derive(Debug)]
struct Recording;

impl Recording {
    fn start(cancel: Option<Stage>) -> Self {
        assert!(!ACTIVE.replace(true));
        CANCEL.set(cancel);
        SNAPSHOT.set(Snapshot::new());
        observations::reset(None);
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        ACTIVE.set(false);
        CANCEL.set(None);
    }
}

/// Marks the tuple state's lifetime; the driver stores this after its owned buffers.
/// Retirement therefore observes the point after those buffers have been destroyed.
#[derive(Debug)]
pub(in crate::types::infer) struct OwnerLifetime {
    recording: bool,
}

impl OwnerLifetime {
    pub(in crate::types::infer) fn new() -> Self {
        let recording = ACTIVE.get();
        if recording {
            let mut snapshot = SNAPSHOT.get();
            snapshot.created += 1;
            snapshot.live += 1;
            snapshot.peak_live = snapshot.peak_live.max(snapshot.live);
            SNAPSHOT.set(snapshot);
        }
        Self { recording }
    }
}

impl Drop for OwnerLifetime {
    fn drop(&mut self) {
        if self.recording {
            let mut snapshot = SNAPSHOT.get();
            snapshot.live -= 1;
            snapshot.retired += 1;
            SNAPSHOT.set(snapshot);
        }
    }
}

/// Captures a production boundary and optionally requests native local cancellation there.
pub(in crate::types::infer) fn observe_boundary(db: &dyn Db, stage: Stage) {
    if !ACTIVE.get() {
        return;
    }
    let mut snapshot = SNAPSHOT.get();
    snapshot.boundaries[stage as usize] += 1;
    if snapshot.remaining[stage as usize].is_none() {
        snapshot.remaining[stage as usize] =
            salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
    }
    SNAPSHOT.set(snapshot);
    if CANCEL.get() == Some(stage) {
        CANCEL.set(None);
        db.cancellation_token().cancel();
    }
}

/// Records the completed production preparation before its owned result is installed in the state.
pub(in crate::types::infer) fn observe_prepared(
    db: &dyn Db,
    spec: Option<&TupleSpec<'_>>,
    annotations: &[Type<'_>],
    can_use_type_context: bool,
) {
    if !ACTIVE.get() {
        return;
    }
    let mut snapshot = SNAPSHOT.get();
    let index = snapshot.prepared;
    snapshot.prepared += 1;
    snapshot.specifications += usize::from(spec.is_some());
    if index < CAPACITY {
        snapshot.annotation_counts[index] = Some(annotations.len());
        snapshot.can_use[index] = Some(can_use_type_context);
    } else {
        snapshot.overflowed = true;
    }
    SNAPSHOT.set(snapshot);
    observe_boundary(db, Stage::Prepared);
}

/// Records the context the production driver is about to deliver to a source child expression.
pub(in crate::types::infer) fn observe_element(
    db: &dyn Db,
    expression: &ast::Expr,
    context: TypeContext<'_>,
) {
    if !ACTIVE.get() {
        return;
    }
    let mut snapshot = SNAPSHOT.get();
    let index = snapshot.element_count;
    snapshot.element_count += 1;
    if index < CAPACITY {
        snapshot.elements[index] = Some((expression.range(), Annotation::of(context.annotation)));
    } else {
        snapshot.overflowed = true;
    }
    SNAPSHOT.set(snapshot);
    if index == 0 {
        observe_boundary(db, Stage::FirstElement);
    }
}

/// Observes real suspensions of an expression-inference body while its tuple owners remain live.
/// The outer query request can remain pending throughout this body's execution; observing that
/// request would miss the lifetime of owners created and retired inside the body.
pub(in crate::types::infer::source_runtime) async fn observe_inference<F: Future>(
    inference: F,
) -> F::Output {
    let mut inference = pin!(inference);
    poll_fn(|cx| {
        let result = inference.as_mut().poll(cx);
        if result.is_pending() && ACTIVE.get() {
            let mut snapshot = SNAPSHOT.get();
            if snapshot.live > 0 {
                snapshot.pending_with_owner += 1;
                if snapshot.prepared > 0 {
                    snapshot.pending_after_preparation += 1;
                }
            }
            SNAPSHOT.set(snapshot);
        }
        result
    })
    .await
}

/// Describes canonical annotation inputs constructed inside the existing controlled run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Context {
    Absent,
    NonTuple,
    Homogeneous,
    Fixed(usize),
    Mixed,
    Nested,
}

impl Context {
    fn specification<'db>(self, nested: Option<Type<'db>>) -> Option<TupleSpec<'db>> {
        match self {
            Self::Absent | Self::NonTuple => None,
            Self::Homogeneous => Some(TupleSpec::homogeneous(Type::any())),
            Self::Fixed(length) => Some(TupleSpec::heterogeneous(
                [Type::any(), Type::unknown(), Type::any()]
                    .into_iter()
                    .take(length),
            )),
            Self::Mixed => Some(VariableLengthTuple::mixed(
                [Type::any()],
                VariableSegment::Homogeneous(Type::unknown()),
                [Type::any()],
            )),
            Self::Nested => nested.map(TupleSpec::homogeneous),
        }
    }

    fn type_context<'db>(self, tuple: Option<TupleType<'db>>) -> TypeContext<'db> {
        TypeContext::new(match self {
            Self::Absent => None,
            Self::NonTuple => Some(Type::any()),
            Self::Homogeneous | Self::Fixed(_) | Self::Mixed | Self::Nested => {
                tuple.map(Type::tuple)
            }
        })
    }
}

/// Requests the real contextual expression query, using the existing session, routes, and owners.
#[derive(Debug)]
struct ContextualTuple<'db> {
    expression: Expression<'db>,
    context: Context,
}

#[derive(Debug)]
struct Completed<'db> {
    inference: &'db ExpressionInference<'db>,
    context: TypeContext<'db>,
}

impl<'db> MemberOperation<'db> for ContextualTuple<'db> {
    type Output = Completed<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let nested = if self.context == Context::Nested {
            Some(Type::tuple(
                access
                    .intern_tuple(program, TupleSpec::homogeneous(Type::any()))
                    .await?,
            ))
        } else {
            None
        };
        let endpoint = access.endpoint();
        // At most three fixture elements are allocated here. Include their relocation and
        // disposal separately from the representation bytes; production preparation pays its own costs.
        let spec = endpoint
            .local_call(|| {
                endpoint.admit_work(32)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: 24 * size_of::<Type<'db>>() + size_of::<TupleSpec<'db>>(),
                })?;
                endpoint.check_completion()?;
                Ok(self.context.specification(nested))
            })
            .await;
        let tuple = if let Some(spec) = spec {
            Some(access.intern_tuple(program, spec).await?)
        } else {
            None
        };
        let context = self.context.type_context(tuple);
        let inference = access.expression(self.expression, context).await?;
        Ok(Completed { inference, context })
    }
}

fn database(source: &str) -> anyhow::Result<TestDb> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file("src/main.py", format!("flag = True\nleft = right = {source}\n"))?;
    Ok(db)
}

fn complete<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    context: Context,
    policy: &AnalysisPolicy,
) -> anyhow::Result<Completed<'db>> {
    match controlled_member_operation(
        prepared,
        ContextualTuple {
            expression: super::expression(db),
            context,
        },
        policy,
    ) {
        Ok(AnalysisOutcome::Complete(result)) => Ok(result),
        result => anyhow::bail!("contextual tuple did not complete: {result:?}"),
    }
}

fn assert_cleanup(snapshot: Snapshot) {
    assert!(!snapshot.overflowed);
    assert_eq!(snapshot.live, 0);
    assert_eq!(snapshot.created, snapshot.retired);
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// Builds the same annotation in a separate ordinary database after the controlled run finishes.
fn ordinary_context<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    context: Context,
) -> TypeContext<'db> {
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let nested = (context == Context::Nested).then(|| {
        Type::tuple(TupleType::new(
            db,
            &env,
            &TupleSpec::homogeneous(Type::any()),
        ))
    });
    let tuple = context
        .specification(nested)
        .map(|spec| TupleType::new(db, &env, &spec));
    context.type_context(tuple)
}

/// Checks delivered contexts, ordinary parity, and canonical reuse after cold tuple preparation.
/// The absent-context case resolves `flag` so a child query suspends while the tuple owner lives.
#[test_case("(flag, False)", Context::Absent, &[Annotation::Absent, Annotation::Absent], None; "absent context")]
#[test_case("(True, False)", Context::NonTuple, &[Annotation::Absent, Annotation::Absent], None; "non tuple context")]
#[test_case("()", Context::Homogeneous, &[], Some(0); "empty homogeneous tuple")]
#[test_case("(True, False, True)", Context::Homogeneous, &[Annotation::Any, Annotation::Any, Annotation::Any], Some(3); "several homogeneous elements")]
#[test_case("(True, False)", Context::Fixed(2), &[Annotation::Any, Annotation::Unknown], Some(2); "fixed order")]
#[test_case("(True, False)", Context::Fixed(1), &[Annotation::Absent, Annotation::Absent], None; "too many source elements")]
#[test_case("(True,)", Context::Fixed(2), &[Annotation::Absent], None; "too few source elements")]
#[test_case("(True, False, True)", Context::Mixed, &[Annotation::Any, Annotation::Unknown, Annotation::Any], Some(3); "prefix variable suffix order")]
#[test_case("(True, False)", Context::Mixed, &[Annotation::Any, Annotation::Any], Some(2); "zero variable repetitions")]
#[test_case("(True,)", Context::Mixed, &[Annotation::Absent], None; "variable minimum length recovery")]
fn preparation_contexts_match_ordinary_results_and_reuse_canonical_memos(
    source: &str,
    context: Context,
    expected: &[Annotation],
    resized_length: Option<usize>,
) -> anyhow::Result<()> {
    let db = database(source)?;
    let prepared = prepare(&db);
    let expression = expression(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let recording = Recording::start(None);
    let completed = complete(&db, &prepared, context, &funded())?;
    let snapshot = SNAPSHOT.get();
    drop(recording);
    assert_cleanup(snapshot);
    assert_eq!(snapshot.prepared, 1);
    assert_eq!(
        snapshot.specifications,
        usize::from(resized_length.is_some())
    );
    assert_eq!(
        snapshot.annotation_counts[0],
        Some(resized_length.unwrap_or(0))
    );
    assert_eq!(snapshot.can_use[0], Some(true));
    assert_eq!(snapshot.element_count, expected.len());
    let ast::Expr::Tuple(tuple) = expression.node_ref(&db).node(prepared.parsed_module()) else {
        anyhow::bail!("fixture must end in a tuple expression");
    };
    let actual: Vec<_> = snapshot.elements.iter().flatten().copied().collect();
    let expected: Vec<_> = tuple
        .elts
        .iter()
        .zip(expected)
        .map(|(element, annotation)| (element.range(), *annotation))
        .collect();
    assert_eq!(actual, expected);
    assert!(snapshot.pending_with_owner > 0);
    let expected_types = tuple
        .elts
        .iter()
        .map(|element| match element {
            ast::Expr::BooleanLiteral(literal) => Ok(Type::bool_literal(literal.value)),
            ast::Expr::Name(name) if name.id == "flag" => Ok(Type::bool_literal(true)),
            _ => anyhow::bail!("parity fixture must contain only boolean literals or flag"),
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let expected_tuple = TupleType::new(&db, &env, &TupleSpec::heterogeneous(expected_types));
    assert_eq!(
        completed
            .inference
            .expression_type(expression.node_ref(&db)),
        Type::tuple(expected_tuple),
    );
    let key = InferExpression::new(&db, expression, completed.context).as_id();
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, expression_inference_ingredient(&db), key).is_ok()
    );

    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let repeated = complete(&db, &prepared, context, &funded())?;
    assert!(std::ptr::eq(completed.inference, repeated.inference));
    assert!(std::ptr::eq(
        completed.inference,
        infer_expression_types(&db, expression, completed.context),
    ));
    assert_function_query_was_not_run_by_name(
        &db,
        "infer_expression_types_impl",
        None,
        &events_db.take_salsa_events(),
    );
    let ordinary_db = database(source)?;
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_expression = super::expression(&ordinary_db);
    let ordinary = infer_expression_types(
        &ordinary_db,
        ordinary_expression,
        ordinary_context(&ordinary_db, &ordinary_prepared, context),
    );
    assert_eq!(
        completed
            .inference
            .expression_type(expression.node_ref(&db))
            .display(&db, &ProgramEnvironment::from_file(prepared.program_file()))
            .to_string(),
        ordinary
            .expression_type(ordinary_expression.node_ref(&ordinary_db))
            .display(&ordinary_db, &ProgramEnvironment::from_file(ordinary_prepared.program_file()))
            .to_string(),
    );
    assert_eq!(
        completed.inference.expressions.iter().len(),
        ordinary.expressions.iter().len()
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup(SNAPSHOT.get());
    Ok(())
}

/// Exercises nested contextual tuples through the existing driver while outer annotation storage lives.
#[test]
fn nested_tuple_children_retain_outer_preparation() -> anyhow::Result<()> {
    let db = database("((True,), (False,))")?;
    let prepared = prepare(&db);
    let recording = Recording::start(None);
    let result = complete(&db, &prepared, Context::Nested, &funded())?;
    let snapshot = SNAPSHOT.get();
    drop(recording);
    assert_cleanup(snapshot);
    assert_eq!(snapshot.prepared, 3);
    assert_eq!(snapshot.created, 3);
    assert_eq!(snapshot.peak_live, 2);
    assert_eq!(snapshot.annotation_counts[..3], [Some(2), Some(1), Some(1)]);
    assert_eq!(snapshot.element_count, 4);
    assert_eq!(
        snapshot
            .elements
            .iter()
            .flatten()
            .map(|(_, annotation)| *annotation)
            .collect::<Vec<_>>(),
        [
            Annotation::Tuple,
            Annotation::Any,
            Annotation::Tuple,
            Annotation::Any
        ],
    );
    assert!(snapshot.pending_after_preparation > 0);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let nested = [true, false].map(|value| {
        Type::tuple(TupleType::new(
            &db,
            &env,
            &TupleSpec::heterogeneous([Type::bool_literal(value)]),
        ))
    });
    let expected = Type::tuple(TupleType::new(&db, &env, &TupleSpec::heterogeneous(nested)));
    assert_eq!(
        result
            .inference
            .expression_type(expression(&db).node_ref(&db)),
        expected
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Limit {
    Work,
    Bytes,
}

/// Calibrates one cold boundary in a fresh database, so completed children cannot hide admission.
fn calibration(policy: &AnalysisPolicy) -> anyhow::Result<Snapshot> {
    let db = database("(True, False, True)")?;
    let prepared = prepare(&db);
    let recording = Recording::start(None);
    let result = controlled_member_operation(
        &prepared,
        ContextualTuple {
            expression: expression(&db),
            context: Context::Homogeneous,
        },
        policy,
    );
    match result {
        Ok(AnalysisOutcome::Complete(_))
        | Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit | AnalysisIncomplete::WorkLimit,
            ..
        }) => {}
        result => anyhow::bail!("unexpected calibration outcome: {result:?}"),
    }
    let snapshot = SNAPSHOT.get();
    drop(recording);
    assert_cleanup(snapshot);
    Ok(snapshot)
}

/// Selects a work refusal immediately after a boundary, or a byte refusal immediately before it.
fn refusal_policy(stage: Stage, limit: Limit) -> anyhow::Result<AnalysisPolicy> {
    match limit {
        Limit::Work => {
            let remaining = calibration(&funded())?.remaining[stage as usize]
                .ok_or_else(|| anyhow::anyhow!("funded run missed {stage:?}"))?;
            Ok(AnalysisPolicy {
                semantic_work_limit: funded().semantic_work_limit - remaining,
                ..funded()
            })
        }
        Limit::Bytes => {
            let mut lower = 0;
            let mut upper = funded().requested_bytes_limit;
            while lower < upper {
                let middle = lower + (upper - lower) / 2;
                let snapshot = calibration(&AnalysisPolicy {
                    requested_bytes_limit: middle,
                    ..funded()
                })?;
                if snapshot.remaining[stage as usize].is_some() {
                    upper = middle;
                } else {
                    lower = middle + 1;
                }
            }
            let requested_bytes_limit = upper
                .checked_sub(1)
                .ok_or_else(|| anyhow::anyhow!("tuple boundary required no allocation"))?;
            Ok(AnalysisPolicy {
                requested_bytes_limit,
                ..funded()
            })
        }
    }
}

/// Verifies refusal withholds the parent memo, drains retained tuple state, and permits a same-revision retry.
#[test_case(Stage::Prepared, Limit::Work; "work after preparation")]
#[test_case(Stage::FirstElement, Limit::Work; "work after first context delivery")]
#[test_case(Stage::Prepared, Limit::Bytes; "bytes before prepared result")]
#[test_case(Stage::FirstElement, Limit::Bytes; "bytes before first context delivery")]
fn resource_refusal_drains_retained_preparation_and_retries(
    stage: Stage,
    limit: Limit,
) -> anyhow::Result<()> {
    let policy = refusal_policy(stage, limit)?;
    let db = database("(True, False, True)")?;
    let prepared = prepare(&db);
    let expression = expression(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let recording = Recording::start(None);
    let result = controlled_member_operation(
        &prepared,
        ContextualTuple {
            expression,
            context: Context::Homogeneous,
        },
        &policy,
    );
    let expected_reason = match limit {
        Limit::Work => AnalysisIncomplete::WorkLimit,
        Limit::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
    };
    assert!(
        matches!(result, Ok(AnalysisOutcome::Incomplete { reason, .. }) if reason == expected_reason),
        "{result:?}"
    );
    let snapshot = SNAPSHOT.get();
    drop(recording);
    assert_cleanup(snapshot);
    assert_eq!(snapshot.created, 1);
    assert_eq!(
        snapshot.remaining[stage as usize].is_some(),
        limit == Limit::Work
    );
    let key = executed_expression_key(&db, &events_db.take_salsa_events())?;
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, expression_inference_ingredient(&db), key)
            .is_err()
    );

    let recording = Recording::start(None);
    let retried = complete(&db, &prepared, Context::Homogeneous, &funded())?;
    let snapshot = SNAPSHOT.get();
    drop(recording);
    assert_cleanup(snapshot);
    assert_eq!(snapshot.element_count, 3);
    assert_eq!(
        InferExpression::new(&db, expression, retried.context).as_id(),
        key
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    let reused = complete(&db, &prepared, Context::Homogeneous, &funded())?;
    assert!(std::ptr::eq(retried.inference, reused.inference));
    Ok(())
}

/// Requests native cancellation with tuple state live, then verifies cleanup and canonical retry.
#[test_case(Stage::AfterResize; "cancel after resized buffer")]
#[test_case(Stage::AfterAnnotations; "cancel after annotation buffer")]
#[test_case(Stage::Prepared; "cancel after preparation")]
#[test_case(Stage::FirstElement; "cancel after first context delivery")]
fn cancellation_retires_preparation_and_retries(stage: Stage) -> anyhow::Result<()> {
    let db = database("(True, False, True)")?;
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let recording = Recording::start(Some(stage));
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(
            &prepared,
            ContextualTuple {
                expression: expression(&db),
                context: Context::Homogeneous,
            },
            &funded(),
        )
    }));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    let snapshot = SNAPSHOT.get();
    drop(recording);
    assert_cleanup(snapshot);
    assert_eq!(snapshot.created, 1);
    assert!(snapshot.remaining[stage as usize].is_some());
    let recording = Recording::start(None);
    let retried = complete(&db, &prepared, Context::Homogeneous, &funded())?;
    let snapshot = SNAPSHOT.get();
    drop(recording);
    assert_cleanup(snapshot);
    let reused = complete(&db, &prepared, Context::Homogeneous, &funded())?;
    assert!(std::ptr::eq(retried.inference, reused.inference));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}

/// Finds the first expression query entered by a run whose root request is a contextual expression.
/// Its recorded key permits memo checks after refusal without performing ordinary inference.
fn executed_expression_key(db: &TestDb, events: &[salsa::Event]) -> anyhow::Result<salsa::Id> {
    let event = find_will_execute_event_by_name(db, "infer_expression_types_impl", None, events)
        .ok_or_else(|| anyhow::anyhow!("controlled root did not enter its expression query"))?;
    let salsa::EventKind::WillExecute { database_key } = event.kind else {
        anyhow::bail!("expression execution lookup returned a different event kind");
    };
    Ok(database_key.key_index())
}

/// Keeps unsupported child operations exact after successful preparation, with no parent publication.
#[test_case("(True, [False], True)", OperationId::ExpressionKind, 2; "list child remains unavailable")]
#[test_case("(*[True],)", OperationId::ContextualExpression, 0; "starred iterable context remains unavailable")]
fn unavailable_children_retire_preparation_without_publishing_a_parent(
    source: &str,
    operation: OperationId,
    delivered: usize,
) -> anyhow::Result<()> {
    let db = database(source)?;
    let prepared = prepare(&db);
    let source_expression = expression(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let recording = Recording::start(None);
    let first = controlled_member_operation(
        &prepared,
        ContextualTuple {
            expression: source_expression,
            context: Context::Homogeneous,
        },
        &funded(),
    );
    assert!(
        matches!(first, Ok(AnalysisOutcome::Incomplete {
        reason: AnalysisIncomplete::UnavailableOperation(actual), ..
    }) if actual == operation),
        "{first:?}"
    );
    let snapshot = SNAPSHOT.get();
    drop(recording);
    assert_cleanup(snapshot);
    assert_eq!(snapshot.prepared, 1);
    assert_eq!(snapshot.element_count, delivered);
    assert_eq!(snapshot.created, 1);
    let key = executed_expression_key(&db, &events_db.take_salsa_events())?;
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, expression_inference_ingredient(&db), key)
            .is_err()
    );

    let recording = Recording::start(None);
    let repeated = controlled_member_operation(
        &prepared,
        ContextualTuple {
            expression: source_expression,
            context: Context::Homogeneous,
        },
        &funded(),
    );
    assert!(
        matches!(repeated, Ok(AnalysisOutcome::Incomplete {
        reason: AnalysisIncomplete::UnavailableOperation(actual), ..
    }) if actual == operation),
        "{repeated:?}"
    );
    let snapshot = SNAPSHOT.get();
    drop(recording);
    assert_cleanup(snapshot);
    assert_eq!(snapshot.element_count, delivered);
    assert_eq!(
        executed_expression_key(&db, &events_db.take_salsa_events())?,
        key
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    Ok(())
}

/// Verifies denied buffer constructors do not run and denied final tuple construction does not return.
/// The same database then completes with the original configured limits and reuses its final memo.
#[test_case(Stage::BeforeResize, Stage::AfterResize, Limit::Work; "work before resized buffer")]
#[test_case(Stage::BeforeResize, Stage::AfterResize, Limit::Bytes; "bytes before resized buffer")]
#[test_case(Stage::BeforeAnnotations, Stage::AfterAnnotations, Limit::Work; "work before annotation buffer")]
#[test_case(Stage::BeforeAnnotations, Stage::AfterAnnotations, Limit::Bytes; "bytes before annotation buffer")]
#[test_case(Stage::BeforeTupleTransfer, Stage::AfterTupleTransfer, Limit::Work; "work before final tuple transfer")]
#[test_case(Stage::BeforeTupleTransfer, Stage::AfterTupleTransfer, Limit::Bytes; "bytes before final tuple transfer")]
fn denied_construction_has_no_completed_action(
    before: Stage,
    after: Stage,
    limit: Limit,
) -> anyhow::Result<()> {
    let boundary = match limit {
        Limit::Work => before,
        Limit::Bytes => after,
    };
    let policy = refusal_policy(boundary, limit)?;
    let db = database("(True, False, True)")?;
    let prepared = prepare(&db);
    let source_expression = expression(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let recording = Recording::start(None);
    let result = controlled_member_operation(
        &prepared,
        ContextualTuple {
            expression: source_expression,
            context: Context::Homogeneous,
        },
        &policy,
    );
    let expected = match limit {
        Limit::Work => AnalysisIncomplete::WorkLimit,
        Limit::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
    };
    assert!(
        matches!(result, Ok(AnalysisOutcome::Incomplete { reason, .. }) if reason == expected),
        "{result:?}"
    );
    let snapshot = SNAPSHOT.get();
    drop(recording);
    assert_cleanup(snapshot);
    assert_eq!(snapshot.boundaries[before as usize], 1);
    assert_eq!(snapshot.boundaries[after as usize], 0);
    assert_eq!(snapshot.created, 1);
    let key = executed_expression_key(&db, &events_db.take_salsa_events())?;
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, expression_inference_ingredient(&db), key)
            .is_err()
    );

    let recording = Recording::start(None);
    let retried = complete(&db, &prepared, Context::Homogeneous, &funded())?;
    let snapshot = SNAPSHOT.get();
    drop(recording);
    assert_cleanup(snapshot);
    assert_eq!(snapshot.boundaries[after as usize], 1);
    assert_eq!(
        InferExpression::new(&db, source_expression, retried.context).as_id(),
        key
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    let reused = complete(&db, &prepared, Context::Homogeneous, &funded())?;
    assert!(std::ptr::eq(retried.inference, reused.inference));
    Ok(())
}
