use std::any::Any;
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use crate::attempt_probe::transfer_test_support::{
    self as trace, Event as Observation, GraphSnapshot, Kind, Mode, TraceConfig, TransferTrace,
};
use crate::function::SyncOwner;
use crate::plumbing::AsId;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseKeyIndex, Event, EventKind, Id};

const DEADLINE: Duration = Duration::from_secs(10);
const W: usize = 0;
const B: usize = 1;
const A: usize = 2;

#[derive(Default)]
struct Counts([AtomicUsize; 9]);
impl Counts {
    fn snapshot(&self) -> [usize; 9] {
        self.0.each_ref().map(|count| count.load(Ordering::SeqCst))
    }
    fn step(&self, query: usize, step: usize) {
        self.0[query * 3 + step].fetch_add(1, Ordering::SeqCst);
    }
}

#[crate::db]
trait FreshDatabase: Database {
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
impl FreshDatabase for Db {
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
    three_queries: bool,
}

fn keys(db: &dyn FreshDatabase, input: Input) -> [DatabaseKeyIndex; 3] {
    [
        w::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
        b::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
        a::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
    ]
}

fn value(db: &dyn FreshDatabase, input: Input, query: usize, step: usize, value: u32) -> u32 {
    let mut event = Observation::new(match step {
        0 => Kind::BodyValue,
        1 => Kind::InitialValue,
        _ => Kind::RecoveryValue,
    })
    .key(keys(db, input)[query]);
    event.value = value;
    trace::record(event);
    value
}

#[crate::tracked(returns(copy))]
fn w(db: &dyn FreshDatabase, input: Input) -> u32 {
    db.counts().step(W, 0);
    body_gate(W);
    value(db, input, W, 0, b(db, input))
}
#[crate::tracked(returns(copy), cycle_initial = initial_b, cycle_fn = recover_b)]
fn b(db: &dyn FreshDatabase, input: Input) -> u32 {
    db.counts().step(B, 0);
    body_gate(B);
    let previous = if input.three_queries(db) {
        a(db, input)
    } else {
        w(db, input)
    };
    value(db, input, B, 0, (previous + 1).min(3))
}
#[crate::tracked(returns(copy), cycle_initial = initial_a, cycle_fn = recover_a)]
fn a(db: &dyn FreshDatabase, input: Input) -> u32 {
    db.counts().step(A, 0);
    value(db, input, A, 0, w(db, input))
}
fn initial_b(db: &dyn FreshDatabase, _id: Id, input: Input) -> u32 {
    db.counts().step(B, 1);
    value(db, input, B, 1, 0)
}
fn initial_a(db: &dyn FreshDatabase, _id: Id, input: Input) -> u32 {
    db.counts().step(A, 1);
    value(db, input, A, 1, 0)
}
fn recover_b(
    db: &dyn FreshDatabase,
    _cycle: &Cycle<'_>,
    _last: &u32,
    next: u32,
    input: Input,
) -> u32 {
    db.counts().step(B, 2);
    value(db, input, B, 2, next)
}
fn recover_a(
    db: &dyn FreshDatabase,
    _cycle: &Cycle<'_>,
    _last: &u32,
    next: u32,
    input: Input,
) -> u32 {
    db.counts().step(A, 2);
    value(db, input, A, 2, next)
}

#[derive(Debug)]
enum Message {
    BodyPaused(ThreadId),
    WillWait(ThreadId),
    RootReturned(usize, Result<u32, String>),
}
struct Schedule {
    pause: Option<(usize, mpsc::Receiver<()>)>,
    w: DatabaseKeyIndex,
    messages: mpsc::Sender<Message>,
}
thread_local! {
    static SCHEDULE: RefCell<Option<Schedule>> = const { RefCell::new(None) };
}
fn body_gate(query: usize) {
    let pause = SCHEDULE.with_borrow_mut(|slot| {
        let schedule = slot.as_mut()?;
        if schedule
            .pause
            .as_ref()
            .is_some_and(|(target, _)| *target == query)
        {
            Some((schedule.pause.take().unwrap().1, schedule.messages.clone()))
        } else {
            None
        }
    });
    if let Some((resume, messages)) = pause {
        messages
            .send(Message::BodyPaused(thread::current().id()))
            .unwrap();
        resume
            .recv_timeout(DEADLINE)
            .expect("coordinator releases the native body");
    }
}
fn wait_event(event: Event) {
    if let EventKind::WillBlockOn { database_key, .. } = event.kind {
        SCHEDULE.with_borrow(|slot| {
            if let Some(schedule) = slot
                && database_key == schedule.w
            {
                // This event runs under runtime locks; only notify and return.
                let _ = schedule
                    .messages
                    .send(Message::WillWait(thread::current().id()));
            }
        });
    }
}
fn panic_text(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else {
        format!("non-string panic {:?}", payload.type_id())
    }
}
struct Report {
    result: Result<u32, String>,
    trace: TransferTrace,
}
fn worker(
    db: Db,
    input: Input,
    worker: usize,
    pause: Option<(usize, mpsc::Receiver<()>)>,
    messages: mpsc::Sender<Message>,
    ordinal: Arc<AtomicUsize>,
) -> Report {
    let (result, trace) = trace::collect(TraceConfig { worker, ordinal }, || {
        SCHEDULE.with_borrow_mut(|slot| {
            assert!(
                slot.replace(Schedule {
                    pause,
                    w: keys(&db, input)[W],
                    messages: messages.clone()
                })
                .is_none()
            );
        });
        let result = catch_unwind(AssertUnwindSafe(|| {
            let root = if worker == 0 {
                W
            } else if input.three_queries(&db) {
                A
            } else {
                B
            };
            let result = match root {
                W => w(&db, input),
                B => b(&db, input),
                _ => a(&db, input),
            };
            let mut event = Observation::new(Kind::RootResult).key(keys(&db, input)[root]);
            event.value = result;
            trace::record(event);
            result
        }))
        .map_err(|payload| panic_text(payload.as_ref()));
        // Deliver the native return before cleanup or waiting for another root.
        messages
            .send(Message::RootReturned(worker, result.clone()))
            .unwrap();
        SCHEDULE.with_borrow_mut(|slot| *slot = None);
        super::assert_worker_clean(&db);
        result
    });
    Report { result, trace }
}
fn assert_graph_clean(graph: GraphSnapshot) {
    assert!(!graph.edges.overflow && graph.edges.is_empty(), "{graph:?}");
    assert!(
        !graph.pending.overflow && graph.pending.is_empty(),
        "{graph:?}"
    );
    assert!(
        !graph.transferred.overflow && graph.transferred.is_empty(),
        "{graph:?}"
    );
    assert!(
        !graph.dependents.overflow && !graph.reverse.overflow,
        "{graph:?}"
    );
    for (_, entries) in graph.dependents.entries.into_iter().flatten() {
        assert!(!entries.overflow && entries.is_empty(), "{graph:?}");
    }
    for (_, entries) in graph.reverse.entries.into_iter().flatten() {
        assert!(!entries.overflow && entries.is_empty(), "{graph:?}");
    }
}
fn assert_installed_wait(
    db: &Db,
    w: DatabaseKeyIndex,
    owner: ThreadId,
    waiter: ThreadId,
) -> GraphSnapshot {
    let deadline = Instant::now() + DEADLINE;
    loop {
        let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
        assert!(
            !graph.edges.overflow && !graph.dependents.overflow,
            "{graph:?}"
        );
        let edge = graph
            .edges
            .entries
            .into_iter()
            .flatten()
            .any(|pair| pair == (waiter, owner));
        let dependent = graph
            .dependents
            .entries
            .into_iter()
            .flatten()
            .any(|(key, threads)| {
                key == w
                    && !threads.overflow
                    && threads
                        .entries
                        .into_iter()
                        .flatten()
                        .any(|thread| thread == waiter)
            });
        if edge && dependent {
            return graph;
        }
        assert!(
            Instant::now() < deadline,
            "real W wait did not install: {graph:?}"
        );
        thread::yield_now();
    }
}
fn claim(db: &Db, input: Input, key: DatabaseKeyIndex) -> Option<trace::SyncSnapshot> {
    db.zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap()
        .sync_table()
        .test_transfer_state(input.as_id())
}
fn run(three_queries: bool) {
    let db = Db::default();
    let input = Input::new(&db, three_queries);
    let keys = keys(&db, input);
    let ordinal = Arc::new(AtomicUsize::new(0));
    let (messages, receive) = mpsc::channel();
    let (resume, pause) = mpsc::channel();
    let root_db = db.clone();
    let root_messages = messages.clone();
    let root_ordinal = ordinal.clone();
    let first = thread::spawn(move || {
        worker(
            root_db,
            input,
            0,
            Some((if three_queries { B } else { W }, pause)),
            root_messages,
            root_ordinal,
        )
    });
    let Message::BodyPaused(owner) = receive.recv_timeout(DEADLINE).expect("W ownership barrier")
    else {
        panic!("W must pause before requesting the remote query");
    };
    let remote_db = db.clone();
    let remote_ordinal = ordinal.clone();
    let second = thread::spawn(move || worker(remote_db, input, 1, None, messages, remote_ordinal));
    let Message::WillWait(waiter) = receive
        .recv_timeout(DEADLINE)
        .expect("remote W wait notification")
    else {
        panic!("remote root must wait on W");
    };
    let graph = assert_installed_wait(&db, keys[W], owner, waiter);
    let owners = keys.map(|key| claim(&db, input, key));
    eprintln!(
        "FRESH installed three={three_queries} keys={keys:?} owner={owner:?} waiter={waiter:?} graph={graph:?} claims={owners:?}"
    );
    assert!(
        matches!(owners[W], Some(state) if matches!(state.owner, SyncOwner::Thread(thread) if thread == owner))
    );
    let remote = if three_queries { A } else { B };
    assert!(
        matches!(owners[remote], Some(state) if matches!(state.owner, SyncOwner::Thread(thread) if thread == waiter))
    );
    if three_queries {
        assert!(
            matches!(owners[B], Some(state) if matches!(state.owner, SyncOwner::Thread(thread) if thread == owner))
        );
    }
    resume.send(()).unwrap();
    let mut observed = [None, None];
    while observed.iter().any(Option::is_none) {
        match receive
            .recv_timeout(DEADLINE)
            .expect("native root result before joining")
        {
            Message::RootReturned(worker, result) => {
                eprintln!("FRESH immediate root worker={worker} result={result:?}");
                assert!(observed[worker].replace(result).is_none());
            }
            Message::WillWait(_) => {}
            Message::BodyPaused(_) => panic!("first-body gate repeated"),
        }
    }
    let reports = [
        first.join().expect("W worker cleanup"),
        second.join().expect("remote worker cleanup"),
    ];
    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
    let claims = keys.map(|key| claim(&db, input, key));
    eprintln!(
        "FRESH cleanup graph={graph:?} claims={claims:?} counts[W,B,A;body,initial,recovery]={:?}",
        db.counts.snapshot()
    );
    assert_graph_clean(graph);
    for state in claims.into_iter().flatten() {
        assert!(
            matches!(state.owner, SyncOwner::Transferred),
            "remaining thread-owned claim"
        );
        assert!(!state.claimed_twice, "{state:?}");
    }
    super::assert_worker_clean(&db);
    assert_eq!(db.zalsa().attempt_operations.load(Ordering::SeqCst), 0);
    let mut records = Vec::new();
    for report in &reports {
        assert!(!report.trace.broken);
        records.extend(report.trace.records.iter().copied());
    }
    records.sort_by_key(|record| record.ordinal);
    for (expected, record) in records.iter().enumerate() {
        eprintln!("FRESH trace {record:?}");
        assert_eq!(record.ordinal, expected);
    }
    assert_eq!(records.len(), ordinal.load(Ordering::SeqCst));
    let seed = records
        .iter()
        .find(|record| {
            record.worker == 0
                && record.event.kind == Kind::InitialInserted
                && record.event.key == Some(keys[remote])
        })
        .expect("native cold seed selected inside W's live execution");
    let wait = records
        .iter()
        .find(|record| record.event.kind == Kind::Edge && record.event.key == Some(keys[W]))
        .unwrap();
    assert!(wait.ordinal < seed.ordinal);
    if three_queries {
        let transfer = records
            .iter()
            .find(|record| {
                record.worker == 0
                    && record.event.kind == Kind::Mode
                    && record.event.key == Some(keys[B])
                    && record.event.mode == Some(Mode::TransferTo(keys[A]))
            })
            .expect("B transfers to A");
        let released = records
            .iter()
            .find(|record| {
                record.worker == 0
                    && record.event.kind == Kind::TransferEnd
                    && record.event.key == Some(keys[B])
            })
            .expect("real B transfer returns");
        assert!(seed.ordinal < transfer.ordinal && transfer.ordinal < released.ordinal);
        assert!(
            !released.event.decision,
            "A still waits on the retained ancestor W"
        );
    }
    eprintln!(
        "FRESH ordered-and-clean three={three_queries} results={observed:?} counts={:?} records={}",
        db.counts.snapshot(),
        records.len()
    );
    for (worker, report) in reports.iter().enumerate() {
        assert_eq!(observed[worker].as_ref(), Some(&report.result));
        assert_eq!(
            report.result,
            Ok(3),
            "every successful independent root must return the fixed point (worker {worker})"
        );
    }
}

// A's wait on W remains installed when the nested B claim transfers to A.
#[test]
fn fresh_ancestor_wait_requires_wrapper_handoff() {
    run(true);
}

// The cold B initial value belongs inside W's still-owned execution.
#[test]
fn fresh_cold_seed_requires_wrapper_handoff() {
    run(false);
}

#[test]
fn single_thread_recoverable_head_converges() {
    let db = Db::default();
    let input = Input::new(&db, false);
    assert_eq!(b(&db, input), 3);
    assert_eq!(w(&db, input), 3);
    super::assert_worker_clean(&db);
    assert_graph_clean(db.zalsa().runtime().test_transfer_graph_snapshot());
    eprintln!(
        "FRESH recoverable-head control counts={:?}",
        db.counts.snapshot()
    );
}

#[test]
fn single_thread_panic_head_still_panics() {
    let db = Db::default();
    let input = Input::new(&db, false);
    let result = catch_unwind(AssertUnwindSafe(|| w(&db, input)));
    let message = panic_text(
        result
            .expect_err("a detected W head has no recovery")
            .as_ref(),
    );
    assert!(message.contains("dependency graph cycle"), "{message}");
    super::assert_worker_clean(&db);
    assert_graph_clean(db.zalsa().runtime().test_transfer_graph_snapshot());
    for key in keys(&db, input) {
        assert!(claim(&db, input, key).is_none());
    }
    eprintln!(
        "FRESH panic-head control counts={:?} payload={message}",
        db.counts.snapshot()
    );
}
