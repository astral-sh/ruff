use std::cell::{Cell, RefCell};
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use super::{Driver, Endpoint, ExecutionAdmission, ExecutionWork, Provider, RunError, RunResult};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, QueryPolicy, try_with_attempt};
use crate::function::memo::SelectedMemo;
use crate::function::{ClaimResult, Configuration, IngredientImpl, Memo, Reentrancy};
use crate::plumbing::AsId;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseImpl, DatabaseKeyIndex, Id, Setter};

#[cfg(not(feature = "shuttle"))]
use crate::attempt_probe::transfer_test_support::{
    self as transfer_trace, Action, Event, Kind, Mode,
};
#[cfg(not(feature = "shuttle"))]
use crate::function::SyncOwner;

// Use only after auditing the concrete fixture's input conversion and output equality.
// These operations own no heap payload; output destruction remains in the producer fixture.
macro_rules! fixture_native_value {
    (executable, $run:lifetime, $db:lifetime, $config:ident, $work:expr) => {
        async fn native_value<'call>(
            &$run self,
            _context: crate::function::execute::execution_run::registration::ProviderContext<$run, $db, Self>,
            _db: &$db <$config as crate::function::Configuration>::DbView,
            _operation: crate::function::execute::execution_run::native_values::NativeValueOperation<'call, $db, $config>,
        ) -> crate::function::execute::execution_run::RunResult<crate::function::execute::execution_run::native_values::NativeValueQuote>
        where
            $run: 'call,
        {
            Ok(crate::function::execute::execution_run::native_values::NativeValueQuote {
                work: $work,
                requested_bytes: 0,
                cleanup_work: 0,
            })
        }
    };
    (callable, $run:lifetime, $db:lifetime, $config:ident, $work:expr) => {
        async fn native_value<'call>(
            &'call self,
            _endpoint: crate::function::execute::execution_run::registration::TaskEndpoint<$run, $db>,
            _db: &$db <$config as crate::function::Configuration>::DbView,
            _operation: crate::function::execute::execution_run::native_values::NativeValueOperation<'call, $db, $config>,
        ) -> crate::function::execute::execution_run::RunResult<crate::function::execute::execution_run::native_values::NativeValueQuote>
        where
            $run: 'call,
        {
            Ok(crate::function::execute::execution_run::native_values::NativeValueQuote {
                work: $work,
                requested_bytes: 0,
                cleanup_work: 0,
            })
        }
    };
    (provider, $run:lifetime, $db:lifetime, $config:ident, $work:expr) => {
        async fn native_value(
            &self,
            _db: &$db <$config as crate::function::Configuration>::DbView,
            _operation: crate::function::execute::execution_run::native_values::NativeValueOperation<'_, $db, $config>,
            _endpoint: crate::function::execute::execution_run::Endpoint<$run, $db>,
        ) -> crate::function::execute::execution_run::RunResult<crate::function::execute::execution_run::native_values::NativeValueQuote> {
            Ok(crate::function::execute::execution_run::native_values::NativeValueQuote {
                work: $work,
                requested_bytes: 0,
                cleanup_work: 0,
            })
        }
    };
    (execution, $run:lifetime, $db:lifetime, $config:ident, $work:expr) => {
        async fn native_value<'call>(
            &'call self,
            _db: &$db <$config as crate::function::Configuration>::DbView,
            _operation: crate::function::execute::execution_run::native_values::NativeValueOperation<'call, $db, $config>,
            _endpoint: crate::function::execute::execution_run::Endpoint<$run, $db>,
        ) -> crate::function::execute::execution_run::RunResult<crate::function::execute::execution_run::native_values::NativeValueQuote>
        where
            $run: 'call,
        {
            Ok(crate::function::execute::execution_run::native_values::NativeValueQuote {
                work: $work,
                requested_bytes: 0,
                cleanup_work: 0,
            })
        }
    };
}

mod admission_authority;
mod callable_routes;

mod callback;
mod canonical_callouts;
mod checkpoint;
mod complete_only_callable;
mod completion_preparation;
mod cycle_publication;
mod explicit_reads;
pub(super) mod fetch;
mod field_reads;
mod finalized_source;
mod native_value_cycles;
mod native_values;
pub(super) mod observation;
mod output_drop;
#[cfg(not(feature = "shuttle"))]
mod ownership;
#[cfg(not(feature = "shuttle"))]
mod parallel_wait;
#[cfg(not(feature = "shuttle"))]
mod participant_callbacks;
#[cfg(not(feature = "shuttle"))]
mod provisional_eligibility;
mod publication;
mod query_keys;
mod shared_routes;
mod source_callbacks;
mod structural_dependencies;
mod terminal;
mod validation;
mod validation_direct_probe;
pub(super) mod validation_trace;

#[crate::input]
struct Node {
    #[returns(copy)]
    next: Option<Node>,
    #[returns(copy)]
    seed: u32,
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover)]
fn fixpoint(db: &dyn Database, node: Node) -> u32 {
    let value = node.next(db).map_or(0, |next| fixpoint(db, next));
    (value + 1).min(3)
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_result = initial)]
fn fallback(db: &dyn Database, node: Node) -> u32 {
    node.next(db).map_or(0, |next| fallback(db, next)) + 1
}

fn initial(db: &dyn Database, _id: Id, node: Node) -> u32 {
    node.seed(db)
}
fn recover(_db: &dyn Database, _cycle: &Cycle<'_>, _old: &u32, value: u32, _node: Node) -> u32 {
    value
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Body,
    Initial,
    Recovery,
    Complete,
}

#[derive(Clone, Copy)]
enum Stop {
    Never,
    At(Phase, usize),
    Panic(Phase),
    MissingChild,
    RefuseThenSuccess,
    DropPanic,
    CancelRevision,
}

struct Script {
    stop: Stop,
    bodies: Cell<usize>,
    initials: Cell<usize>,
    recoveries: Cell<usize>,
    completions: Cell<usize>,
    suspended: Cell<usize>,
    observe_fault: bool,
    fault: RefCell<Option<ScriptFault>>,
    #[cfg(not(feature = "shuttle"))]
    worker: RefCell<Option<DatabaseImpl>>,
}

impl Script {
    fn new(stop: Stop) -> Rc<Self> {
        Self::with_fault_observation(stop, false)
    }

    fn with_fault_observation(stop: Stop, observe_fault: bool) -> Rc<Self> {
        Rc::new(Self {
            stop,
            bodies: Cell::new(0),
            initials: Cell::new(0),
            recoveries: Cell::new(0),
            completions: Cell::new(0),
            suspended: Cell::new(0),
            observe_fault,
            fault: RefCell::new(None),
            #[cfg(not(feature = "shuttle"))]
            worker: RefCell::new(None),
        })
    }

    fn observed(stop: Stop) -> Rc<Self> {
        Self::with_fault_observation(stop, true)
    }

    fn counters(&self) -> [usize; 4] {
        [
            self.bodies.get(),
            self.initials.get(),
            self.recoveries.get(),
            self.completions.get(),
        ]
    }

    fn visit(&self, phase: Phase) -> RunResult<()> {
        #[cfg(not(feature = "shuttle"))]
        if let Some(worker) = self.worker.borrow_mut().take() {
            assert_eq!(attempt_probe::current_policy(), QueryPolicy::ReturnOnly);
            let result = std::thread::spawn(move || {
                (
                    try_with_attempt(&worker, 1, || ()),
                    attempt_probe::try_with_operation(&worker, || ()),
                )
            })
            .join()
            .expect("worker completed");
            assert_eq!(result.0, Ok(AttemptOutcome::Complete(())));
            assert_eq!(result.1, Ok(()));
        }
        let count = match phase {
            Phase::Body => &self.bodies,
            Phase::Initial => &self.initials,
            Phase::Recovery => &self.recoveries,
            Phase::Complete => &self.completions,
        };
        let ordinal = count.get();
        count.set(ordinal + 1);
        match self.stop {
            Stop::At(stop, target) if stop == phase && target == ordinal => {
                Err(RunError::Refused(Incomplete::Allowance))
            }
            Stop::Panic(stop) if stop == phase => panic!("scripted callback panic"),
            _ => Ok(()),
        }
    }
}

// Lifecycle faults select a real callback boundary independently of the work needed to reach it.
const LIFECYCLE_FIXTURE_RESERVE: usize = 100_000;

#[derive(Debug)]
struct ScriptFault {
    phase: Phase,
    counters: [usize; 4],
    remaining: usize,
    key: DatabaseKeyIndex,
    active: Option<(DatabaseKeyIndex, bool)>,
    memo_value: Option<u32>,
    memo_provisional: Option<bool>,
    memo_heads: Vec<DatabaseKeyIndex>,
    #[cfg(not(feature = "shuttle"))]
    sync: Option<transfer_trace::SyncSnapshot>,
    #[cfg(not(feature = "shuttle"))]
    graph: transfer_trace::GraphSnapshot,
}

impl ScriptFault {
    fn capture<C>(
        db: &dyn Database,
        ingredient: &IngredientImpl<C>,
        node: Node,
        phase: Phase,
        counters: [usize; 4],
    ) -> Self
    where
        C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
    {
        let selected = memo(db, ingredient, node);
        Self {
            phase,
            counters,
            remaining: attempt_probe::remaining_allowance_for_diagnostics(db)
                .expect("the scripted fault is observed before refusal propagates"),
            key: ingredient.database_key_index(node.as_id()),
            active: db
                .zalsa_local()
                .try_with_query_stack(|stack| {
                    stack
                        .last()
                        .map(|frame| (frame.database_key_index, frame.attempt_incomplete()))
                })
                .flatten(),
            memo_value: selected.and_then(|memo| memo.value().copied()),
            memo_provisional: selected.map(|memo| memo.header.may_be_provisional()),
            memo_heads: selected.map_or_else(Vec::new, |memo| {
                memo.header
                    .revisions
                    .cycle_heads()
                    .iter()
                    .map(|head| head.database_key_index)
                    .collect()
            }),
            #[cfg(not(feature = "shuttle"))]
            sync: ingredient.sync_table.test_transfer_state(node.as_id()),
            #[cfg(not(feature = "shuttle"))]
            graph: db.zalsa().runtime().test_transfer_graph_snapshot(),
        }
    }

    fn assert_owned(&self, phase: Phase, key: DatabaseKeyIndex, counters: [usize; 4]) {
        assert_eq!(self.phase, phase);
        assert_eq!(self.counters, counters);
        assert!(
            self.remaining > 0,
            "the selected lifecycle fault must precede budget exhaustion"
        );
        assert_eq!(self.key, key);
        assert_eq!(self.active, Some((key, false)));
        #[cfg(not(feature = "shuttle"))]
        assert!(self.sync.is_some_and(|snapshot| {
            matches!(snapshot.owner, SyncOwner::Thread(owner) if owner == std::thread::current().id())
        }));
    }
}

struct ScriptTrace {
    #[cfg(not(feature = "shuttle"))]
    trace: transfer_trace::TransferTrace,
}

impl ScriptTrace {
    fn collect<T>(body: impl FnOnce() -> T) -> (T, Self) {
        #[cfg(not(feature = "shuttle"))]
        {
            let (value, trace) = transfer_trace::collect(
                transfer_trace::TraceConfig {
                    worker: 0,
                    ordinal: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                },
                body,
            );
            (value, Self { trace })
        }
        #[cfg(feature = "shuttle")]
        {
            (body(), Self {})
        }
    }

    fn assert_refusal_cleanup(&self) {
        #[cfg(not(feature = "shuttle"))]
        {
            assert!(!self.trace.broken);
            assert!(!self.trace.records.iter().any(|record| matches!(
                record.event.kind,
                Kind::Poisoned | Kind::Edge | Kind::WaitConsumed
            )));
            for claim in self
                .trace
                .records
                .iter()
                .filter(|record| record.event.kind == Kind::Claim)
            {
                let terminals: Vec<_> = self
                    .trace
                    .records
                    .iter()
                    .filter(|record| {
                        record.event.kind == Kind::Terminal
                            && record.event.serial == claim.event.serial
                    })
                    .collect();
                assert_eq!(terminals.len(), 1, "each acquired claim retires once");
                assert!(terminals[0].ordinal > claim.ordinal);
            }
        }
    }

    fn assert_default_fault(&self, key: DatabaseKeyIndex) {
        #[cfg(not(feature = "shuttle"))]
        {
            let faults: Vec<_> = self
                .trace
                .records
                .iter()
                .filter(|record| record.event.phase == Some("scripted-lifecycle-refusal"))
                .collect();
            assert_eq!(faults.len(), 1);
            let fault = faults[0];
            assert_eq!(fault.event.key, Some(key));
            let claim = self
                .trace
                .records
                .iter()
                .rev()
                .find(|record| {
                    record.ordinal < fault.ordinal
                        && record.event.kind == Kind::Claim
                        && record.event.key == Some(key)
                })
                .unwrap();
            assert_eq!(claim.event.mode, Some(Mode::Default));
            assert!(!claim.event.sync.unwrap().claimed_twice);
            let terminal = self
                .trace
                .records
                .iter()
                .find(|record| {
                    record.ordinal > fault.ordinal
                        && record.event.kind == Kind::Terminal
                        && record.event.serial == claim.event.serial
                })
                .unwrap();
            assert_eq!(terminal.event.mode, Some(Mode::Default));
            assert_eq!(terminal.event.action, Some(Action::Abort));
            assert!(matches!(
                terminal.event.wait,
                Some(crate::runtime::WaitResult::Cancelled)
            ));
        }
        #[cfg(feature = "shuttle")]
        let _ = key;
    }

    fn assert_reclaimed_fault(&self, key: DatabaseKeyIndex, head: DatabaseKeyIndex) {
        #[cfg(not(feature = "shuttle"))]
        {
            let fault = self
                .trace
                .records
                .iter()
                .find(|record| record.event.phase == Some("scripted-lifecycle-refusal"))
                .unwrap();
            assert_eq!(fault.event.key, Some(key));
            let claim = self
                .trace
                .records
                .iter()
                .rev()
                .find(|record| {
                    record.ordinal < fault.ordinal
                        && record.event.kind == Kind::Claim
                        && record.event.key == Some(key)
                })
                .unwrap();
            assert_eq!(claim.event.mode, Some(Mode::SelfOnly));
            assert!(claim.event.sync.unwrap().claimed_twice);
            assert!(
                self.trace
                    .records
                    .iter()
                    .any(|record| record.ordinal < claim.ordinal
                        && record.event.kind == Kind::Terminal
                        && record.event.key == Some(key)
                        && record.event.mode == Some(Mode::TransferTo(head)))
            );
            let terminal = self
                .trace
                .records
                .iter()
                .find(|record| {
                    record.ordinal > fault.ordinal
                        && record.event.kind == Kind::Terminal
                        && record.event.serial == claim.event.serial
                })
                .unwrap();
            assert_eq!(terminal.event.action, Some(Action::Abort));
            // Starting fixpoint execution resets a reclaimed claim's retirement mode.
            assert_eq!(terminal.event.mode, Some(Mode::Default));
            assert!(matches!(
                terminal.event.wait,
                Some(crate::runtime::WaitResult::Cancelled)
            ));
            assert!(
                !self
                    .trace
                    .records
                    .iter()
                    .any(|record| record.ordinal > fault.ordinal
                        && matches!(
                            record.event.kind,
                            Kind::TransferBegin | Kind::TransferEnd | Kind::Restore
                        ))
            );
        }
        #[cfg(feature = "shuttle")]
        let _ = (key, head);
    }

    fn assert_no_transfer(&self) {
        #[cfg(not(feature = "shuttle"))]
        assert!(
            !self
                .trace
                .records
                .iter()
                .any(|record| matches!(record.event.kind, Kind::TransferBegin | Kind::TransferEnd))
        );
    }
}

fn assert_foreign_seeds<C>(db: &dyn Database, ingredient: &IngredientImpl<C>, nodes: [Node; 2])
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    assert!(attempt_probe::remaining_allowance_for_diagnostics(db).is_some());
    for node in nodes {
        if let Some(memo) = memo(db, ingredient, node) {
            assert!(
                !memo.header.can_seed_attempt(db.zalsa()),
                "a new request owner cannot seed from the refused attempt"
            );
        }
    }
}

fn assert_script_idle<C>(db: &dyn Database, ingredient: &IngredientImpl<C>, nodes: [Node; 2])
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(attempt_probe::current().is_none());
    for node in nodes {
        assert!(matches!(
            ingredient
                .sync_table
                .peek_claim(db.zalsa(), node.as_id(), Reentrancy::Deny),
            ClaimResult::Claimed(())
        ));
    }
    #[cfg(not(feature = "shuttle"))]
    {
        let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
        assert!(graph.edges.is_empty() && graph.dependents.is_empty() && graph.pending.is_empty());
        assert!(graph.transferred.is_empty() && graph.reverse.is_empty());
        assert!(
            !graph.edges.overflow
                && !graph.dependents.overflow
                && !graph.pending.overflow
                && !graph.transferred.overflow
                && !graph.reverse.overflow
        );
    }
}

struct ScriptBudget<'db, C: Configuration> {
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    participant: Node,
    remaining: Cell<usize>,
    debits: RefCell<Vec<usize>>,
    last: RefCell<Option<(ExecutionWork, usize)>>,
    #[cfg(not(feature = "shuttle"))]
    last_sync: RefCell<Option<transfer_trace::SyncSnapshot>>,
}

impl<C> ExecutionAdmission for ScriptBudget<'_, C>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let remaining = attempt_probe::remaining_allowance_for_diagnostics(self.db).unwrap();
        let previous = self.remaining.replace(remaining);
        assert!(remaining <= previous);
        if remaining < previous {
            self.debits.borrow_mut().push(previous - remaining);
        }
        #[cfg(not(feature = "shuttle"))]
        {
            *self.last_sync.borrow_mut() = self
                .ingredient
                .sync_table
                .test_transfer_state(self.participant.as_id());
        }
        #[cfg(feature = "shuttle")]
        let _ = (self.ingredient, self.participant);
        *self.last.borrow_mut() = Some((work, remaining));
        Ok(())
    }
}

fn early_script_budget<C>(
    db: &dyn Database,
    ingredient: &IngredientImpl<C>,
    first: Node,
    second: Node,
    stop: Stop,
    expected: [usize; 4],
) where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    // Preserve the retirement cutoff after charging native quotations and provisional publication.
    const RETIREMENT_ALLOWANCE: usize = 100;
    // One participant entry costs eight units, and installing its root memo costs one.
    const PARTICIPANT_PUBLICATION_WORK: usize = 8 + 1;
    // Each input conversion includes two quotation steps and its one-unit native operation.
    const FALLBACK_INPUT_WORK: usize = 3 * 3;
    const FIXPOINT_INPUT_WORK: usize = 2 * 3;
    let native_work = if matches!(stop, Stop::At(Phase::Initial, _)) {
        FALLBACK_INPUT_WORK
    } else {
        FIXPOINT_INPUT_WORK
    };
    let allowance = RETIREMENT_ALLOWANCE + native_work + PARTICIPANT_PUBLICATION_WORK;
    let script = Script::observed(stop);
    let admission = ScriptBudget {
        db,
        ingredient,
        participant: second,
        remaining: Cell::new(allowance),
        debits: RefCell::new(Vec::new()),
        last: RefCell::new(None),
        #[cfg(not(feature = "shuttle"))]
        last_sync: RefCell::new(None),
    };
    let (outcome, trace) = ScriptTrace::collect(|| {
        try_with_attempt(db, allowance, || {
            Driver::run_with_admission(db, &admission, |endpoint| {
                let script = script.clone();
                async move {
                    endpoint
                        .execute(
                            ingredient,
                            db,
                            first.as_id(),
                            None,
                            ScriptProvider {
                                ingredient,
                                node: first,
                                script,
                                ancestors: Vec::new(),
                            },
                        )?
                        .await
                }
            })
        })
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(script.counters(), expected);
    assert!(
        script.fault.borrow().is_none(),
        "the lifecycle fault has not been reached"
    );
    // Native quotation and scalar operations debit one unit each; publishing the provisional memo adds
    // nine units before the next retirement request can exhaust the shared allowance.
    let expected_debits: &[usize] = if matches!(stop, Stop::At(Phase::Initial, _)) {
        &[1, 1, 1, 1, 1, 1, 12, 15, 12, 1, 1, 1, 9, 22, 25]
    } else {
        &[1, 1, 1, 1, 1, 1, 12, 15, 12, 9, 22, 25]
    };
    let remaining = allowance - expected_debits.iter().sum::<usize>();
    assert_eq!(&*admission.debits.borrow(), expected_debits);
    assert_eq!(remaining, 14);
    assert_eq!(admission.remaining.get(), remaining);
    assert_eq!(
        *admission.last.borrow(),
        Some((ExecutionWork::Resource { requested_bytes: 0 }, remaining))
    );
    #[cfg(not(feature = "shuttle"))]
    assert!(admission.last_sync.borrow().is_some_and(|snapshot| {
        matches!(snapshot.owner, SyncOwner::Thread(owner) if owner == std::thread::current().id())
    }));
    trace.assert_refusal_cleanup();
    trace.assert_no_transfer();
    for node in [first, second] {
        if let Some(memo) = memo(db, ingredient, node) {
            assert!(memo.header.may_be_provisional());
            assert!(!memo.header.can_seed_attempt(db.zalsa()));
            assert!(
                memo.value().is_some(),
                "normal refusal does not poison the seed"
            );
        }
    }
    assert_script_idle(db, ingredient, [first, second]);
}

struct ScriptProvider<'db, C: Configuration> {
    ingredient: &'db IngredientImpl<C>,
    node: Node,
    script: Rc<Script>,
    ancestors: Vec<Node>,
}

impl<C: Configuration> Clone for ScriptProvider<'_, C> {
    fn clone(&self) -> Self {
        Self {
            ingredient: self.ingredient,
            node: self.node,
            script: self.script.clone(),
            ancestors: self.ancestors.clone(),
        }
    }
}

// Every scripted callback yields to a real child task before completing. This exercises
// guard ownership across Pending, even when the callback's computation itself is a leaf.
async fn checkpoint<'run, 'db: 'run, C>(
    script: Rc<Script>,
    endpoint: Endpoint<'run, 'db>,
    phase: Phase,
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    node: Node,
) -> RunResult<()>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    endpoint
        .demand(move || async move {
            script.suspended.set(script.suspended.get() + 1);
            let result = script.visit(phase);
            if result.is_err() && script.observe_fault {
                let fault = ScriptFault::capture(db, ingredient, node, phase, script.counters());
                assert!(script.fault.borrow_mut().replace(fault).is_none());
                #[cfg(not(feature = "shuttle"))]
                {
                    let mut event =
                        Event::new(Kind::Gate).key(ingredient.database_key_index(node.as_id()));
                    event.phase = Some("scripted-lifecycle-refusal");
                    transfer_trace::record(event);
                }
            }
            result
        })?
        .await
}

impl<'run, 'db: 'run, C> Provider<'run, 'db, C> for ScriptProvider<'db, C>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    // Node conversion constructs a handle; output equality compares u32.
    fixture_native_value!(provider, 'run, 'db, C, 1);

    type Output = u32;

    async fn body(
        &self,
        db: &'db dyn Database,
        input: Node,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        checkpoint(
            self.script.clone(),
            endpoint.clone(),
            Phase::Body,
            db,
            self.ingredient,
            input,
        )
        .await?;
        if matches!(self.script.stop, Stop::MissingChild) {
            std::future::pending::<()>().await;
        }
        if matches!(self.script.stop, Stop::DropPanic) {
            endpoint
                .demand(|| PendingWithPanic {
                    _on_drop: PanicOnDrop,
                })?
                .await?;
        }
        if matches!(self.script.stop, Stop::CancelRevision) {
            db.zalsa().runtime().set_cancellation_flag();
        }
        if matches!(self.script.stop, Stop::RefuseThenSuccess) {
            attempt_probe::report_incomplete(db, Incomplete::Allowance);
            return Ok(99);
        }
        let value = if let Some(next) = input.next(db) {
            if next == input || self.ancestors.contains(&next) {
                // Only this reentrant seed read uses synchronous fetch. The target is already
                // active, and `initial` above is query-free. This is not cold fetch support.
                *self
                    .ingredient
                    .fetch(db, db.zalsa(), db.zalsa_local(), next.as_id())
            } else {
                let mut child = self.clone();
                child.node = next;
                child.ancestors.push(input);
                let previous = memo(db, self.ingredient, next);
                endpoint
                    .execute(self.ingredient, db, next.as_id(), previous, child)?
                    .await?
            }
        } else {
            0
        };
        Ok(
            if C::CYCLE_STRATEGY == crate::cycle::CycleRecoveryStrategy::Fixpoint {
                (value + 1).min(3)
            } else {
                value + 1
            },
        )
    }

    async fn initial(
        &self,
        db: &'db dyn Database,
        _id: Id,
        input: Node,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        checkpoint(
            self.script.clone(),
            endpoint,
            Phase::Initial,
            db,
            self.ingredient,
            input,
        )
        .await?;
        Ok(input.seed(db))
    }

    async fn recover(
        &self,
        db: &'db dyn Database,
        _cycle: &Cycle<'_>,
        _last: &u32,
        value: u32,
        input: Node,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        checkpoint(
            self.script.clone(),
            endpoint,
            Phase::Recovery,
            db,
            self.ingredient,
            input,
        )
        .await?;
        Ok(value)
    }

    async fn complete(
        &self,
        db: &'db dyn Database,
        memo: Option<SelectedMemo<'db, C>>,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        assert_eq!(attempt_probe::current_policy(), QueryPolicy::ReturnOnly);
        let selected = memo.ok_or(RunError::RequiresFetch)?;
        let selected_address = std::ptr::from_ref(selected.memo());
        checkpoint(
            self.script.clone(),
            endpoint,
            Phase::Complete,
            db,
            self.ingredient,
            self.node,
        )
        .await?;
        assert_eq!(attempt_probe::current_policy(), QueryPolicy::ReturnOnly);
        assert_eq!(std::ptr::from_ref(selected.memo()), selected_address);
        Ok(*self.ingredient.record_memo_read(
            db.zalsa(),
            db.zalsa_local(),
            self.node.as_id(),
            &selected,
        ))
    }
}

fn run<C>(
    db: &dyn Database,
    ingredient: &IngredientImpl<C>,
    node: Node,
    script: Rc<Script>,
) -> RunResult<u32>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    let index = ingredient.memo_ingredient_index(db.zalsa(), node.as_id());
    let old = ingredient.get_memo_from_table_for(db.zalsa(), node.as_id(), index);
    Driver::run(db, move |endpoint| async move {
        let provider = ScriptProvider {
            ingredient,
            node,
            script,
            ancestors: Vec::new(),
        };
        endpoint
            .execute(ingredient, db, node.as_id(), old, provider)?
            .await
    })
}

fn memo<'db, C: Configuration>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    node: Node,
) -> Option<&'db Memo<C>> {
    ingredient.get_memo_from_table_for(
        db.zalsa(),
        node.as_id(),
        ingredient.memo_ingredient_index(db.zalsa(), node.as_id()),
    )
}

#[test]
fn body_abort_has_no_value_and_retry_keeps_revision() {
    let db = DatabaseImpl::default();
    let node = Node::new(&db, None, 0);
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    let stamp = crate::prepared_source_probe::Stamp::current(&db);
    assert_eq!(
        try_with_attempt(&db, 100, || run(
            &db,
            ingredient,
            node,
            Script::new(Stop::At(Phase::Body, 0))
        )),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert!(memo(&db, ingredient, node).is_none());
    assert_eq!(
        try_with_attempt(&db, 100, || run(
            &db,
            ingredient,
            node,
            Script::new(Stop::Never)
        )),
        Ok(AttemptOutcome::Complete(Ok(1)))
    );
    assert!(stamp.belongs_to(&db));
    assert!(db.zalsa_local().active_query().is_none());
}

#[test]
fn every_aborted_ancestor_without_a_completed_child_read_is_marked() {
    let db = DatabaseImpl::default();
    let leaf = Node::new(&db, None, 0);
    let middle = Node::new(&db, Some(leaf), 0);
    let root = Node::new(&db, Some(middle), 0);
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    assert_eq!(
        try_with_attempt(&db, 100, || run(
            &db,
            ingredient,
            root,
            Script::new(Stop::At(Phase::Body, 2))
        )),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    for node in [root, middle, leaf] {
        assert!(memo(&db, ingredient, node).is_none());
    }
    assert_eq!(
        try_with_attempt(&db, 100, || run(
            &db,
            ingredient,
            root,
            Script::new(Stop::Never)
        )),
        Ok(AttemptOutcome::Complete(Ok(3)))
    );
}

#[test]
fn recovery_abort_retains_provisional_value_without_poisoning() {
    // The complete retry includes native quotations, comparisons and memo publication.
    const RETRY_WORK: usize = 124;
    for ordinal in [0, 1] {
        let mut db = DatabaseImpl::default();
        let node = Node::new(&db, None, 0);
        node.set_next(&mut db).to(Some(node));
        let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
        assert_eq!(
            try_with_attempt(&db, 100, || run(
                &db,
                ingredient,
                node,
                Script::new(Stop::At(Phase::Recovery, ordinal))
            )),
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        let provisional = memo(&db, ingredient, node).expect("cycle inserted its initial memo");
        assert!(provisional.value().is_some());
        assert!(provisional.header.may_be_provisional());
        assert_eq!(
            try_with_attempt(&db, RETRY_WORK, || {
                assert!(!provisional.header.can_seed_attempt(db.zalsa()));
                let result = run(&db, ingredient, node, Script::new(Stop::Never));
                assert_eq!(
                    attempt_probe::remaining_allowance_for_diagnostics(&db),
                    Some(0)
                );
                result
            }),
            Ok(AttemptOutcome::Complete(Ok(3)))
        );
    }
}

#[test]
fn fallback_budget_refuses_retirement_before_the_second_initial_callback() {
    let mut db = DatabaseImpl::default();
    let first = Node::new(&db, None, 10);
    let second = Node::new(&db, Some(first), 20);
    first.set_next(&mut db).to(Some(second));
    let ingredient = fallback::fn_ingredient_(&db, db.zalsa());
    let stamp = crate::prepared_source_probe::Stamp::current(&db);
    early_script_budget(
        &db,
        ingredient,
        first,
        second,
        Stop::At(Phase::Initial, 1),
        [2, 1, 0, 0],
    );
    assert!(stamp.belongs_to(&db));
}

#[test]
fn fallback_head_and_participant_abort_keep_distinct_seeds() {
    for ordinal in [0, 1] {
        let mut db = DatabaseImpl::default();
        let first = Node::new(&db, None, 10);
        let second = Node::new(&db, Some(first), 20);
        first.set_next(&mut db).to(Some(second));
        let ingredient = fallback::fn_ingredient_(&db, db.zalsa());
        let first_key = ingredient.database_key_index(first.as_id());
        let second_key = ingredient.database_key_index(second.as_id());
        let stamp = crate::prepared_source_probe::Stamp::current(&db);
        let script = Script::observed(Stop::At(Phase::Initial, ordinal));
        let (outcome, trace) = ScriptTrace::collect(|| {
            try_with_attempt(&db, LIFECYCLE_FIXTURE_RESERVE, || {
                run(&db, ingredient, first, script.clone())
            })
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        let fault = script.fault.borrow();
        let fault = fault
            .as_ref()
            .expect("the chosen initial callback was reached");
        let (key, counters) = if ordinal == 0 {
            (second_key, [2, 1, 0, 0])
        } else {
            (first_key, [2, 2, 0, 1])
        };
        fault.assert_owned(Phase::Initial, key, counters);
        trace.assert_default_fault(key);
        #[cfg(not(feature = "shuttle"))]
        {
            assert!(!fault.sync.unwrap().claimed_twice);
            assert_eq!(fault.sync.unwrap().is_transfer_target, ordinal == 1);
            if ordinal == 0 {
                assert!(fault.graph.transferred.is_empty());
            } else {
                assert!(
                    fault
                        .graph
                        .transferred
                        .entries
                        .iter()
                        .flatten()
                        .any(|(key, _, head)| (*key, *head) == (second_key, first_key))
                );
            }
            assert!(!fault.graph.transferred.overflow);
        }
        if ordinal == 0 {
            assert_eq!(fault.memo_value, None);
            assert_eq!(fault.memo_provisional, None);
            assert!(fault.memo_heads.is_empty());
            assert!(memo(&db, ingredient, second).is_none());
        } else {
            assert_eq!(fault.memo_value, Some(10));
            assert_eq!(fault.memo_provisional, Some(true));
            assert_eq!(fault.memo_heads, [first_key]);
            assert_eq!(memo(&db, ingredient, second).unwrap().value(), Some(&20));
        }
        assert_eq!(memo(&db, ingredient, first).unwrap().value(), Some(&10));
        for node in [first, second] {
            if let Some(memo) = memo(&db, ingredient, node) {
                assert!(memo.header.may_be_provisional());
                assert!(!memo.header.can_seed_attempt(db.zalsa()));
            }
        }
        trace.assert_refusal_cleanup();
        assert_script_idle(&db, ingredient, [first, second]);
        assert_eq!(
            try_with_attempt(&db, LIFECYCLE_FIXTURE_RESERVE, || {
                assert_foreign_seeds(&db, ingredient, [first, second]);
                run(&db, ingredient, first, Script::new(Stop::Never))
            }),
            Ok(AttemptOutcome::Complete(Ok(10)))
        );
        assert_eq!(
            try_with_attempt(&db, LIFECYCLE_FIXTURE_RESERVE, || fallback(&db, second)),
            Ok(AttemptOutcome::Complete(20))
        );
        assert!(stamp.belongs_to(&db));
        assert_script_idle(&db, ingredient, [first, second]);
    }
}

#[test]
fn incomplete_callback_cannot_deliver_success() {
    let db = DatabaseImpl::default();
    let node = Node::new(&db, None, 0);
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    assert_eq!(
        try_with_attempt(&db, 100, || run(
            &db,
            ingredient,
            node,
            Script::new(Stop::RefuseThenSuccess)
        )),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert!(memo(&db, ingredient, node).is_none());
}

#[test]
fn dropping_pending_body_refuses_and_releases_its_operation() {
    let db = DatabaseImpl::default();
    let node = Node::new(&db, None, 0);
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    assert_eq!(
        try_with_attempt(&db, 100, || run(
            &db,
            ingredient,
            node,
            Script::new(Stop::MissingChild)
        )),
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(
        try_with_attempt(&db, 100, || fixpoint(&db, node)),
        Ok(AttemptOutcome::Complete(1))
    );
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn callback_panic_unwinds_owned_stack_and_preserves_panic() {
    for phase in [Phase::Body, Phase::Recovery] {
        let mut db = DatabaseImpl::default();
        let node = Node::new(&db, None, 0);
        node.set_next(&mut db).to(Some(node));
        let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
        let result = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 100, || {
                run(&db, ingredient, node, Script::new(Stop::Panic(phase)))
            })
        }));
        assert!(result.is_err());
        assert!(db.zalsa_local().active_query().is_none());
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(memo(&db, ingredient, node).is_some_and(|memo| memo.value().is_none()));
    }
}

#[test]
fn selected_memo_completion_holds_policy_and_survives_later_refusal() {
    let db = DatabaseImpl::default();
    let node = Node::new(&db, None, 0);
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    assert_eq!(
        try_with_attempt(&db, 100, || run(
            &db,
            ingredient,
            node,
            Script::new(Stop::At(Phase::Complete, 0))
        )),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    let completed = memo(&db, ingredient, node).expect("body completed before its read epilogue");
    assert_eq!(completed.value(), Some(&1));
    assert!(!completed.header.may_be_provisional());
    assert_eq!(
        try_with_attempt(&db, 0, || fixpoint(&db, node)),
        Ok(AttemptOutcome::Complete(1))
    );
    assert_eq!(
        std::ptr::from_ref(completed),
        std::ptr::from_ref(memo(&db, ingredient, node).expect("memo retained"))
    );
}

#[test]
fn nested_driver_is_rejected_and_outer_driver_remains_usable() {
    let db = DatabaseImpl::default();
    assert_eq!(
        try_with_attempt(&db, 10, || Driver::run(&db, |_endpoint| async {
            assert_eq!(
                Driver::run(&db, |_endpoint| async { Ok(()) }),
                Err(RunError::Contract("nested execution driver"))
            );
            Ok(())
        })),
        Ok(AttemptOutcome::Complete(Ok(())))
    );
}

// A panic in a provider-owned destructor must still drain all ancestors deepest-first.
struct PanicOnDrop;
impl Drop for PanicOnDrop {
    fn drop(&mut self) {
        panic!("provider destructor panic");
    }
}
struct PendingWithPanic {
    _on_drop: PanicOnDrop,
}
impl Future for PendingWithPanic {
    type Output = RunResult<()>;
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn task_destructor_panic_still_drains_ancestors() {
    let db = DatabaseImpl::default();
    let node = Node::new(&db, None, 0);
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    let result = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(&db, 10, || {
            run(&db, ingredient, node, Script::new(Stop::DropPanic))
        })
    }));
    assert!(result.is_err());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert!(memo(&db, ingredient, node).is_some_and(|memo| memo.value().is_none()));
    assert_eq!(
        try_with_attempt(&db, 10, || Driver::run(&db, |_| async { Ok(()) })),
        Ok(AttemptOutcome::Complete(Ok(())))
    );
}

#[test]
fn participant_budget_refuses_before_the_reclaimed_body_callback() {
    let mut db = DatabaseImpl::default();
    let first = Node::new(&db, None, 0);
    let second = Node::new(&db, Some(first), 0);
    first.set_next(&mut db).to(Some(second));
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    let stamp = crate::prepared_source_probe::Stamp::current(&db);
    early_script_budget(
        &db,
        ingredient,
        first,
        second,
        Stop::At(Phase::Body, 3),
        [2, 0, 0, 0],
    );
    assert!(stamp.belongs_to(&db));
}

#[test]
fn reclaimed_cycle_participant_aborts_without_waiting() {
    for retry_second in [false, true] {
        let mut db = DatabaseImpl::default();
        let first = Node::new(&db, None, 0);
        let second = Node::new(&db, Some(first), 0);
        first.set_next(&mut db).to(Some(second));
        let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
        let first_key = ingredient.database_key_index(first.as_id());
        let second_key = ingredient.database_key_index(second.as_id());
        let stamp = crate::prepared_source_probe::Stamp::current(&db);
        // The second iteration reclaims the participant transferred to the first query.
        let script = Script::observed(Stop::At(Phase::Body, 3));
        let (outcome, trace) = ScriptTrace::collect(|| {
            try_with_attempt(&db, LIFECYCLE_FIXTURE_RESERVE, || {
                run(&db, ingredient, first, script.clone())
            })
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        let fault = script.fault.borrow();
        let fault = fault
            .as_ref()
            .expect("the reclaimed participant body was reached");
        fault.assert_owned(Phase::Body, second_key, [4, 0, 1, 1]);
        assert_eq!(fault.memo_value, Some(1));
        assert_eq!(fault.memo_provisional, Some(true));
        assert_eq!(fault.memo_heads, [first_key]);
        #[cfg(not(feature = "shuttle"))]
        {
            let sync = fault.sync.unwrap();
            assert!(sync.claimed_twice);
            assert!(matches!(sync.owner, crate::function::SyncOwner::Thread(_)));
            assert!(
                fault
                    .graph
                    .transferred
                    .entries
                    .iter()
                    .flatten()
                    .any(|(key, _, head)| (*key, *head) == (second_key, first_key))
            );
            assert!(fault.graph.edges.is_empty());
            assert!(!fault.graph.transferred.overflow && !fault.graph.edges.overflow);
        }
        trace.assert_reclaimed_fault(second_key, first_key);
        trace.assert_refusal_cleanup();
        assert_script_idle(&db, ingredient, [first, second]);
        for node in [first, second] {
            let memo =
                memo(&db, ingredient, node).expect("the productive cycle retained both seeds");
            assert!(memo.header.may_be_provisional());
            assert!(!memo.header.can_seed_attempt(db.zalsa()));
            assert!(memo.value().is_some());
        }
        let entry = if retry_second { second } else { first };
        assert_eq!(
            try_with_attempt(&db, LIFECYCLE_FIXTURE_RESERVE, || {
                assert_foreign_seeds(&db, ingredient, [first, second]);
                run(&db, ingredient, entry, Script::new(Stop::Never))
            }),
            Ok(AttemptOutcome::Complete(Ok(3)))
        );
        assert!(stamp.belongs_to(&db));
        assert_script_idle(&db, ingredient, [first, second]);
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn suspended_request_keeps_worker_and_outer_scope_registration() {
    let db = DatabaseImpl::default();
    let node = Node::new(&db, None, 0);
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    let script = Script::new(Stop::At(Phase::Body, 0));
    *script.worker.borrow_mut() = Some(db.clone());
    assert_eq!(
        try_with_attempt(&db, 100, || {
            attempt_probe::try_with_operation(&db, || {
                let before = attempt_probe::stack_depths();
                let registrations = db
                    .zalsa()
                    .attempt_operations
                    .load(crate::sync::atomic::Ordering::SeqCst);
                let outcome = run(&db, ingredient, node, script);
                assert_eq!(attempt_probe::stack_depths(), before);
                assert_eq!(
                    db.zalsa()
                        .attempt_operations
                        .load(crate::sync::atomic::Ordering::SeqCst),
                    registrations
                );
                outcome
            })
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(
        db.zalsa()
            .attempt_operations
            .load(crate::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn revision_cancellation_keeps_its_payload_and_releases_owned_state() {
    let mut db = DatabaseImpl::default();
    let node = Node::new(&db, None, 0);
    let cancelled = {
        let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
        crate::Cancelled::catch(AssertUnwindSafe(|| {
            try_with_attempt(&db, 100, || {
                run(&db, ingredient, node, Script::new(Stop::CancelRevision))
            })
        }))
    };
    db.zalsa().runtime().reset_cancellation_flag();
    assert!(matches!(cancelled, Err(crate::Cancelled::PendingWrite)));
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    node.set_seed(&mut db).to(1);
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    assert_eq!(
        try_with_attempt(&db, 100, || run(
            &db,
            ingredient,
            node,
            Script::new(Stop::Never)
        )),
        Ok(AttemptOutcome::Complete(Ok(1)))
    );
}

#[test]
fn borrowed_endpoint_cannot_start_work_after_its_driver_ends() {
    let db = DatabaseImpl::default();
    let endpoint = try_with_attempt(&db, 10, || {
        Driver::run(&db, |endpoint| async move { Ok(endpoint) })
    });
    let Ok(AttemptOutcome::Complete(Ok(endpoint))) = endpoint else {
        panic!("driver completed");
    };
    assert!(matches!(
        endpoint.demand(|| async { Ok(()) }),
        Err(RunError::Contract("execution driver has ended"))
    ));
    assert!(db.zalsa_local().active_query().is_none());
}
