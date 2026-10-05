use std::cell::{Cell, RefCell};
use std::future::{Future, pending, poll_fn};
use std::marker::PhantomPinned;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use super::super::task::TypedTask;
use super::super::{
    Driver, Endpoint, ExecutionAdmission, ExecutionWork, PendingTask, Reply, RunError, RunResult,
};
use super::observation;
use crate::DatabaseImpl;
use crate::attempt_probe::{self, AttemptOutcome, Incomplete, try_with_attempt};
use crate::zalsa::ZalsaDatabase;

#[derive(Default)]
struct Journal {
    events: RefCell<Vec<&'static str>>,
    ancestor_alive: Cell<bool>,
    panic_on_output_drop: Cell<bool>,
    dropped_after_ancestor: Cell<bool>,
    output_left_in_future: Cell<bool>,
    published_before_retirement: Cell<bool>,
}

impl Journal {
    fn record_drop(&self, event: &'static str) {
        self.events.borrow_mut().push(event);
        if !self.ancestor_alive.get() {
            self.dropped_after_ancestor.set(true);
        }
    }

    fn assert_ownership(&self) {
        assert!(!self.dropped_after_ancestor.get());
        assert!(!self.output_left_in_future.get());
        assert!(!self.published_before_retirement.get());
    }
}

struct Ancestor(Rc<Journal>);

impl Ancestor {
    fn new(journal: Rc<Journal>) -> Self {
        assert!(!journal.ancestor_alive.replace(true));
        Self(journal)
    }
}

impl Drop for Ancestor {
    fn drop(&mut self) {
        self.0.events.borrow_mut().push("ancestor");
        assert!(self.0.ancestor_alive.replace(false));
    }
}

struct Owned {
    journal: Rc<Journal>,
    _pin: PhantomPinned,
}

impl Drop for Owned {
    fn drop(&mut self) {
        self.journal.record_drop("output");
        if self.journal.panic_on_output_drop.replace(false) {
            panic!("unpublished output drop panic");
        }
    }
}

struct QueuedChild(Rc<Journal>);

impl Drop for QueuedChild {
    fn drop(&mut self) {
        self.0.record_drop("queued child");
    }
}

type Escaped = Rc<RefCell<Option<Reply<Owned>>>>;

fn poll_reply<T>(reply: &RefCell<Option<Reply<T>>>, cx: &mut Context<'_>) -> Poll<RunResult<T>> {
    let mut reply = reply.borrow_mut();
    Pin::new(reply.as_mut().expect("the child demand has escaped")).poll(cx)
}

#[derive(Clone, Copy, Debug)]
enum Completion {
    Ready,
    CheckpointRequest,
    UnconsumedReceipt,
    QueuedChild,
    Error,
}

#[derive(Clone, Copy, Debug)]
enum Retirement {
    Quiet,
    Refuse,
    Interrupt,
    QueueChild,
    QueueChildAndAbandon,
    Abandon,
    Panic,
    #[cfg(not(feature = "shuttle"))]
    Cancel,
}

// Keep the user future alive after Ready so its destructor can observe the publication boundary.
// Interior mutability permits direct polling without requiring either the future or output to be Unpin.
struct Completing<'run, 'db: 'run> {
    endpoint: Endpoint<'run, 'db>,
    journal: Rc<Journal>,
    escaped: Escaped,
    completion: Completion,
    retirement: Retirement,
    result: RefCell<Option<RunResult<Owned>>>,
    child: RefCell<Option<Reply<()>>>,
    polls: Cell<usize>,
    _pin: PhantomPinned,
}

impl<'run, 'db: 'run> Completing<'run, 'db> {
    fn new(
        endpoint: Endpoint<'run, 'db>,
        journal: Rc<Journal>,
        escaped: Escaped,
        completion: Completion,
        retirement: Retirement,
    ) -> Self {
        let result = if matches!(completion, Completion::Error) {
            Err(RunError::Refused(Incomplete::Allowance))
        } else {
            Ok(Owned {
                journal: journal.clone(),
                _pin: PhantomPinned,
            })
        };
        Self {
            endpoint,
            journal,
            escaped,
            completion,
            retirement,
            result: RefCell::new(Some(result)),
            child: RefCell::new(None),
            polls: Cell::new(0),
            _pin: PhantomPinned,
        }
    }

    fn queue_child(&self) -> RunResult<Reply<()>> {
        let owner = QueuedChild(self.journal.clone());
        self.endpoint.demand(move || {
            poll_fn(move |_| -> Poll<RunResult<()>> {
                let _owner = &owner;
                panic!("a child queued by a completing task must not run");
            })
        })
    }
}

impl Future for Completing<'_, '_> {
    type Output = RunResult<Owned>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_ref().get_ref();
        let ordinal = this.polls.replace(this.polls.get() + 1);
        if ordinal == 0 {
            match this.completion {
                Completion::CheckpointRequest | Completion::UnconsumedReceipt => {
                    let mut checkpoint = this.endpoint.checkpoint()?;
                    assert!(Pin::new(&mut checkpoint).poll(cx).is_pending());
                    if matches!(this.completion, Completion::UnconsumedReceipt) {
                        return Poll::Pending;
                    }
                }
                Completion::QueuedChild => {
                    *this.child.borrow_mut() = Some(this.queue_child()?);
                }
                Completion::Ready | Completion::Error => {}
            }
        } else {
            assert!(matches!(this.completion, Completion::UnconsumedReceipt));
            assert_eq!(ordinal, 1);
        }
        this.journal.events.borrow_mut().push("return");
        Poll::Ready(
            this.result
                .borrow_mut()
                .take()
                .expect("only one Ready poll"),
        )
    }
}

impl Drop for Completing<'_, '_> {
    fn drop(&mut self) {
        self.journal
            .output_left_in_future
            .set(self.result.borrow().is_some());
        // Inspect without consuming a terminal error: the escaped demand checks it after cleanup.
        self.journal.published_before_retirement.set(
            self.escaped
                .borrow()
                .as_ref()
                .is_some_and(|reply| reply.value.borrow().as_ref().is_some_and(Result::is_ok)),
        );
        self.journal.record_drop("future");
        match self.retirement {
            Retirement::Quiet => {}
            Retirement::Refuse => {
                attempt_probe::report_incomplete(self.endpoint.context.db, Incomplete::Allowance);
            }
            Retirement::Interrupt => {
                attempt_probe::report_incomplete(self.endpoint.context.db, Incomplete::Interrupted);
            }
            Retirement::QueueChild | Retirement::QueueChildAndAbandon => {
                let _reply = self
                    .queue_child()
                    .expect("the active poll can demand a child");
                if matches!(self.retirement, Retirement::QueueChildAndAbandon) {
                    drop(self.escaped.borrow_mut().take());
                }
            }
            Retirement::Abandon => drop(self.escaped.borrow_mut().take()),
            Retirement::Panic => panic!("completed future drop panic"),
            #[cfg(not(feature = "shuttle"))]
            Retirement::Cancel => self
                .endpoint
                .context
                .db
                .zalsa()
                .runtime()
                .set_cancellation_flag(),
        }
    }
}

fn assert_restored_and_retry(db: &DatabaseImpl) {
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(
        try_with_attempt(db, 100, || Driver::run(db, |_| async { Ok(()) })),
        Ok(AttemptOutcome::Complete(Ok(())))
    );
}

fn reject(
    completion: Completion,
    retirement: Retirement,
    output_panics: bool,
) -> Vec<&'static str> {
    let db = DatabaseImpl::default();
    let journal = Rc::new(Journal::default());
    journal.panic_on_output_drop.set(output_panics);
    let escaped = Rc::new(RefCell::new(None));
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let journal = journal.clone();
        let escaped = escaped.clone();
        try_with_attempt(&db, 100, || {
            let result = Driver::run(&db, |endpoint| async move {
                let _ancestor = Ancestor::new(journal.clone());
                let future = Completing::new(
                    endpoint.clone(),
                    journal,
                    escaped.clone(),
                    completion,
                    retirement,
                );
                *escaped.borrow_mut() = Some(endpoint.demand(move || future)?);
                pending::<RunResult<()>>().await
            });
            if matches!(completion, Completion::Error) {
                assert_eq!(result, Err(RunError::Refused(Incomplete::Allowance)));
            }
            result
        })
    }));
    #[cfg(not(feature = "shuttle"))]
    db.zalsa().runtime().reset_cancellation_flag();
    if output_panics {
        let payload = outcome.expect_err("the unpublished output destructor panics");
        assert_eq!(
            payload.downcast_ref::<&str>(),
            Some(&"unpublished output drop panic")
        );
    } else {
        match retirement {
            Retirement::Panic => {
                let payload = outcome.expect_err("the completed future destructor panics");
                assert_eq!(
                    payload.downcast_ref::<&str>(),
                    Some(&"completed future drop panic")
                );
            }
            #[cfg(not(feature = "shuttle"))]
            Retirement::Cancel => {
                let payload = outcome.expect_err("revision cancellation still unwinds");
                assert!(matches!(
                    payload.downcast_ref::<crate::Cancelled>(),
                    Some(crate::Cancelled::PendingWrite)
                ));
            }
            _ => {
                let reason = if matches!(completion, Completion::Error)
                    || matches!(retirement, Retirement::Refuse)
                {
                    Incomplete::Allowance
                } else {
                    Incomplete::Interrupted
                };
                assert_eq!(
                    outcome.expect("cooperative rejection does not panic"),
                    Ok(AttemptOutcome::Incomplete(reason))
                );
            }
        }
    }
    journal.assert_ownership();
    if matches!(retirement, Retirement::QueueChildAndAbandon) {
        assert!(escaped.borrow().is_none());
    } else {
        let reply = poll_reply(&escaped, &mut Context::from_waker(Waker::noop()));
        if matches!(completion, Completion::Error) {
            assert!(matches!(
                reply,
                Poll::Ready(Err(RunError::Refused(Incomplete::Allowance)))
            ));
        } else {
            assert!(
                !matches!(reply, Poll::Ready(Ok(_))),
                "a rejected task published success"
            );
        }
    }
    assert!(!journal.ancestor_alive.get());
    assert_restored_and_retry(&db);
    journal.events.borrow().clone()
}

#[test]
fn malformed_ready_never_publishes_its_owned_output() {
    for completion in [Completion::CheckpointRequest, Completion::UnconsumedReceipt] {
        assert_eq!(
            reject(completion, Retirement::Quiet, false),
            ["return", "output", "future", "ancestor"]
        );
    }
}

#[test]
fn queued_child_drops_before_rejected_output_and_future() {
    assert_eq!(
        reject(Completion::QueuedChild, Retirement::Quiet, false),
        ["return", "queued child", "output", "future", "ancestor"]
    );
}

#[test]
fn completed_future_destructor_cannot_publish_after_refusal_or_new_child() {
    assert_eq!(
        reject(Completion::Ready, Retirement::Refuse, false),
        ["return", "future", "output", "ancestor"]
    );
    assert_eq!(
        reject(Completion::Ready, Retirement::QueueChild, false),
        ["return", "future", "queued child", "output", "ancestor"]
    );
    assert_eq!(
        reject(Completion::Ready, Retirement::QueueChildAndAbandon, false),
        ["return", "future", "queued child", "output", "ancestor"]
    );
}

#[test]
fn completed_future_panic_retains_output_and_drains_ancestors() {
    assert_eq!(
        reject(Completion::Ready, Retirement::Panic, false),
        ["return", "future", "output", "ancestor"]
    );
}

#[cfg(not(feature = "shuttle"))]
#[test]
fn completed_future_cancellation_retains_output_and_drains_ancestors() {
    assert_eq!(
        reject(Completion::Ready, Retirement::Cancel, false),
        ["return", "future", "output", "ancestor"]
    );
}

#[test]
fn original_callback_error_survives_destructor_refusal() {
    assert_eq!(
        reject(Completion::Error, Retirement::Interrupt, false),
        ["return", "future", "ancestor"]
    );
}

#[test]
fn rejected_output_panic_still_drops_future_and_ancestors() {
    assert_eq!(
        reject(Completion::QueuedChild, Retirement::Quiet, true),
        ["return", "queued child", "output", "future", "ancestor"]
    );
}

#[test]
fn accepted_completion_preserves_borrowed_and_pinned_values() {
    let db = DatabaseImpl::default();
    let borrowed = String::from("borrowed output");
    let borrowed = &borrowed;
    let journal = Rc::new(Journal::default());
    let escaped = Rc::new(RefCell::new(None));
    let outcome = try_with_attempt(&db, 100, || {
        let journal = journal.clone();
        let escaped = escaped.clone();
        Driver::run(&db, |endpoint| async move {
            let _ancestor = Ancestor::new(journal.clone());
            let returned = endpoint
                .demand(move || async move { Ok(borrowed) })?
                .await?;
            assert!(std::ptr::eq(returned, borrowed));
            let future = Completing::new(
                endpoint.clone(),
                journal,
                escaped.clone(),
                Completion::Ready,
                Retirement::Quiet,
            );
            *escaped.borrow_mut() = Some(endpoint.demand(move || future)?);
            let value = poll_fn(|cx| poll_reply(&escaped, cx)).await?;
            drop(value);
            assert!(poll_fn(|cx| Poll::Ready(poll_reply(&escaped, cx).is_pending())).await);
            Ok(())
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    journal.assert_ownership();
    assert_eq!(
        *journal.events.borrow(),
        ["return", "future", "output", "ancestor"]
    );
    assert_restored_and_retry(&db);
}

fn charged_layout<M, F, T>(_make: &M) -> usize
where
    M: FnOnce() -> F,
    F: Future<Output = RunResult<T>>,
{
    let task = size_of::<TypedTask<'_, M, F, T>>();
    let slot = size_of::<RefCell<Option<RunResult<T>>>>();
    let pending = size_of::<PendingTask<'_>>();
    let requested = task
        + slot
        + size_of::<Cell<bool>>()
        + 8 * pending
        + 4 * size_of::<usize>()
        + align_of::<RefCell<Option<RunResult<T>>>>().max(align_of::<usize>())
        + align_of::<usize>();
    eprintln!(
        "publication layout: TypedTask={task} M={} F={} T={} Reply={} ReplySlot={slot} PendingTask={pending} requested_bytes={requested}",
        size_of::<M>(),
        size_of::<F>(),
        size_of::<T>(),
        size_of::<Reply<T>>(),
    );
    requested
}

#[derive(Default)]
struct RecordTaskAllocations(RefCell<Vec<usize>>);

impl ExecutionAdmission for RecordTaskAllocations {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if let ExecutionWork::Task { requested_bytes } = work {
            self.0.borrow_mut().push(requested_bytes);
        }
        Ok(())
    }
}

#[test]
fn task_allocation_charges_its_concrete_future_and_returned_value() {
    let db = DatabaseImpl::default();
    let admission = RecordTaskAllocations::default();
    let outcome = try_with_attempt(&db, 100, || {
        let recorded = &admission;
        Driver::run_with_admission(&db, &admission, |endpoint| async move {
            let payload = [7_u8; 33];
            let make = move || async move { Ok(payload) };
            let expected = charged_layout(&make);
            let before = recorded.0.borrow().len();
            let reply = endpoint.demand(make)?;
            assert_eq!(recorded.0.borrow()[before], expected);
            assert_eq!(reply.await?, payload);
            Ok(())
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    assert_eq!(admission.0.borrow().len(), 2);
    assert_restored_and_retry(&db);
}

struct RefuseChildAllocation {
    tasks: Cell<usize>,
}

impl ExecutionAdmission for RefuseChildAllocation {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if let ExecutionWork::Task { requested_bytes } = work {
            assert!(requested_bytes > 0);
            let ordinal = self.tasks.replace(self.tasks.get() + 1);
            if ordinal == 1 {
                return Err(RunError::Refused(Incomplete::Allowance));
            }
        }
        Ok(())
    }
}

#[test]
fn task_allocation_refusal_precedes_factory_and_queue_insertion() {
    let db = DatabaseImpl::default();
    let admission = RefuseChildAllocation {
        tasks: Cell::new(0),
    };
    let factory_ran = Cell::new(false);
    let (outcome, observed) = observation::collect(|| {
        try_with_attempt(&db, 100, || {
            let factory_ran = &factory_ran;
            Driver::run_with_admission(&db, &admission, |endpoint| async move {
                assert!(endpoint.queue.pending.borrow().is_empty());
                let result = endpoint.demand(move || {
                    factory_ran.set(true);
                    async { Ok(()) }
                });
                assert!(matches!(
                    result,
                    Err(RunError::Refused(Incomplete::Allowance))
                ));
                assert!(endpoint.queue.pending.borrow().is_empty());
                Err::<(), _>(RunError::Refused(Incomplete::Allowance))
            })
        })
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(admission.tasks.get(), 2);
    assert_eq!(observed.polls, 1);
    assert!(!factory_ran.get());
    assert_restored_and_retry(&db);
}

#[test]
fn unwanted_completed_output_is_discarded_without_refusing_the_run() {
    let db = DatabaseImpl::default();
    let journal = Rc::new(Journal::default());
    let escaped = Rc::new(RefCell::new(None));
    let outcome = try_with_attempt(&db, 100, || {
        let journal = journal.clone();
        let escaped = escaped.clone();
        Driver::run(&db, |endpoint| async move {
            let _ancestor = Ancestor::new(journal.clone());
            let mut started = false;
            poll_fn(|_| {
                if started {
                    return Poll::Ready(Ok(()));
                }
                started = true;
                let future = Completing::new(
                    endpoint.clone(),
                    journal.clone(),
                    escaped.clone(),
                    Completion::Ready,
                    Retirement::Abandon,
                );
                *escaped.borrow_mut() = Some(endpoint.demand(move || future)?);
                Poll::Pending
            })
            .await
        })
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    journal.assert_ownership();
    assert!(escaped.borrow().is_none());
    assert_eq!(
        *journal.events.borrow(),
        ["return", "future", "output", "ancestor"]
    );
    assert_restored_and_retry(&db);
}

#[crate::db]
#[derive(Clone)]
struct CallbackDb {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl crate::Database for CallbackDb {}

thread_local! {
    static CALLBACK_ENDPOINT: RefCell<Option<Endpoint<'static, 'static>>> = const { RefCell::new(None) };
    static CALLBACK_JOURNAL: RefCell<Option<Rc<Journal>>> = const { RefCell::new(None) };
    static CALLBACK_CHILD: RefCell<Option<Reply<()>>> = const { RefCell::new(None) };
}

struct ClearCallback;

impl Drop for ClearCallback {
    fn drop(&mut self) {
        CALLBACK_ENDPOINT.with_borrow_mut(|endpoint| *endpoint = None);
        CALLBACK_JOURNAL.with_borrow_mut(|journal| *journal = None);
        CALLBACK_CHILD.with_borrow_mut(|child| *child = None);
    }
}

struct ArmCompletionCallback {
    endpoint: Endpoint<'static, 'static>,
    journal: Rc<Journal>,
    escaped: Escaped,
    output: RefCell<Option<Owned>>,
}

impl Future for ArmCompletionCallback {
    type Output = RunResult<Owned>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.as_ref().get_ref();
        this.journal.events.borrow_mut().push("return");
        Poll::Ready(Ok(this
            .output
            .borrow_mut()
            .take()
            .expect("only one Ready poll")))
    }
}

impl Drop for ArmCompletionCallback {
    fn drop(&mut self) {
        self.journal
            .output_left_in_future
            .set(self.output.borrow().is_some());
        self.journal.published_before_retirement.set(
            self.escaped
                .borrow()
                .as_ref()
                .is_some_and(|reply| reply.value.borrow().as_ref().is_some_and(Result::is_ok)),
        );
        self.journal.record_drop("future");
        CALLBACK_JOURNAL.with_borrow_mut(|journal| *journal = Some(self.journal.clone()));
        CALLBACK_ENDPOINT.with_borrow_mut(|endpoint| {
            assert!(endpoint.replace(self.endpoint.clone()).is_none());
        });
    }
}

#[test]
fn completion_callback_cannot_queue_a_child_after_the_last_validation() {
    let _clear = ClearCallback;
    // The event hook is static; its endpoint borrows a dedicated test database with the same lifetime.
    let db: &'static CallbackDb = Box::leak(Box::new(CallbackDb {
        storage: crate::Storage::new(Some(Box::new(|event| {
            if !matches!(event.kind, crate::EventKind::WillCheckCancellation) {
                return;
            }
            // Remove the endpoint before demand performs its own cancellation checks.
            let Some(endpoint) = CALLBACK_ENDPOINT.with_borrow_mut(Option::take) else {
                return;
            };
            let journal = CALLBACK_JOURNAL
                .with_borrow(|journal| journal.as_ref().expect("callback journal").clone());
            journal.events.borrow_mut().push("completion callback");
            let owner = QueuedChild(journal);
            let child = endpoint
                .demand(move || {
                    poll_fn(move |_| -> Poll<RunResult<()>> {
                        let _owner = &owner;
                        panic!("the completion callback's child must not run");
                    })
                })
                .expect("the completion check still owns the active poll");
            CALLBACK_CHILD.with_borrow_mut(|slot| assert!(slot.replace(child).is_none()));
        }))),
    }));
    let journal = Rc::new(Journal::default());
    let escaped = Rc::new(RefCell::new(None));
    let outcome = try_with_attempt(db, 100, || {
        let journal = journal.clone();
        let escaped = escaped.clone();
        Driver::<'static, 'static>::run(db, |endpoint| async move {
            let _ancestor = Ancestor::new(journal.clone());
            let future = ArmCompletionCallback {
                endpoint: endpoint.clone(),
                output: RefCell::new(Some(Owned {
                    journal: journal.clone(),
                    _pin: PhantomPinned,
                })),
                journal,
                escaped: escaped.clone(),
            };
            *escaped.borrow_mut() = Some(endpoint.demand(move || future)?);
            pending::<RunResult<()>>().await
        })
    });
    assert_eq!(
        outcome,
        Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
    );
    journal.assert_ownership();
    assert!(!matches!(
        poll_reply(&escaped, &mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(_))
    ));
    assert_eq!(
        *journal.events.borrow(),
        [
            "return",
            "future",
            "completion callback",
            "queued child",
            "output",
            "ancestor"
        ]
    );
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
    drop(_clear);
    assert_eq!(
        try_with_attempt(db, 100, || Driver::run(db, |_| async { Ok(()) })),
        Ok(AttemptOutcome::Complete(Ok(())))
    );
}
