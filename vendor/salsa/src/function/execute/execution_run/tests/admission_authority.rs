use std::cell::Cell;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::rc::Rc;

use super::super::admission::Admission;
use super::super::registration::{NativeCallbackLimits, RegistryBuilder, with_native_callback};
use super::super::{Driver, ExecutionAdmission, ExecutionWork, RunContext, RunError, RunResult};
use crate::attempt_probe::{
    self, AttemptOutcome, ExecutionLimits, Incomplete, MemoReuse, StartError, try_with_attempt,
    try_with_execution_budget,
};
use crate::zalsa::ZalsaDatabase;
use crate::{Cancelled, Database, DatabaseImpl, EventKind};

const RESERVE: usize = 1_000_000;

fn limits(bytes: usize) -> ExecutionLimits {
    ExecutionLimits {
        semantic_work: 10,
        requested_bytes: bytes,
    }
}

#[derive(Default)]
struct Observer {
    calls: Cell<usize>,
    bytes: Cell<usize>,
}

impl ExecutionAdmission for Observer {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.calls.set(self.calls.get() + 1);
        if let ExecutionWork::Resource { requested_bytes }
        | ExecutionWork::Task { requested_bytes } = work
        {
            self.bytes
                .set(self.bytes.get().checked_add(requested_bytes).unwrap());
        }
        Ok(())
    }
}

fn empty_run(builder: RegistryBuilder<'_, '_>) -> RunResult<u32> {
    builder.seal()?.run(|_| async { Ok(7) })
}

#[test]
fn byte_boundaries_and_overflow_sentinel() {
    for (allowance, quote, expected) in [
        (0, 0, Ok(())),
        (0, 1, Err(Incomplete::RequestedAllocation)),
        (7, 7, Ok(())),
        (7, 8, Err(Incomplete::RequestedAllocation)),
        (usize::MAX, usize::MAX, Err(Incomplete::RequestedAllocation)),
    ] {
        let db = DatabaseImpl::default();
        let outcome = try_with_execution_budget(&db, limits(allowance), |_| {
            let context = RunContext::new(&db).unwrap();
            let result = Admission::Budget.admit(
                &context,
                ExecutionWork::Resource {
                    requested_bytes: quote,
                },
            );
            assert_eq!(result, expected.map_err(RunError::Refused));
            assert_eq!(attempt_probe::current().unwrap().reason(), expected.err());
        });
        assert_eq!(
            outcome,
            Ok(match expected {
                Ok(()) => AttemptOutcome::Complete(()),
                Err(reason) => AttemptOutcome::Incomplete(reason),
            })
        );
    }
}

#[test]
fn task_resource_and_work_use_exact_shared_counters() {
    let db = DatabaseImpl::default();
    assert_eq!(
        try_with_execution_budget(&db, limits(7), |_| {
            let context = RunContext::new(&db).unwrap();
            attempt_probe::charge(&db, 3).unwrap();
            for work in [
                ExecutionWork::Work { units: usize::MAX },
                ExecutionWork::Poll,
                ExecutionWork::Task { requested_bytes: 3 },
                ExecutionWork::Resource { requested_bytes: 4 },
            ] {
                Admission::Budget.admit(&context, work).unwrap();
            }
            assert_eq!(
                attempt_probe::remaining_allowance_for_diagnostics(&db),
                Some(7)
            );
            assert_eq!(
                Admission::Budget.admit(&context, ExecutionWork::Task { requested_bytes: 1 }),
                Err(RunError::Refused(Incomplete::RequestedAllocation))
            );
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::RequestedAllocation))
    );
}

#[test]
fn sequential_registries_do_not_renew_or_refund_bytes() {
    let db = DatabaseImpl::default();
    let observer = Observer::default();
    assert_eq!(
        try_with_attempt(&db, 10, || empty_run(
            RegistryBuilder::new(&db, &observer).unwrap()
        )),
        Ok(AttemptOutcome::Complete(Ok(7)))
    );
    let one_run = observer.bytes.get();
    assert!(one_run > 0 && observer.calls.get() > 0);
    assert_eq!(
        try_with_execution_budget(&db, limits(one_run * 2), |budget| {
            assert_eq!(
                empty_run(RegistryBuilder::with_budget(&db, &budget)?),
                Ok(7)
            );
            assert_eq!(
                empty_run(RegistryBuilder::with_budget(&db, &budget)?),
                Ok(7)
            );
            let context = RunContext::new(&db)?;
            Admission::Budget.admit(&context, ExecutionWork::Resource { requested_bytes: 1 })
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::RequestedAllocation))
    );
}

#[test]
fn legacy_constructor_and_private_driver_are_fenced_before_callouts() {
    let db = DatabaseImpl::default();
    let observer = Observer::default();
    let factory = Cell::new(false);
    assert_eq!(
        try_with_execution_budget(&db, limits(0), |budget| {
            assert!(matches!(
                RegistryBuilder::new(&db, &observer),
                Err(RunError::Contract(_))
            ));
            assert!(matches!(
                Driver::run_with_admission(&db, &observer, |_| {
                    factory.set(true);
                    async { Ok(()) }
                }),
                Err(RunError::Contract(_))
            ));
            assert_eq!(observer.calls.get(), 0);
            assert!(!factory.get());
            assert_eq!(
                attempt_probe::remaining_allowance_for_diagnostics(&db),
                Some(10)
            );
            assert_eq!(attempt_probe::current().unwrap().reason(), None);
            assert!(matches!(
                RegistryBuilder::with_budget(&db, &budget),
                Err(RunError::Refused(Incomplete::RequestedAllocation))
            ));
        }),
        Ok(AttemptOutcome::Incomplete(Incomplete::RequestedAllocation))
    );
}

#[test]
fn foreign_and_expired_contexts_cannot_dispatch_or_spend() {
    let db = DatabaseImpl::default();
    let other = DatabaseImpl::default();
    let observer = Observer::default();
    assert!(matches!(
        RegistryBuilder::new(&db, &observer),
        Err(RunError::Contract(_))
    ));
    let mut old = None;
    assert_eq!(
        try_with_attempt(&db, 10, || {
            old = Some(RunContext::new(&db).unwrap());
        }),
        Ok(AttemptOutcome::Complete(()))
    );
    assert!(matches!(
        Admission::Budget.admit(old.as_ref().unwrap(), ExecutionWork::Poll),
        Err(RunError::Contract(_))
    ));
    assert_eq!(
        try_with_execution_budget(&db, limits(1), |budget| {
            assert!(matches!(
                RegistryBuilder::with_budget(&other, &budget),
                Err(RunError::Contract(_))
            ));
            assert!(matches!(
                Admission::Budget.admit(
                    old.as_ref().unwrap(),
                    ExecutionWork::Resource { requested_bytes: 1 }
                ),
                Err(RunError::Contract(_))
            ));
            let context = RunContext::new(&db).unwrap();
            assert_eq!(
                Admission::Budget.admit(&context, ExecutionWork::Resource { requested_bytes: 1 }),
                Ok(())
            );
            assert_eq!(attempt_probe::current().unwrap().reason(), None);
        }),
        Ok(AttemptOutcome::Complete(()))
    );
    assert_eq!(observer.calls.get(), 0);
}

#[test]
fn first_refusal_and_provisional_support_preserve_allocation_cause() {
    let db = DatabaseImpl::default();
    for first in [Incomplete::Allowance, Incomplete::RequestedAllocation] {
        let mut retained = None;
        assert_eq!(
            try_with_execution_budget(
                &db,
                ExecutionLimits {
                    semantic_work: 0,
                    requested_bytes: 0
                },
                |_| {
                    let context = RunContext::new(&db).unwrap();
                    if first == Incomplete::Allowance {
                        assert_eq!(attempt_probe::charge(&db, 1), Err(first));
                    }
                    assert_eq!(
                        Admission::Budget
                            .admit(&context, ExecutionWork::Task { requested_bytes: 1 }),
                        Err(RunError::Refused(first))
                    );
                    assert_eq!(attempt_probe::charge(&db, 1), Err(first));
                    assert_eq!(
                        attempt_probe::report_incomplete(&db, Incomplete::Interrupted),
                        first
                    );
                    let support = attempt_probe::current().unwrap();
                    assert_eq!(support.reason(), Some(first));
                    assert_eq!(support.reuse(db.zalsa(), true), MemoReuse::Incomplete);
                    assert_eq!(support.reuse(db.zalsa(), false), MemoReuse::Ordinary);
                    let mut marked = support.clone();
                    marked.make_incomplete();
                    assert_eq!(marked.reuse(db.zalsa(), false), MemoReuse::Incomplete);
                    retained = Some((support, marked));
                }
            ),
            Ok(AttemptOutcome::Incomplete(first))
        );
        let (support, marked) = retained.unwrap();
        assert_eq!(support.reuse(db.zalsa(), true), MemoReuse::Stale);
        assert_eq!(support.reuse(db.zalsa(), false), MemoReuse::Ordinary);
        assert_eq!(marked.reuse(db.zalsa(), false), MemoReuse::Stale);
        assert_eq!(
            try_with_execution_budget(&db, limits(0), |_| {
                assert_eq!(support.reuse(db.zalsa(), true), MemoReuse::Stale);
                assert_eq!(attempt_probe::current().unwrap().reason(), None);
            }),
            Ok(AttemptOutcome::Complete(()))
        );
    }
}

thread_local! {
    static NATIVE_LEGACY_CALLS: Cell<usize> = const { Cell::new(0) };
}
struct NativeObserver;
impl ExecutionAdmission for NativeObserver {
    fn admit(&self, _: ExecutionWork) -> RunResult<()> {
        NATIVE_LEGACY_CALLS.set(NATIVE_LEGACY_CALLS.get() + 1);
        Ok(())
    }
}

#[crate::input]
struct NativeMode {
    #[returns(copy)]
    budget: bool,
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn native_entry(db: &dyn Database, mode: NativeMode) -> u32 {
    assert_eq!(
        try_with_execution_budget(db, limits(0), |_| ()),
        Err(StartError::ActiveQuery)
    );
    with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |entry| {
        if mode.budget(db) {
            assert!(matches!(
                RegistryBuilder::for_native_callback(db, &entry, &NativeObserver),
                Err(RunError::Contract(_))
            ));
            assert_eq!(NATIVE_LEGACY_CALLS.get(), 0);
            RegistryBuilder::for_native_callback_with_budget(db, &entry)?
                .seal()?
                .run(|endpoint| async move {
                    assert_eq!(
                        attempt_probe::remaining_allowance_for_diagnostics(db),
                        Some(9)
                    );
                    endpoint.local_call(|| endpoint.admit_work(1)).await;
                    Ok(9)
                })
        } else {
            assert!(matches!(
                RegistryBuilder::for_native_callback_with_budget(db, &entry),
                Err(RunError::Contract(_))
            ));
            empty_run(RegistryBuilder::for_native_callback(
                db,
                &entry,
                &NativeObserver,
            )?)
        }
    })
    .unwrap()
}

#[test]
fn native_entry_uses_the_same_sealed_work_owner_and_fences_legacy() {
    let db = DatabaseImpl::default();
    NATIVE_LEGACY_CALLS.set(0);
    let budget_mode = NativeMode::new(&db, true);
    let legacy_mode = NativeMode::new(&db, false);
    assert_eq!(
        try_with_execution_budget(&db, limits(RESERVE), |_| {
            assert_eq!(native_entry(&db, budget_mode), 9);
            assert_eq!(
                attempt_probe::remaining_allowance_for_diagnostics(&db),
                Some(8)
            );
            assert_eq!(native_entry(&db, budget_mode), 9);
            assert_eq!(
                attempt_probe::remaining_allowance_for_diagnostics(&db),
                Some(8)
            );
        }),
        Ok(AttemptOutcome::Complete(()))
    );
    assert_eq!(
        try_with_attempt(&db, 10, || native_entry(&db, legacy_mode)),
        Ok(AttemptOutcome::Complete(7))
    );
    assert!(NATIVE_LEGACY_CALLS.get() > 0);
}

#[test]
fn queued_children_share_work_and_ended_endpoint_is_rejected() {
    let db = DatabaseImpl::default();
    assert_eq!(
        try_with_execution_budget(&db, limits(RESERVE), |budget| -> RunResult<()> {
            let endpoint =
                RegistryBuilder::with_budget(&db, &budget)?
                    .seal()?
                    .run(|endpoint| async move {
                        let child_endpoint = endpoint.clone();
                        endpoint
                            .child_call(|| async {
                                endpoint
                                    .demand(move || async move {
                                        child_endpoint
                                            .local_call(|| child_endpoint.admit_work(4))
                                            .await;
                                        Ok(())
                                    })?
                                    .await
                            })
                            .await;
                        endpoint.local_call(|| endpoint.admit_work(6)).await;
                        endpoint.checkpoint()?.await?;
                        Ok(endpoint)
                    })?;
            assert_eq!(
                attempt_probe::remaining_allowance_for_diagnostics(&db),
                Some(0)
            );
            assert!(matches!(
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: usize::MAX
                }),
                Err(RunError::Contract("execution driver has ended"))
            ));
            assert_eq!(attempt_probe::current().unwrap().reason(), None);
            Ok(())
        }),
        Ok(AttemptOutcome::Complete(Ok(())))
    );
}

#[derive(Clone, Copy)]
enum EventAction {
    Refuse,
    Panic,
    Local,
    PendingWrite,
}
thread_local! { static ACTION: Cell<Option<EventAction>> = const { Cell::new(None) }; }

#[crate::db]
#[derive(Clone)]
struct EventDb {
    storage: crate::Storage<Self>,
}
impl Default for EventDb {
    fn default() -> Self {
        Self {
            storage: crate::Storage::new(Some(Box::new(|event| {
                if matches!(event.kind, EventKind::WillCheckCancellation)
                    && let Some(action) = ACTION.take()
                {
                    crate::with_attached_database(|db| match action {
                        EventAction::Refuse => {
                            attempt_probe::report_incomplete(db, Incomplete::RequestedAllocation);
                        }
                        EventAction::Panic => panic_any(EventPanic(41)),
                        EventAction::Local => db.cancellation_token().cancel(),
                        EventAction::PendingWrite => db.zalsa().runtime().set_cancellation_flag(),
                    })
                    .expect("registered driver owns attachment");
                }
            }))),
        }
    }
}
#[crate::db]
impl Database for EventDb {}
#[derive(Debug, PartialEq)]
struct EventPanic(u32);
struct Dropped(Rc<Cell<bool>>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.set(true);
    }
}

#[test]
fn real_cancellation_events_preserve_refusal_panic_and_cancellation_cleanup() {
    for action in [
        EventAction::Refuse,
        EventAction::Panic,
        EventAction::Local,
        EventAction::PendingWrite,
    ] {
        let db = EventDb::default();
        let dropped = Rc::new(Cell::new(false));
        let owner_dropped = dropped.clone();
        let result = catch_unwind(AssertUnwindSafe(|| {
            try_with_execution_budget(&db, limits(RESERVE), |budget| {
                RegistryBuilder::with_budget(&db, &budget)?
                    .seal()?
                    .run(|endpoint| async move {
                        let owner = Dropped(owner_dropped);
                        ACTION.set(Some(action));
                        endpoint
                            .local_call(|| endpoint.admit(ExecutionWork::Poll))
                            .await;
                        drop(owner);
                        Ok(())
                    })
            })
        }));
        match action {
            EventAction::Refuse => assert_eq!(
                result.unwrap(),
                Ok(AttemptOutcome::Incomplete(Incomplete::RequestedAllocation))
            ),
            EventAction::Panic => assert_eq!(
                result.unwrap_err().downcast_ref::<EventPanic>(),
                Some(&EventPanic(41))
            ),
            EventAction::Local => assert!(matches!(
                result.unwrap_err().downcast_ref::<Cancelled>(),
                Some(Cancelled::Local)
            )),
            EventAction::PendingWrite => {
                assert!(matches!(
                    result.unwrap_err().downcast_ref::<Cancelled>(),
                    Some(Cancelled::PendingWrite)
                ));
                db.zalsa().runtime().reset_cancellation_flag();
            }
        }
        assert!(dropped.get());
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
        assert!(attempt_probe::current().is_none());
        assert!(super::super::native_source::current_driver().is_none());
        assert!(crate::with_attached_database(|_| ()).is_none());
    }
}

#[test]
fn root_rejections_and_panic_abandonment_keep_original_contract() {
    let db = DatabaseImpl::default();
    assert_eq!(
        attempt_probe::try_with_operation(&db, || {
            assert_eq!(
                try_with_execution_budget(&db, limits(0), |_| ()),
                Err(StartError::ActiveOperation)
            );
        }),
        Ok(())
    );
    assert_eq!(
        try_with_execution_budget(&db, limits(0), |_| {
            assert_eq!(
                try_with_attempt(&db, 0, || ()),
                Err(StartError::NestedAttempt)
            );
            assert_eq!(
                try_with_execution_budget(&db, limits(0), |_| ()),
                Err(StartError::NestedAttempt)
            );
        }),
        Ok(AttemptOutcome::Complete(()))
    );
    let mut support = None;
    assert!(
        catch_unwind(AssertUnwindSafe(|| try_with_execution_budget(
            &db,
            limits(0),
            |_| {
                support = attempt_probe::current();
                panic_any(EventPanic(42));
            }
        )))
        .is_err()
    );
    assert_eq!(support.unwrap().reuse(db.zalsa(), true), MemoReuse::Stale);
    assert!(attempt_probe::current().is_none());
    assert_eq!(
        try_with_attempt(&db, 0, || ()),
        Ok(AttemptOutcome::Complete(()))
    );
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn a_shared_storage_worker_cannot_reuse_another_roots_support() {
    let db = DatabaseImpl::default();
    let other = db.clone();
    assert_eq!(
        try_with_execution_budget(&db, limits(0), |_| {
            let support = attempt_probe::current().unwrap();
            std::thread::spawn(move || {
                assert_eq!(support.admission_is_budget(other.zalsa()), None);
                assert_eq!(
                    try_with_execution_budget(&other, limits(0), |_| {
                        assert!(matches!(
                            Admission::native_budget(&other, &support),
                            Err(RunError::Contract(_))
                        ));
                        assert_eq!(attempt_probe::current().unwrap().reason(), None);
                    }),
                    Ok(AttemptOutcome::Complete(()))
                );
            })
            .join()
            .unwrap();
            assert_eq!(attempt_probe::current().unwrap().reason(), None);
        }),
        Ok(AttemptOutcome::Complete(()))
    );
}
