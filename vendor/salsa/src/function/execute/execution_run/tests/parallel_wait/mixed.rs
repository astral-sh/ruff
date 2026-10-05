use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any, resume_unwind};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::super::super::registration::{
    ExecutableRouteProvider, ProviderContext, RegistryBuilder, Route,
};
use super::super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use super::super::validation_trace::{self, TraceEvent};
use crate::attempt_probe::paired_test_support::{PairInstallError, Participant, run_pair};
use crate::attempt_probe::transfer_test_support::{
    self as trace, Action, Event as Observation, GraphSnapshot, Kind, Mode, Record, Step,
    TraceConfig, TransferTrace,
};
use crate::attempt_probe::{
    self, AttemptOutcome, AttemptSupport, Incomplete, MemoReuse, StartError,
};
use crate::function::{Configuration, IngredientImpl, SyncOwner};
use crate::plumbing::AsId;
use crate::runtime::WaitResult;
use crate::zalsa::ZalsaDatabase;
use crate::zalsa_local::QueryEdgeKind;
use crate::{Cycle, Database, DatabaseKeyIndex, Durability, Event, EventKind, Id, Setter};

const ALLOWANCE: usize = 10_000;
const STAGE_TIMEOUT: Duration = Duration::from_secs(5);
const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const CHILD_MARKER: &str = "SALSA_MIXED_OWNERSHIP_CHILD";
const TEST_NAME: &str = "function::execute::execution_run::tests::parallel_wait::mixed::mixed_ordinary_and_controlled_ownership";

type Supports = Arc<[OnceLock<AttemptSupport>; 2]>;
type PairedOutcome = Result<AttemptOutcome<RunResult<u32>>, PairInstallError>;

#[derive(Default)]
struct Counts([AtomicUsize; 6], [AtomicUsize; 3], bool);
impl Counts {
    fn snapshot(&self) -> [usize; 6] {
        self.0.each_ref().map(|count| count.load(Ordering::SeqCst))
    }
    fn increment(&self, query: usize, step: Step) {
        let offset = match step {
            Step::Body => 0,
            Step::Initial => 1,
            Step::Recovery => 2,
        };
        self.0[query * 3 + offset].fetch_add(1, Ordering::SeqCst);
    }
}

#[crate::db]
trait TransferDatabase: Database {
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
impl TransferDatabase for Db {
    fn counts(&self) -> &Counts {
        &self.counts
    }
}
impl Default for Db {
    fn default() -> Self {
        Self {
            storage: crate::Storage::new(Some(Box::new(wait_event))),
            counts: Arc::default(),
        }
    }
}
impl Db {
    fn with_recovery(uses_last: bool) -> Self {
        Self {
            counts: Arc::new(Counts(Default::default(), Default::default(), uses_last)),
            ..Self::default()
        }
    }
}

#[crate::input]
struct Input {
    #[returns(copy)]
    limit: u32,
    #[returns(copy)]
    seed_marker: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::SalsaValue)]
struct AValue(u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::SalsaValue)]
struct BValue(u32);

trait ValueKind: Copy {
    const QUERY: usize;
    fn from_number(value: u32) -> Self;
}
impl ValueKind for AValue {
    const QUERY: usize = 0;
    fn from_number(value: u32) -> Self {
        Self(value)
    }
}
impl ValueKind for BValue {
    const QUERY: usize = 1;
    fn from_number(value: u32) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug)]
struct Keys([DatabaseKeyIndex; 2]);
impl Keys {
    fn new(db: &dyn TransferDatabase, input: Input) -> Self {
        Self([
            a::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
            b::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
        ])
    }
}

fn a_value(b: BValue) -> AValue {
    AValue(b.0)
}
fn b_value(db: &dyn TransferDatabase, input: Input, a: AValue) -> BValue {
    if a.0 == 0 {
        let marker = input.seed_marker(db);
        let mut event = Observation::new(Kind::MarkerRead).key(Keys::new(db, input).0[1]);
        event.value = marker;
        trace::record(event);
    }
    BValue(a.0.saturating_add(1).min(input.limit(db)))
}

fn semantic_step(db: &dyn TransferDatabase, query: usize, step: Step) {
    db.counts().increment(query, step);
}
fn value_observation(
    db: &dyn TransferDatabase,
    input: Input,
    query: usize,
    step: Step,
    value: u32,
) {
    let kind = match step {
        Step::Body => Kind::BodyValue,
        Step::Initial => Kind::InitialValue,
        Step::Recovery => Kind::RecoveryValue,
    };
    let mut event = Observation::new(kind).key(Keys::new(db, input).0[query]);
    event.value = value;
    event.step = Some(step);
    trace::record(event);
}
fn child_observation(keys: Keys, query: usize) {
    let mut event = Observation::new(Kind::ChildRequest).key(keys.0[query]);
    event.other_key = Some(keys.0[1 - query]);
    trace::record(event);
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = initial_a, cycle_fn = recover_a)]
fn a(db: &dyn TransferDatabase, input: Input) -> AValue {
    semantic_step(db, 0, Step::Body);
    native_gate(db, input, 0);
    child_observation(Keys::new(db, input), 0);
    let value = a_value(b(db, input));
    value_observation(db, input, 0, Step::Body, value.0);
    value
}
#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = initial_b, cycle_fn = recover_b)]
fn b(db: &dyn TransferDatabase, input: Input) -> BValue {
    semantic_step(db, 1, Step::Body);
    native_gate(db, input, 1);
    child_observation(Keys::new(db, input), 1);
    let value = b_value(db, input, a(db, input));
    value_observation(db, input, 1, Step::Body, value.0);
    value
}
fn initial_a(db: &dyn TransferDatabase, _id: Id, input: Input) -> AValue {
    semantic_step(db, 0, Step::Initial);
    value_observation(db, input, 0, Step::Initial, 0);
    AValue(0)
}
fn initial_b(db: &dyn TransferDatabase, _id: Id, input: Input) -> BValue {
    semantic_step(db, 1, Step::Initial);
    value_observation(db, input, 1, Step::Initial, 0);
    BValue(0)
}
fn recover_a(
    db: &dyn TransferDatabase,
    _cycle: &Cycle<'_>,
    last: &AValue,
    value: AValue,
    input: Input,
) -> AValue {
    semantic_step(db, 0, Step::Recovery);
    AValue(recovered(db, input, 0, last.0, value.0))
}
fn recover_b(
    db: &dyn TransferDatabase,
    _cycle: &Cycle<'_>,
    last: &BValue,
    value: BValue,
    input: Input,
) -> BValue {
    semantic_step(db, 1, Step::Recovery);
    BValue(recovered(db, input, 1, last.0, value.0))
}

fn recovered(db: &dyn TransferDatabase, input: Input, query: usize, last: u32, new: u32) -> u32 {
    // This fixture setting is immutable for the database's entire lifetime. Both
    // native callbacks and registered providers use this same recovery function.
    let value = if db.counts().2 {
        new.max(last.saturating_add(1).min(input.limit(db)))
    } else {
        new
    };
    let mut event = Observation::new(Kind::RecoveryValue).key(Keys::new(db, input).0[query]);
    event.step = Some(Step::Recovery);
    event.phase = Some("last/new/recovered");
    event.units = last as usize;
    event.identity = new as usize;
    event.value = value;
    trace::record(event);
    value
}

#[derive(Clone, Copy, Debug)]
enum Gate {
    Ready,
    Abort,
}
#[derive(Debug)]
struct HarnessFailure;
struct ScheduleData {
    worker: usize,
    send_a: mpsc::Sender<Gate>,
    receive_a: Option<mpsc::Receiver<Gate>>,
    send_b: mpsc::Sender<Gate>,
    receive_b: Option<mpsc::Receiver<Gate>>,
    send_wait: mpsc::SyncSender<Gate>,
    receive_wait: Option<mpsc::Receiver<Gate>>,
}
fn schedules() -> [ScheduleData; 2] {
    let (a_tx, a_rx) = mpsc::channel();
    let (b_tx, b_rx) = mpsc::channel();
    let (wait_tx, wait_rx) = mpsc::sync_channel(1);
    [
        ScheduleData {
            worker: 0,
            send_a: a_tx.clone(),
            receive_a: None,
            send_b: b_tx.clone(),
            receive_b: Some(b_rx),
            send_wait: wait_tx.clone(),
            receive_wait: None,
        },
        ScheduleData {
            worker: 1,
            send_a: a_tx,
            receive_a: Some(a_rx),
            send_b: b_tx,
            receive_b: None,
            send_wait: wait_tx,
            receive_wait: Some(wait_rx),
        },
    ]
}
struct Schedule {
    data: ScheduleData,
    entered: Cell<bool>,
    wait_sent: Cell<bool>,
    keys: Keys,
    supports: Supports,
}
impl Schedule {
    fn receive(receiver: &Option<mpsc::Receiver<Gate>>) -> RunResult<()> {
        match receiver
            .as_ref()
            .expect("worker has its one-shot gate")
            .recv_timeout(STAGE_TIMEOUT)
        {
            Ok(Gate::Ready) => Ok(()),
            _ => Err(RunError::Refused(Incomplete::Interrupted)),
        }
    }
    fn body(&self, db: &dyn TransferDatabase, query: usize) -> RunResult<()> {
        if query != self.data.worker || self.entered.replace(true) {
            return Ok(());
        }
        trace::record(Observation::new(Kind::Gate).key(self.keys.0[query]));
        if query == 0 {
            let _ = self.data.send_a.send(Gate::Ready);
            Self::receive(&self.data.receive_b)?;
        } else {
            Self::receive(&self.data.receive_a)?;
            let _ = self.data.send_b.send(Gate::Ready);
            Self::receive(&self.data.receive_wait)?;
        }
        self.authority(db);
        Ok(())
    }
    fn authority(&self, db: &dyn TransferDatabase) {
        let Some(own) = attempt_probe::current() else {
            assert_eq!(attempt_probe::remaining_allowance_for_diagnostics(db), None);
            assert!(!attempt_probe::is_incomplete(db));
            if let Some(peer) = self.supports[1 - self.data.worker].get() {
                assert!(peer.local_ownership(db.zalsa()).is_none());
                assert!(!peer.owns_current_session(db.zalsa()));
            }
            return;
        };
        let original = self.supports[self.data.worker]
            .get()
            .expect("installed worker support");
        assert!(own.same_owner(original));
        assert!(original.owns_current_session(db.zalsa()));
        assert!(original.is_current(db.zalsa()));
        assert!(original.local_ownership(db.zalsa()).is_some());
        let mut event = Observation::new(Kind::Authority).decision(true);
        event.support = Some(trace::support_snapshot(original));
        assert!(self.supports[1 - self.data.worker].get().is_none());
        trace::record(event);
    }
}
thread_local! { static SCHEDULE: RefCell<Option<Rc<Schedule>>> = const { RefCell::new(None) }; }
struct ScheduleOwner(Rc<Schedule>);
impl ScheduleOwner {
    fn install(data: ScheduleData, keys: Keys, supports: Supports) -> Self {
        let schedule = Rc::new(Schedule {
            data,
            keys,
            supports,
            entered: Cell::new(false),
            wait_sent: Cell::new(false),
        });
        SCHEDULE.with_borrow_mut(|slot| assert!(slot.replace(schedule.clone()).is_none()));
        Self(schedule)
    }
}
impl Drop for ScheduleOwner {
    fn drop(&mut self) {
        SCHEDULE.with_borrow_mut(|slot| *slot = None);
        if self.0.data.worker == 0 {
            let _ = self.0.data.send_a.send(Gate::Abort);
            let _ = self.0.data.send_wait.try_send(Gate::Abort);
        } else {
            let _ = self.0.data.send_b.send(Gate::Abort);
        }
    }
}
fn native_gate(db: &dyn TransferDatabase, _input: Input, query: usize) {
    let schedule = SCHEDULE.with_borrow(Clone::clone);
    if let Some(schedule) = schedule
        && schedule.body(db, query).is_err()
    {
        panic_any(HarnessFailure);
    }
}
fn wait_event(event: Event) {
    let EventKind::WillBlockOn { database_key, .. } = event.kind else {
        return;
    };
    SCHEDULE.with_borrow(|slot| {
        if let Some(schedule) = slot
            && schedule.data.worker == 0
            && database_key == schedule.keys.0[1]
            && !schedule.wait_sent.replace(true)
        {
            // Running retains both synchronization guards until this notification returns.
            let _ = schedule.data.send_wait.try_send(Gate::Ready);
        }
    });
    ACYCLIC_SCHEDULE.with_borrow(|slot| {
        if let Some(schedule) = slot
            && !schedule.owner
            && database_key == schedule.parent
            && !schedule.wait_sent.replace(true)
        {
            let _ = schedule.release.try_send(Gate::Ready);
        }
    });
}

struct Admission;
impl ExecutionAdmission for Admission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let mut event = Observation::new(Kind::Admission);
        match work {
            ExecutionWork::Task { requested_bytes } => {
                event.phase = Some("task");
                event.units = requested_bytes;
            }
            ExecutionWork::Resource { requested_bytes } => {
                event.phase = Some("resource");
                event.units = requested_bytes;
            }
            ExecutionWork::Work { units } => {
                event.phase = Some("work");
                event.units = units;
            }
            ExecutionWork::Poll => event.phase = Some("poll"),
        }
        trace::record(event);
        Ok(())
    }
}
static ADMISSION: Admission = Admission;

struct Providers<'run, 'db, A: Configuration, B: Configuration> {
    a: Route<'db, A>,
    b: Route<'db, B>,
    schedule: &'run Schedule,
}
impl<'run, 'db: 'run, A, B, C> ExecutableRouteProvider<'run, 'db, C> for Providers<'run, 'db, A, B>
where
    A: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = AValue>,
    B: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = BValue>,
    C: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input>,
    C::Output<'db>: ValueKind,
{
    // Conversion reconstructs one Input handle; AValue and BValue compare their u32 field.
    fixture_native_value!(executable, 'run, 'db, C, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn TransferDatabase,
        input: Input,
    ) -> RunResult<C::Output<'db>> {
        let query = <C::Output<'db> as ValueKind>::QUERY;
        context
            .endpoint()
            .local_call(|| {
                self.schedule.authority(db);
                debit(
                    db,
                    context.endpoint(),
                    self.schedule.keys.0[query],
                    Step::Body,
                )?;
                semantic_step(db, query, Step::Body);
                self.schedule.body(db, query)
            })
            .await;
        child_observation(self.schedule.keys, query);
        let value = if query == 0 {
            a_value(*context.fetch_ref(&self.b, input.as_id())?.await?).0
        } else {
            b_value(
                db,
                input,
                *context.fetch_ref(&self.a, input.as_id())?.await?,
            )
            .0
        };
        value_observation(db, input, query, Step::Body, value);
        Ok(<C::Output<'db> as ValueKind>::from_number(value))
    }
    async fn initial(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn TransferDatabase,
        id: Id,
        input: Input,
    ) -> RunResult<C::Output<'db>> {
        context
            .endpoint()
            .local_call(|| {
                self.schedule.authority(db);
                debit(
                    db,
                    context.endpoint(),
                    self.schedule.keys.0[<C::Output<'db> as ValueKind>::QUERY],
                    Step::Initial,
                )
            })
            .await;
        Ok(C::cycle_initial(db, id, input))
    }
    async fn recover<'call>(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn TransferDatabase,
        cycle: &'call Cycle<'call>,
        last: &'call C::Output<'db>,
        value: C::Output<'db>,
        input: Input,
    ) -> RunResult<C::Output<'db>>
    where
        'run: 'call,
    {
        context
            .endpoint()
            .local_call(|| {
                self.schedule.authority(db);
                debit(
                    db,
                    context.endpoint(),
                    self.schedule.keys.0[<C::Output<'db> as ValueKind>::QUERY],
                    Step::Recovery,
                )
            })
            .await;
        Ok(C::recover_from_cycle(db, cycle, last, value, input))
    }
}

fn debit(
    db: &dyn TransferDatabase,
    endpoint: &super::super::super::registration::TaskEndpoint<'_, '_>,
    key: DatabaseKeyIndex,
    step: Step,
) -> RunResult<()> {
    let mut event = Observation::new(Kind::PreDebit).key(key);
    event.step = Some(step);
    event.units = 1;
    event.sync = db
        .zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap()
        .sync_table()
        .test_transfer_state(key.key_index());
    trace::record(event);
    let result = endpoint.admit_work(1);
    let mut event = Observation::new(if result.is_ok() {
        Kind::DebitAccepted
    } else {
        Kind::DebitRefused
    })
    .key(key);
    event.step = Some(step);
    event.units = 1;
    trace::record(event);
    result
}

fn run_queries<'db, A, B>(
    db: &'db dyn TransferDatabase,
    a_ingredient: &'db IngredientImpl<A>,
    b_ingredient: &'db IngredientImpl<B>,
    input: Input,
    schedule: &Schedule,
) -> RunResult<u32>
where
    A: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = AValue>,
    B: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = BValue>,
{
    let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
    let a = registry.reserve(db, a_ingredient)?;
    let b = registry.reserve(db, b_ingredient)?;
    let providers = Providers {
        a: a.clone(),
        b: b.clone(),
        schedule,
    };
    let mut registry = registry;
    let binding = registry.provider(&providers)?;
    registry.bind_executable(&a, &binding)?;
    registry.bind_executable(&b, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        let context = endpoint.provider(binding)?;
        if schedule.data.worker == 0 {
            Ok(context.fetch_ref(&a, input.as_id())?.await?.0)
        } else {
            Ok(context.fetch_ref(&b, input.as_id())?.await?.0)
        }
    })
}

#[derive(Debug)]
struct WorkerReport {
    worker: usize,
    trace: TransferTrace,
    driver: Option<RunResult<u32>>,
    support: Option<AttemptSupport>,
}

struct WorkerOutcome<T> {
    result: thread::Result<T>,
    cleanup: thread::Result<()>,
}

impl<T> WorkerOutcome<T> {
    // Call only after preserving the trace, so neither panic can discard its evidence.
    fn into_result(self) -> thread::Result<T> {
        match (self.result, self.cleanup) {
            (Err(payload), Err(cleanup)) => {
                let message = cleanup
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| cleanup.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("non-string panic payload");
                eprintln!("MIXED secondary worker cleanup failure: {message}");
                Err(payload)
            }
            (Err(payload), Ok(())) | (Ok(_), Err(payload)) => Err(payload),
            (Ok(value), Ok(())) => Ok(value),
        }
    }
}

fn catch_worker<T>(db: &Db, body: impl FnOnce() -> T) -> WorkerOutcome<T> {
    let result = catch_unwind(AssertUnwindSafe(body));
    // Query, participant and schedule guards have all retired before this separate catch.
    // The peer can still be running, so shared graph audits remain post-join.
    let cleanup = catch_unwind(AssertUnwindSafe(|| {
        super::assert_worker_clean(db);
        SCHEDULE.with_borrow(|slot| assert!(slot.is_none()));
        ACYCLIC_SCHEDULE.with_borrow(|slot| assert!(slot.is_none()));
    }));
    WorkerOutcome { result, cleanup }
}

fn root_observation(db: &Db, input: Input, worker: usize, value: u32) {
    let keys = Keys::new(db, input);
    let function = db
        .zalsa()
        .lookup_ingredient(keys.0[worker].ingredient_index())
        .as_function()
        .unwrap();
    let memo = function.memo(db.zalsa(), input.as_id()).unwrap();
    let mut event = Observation::new(Kind::RootResult)
        .key(keys.0[worker])
        .memo(Some(memo.transfer_test_snapshot()));
    event.value = value;
    trace::record(event);
}
fn paired_worker(
    db: Db,
    participant: Participant<'_>,
    input: Input,
    data: ScheduleData,
    supports: Supports,
    allowance: usize,
    config: TraceConfig,
    reports: mpsc::Sender<WorkerReport>,
) -> PairedOutcome {
    let worker = data.worker;
    let driver = Cell::new(None);
    let (outcome, trace) = trace::collect(config, || {
        catch_worker(&db, || {
            let schedule = ScheduleOwner::install(data, Keys::new(&db, input), supports.clone());
            participant.run(&db, allowance, || {
                supports[worker]
                    .set(attempt_probe::current().expect("paired support"))
                    .unwrap();
                let result = run_queries(
                    &db,
                    a::fn_ingredient_(&db, db.zalsa()),
                    b::fn_ingredient_(&db, db.zalsa()),
                    input,
                    &schedule.0,
                );
                driver.set(Some(result));
                if let Ok(value) = result {
                    root_observation(&db, input, worker, value);
                }
                result
            })
        })
    });
    if outcome.result.is_err() || outcome.cleanup.is_err() {
        for record in &trace.records {
            eprintln!("MIXED native failure {record:?}");
        }
    }
    let report = WorkerReport {
        worker,
        trace,
        driver: driver.get(),
        support: supports[worker].get().cloned(),
    };
    reports
        .send(report)
        .expect("coordinator retains trace receiver");
    match outcome.into_result() {
        Ok(result) => result,
        Err(payload) => resume_unwind(payload),
    }
}
fn ordinary_worker(
    db: Db,
    input: Input,
    data: ScheduleData,
    config: TraceConfig,
    reports: mpsc::Sender<WorkerReport>,
) -> u32 {
    let worker = data.worker;
    let (outcome, trace) = trace::collect(config, || {
        catch_worker(&db, || {
            let _schedule = ScheduleOwner::install(
                data,
                Keys::new(&db, input),
                Arc::new([OnceLock::new(), OnceLock::new()]),
            );
            let value = if worker == 0 {
                a(&db, input).0
            } else {
                b(&db, input).0
            };
            root_observation(&db, input, worker, value);
            value
        })
    });
    if outcome.result.is_err() || outcome.cleanup.is_err() {
        for record in &trace.records {
            eprintln!("MIXED native failure {record:?}");
        }
    }
    reports
        .send(WorkerReport {
            worker,
            trace,
            driver: outcome.result.as_ref().ok().map(|value| Ok(*value)),
            support: None,
        })
        .unwrap();
    match outcome.into_result() {
        Ok(value) => value,
        Err(payload) => resume_unwind(payload),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Label {
    A,
    B,
    Limit,
    Marker,
}
fn label(db: &Db, input: Input, key: DatabaseKeyIndex) -> Label {
    let keys = Keys::new(db, input);
    assert_eq!(key.key_index(), input.as_id());
    if key == keys.0[0] {
        return Label::A;
    }
    if key == keys.0[1] {
        return Label::B;
    }
    match db
        .zalsa()
        .lookup_ingredient(key.ingredient_index())
        .debug_name()
    {
        "limit" => Label::Limit,
        "seed_marker" => Label::Marker,
        name => panic!("unexpected canonical origin ingredient {name:?}: {key:?}"),
    }
}
fn origins<C: Configuration>(db: &Db, ingredient: &IngredientImpl<C>, input: Input) -> Vec<Label> {
    let memo = ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            input.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
        )
        .expect("canonical completed memo");
    let snapshot = memo.transfer_test_snapshot();
    assert!(snapshot.has_value && snapshot.final_);
    assert!(!snapshot.heads.overflow);
    assert_eq!(snapshot.verified_at, db.zalsa().current_revision());
    assert!(snapshot.changed_at <= snapshot.verified_at);
    assert_eq!(snapshot.durability, Durability::LOW);
    if let Some(support) = snapshot.support {
        assert!(!support.explicitly_incomplete && !matches!(support.state, 1 | 3 | 4));
    }
    memo.header
        .origin()
        .edges()
        .iter()
        .map(|edge| {
            assert!(
                matches!(edge.kind(), QueryEdgeKind::Input),
                "return-only origin has an output"
            );
            label(db, input, edge.key())
        })
        .collect()
}
#[derive(Debug, PartialEq, Eq)]
struct Canonical {
    values: [u32; 2],
    origins: [Vec<Label>; 2],
}
fn input_paths(
    report: &Canonical,
    label: Label,
    path: &mut Vec<Label>,
    paths: &mut Vec<Vec<Label>>,
) {
    assert!(
        !path.contains(&label),
        "cycle in completed dependency graph: {path:?} -> {label:?}; {report:?}"
    );
    path.push(label);
    match label {
        Label::A | Label::B => {
            let index = usize::from(label == Label::B);
            for &dependency in &report.origins[index] {
                input_paths(report, dependency, path, paths);
            }
        }
        Label::Marker | Label::Limit => paths.push(path.clone()),
    }
    path.pop();
}
fn assert_input_paths(report: &Canonical, phase: &str) {
    let paths = [Label::A, Label::B].map(|root| {
        let mut paths = Vec::new();
        input_paths(report, root, &mut Vec::new(), &mut paths);
        paths
    });
    // Retain both B -> A -> Limit and B -> Limit when B reads a final A.
    eprintln!("MIXED {phase} final input paths {paths:?}; raw {report:?}");
    for (root, paths) in [Label::A, Label::B].into_iter().zip(paths) {
        for leaf in [Label::Marker, Label::Limit] {
            assert!(
                paths.iter().any(|path| path.last() == Some(&leaf)),
                "{phase} root {root:?} lost {leaf:?}: {paths:?}; {report:?}"
            );
        }
    }
}
fn canonical(db: &Db, input: Input) -> Canonical {
    Canonical {
        values: [a(db, input).0, b(db, input).0],
        origins: [
            origins(db, a::fn_ingredient_(db, db.zalsa()), input),
            origins(db, b::fn_ingredient_(db, db.zalsa()), input),
        ],
    }
}
fn assert_graph_clean(snapshot: GraphSnapshot) {
    assert!(
        !snapshot.edges.overflow && snapshot.edges.is_empty(),
        "live wait edges: {snapshot:?}"
    );
    assert!(
        !snapshot.pending.overflow && snapshot.pending.is_empty(),
        "unconsumed wait result: {snapshot:?}"
    );
    assert!(
        !snapshot.transferred.overflow && snapshot.transferred.is_empty(),
        "live transfer mapping: {snapshot:?}"
    );
    assert!(
        !snapshot.dependents.overflow && !snapshot.reverse.overflow,
        "graph snapshot overflow: {snapshot:?}"
    );
    for (_, entries) in snapshot.dependents.entries.into_iter().flatten() {
        assert!(
            !entries.overflow && entries.is_empty(),
            "live query-dependent thread: {snapshot:?}"
        );
    }
    for (_, entries) in snapshot.reverse.entries.into_iter().flatten() {
        assert!(
            !entries.overflow && entries.is_empty(),
            "live reverse transfer: {snapshot:?}"
        );
    }
}
fn audit(db: &Db, input: Input) -> [bool; 2] {
    super::assert_exclusion_released(db);
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    eprintln!("MIXED post-join graph {graph:?}");
    assert_graph_clean(graph);
    let keys = Keys::new(db, input);
    keys.0.map(|key| {
        let function = db
            .zalsa()
            .lookup_ingredient(key.ingredient_index())
            .as_function()
            .unwrap();
        if let Some(state) = function.sync_table().test_transfer_state(input.as_id()) {
            eprintln!(
                "MIXED detached marker {:?} {key:?}: {state:?}",
                label(db, input, key)
            );
            assert!(
                matches!(state.owner, SyncOwner::Transferred),
                "live thread claim: {state:?}"
            );
            assert!(!state.claimed_twice, "live reentrant claim: {state:?}");
            true
        } else {
            false
        }
    })
}

fn assert_repeat_seeds(records: &[Record]) {
    let mut repeats = 0;
    for published in records
        .iter()
        .filter(|record| record.event.kind == Kind::RootPublished)
    {
        let memo = published.event.memo.unwrap();
        if memo.final_ {
            continue;
        }
        assert_eq!(
            memo.support.map(|support| support.owner),
            published.event.session.map(|session| session.support.owner),
            "provisional publication lost its evaluation support: {published:?}"
        );
        let continuation: Vec<_> = records
            .iter()
            .filter(|record| {
                record.worker == published.worker && record.ordinal > published.ordinal
            })
            .take_while(|record| {
                !(record.event.kind == Kind::Terminal
                    && record.event.serial == published.event.serial)
            })
            .collect();
        let Some(body) = continuation.iter().find(|record| {
            record.event.kind == Kind::BodyValue && record.event.key == published.event.key
        }) else {
            continue;
        };
        repeats += 1;
        let seed = continuation
            .iter()
            .find(|record| {
                record.ordinal < body.ordinal
                    && record.event.kind == Kind::SeedActive
                    && record.event.identity == memo.identity
                    && record.event.key == published.event.key
            })
            .expect("Repeat must seed its exact retained predecessor before the next body");
        assert!(
            seed.event.decision,
            "Repeat silently rejected its retained predecessor: {seed:?}"
        );
        assert_eq!(
            seed.event.support.map(|support| support.owner),
            memo.support.map(|support| support.owner)
        );
        assert!(
            !continuation
                .iter()
                .any(|record| record.ordinal < body.ordinal
                    && record.event.kind == Kind::ColdInitial
                    && record.event.key == published.event.key),
            "Repeat replaced its canonical predecessor before comparing against it"
        );
    }
    assert!(
        repeats > 0,
        "fixture did not observe a retained Repeat predecessor"
    );
}

fn assert_prior_marker(records: &[Record], keys: Keys) {
    let final_body = records
        .iter()
        .rev()
        .find(|record| record.event.kind == Kind::BodyValue && record.event.key == Some(keys.0[1]))
        .expect("B executed a final body");
    assert_eq!(final_body.event.value, 3);
    let final_request = records
        .iter()
        .rev()
        .find(|record| {
            record.ordinal < final_body.ordinal
                && record.worker == final_body.worker
                && record.event.kind == Kind::ChildRequest
                && record.event.key == Some(keys.0[1])
        })
        .expect("final B body requested its actual child");
    let markers: Vec<_> = records
        .iter()
        .filter(|record| record.event.kind == Kind::MarkerRead)
        .collect();
    assert!(!markers.is_empty(), "no earlier iteration read Marker");
    assert!(
        markers
            .iter()
            .all(|record| record.ordinal < final_request.ordinal),
        "Marker must be retained from an earlier body, not read in the final body"
    );
}

fn recovery_progression(records: &[Record], keys: Keys, uses_last: bool) -> Vec<(u32, u32, u32)> {
    let mut progression = Vec::new();
    for record in records
        .iter()
        .filter(|record| record.event.kind == Kind::RecoveryValue)
    {
        assert_eq!(record.event.phase, Some("last/new/recovered"));
        let last = u32::try_from(record.event.units).unwrap();
        let new = u32::try_from(record.event.identity).unwrap();
        let recovered = record.event.value;
        assert_eq!(
            recovered,
            if uses_last {
                new.max(last.saturating_add(1).min(3))
            } else {
                new
            }
        );
        if record.worker == 0 && record.event.key == Some(keys.0[0]) {
            progression.push((last, new, recovered));
        }
    }
    if let Some(first) = progression.first() {
        assert_eq!(
            first.0, 0,
            "recovery did not start from its genuine initializer"
        );
        for pair in progression.windows(2) {
            assert_eq!(
                pair[1].0, pair[0].2,
                "recovery retained a different predecessor"
            );
        }
    }
    progression
}

#[derive(Debug)]
struct Baseline {
    initial: Canonical,
    edited: Canonical,
    edited_work: [usize; 6],
    detached: [bool; 2],
    initial_work: [usize; 6],
    uses_last: bool,
    recoveries: Vec<(u32, u32, u32)>,
}
fn followup(db: &mut Db, input: Input) -> (Canonical, Canonical, [usize; 6]) {
    let before = db.counts.snapshot();
    let initial = canonical(db, input);
    assert_eq!(initial.values, [3, 3]);
    assert_eq!(
        db.counts.snapshot(),
        before,
        "completed reads must not execute callbacks"
    );
    assert_eq!(canonical(db, input), initial);
    assert_eq!(db.counts.snapshot(), before);
    assert_input_paths(&initial, "initial");
    input.set_seed_marker(db).to(18);
    let edited = canonical(db, input);
    let after = db.counts.snapshot();
    let work = std::array::from_fn(|index| after[index] - before[index]);
    assert!(
        work[0] + work[3] > 0,
        "changing the recorded marker did not invalidate a body"
    );
    assert_eq!(edited.values, [3, 3]);
    assert_input_paths(&edited, "edited");
    (initial, edited, work)
}

fn reports(receiver: mpsc::Receiver<WorkerReport>, row: &str) -> [WorkerReport; 2] {
    let first = receiver
        .recv_timeout(STAGE_TIMEOUT)
        .expect("first worker reports after join");
    let second = receiver
        .recv_timeout(STAGE_TIMEOUT)
        .expect("second worker reports after join");
    let reports = if first.worker == 0 {
        [first, second]
    } else {
        [second, first]
    };
    assert_eq!(reports.each_ref().map(|report| report.worker), [0, 1]);
    for report in &reports {
        eprintln!(
            "MIXED {row} worker {} driver {:?} support {:?}",
            report.worker,
            report.driver,
            report.support.as_ref().map(trace::support_snapshot)
        );
    }
    // Print the complete merged trace before checking outcomes, so a native panic preserves
    // the selected identities and the decision immediately preceding its unwind.
    for record in merged(&reports) {
        eprintln!("MIXED {row} {record:?}");
    }
    for report in &reports {
        assert!(!report.trace.broken, "trace capacity or TLS borrow failure");
        for record in &report.trace.records {
            for memo in [record.event.memo, record.event.other_memo]
                .into_iter()
                .flatten()
            {
                assert!(
                    !memo.heads.overflow,
                    "third cycle head exceeds this experiment: {record:?}"
                );
            }
        }
    }
    reports
}
fn merged(reports: &[WorkerReport; 2]) -> Vec<Record> {
    let mut records: Vec<_> = reports
        .iter()
        .flat_map(|report| report.trace.records.iter().copied())
        .collect();
    records.sort_by_key(|record| record.ordinal);
    records
}
fn find<'a>(
    records: &'a [Record],
    description: &str,
    predicate: impl Fn(&Record) -> bool,
) -> &'a Record {
    records
        .iter()
        .find(|record| predicate(record))
        .unwrap_or_else(|| panic!("missing transfer observation: {description}"))
}
fn after<'a>(
    records: &'a [Record],
    previous: &Record,
    description: &str,
    predicate: impl Fn(&Record) -> bool,
) -> &'a Record {
    find(records, description, |record| {
        record.ordinal > previous.ordinal && predicate(record)
    })
}
fn query_event(record: &Record, worker: usize, kind: Kind, key: DatabaseKeyIndex) -> bool {
    record.worker == worker && record.event.kind == kind && record.event.key == Some(key)
}
fn assert_claim_lifetimes(records: &[Record]) {
    let mut claims = BTreeMap::new();
    for record in records {
        if record.event.kind == Kind::Claim {
            let serial = record.event.serial.unwrap();
            assert!(
                claims.insert(serial, (record.event.key, 0)).is_none(),
                "duplicate claim serial"
            );
        }
        if record.event.kind == Kind::Terminal {
            let entry = claims
                .get_mut(&record.event.serial.unwrap())
                .expect("terminal action follows acquisition");
            assert_eq!(entry.0, record.event.key);
            entry.1 += 1;
            assert_ne!(
                record.event.action,
                Some(Action::Panic),
                "native poison is not a restart"
            );
        }
        if matches!(record.event.kind, Kind::CommitCurrent | Kind::TargetCurrent) {
            assert!(
                record.event.decision,
                "publication rejected its selected identity: {record:?}"
            );
        }
        if matches!(
            record.event.kind,
            Kind::Target | Kind::TargetCurrent | Kind::TargetPublished
        ) && record.event.action == Some(Action::Finalize)
            && let Some(support) = record.event.memo.and_then(|memo| memo.support)
        {
            assert!(
                !support.explicitly_incomplete && !matches!(support.state, 1 | 3 | 4),
                "incomplete final target: {record:?}"
            );
        }
        if record.event.kind == Kind::RootPublished
            && let Some(memo) = record.event.memo
            && memo.final_
            && let Some(support) = memo.support
        {
            assert!(
                !support.explicitly_incomplete && !matches!(support.state, 1 | 3 | 4),
                "incomplete final root: {record:?}"
            );
        }
        if record.event.kind == Kind::SupportIncoming
            && let (Some(previous), Some(incoming)) =
                (record.event.previous_support, record.event.support)
        {
            assert_eq!(
                previous.owner, incoming.owner,
                "attempted mixed-owner import: {record:?}"
            );
        }
        if record.event.kind == Kind::SupportAccepted {
            assert_eq!(
                record.event.support.unwrap().owner,
                record.event.session.unwrap().support.owner,
                "foreign accepted support: {record:?}"
            );
        }
    }
    assert!(!claims.is_empty());
    assert!(
        claims.values().all(|(_, terminals)| *terminals == 1),
        "each guard needs exactly one terminal action: {claims:?}"
    );
}

struct Handoff<'a> {
    donor_claim: &'a Record,
    initial: &'a Record,
    provisional: &'a Record,
    mapping: &'a Record,
    wait: &'a Record,
}
fn handoff<'a>(records: &'a [Record], keys: Keys) -> Handoff<'a> {
    let a = keys.0[0];
    let b = keys.0[1];
    let receiver_claim = find(records, "receiver owns A", |r| {
        query_event(r, 0, Kind::Claim, a)
    });
    let donor_claim = find(records, "donor owns B", |r| {
        query_event(r, 1, Kind::Claim, b)
    });
    let edge = after(records, donor_claim, "actual receiver edge on B", |r| {
        query_event(r, 0, Kind::Edge, b)
    });
    assert!(receiver_claim.ordinal < edge.ordinal);
    assert_eq!(edge.event.from, Some(receiver_claim.thread));
    assert_eq!(edge.event.peer, Some(donor_claim.thread));
    let initial_value = after(
        records,
        edge,
        "donor executes genuine zero initializer",
        |r| query_event(r, 1, Kind::InitialValue, a) && r.event.value == 0,
    );
    let initial = after(records, initial_value, "donor inserts A initial", |r| {
        query_event(r, 1, Kind::InitialInserted, a)
    });
    let marker = after(
        records,
        initial,
        "donor reads real marker from zero approximation",
        |r| query_event(r, 1, Kind::MarkerRead, b) && r.event.value == 17,
    );
    let provisional = after(records, marker, "donor publishes provisional B", |r| {
        query_event(r, 1, Kind::RootPublished, b) && r.event.memo.is_some_and(|memo| !memo.final_)
    });
    assert_eq!(provisional.event.serial, donor_claim.event.serial);
    let mapping = after(records, provisional, "real B to A transfer mapping", |r| {
        query_event(r, 1, Kind::Mapping, b) && r.event.other_key == Some(a)
    });
    assert_eq!(mapping.event.peer, Some(receiver_claim.thread));
    let wait = after(records, mapping, "donor blocks in transfer", |r| {
        query_event(r, 1, Kind::TransferWaitBegin, b)
    });
    let receiver_terminal = after(
        records,
        mapping,
        "receiver releases transfer target A",
        |r| query_event(r, 0, Kind::Terminal, a) && r.event.serial == receiver_claim.event.serial,
    );
    let target = receiver_terminal.event.sync.unwrap();
    assert!(target.anyone_waiting && target.is_transfer_target);
    Handoff {
        donor_claim,
        initial,
        provisional,
        mapping,
        wait,
    }
}
fn donor_refetch<'a>(records: &'a [Record], keys: Keys, handoff: &Handoff<'_>) -> &'a Record {
    let b = keys.0[1];
    let consumed = after(
        records,
        handoff.wait,
        "donor consumes actual wait result",
        |r| query_event(r, 1, Kind::WaitConsumed, keys.0[0]),
    );
    let returned = after(
        records,
        consumed,
        "donor transfer returns mandatory refetch",
        |r| query_event(r, 1, Kind::TransferWaitEnd, b) && r.event.decision,
    );
    assert!(matches!(
        (consumed.event.wait, returned.event.wait),
        (Some(WaitResult::Completed), Some(WaitResult::Completed))
            | (Some(WaitResult::Cancelled), Some(WaitResult::Cancelled))
    ));
    let guard_return = after(records, returned, "original B guard returns", |r| {
        query_event(r, 1, Kind::TransferEnd, b)
            && r.event.serial == handoff.donor_claim.event.serial
            && r.event.decision
    });
    let refetch = after(
        records,
        guard_return,
        "publication discards donor pointer",
        |r| {
            query_event(r, 1, Kind::Refetch, b)
                && r.event.serial == handoff.donor_claim.event.serial
                && r.event.decision
        },
    );
    let probe = after(records, refetch, "Complete(None) restarts Probe", |r| {
        query_event(r, 1, Kind::Executed, b) && !r.event.decision && r.event.memo.is_none()
    });
    let result = after(records, probe, "donor returns canonical final value", |r| {
        query_event(r, 1, Kind::RootResult, b)
    });
    let memo = result.event.memo.unwrap();
    assert_eq!(result.event.value, 3);
    assert!(memo.final_ && memo.has_value);
    assert_ne!(
        memo.identity,
        handoff.provisional.event.memo.unwrap().identity,
        "donor retained its stale selected pointer"
    );
    if let (Some(before), Some(after)) = (handoff.wait.event.session, returned.event.session) {
        assert_eq!(
            before, after,
            "donor scope, support, stamp, or allowance changed during transfer wait"
        );
    }
    probe
}

fn ordinary(uses_last: bool) -> Baseline {
    eprintln!("MIXED ordinary baseline starts");
    let mut db = Db::with_recovery(uses_last);
    let input = Input::new(&db, 3, 17);
    let keys = Keys::new(&db, input);
    let [left, right] = schedules();
    let ordinal = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = mpsc::channel();
    let left_db = db.clone();
    let right_db = db.clone();
    let (left, right) = thread::scope(|scope| {
        let left_tx = tx.clone();
        let left_ordinal = ordinal.clone();
        let left = scope.spawn(move || {
            ordinary_worker(
                left_db,
                input,
                left,
                TraceConfig {
                    worker: 0,
                    ordinal: left_ordinal,
                },
                left_tx,
            )
        });
        let right = scope.spawn(move || {
            ordinary_worker(
                right_db,
                input,
                right,
                TraceConfig { worker: 1, ordinal },
                tx,
            )
        });
        (left.join(), right.join())
    });
    let reports = reports(rx, "ordinary");
    assert_eq!((left.unwrap(), right.unwrap()), (3, 3));
    let records = merged(&reports);
    assert_claim_lifetimes(&records);
    let handoff = handoff(&records, keys);
    donor_refetch(&records, keys, &handoff);
    assert!(records.iter().all(|record| record.event.session.is_none()));
    assert_repeat_seeds(&records);
    assert_prior_marker(&records, keys);
    let recoveries = recovery_progression(&records, keys, uses_last);
    let detached = audit(&db, input);
    let initial_work = db.counts.snapshot();
    let (initial, edited, edited_work) = followup(&mut db, input);
    eprintln!("MIXED ordinary origins {initial:?}, edited {edited:?}, edited work {edited_work:?}");
    Baseline {
        initial,
        edited,
        edited_work,
        detached,
        initial_work,
        uses_last,
        recoveries,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Debit {
    step: Option<Step>,
    key: Option<Label>,
    units: usize,
    spent: usize,
}
fn debit_prefix(
    db: &Db,
    input: Input,
    records: &[Record],
    until: &Record,
    allowance: usize,
) -> Vec<Debit> {
    let mut remaining = allowance;
    let mut pending = None;
    let mut debits = Vec::new();
    for record in records
        .iter()
        .filter(|record| record.worker == 0 && record.ordinal < until.ordinal)
    {
        let event = record.event;
        if event.kind == Kind::PreDebit {
            assert!(
                pending
                    .replace((
                        event.step.unwrap(),
                        label(db, input, event.key.unwrap()),
                        event.units
                    ))
                    .is_none()
            );
        }
        if let Some(session) = event.session {
            assert!(
                session.remaining <= remaining,
                "allowance was refilled: {record:?}"
            );
            if session.remaining < remaining {
                let units = remaining - session.remaining;
                assert_eq!(
                    event.kind,
                    Kind::Admission,
                    "actual debit must precede its admission observation"
                );
                assert_eq!(event.phase, Some("work"));
                assert_eq!(event.units, units);
                if let Some((_, _, requested)) = pending {
                    assert_eq!(requested, units);
                }
                debits.push(Debit {
                    step: pending.map(|(step, _, _)| step),
                    key: pending.map(|(_, key, _)| key),
                    units,
                    spent: allowance - session.remaining,
                });
                remaining = session.remaining;
            }
        }
        if event.kind == Kind::DebitAccepted {
            assert!(pending.take().is_some());
        }
        assert_ne!(
            event.kind,
            Kind::DebitRefused,
            "earlier refusal missed the receiver boundary"
        );
    }
    assert!(pending.is_none());
    assert_eq!(until.event.session.unwrap().remaining, remaining);
    debits
}

#[derive(Debug, PartialEq, Eq)]
enum MixedOutcome {
    Controlled(PairedOutcome),
    Ordinary(Result<u32, PairInstallError>),
}

fn mixed_worker(
    db: Db,
    participant: Participant<'_>,
    input: Input,
    data: ScheduleData,
    supports: Supports,
    controlled: bool,
    allowance: usize,
    config: TraceConfig,
    reports: mpsc::Sender<WorkerReport>,
) -> MixedOutcome {
    if controlled {
        return MixedOutcome::Controlled(paired_worker(
            db,
            participant,
            input,
            data,
            supports,
            allowance,
            config,
            reports,
        ));
    }
    let worker = data.worker;
    let (outcome, trace) = trace::collect(config, || {
        catch_worker(&db, || {
            let schedule = ScheduleOwner::install(data, Keys::new(&db, input), supports);
            participant.run_ordinary(&db, || {
                schedule.0.authority(&db);
                let value = if worker == 0 {
                    a(&db, input).0
                } else {
                    b(&db, input).0
                };
                schedule.0.authority(&db);
                root_observation(&db, input, worker, value);
                value
            })
        })
    });
    reports
        .send(WorkerReport {
            worker,
            trace,
            driver: outcome
                .result
                .as_ref()
                .ok()
                .and_then(|result| result.as_ref().ok())
                .map(|value| Ok(*value)),
            support: None,
        })
        .unwrap();
    match outcome.into_result() {
        Ok(value) => MixedOutcome::Ordinary(value),
        Err(payload) => resume_unwind(payload),
    }
}

fn assert_mixed_sessions(
    records: &[Record],
    reports: &[WorkerReport; 2],
    controlled: usize,
    refusal: bool,
) {
    let support = reports[controlled]
        .support
        .as_ref()
        .expect("controlled support");
    assert!(reports[1 - controlled].support.is_none());
    let owner = trace::support_snapshot(support).owner;
    assert_eq!(
        trace::support_snapshot(support).state,
        if refusal { 1 } else { 2 }
    );
    let mut remaining = ALLOWANCE;
    for record in records {
        if record.worker != controlled {
            assert!(
                record.event.session.is_none(),
                "ordinary worker acquired a session: {record:?}"
            );
            assert!(!matches!(
                record.event.kind,
                Kind::PreDebit | Kind::DebitAccepted | Kind::DebitRefused | Kind::Admission
            ));
        } else if let Some(session) = record.event.session {
            assert_eq!(session.support.owner, owner);
            assert!(
                session.remaining <= remaining,
                "controlled allowance increased: {record:?}"
            );
            remaining = session.remaining;
        }
    }
}

struct Restart<'a> {
    claim: &'a Record,
    iteration: &'a Record,
}

fn mixed_restart<'a>(
    records: &'a [Record],
    keys: Keys,
    handoff: &Handoff<'_>,
    controlled: usize,
) -> Restart<'a> {
    let foreign = handoff.provisional.event.memo.unwrap();
    assert!(!foreign.final_);
    if controlled == 0 {
        assert!(foreign.support.is_none());
        assert!(handoff.initial.event.memo.unwrap().support.is_none());
    } else {
        assert_eq!(foreign.support.unwrap().state, 0);
        assert_eq!(
            foreign.support.unwrap().owner,
            handoff.initial.event.memo.unwrap().support.unwrap().owner
        );
    }
    let reuse = after(
        records,
        handoff.mapping,
        "receiver classifies donor provisional",
        |r| r.worker == 0 && r.event.kind == Kind::Reuse && r.event.identity == foreign.identity,
    );
    assert_eq!(reuse.event.reuse, Some(MemoReuse::Stale));
    let claim = after(records, reuse, "receiver reclaims transferred B", |r| {
        query_event(r, 0, Kind::Claim, keys.0[1]) && r.event.mode == Some(Mode::SelfOnly)
    });
    assert!(claim.event.sync.unwrap().claimed_twice);
    assert_ne!(claim.event.serial, handoff.donor_claim.event.serial);
    let verified = after(
        records,
        claim,
        "receiver verifier rejects old provisional",
        |r| {
            query_event(r, 0, Kind::Verification, keys.0[1])
                && r.event.serial == claim.event.serial
                && !r.event.decision
        },
    );
    assert_eq!(verified.event.memo.unwrap().identity, foreign.identity);
    let supplied = after(
        records,
        verified,
        "receiver prepares the actual old baseline",
        |r| {
            query_event(r, 0, Kind::PrepareSupplied, keys.0[1])
                && r.event.serial == claim.event.serial
        },
    );
    assert_eq!(supplied.event.memo.unwrap().identity, foreign.identity);
    let declined = after(records, supplied, "receiver rejects donor seed", |r| {
        r.worker == 0
            && r.event.kind == Kind::SeedAllowed
            && r.event.identity == foreign.identity
            && !r.event.decision
    });
    let retained = after(records, declined, "receiver removes donor baseline", |r| {
        query_event(r, 0, Kind::PrepareRetained, keys.0[1]) && r.event.serial == claim.event.serial
    });
    assert!(retained.event.memo.is_none());
    let iteration = after(
        records,
        retained,
        "receiver starts B without the donor seed",
        |r| query_event(r, 0, Kind::Iteration, keys.0[1]) && r.event.serial == claim.event.serial,
    );
    assert!(iteration.event.memo.is_none() && iteration.event.other_memo.is_none());
    assert_eq!(iteration.event.session.is_some(), controlled == 0);
    Restart { claim, iteration }
}

#[derive(Debug)]
struct Calibration {
    allowance: usize,
    prefix: Vec<Debit>,
}

fn mixed_cycle(
    baseline: &Baseline,
    controlled: usize,
    calibration: Option<&Calibration>,
) -> Option<Calibration> {
    let refusal = calibration.is_some();
    assert!(!refusal || controlled == 0);
    let row = format!(
        "cycle controlled={controlled} refusal={refusal} uses_last={}",
        baseline.uses_last
    );
    let mut db = Db::with_recovery(baseline.uses_last);
    let input = Input::new(&db, 3, 17);
    let keys = Keys::new(&db, input);
    let [left_schedule, right_schedule] = schedules();
    let supports = Arc::new([OnceLock::new(), OnceLock::new()]);
    let ordinal = Arc::new(AtomicUsize::new(0));
    let allowance = calibration.map_or(ALLOWANCE, |c| c.allowance);
    let (tx, rx) = mpsc::channel();
    let left_supports = supports.clone();
    let left_ordinal = ordinal.clone();
    let left_tx = tx.clone();
    let (left, right) = run_pair(
        &db,
        move |db, participant| {
            mixed_worker(
                db,
                participant,
                input,
                left_schedule,
                left_supports,
                controlled == 0,
                allowance,
                TraceConfig {
                    worker: 0,
                    ordinal: left_ordinal,
                },
                left_tx,
            )
        },
        move |db, participant| {
            mixed_worker(
                db,
                participant,
                input,
                right_schedule,
                supports,
                controlled == 1,
                ALLOWANCE,
                TraceConfig { worker: 1, ordinal },
                tx,
            )
        },
    )
    .unwrap();
    eprintln!(
        "MIXED {row} initial callback counts {:?}; ordinary {:?}",
        db.counts.snapshot(),
        baseline.initial_work
    );
    let reports = reports(rx, &row);
    let detached = audit(&db, input);
    eprintln!(
        "MIXED {row} detached {detached:?}; ordinary {:?}",
        baseline.detached
    );
    let outcomes = [left.unwrap(), right.unwrap()];
    for (worker, outcome) in outcomes.iter().enumerate() {
        let expected = if worker != controlled {
            MixedOutcome::Ordinary(Ok(3))
        } else if refusal {
            MixedOutcome::Controlled(Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)))
        } else {
            MixedOutcome::Controlled(Ok(AttemptOutcome::Complete(Ok(3))))
        };
        assert_eq!(outcome, &expected);
        assert_eq!(
            reports[worker].driver,
            Some(if refusal && worker == controlled {
                Err(RunError::Refused(Incomplete::Allowance))
            } else {
                Ok(3)
            })
        );
    }
    let records = merged(&reports);
    assert_mixed_sessions(&records, &reports, controlled, refusal);
    assert_claim_lifetimes(&records);
    assert_repeat_seeds(&records);
    assert_prior_marker(&records, keys);
    let recoveries = recovery_progression(&records, keys, baseline.uses_last);
    if !refusal {
        assert_eq!(
            recoveries, baseline.recoveries,
            "mixed recovery lost its native predecessor progression"
        );
    }
    let handoff = handoff(&records, keys);
    let restart = mixed_restart(&records, keys, &handoff, controlled);
    donor_refetch(&records, keys, &handoff);
    let measured = if controlled == 0 {
        let debit = after(
            &records,
            restart.iteration,
            "first reclaimed controlled Body(B)",
            |r| query_event(r, 0, Kind::PreDebit, keys.0[1]) && r.event.step == Some(Step::Body),
        );
        let prefix = debit_prefix(&db, input, &records, debit, allowance);
        let spent = allowance - debit.event.session.unwrap().remaining;
        assert!(
            prefix.iter().any(|d| d.step.is_none()),
            "missing canonical wait retry charge"
        );
        if let Some(calibration) = calibration {
            assert_eq!(prefix, calibration.prefix);
            assert_eq!(spent, calibration.allowance);
            assert_eq!(debit.event.session.unwrap().remaining, 0);
            let refused = after(&records, debit, "real reclaimed Body(B) refusal", |r| {
                query_event(r, 0, Kind::DebitRefused, keys.0[1]) && r.event.step == Some(Step::Body)
            });
            assert_eq!(
                records
                    .iter()
                    .find(|r| r.event.kind == Kind::DebitRefused)
                    .unwrap()
                    .ordinal,
                refused.ordinal
            );
            let abort = after(&records, refused, "deepest B abort", |r| {
                query_event(r, 0, Kind::Terminal, keys.0[1])
                    && r.event.serial == restart.claim.event.serial
                    && r.event.action == Some(Action::Abort)
            });
            let undo = after(&records, abort, "undo transferred B", |r| {
                query_event(r, 0, Kind::Undo, keys.0[1])
            });
            let ancestor = after(&records, undo, "A abort follows B cleanup", |r| {
                query_event(r, 0, Kind::Terminal, keys.0[0])
                    && r.event.action == Some(Action::Abort)
            });
            let wake = after(
                &records,
                ancestor,
                "native donor receives cancelled wake",
                |r| {
                    r.worker == 0
                        && r.event.kind == Kind::Unblock
                        && r.event.from == Some(handoff.donor_claim.thread)
                        && matches!(r.event.wait, Some(WaitResult::Cancelled))
                },
            );
            let resumed = after(
                &records,
                wake,
                "ordinary donor resumes native transfer",
                |r| {
                    query_event(r, 1, Kind::TransferWaitEnd, keys.0[1])
                        && matches!(r.event.wait, Some(WaitResult::Cancelled))
                },
            );
            assert!(resumed.event.session.is_none());
            after(
                &records,
                resumed,
                "ordinary donor executes after controlled refusal",
                |r| r.worker == 1 && r.event.kind == Kind::BodyValue,
            );
            assert!(!records.iter().any(|r| r.worker == 0
                && r.ordinal > refused.ordinal
                && matches!(
                    r.event.kind,
                    Kind::BodyValue | Kind::DebitAccepted | Kind::RootPublished
                )));
        } else {
            after(&records, debit, "reclaimed Body(B) succeeds", |r| {
                query_event(r, 0, Kind::DebitAccepted, keys.0[1])
            });
            assert!(spent > 0 && spent < ALLOWANCE);
            assert!(!records.iter().any(|r| r.event.kind == Kind::DebitRefused));
        }
        Some(Calibration {
            allowance: spent,
            prefix,
        })
    } else {
        after(
            &records,
            restart.iteration,
            "native receiver executes reclaimed B",
            |r| query_event(r, 0, Kind::BodyValue, keys.0[1]),
        );
        assert!(!records.iter().any(|r| r.event.kind == Kind::DebitRefused));
        None
    };
    let (initial, edited, work) = followup(&mut db, input);
    eprintln!(
        "MIXED {row} initial {initial:?}; edited {edited:?}; work {work:?}; ordinary {baseline:?}; calibration {measured:?}"
    );
    // A new origin shape needs its own native causal evidence before this comparison changes.
    assert_eq!(
        initial, baseline.initial,
        "unclassified mixed initial graph"
    );
    assert_eq!(edited, baseline.edited);
    assert_eq!(work, baseline.edited_work);
    measured
}

fn acyclic_counts(db: &Db) -> [usize; 3] {
    db.counts
        .1
        .each_ref()
        .map(|count| count.load(Ordering::SeqCst))
}

fn acyclic_value(db: &dyn TransferDatabase, input: Input, query: usize, child: Option<u32>) -> u32 {
    db.counts().1[query].fetch_add(1, Ordering::SeqCst);
    let value = child.map_or_else(|| input.limit(db), |child| child + 1);
    let mut event = Observation::new(Kind::BodyValue).key(acyclic_keys(db, input)[query]);
    event.value = value;
    trace::record(event);
    value
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn child(db: &dyn TransferDatabase, input: Input) -> u32 {
    acyclic_value(db, input, 0, None)
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn parent(db: &dyn TransferDatabase, input: Input) -> u32 {
    let child = child(db, input);
    acyclic_gate(db);
    acyclic_value(db, input, 1, Some(child))
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn consumer(db: &dyn TransferDatabase, input: Input) -> u32 {
    acyclic_value(db, input, 2, Some(parent(db, input)))
}

#[crate::tracked(returns(copy))]
fn ordinary_parent(db: &dyn TransferDatabase, input: Input) -> u32 {
    parent(db, input)
}

fn acyclic_keys(db: &dyn TransferDatabase, input: Input) -> [DatabaseKeyIndex; 3] {
    [
        child::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
        parent::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
        consumer::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
    ]
}

struct AcyclicSchedule {
    owner: bool,
    parent: DatabaseKeyIndex,
    entered: Cell<bool>,
    wait_sent: Cell<bool>,
    ready: mpsc::SyncSender<Gate>,
    release: mpsc::SyncSender<Gate>,
    receive: RefCell<Option<mpsc::Receiver<Gate>>>,
    supports: Supports,
    worker: usize,
}

impl AcyclicSchedule {
    fn receive(&self) {
        if let Some(receive) = self.receive.borrow_mut().take() {
            assert!(
                matches!(receive.recv_timeout(STAGE_TIMEOUT), Ok(Gate::Ready)),
                "acyclic fixture gate failed"
            );
        }
    }
    fn authority(&self, db: &dyn TransferDatabase) {
        if let Some(current) = attempt_probe::current() {
            let own = self.supports[self.worker].get().unwrap();
            assert!(current.same_owner(own));
            assert!(own.local_ownership(db.zalsa()).is_some());
            assert!(attempt_probe::remaining_allowance_for_diagnostics(db).is_some());
        } else {
            assert_eq!(attempt_probe::remaining_allowance_for_diagnostics(db), None);
            assert!(!attempt_probe::is_incomplete(db));
            if let Some(peer) = self.supports[1 - self.worker].get() {
                assert!(peer.local_ownership(db.zalsa()).is_none());
                assert!(!peer.owns_current_session(db.zalsa()));
            }
        }
    }
    fn before_root(&self, db: &dyn TransferDatabase) {
        self.authority(db);
        if !self.owner {
            self.receive();
        }
    }
    fn before_parent(&self, db: &dyn TransferDatabase) {
        if self.owner && !self.entered.replace(true) {
            self.authority(db);
            trace::record(Observation::new(Kind::Gate).key(self.parent));
            self.ready.try_send(Gate::Ready).unwrap();
            self.receive();
        }
    }
}

thread_local! { static ACYCLIC_SCHEDULE: RefCell<Option<Rc<AcyclicSchedule>>> = const { RefCell::new(None) }; }
struct AcyclicInstalled(Rc<AcyclicSchedule>);
impl AcyclicInstalled {
    fn install(schedule: AcyclicSchedule) -> Self {
        let schedule = Rc::new(schedule);
        ACYCLIC_SCHEDULE.with_borrow_mut(|slot| assert!(slot.replace(schedule.clone()).is_none()));
        Self(schedule)
    }
}
impl Drop for AcyclicInstalled {
    fn drop(&mut self) {
        ACYCLIC_SCHEDULE.with_borrow_mut(|slot| *slot = None);
        let _ = self.0.ready.try_send(Gate::Abort);
        let _ = self.0.release.try_send(Gate::Abort);
    }
}
fn acyclic_gate(db: &dyn TransferDatabase) {
    if let Some(schedule) = ACYCLIC_SCHEDULE.with_borrow(Clone::clone) {
        schedule.before_parent(db);
    }
}

struct AcyclicProviders<'db, C: Configuration, P: Configuration, R: Configuration> {
    child: Route<'db, C>,
    parent: Route<'db, P>,
    consumer: Route<'db, R>,
}
impl<'run, 'db: 'run, C, P, R, Q> ExecutableRouteProvider<'run, 'db, Q>
    for AcyclicProviders<'db, C, P, R>
where
    C: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = u32>,
    P: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = u32>,
    R: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = u32>,
    Q: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = u32>,
{
    // Conversion reconstructs one Input handle; equality compares one u32.
    fixture_native_value!(executable, 'run, 'db, Q, 1);

    async fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db dyn TransferDatabase,
        input: Input,
    ) -> RunResult<u32> {
        let query = if TypeId::of::<Q>() == TypeId::of::<C>() {
            0
        } else if TypeId::of::<Q>() == TypeId::of::<P>() {
            1
        } else {
            assert_eq!(TypeId::of::<Q>(), TypeId::of::<R>());
            2
        };
        let key = [
            self.child.database_key(input.as_id()),
            self.parent.database_key(input.as_id()),
            self.consumer.database_key(input.as_id()),
        ][query];
        assert_eq!(
            db.zalsa_local().active_query().map(|(key, _)| key),
            Some(key)
        );
        let value = match query {
            0 => None,
            1 => Some(*context.fetch_ref(&self.child, input.as_id())?.await?),
            _ => Some(*context.fetch_ref(&self.parent, input.as_id())?.await?),
        };
        Ok(context
            .endpoint()
            .local_call(|| {
                if query == 1 {
                    acyclic_gate(db);
                }
                debit(
                    db,
                    context.endpoint(),
                    acyclic_keys(db, input)[query],
                    Step::Body,
                )?;
                Ok(acyclic_value(db, input, query, value))
            })
            .await)
    }
    async fn initial(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn TransferDatabase,
        _id: Id,
        _input: Input,
    ) -> RunResult<u32> {
        Err(RunError::Contract(
            "acyclic mixed query requested initial value",
        ))
    }
    async fn recover<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db dyn TransferDatabase,
        _cycle: &'call Cycle<'call>,
        _last: &'call u32,
        _value: u32,
        _input: Input,
    ) -> RunResult<u32>
    where
        'run: 'call,
    {
        Err(RunError::Contract("acyclic mixed query requested recovery"))
    }
}

fn acyclic_run<'db, C, P, R>(
    db: &'db dyn TransferDatabase,
    input: Input,
    child_ingredient: &'db IngredientImpl<C>,
    parent_ingredient: &'db IngredientImpl<P>,
    consumer_ingredient: &'db IngredientImpl<R>,
    validation: bool,
) -> RunResult<u32>
where
    C: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = u32>,
    P: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = u32>,
    R: Configuration<DbView = dyn TransferDatabase, Input<'db> = Input, Output<'db> = u32>,
{
    let mut registry = RegistryBuilder::new(db, &ADMISSION)?;
    let child = registry.reserve(db, child_ingredient)?;
    let parent = registry.reserve(db, parent_ingredient)?;
    let consumer = registry.reserve(db, consumer_ingredient)?;
    let providers = AcyclicProviders {
        child: child.clone(),
        parent: parent.clone(),
        consumer: consumer.clone(),
    };
    let mut registry = registry;
    let binding = registry.provider(&providers)?;
    registry.bind_executable(&child, &binding)?;
    registry.bind_executable(&parent, &binding)?;
    registry.bind_executable(&consumer, &binding)?;
    registry.seal()?.run(move |endpoint| async move {
        let context = endpoint.provider(binding)?;
        let value = if validation {
            context.fetch_ref(&consumer, input.as_id())?.await?
        } else {
            context.fetch_ref(&parent, input.as_id())?.await?
        };
        Ok(*value)
    })
}

struct AcyclicReport {
    report: WorkerReport,
    validation: Vec<TraceEvent>,
}

fn acyclic_worker(
    db: Db,
    participant: Participant<'_>,
    input: Input,
    schedule: AcyclicSchedule,
    controlled: bool,
    validation: bool,
    allowance: usize,
    config: TraceConfig,
    reports: mpsc::Sender<AcyclicReport>,
) -> MixedOutcome {
    let worker = schedule.worker;
    let supports = schedule.supports.clone();
    let driver = Cell::new(None);
    let ((outcome, validation), trace) = trace::collect(config, || {
        validation_trace::collect(|| {
            catch_worker(&db, || {
                let schedule = AcyclicInstalled::install(schedule);
                if controlled {
                    MixedOutcome::Controlled(participant.run(&db, allowance, || {
                        supports[worker]
                            .set(attempt_probe::current().unwrap())
                            .unwrap();
                        schedule.0.before_root(&db);
                        let result = acyclic_run(
                            &db,
                            input,
                            child::fn_ingredient_(&db, db.zalsa()),
                            parent::fn_ingredient_(&db, db.zalsa()),
                            consumer::fn_ingredient_(&db, db.zalsa()),
                            validation,
                        );
                        driver.set(Some(result));
                        result
                    }))
                } else {
                    MixedOutcome::Ordinary(participant.run_ordinary(&db, || {
                        schedule.0.before_root(&db);
                        let value = ordinary_parent(&db, input);
                        schedule.0.authority(&db);
                        driver.set(Some(Ok(value)));
                        value
                    }))
                }
            })
        })
    });
    reports
        .send(AcyclicReport {
            report: WorkerReport {
                worker,
                trace,
                driver: driver.get(),
                support: supports[worker].get().cloned(),
            },
            validation,
        })
        .unwrap();
    match outcome.into_result() {
        Ok(value) => value,
        Err(payload) => resume_unwind(payload),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct AcyclicCanonical {
    values: [u32; 3],
    edges: [Vec<&'static str>; 3],
}

fn acyclic_canonical(db: &Db, input: Input) -> AcyclicCanonical {
    let values = [child(db, input), parent(db, input), consumer(db, input)];
    let edges = acyclic_keys(db, input).map(|key| {
        let memo = db
            .zalsa()
            .lookup_ingredient(key.ingredient_index())
            .as_function()
            .unwrap()
            .memo(db.zalsa(), input.as_id())
            .unwrap();
        let snapshot = memo.transfer_test_snapshot();
        assert!(snapshot.final_ && snapshot.has_value);
        assert!(!snapshot.heads.overflow && snapshot.heads.is_empty());
        assert_eq!(snapshot.verified_at, db.zalsa().current_revision());
        assert!(snapshot.changed_at <= snapshot.verified_at);
        assert_eq!(snapshot.durability, Durability::LOW);
        assert!(snapshot.support.is_none());
        memo.header()
            .origin()
            .edges()
            .iter()
            .map(|edge| {
                assert!(matches!(edge.kind(), QueryEdgeKind::Input));
                db.zalsa()
                    .lookup_ingredient(edge.key().ingredient_index())
                    .debug_name()
            })
            .collect()
    });
    AcyclicCanonical { values, edges }
}

fn acyclic_audit(db: &Db, input: Input) {
    super::assert_exclusion_released(db);
    assert_graph_clean(db.zalsa().runtime().test_transfer_graph_snapshot());
    for key in acyclic_keys(db, input)
        .into_iter()
        .chain([ordinary_parent::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id())])
    {
        assert!(
            db.zalsa()
                .lookup_ingredient(key.ingredient_index())
                .as_function()
                .unwrap()
                .sync_table()
                .test_transfer_state(key.key_index())
                .is_none()
        );
    }
}

fn acyclic_case(controlled_owner: bool, validation: bool, cutoff: Option<usize>) -> usize {
    assert!(!validation || !controlled_owner);
    assert!(cutoff.is_none() || controlled_owner);
    let row = format!(
        "acyclic controlled_owner={controlled_owner} validation={validation} cutoff={cutoff:?}"
    );
    let mut db = Db::default();
    let input = Input::new(&db, 3, 17);
    let mut baseline = Db::default();
    let baseline_input = Input::new(&baseline, 3, 17);
    let keys = acyclic_keys(&db, input);
    let old = if validation {
        assert_eq!(consumer(&db, input), 5);
        assert_eq!(consumer(&baseline, baseline_input), 5);
        let old = [keys[1], keys[2]].map(|key| {
            db.zalsa()
                .lookup_ingredient(key.ingredient_index())
                .as_function()
                .unwrap()
                .memo(db.zalsa(), input.as_id())
                .unwrap()
                .transfer_test_snapshot()
        });
        input.set_limit(&mut db).to(4);
        baseline_input.set_limit(&mut baseline).to(4);
        Some(old)
    } else {
        None
    };
    let before = acyclic_counts(&db);
    let baseline_before = acyclic_counts(&baseline);
    let controlled = usize::from(!controlled_owner);
    let supports = Arc::new([OnceLock::new(), OnceLock::new()]);
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let owner = AcyclicSchedule {
        owner: true,
        parent: keys[1],
        entered: Cell::new(false),
        wait_sent: Cell::new(false),
        ready: ready_tx.clone(),
        release: release_tx.clone(),
        receive: RefCell::new(Some(release_rx)),
        supports: supports.clone(),
        worker: 0,
    };
    let waiter = AcyclicSchedule {
        owner: false,
        parent: keys[1],
        entered: Cell::new(false),
        wait_sent: Cell::new(false),
        ready: ready_tx,
        release: release_tx,
        receive: RefCell::new(Some(ready_rx)),
        supports,
        worker: 1,
    };
    let ordinal = Arc::new(AtomicUsize::new(0));
    let left_ordinal = ordinal.clone();
    let (tx, rx) = mpsc::channel();
    let left_tx = tx.clone();
    let allowance = cutoff.unwrap_or(ALLOWANCE);
    let (left, right) = run_pair(
        &db,
        move |db, participant| {
            acyclic_worker(
                db,
                participant,
                input,
                owner,
                controlled_owner,
                false,
                allowance,
                TraceConfig {
                    worker: 0,
                    ordinal: left_ordinal,
                },
                left_tx,
            )
        },
        move |db, participant| {
            acyclic_worker(
                db,
                participant,
                input,
                waiter,
                !controlled_owner,
                validation,
                ALLOWANCE,
                TraceConfig { worker: 1, ordinal },
                tx,
            )
        },
    )
    .unwrap();
    let mut reports = [
        rx.recv_timeout(STAGE_TIMEOUT).unwrap(),
        rx.recv_timeout(STAGE_TIMEOUT).unwrap(),
    ];
    reports.sort_by_key(|report| report.report.worker);
    for report in &reports {
        eprintln!(
            "MIXED {row} worker {} result {:?} validation {:?}",
            report.report.worker, report.report.driver, report.validation
        );
        assert!(!report.report.trace.broken);
    }
    let [owner_report, waiter_report] = reports;
    let validation_events = waiter_report.validation;
    let reports = [owner_report.report, waiter_report.report];
    let records = merged(&reports);
    for record in &records {
        eprintln!("MIXED {row} {record:?}");
    }
    acyclic_audit(&db, input);
    let outcomes = [left.unwrap(), right.unwrap()];
    let parent_value = if validation { 5 } else { 4 };
    for (worker, outcome) in outcomes.iter().enumerate() {
        assert_eq!(
            outcome,
            &if worker == controlled {
                MixedOutcome::Controlled(Ok(if cutoff.is_some() {
                    AttemptOutcome::Incomplete(Incomplete::Allowance)
                } else {
                    AttemptOutcome::Complete(Ok(if validation { 6 } else { parent_value }))
                }))
            } else {
                MixedOutcome::Ordinary(Ok(parent_value))
            }
        );
    }
    assert_mixed_sessions(&records, &reports, controlled, cutoff.is_some());
    assert_claim_lifetimes(&records);
    let owner_claim = find(&records, "ordinary/controlled owner holds parent", |r| {
        query_event(r, 0, Kind::Claim, keys[1])
    });
    let gate = after(
        &records,
        owner_claim,
        "owner holds parent at reducer gate",
        |r| query_event(r, 0, Kind::Gate, keys[1]),
    );
    let edge = after(&records, gate, "real waiter edge on parent", |r| {
        query_event(r, 1, Kind::Edge, keys[1])
    });
    assert_eq!(edge.event.peer, Some(owner_claim.thread));
    let wake = after(&records, edge, "waiter consumes owner completion", |r| {
        query_event(r, 1, Kind::WaitConsumed, keys[1])
    });
    assert!(matches!(wake.event.wait, Some(WaitResult::Cancelled)) == cutoff.is_some());
    if cutoff.is_none() {
        assert!(matches!(wake.event.wait, Some(WaitResult::Completed)));
    }
    let measured = if controlled_owner {
        // WillBlockOn releases the body gate before registering its edge. The held query
        // lock orders claim release after registration, but does not order this debit.
        let predebit = after(&records, gate, "parent completion debit", |r| {
            query_event(r, 0, Kind::PreDebit, keys[1])
        });
        let spent = allowance - predebit.event.session.unwrap().remaining;
        // The first child read into the fresh parent dependency set costs 105 units.
        assert_eq!(
            spent,
            2 * super::INPUT_CONVERSION_WORK + 1 + 1 + 105,
            "both input conversions, the child body, its publication, and its read precede the parent reducer"
        );
        let child_publication = find(
            &records,
            "independent child completed before parent debit",
            |r| query_event(r, 0, Kind::RootPublished, keys[0]),
        );
        assert!(child_publication.ordinal < predebit.ordinal);
        assert!(child_publication.event.memo.unwrap().final_);
        if cutoff.is_some() {
            assert_eq!(predebit.event.session.unwrap().remaining, 0);
            let refused = after(&records, predebit, "actual parent debit refuses", |r| {
                query_event(r, 0, Kind::DebitRefused, keys[1])
            });
            let abort = after(&records, refused, "parent claim aborts", |r| {
                query_event(r, 0, Kind::Terminal, keys[1]) && r.event.action == Some(Action::Abort)
            });
            assert!(abort.ordinal < wake.ordinal);
            assert!(
                !records
                    .iter()
                    .any(|r| query_event(r, 0, Kind::BodyValue, keys[1])
                        || query_event(r, 0, Kind::RootPublished, keys[1]))
            );
            let retry = after(&records, wake, "ordinary waiter retries parent", |r| {
                query_event(r, 1, Kind::Claim, keys[1])
            });
            after(&records, retry, "ordinary waiter completes parent", |r| {
                query_event(r, 1, Kind::BodyValue, keys[1])
            });
        }
        let final_child = db
            .zalsa()
            .lookup_ingredient(keys[0].ingredient_index())
            .as_function()
            .unwrap()
            .memo(db.zalsa(), input.as_id())
            .unwrap()
            .transfer_test_snapshot();
        assert_eq!(
            final_child.identity,
            child_publication.event.memo.unwrap().identity
        );
        spent
    } else {
        0
    };
    if let Some(old) = old {
        let parent_changed = after(
            &records,
            owner_claim,
            "ordinary parent verification Changed",
            |r| query_event(r, 0, Kind::Verification, keys[1]) && !r.event.decision,
        );
        assert!(parent_changed.ordinal < gate.ordinal);
        assert_eq!(parent_changed.event.memo.unwrap().identity, old[0].identity);
        assert!(old[0].verified_at < db.zalsa().current_revision());
        let consumer_claim = find(&records, "controlled consumer owns validation claim", |r| {
            query_event(r, 1, Kind::Claim, keys[2])
        });
        assert!(consumer_claim.ordinal < edge.ordinal);
        let changed = after(&records, wake, "consumer old memo becomes Changed", |r| {
            query_event(r, 1, Kind::Verification, keys[2])
                && !r.event.decision
                && r.event.serial == consumer_claim.event.serial
        });
        assert_eq!(changed.event.memo.unwrap().identity, old[1].identity);
        let body = after(&records, changed, "Changed precedes consumer body", |r| {
            query_event(r, 1, Kind::BodyValue, keys[2])
        });
        after(
            &records,
            body,
            "consumer claim ends after recomputation",
            |r| {
                query_event(r, 1, Kind::Terminal, keys[2])
                    && r.event.serial == consumer_claim.event.serial
            },
        );
        super::assert_wait_retry(&validation_events, "validation.wait", "probe", keys[1]);
        assert!(!validation_events.iter().any(|event| matches!(event, TraceEvent::Outer { phase: "fetch.wait", key: Some(key), .. } if *key == keys[1])));
    } else if controlled == 1 {
        super::assert_wait_retry(&validation_events, "fetch.wait", "fetch.probe", keys[1]);
    }
    let completed = acyclic_counts(&db);
    assert_eq!(child(&db, input), parent_value - 1);
    assert_eq!(parent(&db, input), parent_value);
    assert_eq!(
        acyclic_counts(&db),
        completed,
        "completed child and parent reads executed bodies"
    );
    let report = acyclic_canonical(&db, input);
    let expected = acyclic_canonical(&baseline, baseline_input);
    eprintln!(
        "MIXED {row} canonical {report:?}, baseline {expected:?}, work {:?}",
        acyclic_counts(&db)
    );
    assert_eq!(report, expected);
    assert_eq!(report.edges, [vec!["limit"], vec!["child"], vec!["parent"]]);
    let counts = acyclic_counts(&db);
    let baseline_counts = acyclic_counts(&baseline);
    assert_eq!(
        std::array::from_fn::<_, 3, _>(|i| counts[i] - before[i]),
        std::array::from_fn::<_, 3, _>(|i| baseline_counts[i] - baseline_before[i])
    );
    if validation {
        assert_eq!(completed, counts);
    }
    assert_eq!(acyclic_canonical(&db, input), report);
    assert_eq!(acyclic_counts(&db), counts);
    measured
}

#[crate::input]
struct PolicyInput {
    #[returns(copy)]
    value: u32,
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn complete_leaf(db: &dyn TransferDatabase, input: PolicyInput) -> u32 {
    assert!(attempt_probe::current().is_none());
    assert_eq!(attempt_probe::remaining_allowance_for_diagnostics(db), None);
    assert!(!attempt_probe::is_incomplete(db));
    input.value(db)
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn complete_outer(db: &dyn TransferDatabase, input: PolicyInput) -> u32 {
    complete_leaf(db, input)
}

#[crate::tracked(returns(copy), attempt = CompleteOnly)]
fn forbidden_outer(db: &dyn TransferDatabase, input: Input) -> u32 {
    child(db, input)
}

#[derive(Debug)]
struct NativeFailure(Arc<()>);

fn worker_failure_priority() {
    for body_failed in [false, true] {
        let body_identity = Arc::new(());
        let cleanup_identity = Arc::new(());
        let result = if body_failed {
            catch_unwind(|| -> u32 { panic_any(NativeFailure(body_identity.clone())) })
        } else {
            Ok(41)
        };
        let cleanup = catch_unwind(|| panic_any(NativeFailure(cleanup_identity.clone())));
        let result = WorkerOutcome { result, cleanup }.into_result();
        let payload = result.expect_err("cleanup failure must not become a successful result");
        let expected = if body_failed {
            &body_identity
        } else {
            &cleanup_identity
        };
        assert!(Arc::ptr_eq(
            &payload.downcast_ref::<NativeFailure>().unwrap().0,
            expected
        ));
    }
}

thread_local! { static PANIC_IDENTITY: RefCell<Option<Arc<()>>> = const { RefCell::new(None) }; }

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn native_failure(_db: &dyn TransferDatabase, _input: Input) -> u32 {
    let identity = PANIC_IDENTITY.with_borrow(|identity| identity.as_ref().unwrap().clone());
    panic_any(NativeFailure(identity));
}

struct PeerFinished(mpsc::SyncSender<()>);
impl Drop for PeerFinished {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}

fn permission_policy(case: usize) {
    let db = Db::default();
    let input = Input::new(&db, 3, 17);
    let policy = PolicyInput::new(&db, 41);
    let identity = Arc::new(());
    let expected_identity = identity.clone();
    let (support_tx, support_rx) = mpsc::sync_channel(1);
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let outsider = db.clone();
    let (left, right) = run_pair(
        &db,
        move |db, participant| {
            let _finished = PeerFinished(done_tx);
            let (result, trace) = trace::collect(
                TraceConfig {
                    worker: 0,
                    ordinal: Arc::new(AtomicUsize::new(0)),
                },
                || {
                    catch_worker(&db, || {
                        participant.run_ordinary(&db, || {
                            let peer: AttemptSupport =
                                support_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
                            assert!(attempt_probe::current().is_none());
                            assert_eq!(
                                attempt_probe::remaining_allowance_for_diagnostics(&db),
                                None
                            );
                            assert!(!attempt_probe::is_incomplete(&db));
                            assert!(peer.local_ownership(db.zalsa()).is_none());
                            assert!(!peer.owns_current_session(db.zalsa()));
                            assert_eq!(
                                attempt_probe::try_with_attempt(&db, 10, || ()),
                                Err(StartError::ActiveOperation)
                            );
                            assert_eq!(
                                attempt_probe::try_with_operation(&db, || complete_outer(
                                    &db, policy
                                )),
                                Ok(41)
                            );
                            assert!(attempt_probe::current().is_none());
                            let other = Db::default();
                            assert!(!peer.owns_current_session(other.zalsa()));
                            match case {
                                0 => complete_outer(&db, policy),
                                1 => forbidden_outer(&db, input),
                                2 => {
                                    PANIC_IDENTITY.with_borrow_mut(|slot| *slot = Some(identity));
                                    native_failure(&db, input)
                                }
                                _ => unreachable!(),
                            }
                        })
                    })
                },
            );
            PANIC_IDENTITY.with_borrow_mut(|slot| *slot = None);
            for record in &trace.records {
                eprintln!("MIXED permission case={case} {record:?}");
            }
            assert!(!trace.broken);
            assert_eq!(
                acyclic_counts(&db),
                [0, 0, 0],
                "forbidden child body executed"
            );
            let mut claims = BTreeMap::new();
            for record in &trace.records {
                if record.event.kind == Kind::Claim {
                    assert!(claims.insert(record.event.serial.unwrap(), 0).is_none());
                }
                if record.event.kind == Kind::Terminal {
                    *claims.get_mut(&record.event.serial.unwrap()).unwrap() += 1;
                }
            }
            assert!(!claims.is_empty() && claims.values().all(|terminals| *terminals == 1));
            result.into_result()
        },
        move |db, participant| {
            let outcome = participant.run(&db, 29, || {
                let own = attempt_probe::current().unwrap();
                support_tx.send(own.clone()).unwrap();
                // This thread enters ordinarily without inheriting the peer's Session.
                thread::scope(|scope| {
                    scope
                        .spawn(move || {
                            assert!(attempt_probe::current().is_none());
                            assert_eq!(attempt_probe::try_with_operation(&outsider, || ()), Ok(()));
                            let independent = PolicyInput::new(&outsider, 43);
                            assert_eq!(complete_outer(&outsider, independent), 43);
                            assert!(!attempt_probe::is_incomplete(&outsider));
                            super::assert_worker_clean(&outsider);
                        })
                        .join()
                        .unwrap()
                });
                done_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
                assert!(attempt_probe::current().unwrap().same_owner(&own));
                assert_eq!(
                    attempt_probe::remaining_allowance_for_diagnostics(&db),
                    Some(29)
                );
            });
            super::assert_worker_clean(&db);
            outcome
        },
    )
    .unwrap();
    assert_eq!(right.unwrap(), Ok(AttemptOutcome::Complete(())));
    let result = left.unwrap();
    match case {
        0 => assert_eq!(result.unwrap(), Ok(41)),
        1 => {
            let payload = result.expect_err("CompleteOnly parent must reject ReturnOnly child");
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap();
            assert!(message.contains("complete-only query requested interruptible semantic work"));
        }
        2 => assert!(Arc::ptr_eq(
            &result
                .unwrap_err()
                .downcast_ref::<NativeFailure>()
                .unwrap()
                .0,
            &expected_identity
        )),
        _ => unreachable!(),
    }
    super::assert_exclusion_released(&db);
    assert_graph_clean(db.zalsa().runtime().test_transfer_graph_snapshot());
    for key in [
        complete_outer::fn_ingredient_(&db, db.zalsa()).database_key_index(policy.as_id()),
        complete_leaf::fn_ingredient_(&db, db.zalsa()).database_key_index(policy.as_id()),
        forbidden_outer::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id()),
        native_failure::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id()),
    ] {
        assert!(
            db.zalsa()
                .lookup_ingredient(key.ingredient_index())
                .as_function()
                .unwrap()
                .sync_table()
                .test_transfer_state(key.key_index())
                .is_none()
        );
    }
}

fn permission_setup() {
    worker_failure_priority();
    for case in 0..4 {
        let db = Db::default();
        let (left, right) = run_pair(
            &db,
            |db, participant| {
                let (result, trace) = trace::collect(
                    TraceConfig {
                        worker: 0,
                        ordinal: Arc::new(AtomicUsize::new(0)),
                    },
                    || {
                        catch_unwind(AssertUnwindSafe(|| match case {
                            0 => {
                                drop(participant);
                                None
                            }
                            1 => Some(participant.run_ordinary(&Db::default(), || ())),
                            2 => {
                                let other = Db::default();
                                Some(
                                    attempt_probe::try_with_operation(&other, || {
                                        participant.run_ordinary(&db, || ())
                                    })
                                    .unwrap(),
                                )
                            }
                            3 => {
                                let other = Db::default();
                                let installed = attempt_probe::try_with_attempt(&other, 3, || {
                                    participant.run_ordinary(&db, || ())
                                })
                                .unwrap();
                                let AttemptOutcome::Complete(result) = installed else {
                                    panic!("outer control refused");
                                };
                                Some(result)
                            }
                            _ => unreachable!(),
                        }))
                    },
                );
                for record in &trace.records {
                    eprintln!("MIXED setup case={case} {record:?}");
                }
                assert!(!trace.broken);
                super::assert_worker_clean(&db);
                result
            },
            |db, participant| {
                let result = participant
                    .run_ordinary(&db, || -> () { panic!("aborted setup ran ordinary body") });
                super::assert_worker_clean(&db);
                result
            },
        )
        .unwrap();
        let result = left.unwrap();
        match case {
            0 => assert_eq!(result.unwrap(), None),
            1 => assert!(result.is_err(), "foreign participant database was admitted"),
            2 => assert_eq!(
                result.unwrap(),
                Some(Err(PairInstallError::Start(StartError::ActiveOperation)))
            ),
            3 => assert_eq!(
                result.unwrap(),
                Some(Err(PairInstallError::Start(StartError::NestedAttempt)))
            ),
            _ => unreachable!(),
        }
        assert_eq!(right.unwrap(), Err(PairInstallError::SetupAborted));
        super::assert_exclusion_released(&db);
    }
    for case in 0..3 {
        permission_policy(case);
    }
}

#[test]
fn mixed_ordinary_and_controlled_ownership() {
    if std::env::var(CHILD_MARKER).as_deref() == Ok(TEST_NAME) {
        permission_setup();
        acyclic_case(false, false, None);
        let parent_cutoff = acyclic_case(true, false, None);
        acyclic_case(true, false, Some(parent_cutoff));
        acyclic_case(false, true, None);
        for uses_last in [false, true] {
            let baseline = ordinary(uses_last);
            let calibration = mixed_cycle(&baseline, 0, None).unwrap();
            mixed_cycle(&baseline, 1, None);
            mixed_cycle(&baseline, 0, Some(&calibration));
        }
        return;
    }
    let executable = std::env::current_exe().expect("saved current test binary");
    let mut child = Command::new(executable)
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(CHILD_MARKER, TEST_NAME)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn mixed ownership fixture");
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "mixed ownership fixture failed: {status}");
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let status = child.wait().unwrap();
            panic!(
                "mixed ownership fixture exceeded {PROCESS_TIMEOUT:?}; trace retained above ({status})"
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}
