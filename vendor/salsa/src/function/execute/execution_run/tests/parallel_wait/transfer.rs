use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any, resume_unwind};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::super::super::registration::{
    ExecutableRouteProvider, ProviderContext, RegistryBuilder, Route,
};
use super::super::super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use super::native_cancellation::{
    NativeOutcome, Release, WriterLease, after_native_checks, assert_native, emit_native,
    is_native_check,
};
use crate::attempt_probe::paired_test_support::{PairInstallError, Participant, run_pair};
use crate::attempt_probe::transfer_test_support::{
    self as trace, Action, Event as Observation, GraphSnapshot, Kind, Mode, Record, Step,
    TraceConfig, TransferTrace,
};
use crate::attempt_probe::{self, AttemptOutcome, AttemptSupport, Incomplete, MemoReuse};
use crate::function::{Configuration, IngredientImpl, SyncOwner};
use crate::plumbing::AsId;
use crate::runtime::WaitResult;
use crate::zalsa::ZalsaDatabase;
use crate::zalsa_local::QueryEdgeKind;
use crate::{
    CancellationToken, Cycle, Database, DatabaseKeyIndex, Durability, Event, EventKind, Id, Setter,
};

const ALLOWANCE: usize = 10_000;
const STAGE_TIMEOUT: Duration = Duration::from_secs(5);
const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const CHILD_MARKER: &str = "SALSA_TWO_WORKER_TRANSFER_CHILD";
const TEST_NAME: &str = "function::execute::execution_run::tests::parallel_wait::transfer::transferred_provisional_restart_and_receiver_allowance";

mod native_release;

type Supports = Arc<[OnceLock<AttemptSupport>; 2]>;
type PairedOutcome = Result<AttemptOutcome<RunResult<u32>>, PairInstallError>;

#[derive(Default)]
struct Counts([AtomicUsize; 6]);
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
    native_reclaim_pause(db, Keys::new(db, input).0[1], Step::Body);
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
    _last: &AValue,
    value: AValue,
    input: Input,
) -> AValue {
    semantic_step(db, 0, Step::Recovery);
    value_observation(db, input, 0, Step::Recovery, value.0);
    value
}
fn recover_b(
    db: &dyn TransferDatabase,
    _cycle: &Cycle<'_>,
    _last: &BValue,
    value: BValue,
    input: Input,
) -> BValue {
    semantic_step(db, 1, Step::Recovery);
    value_observation(db, input, 1, Step::Recovery, value.0);
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
    send_wait: mpsc::Sender<Gate>,
    receive_wait: Option<mpsc::Receiver<Gate>>,
}
fn schedules() -> [ScheduleData; 2] {
    let (a_tx, a_rx) = mpsc::channel();
    let (b_tx, b_rx) = mpsc::channel();
    let (wait_tx, wait_rx) = mpsc::channel();
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
        self.authority(db, true);
        Ok(())
    }
    fn authority(&self, db: &dyn TransferDatabase, require_peer: bool) {
        let Some(own) = attempt_probe::current() else {
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
        if let Some(peer) = self.supports[1 - self.data.worker].get() {
            assert!(!own.same_owner(peer));
            assert!(!peer.owns_current_session(db.zalsa()));
            assert!(!peer.is_current(db.zalsa()));
            assert!(peer.local_ownership(db.zalsa()).is_none());
            event.previous_support = Some(trace::support_snapshot(peer));
        } else {
            assert!(!require_peer, "peer did not install its own support");
        }
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
            let _ = self.0.data.send_wait.send(Gate::Abort);
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
    native_interruption_event(&event);
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
            let _ = schedule.data.send_wait.send(Gate::Ready);
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
                self.schedule.authority(db, false);
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
                self.schedule.authority(db, true);
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
                self.schedule.authority(db, true);
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
    native_reclaim_pause(db, key, step);
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
        catch_unwind(AssertUnwindSafe(|| {
            let schedule = ScheduleOwner::install(data, Keys::new(&db, input), supports.clone());
            let outcome = participant.run(&db, allowance, || {
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
            });
            super::assert_worker_clean(&db);
            outcome
        }))
    });
    if outcome.is_err() {
        for record in &trace.records {
            eprintln!("TRANSFER native failure {record:?}");
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
    match outcome {
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
        catch_unwind(AssertUnwindSafe(|| {
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
            super::assert_worker_clean(&db);
            value
        }))
    });
    if outcome.is_err() {
        for record in &trace.records {
            eprintln!("TRANSFER native failure {record:?}");
        }
    }
    reports
        .send(WorkerReport {
            worker,
            trace,
            driver: outcome.as_ref().ok().map(|value| Ok(*value)),
            support: None,
        })
        .unwrap();
    match outcome {
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
        assert!(!support.explicitly_incomplete);
    }
    assert_eq!(memo.header.attempt_reuse(db.zalsa()), MemoReuse::Ordinary);
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
    eprintln!("TRANSFER {phase} final input paths {paths:?}; raw {report:?}");
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
    eprintln!("TRANSFER post-join graph {graph:?}");
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
                "TRANSFER detached marker {:?} {key:?}: {state:?}",
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

#[derive(Debug)]
struct Baseline {
    initial: Canonical,
    edited: Canonical,
    edited_work: [usize; 6],
    detached: [bool; 2],
    body_work: usize,
}
fn followup(db: &mut Db, input: Input) -> (Canonical, Canonical, [usize; 6]) {
    let snapshots = |db: &Db| {
        Keys::new(db, input).0.map(|key| {
            db.zalsa()
                .lookup_ingredient(key.ingredient_index())
                .as_function()
                .unwrap()
                .memo(db.zalsa(), key.key_index())
                .unwrap()
                .transfer_test_snapshot()
        })
    };
    let retained = snapshots(db);
    let before = db.counts.snapshot();
    let (initial, observations) = trace::collect(
        TraceConfig {
            worker: 2,
            ordinal: Arc::new(AtomicUsize::new(0)),
        },
        || canonical(db, input),
    );
    assert!(!observations.broken);
    for (old, final_memo) in retained.into_iter().zip(snapshots(db)) {
        assert_eq!(final_memo.identity, old.identity);
        assert_eq!(final_memo.execution_revision, old.execution_revision);
        assert_eq!(final_memo.iteration, old.iteration);
        assert_eq!(final_memo.heads.entries, old.heads.entries);
        assert!(final_memo.final_);
        if !old.final_ {
            let proof = observations
                .records
                .iter()
                .find(|record| {
                    record.event.phase == Some("finality.head")
                        && record
                            .event
                            .memo
                            .is_some_and(|memo| memo.identity == old.identity)
                })
                .expect("retained participant proved its completed head");
            assert!(proof.event.decision);
            assert!(proof.event.other_memo.unwrap().final_);
            let published = observations
                .records
                .iter()
                .find(|record| {
                    record.event.phase == Some("finality.published")
                        && record
                            .event
                            .memo
                            .is_some_and(|memo| memo.identity == old.identity)
                })
                .expect("proof published the retained participant");
            assert!(published.ordinal > proof.ordinal);
        }
    }
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
            "TRANSFER {row} worker {} driver {:?} support {:?}",
            report.worker,
            report.driver,
            report.support.as_ref().map(trace::support_snapshot)
        );
    }
    // Print the complete merged trace before checking outcomes, so a native panic preserves
    // the selected identities and the decision immediately preceding its unwind.
    for record in merged(&reports) {
        eprintln!("TRANSFER {row} {record:?}");
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

fn assert_donor_restart(records: &[Record], keys: Keys, handoff: &Handoff<'_>, owners: [usize; 2]) {
    let a = keys.0[0];
    let b = keys.0[1];
    let probe = donor_refetch(records, keys, handoff);
    let stale = after(
        records,
        probe,
        "donor rejects receiver provisional B",
        |r| r.worker == 1 && r.event.kind == Kind::Reuse && r.event.reuse == Some(MemoReuse::Stale),
    );
    let provisional = find(records, "receiver published the donor's rejected B", |r| {
        query_event(r, 0, Kind::RootPublished, b)
            && r.event
                .memo
                .is_some_and(|memo| !memo.final_ && memo.identity == stale.event.identity)
    });
    assert!(provisional.ordinal < stale.ordinal);
    let foreign = provisional.event.memo.unwrap();
    assert_eq!(foreign.support.unwrap().owner, owners[0]);
    assert_eq!(stale.event.support.unwrap().owner, owners[0]);
    assert_eq!(stale.event.support.unwrap().state, 0);
    assert_eq!(stale.event.session.unwrap().support.owner, owners[1]);

    let claim = after(
        records,
        stale,
        "donor claims B after mandatory refetch",
        |r| query_event(r, 1, Kind::Claim, b),
    );
    assert_ne!(claim.event.serial, handoff.donor_claim.event.serial);
    let terminal = after(records, claim, "donor's new B claim terminates", |r| {
        query_event(r, 1, Kind::Terminal, b) && r.event.serial == claim.event.serial
    });
    let verified = after(records, claim, "donor B verifier returns Changed", |r| {
        query_event(r, 1, Kind::Verification, b)
            && r.event.serial == claim.event.serial
            && !r.event.decision
    });
    assert_eq!(verified.event.memo.unwrap().identity, foreign.identity);
    let execute = after(records, verified, "donor Changed enters execution", |r| {
        query_event(r, 1, Kind::Verified, b)
            && r.event.serial == claim.event.serial
            && !r.event.decision
    });
    assert_eq!(execute.event.memo.unwrap().identity, foreign.identity);
    let supplied = after(records, execute, "donor receives foreign B baseline", |r| {
        query_event(r, 1, Kind::PrepareSupplied, b) && r.event.serial == claim.event.serial
    });
    assert_eq!(supplied.event.memo.unwrap().identity, foreign.identity);
    let declined = after(
        records,
        supplied,
        "donor cannot seed from receiver B",
        |r| {
            r.worker == 1
                && r.event.kind == Kind::SeedAllowed
                && r.event.identity == foreign.identity
                && !r.event.decision
        },
    );
    let retained = after(
        records,
        declined,
        "donor removes receiver B baseline",
        |r| query_event(r, 1, Kind::PrepareRetained, b) && r.event.serial == claim.event.serial,
    );
    assert!(retained.event.memo.is_none());
    let iteration = after(records, retained, "donor B starts without a seed", |r| {
        query_event(r, 1, Kind::Iteration, b) && r.event.serial == claim.event.serial
    });
    assert!(iteration.event.memo.is_none() && iteration.event.other_memo.is_none());
    let debit = after(records, iteration, "donor admits a fresh B body", |r| {
        query_event(r, 1, Kind::PreDebit, b) && r.event.step == Some(Step::Body)
    });
    let accepted = after(records, debit, "donor B body debit succeeds", |r| {
        query_event(r, 1, Kind::DebitAccepted, b) && r.event.step == Some(Step::Body)
    });
    assert_eq!(debit.event.units, 1);
    assert_eq!(accepted.event.units, 1);
    assert_eq!(
        accepted.event.session.unwrap().remaining + 1,
        debit.event.session.unwrap().remaining
    );
    let request = after(records, accepted, "donor B requests final A", |r| {
        query_event(r, 1, Kind::ChildRequest, b) && r.event.other_key == Some(a)
    });

    let a_result = find(records, "receiver returns final A", |r| {
        query_event(r, 0, Kind::RootResult, a)
    });
    let a_memo = a_result.event.memo.unwrap();
    assert_eq!(a_result.event.value, 3);
    let final_a = find(records, "receiver published the selected final A", |r| {
        query_event(r, 0, Kind::RootPublished, a)
            && r.event
                .memo
                .is_some_and(|memo| memo.final_ && memo.identity == a_memo.identity)
    });
    let published_a = final_a.event.memo.unwrap();
    assert!(final_a.ordinal < request.ordinal);
    assert!(published_a.has_value && published_a.final_);
    assert!(!published_a.heads.overflow && published_a.heads.is_empty());
    assert_eq!(
        published_a.verified_at,
        request.event.session.unwrap().support.revision
    );
    let reused_a = after(records, request, "donor reads that A as Ordinary", |r| {
        r.worker == 1
            && r.event.kind == Kind::Reuse
            && r.event.identity == published_a.identity
            && r.event.reuse == Some(MemoReuse::Ordinary)
    });
    let body = after(records, reused_a, "donor B finishes from final A", |r| {
        query_event(r, 1, Kind::BodyValue, b)
    });
    assert_eq!(body.event.value, 3);
    assert!(
        !records.iter().any(|r| r.ordinal > final_a.ordinal
            && r.ordinal < body.ordinal
            && r.event.key == Some(a)
            && matches!(r.event.kind, Kind::RootPublished | Kind::TargetPublished)),
        "selected final A was replaced before donor B completed"
    );
    assert!(
        !records.iter().any(|r| r.worker == 1
            && r.ordinal > debit.ordinal
            && r.ordinal < body.ordinal
            && (matches!(
                r.event.kind,
                Kind::MarkerRead | Kind::InitialValue | Kind::RecoveryValue
            ) || (r.event.kind == Kind::PreDebit
                && matches!(r.event.step, Some(Step::Initial | Step::Recovery))))),
        "donor's acyclic B body used a marker or cycle callback"
    );
    let current = after(
        records,
        body,
        "donor final B passes publication check",
        |r| query_event(r, 1, Kind::CommitCurrent, b) && r.event.serial == claim.event.serial,
    );
    assert!(current.event.decision);
    let published = after(records, current, "donor publishes fresh acyclic B", |r| {
        query_event(r, 1, Kind::RootPublished, b) && r.event.serial == claim.event.serial
    });
    let memo = published.event.memo.unwrap();
    assert_ne!(memo.identity, foreign.identity);
    assert!(memo.has_value && memo.final_);
    assert!(!memo.heads.overflow && memo.heads.is_empty());
    assert!(memo.iteration.is_default() && !memo.converged);
    assert!(memo.support.is_none());
    assert_eq!(
        memo.verified_at,
        published.event.session.unwrap().support.revision
    );
    assert_eq!(memo.changed_at, memo.verified_at);

    for record in [
        verified, execute, supplied, declined, retained, iteration, debit, accepted, request,
        reused_a, body, current, published,
    ] {
        assert!(
            claim.ordinal < record.ordinal && record.ordinal < terminal.ordinal,
            "donor restart record lies outside its claim lifetime: {record:?}"
        );
        assert_eq!(record.event.session.unwrap().support.owner, owners[1]);
        if let Some(serial) = record.event.serial {
            assert_eq!(Some(serial), claim.event.serial);
        }
    }
    let b_result = after(records, terminal, "donor returns its fresh final B", |r| {
        query_event(r, 1, Kind::RootResult, b)
    });
    assert_eq!(b_result.event.value, 3);
    assert_eq!(b_result.event.memo.unwrap().identity, memo.identity);
}

fn ordinary() -> Baseline {
    eprintln!("TRANSFER ordinary baseline starts");
    let mut db = Db::default();
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
    let detached = audit(&db, input);
    let body_work = db.counts.snapshot()[0] + db.counts.snapshot()[3];
    let (initial, edited, edited_work) = followup(&mut db, input);
    eprintln!(
        "TRANSFER ordinary origins {initial:?}, edited {edited:?}, edited work {edited_work:?}"
    );
    Baseline {
        initial,
        edited,
        edited_work,
        detached,
        body_work,
    }
}

struct Restart<'a> {
    claim: &'a Record,
    debit: &'a Record,
}
fn receiver_restart<'a>(
    records: &'a [Record],
    keys: Keys,
    handoff: &Handoff<'_>,
    owners: [usize; 2],
) -> Restart<'a> {
    let b = keys.0[1];
    let foreign = handoff.provisional.event.memo.unwrap();
    assert_eq!(foreign.support.unwrap().owner, owners[1]);
    assert_eq!(foreign.support.unwrap().state, 0);
    assert_eq!(
        handoff.initial.event.memo.unwrap().support.unwrap().owner,
        owners[1]
    );
    let stale = after(
        records,
        handoff.mapping,
        "receiver hot probe rejects live donor provisional",
        |r| {
            r.worker == 0
                && r.event.kind == Kind::Reuse
                && r.event.identity == foreign.identity
                && r.event.reuse == Some(MemoReuse::Stale)
        },
    );
    assert_eq!(stale.event.support.unwrap().state, 0);
    assert_eq!(stale.event.session.unwrap().support.owner, owners[0]);
    let claim = after(records, stale, "receiver reclaims transferred B", |r| {
        query_event(r, 0, Kind::Claim, b) && r.event.mode == Some(Mode::SelfOnly)
    });
    let sync = claim.event.sync.unwrap();
    assert!(sync.claimed_twice);
    assert!(matches!(sync.owner, SyncOwner::Thread(id) if id == claim.thread));
    assert_ne!(claim.event.serial, handoff.donor_claim.event.serial);
    let verified = after(records, claim, "claimed B verifier returns Changed", |r| {
        query_event(r, 0, Kind::Verification, b)
            && r.event.serial == claim.event.serial
            && !r.event.decision
    });
    assert_eq!(verified.event.memo.unwrap().identity, foreign.identity);
    let execute = after(records, verified, "Changed enters execution", |r| {
        query_event(r, 0, Kind::Verified, b)
            && r.event.serial == claim.event.serial
            && !r.event.decision
    });
    let supplied = after(
        records,
        execute,
        "foreign baseline supplied to prepare_start",
        |r| query_event(r, 0, Kind::PrepareSupplied, b) && r.event.serial == claim.event.serial,
    );
    assert_eq!(supplied.event.memo.unwrap().identity, foreign.identity);
    let declined = after(records, supplied, "foreign B cannot seed execution", |r| {
        r.worker == 0
            && r.event.kind == Kind::SeedAllowed
            && r.event.identity == foreign.identity
            && !r.event.decision
    });
    let retained = after(records, declined, "foreign B baseline removed", |r| {
        query_event(r, 0, Kind::PrepareRetained, b) && r.event.serial == claim.event.serial
    });
    assert!(retained.event.memo.is_none());
    let default = after(records, retained, "reentrant B mode becomes Default", |r| {
        query_event(r, 0, Kind::Mode, b)
            && r.event.serial == claim.event.serial
            && r.event.mode == Some(Mode::Default)
    });
    let iteration = after(
        records,
        default,
        "B starts with no foreign iteration seed",
        |r| query_event(r, 0, Kind::Iteration, b) && r.event.serial == claim.event.serial,
    );
    assert!(iteration.event.memo.is_none() && iteration.event.other_memo.is_none());
    assert!(
        !records.iter().any(|r| r.worker == 0
            && r.ordinal > retained.ordinal
            && r.ordinal < iteration.ordinal
            && matches!(r.event.kind, Kind::Previous | Kind::SeedActive)
            && r.event.identity == foreign.identity),
        "filtered foreign baseline was consulted again"
    );
    let debit = after(
        records,
        iteration,
        "first receiver semantic B debit after reclaim",
        |r| query_event(r, 0, Kind::PreDebit, b) && r.event.step == Some(Step::Body),
    );
    let actual_claim = records
        .iter()
        .filter(|r| r.worker == 0 && r.ordinal < debit.ordinal && query_event(r, 0, Kind::Claim, b))
        .last()
        .unwrap();
    assert_eq!(actual_claim.event.serial, claim.event.serial);
    assert_eq!(debit.event.session.unwrap().support.owner, owners[0]);
    assert!(debit.event.sync.unwrap().claimed_twice);
    assert_eq!(debit.event.units, 1);
    Restart { claim, debit }
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

fn assert_sessions(records: &[Record], reports: &[WorkerReport; 2], refusal: bool) -> [usize; 2] {
    let supports = reports
        .each_ref()
        .map(|report| report.support.as_ref().expect("real retained support"));
    assert!(!supports[0].same_owner(supports[1]));
    let snapshots = supports.map(trace::support_snapshot);
    assert_eq!(snapshots[0].database, snapshots[1].database);
    assert_eq!(snapshots[0].revision, snapshots[1].revision);
    assert_eq!(snapshots[0].cancellation, snapshots[1].cancellation);
    assert_eq!(snapshots[0].state, if refusal { 1 } else { 2 });
    assert_eq!(snapshots[1].state, 2);
    for worker in 0..2 {
        for record in records.iter().filter(|record| record.worker == worker) {
            if let Some(session) = record.event.session {
                assert_eq!(
                    session.support.owner, snapshots[worker].owner,
                    "worker changed attempt token: {record:?}"
                );
                assert_eq!(session.support.database, snapshots[worker].database);
                assert_eq!(session.support.revision, snapshots[worker].revision);
                assert_eq!(session.support.cancellation, snapshots[worker].cancellation);
            }
        }
        let authority = find(
            records,
            "independent local receipt and foreign receipt refusal",
            |record| {
                record.worker == worker
                    && record.event.kind == Kind::Authority
                    && record.event.support.is_some_and(|s| s.state == 0)
                    && record.event.previous_support.is_some_and(|s| s.state == 0)
            },
        );
        assert!(authority.event.decision);
        assert_eq!(
            authority.event.support.unwrap().owner,
            snapshots[worker].owner
        );
        assert_eq!(
            authority.event.previous_support.unwrap().owner,
            snapshots[1 - worker].owner
        );
    }
    snapshots.map(|support| support.owner)
}

fn assert_success(
    records: &[Record],
    keys: Keys,
    handoff: &Handoff<'_>,
    restart: &Restart<'_>,
    owners: [usize; 2],
) {
    let a = keys.0[0];
    let b = keys.0[1];
    let accepted = after(records, restart.debit, "receiver B step admitted", |r| {
        query_event(r, 0, Kind::DebitAccepted, b) && r.event.step == Some(Step::Body)
    });
    assert_eq!(
        accepted.event.session.unwrap().remaining + 1,
        restart.debit.event.session.unwrap().remaining
    );
    let request = after(
        records,
        accepted,
        "receiver B requests actual A route",
        |r| query_event(r, 0, Kind::ChildRequest, b) && r.event.other_key == Some(a),
    );
    let foreign_initial = handoff.initial.event.memo.unwrap();
    let seed = after(
        records,
        request,
        "receiver declines donor A initial seed",
        |r| {
            r.worker == 0
                && r.event.kind == Kind::SeedAllowed
                && r.event.identity == foreign_initial.identity
                && !r.event.decision
        },
    );
    let initial = after(
        records,
        seed,
        "receiver invokes genuine A zero initializer",
        |r| query_event(r, 0, Kind::InitialValue, a) && r.event.value == 0,
    );
    let installed = after(records, initial, "receiver inserts own A initial", |r| {
        query_event(r, 0, Kind::InitialInserted, a)
    });
    assert_eq!(
        installed.event.memo.unwrap().support.unwrap().owner,
        owners[0]
    );
    assert_ne!(
        installed.event.memo.unwrap().identity,
        foreign_initial.identity
    );
    let marker = after(
        records,
        installed,
        "receiver repeats the real zero-dependent marker read",
        |r| query_event(r, 0, Kind::MarkerRead, b) && r.event.value == 17,
    );
    let body = after(
        records,
        marker,
        "receiver executes B from own initial",
        |r| query_event(r, 0, Kind::BodyValue, b) && r.event.value == 1,
    );
    assert_eq!(body.event.session.unwrap().support.owner, owners[0]);
    assert!(
        !records
            .iter()
            .any(|record| record.event.kind == Kind::DebitRefused)
    );
    assert!(
        records
            .iter()
            .filter(|r| r.event.kind == Kind::RootResult)
            .all(|r| r.event.value == 3 && r.event.memo.unwrap().final_)
    );
    donor_refetch(records, keys, handoff);
}
fn assert_refusal(
    records: &[Record],
    keys: Keys,
    handoff: &Handoff<'_>,
    restart: &Restart<'_>,
    owners: [usize; 2],
) {
    assert_refusal_progress(records, keys, handoff, restart, owners, DonorProgress::Body);
}

enum DonorProgress<'a> {
    Body,
    CheckedRetirement(&'a Record),
}

fn assert_refusal_progress(
    records: &[Record],
    keys: Keys,
    handoff: &Handoff<'_>,
    restart: &Restart<'_>,
    owners: [usize; 2],
    progress: DonorProgress<'_>,
) {
    let b = keys.0[1];
    assert_eq!(restart.debit.event.session.unwrap().remaining, 0);
    let refused = after(
        records,
        restart.debit,
        "actual first receiver Body(B) allowance refusal",
        |r| query_event(r, 0, Kind::DebitRefused, b) && r.event.step == Some(Step::Body),
    );
    assert_eq!(refused.event.session.unwrap().support.state, 1);
    assert_eq!(
        records
            .iter()
            .find(|r| r.event.kind == Kind::DebitRefused)
            .unwrap()
            .ordinal,
        refused.ordinal
    );
    assert!(
        !records.iter().any(|r| r.worker == 0
            && r.ordinal > restart.debit.ordinal
            && (r.event.kind == Kind::DebitAccepted || r.event.kind == Kind::BodyValue)),
        "refused receiver performed semantic work"
    );
    let b_abort = after(
        records,
        refused,
        "deepest transferred B guard aborts",
        |r| {
            query_event(r, 0, Kind::Terminal, b)
                && r.event.serial == restart.claim.event.serial
                && r.event.action == Some(Action::Abort)
        },
    );
    assert_eq!(b_abort.event.session.unwrap().support.state, 1);
    assert!(b_abort.event.sync.unwrap().claimed_twice);
    let undo = after(
        records,
        b_abort,
        "actual transferred B ownership is undone",
        |r| query_event(r, 0, Kind::Undo, b),
    );
    let a_abort = after(records, undo, "ancestor A aborts after B", |r| {
        query_event(r, 0, Kind::Terminal, keys.0[0]) && r.event.action == Some(Action::Abort)
    });
    assert_eq!(a_abort.event.session.unwrap().support.state, 1);
    let wake = after(records, a_abort, "A abort releases blocked donor", |r| {
        r.worker == 0
            && r.event.kind == Kind::Unblock
            && matches!(r.event.wait, Some(WaitResult::Cancelled))
            && r.event.from == Some(handoff.donor_claim.thread)
    });
    let resumed = after(records, wake, "donor resumes in original session", |r| {
        query_event(r, 1, Kind::TransferWaitEnd, b)
    });
    assert!(matches!(resumed.event.wait, Some(WaitResult::Cancelled)));
    assert_eq!(resumed.event.session.unwrap().support.owner, owners[1]);
    assert_eq!(resumed.event.session.unwrap().support.state, 0);
    assert_eq!(resumed.event.session, handoff.wait.event.session);
    match progress {
        DonorProgress::Body => {
            after(
                records,
                resumed,
                "surviving donor makes same-session progress",
                |r| {
                    r.worker == 1
                        && r.event.kind == Kind::DebitAccepted
                        && r.event.session.is_some_and(|s| s.support.owner == owners[1])
                },
            );
        }
        DonorProgress::CheckedRetirement(claim) => {
            assert!(claim.ordinal > resumed.ordinal);
            assert!(query_event(claim, 1, Kind::Claim, b));
            assert_eq!(claim.event.mode, Some(Mode::Default));
            let transfer = after(records, claim, "cached donor transfers its new claim", |r| {
                query_event(r, 1, Kind::TransferBegin, b) && r.event.serial == claim.event.serial
            });
            assert_eq!(transfer.event.mode, Some(Mode::TransferTo(keys.0[0])));
            let refetch = after(records, transfer, "cached donor must refetch after transfer", |r| {
                query_event(r, 1, Kind::Refetch, b) && r.event.serial == claim.event.serial
            });
            assert!(refetch.event.decision);
            let result = after(records, refetch, "cached donor returns the final fixed point", |r| {
                query_event(r, 1, Kind::RootResult, b)
            });
            assert_eq!(result.event.value, 3);
            assert!(result.event.memo.unwrap().final_);
            for record in [claim, transfer, refetch, result] {
                let session = record.event.session.unwrap();
                assert_eq!(session.support.owner, owners[1]);
                assert_eq!(session.support.state, 0);
            }
        }
    }
    for record in records
        .iter()
        .filter(|r| r.worker == 1 && r.ordinal > resumed.ordinal)
    {
        if matches!(record.event.kind, Kind::SeedAllowed | Kind::SeedActive)
            && record.event.decision
            && let Some(support) = record.event.support
        {
            assert_ne!(
                support.owner, owners[0],
                "donor seeded from failed receiver: {record:?}"
            );
        }
        if record.event.kind == Kind::SupportAccepted {
            assert_ne!(
                record.event.support.unwrap().owner,
                owners[0],
                "donor imported failed receiver support"
            );
        }
    }
    donor_refetch(records, keys, handoff);
}

#[derive(Debug)]
struct Calibration {
    allowance: usize,
    prefix: Vec<Debit>,
}
fn paired(baseline: &Baseline, calibration: Option<&Calibration>) -> Calibration {
    let refusal = calibration.is_some();
    let row = if refusal {
        "receiver-refusal"
    } else {
        "success"
    };
    eprintln!("TRANSFER {row} starts");
    let mut db = Db::default();
    let input = Input::new(&db, 3, 17);
    let keys = Keys::new(&db, input);
    let supports = Arc::new([OnceLock::new(), OnceLock::new()]);
    let [left, right] = schedules();
    let ordinal = Arc::new(AtomicUsize::new(0));
    let receiver_allowance = calibration.map_or(ALLOWANCE, |calibration| calibration.allowance);
    let (tx, rx) = mpsc::channel();
    let left_supports = supports.clone();
    let right_supports = supports.clone();
    let left_ordinal = ordinal.clone();
    let left_tx = tx.clone();
    let (left, right) = run_pair(
        &db,
        move |db, participant| {
            paired_worker(
                db,
                participant,
                input,
                left,
                left_supports,
                receiver_allowance,
                TraceConfig {
                    worker: 0,
                    ordinal: left_ordinal,
                },
                left_tx,
            )
        },
        move |db, participant| {
            paired_worker(
                db,
                participant,
                input,
                right,
                right_supports,
                ALLOWANCE,
                TraceConfig { worker: 1, ordinal },
                tx,
            )
        },
    )
    .unwrap();
    let reports = reports(rx, row);
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(right, Ok(AttemptOutcome::Complete(Ok(3))));
    assert_eq!(reports[1].driver, Some(Ok(3)));
    if refusal {
        assert_eq!(left, Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)));
        assert_eq!(
            reports[0].driver,
            Some(Err(RunError::Refused(Incomplete::Allowance)))
        );
    } else {
        assert_eq!(left, Ok(AttemptOutcome::Complete(Ok(3))));
        assert_eq!(reports[0].driver, Some(Ok(3)));
    }
    let records = merged(&reports);
    let owners = assert_sessions(&records, &reports, refusal);
    assert_claim_lifetimes(&records);
    let handoff = handoff(&records, keys);
    let restart = receiver_restart(&records, keys, &handoff, owners);
    let prefix = debit_prefix(&db, input, &records, restart.debit, receiver_allowance);
    let spent = receiver_allowance - restart.debit.event.session.unwrap().remaining;
    if let Some(calibration) = calibration {
        assert_eq!(
            prefix, calibration.prefix,
            "refusal did not reproduce the successful actual debit prefix"
        );
        assert_eq!(spent, calibration.allowance);
        assert_refusal(&records, keys, &handoff, &restart, owners);
    } else {
        assert_success(&records, keys, &handoff, &restart, owners);
        assert!(spent > 0 && spent < ALLOWANCE);
        assert_eq!(prefix.last().unwrap().spent, spent);
        assert!(
            prefix.iter().any(|debit| debit.step.is_none()),
            "calibration did not include canonical wait retry"
        );
        let counts = db.counts.snapshot();
        assert!(
            counts[0] + counts[3] > baseline.body_work,
            "receiver restart did not expose extra real body work"
        );
    }
    // Audit before completed reads can lazily remove a detached Transferred table marker.
    let detached = audit(&db, input);
    eprintln!(
        "TRANSFER {row} detached markers {detached:?}; ordinary {:?}",
        baseline.detached
    );
    let (initial, edited, edited_work) = followup(&mut db, input);
    eprintln!(
        "TRANSFER {row} followup initial {initial:?}, edited {edited:?}, edited work {edited_work:?}; ordinary initial {:?}, edited {:?}, edited work {:?}; exact comparisons initial={}, edited={}, work={}",
        baseline.initial,
        baseline.edited,
        baseline.edited_work,
        initial == baseline.initial,
        edited == baseline.edited,
        edited_work == baseline.edited_work,
    );
    assert_eq!(initial.values, baseline.initial.values);
    if initial != baseline.initial && !refusal {
        // A fresh donor B can read final A after rejecting the receiver's provisional B.
        // Its direct A dependency preserves the marker through A's completed origin.
        assert_eq!(
            baseline.initial.origins,
            [
                vec![Label::Marker, Label::Limit],
                vec![Label::Marker, Label::Limit]
            ],
            "alternate donor graph requires the observed ordinary baseline"
        );
        assert_eq!(
            initial.origins,
            [
                vec![Label::Marker, Label::Limit],
                vec![Label::A, Label::Limit]
            ],
            "unclassified completed canonical origin difference"
        );
        assert_donor_restart(&records, keys, &handoff, owners);
    } else {
        assert_eq!(
            initial, baseline.initial,
            "completed canonical origins differ from ordinary execution"
        );
    }
    assert_eq!(
        edited, baseline.edited,
        "marker edit produced different canonical origins"
    );
    assert_eq!(
        edited_work, baseline.edited_work,
        "marker invalidation/reexecution differs from ordinary execution"
    );
    eprintln!("TRANSFER {row} passed; receiver actual allowance prefix {spent}: {prefix:?}");
    Calibration {
        allowance: spent,
        prefix,
    }
}

#[test]
fn transferred_provisional_restart_and_receiver_allowance() {
    if std::env::var(CHILD_MARKER).as_deref() == Ok(TEST_NAME) {
        let baseline = ordinary();
        let calibration = paired(&baseline, None);
        paired(&baseline, Some(&calibration));
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
        .expect("spawn transfer experiment child");
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().expect("poll transfer child") {
            assert!(
                status.success(),
                "transfer experiment child failed: {status}"
            );
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let status = child.wait().expect("reap timed-out transfer child");
            panic!(
                "transfer child exceeded {PROCESS_TIMEOUT:?}; retained trace is above ({status})"
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

const NATIVE_TEST: &str = "function::execute::execution_run::tests::parallel_wait::transfer::native_interruption_after_transferred_reclaim";
const NATIVE_CHILD: &str = "SALSA_TRANSFERRED_NATIVE_INTERRUPTION_CHILD";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeRow {
    Calibration,
    ReceiverLocal,
    ReceiverLocalWaiter,
    DonorLocal,
    Writer,
    Refusal,
}

impl NativeRow {
    fn overlap(self) -> bool {
        matches!(
            self,
            Self::Calibration | Self::ReceiverLocalWaiter | Self::Writer | Self::Refusal
        )
    }

    fn receiver_local(self) -> bool {
        matches!(self, Self::ReceiverLocal | Self::ReceiverLocalWaiter)
    }
}

#[derive(Clone, Copy, Debug)]
struct ReclaimPause {
    thread: thread::ThreadId,
    session: Option<trace::SessionSnapshot>,
    sync: trace::SyncSnapshot,
}

struct ReclaimGate {
    key: DatabaseKeyIndex,
    entered: mpsc::Sender<ReclaimPause>,
    release: mpsc::Receiver<()>,
}

struct NativeObserver {
    gate: Option<ReclaimGate>,
    wait: Option<(DatabaseKeyIndex, mpsc::Sender<thread::ThreadId>)>,
}

thread_local! {
    static NATIVE_OBSERVER: RefCell<Option<NativeObserver>> = const { RefCell::new(None) };
}

struct NativeObserverInstalled;
impl Drop for NativeObserverInstalled {
    fn drop(&mut self) {
        NATIVE_OBSERVER.with_borrow_mut(|slot| *slot = None);
    }
}

fn native_status(db: &dyn Database, phase: &'static str, key: Option<DatabaseKeyIndex>) {
    let mut event = Observation::new(Kind::Gate);
    event.phase = Some(phase);
    event.key = key;
    event.decision = db.cancellation_token().is_cancelled();
    event.units = usize::from(db.zalsa_local().should_trigger_local_cancellation());
    trace::record(event);
}

fn native_interruption_event(event: &Event) {
    if !NATIVE_OBSERVER.with_borrow(|observer| observer.is_some()) {
        return;
    }
    if matches!(&event.kind, EventKind::WillCheckCancellation) {
        crate::with_attached_database(|db| native_status(db, "native.check", None));
    }
    if let EventKind::WillBlockOn { database_key, .. } = &event.kind {
        let waiting = NATIVE_OBSERVER.with_borrow_mut(|slot| slot.as_mut()?.wait.take());
        if let Some((key, sender)) = waiting {
            assert_eq!(key, *database_key);
            sender.send(thread::current().id()).unwrap();
        }
    }
}

fn native_reclaim_pause(db: &dyn TransferDatabase, key: DatabaseKeyIndex, step: Step) {
    if step != Step::Body {
        return;
    }
    let wanted = NATIVE_OBSERVER.with_borrow(|slot| {
        slot.as_ref()
            .and_then(|observer| observer.gate.as_ref())
            .map(|gate| gate.key)
    });
    if wanted != Some(key) {
        return;
    }
    let sync = db
        .zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap()
        .sync_table()
        .test_transfer_state(key.key_index())
        .unwrap();
    if !sync.claimed_twice {
        return;
    }
    assert!(matches!(sync.owner, SyncOwner::Thread(owner) if owner == thread::current().id()));
    let gate = NATIVE_OBSERVER.with_borrow_mut(|slot| slot.as_mut().unwrap().gate.take().unwrap());
    let mut event = Observation::new(Kind::Gate).key(key);
    event.phase = Some("native.reclaim.pause");
    event.sync = Some(sync);
    trace::record(event);
    gate.entered
        .send(ReclaimPause {
            thread: thread::current().id(),
            session: trace::session_snapshot(),
            sync,
        })
        .unwrap();
    gate.release
        .recv_timeout(STAGE_TIMEOUT)
        .expect("transferred reclaim release stage");
    native_status(db, "native.reclaim.resume", Some(key));
}

struct NativeLive {
    thread: thread::ThreadId,
    token: CancellationToken,
    support: Option<AttemptSupport>,
}

struct NativeReport {
    outcome: NativeOutcome,
    report: WorkerReport,
    exit: Option<trace::SessionSnapshot>,
    allowance: usize,
}

fn native_worker(
    db: Db,
    input: Input,
    data: ScheduleData,
    supports: Supports,
    worker: usize,
    controlled: bool,
    allowance: usize,
    observer: NativeObserver,
    live: mpsc::Sender<NativeLive>,
    ordinal: Arc<AtomicUsize>,
    passive_schedule: bool,
) -> NativeReport {
    let root = data.worker;
    assert!(root < 2);
    let token = db.cancellation_token();
    assert!(!token.is_cancelled());
    let installed = NativeObserverInstalled;
    NATIVE_OBSERVER.with_borrow_mut(|slot| assert!(slot.replace(observer).is_none()));
    let exit = Cell::new(None);
    let driver = Cell::new(None);
    let support = RefCell::new(None);
    let (outcome, records) = trace::collect(TraceConfig { worker, ordinal }, || {
        catch_unwind(AssertUnwindSafe(|| {
            crate::attach(&db, || {
                let schedule =
                    ScheduleOwner::install(data, Keys::new(&db, input), supports.clone());
                if passive_schedule {
                    schedule.0.entered.set(true);
                }
                let result = if controlled {
                    attempt_probe::try_with_attempt(&db, allowance, || {
                        let _exit = super::ExitSession(&exit);
                        let own = attempt_probe::current().unwrap();
                        supports[root].set(own.clone()).unwrap();
                        *support.borrow_mut() = Some(own.clone());
                        live.send(NativeLive {
                            thread: thread::current().id(),
                            token: token.clone(),
                            support: Some(own),
                        })
                        .unwrap();
                        native_status(&db, "native.entry", Some(schedule.0.keys.0[root]));
                        let result = run_queries(
                            &db,
                            a::fn_ingredient_(&db, db.zalsa()),
                            b::fn_ingredient_(&db, db.zalsa()),
                            input,
                            &schedule.0,
                        );
                        driver.set(Some(result));
                        if let Ok(value) = result {
                            root_observation(&db, input, root, value);
                        }
                        result
                    })
                } else {
                    live.send(NativeLive {
                        thread: thread::current().id(),
                        token: token.clone(),
                        support: None,
                    })
                    .unwrap();
                    let value = if root == 0 {
                        a(&db, input).0
                    } else {
                        b(&db, input).0
                    };
                    root_observation(&db, input, root, value);
                    driver.set(Some(Ok(value)));
                    Ok(AttemptOutcome::Complete(Ok(value)))
                };
                native_status(&db, "native.attachment.exit", Some(schedule.0.keys.0[root]));
                result
            })
        }))
    });
    drop(installed);
    emit_native(
        &format!("transfer worker={worker} controlled={controlled}"),
        &outcome,
    );
    for record in &records.records {
        eprintln!("NATIVE_TRANSFER worker={worker} controlled={controlled} {record:?}");
    }
    super::assert_worker_clean(&db);
    assert!(crate::with_attached_database(|_| ()).is_none());
    assert!(
        !token.is_cancelled(),
        "the outer attachment resets the actual handle token"
    );
    assert!(!records.broken);
    let support = support.into_inner();
    if let Some(support) = &support {
        assert!(!support.owns_current_session(db.zalsa()));
        assert_eq!(
            exit.get().unwrap().support.owner,
            trace::support_snapshot(support).owner
        );
    } else {
        assert!(
            records
                .records
                .iter()
                .all(|record| record.event.session.is_none())
        );
    }
    eprintln!(
        "NATIVE_TRANSFER worker={worker} exit={:?}, support={:?}",
        exit.get(),
        support.as_ref().map(trace::support_snapshot)
    );
    drop(db);
    NativeReport {
        outcome,
        report: WorkerReport {
            worker,
            trace: records,
            driver: driver.get(),
            support,
        },
        exit: exit.get(),
        allowance,
    }
}

fn third_schedule() -> ScheduleData {
    let (send_a, _) = mpsc::channel();
    let (send_b, _) = mpsc::channel();
    let (send_wait, _) = mpsc::channel();
    ScheduleData {
        worker: 0,
        send_a,
        receive_a: None,
        send_b,
        receive_b: None,
        send_wait,
        receive_wait: None,
    }
}

fn installed_reclaim(
    db: &Db,
    input: Input,
    paused: ReclaimPause,
    donor: thread::ThreadId,
    third: Option<thread::ThreadId>,
) {
    let keys = Keys::new(db, input);
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    let states = keys.0.map(|key| {
        db.zalsa()
            .lookup_ingredient(key.ingredient_index())
            .as_function()
            .unwrap()
            .sync_table()
            .test_transfer_state(key.key_index())
            .unwrap()
    });
    assert!(
        !graph.edges.overflow
            && !graph.dependents.overflow
            && !graph.pending.overflow
            && !graph.transferred.overflow
            && !graph.reverse.overflow
    );
    assert!(graph.pending.is_empty());
    let waiters = std::iter::once(donor).chain(third).collect::<Vec<_>>();
    let edges = graph
        .edges
        .entries
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(edges.len(), waiters.len());
    for waiter in &waiters {
        assert!(edges.contains(&(*waiter, paused.thread)));
    }
    let dependents = graph
        .dependents
        .entries
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(dependents.len(), 1);
    assert_eq!(dependents[0].0, keys.0[0]);
    assert!(!dependents[0].1.overflow);
    let actual = dependents[0]
        .1
        .entries
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), waiters.len());
    for waiter in &waiters {
        assert!(actual.contains(waiter));
    }
    assert_eq!(
        graph
            .transferred
            .entries
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        [(keys.0[1], paused.thread, keys.0[0])]
    );
    let reverse = graph
        .reverse
        .entries
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(reverse.len(), 1);
    assert_eq!(reverse[0].0, keys.0[0]);
    assert!(!reverse[0].1.overflow);
    assert_eq!(
        reverse[0]
            .1
            .entries
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
        [keys.0[1]]
    );
    for state in states {
        assert!(matches!(state.owner, SyncOwner::Thread(owner) if owner == paused.thread));
    }
    assert!(states[0].anyone_waiting && states[0].is_transfer_target);
    assert!(states[1].claimed_twice && paused.sync.claimed_twice);
    eprintln!("NATIVE_TRANSFER installed pause={paused:?}; graph={graph:?}; sync={states:?}");
}

fn assert_native_claims(records: &[Record], row: NativeRow) {
    let mut claims = BTreeMap::new();
    for record in records {
        if record.event.kind == Kind::Claim {
            assert!(
                claims
                    .insert(record.event.serial.unwrap(), (record.event.key, 0))
                    .is_none()
            );
        }
        if record.event.kind == Kind::Terminal {
            let claim = claims.get_mut(&record.event.serial.unwrap()).unwrap();
            assert_eq!(claim.0, record.event.key);
            claim.1 += 1;
            if record.event.action == Some(Action::Panic) {
                assert!(matches!(
                    row,
                    NativeRow::ReceiverLocal
                        | NativeRow::ReceiverLocalWaiter
                        | NativeRow::DonorLocal
                        | NativeRow::Writer
                ));
                assert!(matches!(
                    record.event.wait,
                    Some(WaitResult::Cancelled | WaitResult::Panicked)
                ));
            }
        }
        if record.event.kind == Kind::SupportIncoming
            && let (Some(previous), Some(incoming)) =
                (record.event.previous_support, record.event.support)
        {
            assert_eq!(previous.owner, incoming.owner);
        }
        if record.event.kind == Kind::SupportAccepted {
            assert_eq!(
                record.event.support.unwrap().owner,
                record.event.session.unwrap().support.owner
            );
        }
        if matches!(
            record.event.kind,
            Kind::RootPublished | Kind::TargetPublished
        ) && let Some(memo) = record.event.memo
            && memo.final_
            && memo.has_value
            && let Some(support) = memo.support
        {
            assert!(!support.explicitly_incomplete && !matches!(support.state, 1 | 3 | 4));
        }
    }
    assert!(!claims.is_empty() && claims.values().all(|(_, count)| *count == 1));
}

fn native_donor_refetch<'a>(
    records: &'a [Record],
    keys: Keys,
    handoff: &Handoff<'_>,
    writer: bool,
) -> &'a Record {
    let consumed = after(records, handoff.wait, "native transfer wake", |record| {
        query_event(record, 1, Kind::WaitConsumed, keys.0[0])
    });
    if writer {
        assert!(matches!(consumed.event.wait, Some(WaitResult::Cancelled)));
    }
    let returned = after(
        records,
        consumed,
        "native transfer mandatory refetch",
        |record| query_event(record, 1, Kind::TransferWaitEnd, keys.0[1]) && record.event.decision,
    );
    assert!(matches!(
        (consumed.event.wait, returned.event.wait),
        (Some(WaitResult::Completed), Some(WaitResult::Completed))
            | (Some(WaitResult::Cancelled), Some(WaitResult::Cancelled))
            | (Some(WaitResult::Panicked), Some(WaitResult::Panicked))
    ));
    if let (Some(before), Some(after)) = (handoff.wait.event.session, returned.event.session) {
        assert_eq!(before, after);
    }
    let guard = after(
        records,
        returned,
        "transferred guard already consumed",
        |record| {
            query_event(record, 1, Kind::TransferEnd, keys.0[1])
                && record.event.serial == handoff.donor_claim.event.serial
                && record.event.decision
        },
    );
    after(
        records,
        guard,
        "discard pre-transfer selected pointer",
        |record| {
            query_event(record, 1, Kind::Refetch, keys.0[1])
                && record.event.serial == handoff.donor_claim.event.serial
                && record.event.decision
        },
    );
    returned
}

fn native_allowances(reports: &[NativeReport], writer_reclaim: Option<&Restart<'_>>) {
    let owners = reports
        .iter()
        .filter_map(|report| report.report.support.as_ref().map(trace::support_snapshot))
        .collect::<Vec<_>>();
    for (index, owner) in owners.iter().enumerate() {
        for peer in &owners[index + 1..] {
            assert_ne!(owner.owner, peer.owner);
        }
    }
    for report in reports {
        let Some(exit) = report.exit else {
            continue;
        };
        let mut remaining = report.allowance;
        let mut interrupted_debits = 0;
        for (index, record) in report.report.trace.records.iter().enumerate() {
            if let Some(session) = record.event.session {
                assert_eq!(session.support.owner, exit.support.owner);
                assert_eq!(
                    (
                        session.support.database,
                        session.support.revision,
                        session.support.cancellation
                    ),
                    (
                        exit.support.database,
                        exit.support.revision,
                        exit.support.cancellation
                    )
                );
                assert!(session.remaining <= remaining);
                if session.remaining < remaining {
                    let following = after_native_checks(&report.report.trace.records, index);
                    if following.event.kind == Kind::Admission {
                        assert_eq!(following.event.phase, Some("work"));
                        assert_eq!(following.event.units, remaining - session.remaining);
                    } else {
                        // admit_work subtracts before its cancellation-aware admission check.
                        // Only this actual writer interruption may omit the admission report.
                        assert_eq!(report.report.worker, 0);
                        let restart =
                            writer_reclaim.expect("unattributed debit outside the writer receiver");
                        assert_native(&report.outcome, "PendingWrite", None);
                        assert!(is_native_check(record));
                        assert!(record.ordinal > restart.debit.ordinal);
                        assert_eq!(restart.debit.event.step, Some(Step::Body));
                        assert!(restart.debit.event.sync.unwrap().claimed_twice);
                        assert_eq!(restart.debit.event.units, 1);
                        assert_eq!(remaining - session.remaining, 1);
                        assert_eq!(remaining, restart.debit.event.session.unwrap().remaining);
                        assert_eq!(following.event.kind, Kind::SupportIncoming);
                        assert_eq!(following.event.key, restart.claim.event.key);
                        let interrupted = following.event.session.unwrap();
                        assert_eq!(interrupted.support.state, 4);
                        assert!(interrupted.support.explicitly_incomplete);
                        let terminal = after(
                            &report.report.trace.records,
                            following,
                            "incomplete support releases its interrupted claim",
                            |r| r.event.kind == Kind::Terminal,
                        );
                        for retained in report.report.trace.records.iter().filter(|r| {
                            following.ordinal <= r.ordinal && r.ordinal < terminal.ordinal
                        }) {
                            assert!(matches!(
                                retained.event.kind,
                                Kind::SupportIncoming | Kind::SupportAccepted
                            ));
                            assert_eq!(retained.event.key, restart.claim.event.key);
                            assert_eq!(retained.event.session, Some(interrupted));
                            let support = retained.event.support.unwrap();
                            assert_eq!(support.owner, interrupted.support.owner);
                            assert_eq!(support.state, 4);
                            if retained.event.kind == Kind::SupportAccepted {
                                assert!(support.explicitly_incomplete);
                            }
                        }
                        assert_eq!(terminal.event.key, restart.claim.event.key);
                        assert_eq!(terminal.event.serial, restart.claim.event.serial);
                        assert_eq!(terminal.event.session, Some(interrupted));
                        assert_eq!(terminal.event.action, Some(Action::Abort));
                        assert!(matches!(terminal.event.wait, Some(WaitResult::Cancelled)));
                        assert!(
                            !report.report.trace.records.iter().any(|prior| prior.ordinal
                                > restart.debit.ordinal
                                && prior.ordinal < record.ordinal
                                && matches!(
                                    prior.event.kind,
                                    Kind::Claim
                                        | Kind::PreDebit
                                        | Kind::DebitAccepted
                                        | Kind::BodyValue
                                        | Kind::RootResult
                                ))
                        );
                        interrupted_debits += 1;
                        assert_eq!(interrupted_debits, 1);
                        eprintln!(
                            "NATIVE_TRANSFER charged before PendingWrite: predebit={} observed={} terminal={} units=1",
                            restart.debit.ordinal, record.ordinal, terminal.ordinal
                        );
                    }
                }
                remaining = session.remaining;
            }
        }
        assert_eq!(remaining, exit.remaining);
        assert_eq!(
            interrupted_debits,
            usize::from(writer_reclaim.is_some() && report.report.worker == 0)
        );
        let expected = if report.outcome.is_err() {
            3
        } else if matches!(
            report.outcome,
            Ok(Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)))
        ) {
            1
        } else {
            2
        };
        assert_eq!(
            trace::support_snapshot(report.report.support.as_ref().unwrap()).state,
            expected
        );
    }
}

fn native_registered_retries(records: &[Record], reports: &[NativeReport]) {
    for consumed in records
        .iter()
        .filter(|record| record.event.kind == Kind::WaitConsumed && record.event.session.is_some())
    {
        let next = records
            .iter()
            .find(|record| record.worker == consumed.worker && record.ordinal > consumed.ordinal);
        if next.is_some_and(|record| record.event.kind == Kind::TransferWaitEnd) {
            continue;
        }
        let next_work = records.iter().find(|record| {
            record.worker == consumed.worker
                && record.ordinal > consumed.ordinal
                && record.event.kind == Kind::Admission
                && record.event.phase == Some("work")
        });
        if let Some(work) = next_work {
            assert!(!matches!(consumed.event.wait, Some(WaitResult::Panicked)));
            assert_eq!(work.event.units, 1);
            let before = consumed.event.session.unwrap();
            let after = work.event.session.unwrap();
            assert_eq!(before.support, after.support);
            assert_eq!(before.scope, after.scope);
            assert_eq!(before.remaining - 1, after.remaining);
            assert!(
                !records.iter().any(|record| record.worker == consumed.worker
                    && record.ordinal > consumed.ordinal
                    && record.ordinal < work.ordinal
                    && matches!(
                        record.event.kind,
                        Kind::Claim | Kind::PreDebit | Kind::BodyValue
                    ))
            );
        } else {
            assert!(
                reports[consumed.worker].outcome.is_err(),
                "surviving registered wake omitted its retry debit"
            );
        }
    }
}

fn fresh_native_consumer(db: &Db, input: Input) -> u32 {
    let result = attempt_probe::try_with_attempt(db, ALLOWANCE, || a(db, input).0);
    let Ok(AttemptOutcome::Complete(value)) = result else {
        panic!("fresh transferred recovery failed: {result:?}");
    };
    value
}

struct AcceptedLocal<'a> {
    published: &'a Record,
    retired: &'a Record,
}

fn accepted_before_local<'a>(
    records: &'a [Record],
    keys: Keys,
    resumed: &Record,
    transfer_returned: &Record,
) -> AcceptedLocal<'a> {
    let published = after(
        records,
        resumed,
        "receiver accepts A before Local",
        |record| {
            query_event(record, 0, Kind::RootPublished, keys.0[0])
                && record
                    .event
                    .memo
                    .is_some_and(|memo| memo.has_value && memo.final_)
        },
    );
    let memo = published.event.memo.unwrap();
    assert!(!memo.heads.overflow && memo.heads.is_empty());
    assert_eq!(
        memo.support.unwrap().owner,
        published.event.session.unwrap().support.owner
    );
    assert!(!memo.support.unwrap().explicitly_incomplete);
    let prepared_after = records
        .iter()
        .rev()
        .find(|record| {
            record.ordinal < published.ordinal
                && query_event(record, 0, Kind::RecoveryValue, keys.0[0])
        })
        .expect("final preparation follows receiver recovery");
    let storage = after(
        records,
        prepared_after,
        "masked final storage admission",
        |record| {
            record.worker == 0
                && record.event.kind == Kind::Admission
                && record.event.phase == Some("resource")
        },
    );
    let work = after(
        records,
        storage,
        "masked final publication admission",
        |record| {
            record.worker == 0
                && record.event.kind == Kind::Admission
                && record.event.phase == Some("work")
        },
    );
    let current = after(
        records,
        work,
        "accepted root still selects current memos",
        |record| {
            query_event(record, 0, Kind::CommitCurrent, keys.0[0])
                && record.event.serial == published.event.serial
        },
    );
    assert!(current.event.decision && current.ordinal < published.ordinal);
    let checks = records
        .iter()
        .filter(|record| {
            record.worker == 0
                && prepared_after.ordinal < record.ordinal
                && record.ordinal < published.ordinal
                && is_native_check(record)
        })
        .collect::<Vec<_>>();
    assert!(!checks.is_empty());
    for check in checks {
        assert!(
            check.event.decision,
            "Local remains requested through preparation"
        );
        assert_eq!(
            check.event.units, 0,
            "Local remains masked through preparation"
        );
    }
    let terminal = after(
        records,
        published,
        "accepted receiver claim retires normally",
        |record| {
            query_event(record, 0, Kind::Terminal, keys.0[0])
                && record.event.serial == published.event.serial
        },
    );
    assert_eq!(terminal.event.action, Some(Action::Drop));
    assert!(matches!(terminal.event.wait, Some(WaitResult::Completed)));
    for target in records.iter().filter(|record| {
        record.worker == 0
            && prepared_after.ordinal < record.ordinal
            && record.ordinal < published.ordinal
            && record.event.kind == Kind::Target
    }) {
        let checked = after(
            records,
            target,
            "selected target remains current",
            |record| {
                record.worker == 0
                    && record.event.kind == Kind::TargetCurrent
                    && record.event.key == target.event.key
            },
        );
        assert!(checked.event.decision && checked.ordinal < published.ordinal);
        assert_eq!(
            checked.event.memo.map(|memo| memo.identity),
            target.event.memo.map(|memo| memo.identity)
        );
        let written = after(
            records,
            published,
            "selected target publishes before retirement",
            |record| {
                record.worker == 0
                    && record.event.kind == Kind::TargetPublished
                    && record.event.key == target.event.key
            },
        );
        assert!(written.ordinal < terminal.ordinal);
        assert_eq!(written.event.action, target.event.action);
        assert_eq!(
            written.event.memo.map(|memo| memo.identity),
            target.event.memo.map(|memo| memo.identity)
        );
    }
    let retired = after(
        records,
        terminal,
        "native receiver retirement has returned",
        |record| {
            query_event(record, 0, Kind::Refetch, keys.0[0])
                && record.event.serial == published.event.serial
        },
    );
    assert!(!retired.event.decision);
    let enabled = after(
        records,
        resumed,
        "receiver Local reaches an enabled checkpoint",
        |record| {
            record.worker == 0
                && is_native_check(record)
                && record.event.decision
                && record.event.units == 1
        },
    );
    assert!(retired.ordinal < enabled.ordinal);
    assert!(terminal.ordinal < transfer_returned.ordinal);
    assert!(matches!(
        transfer_returned.event.wait,
        Some(WaitResult::Completed)
    ));
    assert!(
        !records
            .iter()
            .any(|record| record.worker == 0 && record.event.kind == Kind::RootResult)
    );
    for target in records.iter().filter(|record| {
        record.worker == 0
            && published.ordinal < record.ordinal
            && record.ordinal < retired.ordinal
            && record.event.kind == Kind::TargetPublished
    }) {
        assert!(target.ordinal < terminal.ordinal);
    }
    AcceptedLocal { published, retired }
}

// The cancelled receiver cannot supply the RootResult required by the original success helper.
// Its accepted allocation and completed claim retirement give this donor the same final input.
fn donor_restart_after_accepted_local(
    records: &[Record],
    keys: Keys,
    handoff: &Handoff<'_>,
    owners: [usize; 2],
    accepted: &AcceptedLocal<'_>,
) {
    assert_eq!(
        accepted.published.event.session,
        accepted.retired.event.session
    );
    let probe = donor_refetch(records, keys, handoff);
    let supplied = after(records, probe, "donor supplied receiver B", |record| {
        query_event(record, 1, Kind::PrepareSupplied, keys.0[1])
    });
    let foreign = supplied.event.memo.unwrap();
    assert!(!foreign.final_ && foreign.has_value);
    assert_eq!(foreign.support.unwrap().owner, owners[0]);
    let claim = records
        .iter()
        .find(|record| {
            query_event(record, 1, Kind::Claim, keys.0[1])
                && record.event.serial == supplied.event.serial
        })
        .unwrap();
    assert!(probe.ordinal < claim.ordinal && claim.ordinal < supplied.ordinal);
    let verified = after(
        records,
        claim,
        "donor verifies the exact foreign B as changed",
        |record| {
            query_event(record, 1, Kind::Verification, keys.0[1])
                && record.event.serial == claim.event.serial
                && !record.event.decision
        },
    );
    let execute = after(
        records,
        verified,
        "changed foreign B enters execution",
        |record| {
            query_event(record, 1, Kind::Verified, keys.0[1])
                && record.event.serial == claim.event.serial
                && !record.event.decision
        },
    );
    assert!(execute.ordinal < supplied.ordinal);
    for record in [verified, execute] {
        assert_eq!(record.event.memo.unwrap().identity, foreign.identity);
    }
    let declined = after(
        records,
        supplied,
        "donor rejects foreign provisional seed",
        |record| {
            record.worker == 1
                && record.event.kind == Kind::SeedAllowed
                && record.event.identity == foreign.identity
                && !record.event.decision
        },
    );
    let retained = after(
        records,
        declined,
        "donor clears foreign baseline",
        |record| {
            query_event(record, 1, Kind::PrepareRetained, keys.0[1])
                && record.event.serial == claim.event.serial
        },
    );
    assert!(retained.event.memo.is_none());
    let iteration = after(
        records,
        retained,
        "donor starts its own history",
        |record| {
            query_event(record, 1, Kind::Iteration, keys.0[1])
                && record.event.serial == claim.event.serial
        },
    );
    assert!(iteration.event.memo.is_none() && iteration.event.other_memo.is_none());
    let debit = after(records, iteration, "donor fresh B debit", |record| {
        query_event(record, 1, Kind::PreDebit, keys.0[1]) && record.event.step == Some(Step::Body)
    });
    let charged = after(records, debit, "donor fresh B debit accepted", |record| {
        query_event(record, 1, Kind::DebitAccepted, keys.0[1])
            && record.event.step == Some(Step::Body)
    });
    assert_eq!((debit.event.units, charged.event.units), (1, 1));
    assert_eq!(
        debit.event.session.unwrap().remaining,
        charged.event.session.unwrap().remaining + 1
    );
    let request = after(records, charged, "donor asks for accepted A", |record| {
        query_event(record, 1, Kind::ChildRequest, keys.0[1])
            && record.event.other_key == Some(keys.0[0])
    });
    let a = accepted.published.event.memo.unwrap();
    assert!(accepted.published.ordinal < request.ordinal);
    let reused = after(
        records,
        request,
        "donor reuses exact accepted A",
        |record| {
            record.worker == 1
                && record.event.kind == Kind::Reuse
                && record.event.identity == a.identity
                && record.event.reuse == Some(MemoReuse::Ordinary)
        },
    );
    let body = after(records, reused, "donor fresh body completes", |record| {
        query_event(record, 1, Kind::BodyValue, keys.0[1])
    });
    assert_eq!(body.event.value, 3);
    assert!(
        !records
            .iter()
            .any(|record| accepted.published.ordinal < record.ordinal
                && record.ordinal < body.ordinal
                && record.event.key == Some(keys.0[0])
                && matches!(
                    record.event.kind,
                    Kind::RootPublished | Kind::TargetPublished
                ))
    );
    assert!(!records.iter().any(|record| record.worker == 1
        && debit.ordinal < record.ordinal
        && record.ordinal < body.ordinal
        && (matches!(
            record.event.kind,
            Kind::MarkerRead | Kind::InitialValue | Kind::RecoveryValue
        ) || (record.event.kind == Kind::PreDebit
            && matches!(record.event.step, Some(Step::Initial | Step::Recovery))))));
    let current = after(records, body, "donor current publication", |record| {
        query_event(record, 1, Kind::CommitCurrent, keys.0[1])
            && record.event.serial == claim.event.serial
    });
    assert!(current.event.decision);
    let published = after(records, current, "donor fresh B publication", |record| {
        query_event(record, 1, Kind::RootPublished, keys.0[1])
            && record.event.serial == claim.event.serial
    });
    let b = published.event.memo.unwrap();
    assert_ne!(b.identity, foreign.identity);
    assert!(b.has_value && b.final_ && !b.converged && b.iteration.is_default());
    assert!(b.support.is_none() && !b.heads.overflow && b.heads.is_empty());
    assert_eq!(
        b.verified_at,
        published.event.session.unwrap().support.revision
    );
    assert_eq!(b.changed_at, b.verified_at);
    let terminal = after(records, published, "donor fresh B releases", |record| {
        query_event(record, 1, Kind::Terminal, keys.0[1])
            && record.event.serial == claim.event.serial
    });
    for record in [
        verified, execute, supplied, declined, retained, iteration, debit, charged, request,
        reused, body, current, published,
    ] {
        assert!(claim.ordinal < record.ordinal && record.ordinal < terminal.ordinal);
        assert_eq!(record.event.session.unwrap().support.owner, owners[1]);
        if let Some(serial) = record.event.serial {
            assert_eq!(Some(serial), claim.event.serial);
        }
    }
    let result = after(
        records,
        terminal,
        "donor delivers its own final B",
        |record| query_event(record, 1, Kind::RootResult, keys.0[1]),
    );
    assert_eq!(result.event.value, 3);
    assert_eq!(result.event.memo.unwrap().identity, b.identity);
}

fn native_recovery(
    db: &mut Db,
    input: Input,
    baseline: &Baseline,
    row: NativeRow,
    records: &[Record],
    owners: Option<[usize; 2]>,
    accepted_local: Option<&AcceptedLocal<'_>>,
) {
    let wanted = if row == NativeRow::Writer { 4 } else { 3 };
    // A fresh evaluation must work before subsequent ordinary reads can finish lazy finality.
    assert_eq!(fresh_native_consumer(db, input), wanted);
    let initial = canonical(db, input);
    assert_eq!(initial.values, [wanted, wanted]);
    assert_input_paths(&initial, "native recovery");
    if row != NativeRow::Writer {
        if (row == NativeRow::Calibration || accepted_local.is_some())
            && initial != baseline.initial
        {
            assert_eq!(
                initial.origins,
                [
                    vec![Label::Marker, Label::Limit],
                    vec![Label::A, Label::Limit]
                ]
            );
            let keys = Keys::new(db, input);
            let handoff = handoff(records, keys);
            if let Some(accepted) = accepted_local {
                donor_restart_after_accepted_local(
                    records,
                    keys,
                    &handoff,
                    owners.unwrap(),
                    accepted,
                );
            } else {
                assert_donor_restart(records, keys, &handoff, owners.unwrap());
            }
        } else {
            assert_eq!(initial, baseline.initial);
        }
    } else {
        let mut ordinary = Db::default();
        let original = Input::new(&ordinary, 3, 17);
        canonical(&ordinary, original);
        original.set_limit(&mut ordinary).to(4);
        assert_eq!(initial, canonical(&ordinary, original));
    }
    let counts = db.counts.snapshot();
    let snapshots = Keys::new(db, input).0.map(|key| {
        db.zalsa()
            .lookup_ingredient(key.ingredient_index())
            .as_function()
            .unwrap()
            .memo(db.zalsa(), key.key_index())
            .unwrap()
            .transfer_test_snapshot()
    });
    assert_eq!(fresh_native_consumer(db, input), wanted);
    assert_eq!(canonical(db, input), initial);
    assert_eq!(db.counts.snapshot(), counts);
    for (key, old) in Keys::new(db, input).0.into_iter().zip(snapshots) {
        let current = db
            .zalsa()
            .lookup_ingredient(key.ingredient_index())
            .as_function()
            .unwrap()
            .memo(db.zalsa(), key.key_index())
            .unwrap()
            .transfer_test_snapshot();
        assert_eq!(old.identity, current.identity);
        assert!(current.final_ && current.has_value);
        assert_eq!(current.verified_at, db.zalsa().current_revision());
    }
    if row != NativeRow::Writer {
        input.set_seed_marker(db).to(23);
        assert_eq!(fresh_native_consumer(db, input), 3);
        assert_eq!(canonical(db, input), baseline.edited);
    }
}

fn native_row(
    row: NativeRow,
    controlled: bool,
    baseline: &Baseline,
    calibration: Option<&Calibration>,
) -> Option<Calibration> {
    eprintln!("NATIVE_TRANSFER {row:?} controlled={controlled} starts");
    let (flag_tx, flag_rx) = mpsc::channel();
    let armed = Arc::new(AtomicBool::new(false));
    let event_armed = armed.clone();
    let db = Db {
        storage: crate::Storage::new(Some(Box::new(move |event| {
            if matches!(&event.kind, EventKind::DidSetCancellationFlag)
                && event_armed.load(Ordering::SeqCst)
            {
                flag_tx.send(()).unwrap();
            }
            wait_event(event);
        }))),
        counts: Arc::default(),
    };
    let input = Input::new(&db, 3, 17);
    let keys = Keys::new(&db, input);
    let revision = db.zalsa().current_revision();
    let epoch = db.zalsa().runtime().cancellation_count();
    let allowance = if row == NativeRow::Refusal {
        calibration.unwrap().allowance
    } else {
        ALLOWANCE
    };
    let supports = Arc::new([OnceLock::new(), OnceLock::new()]);
    let ordinal = Arc::new(AtomicUsize::new(0));
    let (mut db, reports, paused) = thread::scope(|scope| {
        let mut lease = WriterLease {
            db: Some(db),
            start: None,
        };
        let (start_tx, start_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let writer = if row == NativeRow::Writer {
            let mut writer_db = lease.db.as_ref().unwrap().clone();
            lease.start = Some(start_tx);
            Some(scope.spawn(move || {
                start_rx
                    .recv_timeout(STAGE_TIMEOUT)
                    .expect("transfer writer start");
                armed.store(true, Ordering::SeqCst);
                input.set_limit(&mut writer_db).to(4);
                armed.store(false, Ordering::SeqCst);
                let _ = done_tx.send(());
                writer_db
            }))
        } else {
            None
        };
        let [left, right] = schedules();
        let (paused_tx, paused_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release = Release {
            sender: Some(release_tx),
            value: Some(()),
        };
        let (r_tx, r_rx) = mpsc::channel();
        let (d_tx, d_rx) = mpsc::channel();
        let r_db = lease.db.as_ref().unwrap().clone();
        let d_db = lease.db.as_ref().unwrap().clone();
        let r_supports = supports.clone();
        let d_supports = supports.clone();
        let r_ordinal = ordinal.clone();
        let d_ordinal = ordinal.clone();
        let receiver = scope.spawn(move || {
            native_worker(
                r_db,
                input,
                left,
                r_supports,
                0,
                controlled,
                allowance,
                NativeObserver {
                    gate: Some(ReclaimGate {
                        key: keys.0[1],
                        entered: paused_tx,
                        release: release_rx,
                    }),
                    wait: None,
                },
                r_tx,
                r_ordinal,
                false,
            )
        });
        let donor = scope.spawn(move || {
            native_worker(
                d_db,
                input,
                right,
                d_supports,
                1,
                controlled,
                ALLOWANCE,
                NativeObserver {
                    gate: None,
                    wait: None,
                },
                d_tx,
                d_ordinal,
                false,
            )
        });
        let receiver_live = r_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        let donor_live = d_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        let paused = paused_rx
            .recv_timeout(STAGE_TIMEOUT)
            .expect("actual receiver Body(B) re-claim boundary");
        assert_eq!(paused.thread, receiver_live.thread);
        if controlled {
            assert_eq!(
                paused.session.unwrap().support.owner,
                trace::support_snapshot(receiver_live.support.as_ref().unwrap()).owner
            );
        } else {
            assert!(paused.session.is_none());
        }
        installed_reclaim(
            lease.db.as_ref().unwrap(),
            input,
            paused,
            donor_live.thread,
            None,
        );
        let third = if row.overlap() {
            assert!(controlled);
            let third_supports = Arc::new([OnceLock::new(), OnceLock::new()]);
            third_supports[1]
                .set(receiver_live.support.as_ref().unwrap().clone())
                .unwrap();
            let (live_tx, live_rx) = mpsc::channel();
            let (wait_tx, wait_rx) = mpsc::channel();
            let third_db = lease.db.as_ref().unwrap().clone();
            let third_ordinal = ordinal.clone();
            let third = scope.spawn(move || {
                native_worker(
                    third_db,
                    input,
                    third_schedule(),
                    third_supports,
                    2,
                    true,
                    ALLOWANCE,
                    NativeObserver {
                        gate: None,
                        wait: Some((keys.0[0], wait_tx)),
                    },
                    live_tx,
                    third_ordinal,
                    true,
                )
            });
            let live = live_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
            let waiting = wait_rx
                .recv_timeout(STAGE_TIMEOUT)
                .expect("independent third evaluation installed A wait");
            assert_eq!(live.thread, waiting);
            assert!(
                !live
                    .support
                    .as_ref()
                    .unwrap()
                    .same_owner(receiver_live.support.as_ref().unwrap())
            );
            installed_reclaim(
                lease.db.as_ref().unwrap(),
                input,
                paused,
                donor_live.thread,
                Some(waiting),
            );
            Some((third, live))
        } else {
            None
        };
        match row {
            NativeRow::ReceiverLocal | NativeRow::ReceiverLocalWaiter | NativeRow::Refusal => {
                receiver_live.token.cancel()
            }
            NativeRow::DonorLocal => donor_live.token.cancel(),
            NativeRow::Writer => {
                lease.start();
                flag_rx
                    .recv_timeout(STAGE_TIMEOUT)
                    .expect("actual transfer writer cancellation event");
                let observer = lease.db.as_ref().unwrap();
                assert!(observer.zalsa().runtime().load_cancellation_flag());
                assert_eq!(observer.zalsa().current_revision(), revision);
                assert_eq!(observer.zalsa().runtime().cancellation_count(), epoch);
                assert!(!receiver_live.token.is_cancelled() && !donor_live.token.is_cancelled());
                assert!(!third.as_ref().unwrap().1.token.is_cancelled());
            }
            NativeRow::Calibration => {}
        }
        if row == NativeRow::DonorLocal {
            assert!(donor_live.token.is_cancelled());
            installed_reclaim(
                lease.db.as_ref().unwrap(),
                input,
                paused,
                donor_live.thread,
                None,
            );
        }
        eprintln!("NATIVE_TRANSFER {row:?} trigger installed; release receiver");
        drop(release);
        let mut reports = vec![receiver.join().unwrap(), donor.join().unwrap()];
        if let Some((third, _)) = third {
            reports.push(third.join().unwrap());
        }
        audit(lease.db.as_ref().unwrap(), input);
        if let Some(writer) = writer {
            let observer = lease.db.as_ref().unwrap();
            assert_eq!(observer.zalsa().current_revision(), revision);
            assert_eq!(observer.zalsa().runtime().cancellation_count(), epoch);
            assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
            eprintln!("NATIVE_TRANSFER writer readers dropped; final observer drops now");
            drop(lease.db.take());
            let returned = writer.join().unwrap();
            done_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
            (returned, reports, paused)
        } else {
            (lease.db.take().unwrap(), reports, paused)
        }
    });
    let mut records = reports
        .iter()
        .flat_map(|report| report.report.trace.records.iter().copied())
        .collect::<Vec<_>>();
    records.sort_unstable_by_key(|record| record.ordinal);
    assert_native_claims(&records, row);
    if controlled {
        native_registered_retries(&records, &reports);
    }
    let handoff = handoff(&records, keys);
    let owners = controlled.then(|| {
        [
            trace::support_snapshot(reports[0].report.support.as_ref().unwrap()).owner,
            trace::support_snapshot(reports[1].report.support.as_ref().unwrap()).owner,
        ]
    });
    let restart = owners.map(|owners| receiver_restart(&records, keys, &handoff, owners));
    native_allowances(
        &reports,
        if row == NativeRow::Writer {
            restart.as_ref()
        } else {
            None
        },
    );
    let pause = find(&records, "paused actual receiver B", |record| {
        record.worker == 0 && record.event.phase == Some("native.reclaim.pause")
    });
    let resumed = after(
        &records,
        pause,
        "receiver resumes at same boundary",
        |record| record.worker == 0 && record.event.phase == Some("native.reclaim.resume"),
    );
    assert_eq!(pause.thread, paused.thread);
    if let Some(restart) = &restart {
        assert!(restart.debit.ordinal < pause.ordinal);
        assert!(!records.iter().any(|record| record.worker == 0
            && record.ordinal > restart.debit.ordinal
            && record.ordinal < resumed.ordinal
            && matches!(
                record.event.kind,
                Kind::DebitAccepted | Kind::DebitRefused | Kind::BodyValue
            )));
    }
    if row.receiver_local() || row == NativeRow::Refusal {
        assert!(
            resumed.event.decision,
            "actual receiver token remains requested"
        );
        assert_eq!(
            resumed.event.units, 0,
            "native cycle iteration still masks Local"
        );
    }
    let transfer_returned =
        native_donor_refetch(&records, keys, &handoff, row == NativeRow::Writer);
    for (worker, report) in reports.iter().enumerate() {
        if controlled {
            match (row, worker) {
                (NativeRow::ReceiverLocal | NativeRow::ReceiverLocalWaiter, 0)
                | (NativeRow::DonorLocal, 1) => assert_native(&report.outcome, "Local", None),
                (NativeRow::Writer, 0..=2) => {
                    assert_native(&report.outcome, "PendingWrite", None)
                }
                (NativeRow::Refusal, 0) => assert_eq!(
                    report.outcome.as_ref().unwrap(),
                    &Ok(AttemptOutcome::Incomplete(Incomplete::Allowance))
                ),
                _ => assert_eq!(
                    report.outcome.as_ref().unwrap(),
                    &Ok(AttemptOutcome::Complete(Ok(3)))
                ),
            }
        } else if report.outcome.is_err() {
            assert_eq!(worker, usize::from(row == NativeRow::DonorLocal));
            assert_native(&report.outcome, "Local", None);
            assert!(records.iter().any(|record| record.worker == worker
                && record.event.phase == Some("native.check")
                && record.event.decision
                && record.event.units == 1));
            assert!(
                !records
                    .iter()
                    .any(|record| record.worker == worker && record.event.kind == Kind::RootResult)
            );
        } else {
            assert_eq!(
                report.outcome.as_ref().unwrap(),
                &Ok(AttemptOutcome::Complete(Ok(3)))
            );
            if worker == usize::from(row == NativeRow::DonorLocal) {
                let exit = find(
                    &records,
                    "ordinary request deferred through final value",
                    |record| {
                        record.worker == worker
                            && record.event.phase == Some("native.attachment.exit")
                    },
                );
                assert!(exit.event.decision);
            }
        }
    }
    if row == NativeRow::Writer {
        let restart = restart.as_ref().unwrap();
        let b_terminal = after(&records, resumed, "writer aborts reclaimed B", |record| {
            query_event(record, 0, Kind::Terminal, keys.0[1])
                && record.event.serial == restart.claim.event.serial
                && record.event.action == Some(Action::Abort)
        });
        assert!(matches!(b_terminal.event.wait, Some(WaitResult::Cancelled)));
        let undo = after(&records, b_terminal, "writer undoes B transfer", |record| {
            query_event(record, 0, Kind::Undo, keys.0[1])
        });
        let a_terminal = after(&records, undo, "writer aborts ancestor A", |record| {
            query_event(record, 0, Kind::Terminal, keys.0[0])
                && record.event.action == Some(Action::Abort)
        });
        assert!(matches!(a_terminal.event.wait, Some(WaitResult::Cancelled)));
        for key in keys.0 {
            assert!(
                !records
                    .iter()
                    .any(|record| query_event(record, 0, Kind::Poisoned, key)),
                "native cancellation aborts owned queries without poisoning their memos"
            );
        }

        assert!(transfer_returned.ordinal > a_terminal.ordinal);
        let third_consumed = find(
            &records,
            "third registered waiter consumes Cancelled",
            |record| query_event(record, 2, Kind::WaitConsumed, keys.0[0]),
        );
        assert!(matches!(
            third_consumed.event.wait,
            Some(WaitResult::Cancelled)
        ));
        assert_eq!(reports[2].exit.unwrap().remaining, ALLOWANCE);
        assert!(
            records
                .iter()
                .all(|record| record.event.kind != Kind::RootResult)
        );
        assert!(db.zalsa().current_revision() > revision);
        assert_eq!(db.zalsa().runtime().cancellation_count(), 0);
        assert!(!db.zalsa().runtime().load_cancellation_flag());
        assert_eq!(input.limit(&db), 4);
    }
    if row == NativeRow::DonorLocal && controlled {
        assert!(records.iter().any(|record| record.worker == 1
            && record.ordinal > transfer_returned.ordinal
            && record.event.phase == Some("native.check")
            && record.event.decision
            && record.event.units == 1));
        assert!(
            !records
                .iter()
                .any(|record| record.worker == 1 && record.event.kind == Kind::RootResult)
        );
        assert!(records.iter().any(|record| {
            query_event(record, 0, Kind::RootPublished, keys.0[0])
                && record
                    .event
                    .memo
                    .is_some_and(|memo| memo.has_value && memo.final_)
        }));
    }
    let accepted_local = (row.receiver_local() && controlled)
        .then(|| accepted_before_local(&records, keys, resumed, transfer_returned));
    if let Some(accepted) = &accepted_local {
        let current = db
            .zalsa()
            .lookup_ingredient(keys.0[0].ingredient_index())
            .as_function()
            .unwrap()
            .memo(db.zalsa(), keys.0[0].key_index())
            .unwrap()
            .transfer_test_snapshot();
        assert_eq!(
            current.identity,
            accepted.published.event.memo.unwrap().identity
        );
        assert!(current.has_value && current.final_);
        assert_eq!(
            current.support.unwrap().state,
            3,
            "accepted A survives receiver abandonment"
        );
        if row == NativeRow::ReceiverLocalWaiter {
            let consumed = find(&records, "third waiter consumes accepted A", |record| {
                query_event(record, 2, Kind::WaitConsumed, keys.0[0])
            });
            assert!(matches!(consumed.event.wait, Some(WaitResult::Completed)));
            assert!(accepted.published.ordinal < consumed.ordinal);
            assert_eq!(
                reports[2].exit.unwrap().remaining,
                ALLOWANCE - 1 - super::ROOT_READ_WORK
            );
            let result = after(
                &records,
                consumed,
                "third waiter reuses accepted A",
                |record| query_event(record, 2, Kind::RootResult, keys.0[0]),
            );
            assert_eq!(result.event.value, 3);
            assert_eq!(result.event.memo.unwrap().identity, current.identity);
        }
    }
    let actual_calibration = restart.as_ref().map(|restart| {
        // Raw allowance attribution above includes every cancellation observation.
        // The original prefix helper needs only the original admission/semantic events.
        let accounting = records
            .iter()
            .filter(|record| !is_native_check(record))
            .copied()
            .collect::<Vec<_>>();
        let prefix = debit_prefix(&db, input, &accounting, restart.debit, allowance);
        let spent = allowance - restart.debit.event.session.unwrap().remaining;
        eprintln!("NATIVE_TRANSFER {row:?} exact receiver prefix {spent}: {prefix:?}");
        if row == NativeRow::Refusal {
            assert_eq!(prefix, calibration.unwrap().prefix);
            assert_eq!(spent, calibration.unwrap().allowance);
            assert_refusal(&records, keys, &handoff, restart, owners.unwrap());
            let receiver = owners.unwrap()[0];
            for record in records
                .iter()
                .filter(|record| record.worker != 0 && record.ordinal > resumed.ordinal)
            {
                if matches!(
                    record.event.kind,
                    Kind::SeedAllowed | Kind::SeedActive | Kind::SupportAccepted
                ) && (record.event.decision || record.event.kind == Kind::SupportAccepted)
                    && let Some(support) = record.event.support
                {
                    assert_ne!(support.owner, receiver);
                }
            }
        }
        Calibration {
            allowance: spent,
            prefix,
        }
    });
    native_recovery(
        &mut db,
        input,
        baseline,
        row,
        &records,
        owners,
        accepted_local.as_ref(),
    );
    eprintln!("NATIVE_TRANSFER {row:?} controlled={controlled} passed");
    actual_calibration
}

#[test]
fn native_interruption_after_transferred_reclaim() {
    if std::env::var(NATIVE_CHILD).as_deref() != Ok(NATIVE_TEST) {
        super::watchdog(NATIVE_TEST, NATIVE_CHILD);
        return;
    }
    let baseline = ordinary();
    let reference = paired(&baseline, None);
    let calibration = native_row(NativeRow::Calibration, true, &baseline, None).unwrap();
    assert_eq!(calibration.allowance, reference.allowance);
    assert_eq!(calibration.prefix, reference.prefix);
    for controlled in [false, true] {
        native_row(NativeRow::ReceiverLocal, controlled, &baseline, None);
        native_row(NativeRow::DonorLocal, controlled, &baseline, None);
    }
    native_row(NativeRow::ReceiverLocalWaiter, true, &baseline, None);
    native_row(NativeRow::Writer, true, &baseline, None);
    native_row(NativeRow::Refusal, true, &baseline, Some(&calibration));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefusalReplayOrder {
    WaitBeforeVerification,
    VerificationBeforeWait,
}

impl RefusalReplayOrder {
    fn test_name(self) -> &'static str {
        match self {
            Self::WaitBeforeVerification => "function::execute::execution_run::tests::parallel_wait::transfer::transferred_refusal_wait_before_verification",
            Self::VerificationBeforeWait => "function::execute::execution_run::tests::parallel_wait::transfer::transferred_refusal_verification_before_wait",
        }
    }
}

struct ReplayDonorObserver {
    pause: mpsc::Receiver<trace::FinalityPause>,
    installed: Option<trace::FinalityPauseInstalled>,
    waits: mpsc::Sender<(thread::ThreadId, DatabaseKeyIndex)>,
}

thread_local! {
    static REPLAY_DONOR: RefCell<Option<ReplayDonorObserver>> = const { RefCell::new(None) };
}

struct ReplayDonorInstalled;

impl ReplayDonorInstalled {
    fn install(observer: ReplayDonorObserver) -> Self {
        REPLAY_DONOR.with_borrow_mut(|slot| assert!(slot.replace(observer).is_none()));
        Self
    }

    fn finish(self) {
        let observer = REPLAY_DONOR.with_borrow_mut(|slot| slot.take().unwrap());
        assert!(observer.installed.is_some(), "donor installed its exact-head pause");
        drop(observer);
        drop(self);
        assert!(REPLAY_DONOR.with_borrow(|slot| slot.is_none()));
        eprintln!("TRANSFER_REPLAY donor hook retired");
    }
}

impl Drop for ReplayDonorInstalled {
    fn drop(&mut self) {
        REPLAY_DONOR.with_borrow_mut(|slot| *slot = None);
    }
}

fn refusal_replay_event(event: &Event) {
    REPLAY_DONOR.with_borrow_mut(|slot| {
        let Some(observer) = slot.as_mut() else {
            return;
        };
        if matches!(&event.kind, EventKind::WillCheckCancellation)
            && let Ok(pause) = observer.pause.try_recv()
        {
            assert!(observer.installed.is_none());
            observer.installed = Some(trace::install_finality_pause(pause));
        }
        if let EventKind::WillBlockOn { database_key, .. } = &event.kind {
            // This event can retain graph locks. Notification never waits for the controller.
            observer.waits.send((thread::current().id(), *database_key)).unwrap();
        }
    });
}

fn replay_fresh_claim(db: &Db, key: DatabaseKeyIndex, owner: thread::ThreadId) {
    let state = db.zalsa().lookup_ingredient(key.ingredient_index())
        .as_function().unwrap().sync_table().test_transfer_state(key.key_index()).unwrap();
    assert!(matches!(state.owner, SyncOwner::Thread(actual) if actual == owner));
    assert!(!state.claimed_twice);
}

fn replay_installed_wait(db: &Db, waiter: thread::ThreadId, owner: thread::ThreadId) {
    // Taking this snapshot after the notification also waits for block_on to release its locks.
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    assert!(!graph.edges.overflow);
    assert!(graph.edges.entries.into_iter().flatten().any(|edge| edge == (waiter, owner)));
    eprintln!("TRANSFER_REPLAY installed wait {waiter:?} -> {owner:?}: {graph:?}");
}

fn replay_refused_transfer(order: RefusalReplayOrder, baseline: &Baseline, calibration: &Calibration) {
    let mut db = Db {
        storage: crate::Storage::new(Some(Box::new(|event| {
            refusal_replay_event(&event);
            wait_event(event);
        }))),
        counts: Arc::default(),
    };
    let input = Input::new(&db, 3, 17);
    let keys = Keys::new(&db, input);
    let supports = Arc::new([OnceLock::new(), OnceLock::new()]);
    let ordinal = Arc::new(AtomicUsize::new(0));
    let reports = thread::scope(|scope| {
        let [left, right] = schedules();
        let (paused_tx, paused_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release = Release { sender: Some(release_tx), value: Some(()) };
        let (r_tx, r_rx) = mpsc::channel();
        let (d_tx, d_rx) = mpsc::channel();
        let (pause_tx, pause_rx) = mpsc::channel();
        let (donor_wait_tx, donor_wait_rx) = mpsc::channel();
        let r_db = db.clone();
        let d_db = db.clone();
        let r_supports = supports.clone();
        let d_supports = supports.clone();
        let r_ordinal = ordinal.clone();
        let d_ordinal = ordinal.clone();
        let receiver = scope.spawn(move || native_worker(
            r_db, input, left, r_supports, 0, true, calibration.allowance,
            NativeObserver {
                gate: Some(ReclaimGate { key: keys.0[1], entered: paused_tx, release: release_rx }),
                wait: None,
            }, r_tx, r_ordinal, false,
        ));
        let donor = scope.spawn(move || {
            let installed = ReplayDonorInstalled::install(ReplayDonorObserver {
                pause: pause_rx, installed: None, waits: donor_wait_tx,
            });
            let report = native_worker(
                d_db, input, right, d_supports, 1, true, ALLOWANCE,
                NativeObserver { gate: None, wait: None }, d_tx, d_ordinal, false,
            );
            installed.finish();
            report
        });
        let receiver_live = r_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        let donor_live = d_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        let paused = paused_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        assert_eq!(paused.thread, receiver_live.thread);
        installed_reclaim(&db, input, paused, donor_live.thread, None);
        let [old_a, old_b] = keys.0.map(|key| {
            db.zalsa().lookup_ingredient(key.ingredient_index()).as_function().unwrap()
                .memo(db.zalsa(), key.key_index()).unwrap().transfer_test_snapshot()
        });
        assert!(!old_a.final_ && !old_b.final_);
        assert_eq!(old_a.support.unwrap().owner, old_b.support.unwrap().owner);
        assert_eq!(old_b.support.unwrap().owner, trace::support_snapshot(donor_live.support.as_ref().unwrap()).owner);
        let (donor_pause, donor_control) = trace::finality_pause(keys.0[1], old_b.identity, keys.0[0], old_a.identity);
        let (third_pause, third_control) = trace::finality_pause(keys.0[0], old_a.identity, keys.0[0], old_a.identity);
        pause_tx.send(donor_pause).unwrap();
        receiver_live.token.cancel();
        drop(release);
        donor_control.wait_for_selection();
        let receiver_report = receiver.join().unwrap();
        assert_eq!(receiver_report.outcome.as_ref().unwrap(), &Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)));
        replay_fresh_claim(&db, keys.0[1], donor_live.thread);

        let third_supports = Arc::new([OnceLock::new(), OnceLock::new()]);
        third_supports[1].set(receiver_live.support.as_ref().unwrap().clone()).unwrap();
        let (third_tx, third_rx) = mpsc::channel();
        let (third_wait_tx, third_wait_rx) = mpsc::channel();
        let third_db = db.clone();
        let third_ordinal = ordinal.clone();
        let third = scope.spawn(move || {
            let installed = trace::install_finality_pause(third_pause);
            let report = native_worker(
                third_db, input, third_schedule(), third_supports, 2, true, ALLOWANCE,
                NativeObserver {
                    gate: None,
                    wait: (order == RefusalReplayOrder::WaitBeforeVerification).then_some((keys.0[1], third_wait_tx)),
                }, third_tx, third_ordinal, true,
            );
            drop(installed);
            eprintln!("TRANSFER_REPLAY independent hook retired");
            report
        });
        let third_live = third_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        third_control.wait_for_selection();
        replay_fresh_claim(&db, keys.0[0], third_live.thread);
        assert!(!third_live.support.as_ref().unwrap().same_owner(donor_live.support.as_ref().unwrap()));
        eprintln!("TRANSFER_REPLAY {order:?} exact selections A={} B={}", old_a.identity, old_b.identity);

        match order {
            RefusalReplayOrder::WaitBeforeVerification => {
                drop(third_control);
                assert_eq!(third_wait_rx.recv_timeout(STAGE_TIMEOUT).unwrap(), third_live.thread);
                replay_installed_wait(&db, third_live.thread, donor_live.thread);
                drop(donor_control);
            }
            RefusalReplayOrder::VerificationBeforeWait => {
                drop(donor_control);
                assert_eq!(donor_wait_rx.recv_timeout(STAGE_TIMEOUT).unwrap(), (donor_live.thread, keys.0[0]));
                replay_installed_wait(&db, donor_live.thread, third_live.thread);
                drop(third_control);
            }
        }
        vec![receiver_report, donor.join().unwrap(), third.join().unwrap()]
    });
    audit(&db, input);
    let mut records = reports.iter().flat_map(|report| report.report.trace.records.iter().copied()).collect::<Vec<_>>();
    records.sort_unstable_by_key(|record| record.ordinal);
    assert!(records.iter().enumerate().all(|(index, record)| index == record.ordinal));
    assert_native_claims(&records, NativeRow::Refusal);
    native_registered_retries(&records, &reports);
    native_allowances(&reports, None);
    let handoff = handoff(&records, keys);
    let owners = [
        trace::support_snapshot(reports[0].report.support.as_ref().unwrap()).owner,
        trace::support_snapshot(reports[1].report.support.as_ref().unwrap()).owner,
    ];
    let restart = receiver_restart(&records, keys, &handoff, owners);
    let accounting = records.iter().filter(|record| !is_native_check(record)).copied().collect::<Vec<_>>();
    let prefix = debit_prefix(&db, input, &accounting, restart.debit, calibration.allowance);
    assert_eq!(prefix, calibration.prefix);
    assert_eq!(calibration.allowance - restart.debit.event.session.unwrap().remaining, calibration.allowance);
    let returned = native_donor_refetch(&records, keys, &handoff, false);
    let claim = after(&records, returned, "donor newly claims B after receiver refusal", |record| {
        query_event(record, 1, Kind::Claim, keys.0[1])
    });
    assert_eq!(claim.event.mode, Some(Mode::Default));
    assert!(!claim.event.sync.unwrap().claimed_twice);
    let verified = after(&records, claim, "donor verifies its retained B", |record| {
        query_event(record, 1, Kind::Verified, keys.0[1]) && record.event.serial == claim.event.serial
    });
    assert_eq!(verified.event.memo.unwrap().identity, handoff.provisional.event.memo.unwrap().identity);
    let head = after(&records, claim, "donor selects the retained provisional A head", |record| {
        query_event(record, 1, Kind::Verification, keys.0[1]) && record.event.phase == Some("finality.head")
    });
    assert!(!head.event.decision && head.ordinal < verified.ordinal);
    assert_eq!(head.event.other_memo.unwrap().identity, handoff.initial.event.memo.unwrap().identity);
    let terminal = after(&records, verified, "donor releases its new B claim", |record| {
        query_event(record, 1, Kind::Terminal, keys.0[1]) && record.event.serial == claim.event.serial
    });
    let fresh = records.iter().find(|record| record.ordinal > verified.ordinal
        && query_event(record, 1, Kind::PreDebit, keys.0[1]) && record.event.step == Some(Step::Body));
    let transfers = records.iter().filter(|record| record.ordinal > verified.ordinal && record.event.kind == Kind::TransferBegin).count();
    match order {
        RefusalReplayOrder::WaitBeforeVerification => {
            let wait = after(&records, claim, "independent A waits on donor B", |record| {
                query_event(record, 2, Kind::Edge, keys.0[1])
            });
            assert!(wait.ordinal < verified.ordinal);
        }
        RefusalReplayOrder::VerificationBeforeWait => {
            assert!(!verified.event.decision);
            let fresh = fresh.expect("rejected provisional B executes a fresh body");
            let wait = after(&records, fresh, "fresh donor B waits on independent A", |record| {
                query_event(record, 1, Kind::Edge, keys.0[0])
            });
            assert!(verified.ordinal < fresh.ordinal && fresh.ordinal < wait.ordinal);
            assert!(transfers > 0, "the restarted component transfers a participant claim");
        }
    }
    eprintln!("TRANSFER_REPLAY {order:?} fresh_claim={:?} serial={:?} verified={} release={:?} fresh_body={} transfers={transfers}",
        claim.event.mode, claim.event.serial, verified.event.decision, terminal.event.mode, fresh.is_some());
    for (worker, report) in reports.iter().enumerate() {
        if worker == 0 {
            assert_eq!(report.outcome.as_ref().unwrap(), &Ok(AttemptOutcome::Incomplete(Incomplete::Allowance)));
        } else {
            assert_eq!(report.outcome.as_ref().unwrap(), &Ok(AttemptOutcome::Complete(Ok(3))), "{order:?} worker={worker}");
        }
    }
    match order {
        RefusalReplayOrder::WaitBeforeVerification => assert_refusal_progress(
            &records, keys, &handoff, &restart, owners, DonorProgress::CheckedRetirement(claim),
        ),
        RefusalReplayOrder::VerificationBeforeWait => {
            assert_refusal(&records, keys, &handoff, &restart, owners);
        }
    }
    native_recovery(&mut db, input, baseline, NativeRow::Refusal, &records, Some(owners), None);
    eprintln!("TRANSFER_REPLAY {order:?} passed");
}

fn refusal_replay_control(order: RefusalReplayOrder) {
    const CHILD: &str = "SALSA_TRANSFERRED_REFUSAL_REPLAY_CHILD";
    if std::env::var(CHILD).as_deref() != Ok(order.test_name()) {
        super::watchdog(order.test_name(), CHILD);
        return;
    }
    let baseline = ordinary();
    let reference = paired(&baseline, None);
    let calibration = native_row(NativeRow::Calibration, true, &baseline, None).unwrap();
    assert_eq!(calibration.allowance, reference.allowance);
    assert_eq!(calibration.prefix, reference.prefix);
    replay_refused_transfer(order, &baseline, &calibration);
}

#[test]
fn transferred_refusal_wait_before_verification() {
    refusal_replay_control(RefusalReplayOrder::WaitBeforeVerification);
}

#[test]
fn transferred_refusal_verification_before_wait() {
    refusal_replay_control(RefusalReplayOrder::VerificationBeforeWait);
}

mod retained_wrapper;
