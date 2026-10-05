use std::marker::PhantomPinned;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::pin::pin;

use super::*;
use crate::function::execute::execution_run::registration::{
    ExecutableRouteProvider, ProviderContext, Route, TaskEndpoint,
};
use crate::prepared_source_probe::Stamp;

#[derive(Debug)]
struct Entry {
    stage: &'static str,
    owner: bool,
    result: bool,
    storage_free: bool,
    reason: Option<Incomplete>,
}

#[derive(Default)]
struct Journal {
    events: RefCell<Vec<Entry>>,
    storage: RefCell<u32>,
    owner: Cell<bool>,
    result: Cell<bool>,
    factories: Cell<usize>,
    continued: Cell<bool>,
}

impl Journal {
    fn record(&self, stage: &'static str) {
        self.events.borrow_mut().push(Entry {
            stage,
            owner: self.owner.get(),
            result: self.result.get(),
            storage_free: self.storage.try_borrow_mut().is_ok(),
            reason: reason(),
        });
    }

    fn assert_child_first(&self, result: bool, reason: Option<Incomplete>) {
        let events = self.events.borrow();
        let stages: Vec<_> = events.iter().map(|event| event.stage).collect();
        let at = stages.iter().position(|stage| *stage == "child").unwrap();
        assert_eq!(stages.iter().filter(|stage| **stage == "child").count(), 1);
        assert_eq!(stages.iter().filter(|stage| **stage == "owner").count(), 1);
        assert!(at < stages.iter().position(|stage| *stage == "owner").unwrap());
        assert!(events[at].owner && events[at].storage_free);
        assert_eq!(events[at].result, result);
        assert_eq!(events[at].reason, reason);
        if result {
            assert!(at < stages.iter().position(|stage| *stage == "result").unwrap());
        }
        assert!(!self.owner.get() && !self.result.get() && !self.continued.get());
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
            owner: self,
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

struct Borrowed<'a> {
    owner: &'a Owner,
    _pin: PhantomPinned,
}

impl Drop for Borrowed<'_> {
    fn drop(&mut self) {
        self.owner.journal.record("result");
        self.owner.journal.result.set(false);
    }
}

struct Child {
    journal: Rc<Journal>,
    observe: Option<Box<dyn FnOnce()>>,
}

impl Drop for Child {
    fn drop(&mut self) {
        self.journal.record("child");
        if let Some(observe) = self.observe.take() {
            observe();
        }
    }
}

fn queue_child(
    endpoint: &TaskEndpoint<'_, '_>,
    journal: Rc<Journal>,
    observe: Option<Box<dyn FnOnce()>>,
) -> RunResult<()> {
    let child = Child { journal, observe };
    let _reply = endpoint.demand(move || {
        poll_fn(move |_| -> Poll<RunResult<()>> {
            let _held = &child;
            panic!("a rejected creation must not run its queued child");
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

async fn request<'a, 'run: 'a, 'db: 'run>(
    endpoint: &'a TaskEndpoint<'run, 'db>,
    owner: &'a Owner,
) -> RunResult<Borrowed<'a>> {
    owner
        .journal
        .factories
        .set(owner.journal.factories.get() + 1);
    owner.journal.record("request");
    let child_endpoint = endpoint.clone();
    let child_journal = owner.journal.clone();
    let value = endpoint
        .demand(move || async move {
            child_journal.record("nested child");
            let grandchild = child_journal.clone();
            let result = child_endpoint
                .demand(move || async move {
                    grandchild.record("grandchild");
                    Ok(17)
                })?
                .await?;
            child_journal.record("child resumed");
            Ok(result)
        })?
        .await?;
    assert_eq!(value, owner.value);
    owner.journal.record("request resumed");
    Ok(owner.result())
}

fn successful_run(wrapped: bool) -> (Vec<ExecutionWork>, usize) {
    let callbacks = Arc::new(AtomicUsize::new(0));
    let counted = callbacks.clone();
    let db = EventDb {
        storage: crate::Storage::new(Some(Box::new(move |event| {
            if matches!(event.kind, crate::EventKind::WillCheckCancellation) {
                counted.fetch_add(1, Ordering::Relaxed);
            }
        }))),
    };
    let admission = Admissions::default();
    let journal = Rc::new(Journal::default());
    let state = journal.clone();
    let outcome = try_with_attempt(&db, 100_000, || {
        RegistryBuilder::new(&db, &admission)?
            .seal()?
            .run(move |endpoint| async move {
                let owner = Owner::new(state.clone());
                let unused = endpoint.child_call(|| {
                    state.factories.set(100);
                    std::future::ready(Ok(()))
                });
                drop(unused);
                assert_eq!(state.factories.get(), 0);
                if wrapped {
                    let deferred = endpoint.child_call(|| request(&endpoint, &owner));
                    let before = state.clone();
                    endpoint
                        .demand(move || async move {
                            before.record("earlier child");
                            Ok(())
                        })?
                        .await?;
                    assert_eq!(state.factories.get(), 0);
                    let result = deferred.await;
                    assert!(std::ptr::eq(result.owner, &owner));
                    drop(result);
                } else {
                    let before = state.clone();
                    endpoint
                        .demand(move || async move {
                            before.record("earlier child");
                            Ok(())
                        })?
                        .await?;
                    let result = request(&endpoint, &owner).await?;
                    assert!(std::ptr::eq(result.owner, &owner));
                    drop(result);
                }
                Ok(())
            })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    assert_eq!(journal.factories.get(), 1);
    assert_eq!(
        journal
            .events
            .borrow()
            .iter()
            .map(|entry| entry.stage)
            .collect::<Vec<_>>(),
        [
            "earlier child",
            "request",
            "nested child",
            "grandchild",
            "child resumed",
            "request resumed",
            "result",
            "owner"
        ]
    );
    idle(&db);
    (admission.0.into_inner(), callbacks.load(Ordering::Relaxed))
}

#[test]
fn child_call_runs_real_children_with_short_borrowed_results() {
    let (direct, direct_callbacks) = successful_run(false);
    let (wrapped, wrapped_callbacks) = successful_run(true);
    let without_sizes = |events: &[ExecutionWork]| {
        events
            .iter()
            .map(|work| match work {
                ExecutionWork::Task { .. } => ExecutionWork::Task { requested_bytes: 0 },
                other => *other,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(without_sizes(&direct), without_sizes(&wrapped));
    assert_eq!(direct_callbacks, wrapped_callbacks);
    assert_eq!(
        wrapped
            .iter()
            .filter(|work| matches!(work, ExecutionWork::Task { .. }))
            .count(),
        4
    );
    if std::env::var_os("SALSA_TASK_LAYOUT_PROBE").is_some() {
        eprintln!(
            "CHILD_CALL_ADMISSIONS direct={direct:?} wrapped={wrapped:?} callbacks={wrapped_callbacks}"
        );
    }
}

#[derive(Clone, Copy)]
enum Fault {
    Refuse,
    EarlierReason,
    Panic,
    Cancel,
}

#[derive(Debug)]
struct Payload(Arc<()>);

struct Injection {
    db: &'static DatabaseImpl,
    endpoint: RefCell<Option<TaskEndpoint<'static, 'static>>>,
    child_observer: RefCell<Option<Box<dyn FnOnce()>>>,
    journal: Rc<Journal>,
    armed: Cell<bool>,
    fired: Cell<bool>,
    fault: Fault,
    payload: Arc<()>,
}

impl Injection {
    fn new(db: &'static DatabaseImpl, fault: Fault) -> &'static Self {
        Box::leak(Box::new(Self {
            db,
            endpoint: RefCell::new(None),
            child_observer: RefCell::new(None),
            journal: Rc::new(Journal::default()),
            armed: Cell::new(false),
            fired: Cell::new(false),
            fault,
            payload: Arc::new(()),
        }))
    }

    fn arm(&self, endpoint: &TaskEndpoint<'static, 'static>) {
        assert!(!self.armed.replace(true));
        *self.endpoint.borrow_mut() = Some(endpoint.clone());
    }
}

impl ExecutionAdmission for Injection {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if !matches!(work, ExecutionWork::Task { .. }) || !self.armed.replace(false) {
            return Ok(());
        }
        assert!(!self.fired.replace(true));
        let endpoint = self.endpoint.borrow().as_ref().unwrap().clone();
        queue_child(
            &endpoint,
            self.journal.clone(),
            self.child_observer.borrow_mut().take(),
        )?;
        match self.fault {
            Fault::Refuse => Err(RunError::Refused(Incomplete::Allowance)),
            Fault::EarlierReason => {
                attempt_probe::report_incomplete(self.db, Incomplete::Allowance);
                Err(RunError::Refused(Incomplete::Interrupted))
            }
            Fault::Panic => panic_any(Payload(self.payload.clone())),
            Fault::Cancel => {
                self.db.zalsa().runtime().set_cancellation_flag();
                endpoint.check_completion()?;
                panic!("native cancellation returned");
            }
        }
    }
}

struct ClearInjection(&'static Injection);
impl Drop for ClearInjection {
    fn drop(&mut self) {
        self.0.endpoint.borrow_mut().take();
        self.0.child_observer.borrow_mut().take();
        self.0.armed.set(false);
        self.0.db.zalsa().runtime().reset_cancellation_flag();
    }
}

#[test]
#[cfg(not(feature = "shuttle"))]
fn child_call_creation_failures_keep_the_caller_until_queued_children_drop() {
    for fault in [
        Fault::Refuse,
        Fault::EarlierReason,
        Fault::Panic,
        Fault::Cancel,
    ] {
        let db: &'static DatabaseImpl = Box::leak(Box::new(DatabaseImpl::default()));
        let injection = Injection::new(db, fault);
        let clear = ClearInjection(injection);
        let stamp = Stamp::current(db);
        let revision = db.zalsa().current_revision();
        let mut returned = None;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(db, 100_000, || {
                let result =
                    RegistryBuilder::new(db, injection)?
                        .seal()?
                        .run(move |endpoint| async move {
                            let owner = Owner::new(injection.journal.clone());
                            injection.arm(&endpoint);
                            let result = endpoint
                                .child_call(|| async {
                                    *owner.journal.storage.borrow_mut() += 1;
                                    let state = injection.journal.clone();
                                    endpoint
                                        .demand(move || async move {
                                            state.factories.set(state.factories.get() + 1);
                                            Ok(())
                                        })?
                                        .await?;
                                    Ok(owner.result())
                                })
                                .await;
                            injection.journal.continued.set(true);
                            drop(result);
                            Ok(())
                        });
                returned = Some(result);
                result
            })
        }));
        assert!(injection.fired.get());
        assert_eq!(injection.journal.factories.get(), 0);
        let expected_reason = match fault {
            Fault::Refuse | Fault::EarlierReason => Some(Incomplete::Allowance),
            Fault::Cancel => Some(Incomplete::Interrupted),
            Fault::Panic => None,
        };
        injection.journal.assert_child_first(false, expected_reason);
        match fault {
            Fault::Refuse | Fault::EarlierReason => {
                let error = RunError::Refused(if matches!(fault, Fault::EarlierReason) {
                    Incomplete::Interrupted
                } else {
                    Incomplete::Allowance
                });
                assert_eq!(returned, Some(Err(error)));
                assert_eq!(
                    outcome.unwrap(),
                    Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
                );
            }
            Fault::Panic => {
                assert!(returned.is_none());
                let payload = outcome.unwrap_err();
                assert!(Arc::ptr_eq(
                    &payload.downcast_ref::<Payload>().unwrap().0,
                    &injection.payload
                ));
            }
            Fault::Cancel => {
                assert!(returned.is_none());
                assert!(matches!(
                    outcome.unwrap_err().downcast_ref::<crate::Cancelled>(),
                    Some(crate::Cancelled::PendingWrite)
                ));
            }
        }
        drop(clear);
        idle(db);
        assert_eq!(db.zalsa().current_revision(), revision);
        if !matches!(fault, Fault::Cancel) {
            assert!(stamp.belongs_to(db));
        }
        assert_eq!(
            try_with_attempt(db, 100, || drive(db, |endpoint| async move {
                let value = endpoint
                    .child_call(|| async { endpoint.demand(|| async { Ok(17) })?.await })
                    .await;
                Ok(value)
            })),
            Ok(AttemptOutcome::Complete(Ok(17)))
        );
    }
}

struct QueryProvider<C: Configuration> {
    ingredient: &'static IngredientImpl<C>,
    route: Route<'static, C>,
    injection: &'static Injection,
    snapshots: Rc<RefCell<Vec<OwnerSnapshot>>>,
    reject: bool,
}

impl<C> ExecutableRouteProvider<'static, 'static, C> for QueryProvider<C>
where
    C: Configuration<DbView = dyn Database, Input<'static> = Node, Output<'static> = u32>,
{
    // Node conversion constructs a handle; output equality compares u32.
    fixture_native_value!(executable, 'static, 'static, C, 1);

    async fn body(
        &'static self,
        context: ProviderContext<'static, 'static, Self>,
        db: &'static dyn Database,
        node: Node,
    ) -> RunResult<u32> {
        let Some(next) = node.next(db) else {
            return Ok(1);
        };
        let _provider = ObserveOwner {
            db,
            ingredient: self.ingredient,
            node,
            stage: "provider",
            drops: self.snapshots.clone(),
        };
        let owner = Owner::new(self.injection.journal.clone());
        let endpoint = context.endpoint();
        if self.reject {
            let ingredient = self.ingredient;
            let drops = self.snapshots.clone();
            *self.injection.child_observer.borrow_mut() = Some(Box::new(move || {
                drop(ObserveOwner {
                    db,
                    ingredient,
                    node,
                    stage: "queued child",
                    drops,
                });
            }));
            self.injection.arm(endpoint);
        }
        let (value, borrowed) = endpoint
            .child_call(|| async {
                let result = context.fetch_ref(&self.route, next.as_id())?.await?;
                Ok((*result + 1, &owner.value))
            })
            .await;
        assert_eq!(*borrowed, 17);
        self.injection.journal.continued.set(true);
        Ok(value)
    }

    async fn initial(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _id: Id,
        _input: Node,
    ) -> RunResult<u32> {
        Err(RunError::RequiresFetch)
    }

    async fn recover<'call>(
        &'static self,
        _context: ProviderContext<'static, 'static, Self>,
        _db: &'static dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: Node,
    ) -> RunResult<u32>
    where
        'static: 'call,
    {
        Err(RunError::RequiresFetch)
    }
}

fn query_run<C>(
    db: &'static DatabaseImpl,
    ingredient: &'static IngredientImpl<C>,
    node: Node,
    injection: &'static Injection,
    snapshots: Rc<RefCell<Vec<OwnerSnapshot>>>,
    reject: bool,
) -> RunResult<u32>
where
    C: Configuration<DbView = dyn Database, Input<'static> = Node, Output<'static> = u32>,
{
    let mut registry = RegistryBuilder::new(db, injection)?;
    let route = registry.reserve(db as &dyn Database, ingredient)?;
    let provider: &'static QueryProvider<C> = Box::leak(Box::new(QueryProvider {
        ingredient,
        route,
        injection,
        snapshots,
        reject,
    }));
    let binding = registry.provider(provider)?;
    registry.bind_executable(&provider.route, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        Ok(*endpoint
            .provider(binding)?
            .fetch_ref(&provider.route, node.as_id())?
            .await?)
    })
}

#[test]
fn child_call_fetch_retains_the_real_query_owner() {
    for reject in [false, true] {
        let db: &'static DatabaseImpl = Box::leak(Box::new(DatabaseImpl::default()));
        let child = Node::new(db, None, 0);
        let parent = Node::new(db, Some(child), 0);
        let ingredient = fixpoint::fn_ingredient_(db, db.zalsa());
        let key = ingredient.database_key_index(parent.as_id());
        let stamp = Stamp::current(db);
        let injection = Injection::new(db, Fault::Refuse);
        let clear = ClearInjection(injection);
        let snapshots = Rc::new(RefCell::new(Vec::new()));
        let outcome = try_with_attempt(db, 100_000, || {
            let result = query_run(db, ingredient, parent, injection, snapshots.clone(), reject);
            assert_eq!(
                result,
                if reject {
                    Err(RunError::Refused(Incomplete::Allowance))
                } else {
                    Ok(2)
                }
            );
            result
        });
        if reject {
            assert_eq!(
                outcome,
                Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
            );
            assert!(injection.fired.get());
            injection
                .journal
                .assert_child_first(false, Some(Incomplete::Allowance));
            let snapshots = snapshots.borrow();
            assert_eq!(
                snapshots
                    .iter()
                    .map(|snapshot| snapshot.stage)
                    .collect::<Vec<_>>(),
                ["queued child", "provider"]
            );
            for snapshot in snapshots.iter() {
                assert_eq!(snapshot.frame, Some((key, true)));
                assert!(snapshot.claim_held);
                assert_eq!(snapshot.operation_depth, 1);
                assert_eq!(snapshot.policy, QueryPolicy::ReturnOnly);
                assert_eq!(snapshot.reason, Some(Incomplete::Allowance));
            }
            assert!(memo(db, ingredient, parent).is_none());
            assert!(memo(db, ingredient, child).is_none());
        } else {
            assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(2))));
            assert!(!injection.fired.get());
            assert!(injection.journal.continued.get());
            assert_eq!(memo(db, ingredient, child).unwrap().value(), Some(&1));
            assert_eq!(memo(db, ingredient, parent).unwrap().value(), Some(&2));
        }
        drop(clear);
        idle(db);
        assert!(stamp.belongs_to(db));
        assert_eq!(
            try_with_attempt(db, 100_000, || query_run(
                db,
                ingredient,
                parent,
                injection,
                Rc::new(RefCell::new(Vec::new())),
                false,
            )),
            Ok(AttemptOutcome::Complete(Ok(2)))
        );
        assert_eq!(fixpoint(db, parent), 2);
        idle(db);
        assert!(stamp.belongs_to(db));
    }
}

struct Retirement<'a, 'run, 'db: 'run> {
    endpoint: &'a TaskEndpoint<'run, 'db>,
    owner: &'a Owner,
    _pin: PhantomPinned,
}

impl<'a> Future for Retirement<'a, '_, '_> {
    type Output = RunResult<Borrowed<'a>>;

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(Ok(self.as_ref().get_ref().owner.result()))
    }
}

impl Drop for Retirement<'_, '_, '_> {
    fn drop(&mut self) {
        self.owner.journal.record("retirement");
        queue_child(self.endpoint, self.owner.journal.clone(), None).unwrap();
        attempt_probe::report_incomplete(self.endpoint.inner.context.db, Incomplete::Allowance);
    }
}

#[test]
fn child_call_retirement_keeps_the_staged_borrowed_result() {
    let db = DatabaseImpl::default();
    let journal = Rc::new(Journal::default());
    let state = journal.clone();
    let outcome = try_with_attempt(&db, 100, || {
        let result = drive(&db, move |endpoint| async move {
            let owner = Owner::new(state.clone());
            let result = endpoint
                .child_call(|| Retirement {
                    endpoint: &endpoint,
                    owner: &owner,
                    _pin: PhantomPinned,
                })
                .await;
            state.continued.set(true);
            drop(result);
            Ok(())
        });
        assert_eq!(result, Err(RunError::Refused(Incomplete::Allowance)));
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    journal.assert_child_first(true, Some(Incomplete::Allowance));
    assert_eq!(
        journal
            .events
            .borrow()
            .iter()
            .map(|event| event.stage)
            .collect::<Vec<_>>(),
        ["retirement", "child", "result", "owner"]
    );
    idle(&db);
}

struct FactoryCapture(Rc<Cell<usize>>);
impl Drop for FactoryCapture {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn child_call_ineligible_endpoints_do_not_invoke_the_factory() {
    let db = DatabaseImpl::default();
    let foreign = DatabaseImpl::default();
    let expired = match try_with_attempt(&db, 100, || {
        drive(&db, |endpoint| async move { Ok(endpoint) })
    })
    .unwrap()
    {
        AttemptOutcome::Complete(Ok(endpoint)) => endpoint,
        _ => panic!("the completed run returns its endpoint"),
    };
    let calls = Cell::new(0);
    let drops = Rc::new(Cell::new(0));
    for current in [&db as &dyn Database, &foreign as &dyn Database] {
        let (expired, calls, drops) = (&expired, &calls, drops.clone());
        let outcome = try_with_attempt(current, 100, || {
            drive(current, |endpoint| async move {
                let before = drops.get();
                {
                    let capture = FactoryCapture(drops.clone());
                    let mut invalid = pin!(expired.child_call(move || {
                        let _capture = capture;
                        calls.set(calls.get() + 1);
                        std::future::ready(Ok(()))
                    }));
                    poll_fn(|cx| {
                        assert!(invalid.as_mut().poll(cx).is_pending());
                        assert_eq!(reason(), None);
                        assert_eq!(drops.get(), before);
                        Poll::Ready(())
                    })
                    .await;
                }
                assert_eq!(drops.get(), before + 1);
                endpoint
                    .child_call(|| async { endpoint.demand(|| async { Ok(()) })?.await })
                    .await;
                Ok(())
            })
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
        assert_eq!(calls.get(), 0);
        idle(current);
    }
    let db_ref = &db;
    let calls = &calls;
    let outcome = try_with_attempt(&db, 100, || {
        drive(&db, |endpoint| async move {
            {
                let _operation = attempt_probe::enter(
                    db_ref.zalsa(),
                    QueryPolicy::CompleteOnly,
                    "child facade policy control",
                );
                let mut invalid = pin!(endpoint.child_call(|| {
                    calls.set(calls.get() + 1);
                    std::future::ready(Ok(()))
                }));
                poll_fn(|cx| {
                    assert!(invalid.as_mut().poll(cx).is_pending());
                    assert_eq!(reason(), None);
                    Poll::Ready(())
                })
                .await;
            }
            endpoint.check_completion()?;
            Ok(())
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    assert_eq!(calls.get(), 0);
    idle(&db);
}

#[test]
fn child_call_rejects_malformed_pending_and_ignored_failure() {
    let db = DatabaseImpl::default();
    let outcome = try_with_attempt(&db, 100, || {
        let result: RunResult<()> = drive(&db, |endpoint| async move {
            endpoint.child_call(|| pending::<RunResult<()>>()).await;
            Ok(())
        });
        assert!(matches!(result, Err(RunError::Contract(_))));
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    idle(&db);
    let journal = Rc::new(Journal::default());
    let state = journal.clone();
    let error = RunError::Contract("ignored child-call failure");
    let output_drops = Rc::new(Cell::new(0));
    let escaped = RefCell::new(None);
    let outcome = try_with_attempt(&db, 100, || {
        let output_drops = output_drops.clone();
        let escaped = &escaped;
        let result: RunResult<()> = drive(&db, move |endpoint| async move {
            let child = endpoint.clone();
            *escaped.borrow_mut() = Some(endpoint.demand(move || async move {
                let _owner = Owner::new(state.clone());
                let mut stopped =
                    pin!(child.child_call(|| std::future::ready(Err::<(), _>(error))));
                poll_fn(|cx| {
                    assert!(stopped.as_mut().poll(cx).is_pending());
                    Poll::Ready(())
                })
                .await;
                state.record("ignored pending");
                Ok(Returned(output_drops))
            })?);
            pending().await
        });
        assert_eq!(result, Err(error));
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    assert_eq!(
        journal
            .events
            .borrow()
            .iter()
            .map(|event| event.stage)
            .collect::<Vec<_>>(),
        ["ignored pending", "owner"]
    );
    assert_eq!(output_drops.get(), 1);
    let mut escaped = escaped.into_inner().unwrap();
    assert!(
        matches!(Pin::new(&mut escaped).poll(&mut Context::from_waker(Waker::noop())), Poll::Ready(Err(actual)) if actual == error)
    );
    idle(&db);
}
