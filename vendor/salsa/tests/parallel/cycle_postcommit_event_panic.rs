// Shuttle does not support the native panic and timed-channel coordination in this control.
#![cfg(not(feature = "shuttle"))]

use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use salsa::plumbing::{AsId, ZalsaDatabase};
use salsa::{Cycle, Database, DatabaseKeyIndex, Event, EventKind, Id};

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Counts {
    a: AtomicUsize,
    b: AtomicUsize,
    recovery: AtomicUsize,
}

impl Counts {
    fn snapshot(&self) -> [usize; 3] {
        [
            self.a.load(Ordering::SeqCst),
            self.b.load(Ordering::SeqCst),
            self.recovery.load(Ordering::SeqCst),
        ]
    }
}

#[salsa::db]
trait TestDatabase: Database {
    fn counts(&self) -> &Counts;
}

#[salsa::db]
#[derive(Clone)]
struct Db {
    storage: salsa::Storage<Self>,
    counts: Arc<Counts>,
}

#[salsa::db]
impl Database for Db {}

#[salsa::db]
impl TestDatabase for Db {
    fn counts(&self) -> &Counts {
        &self.counts
    }
}

impl Db {
    fn new(event: Option<Box<dyn Fn(Event) + Send + Sync + 'static>>) -> Self {
        Self {
            storage: salsa::Storage::new(event),
            counts: Arc::new(Counts::default()),
        }
    }
}

#[salsa::input]
struct Input {
    marker: u32,
}

#[salsa::tracked(returns(copy), cycle_initial = initial, cycle_fn = recover_a)]
fn a(db: &dyn TestDatabase, input: Input) -> u32 {
    db.counts().a.fetch_add(1, Ordering::SeqCst);
    b(db, input) + 1
}

#[salsa::tracked(returns(copy), cycle_initial = initial)]
fn b(db: &dyn TestDatabase, input: Input) -> u32 {
    db.counts().b.fetch_add(1, Ordering::SeqCst);
    a(db, input).max(b(db, input))
}

fn initial(_db: &dyn TestDatabase, _id: Id, _input: Input) -> u32 {
    0
}

fn recover_a(
    db: &dyn TestDatabase,
    _cycle: &Cycle<'_>,
    _previous: &u32,
    value: u32,
    _input: Input,
) -> u32 {
    db.counts().recovery.fetch_add(1, Ordering::SeqCst);
    value.min(3)
}

#[derive(Debug)]
struct FinalizationPanic(Arc<()>);

struct Coordination {
    a: OnceLock<DatabaseKeyIndex>,
    b: OnceLock<DatabaseKeyIndex>,
    paused: AtomicBool,
    blocked: AtomicBool,
    finalized: AtomicBool,
    start_waiter: mpsc::SyncSender<()>,
    waiter_blocked: Mutex<mpsc::Receiver<()>>,
    signal_blocked: mpsc::SyncSender<()>,
    waiter_result: Mutex<mpsc::Receiver<Result<u32, &'static str>>>,
    identity: Arc<()>,
}

impl Coordination {
    fn event(&self, event: Event) {
        match event.kind {
            EventKind::WillIterateCycle { database_key, .. }
                if Some(&database_key) == self.a.get()
                    && !self.paused.swap(true, Ordering::SeqCst) =>
            {
                self.start_waiter
                    .send(())
                    .expect("waiter remains available");
                self.waiter_blocked
                    .lock()
                    .unwrap()
                    .recv_timeout(TIMEOUT)
                    .expect("nested query must register a real waiter before iteration continues");
            }
            EventKind::WillBlockOn { database_key, .. }
                if Some(&database_key) == self.b.get()
                    && !self.blocked.swap(true, Ordering::SeqCst) =>
            {
                self.signal_blocked
                    .send(())
                    .expect("outer query waits for waiter registration");
            }
            EventKind::DidFinalizeCycle { database_key, .. }
                if Some(&database_key) == self.a.get() =>
            {
                assert!(self.blocked.load(Ordering::SeqCst));
                let result = self
                    .waiter_result
                    .lock()
                    .unwrap()
                    .recv_timeout(TIMEOUT)
                    .expect("normal group release must wake the waiter before the event returns");
                assert_eq!(
                    result,
                    Ok(3),
                    "a postcommit event must not poison an accepted waiter"
                );
                assert!(!self.finalized.swap(true, Ordering::SeqCst));
                panic_any(FinalizationPanic(self.identity.clone()));
            }
            _ => {}
        }
    }
}

#[test]
fn blocked_waiter_receives_accepted_cycle_before_finalization_event_panics() {
    let baseline = Db::new(None);
    let input = Input::new(&baseline, 0);
    assert_eq!((a(&baseline, input), b(&baseline, input)), (3, 3));

    let (start_tx, start_rx) = mpsc::sync_channel(1);
    let (blocked_tx, blocked_rx) = mpsc::sync_channel(1);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let coordination = Arc::new(Coordination {
        a: OnceLock::new(),
        b: OnceLock::new(),
        paused: AtomicBool::new(false),
        blocked: AtomicBool::new(false),
        finalized: AtomicBool::new(false),
        start_waiter: start_tx,
        waiter_blocked: Mutex::new(blocked_rx),
        signal_blocked: blocked_tx,
        waiter_result: Mutex::new(result_rx),
        identity: Arc::new(()),
    });
    let observed = coordination.clone();
    let db = Db::new(Some(Box::new(move |event| observed.event(event))));
    let input = Input::new(&db, 0);
    coordination
        .a
        .set(a::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id()))
        .unwrap();
    coordination
        .b
        .set(b::fn_ingredient_(&db, db.zalsa()).database_key_index(input.as_id()))
        .unwrap();

    let (finished_a_tx, finished_a_rx) = mpsc::sync_channel(1);
    let (finished_b_tx, finished_b_rx) = mpsc::sync_channel(1);
    let db_a = db.clone();
    let outer = std::thread::spawn(move || {
        let result = catch_unwind(AssertUnwindSafe(|| a(&db_a, input)));
        let _ = finished_a_tx.send(());
        result
    });
    let db_b = db.clone();
    let waiter = std::thread::spawn(move || {
        let result = catch_unwind(AssertUnwindSafe(|| {
            start_rx
                .recv_timeout(TIMEOUT)
                .expect("outer cycle reaches a provisional iteration");
            b(&db_b, input)
        }));
        let summary = result.as_ref().copied().map_err(|_| "waiter panicked");
        let _ = result_tx.send(summary);
        let _ = finished_b_tx.send(());
        result
    });
    // Both callbacks have bounded waits. Their unwind releases outstanding claims on failure.
    finished_a_rx
        .recv_timeout(TIMEOUT * 3)
        .expect("outer worker completed");
    finished_b_rx
        .recv_timeout(TIMEOUT * 3)
        .expect("waiter worker completed");
    let payload = outer
        .join()
        .unwrap()
        .expect_err("the finalization event panics");
    let marker = payload
        .downcast_ref::<FinalizationPanic>()
        .expect("original finalization panic");
    assert!(Arc::ptr_eq(&marker.0, &coordination.identity));
    assert_eq!(waiter.join().unwrap().unwrap(), 3);
    assert!(coordination.finalized.load(Ordering::SeqCst));
    let counts = db.counts.snapshot();
    assert_eq!(
        [
            (a(&db, input), b(&db, input)),
            (a(&db, input), b(&db, input))
        ],
        [(3, 3), (3, 3)]
    );
    assert_eq!(db.counts.snapshot(), counts);
}
