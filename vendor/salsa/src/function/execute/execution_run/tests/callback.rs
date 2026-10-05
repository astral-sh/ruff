use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn, ready};
use std::marker::PhantomPinned;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use super::super::registration::{
    Demand, ExecutableRouteProvider, FinalSourceError, FinalSourceMemo, ProviderContext,
    RegistryBuilder, Route,
};
use super::super::{Driver, Endpoint, ExecutionAdmission, ExecutionWork, RunError, RunResult};
use super::{Node, fixpoint};
use crate::attempt_probe::{
    self, AttemptOutcome, Incomplete, MemoReuse, QueryPolicy, try_with_attempt,
};
use crate::function::{ClaimResult, Configuration, IngredientImpl, Reentrancy};
use crate::plumbing::AsId;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseImpl, DatabaseKeyIndex, Id, Setter};

#[crate::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

#[derive(Debug, Eq, PartialEq)]
struct Value {
    value: u32,
    observed: bool,
    _pin: PhantomPinned,
}

impl Drop for Value {
    fn drop(&mut self) {
        if self.observed {
            record("output");
        }
    }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn scalar(db: &dyn Database, input: Input) -> Value {
    Value {
        value: input.value(db),
        observed: false,
        _pin: PhantomPinned,
    }
}

#[derive(Debug)]
struct Event {
    stage: &'static str,
    frame: Option<DatabaseKeyIndex>,
    query_depth: usize,
    claim_held: bool,
    memo_present: bool,
    operation_depth: usize,
    reason: Option<Incomplete>,
    panicking: bool,
}

#[derive(Default)]
struct Observed {
    target: Option<DatabaseKeyIndex>,
    events: Vec<Event>,
    polls: usize,
    future_drops: usize,
    returned: bool,
}

thread_local! {
    static OBSERVED: RefCell<Observed> = RefCell::new(Observed::default());
}

fn record(stage: &'static str) {
    let target = OBSERVED.with_borrow(|observed| observed.target);
    let Some(target) = target else { return };
    let (frame, query_depth, claim_held, memo_present) = crate::with_attached_database(|db| {
        let claim_held = db
            .zalsa()
            .lookup_ingredient(target.ingredient_index())
            .as_function()
            .is_some_and(|function| {
                matches!(
                    function.sync_table().peek_claim(
                        db.zalsa(),
                        target.key_index(),
                        Reentrancy::Deny
                    ),
                    ClaimResult::Cycle { .. }
                )
            });
        (
            db.zalsa_local().active_query().map(|(key, _)| key),
            db.zalsa_local()
                .try_with_query_stack(|stack| stack.len())
                .unwrap(),
            claim_held,
            db.zalsa()
                .lookup_ingredient(target.ingredient_index())
                .as_function()
                .is_some_and(|function| function.memo(db.zalsa(), target.key_index()).is_some()),
        )
    })
    .expect("callback cleanup retains the attached database");
    OBSERVED.with_borrow_mut(|observed| {
        observed.events.push(Event {
            stage,
            frame,
            query_depth,
            claim_held,
            memo_present,
            operation_depth: attempt_probe::stack_depths().0,
            reason: attempt_probe::current().and_then(|support| support.reason()),
            panicking: crate::sync::thread::panicking(),
        })
    });
}

struct RecordDrop(&'static str);

impl Drop for RecordDrop {
    fn drop(&mut self) {
        record(self.0);
    }
}

struct ClearObservation;

impl Drop for ClearObservation {
    fn drop(&mut self) {
        OBSERVED.with_borrow_mut(|observed| observed.target = None);
    }
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    ConstructorPanic,
    PollError(RunError),
    PollPanic,
    TerminalThenPanic,
    CheckpointReady,
    DropChild,
    DropRefuse,
    DropPanic,
    PostAdmissionRefuse,
    #[cfg(not(feature = "shuttle"))]
    LocalCancellation,
    #[cfg(not(feature = "shuttle"))]
    PendingWriteCancellation,
}

#[derive(Debug)]
struct PanicMarker {
    identity: Arc<()>,
    phase: &'static str,
}

fn queue_child(endpoint: &Endpoint<'_, '_>) -> RunResult<()> {
    let child = RecordDrop("child");
    let _reply = endpoint.demand(move || {
        poll_fn(move |_| -> Poll<RunResult<()>> {
            let _child = &child;
            panic!("rejected callback child must not run");
        })
    })?;
    Ok(())
}

struct CallbackFuture<'run, 'db: 'run> {
    endpoint: Endpoint<'run, 'db>,
    fault: Fault,
    identity: Arc<()>,
    returned: Cell<bool>,
    _pin: PhantomPinned,
}

impl Future for CallbackFuture<'_, '_> {
    type Output = RunResult<Value>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_ref().get_ref();
        OBSERVED.with_borrow_mut(|observed| observed.polls += 1);
        match this.fault {
            Fault::PollError(error) => {
                queue_child(&this.endpoint)?;
                return Poll::Ready(Err(error));
            }
            Fault::PollPanic | Fault::TerminalThenPanic => {
                queue_child(&this.endpoint)?;
                if matches!(this.fault, Fault::TerminalThenPanic) {
                    let mut terminal = this
                        .endpoint
                        .suspend_error(RunError::Refused(Incomplete::Allowance))?;
                    assert!(Pin::new(&mut terminal).poll(cx).is_pending());
                }
                panic_any(PanicMarker {
                    identity: this.identity.clone(),
                    phase: "poll",
                });
            }
            Fault::CheckpointReady => {
                let mut checkpoint = this.endpoint.checkpoint()?;
                assert!(Pin::new(&mut checkpoint).poll(cx).is_pending());
            }
            #[cfg(not(feature = "shuttle"))]
            Fault::LocalCancellation | Fault::PendingWriteCancellation => {
                queue_child(&this.endpoint)?;
                if matches!(this.fault, Fault::LocalCancellation) {
                    this.endpoint.context.db.cancellation_token().cancel();
                } else {
                    this.endpoint
                        .context
                        .db
                        .zalsa()
                        .runtime()
                        .set_cancellation_flag();
                }
                this.endpoint.context.db.unwind_if_revision_cancelled();
                panic!("cancelled callback unexpectedly resumed");
            }
            _ => {}
        }
        assert!(!this.returned.replace(true));
        OBSERVED.with_borrow_mut(|observed| observed.returned = true);
        Poll::Ready(Ok(Value {
            value: 17,
            observed: true,
            _pin: PhantomPinned,
        }))
    }
}

impl Drop for CallbackFuture<'_, '_> {
    fn drop(&mut self) {
        OBSERVED.with_borrow_mut(|observed| observed.future_drops += 1);
        record("future");
        if !self.returned.get() {
            return;
        }
        match self.fault {
            Fault::DropChild => {
                queue_child(&self.endpoint).expect("callback retirement retains its poll")
            }
            Fault::DropRefuse => {
                attempt_probe::report_incomplete(self.endpoint.context.db, Incomplete::Allowance);
            }
            Fault::DropPanic => {
                queue_child(&self.endpoint).expect("callback retirement retains its poll");
                panic_any(PanicMarker {
                    identity: self.identity.clone(),
                    phase: "drop",
                });
            }
            _ => {}
        }
    }
}

struct Providers {
    fault: Fault,
    identity: Arc<()>,
}

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for Providers
where
    C: Configuration<DbView = dyn Database, Input<'db> = Input, Output<'db> = Value>,
{
    // Input conversion constructs a handle; Value equality checks u32, bool and PhantomPinned.
    fixture_native_value!(executable, 'run, 'db, C, 3);

    fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _input: Input,
    ) -> impl Future<Output = RunResult<Value>> + 'run {
        let endpoint = context.endpoint().inner.clone();
        if matches!(self.fault, Fault::ConstructorPanic) {
            queue_child(&endpoint).expect("callback construction retains its poll");
            panic_any(PanicMarker {
                identity: self.identity.clone(),
                phase: "constructor",
            });
        }
        CallbackFuture {
            endpoint,
            fault: self.fault,
            identity: self.identity.clone(),
            returned: Cell::new(false),
            _pin: PhantomPinned,
        }
    }

    fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _id: Id,
        _input: Input,
    ) -> impl Future<Output = RunResult<Value>> + 'run {
        ready(Err(RunError::Contract(
            "acyclic callback requested initialization",
        )))
    }

    fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call Value,
        _value: Value,
        _input: Input,
    ) -> impl Future<Output = RunResult<Value>> + 'call
    where
        'run: 'call,
    {
        ready(Err(RunError::Contract(
            "acyclic callback requested recovery",
        )))
    }
}

struct Admission {
    refuse_after_return: bool,
}

impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if self.refuse_after_return
            && matches!(work, ExecutionWork::Work { .. })
            && OBSERVED.with_borrow(|observed| observed.returned)
        {
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        Ok(())
    }
}

fn run<'db>(
    db: &'db DatabaseImpl,
    input: Input,
    fault: Fault,
    identity: Arc<()>,
    escaped: &RefCell<Option<Demand<&'db Value>>>,
) -> RunResult<u32> {
    let admission = Admission {
        refuse_after_return: matches!(fault, Fault::PostAdmissionRefuse),
    };
    let providers = Providers { fault, identity };
    let mut registry = RegistryBuilder::new(db, &admission)?;
    let route = registry.reserve(db as &dyn Database, scalar::fn_ingredient_(db, db.zalsa()))?;
    let binding = registry.provider(&providers)?;
    registry.bind_executable(&route, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        let _root = RecordDrop("root");
        *escaped.borrow_mut() = Some(
            endpoint
                .provider(binding)?
                .fetch_ref(&route, input.as_id())?,
        );
        let value = poll_fn(|cx| {
            let mut escaped = escaped.borrow_mut();
            Pin::new(escaped.as_mut().expect("the query demand has escaped")).poll(cx)
        })
        .await?;
        Ok(value.value)
    })
}

fn start_observation(db: &DatabaseImpl, input: Input) -> (DatabaseKeyIndex, ClearObservation) {
    let key = scalar::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    OBSERVED.with_borrow_mut(|observed| {
        *observed = Observed {
            target: Some(key),
            ..Observed::default()
        }
    });
    (key, ClearObservation)
}

fn assert_owners(key: DatabaseKeyIndex, stages: &[&str], reason: Option<Incomplete>) {
    OBSERVED.with_borrow(|observed| {
        assert_eq!(
            observed
                .events
                .iter()
                .map(|event| event.stage)
                .collect::<Vec<_>>(),
            stages
        );
        for event in &observed.events {
            let expected_reason = if event.stage == "future" && stages.first() == Some(&"future") {
                // Retirement runs before its destructor or the post-callback admission refuses.
                None
            } else {
                reason
            };
            assert_eq!(event.reason, expected_reason, "{event:?}");
            if event.stage == "root" {
                assert_eq!(event.frame, None);
                assert!(!event.claim_held);
                assert_eq!(event.operation_depth, 0);
            } else {
                assert_eq!(event.frame, Some(key), "{event:?}");
                assert!(event.claim_held, "{event:?}");
                assert_eq!(event.operation_depth, 1);
            }
        }
    });
}

fn assert_unpublished(escaped: &RefCell<Option<Demand<&Value>>>) {
    let mut escaped = escaped.borrow_mut();
    let reply = escaped.as_mut().expect("the query demand has escaped");
    assert!(!matches!(
        Pin::new(reply).poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(_))
    ));
}

#[test]
fn callback_errors_retain_children_output_and_query_ownership() {
    for (fault, stages, reason) in [
        (
            Fault::PollError(RunError::Contract("exact callback error")),
            &["child", "future", "root"][..],
            Incomplete::Interrupted,
        ),
        (
            Fault::PollError(RunError::RequiresFetch),
            &["child", "future", "root"][..],
            Incomplete::Interrupted,
        ),
        (
            Fault::PollError(RunError::Refused(Incomplete::Allowance)),
            &["child", "future", "root"][..],
            Incomplete::Allowance,
        ),
        (
            Fault::DropChild,
            &["future", "child", "output", "root"][..],
            Incomplete::Interrupted,
        ),
        (
            Fault::DropRefuse,
            &["future", "output", "root"][..],
            Incomplete::Allowance,
        ),
        (
            Fault::PostAdmissionRefuse,
            &["future", "output", "root"][..],
            Incomplete::Allowance,
        ),
    ] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7);
        let stamp = crate::prepared_source_probe::Stamp::current(&db);
        for _ in 0..2 {
            let (key, _clear) = start_observation(&db, input);
            let escaped = RefCell::new(None);
            let outcome = try_with_attempt(&db, 100_000, || {
                let result = run(&db, input, fault, Arc::new(()), &escaped);
                if let Fault::PollError(error) = fault {
                    assert_eq!(result, Err(error));
                }
                assert!(result.is_err());
            });
            assert_eq!(outcome, Ok(AttemptOutcome::Incomplete(reason)));
            assert_unpublished(&escaped);
            assert_owners(key, stages, Some(reason));
            OBSERVED.with_borrow(|observed| {
                assert_eq!(observed.polls, 1);
                assert_eq!(observed.future_drops, 1);
                assert!(observed.events.iter().all(|event| !event.panicking));
            });
            assert_eq!(attempt_probe::stack_depths(), (0, 0));
            assert!(db.zalsa_local().active_query().is_none());
            assert!(stamp.belongs_to(&db));
        }
        assert_eq!(scalar(&db, input).value, 7);
    }
}

#[test]
fn malformed_callback_ready_cannot_publish_a_reusable_memo() {
    let db = DatabaseImpl::default();
    let input = Input::new(&db, 7);
    let (key, _clear) = start_observation(&db, input);
    let stamp = crate::prepared_source_probe::Stamp::current(&db);
    let escaped = RefCell::new(None);
    let outcome = try_with_attempt(&db, 100_000, || {
        run(&db, input, Fault::CheckpointReady, Arc::new(()), &escaped)
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    assert_unpublished(&escaped);
    assert_owners(
        key,
        &["output", "future", "root"],
        Some(Incomplete::Interrupted),
    );
    assert_eq!(scalar(&db, input).value, 7);
    assert!(stamp.belongs_to(&db));
}

#[test]
fn native_callback_panics_retain_the_query_until_queued_children_drop() {
    for (fault, phase, stages, reason) in [
        (
            Fault::ConstructorPanic,
            "constructor",
            &["child", "root"][..],
            None,
        ),
        (
            Fault::PollPanic,
            "poll",
            &["child", "future", "root"][..],
            None,
        ),
        (
            Fault::TerminalThenPanic,
            "poll",
            &["child", "future", "root"][..],
            Some(Incomplete::Allowance),
        ),
        (
            Fault::DropPanic,
            "drop",
            &["future", "child", "output", "root"][..],
            None,
        ),
    ] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7);
        let (key, _clear) = start_observation(&db, input);
        let escaped = RefCell::new(None);
        let identity = Arc::new(());
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 100_000, || {
                run(&db, input, fault, identity.clone(), &escaped)
            })
        }));
        let payload = outcome.expect_err("the original native callback panic escapes the driver");
        let marker = payload
            .downcast_ref::<PanicMarker>()
            .expect("the original panic type is preserved");
        assert!(Arc::ptr_eq(&marker.identity, &identity));
        assert_eq!(marker.phase, phase);
        assert_unpublished(&escaped);
        assert_owners(key, stages, reason);
        OBSERVED.with_borrow(|observed| {
            let constructed = !matches!(fault, Fault::ConstructorPanic);
            assert_eq!(observed.polls, usize::from(constructed));
            assert_eq!(observed.future_drops, usize::from(constructed));
            assert!(
                observed
                    .events
                    .iter()
                    .filter(|event| event.stage != "future" || !matches!(fault, Fault::DropPanic))
                    .all(|event| event.panicking)
            );
        });
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert_eq!(scalar(&db, input).value, 7);
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn callback_local_and_revision_cancellation_keep_native_payloads() {
    for fault in [Fault::LocalCancellation, Fault::PendingWriteCancellation] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7);
        let (key, _clear) = start_observation(&db, input);
        let escaped = RefCell::new(None);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 100_000, || {
                run(&db, input, fault, Arc::new(()), &escaped)
            })
        }));
        db.zalsa_local().uncancel();
        db.zalsa().runtime().reset_cancellation_flag();
        let payload = outcome.expect_err("cancellation escapes through native panic transport");
        assert!(matches!(
            (fault, payload.downcast_ref::<crate::Cancelled>()),
            (Fault::LocalCancellation, Some(crate::Cancelled::Local))
                | (
                    Fault::PendingWriteCancellation,
                    Some(crate::Cancelled::PendingWrite)
                )
        ));
        assert_unpublished(&escaped);
        assert_owners(
            key,
            &["child", "future", "root"],
            Some(Incomplete::Interrupted),
        );
        OBSERVED
            .with_borrow(|observed| assert!(observed.events.iter().all(|event| !event.panicking)));
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert_eq!(scalar(&db, input).value, 7);
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn direct_poll_cancellation_drains_retained_tasks_outside_unwinding() {
    for fault in [Fault::LocalCancellation, Fault::PendingWriteCancellation] {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7);
        let (_key, _clear) = start_observation(&db, input);
        let stamp = crate::prepared_source_probe::Stamp::current(&db);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            crate::attach(&db, || {
                try_with_attempt(&db, 100_000, || {
                    Driver::run(&db, |endpoint| CallbackFuture {
                        endpoint,
                        fault,
                        identity: Arc::new(()),
                        returned: Cell::new(false),
                        _pin: PhantomPinned,
                    })
                })
            })
        }));
        db.zalsa_local().uncancel();
        db.zalsa().runtime().reset_cancellation_flag();
        let payload = outcome.expect_err("direct task polling preserves native cancellation");
        assert!(matches!(
            (fault, payload.downcast_ref::<crate::Cancelled>()),
            (Fault::LocalCancellation, Some(crate::Cancelled::Local))
                | (
                    Fault::PendingWriteCancellation,
                    Some(crate::Cancelled::PendingWrite)
                )
        ));
        OBSERVED.with_borrow(|observed| {
            assert_eq!(observed.polls, 1);
            assert_eq!(observed.future_drops, 1);
            assert_eq!(
                observed
                    .events
                    .iter()
                    .map(|event| event.stage)
                    .collect::<Vec<_>>(),
                ["child", "future"]
            );
            for event in &observed.events {
                assert!(!event.panicking);
                assert_eq!(event.reason, Some(Incomplete::Interrupted));
                assert_eq!(event.frame, None);
                assert_eq!(event.operation_depth, 0);
            }
        });
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert!(stamp.belongs_to(&db));
        assert_eq!(
            try_with_attempt(&db, 100_000, || Driver::run(&db, |_| ready(Ok(7)))),
            Ok(AttemptOutcome::Complete(Ok(7)))
        );
    }
}

struct BorrowedRecovery<'db, C: Configuration> {
    route: Route<'db, C>,
    recoveries: Cell<usize>,
}

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for BorrowedRecovery<'db, C>
where
    C: Configuration<DbView = dyn Database, Input<'db> = Node, Output<'db> = u32>,
{
    // Node conversion constructs a handle; output equality compares u32.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        node: Node,
    ) -> RunResult<u32> {
        let value = if let Some(next) = node.next(db) {
            *context.fetch_ref(&self.route, next.as_id())?.await?
        } else {
            0
        };
        Ok((value + 1).min(3))
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        _id: Id,
        node: Node,
    ) -> RunResult<u32> {
        Ok(node.seed(db))
    }

    fn recover<'call>(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        cycle: &'call Cycle<'call>,
        last: &'call u32,
        value: u32,
        _node: Node,
    ) -> impl Future<Output = RunResult<u32>> + 'call
    where
        'run: 'call,
    {
        async move {
            let old = *last;
            let cycle_address = std::ptr::from_ref(cycle);
            let last_address = std::ptr::from_ref(last);
            context.endpoint().checkpoint()?.await?;
            assert_eq!(*last, old);
            assert_eq!(std::ptr::from_ref(cycle), cycle_address);
            assert_eq!(std::ptr::from_ref(last), last_address);
            assert_eq!(attempt_probe::current_policy(), QueryPolicy::ReturnOnly);
            self.recoveries.set(self.recoveries.get() + 1);
            Ok(value)
        }
    }
}

#[test]
fn recovery_borrows_its_cycle_and_previous_value_across_local_suspension() {
    let mut db = DatabaseImpl::default();
    let node = Node::new(&db, None, 0);
    node.set_next(&mut db).to(Some(node));
    let outcome = try_with_attempt(&db, 100_000, || {
        let admission = Admission {
            refuse_after_return: false,
        };
        let providers;
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        let route = registry.reserve(
            &db as &dyn Database,
            fixpoint::fn_ingredient_(&db, db.zalsa()),
        )?;
        providers = BorrowedRecovery {
            route: route.clone(),
            recoveries: Cell::new(0),
        };
        let binding = registry.provider(&providers)?;
        registry.bind_executable(&route, &binding)?;
        let result = registry.seal()?.run(move |endpoint| async move {
            Ok(*endpoint
                .provider(binding)?
                .fetch_ref(&route, node.as_id())?
                .await?)
        });
        assert!(providers.recoveries.get() > 0);
        result
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(3))));
}

#[derive(Debug)]
struct CycleValue {
    value: u32,
    observed: bool,
}

impl PartialEq for CycleValue {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for CycleValue {}

impl Drop for CycleValue {
    fn drop(&mut self) {
        if self.observed {
            record("operand");
        }
    }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_result = cycle_initial)]
fn cycle_fallback(db: &dyn Database, node: Node) -> CycleValue {
    CycleValue {
        value: node
            .next(db)
            .map_or(0, |next| cycle_fallback(db, next).value)
            + 1,
        observed: false,
    }
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = cycle_initial, cycle_fn = cycle_recover)]
fn cycle_fixpoint(db: &dyn Database, node: Node) -> CycleValue {
    CycleValue {
        value: (node
            .next(db)
            .map_or(0, |next| cycle_fixpoint(db, next).value)
            + 1)
        .min(3),
        observed: false,
    }
}

fn cycle_initial(db: &dyn Database, _id: Id, node: Node) -> CycleValue {
    CYCLE.with_borrow_mut(|state| state.ordinary_initials += 1);
    CycleValue {
        value: node.seed(db),
        observed: false,
    }
}

fn cycle_recover(
    _db: &dyn Database,
    _cycle: &Cycle<'_>,
    _last: &CycleValue,
    value: CycleValue,
    _node: Node,
) -> CycleValue {
    value
}

#[derive(Default)]
struct CycleState {
    ordinary_initials: usize,
    initial_calls: usize,
    recovery_calls: usize,
    body_returned: bool,
    post_body_work: usize,
}

thread_local! {
    static CYCLE: RefCell<CycleState> = RefCell::new(CycleState::default());
}

fn assert_cycle_boundary(checkpoint: &str, operation_depth: usize) {
    assert_eq!(
        attempt_probe::stack_depths().0,
        operation_depth,
        "{checkpoint}"
    );
    assert_eq!(attempt_probe::current_policy(), QueryPolicy::ReturnOnly);
    assert_eq!(
        attempt_probe::current().and_then(|support| support.reason()),
        None
    );
    crate::with_attached_database(|db| {
        let key = OBSERVED.with_borrow(|observed| observed.target.unwrap());
        assert_eq!(
            db.zalsa_local().active_query().map(|(key, _)| key),
            Some(key),
            "{checkpoint} retains the real query frame"
        );
        assert!(
            matches!(
                db.zalsa()
                    .lookup_ingredient(key.ingredient_index())
                    .as_function()
                    .unwrap()
                    .sync_table()
                    .peek_claim(db.zalsa(), key.key_index(), Reentrancy::Deny),
                ClaimResult::Cycle { .. }
            ),
            "{checkpoint} retains the query claim"
        );
    })
    .expect("the callback boundary has an attached database");
}

struct CycleAdmission {
    refuse_recovery: bool,
}

impl ExecutionAdmission for CycleAdmission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if self.refuse_recovery && matches!(work, ExecutionWork::Work { .. }) {
            let refuse = CYCLE.with_borrow_mut(|state| {
                if !state.body_returned {
                    return false;
                }
                state.post_body_work += 1;
                state.post_body_work == 2
            });
            if refuse {
                assert_cycle_boundary("Recovery pre-admission before provider construction", 1);
                CYCLE.with_borrow(|state| assert_eq!(state.recovery_calls, 0));
                return Err(RunError::Refused(Incomplete::Allowance));
            }
        }
        Ok(())
    }
}

struct CycleProviders<'db, C: Configuration> {
    ingredient: &'db IngredientImpl<C>,
}

impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for CycleProviders<'db, C>
where
    C: Configuration<DbView = dyn Database, Input<'db> = Node, Output<'db> = CycleValue>,
{
    // Node conversion constructs a handle; CycleValue equality compares its u32.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn Database,
        node: Node,
    ) -> RunResult<CycleValue> {
        // The reentrant seed read selects a genuine execution Initial or Recovery step.
        let seed = self
            .ingredient
            .fetch(db, db.zalsa(), db.zalsa_local(), node.as_id())
            .value;
        CYCLE.with_borrow_mut(|state| state.body_returned = true);
        Ok(CycleValue {
            value: seed + 1,
            observed: true,
        })
    }

    fn initial(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _id: Id,
        _node: Node,
    ) -> impl Future<Output = RunResult<CycleValue>> + 'run {
        assert_cycle_boundary("Execution Initial provider construction", 1);
        CYCLE.with_borrow_mut(|state| state.initial_calls += 1);
        queue_child(&context.endpoint().inner).expect("Initial retains the active task");
        ready(Err(RunError::Contract("execution Initial rejected")))
    }

    fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call CycleValue,
        value: CycleValue,
        _node: Node,
    ) -> impl Future<Output = RunResult<CycleValue>> + 'call
    where
        'run: 'call,
    {
        CYCLE.with_borrow_mut(|state| state.recovery_calls += 1);
        ready(Ok(value))
    }
}

fn reject_cycle_callback<C>(
    db: &DatabaseImpl,
    node: Node,
    ingredient: &IngredientImpl<C>,
    recovery: bool,
) where
    C: for<'db> Configuration<DbView = dyn Database, Input<'db> = Node, Output<'db> = CycleValue>,
{
    let key = ingredient.database_key_index(node.as_id());
    let stamp = crate::prepared_source_probe::Stamp::current(db);
    let _clear = ClearObservation;
    for _ in 0..2 {
        CYCLE.with_borrow_mut(|state| *state = CycleState::default());
        OBSERVED.with_borrow_mut(|observed| {
            *observed = Observed {
                target: Some(key),
                ..Observed::default()
            }
        });
        let outcome = try_with_attempt(db, 100_000, || {
            let index = ingredient.memo_ingredient_index(db.zalsa(), node.as_id());
            if let Some(previous) =
                ingredient.get_memo_from_table_for(db.zalsa(), node.as_id(), index)
            {
                assert_eq!(previous.header.attempt_reuse(db.zalsa()), MemoReuse::Stale);
                assert!(!previous.header.can_seed_attempt(db.zalsa()));
            }
            let admission = CycleAdmission {
                refuse_recovery: recovery,
            };
            let providers = CycleProviders { ingredient };
            let mut registry = RegistryBuilder::new(db, &admission)?;
            let route = registry.reserve(db as &dyn Database, ingredient)?;
            let binding = registry.provider(&providers)?;
            registry.bind_executable(&route, &binding)?;
            let result = registry.seal()?.run(move |endpoint| async move {
                let _root = RecordDrop("root");
                Ok(endpoint
                    .provider(binding)?
                    .fetch_ref(&route, node.as_id())?
                    .await?
                    .value)
            });
            let expected = if recovery {
                RunError::Refused(Incomplete::Allowance)
            } else {
                RunError::Contract("execution Initial rejected")
            };
            assert_eq!(result, Err(expected));
            result
        });
        let reason = if recovery {
            Incomplete::Allowance
        } else {
            Incomplete::Interrupted
        };
        assert_eq!(outcome, Ok(AttemptOutcome::Incomplete(reason)));
        assert_owners(
            key,
            if recovery {
                &["operand", "root"]
            } else {
                &["child", "operand", "root"]
            },
            Some(reason),
        );
        CYCLE.with_borrow(|state| {
            assert_eq!(
                state.ordinary_initials, 1,
                "the synchronous reentrant seed is separate from execution Initial"
            );
            assert_eq!(state.initial_calls, usize::from(!recovery));
            assert_eq!(
                state.recovery_calls, 0,
                "Recovery pre-admission must precede provider construction"
            );
            assert_eq!(state.post_body_work, if recovery { 2 } else { 0 });
        });
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        let index = ingredient.memo_ingredient_index(db.zalsa(), node.as_id());
        let provisional = ingredient
            .get_memo_from_table_for(db.zalsa(), node.as_id(), index)
            .expect("the reentrant cycle read inserted its provisional seed");
        // Expected refusal keeps the seed allocation, but its attempt cannot supply a later run.
        assert!(provisional.value().is_some());
        assert!(provisional.header.may_be_provisional());
        assert_eq!(
            FinalSourceMemo::certify(db as &dyn Database, ingredient, node.as_id()).unwrap_err(),
            FinalSourceError::ProvisionalMemo,
        );
        assert_eq!(
            provisional
                .header
                .revisions
                .attempt_support()
                .and_then(|support| support.reason()),
            Some(reason),
        );
        assert_eq!(
            provisional.header.attempt_reuse(db.zalsa()),
            MemoReuse::Stale
        );
        assert!(!provisional.header.can_seed_attempt(db.zalsa()));
        assert!(stamp.belongs_to(db));
    }
}

#[test]
fn execution_initial_rejection_retains_computed_operand_and_query_owner() {
    let mut db = DatabaseImpl::default();
    let node = Node::new(&db, None, 0);
    node.set_next(&mut db).to(Some(node));
    reject_cycle_callback(
        &db,
        node,
        cycle_fallback::fn_ingredient_(&db, db.zalsa()),
        false,
    );
}

#[test]
fn recovery_pre_admission_retains_operand_before_provider_construction() {
    let mut db = DatabaseImpl::default();
    let node = Node::new(&db, None, 0);
    node.set_next(&mut db).to(Some(node));
    reject_cycle_callback(
        &db,
        node,
        cycle_fixpoint::fn_ingredient_(&db, db.zalsa()),
        true,
    );
}

#[test]
fn first_native_payload_survives_a_second_receipt_in_the_same_poll() {
    let db = DatabaseImpl::default();
    let first = Arc::new(());
    let second = Arc::new(());
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(&db, 100, || {
            let first = &first;
            let second = &second;
            Driver::run(&db, move |endpoint| async move {
                let receipt = endpoint
                    .callback_poll()
                    .expect("the root task owns its poll");
                receipt.panic(Box::new(PanicMarker {
                    identity: first.clone(),
                    phase: "first",
                }));
                receipt.panic(Box::new(PanicMarker {
                    identity: second.clone(),
                    phase: "second",
                }));
                Ok(())
            })
        })
    }));
    let payload = outcome.expect_err("the terminal native payload suppresses Ready");
    let marker = payload
        .downcast_ref::<PanicMarker>()
        .expect("native payload type is unchanged");
    assert!(Arc::ptr_eq(&marker.identity, &first));
    assert!(!Arc::ptr_eq(&marker.identity, &second));
    assert_eq!(marker.phase, "first");
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
}

#[crate::db]
#[derive(Clone)]
struct ColdDb {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl Database for ColdDb {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum ColdStop {
    #[default]
    Caller,
    Storage,
    PublicationWork,
}

#[derive(Default)]
struct ColdState {
    endpoint: Option<Endpoint<'static, 'static>>,
    stop: ColdStop,
    checks_left: usize,
    post_initial_work: bool,
    storage_seen: bool,
    publication_work_seen: bool,
    fired: bool,
    initial_calls: usize,
}

thread_local! {
    static COLD: RefCell<ColdState> = RefCell::new(ColdState::default());
}

struct ClearCold;

impl Drop for ClearCold {
    fn drop(&mut self) {
        COLD.with_borrow_mut(|state| *state = ColdState::default());
    }
}

struct ColdAdmission;

impl ExecutionAdmission for ColdAdmission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if matches!(work, ExecutionWork::Work { .. }) {
            let arm =
                COLD.with_borrow(|state| state.endpoint.is_some() && !state.post_initial_work);
            if arm {
                assert_cycle_boundary("ColdInitial post-return Work", 2);
                COLD.with_borrow_mut(|state| {
                    state.post_initial_work = true;
                    // Successful admission observes its result before the original Caller checks.
                    if state.stop == ColdStop::Caller {
                        state.checks_left = 2;
                    }
                });
            }
        }
        let endpoint = COLD.with_borrow_mut(|state| {
            if !state.post_initial_work || state.endpoint.is_none() || state.fired {
                return None;
            }
            let reject = match work {
                ExecutionWork::Resource { .. } => {
                    state.storage_seen = true;
                    state.stop == ColdStop::Storage
                }
                ExecutionWork::Work { units } if state.storage_seen => {
                    state.publication_work_seen = true;
                    assert_eq!(
                        units, 1,
                        "cold publication installs one previously empty slot"
                    );
                    state.stop == ColdStop::PublicationWork
                }
                _ => false,
            };
            if reject {
                state.fired = true;
                state.endpoint.take()
            } else {
                None
            }
        });
        if let Some(endpoint) = endpoint {
            assert_cycle_boundary("ColdInitial prepared insertion admission", 2);
            record("admission");
            // Demand can invoke cancellation callbacks; the endpoint is no longer armed in TLS.
            queue_child(&endpoint)?;
            return Err(RunError::Refused(Incomplete::Allowance));
        }
        Ok(())
    }
}

static COLD_ADMISSION: ColdAdmission = ColdAdmission;

struct ColdProviders<C: Configuration> {
    route: Route<'static, C>,
}

impl<C> ExecutableRouteProvider<'static, 'static, C> for ColdProviders<C>
where
    C: Configuration<DbView = dyn Database, Input<'static> = Node, Output<'static> = CycleValue>,
{
    // Node conversion constructs a handle; CycleValue equality compares its u32.
    fixture_native_value!(executable, 'static, 'static, C, 1);

    async fn body(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        node: Node,
    ) -> RunResult<CycleValue> {
        let value = context.fetch_ref(&self.route, node.as_id())?.await?;
        Ok(CycleValue {
            value: value.value + 1,
            observed: true,
        })
    }

    fn initial(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        db: &'static dyn Database,
        _id: Id,
        node: Node,
    ) -> impl Future<Output = RunResult<CycleValue>> + 'static {
        assert_cycle_boundary("ColdInitial provider construction", 2);
        COLD.with_borrow_mut(|state| {
            state.initial_calls += 1;
            assert!(
                state
                    .endpoint
                    .replace(context.endpoint().inner.clone())
                    .is_none()
            );
        });
        ready(Ok(CycleValue {
            value: node.seed(db),
            observed: true,
        }))
    }

    fn recover<'call>(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call CycleValue,
        value: CycleValue,
        _node: Node,
    ) -> impl Future<Output = RunResult<CycleValue>> + 'call
    where
        'static: 'call,
    {
        ready(Ok(value))
    }
}

fn reject_cold_caller<C>(
    db: &'static ColdDb,
    ingredient: &'static IngredientImpl<C>,
    node: Node,
) -> RunResult<u32>
where
    C: Configuration<DbView = dyn Database, Input<'static> = Node, Output<'static> = CycleValue>,
{
    let mut registry = RegistryBuilder::new(db, &COLD_ADMISSION)?;
    let route = registry.reserve(db as &dyn Database, ingredient)?;
    let providers: &'static _ = Box::leak(Box::new(ColdProviders {
        route: route.clone(),
    }));
    let binding = registry.provider(providers)?;
    registry.bind_executable(&route, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        let _root = RecordDrop("root");
        Ok(endpoint
            .provider(binding)?
            .fetch_ref(&route, node.as_id())?
            .await?
            .value)
    })
}

#[test]
fn cold_initial_caller_check_cannot_publish_after_queuing_a_child() {
    let _clear = (ClearCold, ClearObservation);
    let mut db = ColdDb {
        storage: crate::Storage::new(Some(Box::new(|event| {
            if !matches!(event.kind, crate::EventKind::WillCheckCancellation) {
                return;
            }
            let endpoint = COLD.with_borrow_mut(|state| {
                if state.checks_left == 0 {
                    return None;
                }
                state.checks_left -= 1;
                if state.checks_left != 0 {
                    return None;
                }
                assert!(state.post_initial_work);
                state.fired = true;
                state.endpoint.take()
            });
            if let Some(endpoint) = endpoint {
                assert_cycle_boundary(
                    "ColdInitial original Caller check after post-return Work",
                    2,
                );
                crate::with_attached_database(|db| {
                    assert_eq!(
                        db.zalsa_local().try_with_query_stack(|stack| stack.len()),
                        Some(1)
                    );
                });
                // The endpoint is removed from TLS before demand performs cancellation checks.
                queue_child(&endpoint).expect("the original Caller check retains the active poll");
            }
        }))),
    };
    let node = Node::new(&db, None, 0);
    node.set_next(&mut db).to(Some(node));
    // The static event hook keeps only an endpoint borrowing this dedicated static fixture.
    let db: &'static ColdDb = Box::leak(Box::new(db));
    let ingredient = cycle_fixpoint::fn_ingredient_(db, db.zalsa());
    let key = ingredient.database_key_index(node.as_id());
    for _ in 0..2 {
        COLD.with_borrow_mut(|state| *state = ColdState::default());
        OBSERVED.with_borrow_mut(|observed| {
            *observed = Observed {
                target: Some(key),
                ..Observed::default()
            }
        });
        let outcome = try_with_attempt(db, 100_000, || reject_cold_caller(db, ingredient, node));
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        COLD.with_borrow(|state| {
            assert!(state.post_initial_work && state.fired);
            assert_eq!(state.initial_calls, 1);
            assert_eq!(state.checks_left, 0);
            assert!(state.endpoint.is_none());
        });
        OBSERVED.with_borrow(|observed| {
            assert_eq!(
                observed
                    .events
                    .iter()
                    .map(|event| event.stage)
                    .collect::<Vec<_>>(),
                ["child", "operand", "root"]
            );
            for event in &observed.events {
                assert_eq!(event.reason, Some(Incomplete::Interrupted));
                assert!(!event.panicking);
                if event.stage == "root" {
                    assert_eq!(event.frame, None);
                    assert!(!event.claim_held);
                    assert_eq!(event.operation_depth, 0);
                } else {
                    assert_eq!(event.frame, Some(key));
                    assert!(event.claim_held);
                    assert_eq!(event.operation_depth, 2);
                }
            }
        });
        let index = ingredient.memo_ingredient_index(db.zalsa(), node.as_id());
        assert!(
            ingredient
                .get_memo_from_table_for(db.zalsa(), node.as_id(), index)
                .is_none_or(|memo| memo.value().is_none())
        );
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
    }
}

#[test]
fn cold_prepared_insertion_admissions_retain_operand_and_original_caller() {
    let _clear = (ClearCold, ClearObservation);
    for stop in [ColdStop::Storage, ColdStop::PublicationWork] {
        let mut db = ColdDb {
            storage: crate::Storage::new(None),
        };
        let node = Node::new(&db, None, 0);
        node.set_next(&mut db).to(Some(node));
        let db: &'static ColdDb = Box::leak(Box::new(db));
        let ingredient = cycle_fixpoint::fn_ingredient_(db, db.zalsa());
        let key = ingredient.database_key_index(node.as_id());
        let stamp = crate::prepared_source_probe::Stamp::current(db);
        COLD.with_borrow_mut(|state| {
            *state = ColdState {
                stop,
                ..ColdState::default()
            }
        });
        OBSERVED.with_borrow_mut(|observed| {
            *observed = Observed {
                target: Some(key),
                ..Observed::default()
            }
        });
        let outcome = try_with_attempt(db, 100_000, || {
            let direct = reject_cold_caller(db, ingredient, node);
            assert_eq!(
                direct,
                Err(RunError::Refused(Incomplete::Allowance)),
                "{stop:?}"
            );
            direct
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        COLD.with_borrow(|state| {
            assert!(state.post_initial_work && state.storage_seen && state.fired);
            assert_eq!(
                state.publication_work_seen,
                stop == ColdStop::PublicationWork
            );
            assert_eq!(state.initial_calls, 1);
            assert_eq!(state.checks_left, 0);
            assert!(state.endpoint.is_none());
        });
        OBSERVED.with_borrow(|observed| {
            assert_eq!(
                observed
                    .events
                    .iter()
                    .map(|event| event.stage)
                    .collect::<Vec<_>>(),
                ["admission", "child", "operand", "root"]
            );
            for event in &observed.events {
                let root = event.stage == "root";
                assert_eq!(event.frame, (!root).then_some(key), "{event:?}");
                assert_eq!(event.query_depth, usize::from(!root), "{event:?}");
                assert_eq!(event.claim_held, !root, "{event:?}");
                assert_eq!(event.operation_depth, if root { 0 } else { 2 }, "{event:?}");
                assert!(!event.memo_present, "{event:?}");
                assert_eq!(
                    event.reason,
                    (event.stage != "admission").then_some(Incomplete::Allowance),
                    "{event:?}"
                );
                assert!(!event.panicking, "{event:?}");
            }
        });
        let index = ingredient.memo_ingredient_index(db.zalsa(), node.as_id());
        assert!(
            ingredient
                .get_memo_from_table_for(db.zalsa(), node.as_id(), index)
                .is_none()
        );
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert_eq!(crate::prepared_source_probe::Stamp::current(db), stamp);

        assert_eq!(cycle_fixpoint(db, node).value, 3);
        let accepted = ingredient
            .get_memo_from_table_for(db.zalsa(), node.as_id(), index)
            .unwrap();
        assert!(!accepted.header.may_be_provisional());
        assert_eq!(cycle_fixpoint(db, node).value, 3);
        let cached = ingredient
            .get_memo_from_table_for(db.zalsa(), node.as_id(), index)
            .unwrap();
        assert!(std::ptr::eq(accepted, cached));
        assert_eq!(crate::prepared_source_probe::Stamp::current(db), stamp);
    }
}
