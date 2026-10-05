//! Native source and registered generated queries share canonical keys and one attempt.
//! These fixtures deliberately retain the ordinary native equations as the answer oracle.

use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn, ready};
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use super::super::native_source::{self, NativeTrace};
use super::super::registration::{
    CallableRoute, CallableRouteProvider, ExecutableRouteProvider, FinalSourceMemo, FixedQueryKeys,
    NativeCallbackLimits, NativeSourceRoute, ProviderContext, RegistryBuilder, RouteProvider,
    TaskEndpoint, with_native_callback,
};
use super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, try_with_attempt};
#[cfg(not(feature = "shuttle"))]
use crate::attempt_probe::transfer_test_support::{self, Kind, MemoSnapshot, TraceConfig, TransferTrace};
#[cfg(not(feature = "shuttle"))]
use crate::DatabaseKeyIndex;
use crate::function::{ClaimResult, Configuration, IngredientImpl, InternedQueryConfiguration, Reentrancy, VerifyResult};
use crate::plumbing::{AsId, QuoteError, QuoteFuel};
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, EventKind, Id, Revision, Setter};

#[derive(Clone, Debug)]
enum Event {
    Source(Id),
    NativeGenerated(Id),
    SourceReturn(Id, u32),
    NativeGeneratedReturn(Id, u32),
    Registered(Id, Option<usize>),
    Initial(&'static str, Id),
    Recovery(&'static str, Id, u32, u32),
    Work(ExecutionWork),
    Native(String),
}

thread_local! {
    static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static LIMIT: Cell<usize> = const { Cell::new(3) };
    static SEED_ENTRY: Cell<bool> = const { Cell::new(false) };
    static CHECK_LIMIT: Cell<bool> = const { Cell::new(false) };
}

fn record(event: Event) { EVENTS.with_borrow_mut(|events| events.push(event)); }
fn events() -> Vec<Event> { EVENTS.with_borrow(|events| events.clone()) }
fn clear_trace() { EVENTS.with_borrow_mut(Vec::clear); native_source::take_trace(); }

#[cfg(not(feature = "shuttle"))]
fn collect_support<T>(body: impl FnOnce() -> T) -> (T, TransferTrace) {
    let (value, trace) = transfer_test_support::collect(TraceConfig {
        ordinal: Arc::new(std::sync::atomic::AtomicUsize::new(0)), worker: 0,
    }, body);
    assert!(!trace.broken, "support collector must retain every record");
    assert!(!format!("{trace:?}").contains("overflow: true"), "support observation slots must be complete");
    (value, trace)
}

#[cfg(not(feature = "shuttle"))]
fn incomplete_publication(trace: &TransferTrace, key: DatabaseKeyIndex, address: usize) -> MemoSnapshot {
    let memo = trace.records.iter().find_map(|record| {
        let event = &record.event;
        (event.kind == Kind::RootPublished && event.key == Some(key))
            .then_some(event.memo).flatten().filter(|memo| memo.identity == address)
    }).expect("the observed native root published its own memo");
    assert!(memo.has_value);
    assert!(memo.support.is_some_and(|support| support.explicitly_incomplete));
    memo
}

fn limits() -> NativeCallbackLimits {
    NativeCallbackLimits::new(NonZeroUsize::new(LIMIT.with(Cell::get)).expect("positive fixture limit"))
}

#[crate::db]
#[derive(Clone)]
struct TestDb { storage: crate::Storage<Self> }

impl Default for TestDb {
    fn default() -> Self {
        Self { storage: crate::Storage::new(Some(Box::new(|event| {
            let cancellation = matches!(event.kind, EventKind::WillCheckCancellation);
            record(Event::Native(format!("{:?}", event.kind)));
            if cancellation { fire_hook(HookStage::Callout); }
        }))) }
    }
}

#[crate::db]
impl Database for TestDb {}

#[crate::input]
struct Node {
    #[returns(copy)]
    next: Option<Node>,
    #[returns(copy)]
    leaf: u32,
}

fn combine(next: Option<u32>, leaf: u32) -> u32 {
    next.map_or(leaf, |value| (value + 1).min(3))
}

fn complete_or_refused(result: RunResult<u32>) -> u32 {
    match result {
        Ok(value) => value,
        Err(RunError::Refused(_)) => 0,
        Err(error) => panic!("native entry contract failed: {error:?}"),
    }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = source_initial, cycle_fn = source_recover)]
fn source(db: &dyn Database, node: Node) -> u32 {
    record(Event::Source(node.as_id()));
    fire_hook(HookStage::Before);
    if native_source::current_driver().is_some() && CHECK_LIMIT.with(|flag| flag.replace(false)) {
        let called = Cell::new(false);
        let changed = NativeCallbackLimits::new(NonZeroUsize::new(LIMIT.with(Cell::get) + 1).expect("larger positive limit"));
        assert!(matches!(with_native_callback(db, changed, |_| { called.set(true); Ok(()) }), Err(RunError::Contract(_))));
        assert!(!called.get());
    }
    let value = if attempt_probe::current().is_some() {
        complete_or_refused(request_generated(db, node,
            source::fn_ingredient_(db, db.zalsa()), generated::fn_ingredient_(db, db.zalsa())).copied())
    } else {
        *generated(db, node)
    };
    fire_hook(HookStage::After);
    record(Event::SourceReturn(node.as_id(), value));
    value
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = generated_initial, cycle_fn = generated_recover)]
fn generated(db: &dyn Database, node: Node) -> u32 {
    record(Event::NativeGenerated(node.as_id()));
    let value = if attempt_probe::current().is_some() {
        complete_or_refused(native_generated_body(db, node,
            source::fn_ingredient_(db, db.zalsa()), generated::fn_ingredient_(db, db.zalsa())))
    } else {
        combine(node.next(db).map(|next| *source(db, next)), node.leaf(db))
    };
    record(Event::NativeGeneratedReturn(node.as_id(), value));
    value
}

fn source_initial(db: &dyn Database, _id: Id, node: Node) -> u32 {
    record(Event::Initial("source", node.as_id()));
    if SEED_ENTRY.with(Cell::get) { seed_operation(db, node) } else { 0 }
}
fn generated_initial(_db: &dyn Database, _id: Id, node: Node) -> u32 {
    record(Event::Initial("generated", node.as_id())); 0
}
fn source_recover(_db: &dyn Database, _cycle: &Cycle<'_>, old: &u32, value: u32, node: Node) -> u32 {
    record(Event::Recovery("source", node.as_id(), *old, value)); value
}
fn generated_recover(_db: &dyn Database, _cycle: &Cycle<'_>, old: &u32, value: u32, node: Node) -> u32 {
    record(Event::Recovery("generated", node.as_id(), *old, value)); value
}

struct Admission;
impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        record(Event::Work(work));
        fire_hook(HookStage::Callout);
        admission_action()
    }
}
static ADMISSION: Admission = Admission;

struct GraphProvider<'db, S: Configuration> { source: NativeSourceRoute<'db, S> }

async fn shared_generated_body<'call, 'run: 'call, 'db: 'run, S>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    db: &'db dyn Database,
    source: &'call NativeSourceRoute<'db, S>,
    node: Node,
) -> RunResult<u32>
where S: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    endpoint.local_call(|| {
        record(Event::Registered(node.as_id(), attempt_probe::remaining_allowance_for_diagnostics(db)));
        endpoint.admit_work(1)
    }).await;
    #[cfg(not(feature = "shuttle"))]
    exercise_cancellation(endpoint, db, node).await;
    exercise_inner(endpoint, db).await;
    let next = endpoint.local_call(|| Ok(node.next(db))).await;
    let value = match next {
        Some(next) => Some(*endpoint.read_native_source(source, next.as_id()).await),
        None => None,
    };
    let leaf = endpoint.local_call(|| Ok(node.leaf(db))).await;
    Ok(combine(value, leaf))
}

impl<'run, 'db: 'run, S, G> CallableRouteProvider<'run, 'db, G> for GraphProvider<'db, S>
where
    S: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
    G: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    // Node conversion constructs a handle; output equality compares u32.
    fixture_native_value!(callable, 'run, 'db, G, 1);

    async fn body<'call>(&'call self, endpoint: TaskEndpoint<'run, 'db>, db: &'db dyn Database, node: Node)
        -> RunResult<u32> where 'run: 'call
    {
        shared_generated_body(&endpoint, db, &self.source, node).await
    }
    fn initial<'call>(&'call self, _endpoint: TaskEndpoint<'run, 'db>, db: &'db dyn Database, id: Id, node: Node)
        -> impl Future<Output = RunResult<u32>> + 'call where 'run: 'call
    { ready(Ok(G::cycle_initial(db, id, node))) }
    fn recover<'call>(&'call self, _endpoint: TaskEndpoint<'run, 'db>, db: &'db dyn Database,
        cycle: &'call Cycle<'call>, old: &'call u32, value: u32, node: Node)
        -> impl Future<Output = RunResult<u32>> + 'call where 'run: 'call
    { ready(Ok(G::recover_from_cycle(db, cycle, old, value, node))) }
}

fn request_generated<'db, S, G>(db: &'db dyn Database, node: Node,
    source: &'db IngredientImpl<S>, generated: &'db IngredientImpl<G>) -> RunResult<&'db u32>
where
    S: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
    G: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    with_native_callback(db, limits(), |entry| {
        let mut registry = RegistryBuilder::for_native_callback(db, &entry, &ADMISSION)?;
        let source = registry.register_native_source(db, source)?;
        let generated = registry.reserve_callable(db, generated)?;
        registry.bind_callable(&generated, GraphProvider { source })?;
        registry.seal()?.run(move |endpoint| async move {
            endpoint.fetch_ref(&generated, node.as_id())?.await
        })
    })
}

fn native_generated_body<'db, S, G>(db: &'db dyn Database, node: Node,
    source: &'db IngredientImpl<S>, generated: &'db IngredientImpl<G>) -> RunResult<u32>
where
    S: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
    G: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    with_native_callback(db, limits(), |entry| {
        let mut registry = RegistryBuilder::for_native_callback(db, &entry, &ADMISSION)?;
        let source = registry.register_native_source(db, source)?;
        let generated = registry.reserve_callable(db, generated)?;
        registry.bind_callable(&generated, GraphProvider { source: source.clone() })?;
        registry.seal()?.run(move |endpoint| async move {
            // Native Salsa already owns this G key. Evaluate its body, not a self-fetch.
            shared_generated_body(&endpoint, db, &source, node).await
        })
    })
}

fn registered_graph_root<'db, S, G>(db: &'db dyn Database, node: Node,
    source: &'db IngredientImpl<S>, generated: &'db IngredientImpl<G>) -> RunResult<u32>
where
    S: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
    G: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
    let source = registry.register_native_source(db, source)?;
    let generated = registry.reserve_callable(db, generated)?;
    registry.bind_callable(&generated, GraphProvider { source })?;
    registry.seal()?.run(move |endpoint| async move { Ok(*endpoint.fetch_ref(&generated, node.as_id())?.await?) })
}

fn graph(db: &mut TestDb, cycle: bool) -> [Node; 2] {
    let left = Node::new(db, None, 1);
    let right = Node::new(db, None, 1);
    left.set_next(db).to(Some(right));
    if cycle { right.set_next(db).to(Some(left)); }
    [left, right]
}

fn root(db: &dyn Database, node: Node, generated_root: bool) -> u32 {
    if generated_root { *generated(db, node) } else { *source(db, node) }
}

fn assert_restored(db: &dyn Database) {
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert!(native_source::current_driver().is_none());
    assert!(native_source::source_permit_is_clear());
    assert!(!super::super::RUN_ACTIVE.with(Cell::get));
}

#[test]
fn independently_cold_source_and_generated_cycles_have_distinct_depths() {
    let _trace = native_source::trace();
    for generated_root in [false, true] {
        for index in [0, 1] {
            let mut ordinary = TestDb::default();
            let original = graph(&mut ordinary, true);
            assert_eq!(root(&ordinary, original[index], generated_root), 3);
            let mut db = TestDb::default();
            let nodes = graph(&mut db, true);
            let ceiling = if generated_root { 3 } else { 2 };
            LIMIT.with(|limit| limit.set(ceiling));
            clear_trace();
            let allowance = 1_000_000;
            let mut remaining = None;
            let (captured, ownership) = super::observation::collect(|| crate::prepared_source_probe::capture(&db, || {
                try_with_attempt(&db, allowance, || {
                    let value = root(&db, nodes[index], generated_root);
                    remaining = attempt_probe::remaining_allowance_for_diagnostics(&db);
                    value
                })
            }).expect("capture starts outside all queries"));
            assert_eq!(captured.value, Ok(AttemptOutcome::Complete(3)));
            captured.check_root_reads().expect("completed canonical root");
            let trace = native_source::take_trace();
            let entries: Vec<_> = trace.iter().filter_map(|event| match event {
                NativeTrace::Enter { identity, caller, .. } => Some((identity.depth(), *caller)),
                _ => None,
            }).collect();
            assert_eq!(entries.iter().map(|(depth, _)| *depth).max(), Some(ceiling));
            assert_eq!(entries.iter().take(ceiling).map(|(depth, _)| *depth).collect::<Vec<_>>(),
                (1..=ceiling).collect::<Vec<_>>());
            let generated_key = generated::fn_ingredient_(&db, db.zalsa()).database_key_index(nodes[index].as_id());
            let source_key = source::fn_ingredient_(&db, db.zalsa()).database_key_index(nodes[index].as_id());
            assert_eq!(entries[0].1.key, Some(if generated_root { generated_key } else { source_key }));
            let other_source_key = source::fn_ingredient_(&db, db.zalsa()).database_key_index(nodes[1 - index].as_id());
            let expected_callers = if generated_root {
                vec![generated_key, other_source_key, source_key]
            } else { vec![source_key, other_source_key] };
            assert_eq!(entries.iter().take(ceiling).map(|(_, caller)| caller.key).collect::<Vec<_>>(),
                expected_callers.into_iter().map(Some).collect::<Vec<_>>());
            let requested_sources: Vec<_> = trace.iter().filter_map(|event| match event {
                NativeTrace::Read { key } => Some(*key), _ => None,
            }).take(2).collect();
            assert_eq!(requested_sources, [other_source_key, source_key]);
            assert_eq!(ownership.max_active_polls, ceiling);
            assert!(events().iter().any(|event| matches!(event, Event::Initial(_, _))));
            assert!(events().iter().any(|event| matches!(event, Event::Recovery(_, _, _, 3))));
            assert!(captured.reads.iter().any(|read| read.key == generated_key));
            assert!(captured.reads.iter().any(|read| read.key == source_key));
            println!("SOURCE_ENTRY_POSITIVE\tgenerated={generated_root}\tindex={index}\ttrace={trace:?}\tevents={:?}\treads={:?}\tclaims={:?}\tpolls={}\tpeak_polls={}", events(), captured.reads, ownership.events, ownership.polls, ownership.max_active_polls);
            let used = allowance - remaining.expect("successful attempt has remaining work");
            drop(captured);
            assert_restored(&db);

            // A separate cold database ensures the denied extra source/driver cannot be warmed away.
            let mut lower = TestDb::default();
            let lower_nodes = graph(&mut lower, true);
            LIMIT.with(|limit| limit.set(ceiling - 1));
            clear_trace();
            let lower_call = || crate::prepared_source_probe::capture(&lower, || {
                try_with_attempt(&lower, used + 100, || root(&lower, lower_nodes[index], generated_root))
            }).expect("lower attempt starts outside native queries");
            #[cfg(not(feature = "shuttle"))]
            let (lower_read, lower_support) = collect_support(lower_call);
            #[cfg(feature = "shuttle")]
            let lower_read = lower_call();
            assert_eq!(lower_read.value, Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)));
            let lower_key = if generated_root {
                generated::fn_ingredient_(&lower, lower.zalsa()).database_key_index(lower_nodes[index].as_id())
            } else {
                source::fn_ingredient_(&lower, lower.zalsa()).database_key_index(lower_nodes[index].as_id())
            };
            let incomplete_root = lower_read.reads.iter().find(|read| read.parent.is_none() && read.key == lower_key)
                .expect("the native root returned its actual fallback memo");
            assert_eq!(incomplete_root.status, crate::prepared_source_probe::Status::Incomplete);
            #[cfg(not(feature = "shuttle"))]
            let incomplete_address = incomplete_root.memo_address;
            #[cfg(not(feature = "shuttle"))]
            let published = incomplete_publication(&lower_support, lower_key, incomplete_address);
            let denied_trace = native_source::take_trace();
            let denied: Vec<_> = denied_trace.iter().filter_map(|event| match event {
                NativeTrace::Denied { depth, limit, remaining, caller } => Some((*depth, *limit, *remaining, *caller)),
                _ => None,
            }).collect();
            assert!(!denied.is_empty());
            assert_eq!(denied[0].0, ceiling);
            assert_eq!(denied[0].1, ceiling - 1);
            let denied_source = lower_nodes[if generated_root { index } else { 1 - index }];
            assert_eq!(denied[0].3.key, Some(source::fn_ingredient_(&lower, lower.zalsa()).database_key_index(denied_source.as_id())));
            assert!(denied[0].2.is_some_and(|work| work >= 1));
            assert!(!denied_trace.iter().any(|event| matches!(event,
                NativeTrace::Enter { identity, .. } if identity.depth() >= ceiling)));
            assert!(!events().iter().any(|event| matches!(event, Event::Initial(_, _))));
            println!("SOURCE_ENTRY_DEPTH_REFUSAL\tgenerated={generated_root}\tindex={index}\ttrace={denied_trace:?}\tevents={:?}", events());
            assert_restored(&lower);
            LIMIT.with(|limit| limit.set(ceiling));
            for retry in 0..2 {
                clear_trace();
                let retry_call = || crate::prepared_source_probe::capture(&lower, || {
                    try_with_attempt(&lower, used + 100, || root(&lower, lower_nodes[index], generated_root))
                }).expect("retry starts outside native queries");
                #[cfg(not(feature = "shuttle"))]
                let (retry_read, retry_support) = collect_support(retry_call);
                #[cfg(feature = "shuttle")]
                let retry_read = retry_call();
                assert_eq!(retry_read.value, Ok(AttemptOutcome::Complete(3)));
                retry_read.check_root_reads().expect("retry publishes a supported complete root");
                let recomputed = events().iter().any(|event| matches!(event,
                    Event::Source(id) if !generated_root && *id == lower_nodes[index].as_id())
                    || matches!(event, Event::NativeGenerated(id) if generated_root && *id == lower_nodes[index].as_id()));
                assert_eq!(recomputed, retry == 0);
                #[cfg(not(feature = "shuttle"))]
                {
                    if retry == 0 {
                        assert!(retry_support.records.iter().any(|record| record.event.kind == Kind::Reuse
                            && record.event.identity == incomplete_address
                            && record.event.reuse == Some(attempt_probe::MemoReuse::Stale)));
                    }
                    println!("SOURCE_ENTRY_RETRY_SUPPORT\tgenerated={generated_root}\tindex={index}\tretry={retry}\trecomputed={recomputed}\tbroken=false\tpublished={published:?}\treads={:?}\ttransfer={retry_support:?}", retry_read.reads);
                }
                assert_restored(&lower);
            }
        }
    }
}

#[test]
fn borrowed_cold_warm_edited_validation_and_real_work_refusal() {
    let _trace = native_source::trace();
    let mut ordinary = TestDb::default();
    let original = graph(&mut ordinary, false);
    assert_eq!(*source(&ordinary, original[0]), 2);
    let mut db = TestDb::default();
    let nodes = graph(&mut db, false);
    LIMIT.with(|limit| limit.set(3));
    clear_trace();
    assert_eq!(try_with_attempt(&db, 100_000, || *source(&db, nodes[0])), Ok(AttemptOutcome::Complete(2)));
    let before = events().iter().filter(|event| matches!(event, Event::Registered(_, _))).count();
    for _ in 0..2 {
        assert_eq!(try_with_attempt(&db, 100_000, || *source(&db, nodes[0])), Ok(AttemptOutcome::Complete(2)));
    }
    assert_eq!(events().iter().filter(|event| matches!(event, Event::Registered(_, _))).count(), before);
    nodes[1].set_leaf(&mut db).to(2);
    clear_trace();
    assert_eq!(try_with_attempt(&db, 100_000, || *source(&db, nodes[0])), Ok(AttemptOutcome::Complete(3)));
    assert!(events().iter().any(|event| matches!(event, Event::NativeGenerated(_))));
    let edited_boundary = 100_000 - events().iter().find_map(|event| match event {
        Event::Registered(_, remaining) => *remaining, _ => None,
    }).expect("edited native body reaches its original work boundary");
    assert!(native_source::take_trace().iter().any(|event| matches!(event, NativeTrace::Enter { .. })));
    let unrelated = Node::new(&db, None, 99);
    unrelated.set_leaf(&mut db).to(100);
    clear_trace();
    assert_eq!(try_with_attempt(&db, 100_000, || *source(&db, nodes[0])), Ok(AttemptOutcome::Complete(3)));
    assert!(!events().iter().any(|event| matches!(event, Event::Source(_) | Event::NativeGenerated(_) | Event::Registered(_, _))));
    assert_restored(&db);

    let mut edited_retry = TestDb::default();
    let edited_nodes = graph(&mut edited_retry, false);
    assert_eq!(try_with_attempt(&edited_retry, 100_000, || *source(&edited_retry, edited_nodes[0])), Ok(AttemptOutcome::Complete(2)));
    edited_nodes[1].set_leaf(&mut edited_retry).to(2);
    clear_trace();
    assert_eq!(try_with_attempt(&edited_retry, edited_boundary, || *source(&edited_retry, edited_nodes[0])),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)));
    assert!(events().iter().any(|event| matches!(event, Event::NativeGenerated(_))));
    for _ in 0..2 {
        assert_eq!(try_with_attempt(&edited_retry, 100_000, || *source(&edited_retry, edited_nodes[0])), Ok(AttemptOutcome::Complete(3)));
    }

    // Measure the second registered body's real debit boundary on an independent cold run.
    let mut measured = TestDb::default();
    let measured_nodes = graph(&mut measured, false);
    clear_trace();
    assert_eq!(try_with_attempt(&measured, 100_000, || *source(&measured, measured_nodes[0])), Ok(AttemptOutcome::Complete(2)));
    let remaining = events().iter().filter_map(|event| match event {
        Event::Registered(_, remaining) => *remaining, _ => None,
    }).nth(1).expect("nested registered body is cold");
    let boundary = 100_000 - remaining;
    for prespend in [0, 7] {
        let mut retry = TestDb::default();
        let retry_nodes = graph(&mut retry, false);
        assert_eq!(try_with_attempt(&retry, boundary + prespend, || {
            attempt_probe::charge(&retry, prespend).expect("measured prespend fits");
            *source(&retry, retry_nodes[0])
        }), Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)));
        assert_restored(&retry);
        for _ in 0..2 {
            assert_eq!(try_with_attempt(&retry, 100_000, || *source(&retry, retry_nodes[0])), Ok(AttemptOutcome::Complete(2)));
        }
    }
}

#[test]
fn registered_validation_dispatches_canonical_native_source_validation() {
    let _trace = native_source::trace();
    let mut db = TestDb::default();
    let nodes = graph(&mut db, false);
    LIMIT.with(|limit| limit.set(3));
    // Warm only the terminal source; the generated parent remains independently cold.
    assert_eq!(try_with_attempt(&db, 100_000, || *source(&db, nodes[1])), Ok(AttemptOutcome::Complete(1)));
    assert_eq!(try_with_attempt(&db, 100_000, || registered_graph_root(&db, nodes[0],
        source::fn_ingredient_(&db, db.zalsa()), generated::fn_ingredient_(&db, db.zalsa()))),
        Ok(AttemptOutcome::Complete(Ok(2))));
    nodes[1].set_leaf(&mut db).to(2);
    clear_trace();
    assert_eq!(try_with_attempt(&db, 100_000, || registered_graph_root(&db, nodes[0],
        source::fn_ingredient_(&db, db.zalsa()), generated::fn_ingredient_(&db, db.zalsa()))),
        Ok(AttemptOutcome::Complete(Ok(3))));
    let source_key = source::fn_ingredient_(&db, db.zalsa()).database_key_index(nodes[1].as_id());
    assert!(native_source::take_trace().iter().any(|event| matches!(event,
        NativeTrace::Validation { key, .. } if *key == source_key)));
    assert_restored(&db);
}

#[test]
fn completed_independent_native_child_survives_a_later_refusal() {
    let _trace = native_source::trace();
    LIMIT.with(|limit| limit.set(3));
    let mut measured = TestDb::default();
    let nodes = graph(&mut measured, false);
    let independent = Node::new(&measured, None, 9);
    clear_trace();
    assert_eq!(try_with_attempt(&measured, 100_000, || {
        assert_eq!(*source(&measured, independent), 9);
        *source(&measured, nodes[0])
    }), Ok(AttemptOutcome::Complete(2)));
    let remaining = events().iter().find_map(|event| match event {
        Event::Registered(id, remaining) if *id == nodes[1].as_id() => *remaining,
        _ => None,
    }).expect("the failing body's real work position was measured");
    let mut db = TestDb::default();
    let nodes = graph(&mut db, false);
    let independent = Node::new(&db, None, 9);
    let mut address = None;
    assert_eq!(try_with_attempt(&db, 100_000 - remaining, || {
        let value = source(&db, independent);
        assert_eq!(*value, 9);
        address = Some(std::ptr::from_ref(value).addr());
        *source(&db, nodes[0])
    }), Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)));
    clear_trace();
    assert_eq!(try_with_attempt(&db, 100_000, || {
        let value = source(&db, independent);
        assert_eq!(Some(std::ptr::from_ref(value).addr()), address);
        assert_eq!(*value, 9);
        *source(&db, nodes[0])
    }), Ok(AttemptOutcome::Complete(2)));
    assert!(!events().iter().any(|event| matches!(event,
        Event::Source(id) | Event::NativeGenerated(id) | Event::Registered(id, _) if *id == independent.as_id())));
    assert_restored(&db);
}

#[test]
fn genuine_refusal_before_native_consumption_and_after_source_return() {
    let _trace = native_source::trace();
    LIMIT.with(|limit| limit.set(3));
    for after_return in [false, true] {
        let db = TestDb::default();
        let node = Node::new(&db, None, 1);
        let delivered = Cell::new(false);
        clear_trace();
        let result = try_with_attempt(&db, if after_return { 100_000 } else { 0 }, || {
            let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
            let route = registry.register_native_source(&db as &dyn Database, source::fn_ingredient_(&db, db.zalsa()))?;
            let db_ref = &db;
            let delivered = &delivered;
            registry.seal()?.run(|endpoint| async move {
                let value = *endpoint.read_native_source(&route, node.as_id()).await;
                delivered.set(true);
                endpoint.local_call(|| {
                    endpoint.admit_work(attempt_probe::remaining_allowance_for_diagnostics(db_ref).expect("source returned in a live attempt") + 1)
                }).await;
                Ok(value)
            })
        });
        assert_eq!(result, Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)));
        assert_eq!(delivered.get(), after_return);
        assert_eq!(events().iter().any(|event| matches!(event, Event::Source(_))), after_return);
        assert_restored(&db);
        for _ in 0..2 {
            assert_eq!(try_with_attempt(&db, 100_000, || {
                let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
                let route = registry.register_native_source(&db as &dyn Database, source::fn_ingredient_(&db, db.zalsa()))?;
                registry.seal()?.run(|endpoint| async move { Ok(*endpoint.read_native_source(&route, node.as_id()).await) })
            }), Ok(AttemptOutcome::Complete(Ok(1))));
        }
        assert_restored(&db);
    }
    refusal_delivery_cases();
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeliveryCase {
    AlreadyRefused,
    SetupDebit,
    DriverStartup,
    Terminal,
    LateCompletion,
    Retirement,
    ContractRefused,
    RequiresFetchRefused,
    ContractOnly,
}

thread_local! {
    static DELIVERY_CASE: Cell<Option<DeliveryCase>> = const { Cell::new(None) };
}

struct ResetDelivery;
impl Drop for ResetDelivery { fn drop(&mut self) { DELIVERY_CASE.with(|case| case.set(None)); } }

struct RefuseOnRetirement<'db> { db: &'db dyn Database }
impl Drop for RefuseOnRetirement<'_> {
    fn drop(&mut self) { attempt_probe::report_incomplete(self.db, Incomplete::Allowance); }
}

struct DeliveryValue<'db, 'call> {
    db: &'db dyn Database,
    value: u32,
    reject: bool,
    dropped: &'call Cell<usize>,
}

impl Drop for DeliveryValue<'_, '_> {
    fn drop(&mut self) {
        self.dropped.set(self.dropped.get() + 1);
        if self.reject {
            attempt_probe::try_with_operation(self.db, || {
                assert_eq!(attempt_probe::report_incomplete(self.db, Incomplete::Interrupted), Incomplete::Allowance);
            }).expect("rejected value can retire a nested lexical operation");
            #[cfg(not(feature = "shuttle"))]
            {
                let mut event = transfer_test_support::Event::new(Kind::MarkerRead);
                event.phase = Some("rejected-native-value-dropped");
                transfer_test_support::record(event);
            }
        }
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn delivery_entry(db: &dyn Database, node: Node) -> RunResult<u32> {
    let case = DELIVERY_CASE.with(Cell::get);
    let called = Cell::new(false);
    let dropped = Cell::new(0);
    let result = with_native_callback(db, limits(), |entry| {
        called.set(true);
        match case {
            Some(DeliveryCase::SetupDebit) => {
                let remaining = attempt_probe::remaining_allowance_for_diagnostics(db).expect("live setup allowance");
                attempt_probe::charge(db, remaining + 1).map_err(RunError::Refused)?;
            }
            Some(DeliveryCase::DriverStartup | DeliveryCase::Terminal) => {
                RegistryBuilder::for_native_callback(db, &entry, &ADMISSION)?.seal()?.run(|endpoint| async move {
                    endpoint.local_call(|| {
                        let remaining = attempt_probe::remaining_allowance_for_diagnostics(db).expect("live driver allowance");
                        endpoint.admit_work(remaining + 1)
                    }).await;
                    Ok(())
                })?;
            }
            Some(DeliveryCase::ContractRefused | DeliveryCase::RequiresFetchRefused | DeliveryCase::LateCompletion) => {
                attempt_probe::report_incomplete(db, Incomplete::Allowance);
            }
            _ => {}
        }
        if matches!(case, Some(DeliveryCase::ContractRefused | DeliveryCase::ContractOnly)) {
            return Err(RunError::Contract("original native callback failure"));
        }
        if case == Some(DeliveryCase::RequiresFetchRefused) { return Err(RunError::RequiresFetch); }
        let _retirement = (case == Some(DeliveryCase::Retirement)).then(|| RefuseOnRetirement { db });
        Ok(DeliveryValue {
            db,
            value: node.leaf(db),
            reject: matches!(case, Some(DeliveryCase::LateCompletion | DeliveryCase::Retirement)),
            dropped: &dropped,
        })
    });
    assert_eq!(called.get(), case != Some(DeliveryCase::AlreadyRefused));
    let result = result.map(|value| value.value);
    assert_eq!(dropped.get(), usize::from(case.is_none() || matches!(case, Some(DeliveryCase::LateCompletion | DeliveryCase::Retirement))));
    if let Some(case) = case {
        let expected = match case {
            DeliveryCase::ContractRefused | DeliveryCase::ContractOnly => RunError::Contract("original native callback failure"),
            DeliveryCase::RequiresFetchRefused => RunError::RequiresFetch,
            _ => RunError::Refused(Incomplete::Allowance),
        };
        assert_eq!(result, Err(expected));
        println!("SOURCE_NATIVE_DELIVERY_RESULT\tcase={case:?}\tresult={result:?}\tbody={}\tdrops={}", called.get(), dropped.get());
    }
    result
}

fn refusal_delivery_cases() {
    let _reset = ResetDelivery;
    let retained_before = RETAINED_DATABASES.with(Cell::get);
    for case in [DeliveryCase::AlreadyRefused, DeliveryCase::SetupDebit, DeliveryCase::DriverStartup,
        DeliveryCase::Terminal, DeliveryCase::LateCompletion, DeliveryCase::Retirement,
        DeliveryCase::ContractRefused, DeliveryCase::RequiresFetchRefused, DeliveryCase::ContractOnly]
    {
        let db = TestDb::default();
        let node = Node::new(&db, None, 7);
        let key = delivery_entry::fn_ingredient_(&db, db.zalsa()).database_key_index(node.as_id());
        DELIVERY_CASE.with(|slot| slot.set(Some(case)));
        clear_trace();
        let call = || crate::prepared_source_probe::capture(&db, || {
            try_with_attempt(&db, if case == DeliveryCase::DriverStartup { 0 } else { 100_000 }, || {
                if case == DeliveryCase::AlreadyRefused {
                    assert!(db.zalsa_local().active_query().is_none());
                    attempt_probe::report_incomplete(&db, Incomplete::Allowance);
                }
                delivery_entry(&db, node)
            })
        }).expect("delivery evidence starts outside queries");
        #[cfg(not(feature = "shuttle"))]
        let (captured, support) = collect_support(call);
        #[cfg(feature = "shuttle")]
        let captured = call();
        let root = captured.reads.iter().find(|read| read.parent.is_none() && read.key == key)
            .expect("the delivery fixture returned its canonical memo");
        if case == DeliveryCase::ContractOnly {
            assert_eq!(captured.value, Ok(AttemptOutcome::Complete(Err(RunError::Contract("original native callback failure")))));
            assert_eq!(root.status, crate::prepared_source_probe::Status::Final);
            #[cfg(not(feature = "shuttle"))]
            assert!(support.records.iter().any(|record| record.event.kind == Kind::RootPublished
                && record.event.key == Some(key) && record.event.memo.is_some_and(|memo| memo.support.is_none())));
        } else {
            assert_eq!(captured.value, Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)));
            assert_eq!(root.status, crate::prepared_source_probe::Status::Incomplete);
            #[cfg(not(feature = "shuttle"))]
            {
                incomplete_publication(&support, key, root.memo_address);
                if matches!(case, DeliveryCase::LateCompletion | DeliveryCase::Retirement) {
                    let dropped = support.records.iter().find(|record|
                        record.event.phase == Some("rejected-native-value-dropped")).expect("rejected value actually dropped").ordinal;
                    assert!(support.records.iter().any(|record| record.ordinal > dropped
                        && record.event.kind == Kind::SupportAccepted && record.event.key == Some(key)
                        && record.event.session.is_some_and(|session| session.scope.is_none())));
                }
            }
        }
        println!("SOURCE_NATIVE_DELIVERY_SUPPORT\tcase={case:?}\tstatus={:?}\treads={:?}", root.status, captured.reads);
        #[cfg(not(feature = "shuttle"))]
        println!("SOURCE_NATIVE_DELIVERY_TRACE\tcase={case:?}\tbroken=false\ttransfer={support:?}");
        assert_restored(&db);
        DELIVERY_CASE.with(|slot| slot.set(None));
        if case != DeliveryCase::ContractOnly {
            for _ in 0..2 {
                assert_eq!(try_with_attempt(&db, 100_000, || delivery_entry(&db, node)), Ok(AttemptOutcome::Complete(Ok(7))));
                assert_restored(&db);
            }
        }
    }
    assert_eq!(RETAINED_DATABASES.with(Cell::get), retained_before);
    println!("SOURCE_DELIVERY_DATABASES\tstack=9\tretained_delta=0");
}

#[crate::input]
struct ChainLimit { #[returns(copy)] end: Option<u32> }
impl crate::function::FixedQueryFields for ChainLimit {}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn changing_key(db: &dyn Database, limit: ChainLimit, index: u32) -> u32 {
    if limit.end(db) == Some(index) { index } else { changing_key(db, limit, index + 1) }
}

struct ChainProvider<'run, 'db: 'run, C: InternedQueryConfiguration> {
    route: CallableRoute<'run, 'db, C>,
    keys: FixedQueryKeys<'db, C>,
}

impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for ChainProvider<'run, 'db, C>
where
    C: InternedQueryConfiguration
        + for<'a> crate::interned::Configuration<Fields<'a> = (ChainLimit, u32)>
        + for<'a> Configuration<DbView = dyn Database, Output<'a> = u32>,
{
    // The interned input clones a ChainLimit handle and u32; output equality compares u32.
    fixture_native_value!(callable, 'run, 'db, C, 2);

    async fn body<'call>(&'call self, endpoint: TaskEndpoint<'run, 'db>, db: &'db dyn Database,
        (limit, index): (ChainLimit, u32)) -> RunResult<u32> where 'run: 'call
    {
        endpoint.local_call(|| endpoint.admit_work(1)).await;
        if endpoint.local_call(|| Ok(limit.end(db))).await == Some(index) { return Ok(index); }
        let id = endpoint.intern_query_key(&self.keys, (limit, index + 1)).await;
        Ok(*endpoint.child_call(|| async { endpoint.fetch_ref(&self.route, id)?.await }).await)
    }
    fn initial<'call>(&'call self, _endpoint: TaskEndpoint<'run, 'db>, db: &'db dyn Database,
        id: Id, input: (ChainLimit, u32)) -> impl Future<Output = RunResult<u32>> + 'call where 'run: 'call
    { ready(Ok(C::cycle_initial(db, id, input))) }
    fn recover<'call>(&'call self, _endpoint: TaskEndpoint<'run, 'db>, db: &'db dyn Database,
        cycle: &'call Cycle<'call>, old: &'call u32, value: u32, input: (ChainLimit, u32))
        -> impl Future<Output = RunResult<u32>> + 'call where 'run: 'call
    { ready(Ok(C::recover_from_cycle(db, cycle, old, value, input))) }
}

fn chain_run<'db, C>(db: &'db dyn Database, ingredient: &'db IngredientImpl<C>, limit: ChainLimit) -> RunResult<u32>
where C: InternedQueryConfiguration
    + for<'a> crate::interned::Configuration<Fields<'a> = (ChainLimit, u32)>
    + for<'a> Configuration<DbView = dyn Database, Output<'a> = u32>,
{
    let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
    let route = registry.reserve_callable(db, ingredient)?;
    let keys = registry.fixed_callable_query_keys(&route)?;
    // Root interning uses a second checked token; the provider owns its own capability.
    let root_keys = registry.fixed_callable_query_keys(&route)?;
    registry.bind_callable(&route, ChainProvider { route: route.clone(), keys })?;
    registry.seal()?.run(move |endpoint| async move {
        let id = endpoint.intern_query_key(&root_keys, (limit, 0)).await;
        Ok(*endpoint.fetch_ref(&route, id)?.await?)
    })
}

#[test]
fn changing_generated_keys_share_one_allowance_with_flat_driver_depth() {
    let _trace = native_source::trace();
    for end in [Some(24), None] {
        if let Some(end) = end {
            let ordinary = TestDb::default();
            let limit = ChainLimit::new(&ordinary, Some(end));
            assert_eq!(changing_key(&ordinary, limit, 0), end);
        }
        let db = TestDb::default();
        let limit = ChainLimit::new(&db, end);
        clear_trace();
        let outcome = try_with_attempt(&db, 10_000, || chain_run(&db,
            changing_key::fn_ingredient_(&db, db.zalsa()), limit));
        match end {
            Some(end) => assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(end)))),
            None => assert_eq!(outcome, Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))),
        }
        let trace = native_source::take_trace();
        assert_eq!(trace.iter().filter(|event| matches!(event, NativeTrace::Enter { .. })).count(), 1);
        assert!(!trace.iter().any(|event| matches!(event, NativeTrace::SourceEnter { .. })));
        assert_restored(&db);
    }
}

// The captured native-callback matrices below use an explicitly bounded, static test hook.
// All cold, edit/retry, and lexical entry tests above use stack-owned databases.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HookStage { Before, Callout, After }

impl HookStage {
    const ALL: [Self; 3] = [Self::Before, Self::Callout, Self::After];
    fn index(self) -> usize {
        match self { Self::Before => 0, Self::Callout => 1, Self::After => 2 }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProbeKind { Local, RunningChild, Checkpoint, Terminal, Receipt, NativeSource, FinalSource, QueryKey, InternedValue }

impl ProbeKind {
    const ALL: [Self; 9] = [Self::Local, Self::RunningChild, Self::Checkpoint, Self::Terminal,
        Self::Receipt, Self::NativeSource, Self::FinalSource, Self::QueryKey, Self::InternedValue];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProbeRecord {
    Constructed(HookStage),
    Primed(HookStage, ProbeKind),
    Attempted(HookStage, ProbeKind),
    Consumed(HookStage),
    Retired(HookStage, bool),
}

type ProbeJournal = Rc<RefCell<Vec<ProbeRecord>>>;

struct ProbeSet {
    stage: HookStage,
    probes: Vec<(ProbeKind, HeldProbe)>,
    running_polls: Rc<Cell<usize>>,
    journal: ProbeJournal,
    consumed: bool,
}

impl ProbeSet {
    fn poll_paused(&mut self) {
        assert!(!self.consumed);
        assert_eq!(self.probes.iter().map(|(kind, _)| *kind).collect::<Vec<_>>(), ProbeKind::ALL);
        for (kind, probe) in &mut self.probes {
            self.journal.borrow_mut().push(ProbeRecord::Attempted(self.stage, *kind));
            let result = poll_once(probe.as_mut());
            match kind {
                ProbeKind::Checkpoint | ProbeKind::Terminal => assert!(matches!(result, Poll::Ready(Err(RunError::Contract(_))))),
                ProbeKind::Receipt => assert!(matches!(result, Poll::Ready(Ok(())))),
                _ => assert!(result.is_pending(), "paused {kind:?} unexpectedly completed: {result:?}"),
            }
        }
        assert_eq!(self.running_polls.get(), 1);
        self.consumed = true;
        self.journal.borrow_mut().push(ProbeRecord::Consumed(self.stage));
    }
}

impl Drop for ProbeSet {
    fn drop(&mut self) {
        self.probes.clear();
        self.journal.borrow_mut().push(ProbeRecord::Retired(self.stage, self.consumed));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault { None, Refuse, Panic }
#[derive(Debug)]
struct NativePayload(Arc<()>);

type HeldProbe = Pin<Box<dyn Future<Output = RunResult<()>>>>;
struct Hook {
    db: &'static TestDb,
    node: Node,
    parent: TaskEndpoint<'static, 'static>,
    stages: Rc<RefCell<Vec<HookStage>>>,
    probes: RefCell<[Option<ProbeSet>; 3]>,
    drops: Rc<Cell<usize>>,
    child_ran: Rc<Cell<bool>>,
    fault: Fault,
    fault_fired: Cell<bool>,
    payload: Arc<()>,
    busy: Cell<bool>,
    race: bool,
    armed: Cell<bool>,
    action: RefCell<Option<Box<dyn FnOnce()>>>,
    routed: Box<dyn Fn()>,
}

thread_local! {
    static HOOK: RefCell<Option<Rc<Hook>>> = const { RefCell::new(None) };
    static RETAINED_DATABASES: Cell<usize> = const { Cell::new(0) };
}

struct ClearHook;
impl Drop for ClearHook {
    fn drop(&mut self) { HOOK.with_borrow_mut(|hook| *hook = None); }
}
struct ClearBusy<'a>(&'a Cell<bool>);
impl Drop for ClearBusy<'_> {
    fn drop(&mut self) { self.0.set(false); }
}

#[crate::interned]
struct Scalar<'db> { #[returns(copy)] value: u32 }

impl crate::interned::FiniteInternedConfiguration for Scalar<'static> {
    fn field_work(_fields: &Self::Fields<'_>) -> Option<usize> { Some(1) }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn final_leaf(db: &dyn Database, node: Node) -> u32 { node.leaf(db) }

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn bound_leaf(db: &dyn Database, node: Node) -> u32 { node.leaf(db) }

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn executable_leaf(db: &dyn Database, node: Node) -> u32 { node.leaf(db) }

struct LeafProvider;
static LEAF_PROVIDER: LeafProvider = LeafProvider;

impl<'run, 'db: 'run, C> RouteProvider<'run, 'db, C> for LeafProvider
where C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    async fn body(&'run self, _context: ProviderContext<'run, 'db, Self>, db: &'db dyn Database, node: Node) -> RunResult<u32> {
        Ok(node.leaf(db))
    }
    async fn verify(&'run self, _context: ProviderContext<'run, 'db, Self>, _db: &'db dyn Database,
        _id: Id, _revision: Revision) -> RunResult<VerifyResult>
    { panic!("paused proof callback ran") }
}

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for LeafProvider
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    // Node conversion constructs a handle; output equality compares u32.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(&'run self, _context: ProviderContext<'run, 'db, Self>, db: &'db dyn Database, node: Node) -> RunResult<u32> {
        Ok(node.leaf(db))
    }
    async fn initial(&'run self, _context: ProviderContext<'run, 'db, Self>, db: &'db dyn Database,
        id: Id, node: Node) -> RunResult<u32>
    { Ok(C::cycle_initial(db, id, node)) }
    async fn recover<'call>(&'run self, _context: ProviderContext<'run, 'db, Self>, db: &'db dyn Database,
        cycle: &'call Cycle<'call>, old: &'call u32, value: u32, node: Node) -> RunResult<u32> where 'run: 'call
    { Ok(C::recover_from_cycle(db, cycle, old, value, node)) }
}

async fn next_manual_poll() {
    let mut first = true;
    poll_fn(|_| if std::mem::replace(&mut first, false) { Poll::Pending } else { Poll::Ready(()) }).await;
}

fn retain_precreated(probes: &mut ProbeSet, kind: ProbeKind, mut probe: HeldProbe) {
    assert!(poll_once(probe.as_mut()).is_pending());
    probes.journal.borrow_mut().push(ProbeRecord::Primed(probes.stage, kind));
    probes.probes.push((kind, probe));
}

fn source_claim_live(db: &dyn Database, node: Node) -> bool {
    let ingredient = source::fn_ingredient_(db, db.zalsa());
    let key = ingredient.database_key_index(node.as_id());
    db.zalsa_local().try_with_query_stack(|stack| stack.iter().any(|query| query.database_key_index == key)) == Some(true)
        && matches!(ingredient.sync_table.peek_claim(db.zalsa(), node.as_id(), Reentrancy::Deny), ClaimResult::Cycle { .. })
}

struct SourceWitness {
    db: &'static TestDb,
    node: Node,
    drops: Rc<Cell<usize>>,
}
impl Drop for SourceWitness {
    fn drop(&mut self) {
        assert!(source_claim_live(self.db, self.node), "the actual native source frame and claim own this drop");
        self.drops.set(self.drops.get() + 1);
    }
}

fn poll_once<F: Future + ?Sized>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

fn make_precreated_probes(parent: &TaskEndpoint<'static, 'static>, stage: HookStage, journal: ProbeJournal) -> ProbeSet {
    let armed = Rc::new(Cell::new(false));
    let running = Rc::new(Cell::new(0));
    journal.borrow_mut().push(ProbeRecord::Constructed(stage));
    let mut probes = ProbeSet { stage, probes: Vec::new(), running_polls: running.clone(), journal, consumed: false };
    let endpoint = parent.clone();
    let flag = armed.clone();
    retain_precreated(&mut probes, ProbeKind::Local, Box::pin(async move {
        let future = endpoint.local_call(|| -> RunResult<()> { panic!("paused local factory ran") });
        poll_fn(|_| if flag.get() { Poll::Ready(()) } else { Poll::Pending }).await;
        future.await;
        Ok(())
    }));
    let endpoint = parent.clone();
    retain_precreated(&mut probes, ProbeKind::RunningChild, Box::pin(async move {
        endpoint.child_call(|| poll_fn(|_| {
            running.set(running.get() + 1);
            Poll::<RunResult<()>>::Pending
        })).await;
        Ok(())
    }));
    let endpoint = parent.clone();
    let flag = armed.clone();
    retain_precreated(&mut probes, ProbeKind::Checkpoint, Box::pin(async move {
        let checkpoint = endpoint.checkpoint()?;
        poll_fn(|_| if flag.get() { Poll::Ready(()) } else { Poll::Pending }).await;
        checkpoint.await
    }));
    let endpoint = parent.clone();
    let flag = armed.clone();
    retain_precreated(&mut probes, ProbeKind::Terminal, Box::pin(async move {
        let failure = endpoint.inner.suspend_error(RunError::Contract("captured terminal"))?;
        poll_fn(|_| if flag.get() { Poll::Ready(()) } else { Poll::Pending }).await;
        match failure.await { Err(error) => Err(error), Ok(never) => match never {} }
    }));
    let endpoint = parent.clone();
    let flag = armed.clone();
    retain_precreated(&mut probes, ProbeKind::Receipt, Box::pin(async move {
        let receipt = endpoint.inner.callback_poll().expect("the root poll is live");
        poll_fn(|_| if flag.get() { Poll::Ready(()) } else { Poll::Pending }).await;
        assert!(!receipt.error(RunError::Contract("paused receipt")));
        let returned = receipt.try_panic(Box::new(17_u32)).expect_err("paused receipt cannot take a payload");
        assert_eq!(returned.downcast_ref::<u32>(), Some(&17));
        Ok(())
    }));
    armed.set(true);
    probes
}

fn fire_hook(stage: HookStage) {
    let hook = HOOK.with_borrow(|hook| hook.clone());
    let Some(hook) = hook else { return; };
    if hook.busy.get() || !hook.parent.inner.queue.paused.get() || !source_claim_live(hook.db, hook.node) {
        return;
    }
    if stage == HookStage::Callout && native_source::current_driver() == Some(hook.parent.inner.queue.driver) {
        return;
    }
    // Keep one observation at each boundary; inner callouts may repeat many times.
    if hook.stages.borrow().contains(&stage) { return; }
    hook.busy.set(true);
    let _busy = ClearBusy(&hook.busy);
    hook.stages.borrow_mut().push(stage);
    let snapshot = super::super::progress::snapshot(&hook.parent.inner.queue);
    let pending = hook.parent.inner.queue.pending.borrow().len();
    let generation = hook.parent.inner.queue.last_poll_generation.get();
    let work = events().iter().filter(|event| matches!(event, Event::Work(_))).count();
    let native = events().iter().filter(|event| matches!(event, Event::Native(_))).count();
    let allowance = attempt_probe::remaining_allowance_for_diagnostics(hook.db);
    let dropped = hook.drops.get();
    let witness = SourceWitness { db: hook.db, node: hook.node, drops: hook.drops.clone() };
    assert!(matches!(hook.parent.demand(move || async move {
        let _witness = witness;
        panic!("paused demand factory ran");
        #[expect(unreachable_code)]
        Ok::<(), RunError>(())
    }), Err(RunError::Contract(_))));
    assert_eq!(hook.drops.get(), dropped + 1);
    assert!(matches!(hook.parent.admit(ExecutionWork::Work { units: 9 }), Err(RunError::Contract(_))));
    assert!(matches!(hook.parent.admit_work(9), Err(RunError::Contract(_))));
    assert!(matches!(hook.parent.checkpoint(), Err(RunError::Contract(_))));
    let mut local = Box::pin(hook.parent.local_call(|| -> RunResult<()> { panic!("new paused local ran") }));
    assert!(poll_once(local.as_mut()).is_pending());
    let mut child = Box::pin(hook.parent.child_call(|| async { panic!("new paused child ran"); #[expect(unreachable_code)] Ok::<(), RunError>(()) }));
    assert!(poll_once(child.as_mut()).is_pending());
    (hook.routed)();
    let mut probes = hook.probes.borrow_mut()[stage.index()].take().expect("this stage has its own pre-pause set");
    assert_eq!(probes.stage, stage);
    probes.poll_paused();
    // Completed probes are dropped here, with the native source frame still alive.
    drop(probes);
    assert_eq!(super::super::progress::snapshot(&hook.parent.inner.queue), snapshot);
    assert_eq!(hook.parent.inner.queue.pending.borrow().len(), pending);
    assert_eq!(hook.parent.inner.queue.last_poll_generation.get(), generation);
    assert_eq!(events().iter().filter(|event| matches!(event, Event::Work(_))).count(), work);
    assert_eq!(events().iter().filter(|event| matches!(event, Event::Native(_))).count(), native);
    assert_eq!(attempt_probe::remaining_allowance_for_diagnostics(hook.db), allowance);
}

async fn exercise_inner(endpoint: &TaskEndpoint<'_, '_>, db: &dyn Database) {
    let hook = HOOK.with_borrow(|hook| hook.clone());
    let Some(hook) = hook.filter(|hook| !hook.race && source_claim_live(db, hook.node)) else { return; };
    if hook.fault_fired.replace(true) { return; }
    match hook.fault {
        Fault::None => {
            let ran = hook.child_ran.clone();
            endpoint.child_call(|| async {
                endpoint.demand(move || async move { ran.set(true); Ok(()) })?.await
            }).await;
        }
        fault => {
            endpoint.local_call(|| -> RunResult<()> {
                let witness = SourceWitness { db: hook.db, node: hook.node, drops: hook.drops.clone() };
                let _child = endpoint.demand(move || async move { let _witness = witness; Ok(()) })?;
                match fault {
                    Fault::Refuse => endpoint.admit_work(attempt_probe::remaining_allowance_for_diagnostics(db).expect("live attempt") + 1),
                    Fault::Panic => panic_any(NativePayload(hook.payload.clone())),
                    Fault::None => Ok(()),
                }
            }).await;
        }
    }
}

fn admission_action() -> RunResult<()> {
    let hook = HOOK.with_borrow(|hook| hook.clone());
    let Some(hook) = hook.filter(|hook| hook.race && hook.armed.replace(false)) else { return Ok(()); };
    let action = hook.action.borrow_mut().take().expect("one admitted native source call");
    action();
    match hook.fault {
        Fault::None => Ok(()),
        Fault::Refuse => Err(RunError::Refused(Incomplete::Allowance)),
        Fault::Panic => panic_any(NativePayload(hook.payload.clone())),
    }
}

fn captured_matrix(fault: Fault, race: bool) {
    // Exactly one database per matrix case is intentionally retained; no endpoint/provider is.
    let db: &'static TestDb = Box::leak(Box::new(TestDb::default()));
    RETAINED_DATABASES.with(|count| count.set(count.get() + 1));
    let node = Node::new(db, None, 1);
    let chain_limit = ChainLimit::new(db, Some(1));
    assert_eq!(final_leaf(db, node), 1);
    let final_ingredient = final_leaf::fn_ingredient_(db, db.zalsa());
    let certificate = FinalSourceMemo::certify(db as &dyn Database, final_ingredient, node.as_id()).expect("actual completed source memo");
    LIMIT.with(|limit| limit.set(3));
    let queue = Rc::new(RefCell::new(None));
    let queue_probe = queue.clone();
    let weak_hook = Rc::new(RefCell::new(None));
    let hook_probe = weak_hook.clone();
    let identity = Arc::new(());
    let payload = identity.clone();
    let stages = Rc::new(RefCell::new(Vec::new()));
    let stages_out = stages.clone();
    let dropped = Rc::new(Cell::new(0));
    let drops_out = dropped.clone();
    let probe_journal: ProbeJournal = Rc::new(RefCell::new(Vec::new()));
    let probes_out = probe_journal.clone();
    let _clear = ClearHook;
    let outcome = catch_unwind(AssertUnwindSafe(|| try_with_attempt(db, 100_000, || {
        let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
        let route = registry.register_native_source(db as &dyn Database, source::fn_ingredient_(db, db.zalsa()))?;
        let final_route = registry.register_final_source(db as &dyn Database, final_ingredient, &[certificate])?;
        let values = Rc::new(registry.finite_interned_values_with_memos(Scalar::ingredient(db.zalsa()), ())?);
        let bound = registry.reserve(db as &dyn Database, bound_leaf::fn_ingredient_(db, db.zalsa()))?;
        let executable = registry.reserve(db as &dyn Database, executable_leaf::fn_ingredient_(db, db.zalsa()))?;
        let binding = registry.provider(&LEAF_PROVIDER)?;
        registry.bind(&bound, &binding)?;
        registry.bind_executable(&executable, &binding)?;
        let generated = registry.reserve_callable(db as &dyn Database, generated::fn_ingredient_(db, db.zalsa()))?;
        registry.bind_callable(&generated, GraphProvider { source: route.clone() })?;
        let chain = registry.reserve_callable(db as &dyn Database, changing_key::fn_ingredient_(db, db.zalsa()))?;
        let chain_keys = registry.fixed_callable_query_keys(&chain)?;
        let root_keys = Rc::new(registry.fixed_callable_query_keys(&chain)?);
        registry.bind_callable(&chain, ChainProvider { route: chain.clone(), keys: chain_keys })?;
        registry.seal()?.run(move |endpoint| async move {
            *queue_probe.borrow_mut() = Some(Rc::downgrade(&endpoint.inner.queue));
            let mut probe_sets = [None, None, None];
            for stage in HookStage::ALL {
            let mut probes = make_precreated_probes(&endpoint, stage, probes_out.clone());
            let captured = endpoint.clone();
            let captured_route = route.clone();
            retain_precreated(&mut probes, ProbeKind::NativeSource, Box::pin(async move {
                let future = captured.read_native_source(&captured_route, node.as_id());
                next_manual_poll().await;
                let _ = future.await;
                Ok(())
            }));
            let captured = endpoint.clone();
            let captured_route = final_route.clone();
            retain_precreated(&mut probes, ProbeKind::FinalSource, Box::pin(async move {
                let future = captured.read_final_source(&captured_route, node.as_id());
                next_manual_poll().await;
                let _ = future.await;
                Ok(())
            }));
            let captured = endpoint.clone();
            let captured_keys = root_keys.clone();
            retain_precreated(&mut probes, ProbeKind::QueryKey, Box::pin(async move {
                let future = captured.intern_query_key(&captured_keys, (chain_limit, 0));
                next_manual_poll().await;
                let _ = future.await;
                Ok(())
            }));
            let captured = endpoint.clone();
            let captured_values = values.clone();
            retain_precreated(&mut probes, ProbeKind::InternedValue, Box::pin(async move {
                let future = captured.intern_value(&captured_values, (1,));
                next_manual_poll().await;
                let _ = future.await;
                Ok(())
            }));
            probe_sets[stage.index()] = Some(probes);
            }
            let routed_endpoint = endpoint.clone();
            let routed_source = route.clone();
            let provider = endpoint.provider(binding)?;
            let routed = Box::new(move || {
                assert!(matches!(routed_endpoint.fetch_ref(&generated, node.as_id()), Err(RunError::Contract(_))));
                assert!(matches!(routed_endpoint.validate_callable(&generated, node.as_id(), db.zalsa().current_revision()), Err(RunError::Contract(_))));
                let source_key = source::fn_ingredient_(db, db.zalsa()).database_key_index(node.as_id());
                assert!(matches!(routed_endpoint.validate(source_key, db.zalsa().current_revision()), Err(RunError::Contract(_))));
                assert!(matches!(provider.body_callback(&bound, node), Err(RunError::Contract(_))));
                assert!(matches!(routed_endpoint.verify_callback(bound.database_key(node.as_id()), db.zalsa().current_revision()), Err(RunError::Contract(_))));
                assert!(matches!(provider.fetch_ref(&executable, node.as_id()), Err(RunError::Contract(_))));
                assert!(matches!(provider.validate(&executable, node.as_id(), db.zalsa().current_revision()), Err(RunError::Contract(_))));
                assert!(matches!(routed_endpoint.validate(final_ingredient.database_key_index(node.as_id()), db.zalsa().current_revision()), Err(RunError::Contract(_))));
                let mut read = Box::pin(routed_endpoint.read_native_source(&routed_source, node.as_id()));
                assert!(poll_once(read.as_mut()).is_pending());
                let mut final_read = Box::pin(routed_endpoint.read_final_source(&final_route, node.as_id()));
                assert!(poll_once(final_read.as_mut()).is_pending());
                let mut key = Box::pin(routed_endpoint.intern_query_key(&root_keys, (chain_limit, 0)));
                assert!(poll_once(key.as_mut()).is_pending());
                let mut value = Box::pin(routed_endpoint.intern_value(&values, (1,)));
                assert!(poll_once(value.as_mut()).is_pending());
            });
            let hook = Rc::new(Hook {
                db, node, parent: endpoint.clone(), stages: stages_out.clone(), probes: RefCell::new(probe_sets),
                drops: drops_out, child_ran: Rc::new(Cell::new(false)), fault,
                fault_fired: Cell::new(false), payload, busy: Cell::new(false), race, armed: Cell::new(false),
                action: RefCell::new(None), routed,
            });
            *hook_probe.borrow_mut() = Some(Rc::downgrade(&hook));
            HOOK.with_borrow_mut(|slot| *slot = Some(hook.clone()));
            let _clear = ClearHook;
            let result = if race {
                let captured_endpoint = endpoint.clone();
                let captured_route = route.clone();
                *hook.action.borrow_mut() = Some(Box::new(move || {
                    let mut read = Box::pin(captured_endpoint.read_native_source(&captured_route, node.as_id()));
                    assert!(matches!(poll_once(read.as_mut()), Poll::Ready(&1)));
                }));
                hook.armed.set(true);
                // The observer fully pauses and restores this queue before demand may allocate.
                match endpoint.demand(|| async { panic!("stale admission factory ran"); #[expect(unreachable_code)] Ok::<(), RunError>(()) }) {
                    Err(error) => Err(error),
                    Ok(_) => panic!("admission survived its native source pause"),
                }
            } else {
                let value = *endpoint.read_native_source(&route, node.as_id()).await;
                assert_eq!(value, 1);
                assert!(hook.child_ran.get());
                endpoint.child_call(|| async { endpoint.demand(|| ready(Ok(())))?.await }).await;
                Ok(())
            };
            result
        })
    })));
    HOOK.with_borrow_mut(|hook| {
        *hook = None;
    });
    match (fault, race) {
        (Fault::None, false) => assert!(matches!(outcome, Ok(Ok(AttemptOutcome::Complete(Ok(())))))),
        (Fault::None, true) => assert!(matches!(outcome, Ok(Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))))),
        (Fault::Refuse, _) => assert!(matches!(outcome, Ok(Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))))),
        (Fault::Panic, _) => {
            let payload = outcome.expect_err("native panic survives every driver and source guard");
            assert!(payload.downcast_ref::<NativePayload>().is_some_and(|payload| Arc::ptr_eq(&payload.0, &identity)));
        }
    }
    assert!(queue.borrow().as_ref().is_some_and(|queue| queue.upgrade().is_none()));
    assert!(weak_hook.borrow().as_ref().is_some_and(|hook| hook.upgrade().is_none()));
    assert!(HOOK.with_borrow(Option::is_none));
    assert!(dropped.get() >= 1);
    let expected_stages = if fault == Fault::Panic && !race {
        vec![HookStage::Before, HookStage::Callout]
    } else { HookStage::ALL.to_vec() };
    assert_eq!(*stages.borrow(), expected_stages);
    let journal = probe_journal.borrow();
    let first_attempt = journal.iter().position(|record| matches!(record, ProbeRecord::Attempted(_, _))).expect("a paused set ran");
    assert_eq!(journal[..first_attempt].iter().filter(|record| matches!(record, ProbeRecord::Primed(_, _))).count(), 27);
    for stage in HookStage::ALL {
        assert_eq!(journal.iter().filter(|record| **record == ProbeRecord::Constructed(stage)).count(), 1);
        let primed: Vec<_> = journal.iter().filter_map(|record| match record {
            ProbeRecord::Primed(target, kind) if *target == stage => Some(*kind), _ => None,
        }).collect();
        assert_eq!(primed, ProbeKind::ALL);
        let attempted: Vec<_> = journal.iter().filter_map(|record| match record {
            ProbeRecord::Attempted(target, kind) if *target == stage => Some(*kind), _ => None,
        }).collect();
        let consumed = expected_stages.contains(&stage);
        assert_eq!(attempted, if consumed { ProbeKind::ALL.to_vec() } else { Vec::new() });
        assert_eq!(journal.iter().filter(|record| **record == ProbeRecord::Consumed(stage)).count(), usize::from(consumed));
        let retired: Vec<_> = journal.iter().filter_map(|record| match record {
            ProbeRecord::Retired(target, consumed) if *target == stage => Some(*consumed), _ => None,
        }).collect();
        assert_eq!(retired, [consumed]);
    }
    let consumed: Vec<_> = journal.iter().filter_map(|record| match record {
        ProbeRecord::Consumed(stage) => Some(*stage), _ => None,
    }).collect();
    assert_eq!(consumed, expected_stages);
    let attempts = journal.iter().filter(|record| matches!(record, ProbeRecord::Attempted(_, _))).count();
    assert_eq!(attempts, expected_stages.len() * ProbeKind::ALL.len());
    assert_restored(db);
    println!("SOURCE_CAPTURED_PARENT\trace={race}\tfault={fault:?}\tdrops={}\tstages={:?}", dropped.get(), stages.borrow());
    println!("SOURCE_CAPTURED_PROBES\trace={race}\tfault={fault:?}\tconstructed=3\tprimed=27\tattempted={attempts}\tconsumed={consumed:?}\tretired=3\tjournal={journal:?}");
}

#[test]
fn captured_parent_is_paused_before_during_and_after_inner_driver() {
    let _trace = native_source::trace();
    let before = RETAINED_DATABASES.with(Cell::get);
    for fault in [Fault::None, Fault::Refuse, Fault::Panic] { captured_matrix(fault, false); }
    assert_eq!(RETAINED_DATABASES.with(Cell::get) - before, 3);
    println!("SOURCE_HARNESS_RETAINED_DATABASES\tmatrix=native\tcount=3");
}

#[test]
fn completed_source_pause_invalidates_an_inflight_admission() {
    let _trace = native_source::trace();
    let before = RETAINED_DATABASES.with(Cell::get);
    for fault in [Fault::None, Fault::Refuse, Fault::Panic] { captured_matrix(fault, true); }
    assert_eq!(RETAINED_DATABASES.with(Cell::get) - before, 3);
    println!("SOURCE_HARNESS_RETAINED_DATABASES\tmatrix=admission\tcount=3");
}

#[cfg(not(feature = "shuttle"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CancellationKind { Local, PendingWrite }

// Fixpoint configurations mask Local cancellation before their body starts, including acyclic
// inputs. These ordinary configurations exercise the same source entry at an enabled checkpoint.
#[cfg(not(feature = "shuttle"))]
#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn cancellation_source(db: &dyn Database, node: Node) -> u32 {
    let _owner = cancellation_source_witness(db, node);
    if attempt_probe::current().is_some() {
        complete_or_refused(request_generated(db, node,
            cancellation_source::fn_ingredient_(db, db.zalsa()),
            cancellation_generated::fn_ingredient_(db, db.zalsa())).copied())
    } else { *cancellation_generated(db, node) }
}

#[cfg(not(feature = "shuttle"))]
#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn cancellation_generated(db: &dyn Database, node: Node) -> u32 {
    combine(node.next(db).map(|next| *cancellation_source(db, next)), node.leaf(db))
}

#[cfg(not(feature = "shuttle"))]
fn cancellation_source_claim_live(db: &dyn Database, node: Node) -> bool {
    let ingredient = cancellation_source::fn_ingredient_(db, db.zalsa());
    let key = ingredient.database_key_index(node.as_id());
    db.zalsa_local().try_with_query_stack(|stack| stack.iter().any(|query| query.database_key_index == key)) == Some(true)
        && matches!(ingredient.sync_table.peek_claim(db.zalsa(), node.as_id(), Reentrancy::Deny), ClaimResult::Cycle { .. })
}

#[cfg(not(feature = "shuttle"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CancellationStage { Child, Source, Parent }

#[cfg(not(feature = "shuttle"))]
#[derive(Debug)]
struct CancellationSnapshot {
    stage: CancellationStage,
    source_live: bool,
    driver: Option<native_source::DriverIdentity>,
    permit_clear: bool,
    panicking: bool,
    parent_paused: Option<bool>,
}

#[cfg(not(feature = "shuttle"))]
struct CancellationCase {
    kind: CancellationKind,
    database: usize,
    node: Node,
    parent: Cell<Option<native_source::DriverIdentity>>,
    parent_queue: Cell<usize>,
    fired: Cell<bool>,
    child_ran: Cell<bool>,
    journal: RefCell<Vec<CancellationSnapshot>>,
}

#[cfg(not(feature = "shuttle"))]
impl CancellationCase {
    fn observe(&self, db: &dyn Database, stage: CancellationStage, parent_paused: Option<bool>) {
        let snapshot = CancellationSnapshot {
            stage,
            source_live: cancellation_source_claim_live(db, self.node),
            driver: native_source::current_driver(),
            permit_clear: native_source::source_permit_is_clear(),
            panicking: std::thread::panicking(),
            parent_paused,
        };
        self.journal.borrow_mut().push(snapshot);
    }
}

#[cfg(not(feature = "shuttle"))]
thread_local! {
    // Only scalar fixture identity and observations cross native query callbacks.
    static CANCELLATION_CASE: RefCell<Option<Rc<CancellationCase>>> = const { RefCell::new(None) };
}

#[cfg(not(feature = "shuttle"))]
struct ResetCancellation<'db>(&'db dyn Database);

#[cfg(not(feature = "shuttle"))]
impl ResetCancellation<'_> {
    fn clear(&self) {
        self.0.zalsa_local().uncancel();
        self.0.zalsa().runtime().reset_cancellation_flag();
        CANCELLATION_CASE.with_borrow_mut(|case| *case = None);
    }
}

#[cfg(not(feature = "shuttle"))]
impl Drop for ResetCancellation<'_> {
    fn drop(&mut self) { self.clear(); }
}

#[cfg(not(feature = "shuttle"))]
struct CancellationSourceWitness<'db> {
    db: &'db dyn Database,
    case: Rc<CancellationCase>,
    stage: CancellationStage,
}

#[cfg(not(feature = "shuttle"))]
impl Drop for CancellationSourceWitness<'_> {
    fn drop(&mut self) { self.case.observe(self.db, self.stage, None); }
}

#[cfg(not(feature = "shuttle"))]
fn cancellation_source_witness(db: &dyn Database, node: Node) -> Option<CancellationSourceWitness<'_>> {
    let case = CANCELLATION_CASE.with_borrow(|case| case.clone())?;
    if case.node != node || case.database != std::ptr::from_ref(db.zalsa()).addr() { return None; }
    Some(CancellationSourceWitness { db, case, stage: CancellationStage::Source })
}

#[cfg(not(feature = "shuttle"))]
struct CancellationParentWitness<'run, 'db: 'run> {
    endpoint: TaskEndpoint<'run, 'db>,
    db: &'db dyn Database,
    case: Rc<CancellationCase>,
}

#[cfg(not(feature = "shuttle"))]
impl Drop for CancellationParentWitness<'_, '_> {
    fn drop(&mut self) {
        self.case.observe(self.db, CancellationStage::Parent, Some(self.endpoint.inner.queue.paused.get()));
    }
}

#[cfg(not(feature = "shuttle"))]
async fn exercise_cancellation<'run, 'db: 'run>(endpoint: &TaskEndpoint<'run, 'db>, db: &'db dyn Database, node: Node) {
    let case = CANCELLATION_CASE.with_borrow(|case| case.clone());
    let Some(case) = case.filter(|case| case.node == node && case.database == std::ptr::from_ref(db.zalsa()).addr()) else { return; };
    if case.fired.replace(true) { return; }
    assert!(cancellation_source_claim_live(db, node));
    assert!(!native_source::source_permit_is_clear());
    assert_ne!(native_source::current_driver(), case.parent.get());
    assert_eq!(native_source::current_driver().map(|driver| driver.depth()), Some(2));
    endpoint.local_call(|| {
        let witness = CancellationSourceWitness { db, case: case.clone(), stage: CancellationStage::Child };
        let child_case = case.clone();
        let _child = endpoint.demand(move || async move {
            let _witness = witness;
            child_case.child_ran.set(true);
            Ok(())
        })?;
        match case.kind {
            CancellationKind::Local => {
                db.cancellation_token().cancel();
                assert!(db.zalsa_local().should_trigger_local_cancellation(), "the acyclic fixture must not mask Local cancellation");
            }
            CancellationKind::PendingWrite => db.zalsa().runtime().set_cancellation_flag(),
        }
        // This invokes the real Salsa cancellation checkpoint with the queued child still owned.
        endpoint.check_completion()
    }).await;
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn nested_native_source_cancellation_keeps_payload_and_cleanup_order() {
    let _trace = native_source::trace();
    let retained_before = RETAINED_DATABASES.with(Cell::get);
    LIMIT.with(|limit| limit.set(3));
    for kind in [CancellationKind::Local, CancellationKind::PendingWrite] {
        let db = TestDb::default();
        let db_ref: &dyn Database = &db;
        let node = Node::new(&db, None, 1);
        let case = Rc::new(CancellationCase {
            kind, database: std::ptr::from_ref(db.zalsa()).addr(), node,
            parent: Cell::new(None), parent_queue: Cell::new(0), fired: Cell::new(false),
            child_ran: Cell::new(false), journal: RefCell::new(Vec::new()),
        });
        assert!(HOOK.with_borrow(Option::is_none));
        CANCELLATION_CASE.with_borrow_mut(|current| assert!(current.replace(case.clone()).is_none()));
        let reset = ResetCancellation(db_ref);
        let queue = RefCell::new(None);
        let delivered = Cell::new(false);
        clear_trace();
        let outcome = catch_unwind(AssertUnwindSafe(|| try_with_attempt(db_ref, 100_000, || {
            let mut registry = RegistryBuilder::new(db_ref, &ADMISSION)?;
            let route = registry.register_native_source(db_ref, cancellation_source::fn_ingredient_(db_ref, db_ref.zalsa()))?;
            let case = case.clone();
            let queue = &queue;
            let delivered = &delivered;
            registry.seal()?.run(move |endpoint| async move {
                case.parent.set(Some(endpoint.inner.queue.driver));
                case.parent_queue.set(Rc::as_ptr(&endpoint.inner.queue).addr());
                *queue.borrow_mut() = Some(Rc::downgrade(&endpoint.inner.queue));
                let _parent = CancellationParentWitness { endpoint: endpoint.clone(), db: db_ref, case };
                let value = *endpoint.read_native_source(&route, node.as_id()).await;
                delivered.set(true);
                Ok(value)
            })
        })));
        reset.clear();
        let payload = outcome.expect_err("actual Salsa cancellation must reach the outer caller");
        assert!(matches!((kind, payload.downcast_ref::<crate::Cancelled>()),
            (CancellationKind::Local, Some(crate::Cancelled::Local))
                | (CancellationKind::PendingWrite, Some(crate::Cancelled::PendingWrite))));
        assert!(case.fired.get());
        assert!(!case.child_ran.get());
        assert!(!delivered.get());
        assert!(queue.borrow().as_ref().is_some_and(|queue| queue.upgrade().is_none()));
        assert!(CANCELLATION_CASE.with_borrow(Option::is_none));
        assert!(HOOK.with_borrow(Option::is_none));
        assert_restored(db_ref);
        let source_key = cancellation_source::fn_ingredient_(db_ref, db_ref.zalsa()).database_key_index(node.as_id());
        assert!(matches!(cancellation_source::fn_ingredient_(db_ref, db_ref.zalsa()).sync_table.peek_claim(db_ref.zalsa(), node.as_id(), Reentrancy::Deny), ClaimResult::Claimed(())));
        assert!(matches!(cancellation_generated::fn_ingredient_(db_ref, db_ref.zalsa()).sync_table.peek_claim(db_ref.zalsa(), node.as_id(), Reentrancy::Deny), ClaimResult::Claimed(())));
        let trace = native_source::take_trace();
        let entered: Vec<_> = trace.iter().filter_map(|event| match event {
            NativeTrace::Enter { identity, parent, caller } => Some((*identity, *parent, *caller)), _ => None,
        }).collect();
        assert_eq!(entered.len(), 2);
        let parent = case.parent.get().expect("ordinary parent ran");
        let inner = entered[1].0;
        assert_eq!(entered[0].0, parent);
        assert_eq!(entered[0].1, None);
        assert_eq!(entered[1].1, Some(parent));
        assert_eq!(entered[1].2.key, Some(source_key));
        assert_eq!((parent.depth(), inner.depth()), (1, 2));
        let exited: Vec<_> = trace.iter().filter_map(|event| match event {
            NativeTrace::Exit { identity } => Some(*identity), _ => None,
        }).collect();
        assert_eq!(exited, [inner, parent]);
        let pauses: Vec<_> = trace.iter().filter_map(|event| match event {
            NativeTrace::SourceEnter { driver, epoch, queue, .. } => Some((*driver, *epoch, *queue)), _ => None,
        }).collect();
        assert_eq!(pauses.len(), 1);
        assert_eq!(pauses[0].0, parent);
        assert_eq!(pauses[0].2, case.parent_queue.get());
        let resumes: Vec<_> = trace.iter().filter_map(|event| match event {
            NativeTrace::SourceExit { driver, epoch } => Some((*driver, *epoch)), _ => None,
        }).collect();
        assert_eq!(resumes, [(parent, pauses[0].1)]);
        let journal = case.journal.borrow();
        assert_eq!(journal.iter().map(|snapshot| snapshot.stage).collect::<Vec<_>>(),
            [CancellationStage::Child, CancellationStage::Source, CancellationStage::Parent]);
        assert_eq!(journal.iter().map(|snapshot| snapshot.source_live).collect::<Vec<_>>(), [true, true, false]);
        assert_eq!(journal.iter().map(|snapshot| snapshot.driver).collect::<Vec<_>>(), [Some(inner), Some(parent), Some(parent)]);
        assert_eq!(journal.iter().map(|snapshot| snapshot.permit_clear).collect::<Vec<_>>(), [false, false, true]);
        assert_eq!(journal.iter().map(|snapshot| snapshot.parent_paused).collect::<Vec<_>>(), [None, None, Some(false)]);
        assert_eq!(journal.iter().map(|snapshot| snapshot.panicking).collect::<Vec<_>>(), [false, true, false]);
        assert_eq!(try_with_attempt(db_ref, 100_000, || {
            RegistryBuilder::new(db_ref, &ADMISSION)?.seal()?.run(|_| ready(Ok(())))
        }), Ok(AttemptOutcome::Complete(Ok(()))));
        assert_restored(db_ref);
        assert_eq!(RETAINED_DATABASES.with(Cell::get), retained_before);
        println!("SOURCE_NATIVE_CANCELLATION\tkind={kind:?}\tpayload={kind:?}\tdriver_entries=2\tsource_exits=1\tretained_delta=0\tqueue_dropped=true\tjournal={journal:?}\ttrace={trace:?}");
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn seed_operation(db: &dyn Database, node: Node) -> u32 {
    if attempt_probe::current().is_some() {
        complete_or_refused(with_native_callback(db, limits(), |entry| {
            RegistryBuilder::for_native_callback(db, &entry, &ADMISSION)?.seal()?.run(|endpoint| async move {
                Ok(endpoint.local_call(|| Ok(node.leaf(db))).await)
            })
        }))
    } else { node.leaf(db) }
}

struct ResetSeed;
impl Drop for ResetSeed { fn drop(&mut self) { SEED_ENTRY.with(|value| value.set(false)); } }

#[test]
fn configured_native_cycle_seed_can_enter_its_real_generated_body() {
    let _trace = native_source::trace();
    SEED_ENTRY.with(|value| value.set(true));
    let _reset = ResetSeed;
    let mut ordinary = TestDb::default();
    let original = Node::new(&ordinary, None, 0);
    original.set_next(&mut ordinary).to(Some(original));
    assert_eq!(*source(&ordinary, original), 3);
    let mut db = TestDb::default();
    let node = Node::new(&db, None, 0);
    node.set_next(&mut db).to(Some(node));
    LIMIT.with(|limit| limit.set(2));
    clear_trace();
    assert_eq!(try_with_attempt(&db, 100_000, || *source(&db, node)), Ok(AttemptOutcome::Complete(3)));
    let seed_key = seed_operation::fn_ingredient_(&db, db.zalsa()).database_key_index(node.as_id());
    let trace = native_source::take_trace();
    assert!(trace.iter().any(|event| matches!(event, NativeTrace::Enter { caller, .. } if caller.key == Some(seed_key))));
    assert!(trace.iter().all(|event| !matches!(event, NativeTrace::Enter { identity, .. } if identity.depth() > 2)));
    assert_restored(&db);
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn complete_only_entry(db: &dyn Database, node: Node) -> bool {
    let called = Cell::new(false);
    let result = with_native_callback(db, limits(), |_| { called.set(true); Ok(node.leaf(db)) });
    assert!(!called.get());
    matches!(result, Err(RunError::Contract(_)))
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn entry_contracts(db: &dyn Database, node: Node) -> bool {
    assert!(RegistryBuilder::new(db, &ADMISSION).is_err());
    if attempt_probe::current().is_none() {
        let called = Cell::new(false);
        assert!(matches!(with_native_callback(db, limits(), |_| { called.set(true); Ok(()) }), Err(RunError::Contract(_))));
        return !called.get();
    }
    let mut saved = None;
    let result = with_native_callback(db, limits(), |entry| {
        saved = Some(entry.receipt.clone());
        let foreign = TestDb::default();
        assert!(RegistryBuilder::for_native_callback(&foreign, &entry, &ADMISSION).is_err());
        assert_eq!(entry.receipt.deliver::<()>(&foreign, Err(RunError::Refused(Incomplete::Allowance))),
            Err(RunError::Refused(Incomplete::Allowance)));
        assert!(entry.receipt.support.reason().is_none());
        attempt_probe::try_with_operation(db, || {
            assert!(RegistryBuilder::for_native_callback(db, &entry, &ADMISSION).is_err());
            assert!(matches!(entry.receipt.deliver(db, Ok(())), Err(RunError::Contract(_))));
            assert_eq!(entry.receipt.deliver::<()>(db, Err(RunError::Refused(Incomplete::Allowance))),
                Err(RunError::Refused(Incomplete::Allowance)));
            assert_eq!(entry.receipt.deliver::<()>(db, Err(RunError::Contract("original mismatched failure"))),
                Err(RunError::Contract("original mismatched failure")));
            assert!(entry.receipt.support.reason().is_none());
        }).expect("a nested lexical scope is supported");
        assert!(entry.receipt.support.reason().is_none());
        println!("SOURCE_ENTRY_OWNER_REJECTION\tforeign=true\tscope=true\tfirst_error=true\tunmarked=true");
        let mut registry = RegistryBuilder::for_native_callback(db, &entry, &ADMISSION)?;
        assert!(registry.register_native_source(db, complete_only_entry::fn_ingredient_(db, db.zalsa())).is_err());
        let _source = registry.register_native_source(db, source::fn_ingredient_(db, db.zalsa()))?;
        assert!(registry.reserve_callable(db, source::fn_ingredient_(db, db.zalsa())).is_err());
        registry.seal()?.run(|endpoint| async move {
            endpoint.local_call(|| {
                let called = Cell::new(false);
                let result = with_native_callback(db, limits(), |_| { called.set(true); Ok(()) });
                assert!(matches!(result, Err(RunError::Contract(_))));
                assert!(!called.get());
                Ok(())
            }).await;
            Ok(())
        })?;
        let before = attempt_probe::remaining_allowance_for_diagnostics(db).expect("live callback scope");
        RegistryBuilder::for_native_callback(db, &entry, &ADMISSION)?.seal()?.run(|_| ready(Ok(())))?;
        assert_eq!(attempt_probe::remaining_allowance_for_diagnostics(db), Some(before - 1));
        Ok(())
    });
    assert!(result.is_ok());
    let saved = saved.expect("entry body ran");
    assert!(super::super::RunContext::for_native_callback(db, &saved).is_err());
    assert_eq!(saved.deliver::<()>(db, Err(RunError::Refused(Incomplete::Allowance))),
        Err(RunError::Refused(Incomplete::Allowance)));
    assert!(saved.support.reason().is_none());
    node.leaf(db) != 0
}

#[test]
fn entry_rejects_absent_foreign_stale_complete_only_and_unpermitted_callers() {
    let _trace = native_source::trace();
    let db = TestDb::default();
    let node = Node::new(&db, None, 1);
    LIMIT.with(|limit| limit.set(3));
    assert!(matches!(with_native_callback(&db, limits(), |_| Ok(())), Err(RunError::Contract(_))));
    assert!(entry_contracts(&db, node));
    // A separate node forces the native entry body to execute inside the new attempt.
    let cold = Node::new(&db, None, 1);
    assert_eq!(try_with_attempt(&db, 100_000, || entry_contracts(&db, cold)), Ok(AttemptOutcome::Complete(true)));
    let complete = Node::new(&db, None, 1);
    assert_eq!(try_with_attempt(&db, 100_000, || complete_only_entry(&db, complete)), Ok(AttemptOutcome::Complete(true)));
    assert_eq!(try_with_attempt(&db, 100_000, || {
        assert!(matches!(with_native_callback(&db, limits(), |_| Ok(())), Err(RunError::Contract(_))));
    }), Ok(AttemptOutcome::Complete(())));
    assert_restored(&db);
    let mut nested = TestDb::default();
    let nodes = graph(&mut nested, false);
    LIMIT.with(|limit| limit.set(2));
    CHECK_LIMIT.with(|flag| flag.set(true));
    assert_eq!(try_with_attempt(&nested, 100_000, || *source(&nested, nodes[0])), Ok(AttemptOutcome::Complete(2)));
    assert!(!CHECK_LIMIT.with(Cell::get));
}

#[test]
fn an_issued_checkpoint_prevents_native_source_pause() {
    let _trace = native_source::trace();
    let db = TestDb::default();
    let node = Node::new(&db, None, 1);
    clear_trace();
    assert_eq!(try_with_attempt(&db, 100_000, || {
        let mut registry = RegistryBuilder::new(&db, &ADMISSION)?;
        let source = registry.register_native_source(&db as &dyn Database, source::fn_ingredient_(&db, db.zalsa()))?;
        registry.seal()?.run(move |endpoint| async move {
            let mut checkpoint = Box::pin(endpoint.checkpoint()?);
            assert!(poll_once(checkpoint.as_mut()).is_pending());
            let mut read = Box::pin(endpoint.read_native_source(&source, node.as_id()));
            assert!(poll_once(read.as_mut()).is_pending());
            assert!(endpoint.inner.queue.active_poll.borrow().as_ref().is_some_and(|poll| poll.requested_checkpoint().is_some()));
            Ok(())
        })
    }), Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted)));
    assert!(!events().iter().any(|event| matches!(event, Event::Source(_))));
    assert_restored(&db);
}

fn print_events() {
    for event in events() {
        match event {
            Event::Source(id) => println!("SOURCE_BODY\t{id:?}"),
            Event::NativeGenerated(id) => println!("NATIVE_GENERATED_BODY\t{id:?}"),
            Event::SourceReturn(id, value) => println!("SOURCE_RETURN\t{id:?}\t{value}"),
            Event::NativeGeneratedReturn(id, value) => println!("NATIVE_GENERATED_RETURN\t{id:?}\t{value}"),
            Event::Registered(id, remaining) => println!("REGISTERED_BODY\t{id:?}\t{remaining:?}"),
            Event::Initial(kind, id) => println!("CYCLE_INITIAL\t{kind}\t{id:?}"),
            Event::Recovery(kind, id, old, value) => println!("CYCLE_RECOVERY\t{kind}\t{id:?}\t{old}\t{value}"),
            Event::Work(work) => println!("SOURCE_ENTRY_WORK\t{work:?}"),
            Event::Native(event) => println!("SOURCE_ENTRY_NATIVE\t{event}"),
        }
    }
}

#[test]
fn source_entry_layout_and_ordinary_resource_records() {
    let _trace = native_source::trace();
    println!("SOURCE_ENTRY_LAYOUT\tnative={:?}\tqueue={}:{}\tcontext={}:{}\tcheckpoint={}:{}\tterminal={}:{}",
        native_source::layout(),
        size_of::<super::super::Queue<'_>>(), align_of::<super::super::Queue<'_>>(),
        size_of::<super::super::RunContext<'_>>(), align_of::<super::super::RunContext<'_>>(),
        size_of::<super::super::progress::Checkpoint<'_, '_, '_>>(), align_of::<super::super::progress::Checkpoint<'_, '_, '_>>(),
        size_of::<super::super::progress::TerminalFailure<'_, '_, '_>>(), align_of::<super::super::progress::TerminalFailure<'_, '_, '_>>());
    let db = TestDb::default();
    println!("SOURCE_ENTRY_REGISTRATION_LAYOUT\troute_entry_factory_route={:?}\tdriver={}:{}\taccess={}:{}\tactive_poll={}:{}\tterminal_disposition={}:{}",
        super::super::registration::source_layout(source::fn_ingredient_(&db, db.zalsa())),
        size_of::<super::super::Driver<'_, '_>>(), align_of::<super::super::Driver<'_, '_>>(),
        size_of::<super::super::QueueAccess<'_, '_>>(), align_of::<super::super::QueueAccess<'_, '_>>(),
        size_of::<super::super::progress::ActivePoll>(), align_of::<super::super::progress::ActivePoll>(),
        size_of::<super::super::progress::TerminalDisposition>(), align_of::<super::super::progress::TerminalDisposition>());
    clear_trace();
    assert_eq!(try_with_attempt(&db, 100, || {
        RegistryBuilder::new(&db, &ADMISSION)?.seal()?.run(|endpoint| async move {
            endpoint.local_call(|| Ok(())).await;
            endpoint.checkpoint()?.await?;
            Ok(())
        })
    }), Ok(AttemptOutcome::Complete(Ok(()))));
    print_events();
    assert_restored(&db);
}
