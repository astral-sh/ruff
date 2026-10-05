use std::cell::{Cell, RefCell};
use std::future::{Future, pending, poll_fn};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

use super::super::progress::TerminalFailure;
use super::super::registration::RegistryBuilder;
use super::super::{
    ActivePoll, Driver, Endpoint, ExecutionAdmission, ExecutionWork, Provider, Queue, RunContext,
    RunError, RunResult, Unrestricted,
};
use super::{Node, Script, Stop, fixpoint, memo, observation, run};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, QueryPolicy, try_with_attempt};
use crate::function::memo::SelectedMemo;
use crate::function::{ClaimResult, Configuration, IngredientImpl, Reentrancy};
use crate::plumbing::AsId;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseImpl, DatabaseKeyIndex, Id};

mod child_call;
mod local_call;

fn reason() -> Option<Incomplete> {
    attempt_probe::current().and_then(|support| support.reason())
}

#[derive(Debug)]
struct OwnerSnapshot {
    stage: &'static str,
    frame: Option<(DatabaseKeyIndex, bool)>,
    operation_depth: usize,
    policy: QueryPolicy,
    reason: Option<Incomplete>,
    claim_held: bool,
}

struct ObserveOwner<'db, C: Configuration> {
    db: &'db dyn Database,
    ingredient: &'db IngredientImpl<C>,
    node: Node,
    stage: &'static str,
    drops: Rc<RefCell<Vec<OwnerSnapshot>>>,
}

impl<C: Configuration> Drop for ObserveOwner<'_, C> {
    fn drop(&mut self) {
        self.drops.borrow_mut().push(OwnerSnapshot {
            stage: self.stage,
            frame: self
                .db
                .zalsa_local()
                .try_with_query_stack(|stack| {
                    stack
                        .last()
                        .map(|frame| (frame.database_key_index, frame.attempt_incomplete()))
                })
                .flatten(),
            operation_depth: attempt_probe::stack_depths().0,
            policy: attempt_probe::current_policy(),
            reason: reason(),
            claim_held: matches!(
                self.ingredient.sync_table.peek_claim(
                    self.db.zalsa(),
                    self.node.as_id(),
                    Reentrancy::Deny
                ),
                ClaimResult::Cycle { .. }
            ),
        });
    }
}

struct TerminalProvider<'db, C: Configuration> {
    ingredient: &'db IngredientImpl<C>,
    drops: Rc<RefCell<Vec<OwnerSnapshot>>>,
    error: RunError,
    previous: Option<Incomplete>,
}

impl<'run, 'db: 'run, C> Provider<'run, 'db, C> for TerminalProvider<'db, C>
where
    C: for<'a> Configuration<DbView = dyn Database, Input<'a> = Node, Output<'a> = u32>,
{
    // Node conversion constructs a handle; output equality compares u32.
    fixture_native_value!(provider, 'run, 'db, C, 1);

    type Output = u32;

    async fn body(
        &self,
        db: &'db dyn Database,
        node: Node,
        endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        let _provider = ObserveOwner {
            db,
            ingredient: self.ingredient,
            node,
            stage: "provider",
            drops: self.drops.clone(),
        };
        let child = ObserveOwner {
            db,
            ingredient: self.ingredient,
            node,
            stage: "queued child",
            drops: self.drops.clone(),
        };
        let _reply = endpoint.demand(move || {
            poll_fn(move |_| -> Poll<RunResult<()>> {
                let _child = &child;
                panic!("terminal suspension must stop before polling its queued child");
            })
        })?;
        if let Some(previous) = self.previous {
            attempt_probe::report_incomplete(db, previous);
        }
        match endpoint.suspend_error(self.error)?.await? {}
    }

    async fn initial(
        &self,
        _db: &'db dyn Database,
        _id: Id,
        _node: Node,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        Err(RunError::Contract(
            "terminal body must not enter cycle initialization",
        ))
    }

    async fn recover(
        &self,
        _db: &'db dyn Database,
        _cycle: &Cycle<'_>,
        _last: &u32,
        _value: u32,
        _node: Node,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        Err(RunError::Contract(
            "terminal body must not enter cycle recovery",
        ))
    }

    async fn complete(
        &self,
        _db: &'db dyn Database,
        _memo: Option<SelectedMemo<'db, C>>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> RunResult<u32> {
        Err(RunError::Contract(
            "terminal body must not complete its query",
        ))
    }
}

#[test]
fn terminal_suspension_keeps_query_frame_and_claim_until_children_drop() {
    for (error, previous, expected_reason) in [
        (
            RunError::Refused(Incomplete::Allowance),
            None,
            Incomplete::Allowance,
        ),
        (
            RunError::Contract("provider stopped"),
            Some(Incomplete::Allowance),
            Incomplete::Allowance,
        ),
        (
            RunError::Refused(Incomplete::Interrupted),
            Some(Incomplete::Allowance),
            Incomplete::Allowance,
        ),
    ] {
        let db = DatabaseImpl::default();
        let db_ref = &db;
        let node = Node::new(&db, None, 0);
        let ingredient = fixpoint::fn_ingredient_(&db, db.zalsa());
        let key = ingredient.database_key_index(node.as_id());
        let stamp = crate::prepared_source_probe::Stamp::current(&db);
        let drops = Rc::new(RefCell::new(Vec::new()));
        let (outcome, observed) = observation::collect(|| {
            try_with_attempt(&db, 100, || {
                let drops = drops.clone();
                let result = Driver::run(&db, |endpoint| async move {
                    let _root = ObserveOwner {
                        db: db_ref,
                        ingredient,
                        node,
                        stage: "root",
                        drops: drops.clone(),
                    };
                    endpoint
                        .execute(
                            ingredient,
                            db_ref,
                            node.as_id(),
                            None,
                            TerminalProvider {
                                ingredient,
                                drops,
                                error,
                                previous,
                            },
                        )?
                        .await
                });
                assert_eq!(result, Err(error));
            })
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Incomplete(expected_reason)));
        let snapshots = drops.borrow();
        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.stage)
                .collect::<Vec<_>>(),
            ["queued child", "provider", "root"]
        );
        for snapshot in &snapshots[..2] {
            assert_eq!(snapshot.frame, Some((key, true)));
            assert_eq!(snapshot.operation_depth, 1);
            assert_eq!(snapshot.policy, QueryPolicy::ReturnOnly);
            assert_eq!(snapshot.reason, Some(expected_reason));
            assert!(snapshot.claim_held);
        }
        assert_eq!(snapshots[2].frame, None);
        assert_eq!(snapshots[2].operation_depth, 0);
        assert_eq!(snapshots[2].reason, Some(expected_reason));
        assert!(!snapshots[2].claim_held);
        assert_eq!(observed.polls, 2);
        assert_eq!(observed.events.iter().filter(|event| matches!(event, observation::Event::Execute { key: owner, .. } if *owner == key)).count(), 1);
        assert!(memo(&db, ingredient, node).is_none());
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
        assert!(db.zalsa_local().active_query().is_none());
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
    }
}

#[test]
fn terminal_error_identity_is_separate_from_the_first_incomplete_reason() {
    for (error, previous, expected_reason) in [
        (
            RunError::Contract("exact terminal contract"),
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
            None,
            Incomplete::Interrupted,
        ),
        (
            RunError::Refused(Incomplete::Interrupted),
            Some(Incomplete::Allowance),
            Incomplete::Allowance,
        ),
        (
            RunError::Refused(Incomplete::Allowance),
            Some(Incomplete::Interrupted),
            Incomplete::Interrupted,
        ),
    ] {
        let db = DatabaseImpl::default();
        let outcome = try_with_attempt(&db, 100, || {
            let db_ref = &db;
            let result: RunResult<()> =
                RegistryBuilder::new(&db, &Unrestricted)?
                    .seal()?
                    .run(|endpoint| async move {
                        if let Some(previous) = previous {
                            attempt_probe::report_incomplete(db_ref, previous);
                        }
                        match endpoint.inner.suspend_error(error)?.await? {}
                    });
            assert_eq!(result, Err(error));
            assert_eq!(reason(), Some(expected_reason));
            Ok::<_, RunError>(())
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Incomplete(expected_reason)));
    }
}

#[test]
fn observing_an_error_preserves_it_after_an_earlier_incomplete_reason() {
    let db = DatabaseImpl::default();
    let outcome = try_with_attempt(&db, 100, || {
        let context = RunContext::new(&db).unwrap();
        context.refuse(Incomplete::Allowance);
        let error = RunError::Refused(Incomplete::Interrupted);
        assert_eq!(context.observe::<()>(Err(error)), Err(error));
        assert_eq!(context.reason.get(), Some(Incomplete::Allowance));
        assert_eq!(reason(), Some(Incomplete::Allowance));
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
}

#[derive(Default)]
struct Admissions(RefCell<Vec<ExecutionWork>>);

impl ExecutionAdmission for Admissions {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.0.borrow_mut().push(work);
        Ok(())
    }
}

#[test]
fn semantic_work_spends_the_shared_allowance_before_observation() {
    let db = DatabaseImpl::default();
    let admission = Admissions::default();
    let completed = Cell::new(0);
    let outcome = try_with_attempt(&db, 5, || {
        let completed = &completed;
        let result = RegistryBuilder::new(&db, &admission)
            .unwrap()
            .seal()
            .unwrap()
            .run(|endpoint| async move {
                for units in [0, 2, 3, 1] {
                    endpoint.local_call(|| endpoint.admit_work(units)).await;
                    completed.set(completed.get() + 1);
                }
                Ok(())
            });
        assert_eq!(result, Err(RunError::Refused(Incomplete::Allowance)));
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(completed.get(), 3);
    assert_eq!(
        admission
            .0
            .borrow()
            .iter()
            .filter_map(|work| match work {
                ExecutionWork::Work { units } => Some(*units),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [0, 2, 3]
    );
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
}

#[test]
fn raw_work_observation_leaves_the_shared_allowance_available() {
    let db = DatabaseImpl::default();
    let admission = Admissions::default();
    let outcome = try_with_attempt(&db, 1, || {
        RegistryBuilder::new(&db, &admission)?
            .seal()?
            .run(|endpoint| async move {
                endpoint
                    .local_call(|| endpoint.admit(ExecutionWork::Work { units: 100 }))
                    .await;
                endpoint.local_call(|| endpoint.admit_work(1)).await;
                Ok(())
            })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    assert_eq!(
        admission
            .0
            .borrow()
            .iter()
            .filter_map(|work| match work {
                ExecutionWork::Work { units } => Some(*units),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [100, 1]
    );
}

#[test]
fn ineligible_semantic_work_does_not_charge_or_refuse_another_scope() {
    let db = DatabaseImpl::default();
    let admission = Admissions::default();
    let expired = try_with_attempt(&db, 1, || {
        RegistryBuilder::new(&db, &admission)?
            .seal()?
            .run(|endpoint| async move { Ok(endpoint) })
    });
    let Ok(AttemptOutcome::Complete(Ok(expired))) = expired else {
        panic!("the completed run returns its endpoint")
    };
    let expected = Err(RunError::Contract(
        "semantic work requires the current interruptible execution run",
    ));
    let previous_observations = admission.0.borrow().len();
    assert_eq!(expired.admit_work(1), expected);
    let expired = &expired;
    let foreign = DatabaseImpl::default();
    for current in [&db as &dyn Database, &foreign as &dyn Database] {
        let current_admission = Admissions::default();
        let outcome = try_with_attempt(current, 1, || {
            RegistryBuilder::new(current, &current_admission)?
                .seal()?
                .run(|endpoint| async move {
                    assert_eq!(expired.admit_work(1), expected);
                    let operation = attempt_probe::enter(
                        current.zalsa(),
                        QueryPolicy::CompleteOnly,
                        "semantic work complete-only control",
                    );
                    assert_eq!(endpoint.admit_work(1), expected);
                    assert_eq!(reason(), None);
                    drop(operation);
                    endpoint.local_call(|| endpoint.admit_work(1)).await;
                    Ok(())
                })
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
        assert_eq!(attempt_probe::stack_depths(), (0, 0));
    }
    assert_eq!(admission.0.borrow().len(), previous_observations);
}

#[crate::db]
#[derive(Clone)]
struct EventDb {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl Database for EventDb {}

#[test]
fn terminal_poll_spends_no_work_and_stops_before_checkpoint_resume() {
    eprintln!(
        "terminal layout: Queue={} ActivePoll={} TerminalFailure={}",
        size_of::<Queue<'_>>(),
        size_of::<ActivePoll>(),
        size_of::<TerminalFailure<'_, '_, '_>>()
    );
    for checkpoint in [false, true] {
        let callbacks = Arc::new(AtomicUsize::new(0));
        let counted = callbacks.clone();
        let db = EventDb {
            storage: crate::Storage::new(Some(Box::new(move |_| {
                counted.fetch_add(1, Ordering::Relaxed);
            }))),
        };
        let admission = Admissions::default();
        let (outcome, observed) = observation::collect(|| {
            try_with_attempt(&db, 100, || {
                let admission_ref = &admission;
                let result: RunResult<()> =
                    Driver::run_with_admission(&db, &admission, |endpoint| async move {
                        poll_fn(|cx| {
                            if checkpoint {
                                let mut local = endpoint.checkpoint()?;
                                assert!(Pin::new(&mut local).poll(cx).is_pending());
                            }
                            let admitted = admission_ref.0.borrow().len();
                            let events = callbacks.load(Ordering::Relaxed);
                            let mut first =
                                endpoint.suspend_error(RunError::Refused(Incomplete::Allowance))?;
                            let mut second = endpoint.suspend_error(RunError::RequiresFetch)?;
                            assert_eq!(reason(), None);
                            assert!(Pin::new(&mut first).poll(cx).is_pending());
                            assert!(Pin::new(&mut second).poll(cx).is_pending());
                            assert!(Pin::new(&mut first).poll(cx).is_pending());
                            assert_eq!(reason(), Some(Incomplete::Allowance));
                            assert_eq!(admission_ref.0.borrow().len(), admitted);
                            assert_eq!(callbacks.load(Ordering::Relaxed), events);
                            Poll::<RunResult<()>>::Pending
                        })
                        .await
                    });
                assert_eq!(result, Err(RunError::Refused(Incomplete::Allowance)));
            })
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
        );
        assert_eq!(observed.polls, 1);
        assert_eq!(
            admission
                .0
                .borrow()
                .iter()
                .filter(|work| matches!(work, ExecutionWork::Task { .. }))
                .count(),
            1
        );
        assert_eq!(
            admission
                .0
                .borrow()
                .iter()
                .filter(|work| matches!(work, ExecutionWork::Work { .. }))
                .count(),
            usize::from(checkpoint)
        );
    }
}

struct Returned(Rc<Cell<usize>>);

impl Drop for Returned {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn ignored_terminal_pending_overrides_later_ready_without_publishing_output() {
    for later_error in [false, true] {
        let db = DatabaseImpl::default();
        let reply = RefCell::new(None);
        let drops = Rc::new(Cell::new(0));
        let terminal_error = RunError::Contract("first terminal disposition");
        let outcome = try_with_attempt(&db, 100, || {
            let reply = &reply;
            let drops = drops.clone();
            let result: RunResult<()> = Driver::run(&db, |endpoint| async move {
                let child = endpoint.clone();
                *reply.borrow_mut() = Some(endpoint.demand(move || async move {
                    let mut terminal = child.suspend_error(terminal_error)?;
                    poll_fn(|cx| {
                        assert!(Pin::new(&mut terminal).poll(cx).is_pending());
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
            assert_eq!(result, Err(terminal_error));
        });
        assert_eq!(
            outcome,
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        );
        assert_eq!(drops.get(), usize::from(!later_error));
        let mut reply = reply.into_inner().expect("escaped child reply");
        assert!(
            matches!(Pin::new(&mut reply).poll(&mut Context::from_waker(Waker::noop())), Poll::Ready(Err(error)) if error == terminal_error)
        );
    }
}

struct SuspendOnRetirement<'run, 'db: 'run> {
    endpoint: Endpoint<'run, 'db>,
    error: RunError,
}

impl Future for SuspendOnRetirement<'_, '_> {
    type Output = RunResult<u32>;
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(Ok(17))
    }
}

impl Drop for SuspendOnRetirement<'_, '_> {
    fn drop(&mut self) {
        let mut terminal = self
            .endpoint
            .suspend_error(self.error)
            .expect("retirement retains the active poll");
        assert!(
            Pin::new(&mut terminal)
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }
}

#[test]
fn retirement_terminal_error_survives_the_following_resume_check() {
    let db = DatabaseImpl::default();
    let error = RunError::RequiresFetch;
    let outcome = try_with_attempt(&db, 100, || {
        let result = Driver::run(&db, |endpoint| SuspendOnRetirement { endpoint, error });
        assert_eq!(result, Err(error));
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
}

#[test]
fn terminal_capture_cannot_move_to_a_later_task_poll() {
    let db = DatabaseImpl::default();
    let outcome = try_with_attempt(&db, 100, || {
        Driver::run(&db, |endpoint| async move {
            let mut terminal = endpoint.suspend_error(RunError::RequiresFetch)?;
            endpoint.demand(|| async { Ok(()) })?.await?;
            poll_fn(|cx| {
                assert!(matches!(
                    Pin::new(&mut terminal).poll(cx),
                    Poll::Ready(Err(RunError::Contract(_)))
                ));
                assert_eq!(reason(), None);
                Poll::Ready(Ok(()))
            })
            .await
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
}

#[test]
fn terminal_failure_cannot_cross_a_complete_only_operation() {
    let db = DatabaseImpl::default();
    let db_ref = &db;
    let outcome = try_with_attempt(&db, 100, || {
        Driver::run(&db, |endpoint| async move {
            let mut terminal = endpoint.suspend_error(RunError::RequiresFetch)?;
            let operation = attempt_probe::enter(
                db_ref.zalsa(),
                QueryPolicy::CompleteOnly,
                "terminal complete-only control",
            );
            assert!(matches!(
                endpoint.suspend_error(RunError::RequiresFetch),
                Err(RunError::Contract(_))
            ));
            poll_fn(|cx| {
                assert!(matches!(
                    Pin::new(&mut terminal).poll(cx),
                    Poll::Ready(Err(RunError::Contract(_)))
                ));
                assert_eq!(reason(), None);
                Poll::Ready(())
            })
            .await;
            drop(operation);
            Ok(())
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
}

#[test]
fn unpolled_terminal_and_ended_endpoints_do_not_refuse_other_attempts() {
    let db = DatabaseImpl::default();
    let expired = try_with_attempt(&db, 100, || {
        Driver::run(&db, |endpoint| async move {
            drop(endpoint.suspend_error(RunError::RequiresFetch)?);
            assert_eq!(reason(), None);
            Ok(endpoint)
        })
    });
    let Ok(AttemptOutcome::Complete(Ok(expired))) = expired else {
        panic!("the first run returns its endpoint")
    };
    assert!(matches!(
        expired.suspend_error(RunError::RequiresFetch),
        Err(RunError::Contract(_))
    ));
    let foreign = DatabaseImpl::default();
    for current in [&db as &dyn Database, &foreign as &dyn Database] {
        let outcome = try_with_attempt(current, 100, || {
            Driver::run(current, |_| async {
                assert!(matches!(
                    expired.suspend_error(RunError::Refused(Incomplete::Allowance)),
                    Err(RunError::Contract(_))
                ));
                assert_eq!(reason(), None);
                Ok(())
            })
        });
        assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    }
}

#[test]
fn unrelated_bare_pending_still_fails_the_driver_contract() {
    let db = DatabaseImpl::default();
    let (outcome, observed) = observation::collect(|| {
        try_with_attempt(&db, 100, || {
            let result = Driver::run(&db, |_| pending::<RunResult<()>>());
            assert!(matches!(result, Err(RunError::Contract(_))));
        })
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    assert_eq!(observed.polls, 1);
}
