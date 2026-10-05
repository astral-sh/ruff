use super::*;
use crate::function::FunctionIngredient;

const WRITER_TEST: &str = "function::execute::execution_run::tests::parallel_wait::native_cancellation::pending_writer_interrupts_installed_waits";
const VALIDATION_TEST: &str = "function::execute::execution_run::tests::parallel_wait::native_cancellation::installed_validation_native_interruption";
const CHILD: &str = "SALSA_INSTALLED_NATIVE_INTERRUPTION_CHILD";

/// The last observer handle must drop before the scoped writer can be joined, including
/// when an assertion unwinds. An unstarted writer is also released during teardown.
pub(super) struct WriterLease<D> {
    pub(super) db: Option<D>,
    pub(super) start: Option<mpsc::Sender<()>>,
}

impl<D> WriterLease<D> {
    pub(super) fn start(&mut self) {
        self.start.take().unwrap().send(()).unwrap();
    }
}

impl<D> Drop for WriterLease<D> {
    fn drop(&mut self) {
        if let Some(start) = self.start.take() {
            let _ = start.send(());
        }
        drop(self.db.take());
    }
}

pub(super) struct Release<T> {
    pub(super) sender: Option<mpsc::Sender<T>>,
    pub(super) value: Option<T>,
}

impl<T> Drop for Release<T> {
    fn drop(&mut self) {
        if let (Some(sender), Some(value)) = (self.sender.take(), self.value.take()) {
            let _ = sender.send(value);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Interruption {
    PendingFetch,
    PendingValidation,
    OwnerLocal,
    WaiterLocal,
    OwnerPanic,
}

impl Interruption {
    fn validation(self) -> bool {
        self != Self::PendingFetch
    }
    fn writer(self) -> bool {
        matches!(self, Self::PendingFetch | Self::PendingValidation)
    }
    fn edited(self) -> u32 {
        if self.validation() { 14 } else { 9 }
    }
}

enum Resume {
    Continue,
    Panic(Arc<()>),
}

struct ValueGate {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<Resume>,
}

#[derive(Clone, Copy, Debug)]
struct Waiting {
    thread: ThreadId,
    key: DatabaseKeyIndex,
    session: SessionSnapshot,
    boundary: TraceEvent,
}

struct Observer {
    gate: Option<ValueGate>,
    wait: Option<mpsc::Sender<Waiting>>,
}

thread_local! {
    static OBSERVER: RefCell<Option<Observer>> = const { RefCell::new(None) };
}

struct ObserverInstalled;
impl Drop for ObserverInstalled {
    fn drop(&mut self) {
        OBSERVER.with_borrow_mut(|observer| *observer = None);
    }
}

pub(super) fn before_value(db: &dyn TestDatabase, key: DatabaseKeyIndex) {
    let gate = OBSERVER.with_borrow_mut(|observer| observer.as_mut()?.gate.take());
    let Some(gate) = gate else {
        return;
    };
    assert_eq!(
        db.zalsa_local().active_query().map(|(key, _)| key),
        Some(key)
    );
    wait_phase("native.owner.paused", key);
    gate.entered.send(()).unwrap();
    match gate
        .release
        .recv_timeout(STAGE_TIMEOUT)
        .expect("native owner release stage")
    {
        Resume::Continue => wait_phase("native.owner.resume", key),
        Resume::Panic(identity) => panic_any(OwnerPanic(identity)),
    }
}

pub(super) fn observe_event(event: &Event) {
    if OBSERVER.with_borrow(|observer| observer.is_some())
        && matches!(&event.kind, EventKind::WillCheckCancellation)
    {
        crate::with_attached_database(|db| {
            let mut observation = Observation::new(Kind::Gate);
            observation.phase = Some("native.check");
            observation.key = db.zalsa_local().active_query().map(|(key, _)| key);
            observation.decision = db.cancellation_token().is_cancelled();
            observation.units = usize::from(db.zalsa_local().should_trigger_local_cancellation());
            transfer_trace::record(observation);
        });
    }
    let EventKind::WillBlockOn { database_key, .. } = &event.kind else {
        return;
    };
    let waiting = OBSERVER.with_borrow_mut(|observer| observer.as_mut()?.wait.take());
    if let Some(waiting) = waiting {
        // This callback holds native locks: copy observations and send, but never wait.
        waiting
            .send(Waiting {
                thread: thread::current().id(),
                key: *database_key,
                session: transfer_trace::session_snapshot().unwrap(),
                boundary: validation_trace::latest_boundary().unwrap(),
            })
            .unwrap();
    }
}

struct Live {
    thread: ThreadId,
    token: CancellationToken,
    support: AttemptSupport,
}

pub(super) type NativeOutcome = thread::Result<Result<AttemptOutcome<RunResult<u32>>, StartError>>;

struct Worker {
    outcome: NativeOutcome,
    support: AttemptSupport,
    exit: SessionSnapshot,
    validation: Vec<TraceEvent>,
    trace: TransferTrace,
}

fn worker(
    db: Db,
    input: Input,
    row: Interruption,
    owner: bool,
    observer: Observer,
    live: mpsc::Sender<Live>,
    config: TraceConfig,
) -> Worker {
    let token = db.cancellation_token();
    assert!(!token.is_cancelled());
    let installed = ObserverInstalled;
    OBSERVER.with_borrow_mut(|slot| assert!(slot.replace(observer).is_none()));
    let support = RefCell::new(None);
    let exit = Cell::new(None);
    let ((outcome, validation), trace) = transfer_trace::collect(config, || {
        validation_trace::collect(|| {
            catch_unwind(AssertUnwindSafe(|| {
                attempt_probe::try_with_attempt(&db, WAIT_ALLOWANCE, || {
                    let _exit = ExitSession(&exit);
                    let own = attempt_probe::current().unwrap();
                    *support.borrow_mut() = Some(own.clone());
                    live.send(Live {
                        thread: thread::current().id(),
                        token: token.clone(),
                        support: own,
                    })
                    .unwrap();
                    let key = if !owner && row.validation() {
                        consumer::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id())
                    } else {
                        value::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id())
                    };
                    wait_phase("native.entry", key);
                    let (outbound, _) = mpsc::channel();
                    let schedule = Schedule {
                        owner,
                        case: if row.validation() {
                            Case::Validation
                        } else {
                            Case::Cold
                        },
                        outbound,
                        inbound: RefCell::new(None),
                        body_entered: Cell::new(false),
                        identity: Arc::new(()),
                        installed_wait: None,
                    };
                    let result = run_queries_with_admission(
                        &db,
                        value::fn_ingredient_(&db, db.zalsa()),
                        consumer::fn_ingredient_(&db, db.zalsa()),
                        input,
                        &schedule,
                        &ObservedAdmission { db: &db },
                    );
                    if let Ok(value) = result {
                        let mut observation = Observation::new(Kind::RootResult).key(key);
                        observation.value = value;
                        transfer_trace::record(observation);
                    }
                    result
                })
            }))
        })
    });
    drop(installed);
    emit_native(&format!("{row:?} owner={owner}"), &outcome);
    for record in &trace.records {
        eprintln!("NATIVE {row:?} owner={owner} {record:?}");
    }
    for (ordinal, event) in validation.iter().enumerate() {
        eprintln!("NATIVE_VALIDATION {row:?} owner={owner} ordinal={ordinal} {event:?}");
    }
    assert_worker_clean(&db);
    assert!(crate::with_attached_database(|_| ()).is_none());
    assert!(
        !token.is_cancelled(),
        "the registration attachment owns token reset"
    );
    let support = support.into_inner().unwrap();
    assert!(!support.owns_current_session(db.zalsa()));
    let exit = exit.get().unwrap();
    assert_eq!(
        exit.support.owner,
        transfer_trace::support_snapshot(&support).owner
    );
    assert!(!trace.broken);
    eprintln!(
        "NATIVE {row:?} owner={owner} exit={exit:?} support={:?}",
        transfer_trace::support_snapshot(&support)
    );
    validation_trace::emit(&format!("native.{row:?}.{owner}"), &validation);
    // Neither report nor unwind payload owns the handle which the writer must drain.
    drop(db);
    Worker {
        outcome,
        support,
        exit,
        validation,
        trace,
    }
}

pub(super) fn emit_native(label: &str, outcome: &NativeOutcome) {
    match outcome {
        Ok(result) => eprintln!("NATIVE_OUTCOME {label}: {result:?}"),
        Err(payload) => {
            if let Some(cancelled) = payload.downcast_ref::<Cancelled>() {
                eprintln!("NATIVE_OUTCOME {label}: {cancelled:?}");
            } else if let Some(OwnerPanic(identity)) = payload.downcast_ref::<OwnerPanic>() {
                eprintln!(
                    "NATIVE_OUTCOME {label}: OwnerPanic({:p})",
                    Arc::as_ptr(identity)
                );
            } else {
                eprintln!(
                    "NATIVE_OUTCOME {label}: unexpected panic {:?}",
                    payload.as_ref().type_id()
                );
            }
        }
    }
}

pub(super) fn assert_native(outcome: &NativeOutcome, expected: &str, identity: Option<&Arc<()>>) {
    let payload = outcome
        .as_ref()
        .expect_err("the actual native payload must escape the evaluation");
    match expected {
        "Local" => assert!(matches!(
            payload.downcast_ref::<Cancelled>(),
            Some(Cancelled::Local)
        )),
        "PendingWrite" => assert!(matches!(
            payload.downcast_ref::<Cancelled>(),
            Some(Cancelled::PendingWrite)
        )),
        "PropagatedPanic" => assert!(matches!(
            payload.downcast_ref::<Cancelled>(),
            Some(Cancelled::PropagatedPanic)
        )),
        "OwnerPanic" => assert!(Arc::ptr_eq(
            &payload.downcast_ref::<OwnerPanic>().unwrap().0,
            identity.unwrap()
        )),
        _ => panic!("unrecognized fixture native payload"),
    }
}

fn memo<C: Configuration>(
    db: &Db,
    ingredient: &IngredientImpl<C>,
    input: Input,
) -> Option<MemoSnapshot> {
    ingredient
        .get_memo_from_table_for(
            db.zalsa(),
            input.as_id(),
            ingredient.memo_ingredient_index(db.zalsa(), input.as_id()),
        )
        .map(|memo| memo.transfer_test_snapshot())
}

fn installed(
    db: &Db,
    input: Input,
    owner: ThreadId,
    waiting: Waiting,
    validation: bool,
) -> WaitProof {
    let value_key = value::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    let consumer_key = consumer::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    assert_eq!(waiting.key, value_key);
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    let claim = value::fn_ingredient_(db, db.zalsa())
        .sync_table()
        .test_transfer_state(input.as_id())
        .unwrap();
    let proof = WaitProof {
        graph,
        claim,
        owner,
        waiter: waiting.thread,
    };
    proof.assert_installed(value_key);
    let expected_phase = if validation {
        "validation.wait"
    } else {
        "fetch.wait"
    };
    // The suspended validator retains its claim without installing a query frame.
    // Its operation and sync owner, rather than an active caller, identify this wait.
    assert!(
        matches!(waiting.boundary, TraceEvent::Outer { phase, key: Some(key), operation, owner, query_depth, operation_depth }
        if key == value_key && phase == expected_phase && owner.is_none() && query_depth == 0
        && operation == usize::from(validation) && operation_depth == 1 + usize::from(validation))
    );
    if validation {
        let state = db
            .zalsa()
            .lookup_ingredient(consumer_key.ingredient_index())
            .as_function()
            .unwrap()
            .sync_table()
            .test_transfer_state(input.as_id())
            .unwrap();
        assert!(matches!(state.owner, SyncOwner::Thread(thread) if thread == waiting.thread));
    }
    eprintln!("NATIVE installed {proof:?}; waiting={waiting:?}");
    proof
}

pub(super) fn is_native_check(record: &Record) -> bool {
    record.event.kind == Kind::Gate && record.event.phase == Some("native.check")
}

/// Cancellation checks can observe a completed debit before its admission report.
/// The intervening checks retain the entire snapshot; cancellation cleanup may mark
/// it Interrupted without changing the session identity, allowance, or scope.
pub(super) fn after_native_checks(records: &[Record], first: usize) -> &Record {
    let initial = &records[first];
    assert!(is_native_check(initial) || initial.event.kind == Kind::Admission);
    let session = initial.event.session.unwrap();
    for record in &records[first..] {
        let mut expected = session;
        if !is_native_check(record)
            && record.event.kind != Kind::Admission
            && record.event.session != Some(session)
        {
            assert_eq!(session.support.state, 0);
            assert!(!session.support.explicitly_incomplete);
            expected.support.state = 4;
            expected.support.explicitly_incomplete = true;
        }
        assert_eq!(record.event.session, Some(expected));
        if !is_native_check(record) {
            return record;
        }
    }
    panic!("a recorded debit has no subsequent admission or interruption witness");
}

fn assert_claims(worker: &Worker) {
    for claim in worker
        .trace
        .records
        .iter()
        .filter(|record| record.event.kind == Kind::Claim)
    {
        let terminals = worker
            .trace
            .records
            .iter()
            .filter(|record| {
                record.event.kind == Kind::Terminal && record.event.serial == claim.event.serial
            })
            .collect::<Vec<_>>();
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].event.key, claim.event.key);
        assert!(claim.ordinal < terminals[0].ordinal);
    }
    let mut remaining = WAIT_ALLOWANCE;
    for (index, record) in worker.trace.records.iter().enumerate() {
        if let Some(session) = record.event.session {
            assert_eq!(session.support.owner, worker.exit.support.owner);
            assert!(session.remaining <= remaining);
            if session.remaining < remaining {
                let admission = after_native_checks(&worker.trace.records, index);
                assert_eq!(admission.event.kind, Kind::Admission);
                assert_eq!(admission.event.phase, Some("observed.work"));
                assert_eq!(remaining - session.remaining, admission.event.units);
            }
            remaining = session.remaining;
        }
    }
    assert_eq!(remaining, worker.exit.remaining);
}

fn assert_history(
    row: Interruption,
    db: &Db,
    input: Input,
    waiting: Waiting,
    workers: &[Worker; 2],
    identity: &Arc<()>,
) {
    let [owner, waiter] = workers;
    assert!(!owner.support.same_owner(&waiter.support));
    assert_eq!(waiting.session.support.owner, waiter.exit.support.owner);
    assert_eq!(waiting.session.remaining, WAIT_ALLOWANCE);
    for worker in workers {
        assert_claims(worker);
    }
    let consumed = one_observation(&waiter.trace.records, Kind::WaitConsumed);
    let edge = one_observation(&waiter.trace.records, Kind::Edge);
    let terminal = one_observation(&owner.trace.records, Kind::Terminal);
    assert!(edge.ordinal < terminal.ordinal && terminal.ordinal < consumed.ordinal);
    assert_eq!(consumed.event.session.unwrap(), waiting.session);
    let normal_owner = row == Interruption::WaiterLocal;
    let surviving_waiter = row == Interruption::OwnerLocal;
    let expected_wait = if row == Interruption::OwnerPanic {
        "Panicked"
    } else if row.writer() || surviving_waiter {
        "Cancelled"
    } else {
        "Completed"
    };
    for record in [terminal, consumed] {
        assert!(matches!(
            (record.event.wait, expected_wait),
            (Some(WaitResult::Panicked), "Panicked")
                | (Some(WaitResult::Cancelled), "Cancelled")
                | (Some(WaitResult::Completed), "Completed")
        ));
    }
    assert_eq!(
        terminal.event.action,
        Some(if normal_owner {
            Action::Drop
        } else if row == Interruption::OwnerPanic {
            Action::Panic
        } else {
            Action::Abort
        })
    );
    if !normal_owner {
        assert!(owner.trace.records.iter().all(|record| {
            record.event.kind != Kind::RootResult
                && !(record.event.kind == Kind::RootPublished
                    && record
                        .event
                        .memo
                        .is_some_and(|memo| memo.has_value && memo.final_))
        }));
    }
    if normal_owner {
        assert_eq!(
            owner.outcome.as_ref().unwrap(),
            &Ok(AttemptOutcome::Complete(Ok(9)))
        );
    } else {
        assert_native(
            &owner.outcome,
            if row.writer() {
                "PendingWrite"
            } else if surviving_waiter {
                "Local"
            } else {
                "OwnerPanic"
            },
            Some(identity),
        );
    }
    if surviving_waiter {
        assert_eq!(
            waiter.outcome.as_ref().unwrap(),
            &Ok(AttemptOutcome::Complete(Ok(10)))
        );
    } else {
        assert_native(
            &waiter.outcome,
            if normal_owner {
                "Local"
            } else if row.writer() {
                "PendingWrite"
            } else {
                "PropagatedPanic"
            },
            None,
        );
    }
    let owner_work = INPUT_CONVERSION_WORK
        + if normal_owner {
            SCALAR_COMPARISON_WORK + 2 + ROOT_READ_WORK
        } else {
            0
        };
    assert_eq!(owner.exit.remaining, WAIT_ALLOWANCE - owner_work);
    // The surviving waiter retries validation, then recomputes and replaces both old memos.
    // Reading the child into the consumer's reused empty dependency set costs 65 units.
    let waiter_work = usize::from(surviving_waiter)
        * (1 + 2 * (INPUT_CONVERSION_WORK + SCALAR_COMPARISON_WORK + 2) + 65 + ROOT_READ_WORK);
    assert_eq!(waiter.exit.remaining, WAIT_ALLOWANCE - waiter_work);
    assert_eq!(
        transfer_trace::support_snapshot(&owner.support).state,
        if normal_owner { 2 } else { 3 }
    );
    assert_eq!(
        transfer_trace::support_snapshot(&waiter.support).state,
        if surviving_waiter { 2 } else { 3 }
    );
    for (worker, interrupted) in [
        (owner, row.writer() || surviving_waiter),
        (waiter, row.writer() || normal_owner),
    ] {
        assert_eq!(worker.exit.support.state, if interrupted { 4 } else { 0 });
        assert_eq!(worker.exit.support.explicitly_incomplete, interrupted);
    }
    let value_key = value::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    let consumer_key = consumer::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id());
    if row.validation() {
        assert!(
            waiter.validation.iter().any(|event| matches!(event,
            TraceEvent::Request { owner, key, .. } if *owner == consumer_key && *key == value_key))
        );
        let retained_claim = waiter
            .trace
            .records
            .iter()
            .find(|record| {
                record.event.kind == Kind::Claim && record.event.key == Some(consumer_key)
            })
            .unwrap();
        let enclosing = waiter
            .trace
            .records
            .iter()
            .find(|record| {
                record.event.kind == Kind::Terminal
                    && record.event.serial == retained_claim.event.serial
            })
            .unwrap();
        assert!(consumed.ordinal < enclosing.ordinal);
        // No ordinary operation scope was installed. Registered query operations
        // retain separate ordinals, observed below when their tasks retire.
        assert_eq!(
            [
                retained_claim.event.session.unwrap().scope,
                waiting.session.scope,
                enclosing.event.session.unwrap().scope,
                waiter.exit.scope
            ],
            [None; 4]
        );
        if surviving_waiter {
            assert_wait_retry(&waiter.validation, "validation.wait", "probe", value_key);
            let child = waiter
                .trace
                .records
                .iter()
                .find(|record| {
                    record.event.kind == Kind::Terminal && record.event.key == Some(value_key)
                })
                .unwrap();
            assert!(child.ordinal < enclosing.ordinal);
        } else {
            let wait = waiter.validation.iter().position(|event| matches!(event,
                TraceEvent::Outer { phase: "validation.wait", key: Some(key), .. } if *key == value_key)).unwrap();
            let retired = waiter.validation[wait + 1..]
                .iter()
                .filter_map(|event| match *event {
                    TraceEvent::Outer {
                        phase: phase @ ("panic.drop" | "abort.marked" | "abort.released"),
                        key,
                        operation,
                        owner,
                        query_depth,
                        operation_depth,
                    } => Some((phase, operation, key, owner, query_depth, operation_depth)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let expected = if row == Interruption::OwnerPanic {
                vec![
                    ("panic.drop", 1, None, None, 0, 2),
                    ("panic.drop", 0, None, None, 0, 1),
                ]
            } else {
                vec![
                    ("abort.marked", 1, None, None, 0, 2),
                    ("abort.released", 1, None, None, 0, 2),
                    ("abort.marked", 0, None, None, 0, 1),
                    ("abort.released", 0, None, None, 0, 1),
                ]
            };
            assert_eq!(retired, expected,
                "the child validation operation must retire before its retained consumer operation");
            assert!(waiter.validation[wait + 1..].iter().all(|event| !matches!(
                event,
                TraceEvent::Reply { .. }
                    | TraceEvent::Result {
                        outcome: "complete",
                        ..
                    }
            )));
            assert!(waiter.trace.records.iter().all(|record| !matches!(
                record.event.kind,
                Kind::RootResult | Kind::RootPublished
            )));
        }
    }
}

fn fresh_consumer(db: &Db, input: Input) -> u32 {
    let (outbound, _) = mpsc::channel();
    let schedule = Schedule {
        owner: false,
        case: Case::Validation,
        outbound,
        inbound: RefCell::new(None),
        body_entered: Cell::new(false),
        identity: Arc::new(()),
        installed_wait: None,
    };
    let result = attempt_probe::try_with_attempt(db, WAIT_ALLOWANCE, || {
        run_queries(
            db,
            value::fn_ingredient_(db, db.zalsa()),
            consumer::fn_ingredient_(db, db.zalsa()),
            input,
            &schedule,
        )
    });
    let Ok(AttemptOutcome::Complete(Ok(result))) = result else {
        panic!("fresh recovery failed: {result:?}");
    };
    result
}

fn check_recovery(
    db: &mut Db,
    input: Input,
    baseline: &mut Db,
    baseline_input: Input,
    expected: u32,
    later_edit: bool,
) {
    assert_eq!(fresh_consumer(db, input), expected + 1);
    assert_eq!(value(db, input), expected);
    assert_eq!(consumer(db, input), consumer(baseline, baseline_input));
    for (actual, ordinary) in [
        (
            dependencies(db, value::fn_ingredient_(db, db.zalsa()), input),
            dependencies(
                baseline,
                value::fn_ingredient_(baseline, baseline.zalsa()),
                baseline_input,
            ),
        ),
        (
            dependencies(db, consumer::fn_ingredient_(db, db.zalsa()), input),
            dependencies(
                baseline,
                consumer::fn_ingredient_(baseline, baseline.zalsa()),
                baseline_input,
            ),
        ),
    ] {
        assert_eq!(actual, ordinary);
    }
    let old = [
        memo(db, value::fn_ingredient_(db, db.zalsa()), input).unwrap(),
        memo(db, consumer::fn_ingredient_(db, db.zalsa()), input).unwrap(),
    ];
    let counts = db.counts.snapshot();
    assert_eq!(fresh_consumer(db, input), expected + 1);
    assert_eq!(db.counts.snapshot(), counts);
    let now = [
        memo(db, value::fn_ingredient_(db, db.zalsa()), input).unwrap(),
        memo(db, consumer::fn_ingredient_(db, db.zalsa()), input).unwrap(),
    ];
    for (old, now) in old.into_iter().zip(now) {
        assert_eq!(old.identity, now.identity);
        assert!(now.final_ && now.has_value);
        assert_eq!(now.verified_at, db.zalsa().current_revision());
    }
    if later_edit {
        input.set_number(db).to(14);
        baseline_input.set_number(baseline).to(14);
        check_recovery(db, input, baseline, baseline_input, 14, false);
    }
}

fn run(row: Interruption) {
    eprintln!("NATIVE row {row:?} starts");
    let (flag_tx, flag_rx) = mpsc::channel();
    let armed = Arc::new(AtomicBool::new(false));
    let event_armed = armed.clone();
    let mut db = Db {
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
    let input = Input::new(&db, 4);
    let mut baseline = Db::default();
    let baseline_input = Input::new(&baseline, 4);
    let mut old_consumer = None;
    let mut old_value = None;
    let mut old_dependencies = Vec::new();
    if row.validation() {
        assert_eq!(consumer(&db, input), 5);
        assert_eq!(consumer(&baseline, baseline_input), 5);
        old_consumer = memo(&db, consumer::fn_ingredient_(&db, db.zalsa()), input);
        old_value = memo(&db, value::fn_ingredient_(&db, db.zalsa()), input);
        old_dependencies = dependencies(&db, consumer::fn_ingredient_(&db, db.zalsa()), input);
        input.set_number(&mut db).to(9);
        baseline_input.set_number(&mut baseline).to(9);
    }
    let _ = value::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id());
    let _ = consumer::fn_ingredient_(&db, db.zalsa());
    let revision = db.zalsa().current_revision();
    let epoch = db.zalsa().runtime().cancellation_count();
    let before = db.counts.snapshot();
    let identity = Arc::new(());
    let ordinal = Arc::new(AtomicUsize::new(0));
    let mut recovered = thread::scope(|scope| {
        let mut lease = WriterLease {
            db: Some(db),
            start: None,
        };
        let owner_db = lease.db.as_ref().unwrap().clone();
        let waiter_db = lease.db.as_ref().unwrap().clone();
        let (start_tx, start_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let writer = if row.writer() {
            let mut writer_db = lease.db.as_ref().unwrap().clone();
            lease.start = Some(start_tx);
            Some(scope.spawn(move || {
                start_rx
                    .recv_timeout(STAGE_TIMEOUT)
                    .expect("writer start stage");
                armed.store(true, Ordering::SeqCst);
                input.set_number(&mut writer_db).to(row.edited());
                armed.store(false, Ordering::SeqCst);
                let _ = done_tx.send(());
                writer_db
            }))
        } else {
            None
        };
        let (owner_tx, owner_rx) = mpsc::channel();
        let (waiter_tx, waiter_rx) = mpsc::channel();
        let (paused_tx, paused_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (wait_tx, wait_rx) = mpsc::channel();
        let release = Release {
            sender: Some(release_tx),
            value: Some(if row == Interruption::OwnerPanic {
                Resume::Panic(identity.clone())
            } else {
                Resume::Continue
            }),
        };
        let owner_ordinal = ordinal.clone();
        let owner = scope.spawn(move || {
            worker(
                owner_db,
                input,
                row,
                true,
                Observer {
                    gate: Some(ValueGate {
                        entered: paused_tx,
                        release: release_rx,
                    }),
                    wait: None,
                },
                owner_tx,
                TraceConfig {
                    worker: 0,
                    ordinal: owner_ordinal,
                },
            )
        });
        let owner_live = owner_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        paused_rx
            .recv_timeout(STAGE_TIMEOUT)
            .expect("actual owner callback pause");
        let waiter_ordinal = ordinal.clone();
        let waiter = scope.spawn(move || {
            worker(
                waiter_db,
                input,
                row,
                false,
                Observer {
                    gate: None,
                    wait: Some(wait_tx),
                },
                waiter_tx,
                TraceConfig {
                    worker: 1,
                    ordinal: waiter_ordinal,
                },
            )
        });
        let waiter_live = waiter_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        let waiting = wait_rx
            .recv_timeout(STAGE_TIMEOUT)
            .expect("actual wait callback");
        let observer = lease.db.as_ref().unwrap();
        installed(
            observer,
            input,
            owner_live.thread,
            waiting,
            row.validation(),
        );
        assert_eq!(waiting.thread, waiter_live.thread);
        assert!(!owner_live.support.same_owner(&waiter_live.support));
        if let Some(old) = old_consumer {
            let retained = memo(
                observer,
                consumer::fn_ingredient_(observer, observer.zalsa()),
                input,
            )
            .unwrap();
            assert_eq!(
                (retained.identity, retained.verified_at),
                (old.identity, old.verified_at)
            );
            assert_eq!(
                dependencies(
                    observer,
                    consumer::fn_ingredient_(observer, observer.zalsa()),
                    input
                ),
                old_dependencies
            );
            eprintln!(
                "NATIVE {row:?} pre-edit value={old_value:?}, consumer={old:?}, origins={old_dependencies:?}"
            );
        }
        if row.writer() {
            lease.start();
            flag_rx
                .recv_timeout(STAGE_TIMEOUT)
                .expect("real setter cancellation event");
            let observer = lease.db.as_ref().unwrap();
            assert!(observer.zalsa().runtime().load_cancellation_flag());
            assert_eq!(observer.zalsa().current_revision(), revision);
            assert_eq!(observer.zalsa().runtime().cancellation_count(), epoch);
            assert!(!owner_live.token.is_cancelled() && !waiter_live.token.is_cancelled());
            if row == Interruption::PendingFetch {
                let late_support = RefCell::new(None);
                let late_exit = Cell::new(None);
                let (late, trace) = transfer_trace::collect(
                    TraceConfig {
                        worker: 2,
                        ordinal: ordinal.clone(),
                    },
                    || {
                        catch_unwind(AssertUnwindSafe(|| {
                            attempt_probe::try_with_attempt(observer, 7, || {
                                let _exit = ExitSession(&late_exit);
                                *late_support.borrow_mut() = attempt_probe::current();
                                RegistryBuilder::new(observer, &ObservedAdmission { db: observer })
                                    .map(|_| ())
                            })
                        }))
                    },
                );
                assert!(matches!(
                    late.unwrap_err().downcast_ref::<Cancelled>(),
                    Some(Cancelled::PendingWrite)
                ));
                assert_eq!(
                    transfer_trace::support_snapshot(late_support.borrow().as_ref().unwrap()).state,
                    3
                );
                assert_eq!(late_exit.get().unwrap().remaining, 7);
                assert!(!trace.broken);
                for record in &trace.records {
                    eprintln!("NATIVE late entrant {record:?}");
                }
                assert!(trace.records.iter().all(|record| !matches!(
                    record.event.kind,
                    Kind::Claim | Kind::BodyValue | Kind::RootResult | Kind::RootPublished
                )));
                assert_worker_clean(observer);
                assert_eq!(observer.counts.snapshot(), before);
                installed(
                    observer,
                    input,
                    owner_live.thread,
                    waiting,
                    row.validation(),
                );
            }
        } else if row == Interruption::OwnerLocal {
            owner_live.token.cancel();
            assert!(owner_live.token.is_cancelled() && !waiter_live.token.is_cancelled());
        } else if row == Interruption::WaiterLocal {
            waiter_live.token.cancel();
            assert!(waiter_live.token.is_cancelled() && !owner_live.token.is_cancelled());
            installed(
                lease.db.as_ref().unwrap(),
                input,
                owner_live.thread,
                waiting,
                true,
            );
        } else {
            assert!(!owner_live.token.is_cancelled() && !waiter_live.token.is_cancelled());
        }
        eprintln!("NATIVE {row:?} trigger proven; releasing owner");
        drop(release);
        let reports = [owner.join().unwrap(), waiter.join().unwrap()];
        let observer = lease.db.as_ref().unwrap();
        assert_history(row, observer, input, waiting, &reports, &identity);
        assert_installed_graph_clean(observer.zalsa().runtime().test_transfer_graph_snapshot());
        assert_claim_released(
            observer,
            value::fn_ingredient_(observer, observer.zalsa()),
            input,
        );
        assert_claim_released(
            observer,
            consumer::fn_ingredient_(observer, observer.zalsa()),
            input,
        );
        assert_exclusion_released(observer);
        let retained_value = memo(
            observer,
            value::fn_ingredient_(observer, observer.zalsa()),
            input,
        );
        eprintln!("NATIVE {row:?} interrupted value={retained_value:?}; old={old_value:?}");
        if matches!(row, Interruption::OwnerLocal | Interruption::WaiterLocal) {
            let accepted = retained_value.unwrap();
            assert!(accepted.has_value && accepted.final_);
            assert_eq!(accepted.verified_at, revision);
            assert_ne!(accepted.identity, old_value.unwrap().identity);
        } else if let Some(retained) = retained_value {
            assert!(
                !retained.has_value || retained.verified_at < revision,
                "unfinished owner published a current result: {retained:?}"
            );
        }
        if row != Interruption::OwnerLocal
            && let Some(old) = old_consumer
        {
            let retained = memo(
                observer,
                consumer::fn_ingredient_(observer, observer.zalsa()),
                input,
            )
            .unwrap();
            assert_eq!(
                (retained.identity, retained.verified_at, retained.changed_at),
                (old.identity, old.verified_at, old.changed_at)
            );
            assert_eq!(
                dependencies(
                    observer,
                    consumer::fn_ingredient_(observer, observer.zalsa()),
                    input
                ),
                old_dependencies
            );
        }
        let expected = match row {
            Interruption::OwnerLocal => [before[0] + 1, before[1] + 1],
            Interruption::WaiterLocal => [before[0] + 1, before[1]],
            _ => before,
        };
        assert_eq!(observer.counts.snapshot(), expected);
        if let Some(writer) = writer {
            assert_eq!(observer.zalsa().current_revision(), revision);
            assert_eq!(observer.zalsa().runtime().cancellation_count(), epoch);
            assert!(matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)));
            eprintln!("NATIVE {row:?} readers dropped; final observer handle drops now");
            drop(lease.db.take());
            let returned = writer.join().unwrap();
            done_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
            returned
        } else {
            lease.db.take().unwrap()
        }
    });
    if row.writer() {
        assert!(recovered.zalsa().current_revision() > revision);
        assert_eq!(recovered.zalsa().runtime().cancellation_count(), 0);
        assert!(!recovered.zalsa().runtime().load_cancellation_flag());
        assert_eq!(input.number(&recovered), row.edited());
        baseline_input.set_number(&mut baseline).to(row.edited());
    }
    let expected = if row.writer() { row.edited() } else { 9 };
    check_recovery(
        &mut recovered,
        input,
        &mut baseline,
        baseline_input,
        expected,
        !row.writer(),
    );
    eprintln!("NATIVE {row:?} passed");
}

#[test]
fn pending_writer_interrupts_installed_waits() {
    if std::env::var(CHILD).as_deref() != Ok(WRITER_TEST) {
        watchdog(WRITER_TEST, CHILD);
        return;
    }
    run(Interruption::PendingFetch);
    run(Interruption::PendingValidation);
}

#[test]
fn installed_validation_native_interruption() {
    if std::env::var(CHILD).as_deref() != Ok(VALIDATION_TEST) {
        watchdog(VALIDATION_TEST, CHILD);
        return;
    }
    for row in [
        Interruption::OwnerLocal,
        Interruption::WaiterLocal,
        Interruption::OwnerPanic,
    ] {
        run(row);
    }
}
