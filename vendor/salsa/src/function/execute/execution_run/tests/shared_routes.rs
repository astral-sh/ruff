use std::cell::{Cell, RefCell};
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::rc::Rc;

use super::super::registration::{
    CallableRouteProvider, NativeCallbackLimits, RegistryBuilder, TaskEndpoint,
    with_native_callback,
};
use super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult, callback};
use super::Node;
use crate::attempt_probe::{
    self, AttemptOutcome, ExecutionLimits, Incomplete, try_with_attempt, try_with_execution_budget,
    try_with_metered_execution_budget,
};
use crate::function::Configuration;
use crate::zalsa::ZalsaDatabase;
use crate::{Cancelled, Cycle, Database, DatabaseImpl, Id};

const RESERVE: usize = 1_000_000;

fn limits(work: usize, bytes: usize) -> ExecutionLimits {
    ExecutionLimits {
        semantic_work: work,
        requested_bytes: bytes,
    }
}

fn registration_bytes(db: &dyn Database) -> usize {
    let receipt = try_with_metered_execution_budget(db, limits(0, RESERVE), |budget| {
        RegistryBuilder::with_budget(db, &budget).map(drop)
    })
    .unwrap();
    assert_eq!(receipt.outcome, AttemptOutcome::Complete(Ok(())));
    assert_eq!(receipt.usage.semantic_work, 0);
    receipt.usage.requested_bytes
}

#[repr(C, align(2))]
struct RcAllocation<T> {
    _strong: Cell<usize>,
    _weak: Cell<usize>,
    _value: T,
}

#[derive(Default)]
#[repr(align(64))]
struct Aligned {
    _value: u8,
}

fn check_layout<T: Default>() {
    let db = DatabaseImpl::default();
    let bytes = registration_bytes(&db) + size_of::<RcAllocation<T>>();
    let work = size_of::<T>() * 2 + 17;
    let made = Cell::new(0);
    let receipt =
        try_with_metered_execution_budget(&db, limits(work, bytes), |budget| -> RunResult<()> {
            let registry = RegistryBuilder::with_budget(&db, &budget)?;
            let value = registry.allocate_shared_metadata(work, || {
                made.set(made.get() + 1);
                T::default()
            })?;
            assert_eq!(Rc::as_ptr(&value).addr() % align_of::<T>(), 0);
            drop(value);
            Ok(())
        })
        .unwrap();
    assert_eq!(receipt.outcome, AttemptOutcome::Complete(Ok(())));
    assert_eq!(receipt.usage.semantic_work, work);
    assert_eq!(receipt.usage.requested_bytes, bytes);
    assert_eq!(made.get(), 1);
}

#[test]
fn aligned_storage_and_lifecycle_work_are_charged_once() {
    check_layout::<()>();
    check_layout::<u8>();
    check_layout::<Aligned>();
}

#[test]
fn work_and_byte_refusal_precede_construction_and_keep_accepted_charges() {
    for (work, extra_bytes, reason, accepted_work) in [
        (16, size_of::<RcAllocation<u8>>(), Incomplete::Allowance, 0),
        (
            17,
            size_of::<RcAllocation<u8>>() - 1,
            Incomplete::RequestedAllocation,
            17,
        ),
    ] {
        let db = DatabaseImpl::default();
        let base = registration_bytes(&db);
        let made = Cell::new(false);
        let receipt =
            try_with_metered_execution_budget(&db, limits(work, base + extra_bytes), |budget| {
                let registry = RegistryBuilder::with_budget(&db, &budget)?;
                registry.allocate_shared_metadata(17, || {
                    made.set(true);
                    7_u8
                })
            })
            .unwrap();
        assert_eq!(receipt.outcome, AttemptOutcome::Incomplete(reason));
        assert_eq!(receipt.usage.semantic_work, accepted_work);
        assert_eq!(receipt.usage.requested_bytes, base);
        assert!(!made.get());
    }
}

#[test]
fn zero_lifecycle_work_is_rejected_without_spending_or_construction() {
    let db = DatabaseImpl::default();
    let base = registration_bytes(&db);
    let made = Cell::new(false);
    let receipt =
        try_with_metered_execution_budget(&db, limits(17, RESERVE), |budget| -> RunResult<()> {
            let registry = RegistryBuilder::with_budget(&db, &budget)?;
            assert!(matches!(
                registry.allocate_shared_metadata(0, || made.set(true)),
                Err(RunError::Contract(_))
            ));
            Ok(())
        })
        .unwrap();
    assert_eq!(receipt.outcome, AttemptOutcome::Complete(Ok(())));
    assert_eq!(receipt.usage.semantic_work, 0);
    assert_eq!(receipt.usage.requested_bytes, base);
    assert!(!made.get());
}

#[derive(Clone, Copy)]
enum Action {
    RefuseWork,
    RefuseResource,
    Panic,
    Cancel,
}

struct Observer<'db> {
    db: &'db dyn Database,
    calls: RefCell<Vec<ExecutionWork>>,
    action: Cell<Option<Action>>,
    remaining_at_work: Cell<Option<usize>>,
}

impl<'db> Observer<'db> {
    fn new(db: &'db dyn Database) -> Self {
        Self {
            db,
            calls: RefCell::new(Vec::new()),
            action: Cell::new(None),
            remaining_at_work: Cell::new(None),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ObserverPanic;

impl ExecutionAdmission for Observer<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.calls.borrow_mut().push(work);
        if matches!(work, ExecutionWork::Work { .. }) {
            self.remaining_at_work
                .set(attempt_probe::remaining_allowance_for_diagnostics(self.db));
        }
        match (self.action.get(), work) {
            (Some(Action::RefuseWork), ExecutionWork::Work { .. })
            | (Some(Action::RefuseResource), ExecutionWork::Resource { .. }) => {
                self.action.set(None);
                Err(RunError::Refused(Incomplete::RequestedAllocation))
            }
            (Some(Action::Panic), ExecutionWork::Resource { .. }) => {
                self.action.set(None);
                panic_any(ObserverPanic);
            }
            (Some(Action::Cancel), ExecutionWork::Resource { .. }) => {
                self.action.set(None);
                self.db.cancellation_token().cancel();
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[test]
fn legacy_observer_runs_after_the_shared_work_debit() {
    let db = DatabaseImpl::default();
    let observer = Observer::new(&db);
    assert_eq!(
        try_with_attempt(&db, 17, || -> RunResult<()> {
            let registry = RegistryBuilder::new(&db, &observer)?;
            observer.calls.borrow_mut().clear();
            let value = registry.allocate_shared_metadata(17, || 7_u8)?;
            assert_eq!(*value, 7);
            Ok(())
        }),
        Ok(AttemptOutcome::Complete(Ok(())))
    );
    assert_eq!(observer.remaining_at_work.get(), Some(0));
    assert_eq!(
        &*observer.calls.borrow(),
        &[
            ExecutionWork::Work { units: 17 },
            ExecutionWork::Resource {
                requested_bytes: size_of::<RcAllocation<u8>>()
            },
        ]
    );
}

#[test]
fn observer_refusal_and_native_unwind_precede_construction() {
    for action in [
        Action::RefuseWork,
        Action::RefuseResource,
        Action::Panic,
        Action::Cancel,
    ] {
        let db = DatabaseImpl::default();
        let observer = Observer::new(&db);
        let made = Cell::new(false);
        let result = catch_unwind(AssertUnwindSafe(|| {
            try_with_attempt(&db, 17, || {
                let registry = RegistryBuilder::new(&db, &observer)?;
                observer.action.set(Some(action));
                let result = registry.allocate_shared_metadata(17, || {
                    made.set(true);
                    7_u8
                });
                if matches!(action, Action::RefuseWork | Action::RefuseResource) {
                    assert_eq!(
                        result,
                        Err(RunError::Refused(Incomplete::RequestedAllocation))
                    );
                    assert_eq!(
                        registry.allocate_shared_metadata(1, || made.set(true)),
                        Err(RunError::Refused(Incomplete::RequestedAllocation))
                    );
                }
                result
            })
        }));
        match action {
            Action::RefuseWork | Action::RefuseResource => assert_eq!(
                result.unwrap(),
                Ok(AttemptOutcome::Incomplete(Incomplete::RequestedAllocation))
            ),
            Action::Panic => assert_eq!(
                result.unwrap_err().downcast_ref::<ObserverPanic>(),
                Some(&ObserverPanic)
            ),
            Action::Cancel => assert!(matches!(
                result.unwrap_err().downcast_ref::<Cancelled>(),
                Some(Cancelled::Local)
            )),
        }
        assert_eq!(observer.remaining_at_work.get(), Some(0));
        assert!(!made.get());
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(attempt_probe::current().is_none());
        assert!(db.zalsa_local().active_query().is_none());
    }
}

#[test]
fn expired_registry_cannot_construct_metadata_or_spend_another_attempt() {
    let db = DatabaseImpl::default();
    let observer = Observer::new(&db);
    let Ok(AttemptOutcome::Complete(Ok(registry))) =
        try_with_attempt(&db, 17, || RegistryBuilder::new(&db, &observer))
    else {
        panic!("registration did not complete")
    };
    let calls = observer.calls.borrow().len();
    let made = Cell::new(false);
    assert!(matches!(
        registry.allocate_shared_metadata(17, || made.set(true)),
        Err(RunError::Contract(_))
    ));
    assert_eq!(
        try_with_attempt(&db, 17, || {
            assert!(matches!(
                registry.allocate_shared_metadata(17, || made.set(true)),
                Err(RunError::Contract(_))
            ));
            assert_eq!(
                attempt_probe::remaining_allowance_for_diagnostics(&db),
                Some(17)
            );
        }),
        Ok(AttemptOutcome::Complete(()))
    );
    assert_eq!(observer.calls.borrow().len(), calls);
    assert!(!made.get());
}

#[derive(Debug)]
struct Metadata<'a> {
    drops: &'a Cell<usize>,
    journal: &'a RefCell<Vec<&'static str>>,
}

impl Drop for Metadata<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
        self.journal.borrow_mut().push("metadata");
    }
}

#[test]
fn post_construction_cancellation_drops_prepaid_metadata_once() {
    let db = DatabaseImpl::default();
    let drops = Cell::new(0);
    let journal = RefCell::new(Vec::with_capacity(1));
    let result = catch_unwind(AssertUnwindSafe(|| {
        try_with_execution_budget(&db, limits(64, RESERVE), |budget| {
            let registry = RegistryBuilder::with_budget(&db, &budget)?;
            registry.allocate_shared_metadata(64, || {
                db.cancellation_token().cancel();
                Metadata {
                    drops: &drops,
                    journal: &journal,
                }
            })
        })
    }));
    assert!(matches!(
        result.unwrap_err().downcast_ref::<Cancelled>(),
        Some(Cancelled::Local)
    ));
    assert_eq!(drops.get(), 1);
    assert_eq!(&*journal.borrow(), &["metadata"]);
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(attempt_probe::current().is_none());
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn native_metadata(db: &dyn Database, node: Node) -> u32 {
    let value = node.seed(db);
    with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |entry| {
        let registry = RegistryBuilder::for_native_callback_with_budget(db, &entry)?;
        let shared = registry.allocate_shared_metadata(7, || value)?;
        Ok(*shared)
    })
    .unwrap()
}

#[test]
fn native_callback_spends_the_enclosing_budget() {
    let db = DatabaseImpl::default();
    let node = Node::new(&db, None, 7);
    let receipt =
        try_with_metered_execution_budget(&db, limits(7, RESERVE), |_| native_metadata(&db, node))
            .unwrap();
    assert_eq!(receipt.outcome, AttemptOutcome::Complete(7));
    assert_eq!(receipt.usage.semantic_work, 7);
    assert_eq!(
        receipt.usage.requested_bytes,
        registration_bytes(&db) + size_of::<RcAllocation<u32>>()
    );
}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn retained_value(db: &dyn Database, node: Node) -> u32 {
    node.seed(db)
}

struct MetadataProvider<'run> {
    metadata: Rc<Metadata<'run>>,
}

impl<'run, 'db: 'run, C> CallableRouteProvider<'run, 'db, C> for MetadataProvider<'run>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    fixture_native_value!(callable, 'run, 'db, C, 1);

    async fn body<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Database,
        input: Node,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        assert_eq!(self.metadata.drops.get(), 0);
        Ok(input.seed(db))
    }

    async fn initial<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _id: Id,
        _input: Node,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::Contract("metadata fixture has no cycle"))
    }

    async fn recover<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Database,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: Node,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::Contract("metadata fixture has no cycle"))
    }
}

#[test]
fn partial_registration_retires_one_shared_allocation() {
    for refuse_binding in [false, true] {
        let db = DatabaseImpl::default();
        let observer = Observer::new(&db);
        let drops = Cell::new(0);
        let journal = RefCell::new(Vec::with_capacity(1));
        let outcome = try_with_attempt(&db, 64, || {
            let mut registry = RegistryBuilder::new(&db, &observer)?;
            let route = registry.reserve_callable(
                &db as &dyn Database,
                retained_value::fn_ingredient_(&db, db.zalsa()),
            )?;
            let metadata = registry.allocate_shared_metadata(64, || Metadata {
                drops: &drops,
                journal: &journal,
            })?;
            if refuse_binding {
                observer.action.set(Some(Action::RefuseResource));
            }
            let result = registry.bind_callable(&route, MetadataProvider { metadata });
            assert_eq!(drops.get(), usize::from(refuse_binding));
            drop(registry);
            assert_eq!(drops.get(), 1);
            result
        });
        assert_eq!(
            outcome,
            Ok(if refuse_binding {
                AttemptOutcome::Incomplete(Incomplete::RequestedAllocation)
            } else {
                AttemptOutcome::Complete(Ok(()))
            })
        );
        assert_eq!(drops.get(), 1);
        assert_eq!(&*journal.borrow(), &["metadata"]);
    }
}

struct Child<'run> {
    drops: &'run Cell<usize>,
    journal: &'run RefCell<Vec<&'static str>>,
}

impl Drop for Child<'_> {
    fn drop(&mut self) {
        assert_eq!(self.drops.get(), 0);
        self.journal.borrow_mut().push("child");
    }
}

#[derive(Clone, Copy)]
enum Exit {
    Success,
    Refusal,
    Native,
}

#[test]
fn providers_retain_shared_metadata_until_queued_children_drain() {
    for exit in [Exit::Success, Exit::Refusal, Exit::Native] {
        let db = DatabaseImpl::default();
        let drops = Cell::new(0);
        let journal = RefCell::new(Vec::with_capacity(2));
        let polled = Cell::new(false);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            try_with_execution_budget(&db, limits(RESERVE, RESERVE), |budget| {
                let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
                let route = registry.reserve_callable(
                    &db as &dyn Database,
                    retained_value::fn_ingredient_(&db, db.zalsa()),
                )?;
                let metadata = registry.allocate_shared_metadata(64, || Metadata {
                    drops: &drops,
                    journal: &journal,
                })?;
                registry.bind_callable(&route, MetadataProvider { metadata })?;
                let child = Child {
                    drops: &drops,
                    journal: &journal,
                };
                let polled = &polled;
                registry.seal()?.run(move |endpoint| async move {
                    let inner = endpoint.inner.clone();
                    Ok(callback::child_call(&inner, move || async move {
                        let reply = endpoint.demand(move || async move {
                            let _child = child;
                            polled.set(true);
                            Ok(())
                        })?;
                        drop(endpoint);
                        match exit {
                            Exit::Success => reply.await,
                            Exit::Refusal => Err(RunError::Refused(Incomplete::Allowance)),
                            Exit::Native => panic_any(ObserverPanic),
                        }
                    })
                    .await)
                })
            })
        }));
        match exit {
            Exit::Success => assert_eq!(outcome.unwrap(), Ok(AttemptOutcome::Complete(Ok(())))),
            Exit::Refusal => assert_eq!(
                outcome.unwrap(),
                Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
            ),
            Exit::Native => assert_eq!(
                outcome.unwrap_err().downcast_ref::<ObserverPanic>(),
                Some(&ObserverPanic)
            ),
        }
        assert_eq!(drops.get(), 1);
        assert_eq!(&*journal.borrow(), &["child", "metadata"]);
        assert_eq!(polled.get(), matches!(exit, Exit::Success));
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
    }
}
