#![cfg(feature = "inventory")]

use std::cell::{Cell, RefCell};
use std::num::NonZeroUsize;
use std::sync::{Arc, Barrier};

use salsa::attempt_probe::{
    AttemptOutcome, Incomplete, charge, remaining_allowance_for_diagnostics, try_with_attempt,
};
use salsa::execution_probe::{
    CallableRoute, CallableRouteProvider, ExecutionAdmission, ExecutionBudget, ExecutionLimits,
    ExecutionWork, NativeCallbackLimits, RegistryBuilder, RunError, RunResult, TaskEndpoint,
    try_with_execution_budget, with_native_callback,
};
use salsa::plumbing::function::{Configuration, IngredientImpl};
use salsa::plumbing::{AsId, ZalsaDatabase};
use salsa::{Cycle, Database, DatabaseImpl, Id, Setter};

const RESERVE: usize = 10_000_000;
fn limits(work: usize) -> ExecutionLimits {
    ExecutionLimits {
        semantic_work: work,
        requested_bytes: RESERVE,
    }
}

#[salsa::input]
struct Node {
    #[returns(copy)]
    next: Option<Node>,
}
thread_local! {
    static STABLE_BODIES: Cell<usize> = const { Cell::new(0) };
    static NATIVE_BEFORE_BODY: Cell<Option<usize>> = const { Cell::new(None) };
    static ORDINARY_BODIES: Cell<usize> = const { Cell::new(0) };
}

#[salsa::tracked(returns(copy), attempt = CompleteOnly)]
fn stable(_db: &dyn Database) -> u32 {
    STABLE_BODIES.set(STABLE_BODIES.get() + 1);
    7
}

#[salsa::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover)]
fn value(db: &dyn Database, node: Node) -> u32 {
    ORDINARY_BODIES.set(ORDINARY_BODIES.get() + 1);
    (node.next(db).map_or(0, |next| *value(db, next)) + 1).min(3)
}
fn initial(_db: &dyn Database, _id: Id, _node: Node) -> u32 {
    0
}
fn recover(_db: &dyn Database, _cycle: &Cycle<'_>, _last: &u32, value: u32, _node: Node) -> u32 {
    value
}

#[derive(Default)]
struct Observed {
    bodies: Cell<usize>,
    initials: Cell<usize>,
    recoveries: Cell<usize>,
    refuse_bytes: Cell<bool>,
    refuse_recovery_bytes: Cell<bool>,
    dropped: Cell<usize>,
    events: RefCell<Vec<&'static str>>,
}
struct Provider<'run, 'db: 'run, C: Configuration> {
    route: CallableRoute<'run, 'db, C>,
    observed: &'run Observed,
}
impl<C: Configuration> Drop for Provider<'_, '_, C> {
    fn drop(&mut self) {
        self.observed.dropped.set(self.observed.dropped.get() + 1);
    }
}
impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for Provider<'run, 'db, C>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Database,
        node: Node,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                self.observed.bodies.set(self.observed.bodies.get() + 1);
                assert_eq!(stable(db), 7);
                Ok(())
            })
            .await;
        if self.observed.refuse_bytes.replace(false) {
            endpoint
                .local_call(|| {
                    endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: usize::MAX,
                    })
                })
                .await;
        }
        let next = endpoint.local_call(|| Ok(node.next(db))).await;
        let child = match next {
            Some(next) => {
                *endpoint
                    .child_call(|| async { endpoint.fetch_ref(&self.route, next.as_id())?.await })
                    .await
            }
            None => 0,
        };
        Ok((child + 1).min(3))
    }
    async fn initial<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _id: Id,
        _node: Node,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        self.observed.initials.set(self.observed.initials.get() + 1);
        Ok(0)
    }
    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        value: u32,
        _node: Node,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        self.observed
            .recoveries
            .set(self.observed.recoveries.get() + 1);
        if self.observed.refuse_recovery_bytes.replace(false) {
            endpoint
                .local_call(|| {
                    endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: usize::MAX,
                    })
                })
                .await;
        }
        Ok(value)
    }
}

fn fetch<'run, 'db: 'run, C>(
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    node: Node,
    budget: &'run ExecutionBudget<'_>,
    observed: &'run Observed,
) -> RunResult<u32>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    let mut registry = RegistryBuilder::with_budget(db, budget)?;
    let route = registry.reserve_callable(db, ingredient)?;
    registry.bind_callable(
        &route,
        Provider {
            route: route.clone(),
            observed,
        },
    )?;
    registry.seal()?.run(move |endpoint| async move {
        endpoint.fetch_ref(&route, node.as_id())?.await.copied()
    })
}

#[test]
fn public_budget_cold_warm_and_same_revision_retry_retain_completed_dependency() {
    STABLE_BODIES.set(0);
    ORDINARY_BODIES.set(0);
    let db = DatabaseImpl::default();
    let node = Node::new(&db, None);
    let ingredient = value::fn_ingredient_(&db, db.zalsa());
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    let observed = Observed::default();
    observed.refuse_bytes.set(true);
    assert_eq!(
        try_with_execution_budget(&db, limits(1000), |budget| fetch(
            &db, ingredient, node, &budget, &observed
        )),
        Ok(AttemptOutcome::Incomplete(Incomplete::RequestedAllocation))
    );
    assert_eq!(observed.bodies.get(), 1);
    assert_eq!(observed.dropped.get(), 1);
    assert_eq!(STABLE_BODIES.get(), 1);
    assert_eq!(
        try_with_execution_budget(&db, limits(1000), |budget| {
            assert_eq!(fetch(&db, ingredient, node, &budget, &observed), Ok(1));
            let remaining = remaining_allowance_for_diagnostics(&db);
            assert_eq!(fetch(&db, ingredient, node, &budget, &observed), Ok(1));
            assert_eq!(remaining_allowance_for_diagnostics(&db), remaining);
        }),
        Ok(AttemptOutcome::Complete(()))
    );
    assert_eq!(observed.bodies.get(), 2);
    assert_eq!(observed.dropped.get(), 3);
    assert_eq!(STABLE_BODIES.get(), 1);
    assert_eq!(ORDINARY_BODIES.get(), 0);
    assert!(stamp.belongs_to(&db));
    assert_eq!(*value(&db, node), 1);
    assert_eq!(ORDINARY_BODIES.get(), 0);
}

#[test]
fn productive_cycle_refusal_retries_and_converges_on_sealed_root() {
    let mut db = DatabaseImpl::default();
    let node = Node::new(&db, None);
    node.set_next(&mut db).to(Some(node));
    let ingredient = value::fn_ingredient_(&db, db.zalsa());
    let observed = Observed::default();
    assert_eq!(
        try_with_execution_budget(&db, limits(1), |budget| fetch(
            &db, ingredient, node, &budget, &observed
        )),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(observed.dropped.get(), 1);
    assert_eq!(
        try_with_execution_budget(&db, limits(1000), |budget| fetch(
            &db, ingredient, node, &budget, &observed
        )),
        Ok(AttemptOutcome::Complete(Ok(3)))
    );
    assert!(observed.initials.get() > 0 && observed.recoveries.get() > 0);
    assert_eq!(observed.dropped.get(), 2);
    assert_eq!(*value(&db, node), 3);
}

#[test]
fn provisional_recovery_allocation_refusal_preserves_retry() {
    let mut db = DatabaseImpl::default();
    let node = Node::new(&db, None);
    node.set_next(&mut db).to(Some(node));
    let ingredient = value::fn_ingredient_(&db, db.zalsa());
    let observed = Observed::default();
    observed.refuse_recovery_bytes.set(true);
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    assert_eq!(
        try_with_execution_budget(&db, limits(1000), |budget| fetch(
            &db, ingredient, node, &budget, &observed
        )),
        Ok(AttemptOutcome::Incomplete(Incomplete::RequestedAllocation))
    );
    assert_eq!(observed.recoveries.get(), 1);
    assert_eq!(observed.dropped.get(), 1);
    assert_eq!(
        try_with_execution_budget(&db, limits(1000), |budget| fetch(
            &db, ingredient, node, &budget, &observed
        )),
        Ok(AttemptOutcome::Complete(Ok(3)))
    );
    assert_eq!(observed.dropped.get(), 2);
    assert!(stamp.belongs_to(&db));
    assert_eq!(*value(&db, node), 3);
}

#[salsa::tracked(returns(copy), attempt = ReturnOnly)]
fn native(db: &dyn Database) -> u32 {
    match with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |entry| {
        RegistryBuilder::for_native_callback_with_budget(db, &entry)?
            .seal()?
            .run(|endpoint| async move {
                NATIVE_BEFORE_BODY.set(remaining_allowance_for_diagnostics(db));
                endpoint.local_call(|| endpoint.admit_work(2)).await;
                Ok(17)
            })
    }) {
        Ok(value) => value,
        Err(RunError::Refused(_)) => 0,
        Err(error) => panic!("native entry failed: {error:?}"),
    }
}

#[test]
fn public_native_entry_derives_the_root_budget_without_an_extra_allowance() {
    let db = DatabaseImpl::default();
    let stamp = salsa::prepared_source_probe::Stamp::current(&db);
    NATIVE_BEFORE_BODY.set(None);
    assert_eq!(
        try_with_execution_budget(&db, limits(2), |_| {
            assert_eq!(native(&db), 0);
            assert_eq!(NATIVE_BEFORE_BODY.get(), Some(1));
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(
        try_with_execution_budget(&db, limits(3), |_| {
            assert_eq!(native(&db), 17);
            assert_eq!(NATIVE_BEFORE_BODY.get(), Some(2));
            assert_eq!(remaining_allowance_for_diagnostics(&db), Some(0));
            NATIVE_BEFORE_BODY.set(None);
            assert_eq!(native(&db), 17);
            assert_eq!(NATIVE_BEFORE_BODY.get(), None);
            assert_eq!(remaining_allowance_for_diagnostics(&db), Some(0));
        }),
        Ok(AttemptOutcome::Complete(()))
    );
    assert!(stamp.belongs_to(&db));
}

struct DropEvent<'a> {
    observed: &'a Observed,
    name: &'static str,
}
impl Drop for DropEvent<'_> {
    fn drop(&mut self) {
        self.observed.events.borrow_mut().push(self.name);
    }
}

#[test]
fn protected_refusal_retires_queued_child_before_parent() {
    let db = DatabaseImpl::default();
    let observed = Observed::default();
    let events = &observed;
    assert_eq!(
        try_with_execution_budget(&db, limits(10), |budget| {
            RegistryBuilder::with_budget(&db, &budget)?
                .seal()?
                .run(|endpoint| async move {
                    let parent = DropEvent {
                        observed: events,
                        name: "parent",
                    };
                    endpoint
                        .local_call(|| {
                            let child = DropEvent {
                                observed: events,
                                name: "child",
                            };
                            let _reply = endpoint.demand(move || async move {
                                drop(child);
                                Ok(())
                            })?;
                            endpoint.admit(ExecutionWork::Resource {
                                requested_bytes: usize::MAX,
                            })
                        })
                        .await;
                    drop(parent);
                    Ok(())
                })
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::RequestedAllocation))
    );
    assert_eq!(&*observed.events.borrow(), &["child", "parent"]);
}

#[derive(Default)]
struct LegacyObserver(Cell<usize>);
impl ExecutionAdmission for LegacyObserver {
    fn admit(&self, _: ExecutionWork) -> RunResult<()> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}

#[test]
fn legacy_registry_cannot_be_reused_by_a_new_sealed_root() {
    let db = DatabaseImpl::default();
    let observer = LegacyObserver::default();
    let old =
        match try_with_attempt(&db, 10, || RegistryBuilder::new(&db, &observer)?.seal()).unwrap() {
            AttemptOutcome::Complete(Ok(old)) => old,
            outcome => panic!(
                "registry setup failed: {}",
                matches!(outcome, AttemptOutcome::Incomplete(_))
            ),
        };
    let calls = observer.0.get();
    let factory = Cell::new(false);
    assert_eq!(
        try_with_execution_budget(&db, limits(0), |_| {
            assert!(matches!(
                old.run(|_| {
                    factory.set(true);
                    async { Ok(()) }
                }),
                Err(RunError::Contract(_))
            ));
            assert_eq!(observer.0.get(), calls);
            assert!(!factory.get());
            assert_eq!(remaining_allowance_for_diagnostics(&db), Some(0));
        }),
        Ok(AttemptOutcome::Complete(()))
    );
}

#[test]
fn independent_workers_keep_budget_mode_counters_and_refusals_separate() {
    let db = DatabaseImpl::default();
    let first = db.clone();
    let second = db.clone();
    let barrier = Arc::new(Barrier::new(3));
    std::thread::scope(|scope| {
        let barrier_a = barrier.clone();
        let a = scope.spawn(move || {
            try_with_execution_budget(&first, limits(0), |_| {
                barrier_a.wait();
                assert_eq!(charge(&first, 1), Err(Incomplete::Allowance));
            })
        });
        let barrier_b = barrier.clone();
        let b = scope.spawn(move || {
            try_with_execution_budget(&second, limits(2), |budget| -> RunResult<()> {
                barrier_b.wait();
                RegistryBuilder::with_budget(&second, &budget)?
                    .seal()?
                    .run(|endpoint| async move {
                        endpoint.local_call(|| endpoint.admit_work(2)).await;
                        Ok(())
                    })?;
                Ok(())
            })
        });
        let observer = LegacyObserver::default();
        assert_eq!(
            try_with_attempt(&db, 1, || -> RunResult<()> {
                barrier.wait();
                assert_eq!(
                    RegistryBuilder::new(&db, &observer)?
                        .seal()?
                        .run(|_| async { Ok(5) }),
                    Ok(5)
                );
                charge(&db, 1).unwrap();
                Ok(())
            }),
            Ok(AttemptOutcome::Complete(Ok(())))
        );
        assert!(observer.0.get() > 0);
        assert_eq!(
            a.join().unwrap(),
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        assert_eq!(b.join().unwrap(), Ok(AttemptOutcome::Complete(Ok(()))));
    });
}
