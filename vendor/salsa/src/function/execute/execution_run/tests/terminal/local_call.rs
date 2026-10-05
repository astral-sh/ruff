use std::marker::PhantomPinned;
use std::mem::size_of_val;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::pin::pin;

use super::*;
use crate::function::execute::execution_run::registration::{
    Demand, ExecutableRouteProvider, ProviderContext, TaskEndpoint,
};
use crate::prepared_source_probe::Stamp;

#[derive(Debug)]
struct Event {
    stage: &'static str,
    owner: bool,
    cursor: bool,
    result: bool,
    storage_free: bool,
    reason: Option<Incomplete>,
    panicking: bool,
}

#[derive(Default)]
struct Journal {
    events: RefCell<Vec<Event>>,
    storage: RefCell<u32>,
    owner: Cell<bool>,
    cursor: Cell<bool>,
    result: Cell<bool>,
    actions: Cell<usize>,
    continued: Cell<bool>,
}

impl Journal {
    fn record(&self, stage: &'static str) {
        let storage_free = self.storage.try_borrow_mut().is_ok();
        self.events.borrow_mut().push(Event {
            stage,
            owner: self.owner.get(),
            cursor: self.cursor.get(),
            result: self.result.get(),
            storage_free,
            reason: reason(),
            panicking: crate::sync::thread::panicking(),
        });
    }
    fn assert_stopped(&self, child: bool, retired: bool, result: bool) {
        assert!(!self.continued.get());
        let events = self.events.borrow();
        let position = |stage| {
            events
                .iter()
                .position(|event| event.stage == stage)
                .unwrap()
        };
        for stage in ["owner", "cursor"] {
            assert_eq!(
                events.iter().filter(|event| event.stage == stage).count(),
                1
            );
        }
        assert_eq!(
            events
                .iter()
                .filter(|event| event.stage == "result")
                .count(),
            usize::from(result)
        );
        if child {
            assert_eq!(
                events.iter().filter(|event| event.stage == "child").count(),
                1
            );
            let at = position("child");
            let observed = &events[at];
            assert!(observed.owner && observed.storage_free);
            assert_eq!(observed.cursor, !retired);
            assert_eq!(observed.result, result);
            assert!(at < position("owner"));
            assert_eq!(position("cursor") < at, retired);
            if result {
                assert!(at < position("result"));
            }
        }
        assert!(!self.owner.get() && !self.cursor.get() && !self.result.get());
        assert!(self.storage.try_borrow_mut().is_ok());
    }
}

struct Owner {
    journal: Rc<Journal>,
    value: u32,
}
impl Owner {
    fn new(journal: Rc<Journal>) -> Self {
        assert!(!journal.owner.replace(true));
        Self { journal, value: 17 }
    }
    fn result(&self) -> Borrowed<'_> {
        assert!(!self.journal.result.replace(true));
        Borrowed {
            value: &self.value,
            journal: self.journal.clone(),
            _pin: PhantomPinned,
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.journal.record("owner");
        self.journal.owner.set(false);
    }
}

struct Cursor<'a>(&'a Owner);
impl<'a> Cursor<'a> {
    fn new(owner: &'a Owner) -> Self {
        assert!(!owner.journal.cursor.replace(true));
        Self(owner)
    }
    fn advance(&mut self) {
        self.0.journal.actions.set(self.0.journal.actions.get() + 1);
        *self.0.journal.storage.borrow_mut() += 1;
    }
}
impl Drop for Cursor<'_> {
    fn drop(&mut self) {
        self.0.journal.record("cursor");
        self.0.journal.cursor.set(false);
    }
}

struct MutableCursor<'a>(&'a mut u32);
impl<'a> MutableCursor<'a> {
    fn begin(owner: &'a mut Owner) -> Self {
        Self(&mut owner.value)
    }
}

struct Borrowed<'a> {
    value: &'a u32,
    journal: Rc<Journal>,
    _pin: PhantomPinned,
}
impl Drop for Borrowed<'_> {
    fn drop(&mut self) {
        self.journal.record("result");
        self.journal.result.set(false);
    }
}

struct Capture(Rc<Journal>, PhantomPinned);
impl Drop for Capture {
    fn drop(&mut self) {
        self.0.record("capture");
    }
}

struct Child(Rc<Journal>);
impl Drop for Child {
    fn drop(&mut self) {
        self.0.record("child");
    }
}

fn queue_child(endpoint: &TaskEndpoint<'_, '_>, journal: &Rc<Journal>) -> RunResult<()> {
    let child = Child(journal.clone());
    let _reply = endpoint.demand(move || {
        poll_fn(move |_| -> Poll<RunResult<()>> {
            let _child = &child;
            panic!("a rejected local action must not run its queued child")
        })
    })?;
    Ok(())
}

fn drive<'run, T: 'run, F: Future<Output = RunResult<T>> + 'run>(
    db: &'run dyn Database,
    make: impl FnOnce(TaskEndpoint<'run, 'run>) -> F + 'run,
) -> RunResult<T> {
    RegistryBuilder::new(db, &Unrestricted)?.seal()?.run(make)
}

fn idle(db: &dyn Database) {
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(db.zalsa().attempt_operations.load(Ordering::SeqCst), 0);
}

#[test]
fn local_call_accepts_short_borrows_without_scheduler_work() {
    let callbacks = Arc::new(AtomicUsize::new(0));
    let observed_callbacks = callbacks.clone();
    let db = EventDb {
        storage: crate::Storage::new(Some(Box::new(move |_| {
            observed_callbacks.fetch_add(1, Ordering::Relaxed);
        }))),
    };
    let admission = Admissions::default();
    let journal = Rc::new(Journal::default());
    let (outcome, observations) = observation::collect(|| {
        try_with_attempt(&db, 100_000, || {
            let journal = journal.clone();
            let callbacks = &callbacks;
            let admissions = &admission.0;
            RegistryBuilder::new(&db, &admission)?
                .seal()?
                .run(|endpoint| async move {
                    let mut owner = Owner::new(journal.clone());
                    let owner_ref = &mut owner;
                    let mut constructed = Some(
                        endpoint
                            .local_call(move || Ok(MutableCursor::begin(owner_ref)))
                            .await,
                    );
                    *constructed.as_mut().unwrap().0 += 1;
                    assert_eq!(*constructed.as_ref().unwrap().0, 18);
                    endpoint
                        .local_call(|| {
                            drop(constructed.take());
                            Ok(())
                        })
                        .await;
                    drop(constructed);
                    let mut cursor = Cursor::new(&owner);
                    let before = (admissions.borrow().len(), callbacks.load(Ordering::Relaxed));
                    let capture = Capture(journal.clone(), PhantomPinned);
                    let action = {
                        let (cursor, owner) = (&mut cursor, &owner);
                        move || {
                            let _capture = &capture;
                            cursor.advance();
                            Ok(owner.result())
                        }
                    };
                    let action_size = size_of_val(&action);
                    let local = endpoint.local_call(action);
                    assert_eq!(journal.actions.get(), 0);
                    assert_eq!(
                        (admissions.borrow().len(), callbacks.load(Ordering::Relaxed)),
                        before
                    );
                    if std::env::var_os("SALSA_TASK_LAYOUT_PROBE").is_some() {
                        eprintln!(
                            "LOCAL_CALL_LAYOUT future={} action={action_size} result={}",
                            size_of_val(&local),
                            size_of::<Borrowed<'_>>()
                        );
                    }
                    let result = local.await;
                    assert!(std::ptr::eq(result.value, &owner.value));
                    assert_eq!(journal.actions.get(), 1);
                    assert_eq!(admissions.borrow().len(), before.0);
                    assert_eq!(callbacks.load(Ordering::Relaxed) - before.1, 2);
                    drop(result);
                    for _ in 0..3 {
                        endpoint
                            .local_call(|| {
                                cursor.advance();
                                Ok(())
                            })
                            .await;
                    }
                    let unused_capture = Capture(journal.clone(), PhantomPinned);
                    let unused = endpoint.local_call(move || {
                        unused_capture.0.actions.set(999);
                        Err::<(), _>(RunError::Contract("unpolled action ran"))
                    });
                    let before_drop = callbacks.load(Ordering::Relaxed);
                    drop(unused);
                    assert_eq!(callbacks.load(Ordering::Relaxed), before_drop);
                    assert_eq!(journal.actions.get(), 4);
                    Ok(())
                })
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    assert_eq!(observations.polls, 1);
    assert_eq!(
        admission
            .0
            .borrow()
            .iter()
            .filter(|work| matches!(work, ExecutionWork::Task { .. }))
            .count(),
        1
    );
    assert!(
        !admission
            .0
            .borrow()
            .iter()
            .any(|work| matches!(work, ExecutionWork::Work { .. }))
    );
    idle(&db);
}

#[test]
fn local_call_first_poll_uses_the_current_generation() {
    let db = DatabaseImpl::default();
    let journal = Rc::new(Journal::default());
    let (outcome, observations) = observation::collect(|| {
        try_with_attempt(&db, 100_000, || {
            let journal = journal.clone();
            drive(&db, |endpoint| async move {
                let owner = Owner::new(journal.clone());
                let mut cursor = Cursor::new(&owner);
                let local = endpoint.local_call(|| {
                    cursor.advance();
                    Ok(owner.result())
                });
                let child_state = journal.clone();
                endpoint
                    .demand(move || async move {
                        *child_state.storage.borrow_mut() = 5;
                        Ok(())
                    })?
                    .await?;
                assert_eq!(journal.actions.get(), 0);
                let result = local.await;
                assert!(std::ptr::eq(result.value, &owner.value));
                assert_eq!(*journal.storage.borrow(), 6);
                Ok(())
            })
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    assert_eq!(observations.polls, 3);
    idle(&db);
}

#[test]
fn local_call_errors_retain_the_borrowed_cursor() {
    for (error, previous, expected) in [
        (
            RunError::Contract("local action failed"),
            None,
            Incomplete::Interrupted,
        ),
        (RunError::RequiresFetch, None, Incomplete::Interrupted),
        (
            RunError::Refused(Incomplete::Allowance),
            None,
            Incomplete::Allowance,
        ),
        (
            RunError::Refused(Incomplete::Interrupted),
            Some(Incomplete::Allowance),
            Incomplete::Allowance,
        ),
    ] {
        let after = Arc::new(AtomicUsize::new(0));
        let calls = Arc::new(AtomicUsize::new(0));
        let (armed, observed) = (after.clone(), calls.clone());
        let db = EventDb {
            storage: crate::Storage::new(Some(Box::new(move |_| {
                if armed.load(Ordering::Relaxed) != 0 {
                    observed.fetch_add(1, Ordering::Relaxed);
                }
            }))),
        };
        let stamp = Stamp::current(&db);
        let journal = Rc::new(Journal::default());
        let outcome = try_with_attempt(&db, 100_000, || {
            let state = journal.clone();
            let (db, after) = (&db, &after);
            let result: RunResult<()> = drive(db, |endpoint| async move {
                let owner = Owner::new(state.clone());
                let mut cursor = Cursor::new(&owner);
                endpoint
                    .local_call(|| {
                        cursor.advance();
                        queue_child(&endpoint, &state)?;
                        if let Some(reason) = previous {
                            attempt_probe::report_incomplete(db, reason);
                        }
                        after.store(1, Ordering::Relaxed);
                        Err::<(), _>(error)
                    })
                    .await;
                state.continued.set(true);
                Ok(())
            });
            assert_eq!(result, Err(error));
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Incomplete(expected)));
        assert_eq!(
            calls.load(Ordering::Relaxed),
            0,
            "returned Err must skip the final resume callback"
        );
        journal.assert_stopped(true, false, false);
        assert_eq!(journal.events.borrow()[0].reason, Some(expected));
        assert_eq!(journal.actions.get(), 1);
        idle(&db);
        assert!(stamp.belongs_to(&db));
        after.store(0, Ordering::Relaxed);
        assert_eq!(
            try_with_attempt(&db, 100, || drive(&db, |endpoint| async move {
                Ok(endpoint.local_call(|| Ok(17)).await)
            })),
            Ok(AttemptOutcome::Complete(Ok(17)))
        );
    }
}

struct QueryProvider<'db, C: Configuration> {
    ingredient: &'db IngredientImpl<C>,
    journal: Rc<Journal>,
    drops: Rc<RefCell<Vec<OwnerSnapshot>>>,
}
impl<'run, 'db: 'run, C> ExecutableRouteProvider<'run, 'db, C> for QueryProvider<'db, C>
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
        let _provider = ObserveOwner {
            db,
            ingredient: self.ingredient,
            node,
            stage: "provider",
            drops: self.drops.clone(),
        };
        let owner = Owner::new(self.journal.clone());
        let mut cursor = Cursor::new(&owner);
        let endpoint = context.endpoint();
        endpoint
            .local_call(|| {
                cursor.advance();
                let child = (
                    ObserveOwner {
                        db,
                        ingredient: self.ingredient,
                        node,
                        stage: "child",
                        drops: self.drops.clone(),
                    },
                    Child(self.journal.clone()),
                );
                let _reply = endpoint.demand(move || {
                    poll_fn(move |_| -> Poll<RunResult<()>> {
                        let _child = &child;
                        panic!("rejected query child ran")
                    })
                })?;
                endpoint.admit_work(17)
            })
            .await;
        self.journal.continued.set(true);
        Ok(0)
    }
    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _id: Id,
        _input: Node,
    ) -> RunResult<u32> {
        Err(RunError::RequiresFetch)
    }
    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: Node,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

struct WorkFailure<'db> {
    db: &'db dyn Database,
    fired: Cell<bool>,
}

impl ExecutionAdmission for WorkFailure<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if work == (ExecutionWork::Work { units: 17 }) {
            assert!(!self.fired.replace(true));
            attempt_probe::report_incomplete(self.db, Incomplete::Allowance);
            return Err(RunError::RequiresFetch);
        }
        Ok(())
    }
}

#[test]
fn local_call_failure_retains_the_real_query_frame_and_claim() {
    let db = DatabaseImpl::default();
    let node = Node::new(&db, None, 0);
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    let key = ingredient.database_key_index(node.as_id());
    let journal = Rc::new(Journal::default());
    let drops = Rc::new(RefCell::new(Vec::new()));
    let admission = WorkFailure {
        db: &db,
        fired: Cell::new(false),
    };
    let outcome = try_with_attempt(&db, 100_000, || {
        let db: &dyn Database = &db;
        let provider = QueryProvider {
            ingredient,
            journal: journal.clone(),
            drops: drops.clone(),
        };
        let mut registry = RegistryBuilder::new(db, &admission).unwrap();
        let route = registry.reserve(db, ingredient).unwrap();
        let binding = registry.provider(&provider).unwrap();
        registry.bind_executable(&route, &binding).unwrap();
        let result = registry.seal().unwrap().run(|endpoint| async move {
            endpoint
                .provider(binding)?
                .fetch_ref(&route, node.as_id())?
                .await
        });
        assert_eq!(result, Err(RunError::RequiresFetch));
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert!(admission.fired.get());
    journal.assert_stopped(true, false, false);
    let snapshots = drops.borrow();
    assert_eq!(
        snapshots.iter().map(|item| item.stage).collect::<Vec<_>>(),
        ["child", "provider"]
    );
    for item in snapshots.iter() {
        assert_eq!(item.frame, Some((key, true)));
        assert!(item.claim_held);
        assert_eq!(item.policy, QueryPolicy::ReturnOnly);
        assert_eq!(item.reason, Some(Incomplete::Allowance));
    }
    assert!(memo(&db, ingredient, node).is_none());
    idle(&db);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Fault {
    Child,
    Refuse,
    Panic,
    Local,
    PendingWrite,
}

impl Fault {
    fn expected(self) -> Option<(RunError, Incomplete)> {
        match self {
            Self::Child => Some((
                RunError::Contract("completed task retained a child"),
                Incomplete::Interrupted,
            )),
            Self::Refuse => Some((
                RunError::Refused(Incomplete::Allowance),
                Incomplete::Allowance,
            )),
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Payload(Arc<()>);

fn fault(endpoint: &TaskEndpoint<'_, '_>, journal: &Rc<Journal>, fault: Fault, identity: Arc<()>) {
    queue_child(endpoint, journal).unwrap();
    match fault {
        Fault::Child => {}
        Fault::Refuse => {
            attempt_probe::report_incomplete(endpoint.inner.context.db, Incomplete::Allowance);
        }
        Fault::Panic => panic_any(Payload(identity)),
        Fault::Local | Fault::PendingWrite => {
            if fault == Fault::Local {
                endpoint.inner.context.db.cancellation_token().cancel();
            } else {
                endpoint
                    .inner
                    .context
                    .db
                    .zalsa()
                    .runtime()
                    .set_cancellation_flag();
            }
            endpoint
                .check_completion()
                .expect("cancellation must unwind through its native payload");
        }
    }
}

type Attempt = Result<AttemptOutcome<RunResult<()>>, attempt_probe::StartError>;

fn assert_fault(
    outcome: std::thread::Result<Attempt>,
    returned: Option<RunResult<()>>,
    fault: Fault,
    identity: &Arc<()>,
) {
    if let Some((error, reason)) = fault.expected() {
        assert_eq!(returned, Some(Err(error)));
        assert_eq!(outcome.unwrap(), Ok(AttemptOutcome::Incomplete(reason)));
    } else {
        assert!(returned.is_none());
        let payload = outcome.expect_err("native disposition reaches the caller");
        match fault {
            Fault::Panic => assert!(Arc::ptr_eq(
                &payload.downcast_ref::<Payload>().unwrap().0,
                identity
            )),
            Fault::Local => assert!(matches!(
                payload.downcast_ref::<crate::Cancelled>(),
                Some(crate::Cancelled::Local)
            )),
            Fault::PendingWrite => assert!(matches!(
                payload.downcast_ref::<crate::Cancelled>(),
                Some(crate::Cancelled::PendingWrite)
            )),
            _ => unreachable!(),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Before,
    After,
}

struct Hook {
    endpoint: TaskEndpoint<'static, 'static>,
    wanted: Rc<Cell<bool>>,
    journal: Rc<Journal>,
    phase: Phase,
    target: Phase,
    fault: Fault,
    identity: Arc<()>,
    fired: bool,
}

thread_local! { static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) }; }

struct ClearHook;
impl Drop for ClearHook {
    fn drop(&mut self) {
        HOOK.with_borrow_mut(|slot| *slot = None);
    }
}

fn on_cancellation() {
    let action = HOOK.with_borrow_mut(|slot| {
        let hook = slot.as_mut()?;
        if hook.fired || hook.phase != hook.target {
            return None;
        }
        let active = hook.endpoint.inner.queue.active_poll.borrow();
        if !active
            .as_ref()
            .is_some_and(|poll| Rc::ptr_eq(&poll.identity.wanted, &hook.wanted))
        {
            return None;
        }
        hook.fired = true;
        Some((
            hook.endpoint.clone(),
            hook.journal.clone(),
            hook.fault,
            hook.identity.clone(),
        ))
    });
    if let Some((endpoint, journal, kind, identity)) = action {
        fault(&endpoint, &journal, kind, identity);
    }
}

fn reject_hook(target: Phase, kind: Fault) {
    let db: &'static EventDb = Box::leak(Box::new(EventDb {
        storage: crate::Storage::new(Some(Box::new(|event| {
            if matches!(event.kind, crate::EventKind::WillCheckCancellation) {
                on_cancellation();
            }
        }))),
    }));
    let _clear = ClearHook;
    let journal = Rc::new(Journal::default());
    let identity = Arc::new(());
    let revision = db.zalsa().current_revision();
    let mut returned = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(db, 100_000, || {
            let (state, payload) = (journal.clone(), identity.clone());
            let result = drive(db, move |endpoint| async move {
                let owner = Owner::new(state.clone());
                let mut cursor = Cursor::new(&owner);
                let wanted = endpoint
                    .inner
                    .queue
                    .active_poll
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .identity
                    .wanted
                    .clone();
                HOOK.with_borrow_mut(|slot| {
                    assert!(slot.is_none());
                    *slot = Some(Hook {
                        endpoint: endpoint.clone(),
                        wanted,
                        journal: state.clone(),
                        phase: Phase::Before,
                        target,
                        fault: kind,
                        identity: payload,
                        fired: false,
                    });
                });
                let capture = Capture(state.clone(), PhantomPinned);
                let (owner_ref, cursor_ref) = (&owner, &mut cursor);
                let result = endpoint
                    .local_call(move || {
                        let _capture = &capture;
                        cursor_ref.advance();
                        HOOK.with_borrow_mut(|slot| slot.as_mut().unwrap().phase = Phase::After);
                        Ok(owner_ref.result())
                    })
                    .await;
                state.continued.set(true);
                drop(result);
                Ok(())
            });
            returned = Some(result);
            result
        })
    }));
    assert!(HOOK.with_borrow(|slot| slot.as_ref().unwrap().fired));
    HOOK.with_borrow_mut(|slot| *slot = None);
    db.zalsa_local().uncancel();
    db.zalsa().runtime().reset_cancellation_flag();
    journal.assert_stopped(true, false, target == Phase::After);
    assert_eq!(journal.actions.get(), usize::from(target == Phase::After));
    let events = journal.events.borrow();
    let child = events
        .iter()
        .position(|event| event.stage == "child")
        .unwrap();
    let capture = events
        .iter()
        .position(|event| event.stage == "capture")
        .unwrap();
    assert_eq!(child < capture, target == Phase::Before);
    assert_eq!(events[child].panicking, kind == Fault::Panic);
    assert_eq!(
        events[child].reason,
        if matches!(kind, Fault::Local | Fault::PendingWrite) {
            Some(Incomplete::Interrupted)
        } else {
            kind.expected().map(|(_, reason)| reason)
        }
    );
    drop(events);
    assert_fault(outcome, returned, kind, &identity);
    idle(db);
    assert_eq!(db.zalsa().current_revision(), revision);
    assert_eq!(
        try_with_attempt(db, 100, || drive(db, |endpoint| async move {
            endpoint.local_call(|| Ok(())).await;
            Ok(())
        })),
        Ok(AttemptOutcome::Complete(Ok(())))
    );
}

#[test]
fn local_call_preflight_and_final_checks_keep_captures_and_results() {
    for phase in [Phase::Before, Phase::After] {
        for kind in [Fault::Child, Fault::Refuse, Fault::Panic] {
            if kind == Fault::Panic && cfg!(feature = "shuttle") {
                continue;
            }
            reject_hook(phase, kind);
        }
    }
}

#[test]
fn local_call_rejects_scheduler_progress_after_a_local_action() {
    for checkpoint in [false, true] {
        let db = DatabaseImpl::default();
        let admission = Admissions::default();
        let journal = Rc::new(Journal::default());
        let (outcome, observations) = observation::collect(|| {
            try_with_attempt(&db, 100_000, || {
                let state = journal.clone();
                let result: RunResult<()> = RegistryBuilder::new(&db, &admission)
                    .unwrap()
                    .seal()
                    .unwrap()
                    .run(|endpoint| async move {
                        let owner = Owner::new(state.clone());
                        let mut cursor = Cursor::new(&owner);
                        let result = endpoint
                            .local_call(|| {
                                cursor.advance();
                                if checkpoint {
                                    let mut checkpoint = pin!(endpoint.checkpoint()?);
                                    assert!(
                                        checkpoint
                                            .as_mut()
                                            .poll(&mut Context::from_waker(Waker::noop()))
                                            .is_pending()
                                    );
                                } else {
                                    queue_child(&endpoint, &state)?;
                                }
                                Ok(owner.result())
                            })
                            .await;
                        state.continued.set(true);
                        drop(result);
                        Ok(())
                    });
                assert_eq!(
                    result,
                    Err(RunError::Contract(if checkpoint {
                        "completed task retained a checkpoint"
                    } else {
                        "completed task retained a child"
                    }))
                );
            })
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        journal.assert_stopped(!checkpoint, false, true);
        assert_eq!(observations.polls, 1);
        let admitted = admission.0.borrow();
        assert_eq!(
            admitted
                .iter()
                .filter(|work| matches!(work, ExecutionWork::Work { units: 1 }))
                .count(),
            usize::from(checkpoint)
        );
        assert_eq!(
            admitted
                .iter()
                .filter(|work| matches!(work, ExecutionWork::Task { .. }))
                .count(),
            if checkpoint { 1 } else { 2 }
        );
        idle(&db);
    }
}

struct Retiring<'a, F: FnOnce()> {
    cursor: Option<Cursor<'a>>,
    after: Option<F>,
}
impl<F: FnOnce()> Drop for Retiring<'_, F> {
    fn drop(&mut self) {
        drop(self.cursor.take());
        if let Some(after) = self.after.take() {
            after();
        }
    }
}

#[test]
fn local_call_retirement_keeps_the_completed_result_and_owner() {
    for kind in [Fault::Child, Fault::Refuse, Fault::Panic] {
        if kind == Fault::Panic && cfg!(feature = "shuttle") {
            continue;
        }
        let db = DatabaseImpl::default();
        let journal = Rc::new(Journal::default());
        let identity = Arc::new(());
        let mut returned = None;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 100_000, || {
                let (state, identity) = (journal.clone(), identity.clone());
                let result = drive(&db, |endpoint| async move {
                    let owner = Owner::new(state.clone());
                    let completed = owner.result();
                    let (retiring_endpoint, retiring_state) = (endpoint.clone(), state.clone());
                    let mut cursor = Some(Retiring {
                        cursor: Some(Cursor::new(&owner)),
                        after: Some(move || {
                            fault(&retiring_endpoint, &retiring_state, kind, identity)
                        }),
                    });
                    endpoint
                        .local_call(|| {
                            drop(cursor.take());
                            Ok(())
                        })
                        .await;
                    state.continued.set(true);
                    drop(completed);
                    Ok(())
                });
                returned = Some(result);
                result
            })
        }));
        journal.assert_stopped(true, true, true);
        let events = journal.events.borrow();
        let child = events.iter().find(|event| event.stage == "child").unwrap();
        assert_eq!(child.panicking, kind == Fault::Panic);
        assert_eq!(child.reason, kind.expected().map(|(_, reason)| reason));
        drop(events);
        assert_fault(outcome, returned, kind, &identity);
        idle(&db);
    }
}

#[test]
fn local_call_retirement_cannot_publish_an_abandoned_demand() {
    let db = DatabaseImpl::default();
    let escaped: RefCell<Option<Demand<u32>>> = RefCell::new(None);
    let journal = Rc::new(Journal::default());
    let outcome = try_with_attempt(&db, 100_000, || {
        let (slot, state) = (&escaped, journal.clone());
        let result: RunResult<()> = drive(&db, |endpoint| async move {
            let child_endpoint = endpoint.clone();
            *slot.borrow_mut() = Some(endpoint.demand(move || async move {
                let owner = Owner::new(state.clone());
                let completed = owner.result();
                let (retiring_endpoint, retiring_state) = (child_endpoint.clone(), state.clone());
                let mut cursor = Some(Retiring {
                    cursor: Some(Cursor::new(&owner)),
                    after: Some(move || {
                        queue_child(&retiring_endpoint, &retiring_state).unwrap();
                        drop(slot.borrow_mut().take());
                    }),
                });
                child_endpoint
                    .local_call(|| {
                        drop(cursor.take());
                        Ok(())
                    })
                    .await;
                state.continued.set(true);
                Ok(*completed.value)
            })?);
            pending().await
        });
        assert_eq!(
            result,
            Err(RunError::Contract("callback completed in a stopped task"))
        );
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    assert!(escaped.borrow().is_none());
    journal.assert_stopped(true, true, true);
    idle(&db);
}

#[test]
#[cfg(not(feature = "shuttle"))]
fn local_call_native_cancellation_preserves_its_payload() {
    for kind in [Fault::Panic, Fault::Local, Fault::PendingWrite] {
        let db = DatabaseImpl::default();
        let journal = Rc::new(Journal::default());
        let identity = Arc::new(());
        let mut returned = None;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 100_000, || {
                let (state, identity) = (journal.clone(), identity.clone());
                let result = drive(&db, |endpoint| async move {
                    let owner = Owner::new(state.clone());
                    let mut cursor = Cursor::new(&owner);
                    endpoint
                        .local_call(|| {
                            cursor.advance();
                            fault(&endpoint, &state, kind, identity);
                            Ok(())
                        })
                        .await;
                    state.continued.set(true);
                    Ok(())
                });
                returned = Some(result);
                result
            })
        }));
        db.zalsa_local().uncancel();
        db.zalsa().runtime().reset_cancellation_flag();
        journal.assert_stopped(true, false, false);
        for event in journal.events.borrow().iter() {
            assert_eq!(event.panicking, kind == Fault::Panic);
            assert_eq!(
                event.reason,
                (kind != Fault::Panic).then_some(Incomplete::Interrupted)
            );
        }
        assert_fault(outcome, returned, kind, &identity);
        idle(&db);
        if kind != Fault::Panic {
            for phase in [Phase::Before, Phase::After] {
                reject_hook(phase, kind);
            }
        }
    }
}

#[test]
fn local_call_invalid_endpoints_do_not_mutate_another_attempt() {
    let db = DatabaseImpl::default();
    let expired = match try_with_attempt(&db, 100, || {
        drive(&db, |endpoint| async move { Ok(endpoint) })
    })
    .unwrap()
    {
        AttemptOutcome::Complete(Ok(endpoint)) => endpoint,
        _ => panic!("the completed run returns its real endpoint"),
    };
    let actions = Cell::new(0);
    drop(expired.local_call(|| {
        actions.set(1);
        Ok(())
    }));
    assert_eq!(actions.get(), 0);
    let foreign = DatabaseImpl::default();
    for current in [&db as &dyn Database, &foreign as &dyn Database] {
        let (expired, actions) = (&expired, &actions);
        let outcome = try_with_attempt(current, 100, || {
            drive(current, |endpoint| async move {
                let mut invalid = pin!(expired.local_call(|| {
                    actions.set(1);
                    Ok(())
                }));
                poll_fn(|cx| {
                    assert!(invalid.as_mut().poll(cx).is_pending());
                    assert_eq!(reason(), None);
                    Poll::Ready(())
                })
                .await;
                endpoint.check_completion()?;
                Ok(())
            })
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
        assert_eq!(actions.get(), 0);
        let outcome = try_with_attempt(current, 100, || {
            let result: RunResult<()> = drive(current, |_| async move {
                expired
                    .local_call(|| {
                        actions.set(1);
                        Ok(())
                    })
                    .await;
                Ok(())
            });
            assert!(matches!(result, Err(RunError::Contract(_))));
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_eq!(actions.get(), 0);
        idle(current);
    }
    let (database, actions) = (&db, &actions);
    let outcome = try_with_attempt(&db, 100, || {
        drive(&db, |endpoint| async move {
            {
                let _operation = attempt_probe::enter(
                    database.zalsa(),
                    QueryPolicy::CompleteOnly,
                    "local facade unsupported policy",
                );
                let mut invalid = pin!(endpoint.local_call(|| {
                    actions.set(1);
                    Ok(())
                }));
                poll_fn(|cx| {
                    assert!(invalid.as_mut().poll(cx).is_pending());
                    assert_eq!(reason(), None);
                    Poll::Ready(())
                })
                .await;
            }
            assert_eq!(actions.get(), 0);
            endpoint.check_completion()?;
            Ok(())
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    idle(&db);
}

#[test]
fn ignored_local_call_pending_cannot_publish_a_later_ready() {
    for later_error in [false, true] {
        let db = DatabaseImpl::default();
        let escaped: RefCell<Option<Demand<Returned>>> = RefCell::new(None);
        let drops = Rc::new(Cell::new(0));
        let error = RunError::Contract("first local disposition");
        let outcome = try_with_attempt(&db, 100, || {
            let (slot, drops) = (&escaped, drops.clone());
            let result: RunResult<()> = drive(&db, |endpoint| async move {
                let child = endpoint.clone();
                *slot.borrow_mut() = Some(endpoint.demand(move || async move {
                    let mut stopped = pin!(child.local_call(|| Err::<(), _>(error)));
                    poll_fn(|cx| {
                        assert!(stopped.as_mut().poll(cx).is_pending());
                        Poll::Ready(if later_error {
                            Err(RunError::RequiresFetch)
                        } else {
                            Ok(Returned(drops.clone()))
                        })
                    })
                    .await
                })?);
                pending().await
            });
            assert_eq!(result, Err(error));
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_eq!(drops.get(), usize::from(!later_error));
        let mut reply = escaped.into_inner().unwrap();
        assert!(
            matches!(Pin::new(&mut reply).poll(&mut Context::from_waker(Waker::noop())), Poll::Ready(Err(observed)) if observed == error)
        );
        idle(&db);
    }
}

#[test]
#[cfg(not(feature = "shuttle"))]
fn local_call_preserves_native_precedence_within_the_same_poll() {
    for first_native in [false, true] {
        let db = DatabaseImpl::default();
        let first = Arc::new(());
        let second = Arc::new(());
        let (first_payload, second_payload) = (first.clone(), second.clone());
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 100, || {
                drive(&db, |endpoint| async move {
                    endpoint
                        .local_call(|| -> RunResult<()> {
                            let mut inner = pin!(endpoint.local_call(|| -> RunResult<()> {
                                if first_native {
                                    panic_any(Payload(first_payload));
                                }
                                Err(RunError::RequiresFetch)
                            }));
                            assert!(
                                inner
                                    .as_mut()
                                    .poll(&mut Context::from_waker(Waker::noop()))
                                    .is_pending()
                            );
                            panic_any(Payload(second_payload));
                        })
                        .await;
                    Ok(())
                })
            })
        }));
        let payload = outcome.unwrap_err();
        assert!(Arc::ptr_eq(
            &payload.downcast_ref::<Payload>().unwrap().0,
            if first_native { &first } else { &second }
        ));
        idle(&db);
    }
}
