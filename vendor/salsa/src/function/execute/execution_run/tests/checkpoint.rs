use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;

use super::super::progress::Checkpoint;
use super::super::{
    ActivePoll, Driver, Endpoint, ExecutionAdmission, ExecutionWork, PendingTask, Queue, RunError,
    RunOperation, RunResult,
};
use super::{fixpoint, observation};
use crate::DatabaseImpl;
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, try_with_attempt};
use crate::function::{Configuration, IngredientImpl};
use crate::zalsa::ZalsaDatabase;

#[derive(Clone, Copy)]
enum Boundary {
    Issue,
    Resume,
}

#[derive(Clone, Copy)]
enum Fault {
    Refuse,
    Panic,
    #[cfg(not(feature = "shuttle"))]
    Cancel,
}

struct Admission<'db> {
    db: &'db DatabaseImpl,
    events: RefCell<Vec<ExecutionWork>>,
    polls: Cell<usize>,
    fault: Option<(Boundary, Fault)>,
}

impl<'db> Admission<'db> {
    fn new(db: &'db DatabaseImpl, fault: Option<(Boundary, Fault)>) -> Self {
        Self {
            db,
            events: RefCell::default(),
            polls: Cell::new(0),
            fault,
        }
    }
}

impl ExecutionAdmission for Admission<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        assert!(self.db.zalsa_local().active_query().is_none());
        self.events.borrow_mut().push(work);
        let boundary = match work {
            ExecutionWork::Work { .. } => Some(Boundary::Issue),
            ExecutionWork::Poll => {
                let ordinal = self.polls.get();
                self.polls.set(ordinal + 1);
                // The parent demands a child, whose first poll issues the checkpoint.
                (ordinal == 2).then_some(Boundary::Resume)
            }
            _ => None,
        };
        if let (Some(boundary), Some((target, fault))) = (boundary, self.fault)
            && matches!(
                (boundary, target),
                (Boundary::Issue, Boundary::Issue) | (Boundary::Resume, Boundary::Resume)
            )
        {
            match fault {
                Fault::Refuse => return Err(RunError::Refused(Incomplete::Allowance)),
                Fault::Panic => panic!("checkpoint admission panic"),
                #[cfg(not(feature = "shuttle"))]
                Fault::Cancel => self.db.zalsa().runtime().set_cancellation_flag(),
            }
        }
        Ok(())
    }
}

fn task_identity(endpoint: &Endpoint<'_, '_>) -> Rc<Cell<bool>> {
    endpoint
        .queue
        .active_poll
        .borrow()
        .as_ref()
        .expect("the driver is polling this task")
        .identity
        .wanted
        .clone()
}

fn enter_operation<'db, C: Configuration>(
    _ingredient: &IngredientImpl<C>,
    endpoint: &Endpoint<'_, 'db>,
) -> RunResult<RunOperation<'db>> {
    RunOperation::enter::<C>(endpoint.context.clone())
}

#[test]
fn local_child_local_preserves_task_and_operation_ownership() {
    let db = DatabaseImpl::default();
    let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
    let admission = Admission::new(&db, None);
    let (outcome, observed) = observation::collect(|| {
        try_with_attempt(&db, 100, || {
            Driver::run_with_admission(&db, &admission, |endpoint| async move {
                let operation = enter_operation(ingredient, &endpoint)?;
                let parent = task_identity(&endpoint);
                endpoint.checkpoint()?.await?;
                assert!(Rc::ptr_eq(&parent, &task_identity(&endpoint)));
                assert!(operation.is_current());
                let child_endpoint = endpoint.clone();
                let parent_identity = parent.clone();
                let ordinal = operation.ordinal;
                endpoint
                    .demand(move || async move {
                        let child = task_identity(&child_endpoint);
                        assert!(!Rc::ptr_eq(&parent_identity, &child));
                        child_endpoint.checkpoint()?.await?;
                        assert!(Rc::ptr_eq(&child, &task_identity(&child_endpoint)));
                        assert_eq!(
                            child_endpoint.context.operations.borrow().last(),
                            Some(&ordinal)
                        );
                        child_endpoint.check_completion()
                    })?
                    .await?;
                endpoint.checkpoint()?.await?;
                assert!(Rc::ptr_eq(&parent, &task_identity(&endpoint)));
                assert!(operation.is_current());
                endpoint.check_completion()
            })
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    assert_eq!(observed.max_active_polls, 1);
    assert_eq!(observed.polls, 6);
    assert_eq!(
        admission
            .events
            .borrow()
            .iter()
            .filter(|work| matches!(work, ExecutionWork::Task { .. }))
            .count(),
        2
    );
}

#[test]
fn repeated_local_progress_adds_no_task_or_resource_admission() {
    eprintln!(
        "checkpoint layout: Queue={} PendingTask={} Checkpoint={} ActivePoll={}",
        size_of::<Queue<'_>>(),
        size_of::<PendingTask<'_>>(),
        size_of::<Checkpoint<'_, '_, '_>>(),
        size_of::<ActivePoll>(),
    );
    let mut baseline = None;
    for pauses in [0, 1, 32] {
        let db = DatabaseImpl::default();
        let admission = Admission::new(&db, None);
        let admission_ref = &admission;
        let (outcome, observed) = observation::collect(|| {
            try_with_attempt(&db, 100, || {
                Driver::run_with_admission(&db, &admission, |endpoint| async move {
                    for _ in 0..pauses {
                        endpoint.checkpoint()?.await?;
                    }
                    let count = admission_ref.events.borrow().len();
                    endpoint.check_completion()?;
                    assert_eq!(admission_ref.events.borrow().len(), count);
                    Ok(())
                })
            })
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
        assert_eq!(observed.max_active_polls, 1);
        assert_eq!(observed.polls, pauses + 1);
        let events = admission.events.borrow();
        assert_eq!(
            events
                .iter()
                .filter(|work| matches!(work, ExecutionWork::Work { units: 1 }))
                .count(),
            pauses
        );
        let allocations = events
            .iter()
            .copied()
            .filter(|work| {
                matches!(
                    work,
                    ExecutionWork::Task { .. } | ExecutionWork::Resource { .. }
                )
            })
            .collect::<Vec<_>>();
        if let Some(baseline) = &baseline {
            assert_eq!(&allocations, baseline);
        } else {
            baseline = Some(allocations);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Malformed {
    BarePending,
    MultipleChildren,
    ChildAndCheckpoint,
    ReadyWithRequest,
    ManualRepoll,
    DropBeforeResume,
    IgnoreResume,
    ReplaceResume,
    CompleteWithReceipt,
}

#[test]
fn malformed_polls_cannot_create_or_reuse_acknowledgements() {
    for case in [
        Malformed::BarePending,
        Malformed::MultipleChildren,
        Malformed::ChildAndCheckpoint,
        Malformed::ReadyWithRequest,
        Malformed::ManualRepoll,
        Malformed::DropBeforeResume,
        Malformed::IgnoreResume,
        Malformed::ReplaceResume,
        Malformed::CompleteWithReceipt,
    ] {
        let db = DatabaseImpl::default();
        let child_ran = Cell::new(false);
        let outcome = try_with_attempt(&db, 100, || {
            let child_ran = &child_ran;
            let result = Driver::run(&db, |endpoint| async move {
                let mut checkpoint = None;
                let mut issued = false;
                let mut children = Vec::new();
                poll_fn(|cx| {
                    if matches!(case, Malformed::BarePending) {
                        return Poll::Pending;
                    }
                    if matches!(case, Malformed::MultipleChildren) {
                        for _ in 0..2 {
                            children.push(endpoint.demand(move || async move {
                                child_ran.set(true);
                                Ok(())
                            })?);
                        }
                        return Poll::Pending;
                    }
                    if !issued {
                        checkpoint = Some(endpoint.checkpoint()?);
                        let future = checkpoint.as_mut().expect("checkpoint was just created");
                        assert!(Pin::new(&mut *future).poll(cx).is_pending());
                        issued = true;
                        match case {
                            Malformed::ManualRepoll => return Pin::new(future).poll(cx),
                            Malformed::ReadyWithRequest => return Poll::Ready(Ok(())),
                            Malformed::ChildAndCheckpoint => {
                                children.push(endpoint.demand(move || async move {
                                    child_ran.set(true);
                                    Ok(())
                                })?);
                            }
                            Malformed::DropBeforeResume => drop(checkpoint.take()),
                            _ => {}
                        }
                        return Poll::Pending;
                    }
                    match case {
                        Malformed::DropBeforeResume => Poll::Ready(Ok(())),
                        Malformed::IgnoreResume => Poll::Pending,
                        Malformed::ReplaceResume => {
                            drop(checkpoint.take());
                            let mut replacement = endpoint.checkpoint()?;
                            let result = Pin::new(&mut replacement).poll(cx);
                            assert!(matches!(result, Poll::Ready(Err(RunError::Contract(_)))));
                            result
                        }
                        Malformed::CompleteWithReceipt => Poll::Ready(endpoint.check_completion()),
                        _ => panic!("malformed issuing poll was accepted: {case:?}"),
                    }
                })
                .await
            });
            assert!(matches!(result, Err(RunError::Contract(_))), "{case:?}");
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert!(!child_ran.get(), "{case:?}");
    }
}

#[test]
fn captured_checkpoint_cannot_move_to_a_later_poll() {
    let db = DatabaseImpl::default();
    let outcome = try_with_attempt(&db, 100, || {
        let result = Driver::run(&db, |endpoint| async move {
            let checkpoint = endpoint.checkpoint()?;
            endpoint.demand(|| async { Ok(()) })?.await?;
            checkpoint.await
        });
        assert!(matches!(result, Err(RunError::Contract(_))));
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
}

#[test]
fn completed_checkpoint_cannot_acknowledge_twice() {
    let db = DatabaseImpl::default();
    let outcome = try_with_attempt(&db, 100, || {
        let result = Driver::run(&db, |endpoint| async move {
            let mut checkpoint = endpoint.checkpoint()?;
            (&mut checkpoint).await?;
            poll_fn(|cx| Pin::new(&mut checkpoint).poll(cx)).await
        });
        assert!(matches!(result, Err(RunError::Contract(_))));
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
}

struct DropOrder<'a>(&'a RefCell<Vec<&'static str>>, &'static str);

impl Drop for DropOrder<'_> {
    fn drop(&mut self) {
        self.0.borrow_mut().push(self.1);
    }
}

fn exercise_fault(boundary: Boundary, fault: Fault) {
    let db = DatabaseImpl::default();
    let admission = Admission::new(&db, Some((boundary, fault)));
    let drops = RefCell::new(Vec::new());
    let resumed = Cell::new(false);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let drops = &drops;
        let resumed = &resumed;
        try_with_attempt(&db, 100, || {
            Driver::run_with_admission(&db, &admission, |endpoint| async move {
                let _parent = DropOrder(drops, "parent");
                let child_endpoint = endpoint.clone();
                endpoint
                    .demand(move || async move {
                        let _child = DropOrder(drops, "child");
                        child_endpoint.checkpoint()?.await?;
                        resumed.set(true);
                        child_endpoint.check_completion()
                    })?
                    .await
            })
        })
    }));
    #[cfg(not(feature = "shuttle"))]
    db.zalsa().runtime().reset_cancellation_flag();
    assert_eq!(*drops.borrow(), ["child", "parent"]);
    assert!(!resumed.get());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    match fault {
        Fault::Refuse => assert!(matches!(
            outcome,
            Ok(Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)))
        )),
        Fault::Panic => {
            let payload = outcome.expect_err("the admission panic is preserved");
            assert_eq!(
                payload.downcast_ref::<&str>(),
                Some(&"checkpoint admission panic")
            );
        }
        #[cfg(not(feature = "shuttle"))]
        Fault::Cancel => {
            let payload = outcome.expect_err("revision cancellation still unwinds");
            assert!(matches!(
                payload.downcast_ref::<crate::Cancelled>(),
                Some(crate::Cancelled::PendingWrite)
            ));
        }
    }
    assert_eq!(
        try_with_attempt(&db, 100, || Driver::run(&db, |endpoint| async move {
            endpoint.checkpoint()?.await?;
            endpoint.check_completion()
        })),
        Ok(AttemptOutcome::Complete(Ok(())))
    );
}

#[test]
fn checkpoint_refusal_and_panic_drop_children_before_parents() {
    for boundary in [Boundary::Issue, Boundary::Resume] {
        for fault in [Fault::Refuse, Fault::Panic] {
            exercise_fault(boundary, fault);
        }
    }
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn checkpoint_cancellation_preserves_payload_and_cleanup() {
    for boundary in [Boundary::Issue, Boundary::Resume] {
        exercise_fault(boundary, Fault::Cancel);
    }
}

#[test]
fn waiting_owner_destructor_panic_still_drains_its_parent() {
    struct PanickingOwner<'a>(&'a RefCell<Vec<&'static str>>);

    impl Drop for PanickingOwner<'_> {
        fn drop(&mut self) {
            self.0.borrow_mut().push("child");
            panic!("waiting checkpoint owner panic");
        }
    }

    let db = DatabaseImpl::default();
    let admission = Admission::new(&db, Some((Boundary::Resume, Fault::Refuse)));
    let drops = RefCell::new(Vec::new());
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let drops = &drops;
        try_with_attempt(&db, 100, || {
            Driver::run_with_admission(&db, &admission, |endpoint| async move {
                let _parent = DropOrder(drops, "parent");
                let child_endpoint = endpoint.clone();
                endpoint
                    .demand(move || async move {
                        let _owner = PanickingOwner(drops);
                        child_endpoint.checkpoint()?.await
                    })?
                    .await
            })
        })
    }));
    let payload = outcome.expect_err("the waiting owner's destructor panics");
    assert_eq!(
        payload.downcast_ref::<&str>(),
        Some(&"waiting checkpoint owner panic")
    );
    assert_eq!(*drops.borrow(), ["child", "parent"]);
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
}

#[test]
fn completion_barrier_preserves_refusal_without_work_admission() {
    let db = DatabaseImpl::default();
    let admission = Admission::new(&db, None);
    let db_ref = &db;
    let admission_ref = &admission;
    let outcome = try_with_attempt(&db, 100, || {
        Driver::run_with_admission(&db, &admission, |endpoint| async move {
            let count = admission_ref.events.borrow().len();
            attempt_probe::report_incomplete(db_ref, Incomplete::Allowance);
            let result = endpoint.check_completion();
            assert_eq!(result, Err(RunError::Refused(Incomplete::Allowance)));
            assert_eq!(admission_ref.events.borrow().len(), count);
            result
        })
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
}

#[crate::db]
#[derive(Clone)]
struct CallbackDb {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl crate::Database for CallbackDb {}

fn callback_db() -> (CallbackDb, Arc<AtomicBool>) {
    let armed = Arc::new(AtomicBool::new(false));
    let callback_armed = armed.clone();
    let db = CallbackDb {
        storage: crate::Storage::new(Some(Box::new(move |event| {
            if matches!(event.kind, crate::EventKind::WillCheckCancellation)
                && callback_armed.swap(false, Ordering::SeqCst)
            {
                crate::with_attached_database(|db| {
                    attempt_probe::report_incomplete(db, Incomplete::Allowance);
                })
                .expect("the run's database is attached");
            }
        }))),
    };
    (db, armed)
}

#[test]
fn completion_barrier_observes_cancellation_callback_refusal() {
    let (db, armed) = callback_db();
    let outcome = try_with_attempt(&db, 100, || {
        crate::attach(&db, || {
            Driver::run(&db, |endpoint| async move {
                armed.store(true, Ordering::SeqCst);
                let result = endpoint.check_completion();
                assert_eq!(result, Err(RunError::Refused(Incomplete::Allowance)));
                result
            })
        })
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
}

#[test]
fn checkpoint_acknowledgment_observes_cancellation_callback_refusal() {
    let (db, armed) = callback_db();
    let outcome = try_with_attempt(&db, 100, || {
        crate::attach(&db, || {
            Driver::run(&db, |endpoint| async move {
                let mut checkpoint = endpoint.checkpoint()?;
                let mut issuing = true;
                poll_fn(|cx| {
                    if !issuing {
                        armed.store(true, Ordering::SeqCst);
                    }
                    issuing = false;
                    let result = Pin::new(&mut checkpoint).poll(cx);
                    if result.is_ready() {
                        assert_eq!(
                            result,
                            Poll::Ready(Err(RunError::Refused(Incomplete::Allowance)))
                        );
                    }
                    result
                })
                .await
            })
        })
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
}

#[test]
fn ended_endpoint_cannot_checkpoint_or_affect_a_later_run() {
    let db = DatabaseImpl::default();
    let expired = try_with_attempt(&db, 100, || {
        Driver::run(&db, |endpoint| async move {
            endpoint.check_completion()?;
            Ok(endpoint)
        })
    });
    let Ok(AttemptOutcome::Complete(Ok(expired))) = expired else {
        panic!("the first task returns its endpoint");
    };
    assert!(matches!(expired.checkpoint(), Err(RunError::Contract(_))));
    assert!(matches!(
        expired.check_completion(),
        Err(RunError::Contract(_))
    ));
    assert_eq!(
        try_with_attempt(&db, 100, || Driver::run(&db, |endpoint| async move {
            assert!(matches!(expired.checkpoint(), Err(RunError::Contract(_))));
            assert!(matches!(
                expired.check_completion(),
                Err(RunError::Contract(_))
            ));
            endpoint.checkpoint()?.await?;
            endpoint.check_completion()
        })),
        Ok(AttemptOutcome::Complete(Ok(())))
    );
}

#[test]
fn poll_generation_exhaustion_cannot_reuse_a_receipt() {
    let db = DatabaseImpl::default();
    let resumed = Cell::new(false);
    let outcome = try_with_attempt(&db, 100, || {
        let resumed = &resumed;
        let result = Driver::run(&db, |endpoint| async move {
            endpoint.queue.last_poll_generation.set(usize::MAX);
            endpoint.checkpoint()?.await?;
            resumed.set(true);
            Ok(())
        });
        assert!(matches!(result, Err(RunError::Contract(_))));
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    assert!(!resumed.get());
}
