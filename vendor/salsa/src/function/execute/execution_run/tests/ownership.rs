use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};

use super::super::registration::RegistryBuilder;
use super::super::{ExecutionWork, RunError, RunResult, Unrestricted};
use crate::attempt_probe::registration_test_support::{EntryControl, EntryKind, paused_entry};
use crate::attempt_probe::{
    self, AttemptOutcome, Incomplete, try_with_attempt, try_with_operation,
};
use crate::sync::atomic::Ordering;
use crate::zalsa::ZalsaDatabase;
use crate::{Cancelled, Database, DatabaseImpl};

thread_local! {
    static ORDINARY_BODY_ENTERED: Cell<bool> = const { Cell::new(false) };
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn ordinary_entry(_db: &dyn Database) -> u32 {
    ORDINARY_BODY_ENTERED.set(true);
    7
}

fn with_worker<T>(db: &DatabaseImpl, kind: EntryKind, body: impl FnOnce(&EntryControl) -> T) -> T {
    std::thread::scope(|threads| {
        let worker_db = db.clone();
        let (control, hook) = paused_entry(kind);
        let worker = threads.spawn(move || {
            hook.run(|| {
                ORDINARY_BODY_ENTERED.set(false);
                match kind {
                    EntryKind::Scope => assert_eq!(
                        try_with_operation(&worker_db, || ORDINARY_BODY_ENTERED.set(true)),
                        Ok(())
                    ),
                    EntryKind::Query => {
                        assert_eq!(ordinary_entry(&worker_db), 7);
                    }
                }
                assert!(ORDINARY_BODY_ENTERED.get());
                assert!(attempt_probe::current().is_none());
                assert_eq!(attempt_probe::stack_depths(), (0, 0));
                assert!(!attempt_probe::is_incomplete(&worker_db));
            })
        });
        let value = body(&control);
        drop(control);
        assert!(matches!(worker.join(), Ok(Some(()))));
        assert_eq!(db.zalsa().attempt_operations.load(Ordering::SeqCst), 0);
        value
    })
}

#[derive(Clone, Copy)]
enum SetupBoundary {
    Create,
    Seal,
    Run,
}

fn run_at_setup_boundary(
    db: &DatabaseImpl,
    control: &EntryControl,
    boundary: SetupBoundary,
) -> RunResult<u32> {
    let support = attempt_probe::current().expect("installed attempt");
    let receipt = support.local_ownership(db.zalsa());
    let depths = attempt_probe::stack_depths();
    if matches!(boundary, SetupBoundary::Create) {
        control.start_and_wait();
    }
    let registry = RegistryBuilder::new(db, &Unrestricted)?;
    if matches!(boundary, SetupBoundary::Create) {
        control.release_and_wait();
    }

    if matches!(boundary, SetupBoundary::Seal) {
        control.start_and_wait();
    }
    let sealed = registry.seal()?;
    if matches!(boundary, SetupBoundary::Seal) {
        control.release_and_wait();
    }

    if matches!(boundary, SetupBoundary::Run) {
        control.start_and_wait();
    }
    let result = sealed.run(|endpoint| async move { endpoint.demand(|| async { Ok(7) })?.await });
    if matches!(boundary, SetupBoundary::Run) {
        control.release_and_wait();
    }
    assert!(attempt_probe::current().unwrap().same_owner(&support));
    assert_eq!(support.local_ownership(db.zalsa()), receipt);
    assert_eq!(attempt_probe::stack_depths(), depths);
    result
}

#[test]
fn independent_entries_cannot_change_registration_ownership() {
    for kind in [EntryKind::Scope, EntryKind::Query] {
        for boundary in [
            SetupBoundary::Create,
            SetupBoundary::Seal,
            SetupBoundary::Run,
        ] {
            for scoped in [false, true] {
                let db = DatabaseImpl::default();
                let result = with_worker(&db, kind, |control| {
                    try_with_attempt(&db, 100, || {
                        if scoped {
                            try_with_operation(&db, || {
                                run_at_setup_boundary(&db, control, boundary)
                            })
                            .expect("enclosing scope is admitted")
                        } else {
                            run_at_setup_boundary(&db, control, boundary)
                        }
                    })
                });
                assert_eq!(result, Ok(AttemptOutcome::Complete(Ok(7))));
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Finish {
    Complete,
    Refuse,
    Panic,
    Cancel,
}

#[derive(Debug)]
struct TaskPanic;

fn finish_with_independent_entry(db: &DatabaseImpl, control: &EntryControl, finish: Finish) {
    let support = attempt_probe::current().expect("installed attempt");
    let receipt = support.local_ownership(db.zalsa());
    let depths = attempt_probe::stack_depths();
    let sealed = RegistryBuilder::new(db, &Unrestricted)
        .expect("registration is admitted")
        .seal()
        .expect("registration is sealed");

    // Catch inside the attempt so the paused worker still observes an active attempt
    // after teardown, including when the task panics or revision cancellation unwinds it.
    let result = catch_unwind(AssertUnwindSafe(|| {
        sealed.run(|endpoint| async move {
            endpoint.demand(|| async { Ok(()) })?.await?;
            control.start_and_wait();
            match finish {
                Finish::Complete => Ok(7),
                Finish::Refuse => Err(RunError::Refused(Incomplete::Allowance)),
                Finish::Panic => panic_any(TaskPanic),
                Finish::Cancel => {
                    db.zalsa().runtime().set_cancellation_flag();
                    endpoint.admit(ExecutionWork::Poll)?;
                    panic!("cancellation did not unwind the task");
                }
            }
        })
    }));
    if matches!(finish, Finish::Cancel) {
        db.zalsa().runtime().reset_cancellation_flag();
    }
    assert_eq!(attempt_probe::stack_depths(), depths);
    assert_eq!(support.local_ownership(db.zalsa()), receipt);
    assert!(db.zalsa_local().active_query().is_none());
    control.release_and_wait();

    match finish {
        Finish::Complete => assert!(matches!(result, Ok(Ok(7)))),
        Finish::Refuse => assert!(matches!(
            result,
            Ok(Err(RunError::Refused(Incomplete::Allowance)))
        )),
        Finish::Panic => {
            let Err(payload) = result else {
                panic!("task panic was lost");
            };
            assert!(payload.is::<TaskPanic>());
        }
        Finish::Cancel => {
            let Err(payload) = result else {
                panic!("revision cancellation was lost");
            };
            assert!(matches!(
                payload.downcast_ref::<Cancelled>(),
                Some(Cancelled::PendingWrite)
            ));
        }
    }
}

#[test]
fn independent_entries_do_not_replace_completion_or_unwind_results() {
    for kind in [EntryKind::Scope, EntryKind::Query] {
        for finish in [
            Finish::Complete,
            Finish::Refuse,
            Finish::Panic,
            Finish::Cancel,
        ] {
            for scoped in [false, true] {
                let db = DatabaseImpl::default();
                let result = with_worker(&db, kind, |control| {
                    try_with_attempt(&db, 100, || {
                        if scoped {
                            assert_eq!(
                                try_with_operation(&db, || {
                                    finish_with_independent_entry(&db, control, finish);
                                }),
                                Ok(())
                            );
                        } else {
                            finish_with_independent_entry(&db, control, finish);
                        }
                    })
                });
                assert_eq!(
                    result,
                    match finish {
                        Finish::Refuse => Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)),
                        Finish::Cancel => Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted)),
                        Finish::Complete | Finish::Panic => Ok(AttemptOutcome::Complete(())),
                    }
                );
            }
        }
    }
}
