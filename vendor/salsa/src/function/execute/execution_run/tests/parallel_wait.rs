use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any, resume_unwind};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use super::super::registration::{
    ExecutableRouteProvider, ProviderContext, RegistryBuilder, Route,
};
use super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use super::validation_trace::{self, TraceEvent};
use crate::attempt_probe::paired_test_support::{PairInstallError, Participant, run_pair};
use crate::attempt_probe::transfer_test_support::{
    self as transfer_trace, Action, Event as Observation, GraphSnapshot, Kind, MemoSnapshot, Mode,
    Record, SessionSnapshot, SyncSnapshot, TraceConfig, TransferTrace,
};
use crate::attempt_probe::{self, AttemptOutcome, AttemptSupport, Incomplete, StartError};
use crate::function::{ClaimResult, Configuration, IngredientImpl, Reentrancy, SyncOwner};
use crate::plumbing::AsId;
use crate::runtime::WaitResult;
use crate::zalsa::ZalsaDatabase;
use crate::{
    CancellationToken, Cancelled, Cycle, Database, DatabaseImpl, DatabaseKeyIndex, Durability,
    Event, EventKind, Id, Setter,
};

const STAGE_TIMEOUT: Duration = Duration::from_secs(5);
const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const CHILD_MARKER: &str = "SALSA_PAIRED_WAIT_PREREQUISITE_CHILD";
const TEST_NAME: &str = "function::execute::execution_run::tests::parallel_wait::paired_registration_and_canonical_waits";
const CANCELLATION_CHILD_MARKER: &str = "SALSA_INSTALLED_WAIT_CANCELLATION_CHILD";
const CANCELLATION_TEST_NAME: &str =
    "function::execute::execution_run::tests::parallel_wait::installed_wait_native_cancellation";
const WAIT_ALLOWANCE: usize = 10_000;
// Retained-input access, provider quotation, and handle conversion each cost one unit.
const INPUT_CONVERSION_WORK: usize = 3;
// Provider quotation and scalar equality each cost one unit.
const SCALAR_COMPARISON_WORK: usize = 2;
// Canonical delivery without an enclosing query admits one unit and no dependency storage.
const ROOT_READ_WORK: usize = 1;

#[derive(Default)]
struct Counts {
    value: AtomicUsize,
    consumer: AtomicUsize,
}

impl Counts {
    fn snapshot(&self) -> [usize; 2] {
        [
            self.value.load(Ordering::SeqCst),
            self.consumer.load(Ordering::SeqCst),
        ]
    }
}

#[crate::db]
trait TestDatabase: Database {
    fn counts(&self) -> &Counts;
}

#[crate::db]
#[derive(Clone)]
struct Db {
    storage: crate::Storage<Self>,
    counts: Arc<Counts>,
}

#[crate::db]
impl Database for Db {}

#[crate::db]
impl TestDatabase for Db {
    fn counts(&self) -> &Counts {
        &self.counts
    }
}

impl Default for Db {
    fn default() -> Self {
        Self {
            storage: crate::Storage::new(Some(Box::new(wait_event))),
            counts: Arc::new(Counts::default()),
        }
    }
}

#[crate::input]
struct Input {
    #[returns(copy)]
    number: u32,
}

fn value_body(db: &dyn TestDatabase, input: Input) -> u32 {
    independent::before_value_body(db, input);
    db.counts().value.fetch_add(1, Ordering::SeqCst);
    input.number(db)
}

fn consumer_body(value: u32) -> u32 {
    value.saturating_add(1)
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn value(db: &dyn TestDatabase, input: Input) -> u32 {
    value_body(db, input)
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn consumer(db: &dyn TestDatabase, input: Input) -> u32 {
    db.counts().consumer.fetch_add(1, Ordering::SeqCst);
    consumer_body(value(db, input))
}

struct Admission;

impl ExecutionAdmission for Admission {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

static ADMISSION: Admission = Admission;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Case {
    Cold,
    Validation,
    OwnerPanic,
    WaitEventPanic,
}

#[derive(Debug)]
struct OwnerPanic(Arc<()>);

#[derive(Debug)]
struct WaitEventPanic(Arc<()>);

#[derive(Debug)]
struct BeforeInstallPanic(Arc<()>);

#[derive(Clone, Copy, Debug)]
enum Gate {
    Ready,
    AbortSchedule,
}

/// The schedule ends after the first real wait. Canonical retries never revisit its gates.
struct Schedule {
    owner: bool,
    case: Case,
    outbound: mpsc::Sender<Gate>,
    inbound: RefCell<Option<mpsc::Receiver<Gate>>>,
    body_entered: Cell<bool>,
    identity: Arc<()>,
    installed_wait: Option<InstalledWait>,
}

impl Schedule {
    fn receive(&self) -> RunResult<()> {
        let Some(inbound) = self.inbound.borrow_mut().take() else {
            return Ok(());
        };
        match inbound.recv_timeout(STAGE_TIMEOUT) {
            Ok(Gate::Ready) => Ok(()),
            Ok(Gate::AbortSchedule) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(RunError::Refused(Incomplete::Interrupted))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                eprintln!(
                    "PAIRED_WAIT {:?}: fixture stage timed out (owner={})",
                    self.case, self.owner
                );
                Err(RunError::Refused(Incomplete::Interrupted))
            }
        }
    }

    fn before_root(&self) -> RunResult<()> {
        if !self.owner {
            self.receive()?;
        }
        Ok(())
    }

    fn before_value(&self, db: &dyn TestDatabase, key: DatabaseKeyIndex) -> RunResult<()> {
        if !self.owner || self.body_entered.replace(true) {
            return Ok(());
        }
        native_cancellation::before_value(db, key);
        eprintln!(
            "PAIRED_WAIT {:?}: owner has the real value claim",
            self.case
        );
        if self.case == Case::OwnerPanic {
            panic_any(OwnerPanic(self.identity.clone()));
        }
        let _ = self.outbound.send(Gate::Ready);
        self.receive()?;
        if let Some(installed) = &self.installed_wait {
            installed.release_owner(db, key);
        }
        Ok(())
    }
}

impl Drop for Schedule {
    fn drop(&mut self) {
        // This owner surrounds Participant::run, so query and driver unwind finish before
        // an early worker exit releases the peer's outstanding fixture gate.
        let _ = self.outbound.send(Gate::AbortSchedule);
    }
}

struct WaitEvent {
    key: DatabaseKeyIndex,
    release_owner: mpsc::Sender<Gate>,
    observed: Arc<AtomicUsize>,
    panic: Option<Arc<()>>,
}

thread_local! {
    static WAIT_EVENT: RefCell<Option<WaitEvent>> = const { RefCell::new(None) };
    static CANCELLATION_EVENT: RefCell<Option<(CancellationToken, DatabaseKeyIndex)>> = const { RefCell::new(None) };
}

fn wait_event(event: Event) {
    native_cancellation::observe_event(&event);
    if matches!(&event.kind, EventKind::WillCheckCancellation) {
        CANCELLATION_EVENT.with_borrow(|observer| {
            if let Some((token, key)) = observer {
                let mut observation = Observation::new(Kind::Gate).key(*key);
                observation.phase = Some("will.check.cancellation");
                observation.decision = token.is_cancelled();
                transfer_trace::record(observation);
            }
        });
    }
    let EventKind::WillBlockOn { database_key, .. } = event.kind else {
        return;
    };
    let waiting = WAIT_EVENT.with_borrow_mut(Option::take);
    if let Some(waiting) = waiting {
        assert_eq!(database_key, waiting.key);
        waiting.observed.fetch_add(1, Ordering::SeqCst);
        // Running still owns its synchronization guards. The event only sends a one-shot
        // notification; it never waits for acknowledgement or calls back into Salsa.
        let _ = waiting.release_owner.send(Gate::Ready);
        if let Some(identity) = waiting.panic {
            panic_any(WaitEventPanic(identity));
        }
    }
}

struct EventInstalled;

impl Drop for EventInstalled {
    fn drop(&mut self) {
        WAIT_EVENT.with_borrow_mut(|event| *event = None);
        CANCELLATION_EVENT.with_borrow_mut(|event| *event = None);
    }
}

struct Providers<'a, 'db, A: Configuration, B: Configuration> {
    value: Route<'db, A>,
    consumer: Route<'db, B>,
    schedule: &'a Schedule,
}

impl<'run, 'db: 'run, A, B, C> ExecutableRouteProvider<'run, 'db, C> for Providers<'run, 'db, A, B>
where
    A: Configuration<DbView = dyn TestDatabase, Input<'db> = Input, Output<'db> = u32>,
    B: Configuration<DbView = dyn TestDatabase, Input<'db> = Input, Output<'db> = u32>,
    C: Configuration<DbView = dyn TestDatabase, Input<'db> = Input, Output<'db> = u32>,
{
    // Conversion reconstructs one Input handle; equality compares one u32.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn TestDatabase,
        input: Input,
    ) -> RunResult<u32> {
        let key = db.zalsa_local().active_query().map(|(key, _)| key);
        if TypeId::of::<C>() == TypeId::of::<A>() {
            assert_eq!(key, Some(self.value.database_key(input.as_id())));
            context
                .endpoint()
                .local_call(|| {
                    self.schedule
                        .before_value(db, self.value.database_key(input.as_id()))
                })
                .await;
            let value = value_body(db, input);
            if self.schedule.installed_wait.is_some() {
                let mut event =
                    Observation::new(Kind::BodyValue).key(self.value.database_key(input.as_id()));
                event.value = value;
                transfer_trace::record(event);
            }
            Ok(value)
        } else {
            assert_eq!(TypeId::of::<C>(), TypeId::of::<B>());
            assert_eq!(key, Some(self.consumer.database_key(input.as_id())));
            db.counts().consumer.fetch_add(1, Ordering::SeqCst);
            let value = context.fetch_ref(&self.value, input.as_id())?.await?;
            Ok(consumer_body(*value))
        }
    }

    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn TestDatabase,
        _id: Id,
        _input: Input,
    ) -> RunResult<u32> {
        Err(RunError::Contract(
            "acyclic paired fixture requested an initial value",
        ))
    }

    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn TestDatabase,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: Input,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::Contract(
            "acyclic paired fixture requested recovery",
        ))
    }
}

fn run_queries<'db, A, B>(
    db: &'db dyn TestDatabase,
    value_ingredient: &'db IngredientImpl<A>,
    consumer_ingredient: &'db IngredientImpl<B>,
    input: Input,
    schedule: &Schedule,
) -> RunResult<u32>
where
    A: Configuration<DbView = dyn TestDatabase, Input<'db> = Input, Output<'db> = u32>,
    B: Configuration<DbView = dyn TestDatabase, Input<'db> = Input, Output<'db> = u32>,
{
    run_queries_with_admission(
        db,
        value_ingredient,
        consumer_ingredient,
        input,
        schedule,
        &ADMISSION,
    )
}

fn run_queries_with_admission<'db, A, B>(
    db: &'db dyn TestDatabase,
    value_ingredient: &'db IngredientImpl<A>,
    consumer_ingredient: &'db IngredientImpl<B>,
    input: Input,
    schedule: &Schedule,
    admission: &dyn ExecutionAdmission,
) -> RunResult<u32>
where
    A: Configuration<DbView = dyn TestDatabase, Input<'db> = Input, Output<'db> = u32>,
    B: Configuration<DbView = dyn TestDatabase, Input<'db> = Input, Output<'db> = u32>,
{
    let mut registry = RegistryBuilder::new(db, admission)?;
    let value = registry.reserve(db, value_ingredient)?;
    let consumer = registry.reserve(db, consumer_ingredient)?;
    let providers = Providers {
        value: value.clone(),
        consumer: consumer.clone(),
        schedule,
    };
    let mut registry = registry;
    let binding = registry.provider(&providers)?;
    registry.bind_executable(&value, &binding)?;
    registry.bind_executable(&consumer, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        endpoint.local_call(|| schedule.before_root()).await;
        let context = endpoint.provider(binding)?;
        let result = if !schedule.owner && schedule.case == Case::Validation {
            context.fetch_ref(&consumer, input.as_id())?.await?
        } else {
            context.fetch_ref(&value, input.as_id())?.await?
        };
        Ok(*result)
    })
}

fn assert_worker_clean(db: &dyn Database) {
    assert!(attempt_probe::current().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(db.zalsa_local().active_query().is_none());
}

fn assert_exclusion_released(db: &dyn Database) {
    assert_worker_clean(db);
    assert_eq!(db.zalsa().attempt_operations.load(Ordering::SeqCst), 0);
    assert_eq!(
        attempt_probe::try_with_attempt(db, 0, || ()),
        Ok(AttemptOutcome::Complete(()))
    );
}

fn empty_run(db: &dyn Database, units: usize) -> RunResult<()> {
    RegistryBuilder::new(db, &ADMISSION)?
        .seal()?
        .run(|endpoint| async move {
            endpoint.local_call(|| endpoint.admit_work(units)).await;
            Ok(())
        })
}

fn empty_worker(
    db: DatabaseImpl,
    participant: Participant<'_>,
    allowance: usize,
    spent: usize,
) -> AttemptSupport {
    let support = RefCell::new(None);
    let outcome = participant.run(&db, allowance, || {
        assert!(attempt_probe::current().unwrap().is_current(db.zalsa()));
        *support.borrow_mut() = attempt_probe::current();
        empty_run(&db, spent)?;
        attempt_probe::charge(&db, allowance - spent).map_err(RunError::Refused)
    });
    assert_eq!(outcome, Ok(AttemptOutcome::Complete(Ok(()))));
    assert_worker_clean(&db);
    support
        .into_inner()
        .expect("participant installed its real support")
}

fn registration_lifecycle() {
    eprintln!("PAIRED_WAIT registration: independent sessions and normal cleanup");
    let db = DatabaseImpl::default();
    let (left, right) = run_pair(
        &db,
        |db, participant| empty_worker(db, participant, 7, 2),
        |db, participant| empty_worker(db, participant, 19, 3),
    )
    .unwrap();
    let (left, right) = (left.unwrap(), right.unwrap());
    assert!(!left.same_owner(&right));
    assert!(!left.owns_current_session(db.zalsa()));
    assert!(!right.owns_current_session(db.zalsa()));
    assert_exclusion_released(&db);

    let (refused, completed) = run_pair(
        &db,
        |db, participant| {
            let outcome = participant.run(&db, 1, || empty_run(&db, 2));
            assert_worker_clean(&db);
            outcome
        },
        |db, participant| empty_worker(db, participant, 19, 3),
    )
    .unwrap();
    assert_eq!(
        refused.unwrap(),
        Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
    );
    assert_eq!(completed.unwrap().reason(), None);
    assert_exclusion_released(&db);

    for panic in [false, true] {
        let ran = AtomicBool::new(false);
        let identity = Arc::new(());
        let expected = identity.clone();
        let (early, peer) = run_pair(
            &db,
            move |_db, _participant| {
                if panic {
                    panic_any(BeforeInstallPanic(identity));
                }
            },
            |db, participant| {
                let outcome = participant.run(&db, 10, || ran.store(true, Ordering::SeqCst));
                assert_worker_clean(&db);
                outcome
            },
        )
        .unwrap();
        if panic {
            let payload = early.expect_err("early worker retains its native failure");
            assert!(Arc::ptr_eq(
                &payload.downcast_ref::<BeforeInstallPanic>().unwrap().0,
                &expected
            ));
        } else {
            early.unwrap();
        }
        assert_eq!(peer.unwrap(), Err(PairInstallError::SetupAborted));
        assert!(!ran.load(Ordering::SeqCst));
        assert_exclusion_released(&db);
    }

    let (rejected, peer) = run_pair(
        &db,
        |db, participant| {
            let other = DatabaseImpl::default();
            let outcome = attempt_probe::try_with_attempt(&other, 10, || {
                participant.run(&db, 10, || -> () {
                    panic!("rejected installation ran its body")
                })
            });
            assert_worker_clean(&db);
            outcome
        },
        |db, participant| {
            participant.run(&db, 10, || -> () { panic!("aborted setup ran its body") })
        },
    )
    .unwrap();
    assert!(matches!(
        rejected.unwrap(),
        Ok(AttemptOutcome::Complete(Err(PairInstallError::Start(
            StartError::NestedAttempt
        ))))
    ));
    assert!(matches!(peer.unwrap(), Err(PairInstallError::SetupAborted)));
    assert_exclusion_released(&db);

    assert!(matches!(
        attempt_probe::try_with_attempt(&db, 0, || { run_pair(&db, |_, _| (), |_, _| ()) }),
        Ok(AttemptOutcome::Complete(Err(StartError::NestedAttempt)))
    ));
    assert!(matches!(
        attempt_probe::try_with_operation(&db, || { run_pair(&db, |_, _| (), |_, _| ()) }),
        Ok(Err(StartError::ActiveOperation))
    ));
    assert_exclusion_released(&db);
}

struct WorkerReport {
    owner: bool,
    support: AttemptSupport,
    trace: Vec<TraceEvent>,
}

fn query_worker(
    db: Db,
    participant: Participant<'_>,
    input: Input,
    schedule: Schedule,
    observed_waits: Arc<AtomicUsize>,
    reports: mpsc::Sender<WorkerReport>,
) -> Result<AttemptOutcome<RunResult<u32>>, PairInstallError> {
    let support = RefCell::new(None);
    let _event = EventInstalled;
    if !schedule.owner {
        WAIT_EVENT.with_borrow_mut(|event| {
            *event = Some(WaitEvent {
                key: value::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id()),
                release_owner: schedule.outbound.clone(),
                observed: observed_waits,
                panic: (schedule.case == Case::WaitEventPanic).then(|| schedule.identity.clone()),
            });
        });
    }
    let (outcome, trace) = validation_trace::collect(|| {
        catch_unwind(AssertUnwindSafe(|| {
            participant.run(&db, 10_000, || {
                *support.borrow_mut() = attempt_probe::current();
                run_queries(
                    &db,
                    value::fn_ingredient_(&db, db.zalsa()),
                    consumer::fn_ingredient_(&db, db.zalsa()),
                    input,
                    &schedule,
                )
            })
        }))
    });
    assert_worker_clean(&db);
    let support = support
        .into_inner()
        .expect("query participant reached its body");
    assert!(!support.owns_current_session(db.zalsa()));
    validation_trace::emit(
        if schedule.owner {
            "paired.owner"
        } else {
            "paired.waiter"
        },
        &trace,
    );
    let _ = reports.send(WorkerReport {
        owner: schedule.owner,
        support,
        trace,
    });
    match outcome {
        Ok(outcome) => outcome,
        Err(payload) => resume_unwind(payload),
    }
}

fn dependencies<C: Configuration>(
    db: &Db,
    ingredient: &IngredientImpl<C>,
    input: Input,
) -> Vec<String> {
    let memo = ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            input.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
        )
        .expect("completed query has its canonical memo");
    memo.header
        .origin()
        .inputs()
        .map(|key| {
            assert_eq!(key.key_index(), input.as_id());
            db.zalsa()
                .lookup_ingredient(key.ingredient_index())
                .debug_name()
                .to_owned()
        })
        .collect()
}

fn assert_claim_released<C: Configuration>(db: &Db, ingredient: &IngredientImpl<C>, input: Input) {
    match ingredient.sync_table.try_claim(
        db.zalsa(),
        db.zalsa_local(),
        input.as_id(),
        Reentrancy::Deny,
    ) {
        ClaimResult::Claimed(claim) => claim.abort(),
        _ => panic!("paired worker retained its query claim"),
    }
}

fn assert_wait_retry(
    trace: &[TraceEvent],
    phase: &'static str,
    probe: &'static str,
    key: DatabaseKeyIndex,
) {
    let wait = trace
        .iter()
        .position(|event| {
            matches!(event,
                TraceEvent::Outer { phase: actual, key: Some(actual_key), .. }
                if *actual == phase && *actual_key == key
            )
        })
        .expect("waiter reached the required immediate Running arm");
    assert!(
        trace[wait + 1..].iter().any(|event| matches!(event,
            TraceEvent::Outer { phase: actual, key: Some(actual_key), .. }
            if *actual == probe && *actual_key == key
        )),
        "accepted wait must re-probe the actual retained request"
    );
}

fn query_case(case: Case) {
    eprintln!("PAIRED_WAIT {case:?}: begin");
    let mut db = Db::default();
    let input = Input::new(&db, 4);
    let mut baseline = Db::default();
    let baseline_input = Input::new(&baseline, 4);
    if case == Case::Validation {
        assert_eq!(consumer(&db, input), 5);
        assert_eq!(consumer(&baseline, baseline_input), 5);
        input.set_number(&mut db).to(9);
        baseline_input.set_number(&mut baseline).to(9);
    }
    let before = db.counts.snapshot();
    let identity = Arc::new(());
    let waits = Arc::new(AtomicUsize::new(0));
    let (start_tx, start_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (reports_tx, reports_rx) = mpsc::channel();
    let owner = Schedule {
        owner: true,
        case,
        outbound: start_tx,
        inbound: RefCell::new(Some(release_rx)),
        body_entered: Cell::new(false),
        identity: identity.clone(),
        installed_wait: None,
    };
    let waiter = Schedule {
        owner: false,
        case,
        outbound: release_tx,
        inbound: RefCell::new(Some(start_rx)),
        body_entered: Cell::new(false),
        identity: identity.clone(),
        installed_wait: None,
    };
    let owner_waits = waits.clone();
    let owner_reports = reports_tx.clone();
    let waiter_waits = waits.clone();
    let (owner, waiter) = run_pair(
        &db,
        move |db, participant| {
            query_worker(db, participant, input, owner, owner_waits, owner_reports)
        },
        move |db, participant| {
            query_worker(db, participant, input, waiter, waiter_waits, reports_tx)
        },
    )
    .unwrap();
    let reports = [
        reports_rx.recv_timeout(STAGE_TIMEOUT).unwrap(),
        reports_rx.recv_timeout(STAGE_TIMEOUT).unwrap(),
    ];
    assert!(!reports[0].support.same_owner(&reports[1].support));
    let waiter_trace = &reports.iter().find(|report| !report.owner).unwrap().trace;
    let value_key = value::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
    let consumer_key = consumer::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
    assert_exclusion_released(&db);

    match case {
        Case::OwnerPanic => {
            let payload = owner.expect_err("the owner fails before allowing a real wait");
            assert!(Arc::ptr_eq(
                &payload.downcast_ref::<OwnerPanic>().unwrap().0,
                &identity
            ));
            assert_eq!(
                waiter.unwrap(),
                Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
            );
            assert_eq!(waits.load(Ordering::SeqCst), 0);
            assert_eq!(db.counts.snapshot(), before);
        }
        Case::WaitEventPanic => {
            assert_eq!(owner.unwrap(), Ok(AttemptOutcome::Complete(Ok(4))));
            let payload = waiter.expect_err("WillBlockOn retains its typed native failure");
            assert!(Arc::ptr_eq(
                &payload.downcast_ref::<WaitEventPanic>().unwrap().0,
                &identity
            ));
            assert_eq!(waits.load(Ordering::SeqCst), 1);
            assert!(waiter_trace.iter().any(|event| matches!(event,
                TraceEvent::Outer { phase: "fetch.wait", key: Some(key), .. } if *key == value_key
            )));
            assert_eq!(db.counts.snapshot(), [1, 0]);
        }
        Case::Cold | Case::Validation => {
            let expected_value = if case == Case::Validation { 9 } else { 4 };
            let expected_waiter = if case == Case::Validation { 10 } else { 4 };
            assert_eq!(
                owner.unwrap(),
                Ok(AttemptOutcome::Complete(Ok(expected_value)))
            );
            assert_eq!(
                waiter.unwrap(),
                Ok(AttemptOutcome::Complete(Ok(expected_waiter)))
            );
            assert_eq!(waits.load(Ordering::SeqCst), 1);
            if case == Case::Validation {
                assert_wait_retry(waiter_trace, "validation.wait", "probe", value_key);
                assert!(waiter_trace.iter().any(|event| matches!(event,
                    TraceEvent::Request { owner, key, .. } if *owner == consumer_key && *key == value_key
                )));
                assert!(waiter_trace.iter().any(|event| matches!(event,
                    TraceEvent::Reply { owner, key, result } if *owner == consumer_key && *key == value_key && !result.is_unchanged()
                )));
                assert_eq!(db.counts.snapshot(), [before[0] + 1, before[1] + 1]);
                assert_eq!(consumer(&db, input), consumer(&baseline, baseline_input));
                assert_eq!(
                    dependencies(&db, consumer::fn_ingredient_(&db, db.zalsa()), input),
                    dependencies(
                        &baseline,
                        consumer::fn_ingredient_(&baseline, baseline.zalsa()),
                        baseline_input
                    ),
                );
            } else {
                assert_wait_retry(waiter_trace, "fetch.wait", "fetch.probe", value_key);
                assert_eq!(db.counts.snapshot(), [1, 0]);
            }
        }
    }
    assert_claim_released(&db, value::fn_ingredient_(&db, db.zalsa()), input);
    assert_claim_released(&db, consumer::fn_ingredient_(&db, db.zalsa()), input);
    assert_eq!(value(&db, input), value(&baseline, baseline_input));
    let value_dependencies = dependencies(&db, value::fn_ingredient_(&db, db.zalsa()), input);
    assert_eq!(value_dependencies.len(), 1);
    assert_eq!(
        value_dependencies,
        dependencies(
            &baseline,
            value::fn_ingredient_(&baseline, baseline.zalsa()),
            baseline_input
        )
    );
    let completed = db.counts.snapshot();
    assert_eq!(value(&db, input), value(&baseline, baseline_input));
    assert_eq!(db.counts.snapshot(), completed);
    assert_exclusion_released(&db);
    eprintln!("PAIRED_WAIT {case:?}: both workers joined and exclusion released");
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WaitAction {
    Complete,
    CancelOwner,
    CancelWaiter,
}

#[derive(Clone, Copy, Debug)]
struct WaitProof {
    graph: GraphSnapshot,
    claim: SyncSnapshot,
    owner: ThreadId,
    waiter: ThreadId,
}

impl WaitProof {
    fn capture(db: &dyn TestDatabase, key: DatabaseKeyIndex, waiter: ThreadId) -> Self {
        let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
        let claim = db
            .zalsa()
            .lookup_ingredient(key.ingredient_index())
            .as_function()
            .expect("the wait targets a registered function")
            .sync_table()
            .test_transfer_state(key.key_index())
            .expect("the paused owner still holds its actual claim");
        let proof = Self {
            graph,
            claim,
            owner: thread::current().id(),
            waiter,
        };
        proof.assert_installed(key);
        proof
    }

    fn assert_installed(self, key: DatabaseKeyIndex) {
        let graph = self.graph;
        assert!(!graph.edges.overflow);
        assert_eq!(
            graph
                .edges
                .entries
                .into_iter()
                .flatten()
                .collect::<Vec<_>>(),
            [(self.waiter, self.owner)]
        );
        assert!(!graph.dependents.overflow);
        let dependents = graph
            .dependents
            .entries
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(dependents.len(), 1);
        assert_eq!(dependents[0].0, key);
        assert!(!dependents[0].1.overflow);
        assert_eq!(
            dependents[0]
                .1
                .entries
                .into_iter()
                .flatten()
                .collect::<Vec<_>>(),
            [self.waiter]
        );
        assert!(!graph.pending.overflow && graph.pending.is_empty());
        assert!(!graph.transferred.overflow && graph.transferred.is_empty());
        assert!(!graph.reverse.overflow && graph.reverse.is_empty());
        assert!(matches!(self.claim.owner, SyncOwner::Thread(owner) if owner == self.owner));
        assert!(self.claim.anyone_waiting);
        assert!(!self.claim.claimed_twice && !self.claim.is_transfer_target);
    }
}

struct InstalledWait {
    action: WaitAction,
    waiter: Option<mpsc::Receiver<(ThreadId, CancellationToken)>>,
    proof: Cell<Option<WaitProof>>,
    cancelled_proof: Cell<Option<WaitProof>>,
}

impl InstalledWait {
    fn new(
        action: WaitAction,
        waiter: Option<mpsc::Receiver<(ThreadId, CancellationToken)>>,
    ) -> Self {
        Self {
            action,
            waiter,
            proof: Cell::new(None),
            cancelled_proof: Cell::new(None),
        }
    }

    fn release_owner(&self, db: &dyn TestDatabase, key: DatabaseKeyIndex) {
        let (waiter, token) = self
            .waiter
            .as_ref()
            .expect("only the owner receives waiter metadata")
            .recv_timeout(STAGE_TIMEOUT)
            .expect("the waiter sent its actual handle's cancellation token");

        // The notification is sent while WillBlockOn holds the graph mutex. Taking
        // it here waits for the real edge and condvar handoff while this query is claimed.
        self.proof.set(Some(WaitProof::capture(db, key, waiter)));
        wait_phase("installed", key);
        match self.action {
            WaitAction::Complete => wait_phase("release.normal", key),
            WaitAction::CancelOwner => {
                let owner_token = db.cancellation_token();
                owner_token.cancel();
                assert!(owner_token.is_cancelled());
                assert!(!token.is_cancelled());
                wait_phase("cancel.owner", key);
            }
            WaitAction::CancelWaiter => {
                token.cancel();
                assert!(token.is_cancelled());
                assert!(!db.cancellation_token().is_cancelled());
                wait_phase("cancel.waiter", key);
                // Local cancellation does not unregister a sleeping native waiter.
                // Observe that edge before allowing its query owner to complete.
                self.cancelled_proof
                    .set(Some(WaitProof::capture(db, key, waiter)));
                wait_phase("cancel.waiter.retained", key);
            }
        }
    }
}

fn wait_phase(phase: &'static str, key: DatabaseKeyIndex) {
    let mut event = Observation::new(Kind::Gate).key(key);
    event.phase = Some(phase);
    transfer_trace::record(event);
    eprintln!("INSTALLED_WAIT {phase}: {key:?}");
}

struct ObservedAdmission<'a> {
    db: &'a dyn TestDatabase,
}

impl ExecutionAdmission for ObservedAdmission<'_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let outcome = ADMISSION.admit(work);
        validation_trace::record(TraceEvent::Admission {
            work,
            outcome: Some(outcome),
            owner: self.db.zalsa_local().active_query().map(|(key, _)| key),
            depths: attempt_probe::stack_depths(),
        });
        let mut event = Observation::new(Kind::Admission);
        event.phase = Some(match work {
            ExecutionWork::Task { .. } => "observed.task",
            ExecutionWork::Resource { .. } => "observed.resource",
            ExecutionWork::Work { units } => {
                event.units = units;
                "observed.work"
            }
            ExecutionWork::Poll => "observed.poll",
        });
        transfer_trace::record(event);
        outcome
    }
}

struct ExitSession<'a>(&'a Cell<Option<SessionSnapshot>>);

impl Drop for ExitSession<'_> {
    fn drop(&mut self) {
        self.0.set(transfer_trace::session_snapshot());
    }
}

type PairedQueryOutcome = Result<AttemptOutcome<RunResult<u32>>, PairInstallError>;

struct InstalledWorker {
    schedule: Schedule,
    metadata: Option<mpsc::Sender<(ThreadId, CancellationToken)>>,
    observed_waits: Arc<AtomicUsize>,
    config: TraceConfig,
    reports: mpsc::Sender<InstalledReport>,
}

struct InstalledReport {
    worker: usize,
    thread: ThreadId,
    support: AttemptSupport,
    exit: SessionSnapshot,
    token_after_cleanup: bool,
    driver: Option<RunResult<u32>>,
    proof: Option<WaitProof>,
    cancelled_proof: Option<WaitProof>,
    validation: Vec<TraceEvent>,
    trace: TransferTrace,
}

fn installed_wait_worker(
    db: Db,
    participant: Participant<'_>,
    input: Input,
    data: InstalledWorker,
) -> PairedQueryOutcome {
    let InstalledWorker {
        schedule,
        metadata,
        observed_waits,
        config,
        reports,
    } = data;
    let worker = config.worker;
    let token = db.cancellation_token();
    assert!(!token.is_cancelled());
    if let Some(metadata) = metadata {
        metadata
            .send((thread::current().id(), token.clone()))
            .expect("the owner retains the metadata receiver");
    }
    let key = value::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
    let event_guard = EventInstalled;
    CANCELLATION_EVENT.with_borrow_mut(|observer| {
        assert!(observer.replace((token.clone(), key)).is_none());
    });
    if !schedule.owner {
        WAIT_EVENT.with_borrow_mut(|event| {
            assert!(
                event
                    .replace(WaitEvent {
                        key,
                        release_owner: schedule.outbound.clone(),
                        observed: observed_waits,
                        panic: None,
                    })
                    .is_none()
            );
        });
    }
    let support = RefCell::new(None);
    let exit = Cell::new(None);
    let driver = Cell::new(None);
    let ((outcome, validation), trace) = transfer_trace::collect(config, || {
        validation_trace::collect(|| {
            catch_unwind(AssertUnwindSafe(|| {
                participant.run(&db, WAIT_ALLOWANCE, || {
                    let _exit = ExitSession(&exit);
                    *support.borrow_mut() = attempt_probe::current();
                    wait_phase("participant.enter", key);
                    let result = run_queries_with_admission(
                        &db,
                        value::fn_ingredient_(&db, db.zalsa()),
                        consumer::fn_ingredient_(&db, db.zalsa()),
                        input,
                        &schedule,
                        &ObservedAdmission { db: &db },
                    );
                    driver.set(Some(result));
                    if let Ok(value) = result {
                        let mut event = Observation::new(Kind::RootResult).key(key);
                        event.value = value;
                        transfer_trace::record(event);
                    }
                    result
                })
            }))
        })
    });
    drop(event_guard);
    assert_worker_clean(&db);
    let installed = schedule
        .installed_wait
        .as_ref()
        .expect("installed-wait row");
    eprintln!(
        "INSTALLED_WAIT {:?} worker {worker} proof {:?}, cancelled proof {:?}",
        installed.action,
        installed.proof.get(),
        installed.cancelled_proof.get()
    );
    for record in &trace.records {
        eprintln!("INSTALLED_WAIT {:?} {record:?}", installed.action);
    }
    validation_trace::emit(
        &format!("installed.{:?}.{worker}", installed.action),
        &validation,
    );
    let support = support
        .into_inner()
        .expect("the participant installed its real Session");
    assert!(!support.owns_current_session(db.zalsa()));
    let exit = exit
        .get()
        .expect("the participant saved its allowance before Session removal");
    eprintln!(
        "INSTALLED_WAIT {:?} worker {worker} exit {exit:?}, support {:?}, native failure {}",
        installed.action,
        transfer_trace::support_snapshot(&support),
        outcome.is_err()
    );
    reports
        .send(InstalledReport {
            worker,
            thread: thread::current().id(),
            support,
            exit,
            token_after_cleanup: token.is_cancelled(),
            driver: driver.get(),
            proof: installed.proof.get(),
            cancelled_proof: installed.cancelled_proof.get(),
            validation,
            trace,
        })
        .expect("the coordinator retains worker reports");
    match outcome {
        Ok(outcome) => outcome,
        Err(payload) => resume_unwind(payload),
    }
}

fn one_observation(records: &[Record], kind: Kind) -> &Record {
    let mut matching = records.iter().filter(|record| record.event.kind == kind);
    let record = matching.next().expect("the native transition was observed");
    assert!(matching.next().is_none(), "duplicate {kind:?}: {records:?}");
    record
}

fn phase_observation<'a>(records: &'a [Record], phase: &str) -> &'a Record {
    let mut matching = records
        .iter()
        .filter(|record| record.event.phase == Some(phase));
    let record = matching.next().expect("the scheduled phase was observed");
    assert!(matching.next().is_none(), "duplicate phase {phase}");
    record
}

fn assert_installed_graph_clean(graph: GraphSnapshot) {
    eprintln!("INSTALLED_WAIT post-join graph {graph:?}");
    assert!(!graph.edges.overflow && graph.edges.is_empty());
    assert!(!graph.pending.overflow && graph.pending.is_empty());
    assert!(!graph.transferred.overflow && graph.transferred.is_empty());
    assert!(!graph.dependents.overflow && !graph.reverse.overflow);
    for (_, dependents) in graph.dependents.entries.into_iter().flatten() {
        assert!(!dependents.overflow && dependents.is_empty());
    }
    for (_, owners) in graph.reverse.entries.into_iter().flatten() {
        assert!(!owners.overflow && owners.is_empty());
    }
}

fn assert_installed_history(
    action: WaitAction,
    key: DatabaseKeyIndex,
    reports: &[InstalledReport; 2],
) {
    let owner = &reports[0];
    let waiter = &reports[1];
    assert_eq!([owner.worker, waiter.worker], [0, 1]);
    assert_ne!(owner.thread, waiter.thread);
    assert!(!owner.support.same_owner(&waiter.support));
    let proof = owner.proof.expect("the owner proved the installed edge");
    proof.assert_installed(key);
    assert_eq!((proof.owner, proof.waiter), (owner.thread, waiter.thread));
    assert!(waiter.proof.is_none() && waiter.cancelled_proof.is_none());
    let edge = one_observation(&waiter.trace.records, Kind::Edge);
    let consumed = one_observation(&waiter.trace.records, Kind::WaitConsumed);
    let unblock = one_observation(&owner.trace.records, Kind::Unblock);
    let installed = phase_observation(&owner.trace.records, "installed");
    let owner_claim = one_observation(&owner.trace.records, Kind::Claim);
    let owner_terminal = one_observation(&owner.trace.records, Kind::Terminal);
    assert_eq!(owner_claim.event.key, Some(key));
    assert_eq!(owner_terminal.event.key, Some(key));
    assert_eq!(owner_claim.event.serial, owner_terminal.event.serial);
    assert!(owner_claim.event.serial.is_some());
    assert!(owner_claim.ordinal < edge.ordinal && edge.ordinal < installed.ordinal);
    assert!(installed.ordinal < owner_terminal.ordinal);
    assert!(owner_terminal.ordinal < unblock.ordinal && unblock.ordinal < consumed.ordinal);
    assert_eq!(edge.event.key, Some(key));
    assert_eq!(consumed.event.key, Some(key));
    for record in [edge, unblock, consumed] {
        assert_eq!(record.event.from, Some(waiter.thread));
        assert_eq!(record.event.peer, Some(owner.thread));
    }
    for record in [owner_terminal, unblock, consumed] {
        assert!(
            match action {
                WaitAction::CancelOwner => matches!(record.event.wait, Some(WaitResult::Cancelled)),
                _ => matches!(record.event.wait, Some(WaitResult::Completed)),
            },
            "unexpected native wait result: {record:?}"
        );
    }
    assert_eq!(
        owner_terminal.event.action,
        Some(if action == WaitAction::CancelOwner {
            Action::Abort
        } else {
            Action::Drop
        })
    );

    let producer = if action == WaitAction::CancelOwner {
        waiter
    } else {
        owner
    };
    let body = one_observation(&producer.trace.records, Kind::BodyValue);
    let publication = one_observation(&producer.trace.records, Kind::RootPublished);
    assert_eq!(body.event.key, Some(key));
    assert_eq!(body.event.value, 4);
    assert_eq!(publication.event.key, Some(key));
    assert!(
        publication
            .event
            .memo
            .is_some_and(|memo| memo.has_value && memo.final_)
    );
    assert!(body.ordinal < publication.ordinal);
    assert!(installed.ordinal < body.ordinal);
    let release = phase_observation(
        &owner.trace.records,
        match action {
            WaitAction::Complete => "release.normal",
            WaitAction::CancelOwner => "cancel.owner",
            WaitAction::CancelWaiter => "cancel.waiter",
        },
    );
    assert!(installed.ordinal < release.ordinal && release.ordinal < owner_terminal.ordinal);

    for report in reports {
        assert!(!report.trace.broken);
        let entry = phase_observation(&report.trace.records, "participant.enter")
            .event
            .session
            .unwrap();
        let support = transfer_trace::support_snapshot(&report.support);
        let cancelled = match action {
            WaitAction::Complete => false,
            WaitAction::CancelOwner => report.worker == 0,
            WaitAction::CancelWaiter => report.worker == 1,
        };
        // The outer registration attachment resets its local token after native claim cleanup.
        assert!(!report.token_after_cleanup);
        assert_eq!(support.state, if cancelled { 3 } else { 2 });
        assert_eq!(
            (
                support.owner,
                support.database,
                support.revision,
                support.cancellation
            ),
            (
                entry.support.owner,
                entry.support.database,
                entry.support.revision,
                entry.support.cancellation
            )
        );
        assert_eq!(report.support.reason(), None);
        assert!(!support.explicitly_incomplete);
        assert_eq!(entry.remaining, WAIT_ALLOWANCE);
        let mut exit_support = entry.support;
        if cancelled {
            exit_support.state = 4;
            exit_support.explicitly_incomplete = true;
        }
        assert_eq!(report.exit.support, exit_support);
        assert_eq!(report.exit.scope, entry.scope);
        assert_eq!(entry.support.state, 0);
        assert_eq!(entry.scope, None);
        for (kind, expected_count) in [
            (Kind::Edge, usize::from(report.worker == 1)),
            (Kind::WaitConsumed, usize::from(report.worker == 1)),
            (Kind::Unblock, usize::from(report.worker == 0)),
        ] {
            assert_eq!(
                report
                    .trace
                    .records
                    .iter()
                    .filter(|record| record.event.kind == kind)
                    .count(),
                expected_count,
                "unexpected {kind:?} count for worker {}",
                report.worker
            );
        }
        let spent = if report.worker == 0 {
            INPUT_CONVERSION_WORK + usize::from(!cancelled) * (1 + ROOT_READ_WORK)
        } else if action == WaitAction::CancelOwner {
            1 + INPUT_CONVERSION_WORK + 1 + ROOT_READ_WORK
        } else {
            usize::from(!cancelled) * (1 + ROOT_READ_WORK)
        };
        let expected = WAIT_ALLOWANCE - spent;
        assert_eq!(report.exit.remaining, expected);
        let cancellation_checks = report
            .trace
            .records
            .iter()
            .filter(|record| {
                record.event.phase == Some("will.check.cancellation") && record.event.decision
            })
            .collect::<Vec<_>>();
        let mut previous = WAIT_ALLOWANCE;
        for record in &report.trace.records {
            assert_eq!(record.worker, report.worker);
            assert_eq!(record.thread, report.thread);
            if let Some(session) = record.event.session {
                // The throwing check still observes active support. Retained owners abort
                // only after the driver marks the same session Interrupted.
                let expected_support = if cancelled
                    && cancellation_checks
                        .last()
                        .is_some_and(|check| record.ordinal > check.ordinal)
                {
                    exit_support
                } else {
                    entry.support
                };
                assert_eq!(session.support, expected_support);
                assert!(session.remaining <= previous && session.remaining >= expected);
                if report.worker == 1 && record.ordinal <= consumed.ordinal {
                    assert_eq!(session.remaining, WAIT_ALLOWANCE);
                }
                if report.worker == 0
                    && record.ordinal >= installed.ordinal
                    && record.ordinal <= release.ordinal
                {
                    assert_eq!(session.remaining, WAIT_ALLOWANCE - INPUT_CONVERSION_WORK);
                }
                previous = session.remaining;
            }
            assert!(!matches!(
                record.event.kind,
                Kind::TransferBegin
                    | Kind::TransferEnd
                    | Kind::Restore
                    | Kind::Mapping
                    | Kind::MappingRemoved
                    | Kind::Undo
                    | Kind::TransferWaitBegin
                    | Kind::TransferWaitEnd
                    | Kind::EdgeRemap
            ));
            if let Some(mode) = record.event.mode {
                assert_eq!(mode, Mode::Default);
            }
            if record.event.kind == Kind::Terminal {
                assert_eq!(record.event.key, Some(key));
                assert_eq!(
                    report
                        .trace
                        .records
                        .iter()
                        .filter(|claim| claim.event.kind == Kind::Claim
                            && claim.event.serial == record.event.serial)
                        .count(),
                    1
                );
            }
        }
        assert_eq!(previous, expected);
        if cancelled {
            assert!(report.driver.is_none());
            assert!(!cancellation_checks.is_empty());
            let trigger = phase_observation(
                &owner.trace.records,
                if report.worker == 0 {
                    "cancel.owner"
                } else {
                    "cancel.waiter"
                },
            );
            assert!(
                cancellation_checks
                    .iter()
                    .all(|check| check.ordinal > trigger.ordinal)
            );
            assert!(report.trace.records.iter().all(|record| !matches!(
                record.event.kind,
                Kind::BodyValue | Kind::RootPublished | Kind::RootResult
            )));
            if report.worker == 1 {
                assert!(cancellation_checks[0].ordinal > consumed.ordinal);
            } else {
                assert!(cancellation_checks[0].ordinal < owner_terminal.ordinal);
            }
        } else {
            assert_eq!(report.driver, Some(Ok(4)));
            assert!(cancellation_checks.is_empty());
            let root = one_observation(&report.trace.records, Kind::RootResult);
            assert_eq!(root.event.key, Some(key));
            assert_eq!(root.event.value, 4);
        }
    }
    let owner_support = transfer_trace::support_snapshot(&owner.support);
    let waiter_support = transfer_trace::support_snapshot(&waiter.support);
    assert_eq!(owner_support.database, waiter_support.database);
    assert_eq!(owner_support.revision, waiter_support.revision);
    assert_eq!(owner_support.cancellation, waiter_support.cancellation);

    let (wait_index, wait_operation) = waiter
        .validation
        .iter()
        .enumerate()
        .find_map(|(index, event)| match event {
            TraceEvent::Outer {
                phase: "fetch.wait",
                key: Some(actual),
                operation,
                ..
            } if *actual == key => Some((index, *operation)),
            _ => None,
        })
        .expect("the retained registered request reached the real wait arm");
    if action == WaitAction::CancelWaiter {
        let retained = owner
            .cancelled_proof
            .expect("cancelled waiter remains installed");
        retained.assert_installed(key);
        assert_eq!(
            (retained.owner, retained.waiter),
            (proof.owner, proof.waiter)
        );
        let retained_event = phase_observation(&owner.trace.records, "cancel.waiter.retained");
        assert!(release.ordinal < retained_event.ordinal && retained_event.ordinal < body.ordinal);
        assert!(
            waiter.validation[wait_index + 1..]
                .iter()
                .all(|event| !matches!(
                    event,
                    TraceEvent::Outer {
                        phase: "fetch.probe" | "fetch.selected",
                        ..
                    }
                ))
        );
        assert!(
            waiter
                .trace
                .records
                .iter()
                .all(|record| !matches!(record.event.kind, Kind::Claim | Kind::Terminal))
        );
    } else {
        assert!(owner.cancelled_proof.is_none());
        let retry = waiter
            .trace
            .records
            .iter()
            .find(|record| {
                record.ordinal > consumed.ordinal
                    && record.event.kind == Kind::Admission
                    && record.event.phase == Some("observed.work")
                    && record.event.units == 1
                    && record
                        .event
                        .session
                        .is_some_and(|session| session.remaining == WAIT_ALLOWANCE - 1)
            })
            .expect("the original waiting Session paid its positive retry");
        assert_eq!(
            retry.event.session.unwrap().scope,
            consumed.event.session.unwrap().scope
        );
        let probe = waiter.validation[wait_index + 1..]
            .iter()
            .position(|event| {
                matches!(event,
                    TraceEvent::Outer { phase: "fetch.probe", key: Some(actual), operation, .. }
                        if *actual == key && *operation == wait_operation
                )
            })
            .map(|index| index + wait_index + 1)
            .expect("the original request re-probed after waking");
        assert!(
            waiter.validation[wait_index + 1..probe]
                .iter()
                .any(|event| matches!(
                    event,
                    TraceEvent::Admission {
                        work: ExecutionWork::Work { units: 1 },
                        outcome: Some(Ok(())),
                        ..
                    }
                ))
        );
        if action == WaitAction::CancelOwner {
            let claim = one_observation(&waiter.trace.records, Kind::Claim);
            let terminal = one_observation(&waiter.trace.records, Kind::Terminal);
            assert_eq!(claim.event.key, Some(key));
            assert!(retry.ordinal < claim.ordinal && claim.ordinal < body.ordinal);
            assert_eq!(claim.event.serial, terminal.event.serial);
            assert_ne!(claim.event.serial, owner_claim.event.serial);
            assert_eq!(terminal.event.action, Some(Action::Drop));
            assert!(matches!(terminal.event.wait, Some(WaitResult::Completed)));
        } else {
            assert!(waiter.trace.records.iter().all(|record| !matches!(
                record.event.kind,
                Kind::Claim | Kind::Terminal | Kind::BodyValue | Kind::RootPublished
            )));
        }
    }
}

fn installed_value_memo(db: &Db, input: Input) -> MemoSnapshot {
    let ingredient = value::fn_ingredient_(db, db.zalsa());
    let memo = ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            input.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
        )
        .expect("the surviving producer published its canonical result");
    assert_eq!(memo.value(), Some(&4));
    assert!(!memo.header.may_be_provisional());
    assert_eq!(
        memo.header.attempt_reuse(db.zalsa()),
        attempt_probe::MemoReuse::Ordinary
    );
    let snapshot = memo.transfer_test_snapshot();
    assert!(snapshot.has_value && snapshot.final_);
    assert!(!snapshot.heads.overflow && snapshot.heads.is_empty());
    assert_eq!(snapshot.verified_at, db.zalsa().current_revision());
    assert!(snapshot.changed_at <= snapshot.verified_at);
    assert_eq!(snapshot.durability, Durability::LOW);
    snapshot
}

#[derive(Debug, Eq, PartialEq)]
struct WaitCanonical {
    dependencies: Vec<String>,
    verified_at: crate::Revision,
    changed_at: crate::Revision,
    durability: Durability,
}

fn assert_local_cancelled(outcome: thread::Result<PairedQueryOutcome>) {
    let payload = outcome.expect_err("the targeted worker retained its native cancellation");
    let cancelled = payload.downcast_ref::<Cancelled>();
    assert!(
        matches!(cancelled, Some(Cancelled::Local)),
        "expected native local cancellation, observed {cancelled:?}"
    );
}

fn installed_wait_case(action: WaitAction) -> WaitCanonical {
    eprintln!("INSTALLED_WAIT {action:?}: begin");
    let db = Db::default();
    let input = Input::new(&db, 4);
    let value_ingredient = value::fn_ingredient_(&db, db.zalsa());
    let consumer_ingredient = consumer::fn_ingredient_(&db, db.zalsa());
    let key = value_ingredient.database_key_index(input.as_id());
    let revision = db.zalsa().current_revision();
    let cancellation = db.zalsa().runtime().cancellation_count();
    let waits = Arc::new(AtomicUsize::new(0));
    let ordinal = Arc::new(AtomicUsize::new(0));
    let (start_tx, start_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (metadata_tx, metadata_rx) = mpsc::channel();
    let (reports_tx, reports_rx) = mpsc::channel();
    let owner = InstalledWorker {
        schedule: Schedule {
            owner: true,
            case: Case::Cold,
            outbound: start_tx,
            inbound: RefCell::new(Some(release_rx)),
            body_entered: Cell::new(false),
            identity: Arc::new(()),
            installed_wait: Some(InstalledWait::new(action, Some(metadata_rx))),
        },
        metadata: None,
        observed_waits: waits.clone(),
        config: TraceConfig {
            worker: 0,
            ordinal: ordinal.clone(),
        },
        reports: reports_tx.clone(),
    };
    let waiter = InstalledWorker {
        schedule: Schedule {
            owner: false,
            case: Case::Cold,
            outbound: release_tx,
            inbound: RefCell::new(Some(start_rx)),
            body_entered: Cell::new(false),
            identity: Arc::new(()),
            installed_wait: Some(InstalledWait::new(action, None)),
        },
        metadata: Some(metadata_tx),
        observed_waits: waits.clone(),
        config: TraceConfig { worker: 1, ordinal },
        reports: reports_tx,
    };
    let (owner_result, waiter_result) = run_pair(
        &db,
        move |db, participant| installed_wait_worker(db, participant, input, owner),
        move |db, participant| installed_wait_worker(db, participant, input, waiter),
    )
    .expect("the coordinator admitted exactly two real Sessions");
    let mut reports = [
        reports_rx
            .recv_timeout(STAGE_TIMEOUT)
            .expect("owner or waiter report"),
        reports_rx
            .recv_timeout(STAGE_TIMEOUT)
            .expect("other worker report"),
    ];
    reports.sort_by_key(|report| report.worker);

    // Inspect untouched native state before test claims or hot queries can hide a leak.
    assert_installed_graph_clean(db.zalsa().runtime().test_transfer_graph_snapshot());
    assert!(
        value_ingredient
            .sync_table
            .test_transfer_state(input.as_id())
            .is_none()
    );
    assert!(
        consumer_ingredient
            .sync_table
            .test_transfer_state(input.as_id())
            .is_none()
    );
    assert_exclusion_released(&db);
    assert_eq!(waits.load(Ordering::SeqCst), 1);
    assert_eq!(db.zalsa().current_revision(), revision);
    assert_eq!(db.zalsa().runtime().cancellation_count(), cancellation);
    assert!(!db.cancellation_token().is_cancelled());
    assert_installed_history(action, key, &reports);
    match action {
        WaitAction::Complete => {
            assert_eq!(owner_result.unwrap(), Ok(AttemptOutcome::Complete(Ok(4))));
            assert_eq!(waiter_result.unwrap(), Ok(AttemptOutcome::Complete(Ok(4))));
        }
        WaitAction::CancelOwner => {
            assert_local_cancelled(owner_result);
            assert_eq!(waiter_result.unwrap(), Ok(AttemptOutcome::Complete(Ok(4))));
        }
        WaitAction::CancelWaiter => {
            assert_eq!(owner_result.unwrap(), Ok(AttemptOutcome::Complete(Ok(4))));
            assert_local_cancelled(waiter_result);
        }
    }
    assert_eq!(db.counts.snapshot(), [1, 0]);
    let memo = installed_value_memo(&db, input);
    if let Some(support) = memo.support {
        let producer = usize::from(action == WaitAction::CancelOwner);
        assert_eq!(
            support,
            transfer_trace::support_snapshot(&reports[producer].support)
        );
        assert_eq!(support.state, 2);
        assert!(!support.explicitly_incomplete);
    }
    assert!(
        consumer_ingredient
            .get_memo_from_table_for(
                db.zalsa(),
                input.as_id(),
                consumer_ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
            )
            .is_none()
    );
    let actual_dependencies = dependencies(&db, value_ingredient, input);
    assert_eq!(actual_dependencies.len(), 1);
    let baseline = Db::default();
    let baseline_input = Input::new(&baseline, 4);
    assert_eq!(value(&baseline, baseline_input), 4);
    let baseline_memo = installed_value_memo(&baseline, baseline_input);
    assert_eq!(
        actual_dependencies,
        dependencies(
            &baseline,
            value::fn_ingredient_(&baseline, baseline.zalsa()),
            baseline_input,
        )
    );
    assert_eq!(
        (memo.verified_at, memo.changed_at, memo.durability),
        (
            baseline_memo.verified_at,
            baseline_memo.changed_at,
            baseline_memo.durability
        )
    );
    for _ in 0..2 {
        assert_eq!(value(&db, input), value(&baseline, baseline_input));
        assert_eq!(db.counts.snapshot(), [1, 0]);
        assert_eq!(baseline.counts.snapshot(), [1, 0]);
        assert_eq!(installed_value_memo(&db, input).identity, memo.identity);
        assert_eq!(
            installed_value_memo(&baseline, baseline_input).identity,
            baseline_memo.identity
        );
    }
    assert_claim_released(&db, value_ingredient, input);
    assert_claim_released(&db, consumer_ingredient, input);
    assert_exclusion_released(&db);
    eprintln!(
        "INSTALLED_WAIT {action:?}: native outcomes, allowances, cleanup and hot reuse checked"
    );
    WaitCanonical {
        dependencies: actual_dependencies,
        verified_at: memo.verified_at,
        changed_at: memo.changed_at,
        durability: memo.durability,
    }
}

#[test]
fn installed_wait_native_cancellation() {
    if std::env::var(CANCELLATION_CHILD_MARKER).as_deref() == Ok(CANCELLATION_TEST_NAME) {
        let normal = installed_wait_case(WaitAction::Complete);
        assert_eq!(installed_wait_case(WaitAction::CancelOwner), normal);
        assert_eq!(installed_wait_case(WaitAction::CancelWaiter), normal);
        return;
    }
    watchdog(CANCELLATION_TEST_NAME, CANCELLATION_CHILD_MARKER);
}

#[test]
fn paired_registration_and_canonical_waits() {
    if std::env::var(CHILD_MARKER).as_deref() == Ok(TEST_NAME) {
        registration_lifecycle();
        query_case(Case::Cold);
        query_case(Case::OwnerPanic);
        query_case(Case::Validation);
        query_case(Case::WaitEventPanic);
        return;
    }

    watchdog(TEST_NAME, CHILD_MARKER);
}

pub(super) fn watchdog(test_name: &str, child_marker: &str) {
    // A broken canonical wait cannot be safely timed out by destroying live Rust workers.
    // Bound the entire saved test process instead, retaining its trace on inherited output.
    let mut child = Command::new(std::env::current_exe().expect("current test binary"))
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .env(child_marker, test_name)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("start isolated paired-wait control");
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().expect("inspect paired-wait child") {
            assert!(status.success(), "paired-wait child failed: {status}");
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let status = child.wait().expect("reap timed-out paired-wait child");
            panic!(
                "paired-wait child exceeded {PROCESS_TIMEOUT:?}; last trace is above ({status})"
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}
mod independent;
mod mixed;
mod native_cancellation;
mod transfer;

mod fresh_cycle_return;
