use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use super::Node;
use crate::attempt_probe::paired_test_support::run_pair;
use crate::attempt_probe::transfer_test_support::{
    self as trace, Event as Observation, Kind, TraceConfig,
};
use crate::attempt_probe::{
    self, AttemptOutcome, Incomplete, MemoReuse, QueryPolicy, try_with_attempt,
};
use crate::function::memo::MemoHeader;
use crate::function::{ClaimResult, Reentrancy};
use crate::plumbing::AsId;
use crate::sync::atomic::Ordering;
use crate::zalsa::ZalsaDatabase;
use crate::{Cycle, Database, DatabaseKeyIndex, Durability, Event, EventKind, Id, Setter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    Execute,
    Iterate,
    Compare,
    ReturnBackdate,
    FinalizeBackdate,
    Finalized,
}

#[derive(Clone, Copy, Default)]
enum CallbackUse {
    #[default]
    Report,
    Status,
    Read,
    Nested {
        panic: bool,
    },
    Quiet,
}

#[derive(Default)]
struct State {
    point: Option<Point>,
    key: Option<DatabaseKeyIndex>,
    reason: Option<Incomplete>,
    panic: Option<Arc<()>>,
    old_value: usize,
    fired: bool,
    at_refusal: [usize; 3],
    counts: [usize; 3],
    executions: Vec<DatabaseKeyIndex>,
    callback_use: CallbackUse,
    read_input: Option<Input>,
    iteration_pause: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    wait_release: Option<(DatabaseKeyIndex, mpsc::Sender<()>)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

struct Reset;
impl Drop for Reset {
    fn drop(&mut self) {
        STATE.with_borrow_mut(|state| *state = State::default());
    }
}

fn reset() -> Reset {
    STATE.with_borrow_mut(|state| *state = State::default());
    Reset
}

fn visit(index: usize) {
    STATE.with_borrow_mut(|state| state.counts[index] += 1);
}

fn counts() -> [usize; 3] {
    STATE.with_borrow(|state| state.counts)
}

fn arm(key: DatabaseKeyIndex, point: Point, reason: Incomplete, callback_use: CallbackUse) {
    STATE.with_borrow_mut(|state| {
        state.key = Some(key);
        state.point = Some(point);
        state.reason = Some(reason);
        state.callback_use = callback_use;
        state.fired = false;
    });
}

fn header(db: &dyn Database, key: DatabaseKeyIndex) -> &MemoHeader {
    db.zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap()
        .memo(db.zalsa(), key.key_index())
        .expect("native query produced a canonical memo")
        .header()
}

fn classify(db: &dyn Database, key: DatabaseKeyIndex, reuse: MemoReuse) {
    let _operation = attempt_probe::enter(
        db.zalsa(),
        QueryPolicy::ReturnOnly,
        "eligibility observation",
    );
    let memo = header(db, key);
    assert_eq!(memo.attempt_reuse(db.zalsa()), reuse);
    assert_eq!(
        memo.can_seed_attempt(db.zalsa()),
        reuse == MemoReuse::Ordinary
    );
}

fn clean(db: &dyn Database) {
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert!(attempt_probe::current().is_none());
    assert_eq!(db.zalsa().attempt_operations.load(Ordering::SeqCst), 0);
}

#[derive(Debug)]
struct NativeFailure(Arc<()>);

fn fire(point: Point) {
    let request = STATE.with_borrow_mut(|state| {
        if state.point != Some(point) || state.fired {
            return None;
        }
        state.fired = true;
        state.at_refusal = state.counts;
        Some((
            state.key.unwrap(),
            state.reason.unwrap(),
            state.panic.clone(),
            state.callback_use,
            state.read_input,
        ))
    });
    let Some((key, reason, panic, callback_use, read_input)) = request else {
        return;
    };
    let mut observation = Observation::new(Kind::Gate).key(key);
    observation.phase = Some(if matches!(callback_use, CallbackUse::Quiet) {
        "native quiet"
    } else {
        "native refusal"
    });
    trace::record(observation);
    crate::with_attached_database(|db| {
        assert_eq!(attempt_probe::current_policy(), QueryPolicy::ReturnOnly);
        assert_eq!(
            db.zalsa_local().active_query().is_some(),
            point == Point::Compare
        );
        let claimed = matches!(
            db.zalsa()
                .lookup_ingredient(key.ingredient_index())
                .as_function()
                .unwrap()
                .sync_table()
                .peek_claim(db.zalsa(), key.key_index(), Reentrancy::Deny),
            ClaimResult::Cycle { .. }
        );
        assert_eq!(claimed, point != Point::Finalized);
        if let Some(identity) = panic {
            panic_any(NativeFailure(identity));
        }
        match callback_use {
            CallbackUse::Report => {
                assert_eq!(attempt_probe::report_incomplete(db, reason), reason);
                let later = match reason {
                    Incomplete::Allowance => Incomplete::Interrupted,
                    Incomplete::Interrupted => Incomplete::Allowance,
                    Incomplete::RequestedAllocation => Incomplete::Allowance,
                };
                assert_eq!(attempt_probe::report_incomplete(db, later), reason);
            }
            CallbackUse::Status => assert!(attempt_probe::is_incomplete(db)),
            CallbackUse::Read => {
                let before = counts();
                assert_eq!(leaf(db, read_input.expect("cached incomplete input")).0, 0);
                assert_eq!(
                    counts(),
                    before,
                    "callback did not read the cached incomplete memo"
                );
            }
            CallbackUse::Nested { panic } => {
                let identity = Arc::new(());
                let caught = catch_unwind(AssertUnwindSafe(|| {
                    let ((), observed) = attempt_probe::with_incomplete_observation(|| {
                        assert_eq!(attempt_probe::report_incomplete(db, reason), reason);
                        if panic {
                            panic_any(NativeFailure(identity.clone()));
                        }
                    });
                    assert!(observed);
                }));
                if panic {
                    let payload = caught.expect_err("nested callback panicked");
                    let payload = payload.downcast::<NativeFailure>().unwrap();
                    assert!(Arc::ptr_eq(&payload.0, &identity));
                } else {
                    assert!(caught.is_ok());
                }
                let ((), observed) = attempt_probe::with_incomplete_observation(|| {});
                assert!(
                    !observed,
                    "a quiet nested callback inherited earlier incomplete use"
                );
            }
            CallbackUse::Quiet => {}
        }
    })
    .expect("native completion keeps the database attached");
}

fn event(event: Event) {
    let point = match event.kind {
        EventKind::WillBlockOn { database_key, .. } => {
            if let Some((expected, release)) =
                STATE.with_borrow_mut(|state| state.wait_release.take())
            {
                assert_eq!(database_key, expected);
                // The wait already owns its graph guards; only notify the producer here.
                let mut observation = Observation::new(Kind::Gate).key(database_key);
                observation.phase = Some("unfinished participant wait");
                trace::record(observation);
                let _ = release.send(());
            }
            return;
        }
        EventKind::WillExecute { database_key } => {
            STATE.with_borrow_mut(|state| state.executions.push(database_key));
            (database_key, Point::Execute)
        }
        EventKind::WillIterateCycle { database_key, .. } => (database_key, Point::Iterate),
        EventKind::DidFinalizeCycle { database_key, .. } => (database_key, Point::Finalized),
        _ => return,
    };
    if STATE.with_borrow(|state| state.key == Some(point.0)) {
        fire(point.1);
        if point.1 == Point::Iterate
            && let Some((entered, resume)) =
                STATE.with_borrow_mut(|state| state.iteration_pause.take())
        {
            entered
                .send(())
                .expect("consumer awaits the unfinished iteration");
            resume
                .recv_timeout(Duration::from_secs(5))
                .expect("consumer reached its canonical wait");
        }
    }
}

#[crate::db]
#[derive(Clone)]
struct Db {
    storage: crate::Storage<Self>,
}
#[crate::db]
impl Database for Db {}
impl Default for Db {
    fn default() -> Self {
        Self {
            storage: crate::Storage::new(Some(Box::new(event))),
        }
    }
}

#[crate::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

#[derive(Debug)]
struct Value(u32);
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        let point = STATE.with_borrow(|state| match state.point {
            Some(Point::Compare) => Some(Point::Compare),
            Some(point @ (Point::ReturnBackdate | Point::FinalizeBackdate))
                if state.old_value == std::ptr::from_ref(self).addr() =>
            {
                Some(point)
            }
            _ => None,
        });
        if let Some(point) = point {
            fire(point);
        }
        self.0 == other.0
    }
}
impl Eq for Value {}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn scalar(db: &dyn Database, input: Input) -> Value {
    visit(0);
    let _ = input.value(db);
    Value(7)
}

#[crate::tracked(returns(ref), attempt = ReturnOnly)]
fn leaf(db: &dyn Database, input: Input) -> Value {
    visit(0);
    Value(input.value(db))
}

#[crate::tracked(returns(copy), attempt = ReturnOnly)]
fn independent(_db: &dyn Database) -> u32 {
    19
}

#[crate::tracked(returns(ref), attempt = ReturnOnly, cycle_initial = initial, cycle_fn = recover)]
fn cycle(db: &dyn Database, input: Input) -> Value {
    visit(0);
    let _ = input.value(db);
    Value((cycle(db, input).0 + 1).min(3))
}
fn initial(_db: &dyn Database, _id: Id, _input: Input) -> Value {
    visit(1);
    Value(0)
}
fn recover(
    _db: &dyn Database,
    _cycle: &Cycle<'_>,
    _last: &Value,
    value: Value,
    _input: Input,
) -> Value {
    visit(2);
    value
}

fn key(db: &Db, input: Input, point: Point) -> DatabaseKeyIndex {
    match point {
        Point::Execute => leaf::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
        Point::ReturnBackdate => {
            scalar::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id())
        }
        _ => cycle::fn_ingredient_(db, db.zalsa()).database_key_index(input.as_id()),
    }
}

fn value(db: &Db, input: Input, point: Point) -> &Value {
    match point {
        Point::Execute => leaf(db, input),
        Point::ReturnBackdate => scalar(db, input),
        _ => cycle(db, input),
    }
}

#[test]
fn native_completion_refusal_preserves_incomplete_ownership() {
    for point in [
        Point::Iterate,
        Point::Compare,
        Point::ReturnBackdate,
        Point::FinalizeBackdate,
    ] {
        for reason in [Incomplete::Allowance, Incomplete::Interrupted] {
            let _reset = reset();
            let mut db = Db::default();
            let input = Input::new(&db, 0);
            let key = key(&db, input, point);
            if matches!(point, Point::ReturnBackdate | Point::FinalizeBackdate) {
                let old = value(&db, input, point);
                STATE.with_borrow_mut(|state| state.old_value = std::ptr::from_ref(old).addr());
                input.set_value(&mut db).to(1);
            }
            STATE.with_borrow_mut(|state| {
                state.point = Some(point);
                state.reason = Some(reason);
                state.key = Some(key);
            });
            let mut delivered = None;
            let (outcome, observations) = trace::collect(
                TraceConfig {
                    worker: 0,
                    ordinal: Arc::new(AtomicUsize::new(0)),
                },
                || {
                    try_with_attempt(&db, 100, || {
                        assert_eq!(independent(&db), 19);
                        let result = value(&db, input, point).0;
                        delivered = Some(result);
                        assert!(header(&db, key).has_incomplete_attempt());
                        classify(&db, key, MemoReuse::Incomplete);
                        let before = counts();
                        assert_eq!(value(&db, input, point).0, result);
                        assert_eq!(counts(), before, "incomplete hit executed native callbacks");
                    })
                },
            );
            assert_eq!(outcome, Ok(AttemptOutcome::Incomplete(reason)));
            assert!(!observations.broken);
            let refusal = observations
                .records
                .iter()
                .find(|record| {
                    record.event.kind == Kind::Gate && record.event.phase == Some("native refusal")
                })
                .expect("native callback reported refusal");
            let mut published = 0;
            for record in observations
                .records
                .iter()
                .filter(|record| record.ordinal > refusal.ordinal)
            {
                assert!(
                    !matches!(
                        record.event.kind,
                        Kind::TargetPublished | Kind::SeedActive | Kind::ColdInitial
                    ),
                    "refused native completion advanced its cycle: {record:?}"
                );
                if record.event.kind == Kind::RootPublished {
                    assert_eq!(record.event.key, Some(key));
                    let memo = record.event.memo.unwrap();
                    assert!(memo.support.unwrap().explicitly_incomplete);
                    assert!(memo.heads.is_empty() && !memo.converged);
                    published += 1;
                }
            }
            assert_eq!(
                published, 1,
                "native refusal must return its one marked completion"
            );
            STATE.with_borrow(|state| {
                assert!(state.fired);
                assert_eq!(
                    state.counts, state.at_refusal,
                    "native continuation ran after refusal"
                );
            });
            assert!(delivered.is_some());
            let snapshot = header(&db, key).transfer_test_snapshot(true);
            assert!(snapshot.support.unwrap().explicitly_incomplete);
            assert!(snapshot.heads.is_empty());
            assert!(!snapshot.converged);
            assert_eq!(
                snapshot.changed_at,
                db.zalsa().current_revision(),
                "refused value was backdated"
            );
            clean(&db);
            classify(&db, key, MemoReuse::Stale);
            let before = counts();
            assert_eq!(
                value(&db, input, point).0,
                if point == Point::ReturnBackdate { 7 } else { 3 }
            );
            assert!(counts()[0] > before[0]);
            let stable = counts();
            assert_eq!(
                try_with_attempt(&db, 0, || {
                    assert_eq!(independent(&db), 19);
                    value(&db, input, point).0
                }),
                Ok(AttemptOutcome::Complete(
                    if point == Point::ReturnBackdate { 7 } else { 3 }
                ))
            );
            assert_eq!(counts(), stable);
            clean(&db);
        }
    }
}

#[test]
fn refusal_after_final_publication_keeps_independent_memos() {
    for reason in [Incomplete::Allowance, Incomplete::Interrupted] {
        let _reset = reset();
        let db = Db::default();
        let input = Input::new(&db, 0);
        let key = key(&db, input, Point::Finalized);
        STATE.with_borrow_mut(|state| {
            state.point = Some(Point::Finalized);
            state.reason = Some(reason);
            state.key = Some(key);
        });
        assert_eq!(
            try_with_attempt(&db, 100, || {
                assert_eq!(independent(&db), 19);
                assert_eq!(cycle(&db, input).0, 3);
            }),
            Ok(AttemptOutcome::Incomplete(reason))
        );
        assert!(STATE.with_borrow(|state| state.fired));
        let snapshot = header(&db, key).transfer_test_snapshot(true);
        assert!(snapshot.final_);
        assert!(!snapshot.support.unwrap().explicitly_incomplete);
        let before = counts();
        classify(&db, key, MemoReuse::Ordinary);
        assert_eq!(cycle(&db, input).0, 3);
        assert_eq!(
            try_with_attempt(&db, 0, || (cycle(&db, input).0, independent(&db))),
            Ok(AttemptOutcome::Complete((3, 19)))
        );
        assert_eq!(counts(), before);
        assert_eq!(
            header(&db, key).transfer_test_snapshot(true).identity,
            snapshot.identity
        );
        clean(&db);
    }
}

#[test]
fn native_iteration_panic_keeps_its_payload_and_releases_ownership() {
    let _reset = reset();
    let db = Db::default();
    let input = Input::new(&db, 0);
    let identity = Arc::new(());
    let key = key(&db, input, Point::Iterate);
    STATE.with_borrow_mut(|state| {
        state.point = Some(Point::Iterate);
        state.key = Some(key);
        state.reason = Some(Incomplete::Interrupted);
        state.panic = Some(identity.clone());
    });
    let Err(payload) = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(&db, 100, || cycle(&db, input).0)
    })) else {
        panic!("native panic was swallowed")
    };
    assert!(Arc::ptr_eq(
        &payload.downcast_ref::<NativeFailure>().unwrap().0,
        &identity
    ));
    clean(&db);
    let function = db
        .zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap();
    assert!(
        function
            .sync_table()
            .test_transfer_state(key.key_index())
            .is_none()
    );
    assert!(
        !function
            .memo(db.zalsa(), key.key_index())
            .unwrap()
            .has_value()
    );
}

#[test]
fn native_callbacks_distinguish_incomplete_use_from_prior_refusal() {
    for point in [Point::Execute, Point::ReturnBackdate] {
        for reason in [Incomplete::Allowance, Incomplete::Interrupted] {
            for callback_use in [
                CallbackUse::Quiet,
                CallbackUse::Report,
                CallbackUse::Status,
                CallbackUse::Read,
                CallbackUse::Nested { panic: false },
                CallbackUse::Nested { panic: true },
            ] {
                let _reset = reset();
                let mut db = Db::default();
                let source = Input::new(&db, 0);
                let input = Input::new(&db, 2);
                let source_key = key(&db, source, Point::Execute);
                let target_key = key(&db, input, point);
                let expected = if point == Point::Execute { 2 } else { 7 };
                if point == Point::ReturnBackdate {
                    let old = scalar(&db, input);
                    STATE.with_borrow_mut(|state| state.old_value = std::ptr::from_ref(old).addr());
                    input.set_value(&mut db).to(3);
                }
                STATE.with_borrow_mut(|state| state.read_input = Some(source));
                let quiet = matches!(callback_use, CallbackUse::Quiet);
                let mut delivered = None;
                assert_eq!(
                    try_with_attempt(&db, 0, || {
                        arm(source_key, Point::Execute, reason, CallbackUse::Report);
                        assert_eq!(leaf(&db, source).0, 0);
                        assert!(header(&db, source_key).has_incomplete_attempt());
                        assert!(STATE.with_borrow(|state| state.fired));

                        arm(target_key, point, reason, callback_use);
                        assert_eq!(value(&db, input, point).0, expected);
                        assert!(STATE.with_borrow(|state| state.fired));
                        let snapshot = header(&db, target_key).transfer_test_snapshot(true);
                        assert!(snapshot.final_ && snapshot.heads.is_empty());
                        assert!(!snapshot.converged);
                        if quiet {
                            assert!(snapshot.support.is_none());
                            classify(&db, target_key, MemoReuse::Ordinary);
                        } else {
                            assert!(snapshot.support.unwrap().explicitly_incomplete);
                            assert_eq!(snapshot.changed_at, db.zalsa().current_revision());
                            classify(&db, target_key, MemoReuse::Incomplete);
                        }
                        delivered = Some(snapshot.identity);
                        let before = counts();
                        assert_eq!(value(&db, input, point).0, expected);
                        assert_eq!(counts(), before, "same-owner read executed the body again");
                        assert_eq!(attempt_probe::current().unwrap().reason(), Some(reason));
                    }),
                    Ok(AttemptOutcome::Incomplete(reason))
                );
                clean(&db);
                assert!(
                    db.zalsa()
                        .lookup_ingredient(target_key.ingredient_index())
                        .as_function()
                        .unwrap()
                        .sync_table()
                        .test_transfer_state(target_key.key_index())
                        .is_none()
                );
                assert_eq!(
                    counts(),
                    [if point == Point::Execute { 2 } else { 3 }, 0, 0]
                );
                let before = counts();
                if quiet {
                    classify(&db, target_key, MemoReuse::Ordinary);
                    assert_eq!(
                        try_with_attempt(&db, 0, || value(&db, input, point).0),
                        Ok(AttemptOutcome::Complete(expected))
                    );
                    assert_eq!(counts(), before, "independent cold result was not reusable");
                    assert_eq!(
                        Some(
                            header(&db, target_key)
                                .transfer_test_snapshot(true)
                                .identity
                        ),
                        delivered
                    );
                } else {
                    classify(&db, target_key, MemoReuse::Stale);
                    assert_eq!(value(&db, input, point).0, expected);
                    assert_eq!(counts(), [before[0] + 1, 0, 0]);
                    assert_ne!(
                        Some(
                            header(&db, target_key)
                                .transfer_test_snapshot(true)
                                .identity
                        ),
                        delivered
                    );
                }
                assert!(
                    header(&db, target_key)
                        .revisions
                        .attempt_support()
                        .is_none()
                );
                let stable = header(&db, target_key)
                    .transfer_test_snapshot(true)
                    .identity;
                let before = counts();
                assert_eq!(value(&db, input, point).0, expected);
                assert_eq!(
                    try_with_attempt(&db, 0, || value(&db, input, point).0),
                    Ok(AttemptOutcome::Complete(expected))
                );
                assert_eq!(counts(), before);
                assert_eq!(
                    header(&db, target_key)
                        .transfer_test_snapshot(true)
                        .identity,
                    stable
                );
                clean(&db);
            }
        }
    }
}

#[test]
fn native_start_panic_keeps_its_payload_and_releases_ownership() {
    let _reset = reset();
    let db = Db::default();
    let input = Input::new(&db, 2);
    let key = key(&db, input, Point::Execute);
    let identity = Arc::new(());
    arm(
        key,
        Point::Execute,
        Incomplete::Interrupted,
        CallbackUse::Report,
    );
    STATE.with_borrow_mut(|state| state.panic = Some(identity.clone()));
    let payload = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(&db, 0, || leaf(&db, input).0)
    }))
    .expect_err("start callback panicked");
    let payload = payload.downcast::<NativeFailure>().unwrap();
    assert!(Arc::ptr_eq(&payload.0, &identity));
    assert_eq!(counts(), [0; 3]);
    clean(&db);
    let function = db
        .zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap();
    assert!(
        function
            .sync_table()
            .test_transfer_state(key.key_index())
            .is_none()
    );
    assert!(function.memo(db.zalsa(), key.key_index()).is_none());
    assert_eq!(
        try_with_attempt(&db, 0, || leaf(&db, input).0),
        Ok(AttemptOutcome::Complete(2))
    );
    assert_eq!(counts(), [1, 0, 0]);
    assert!(header(&db, key).revisions.attempt_support().is_none());
    clean(&db);
}

fn pair(db: &mut Db) -> (super::Node, super::Node, DatabaseKeyIndex) {
    let a = super::Node::new(db, None, 0);
    let b = super::Node::new(db, Some(a), 0);
    a.set_next(db).to(Some(b));
    let key = super::fixpoint::fn_ingredient_(db, db.zalsa()).database_key_index(b.as_id());
    (a, b, key)
}

fn executions(key: DatabaseKeyIndex) -> usize {
    STATE.with_borrow(|state| {
        state
            .executions
            .iter()
            .filter(|&&entry| entry == key)
            .count()
    })
}

fn fallback_pair(
    db: &mut Db,
    reverse: bool,
) -> ([super::Node; 2], [u32; 2], [DatabaseKeyIndex; 2]) {
    let first = super::Node::new(db, None, 10);
    let second = super::Node::new(db, Some(first), 20);
    first.set_next(db).to(Some(second));
    let (nodes, values) = if reverse {
        ([second, first], [20, 10])
    } else {
        ([first, second], [10, 20])
    };
    let ingredient = super::fallback::fn_ingredient_(db, db.zalsa());
    let keys = nodes.map(|node| ingredient.database_key_index(node.as_id()));
    (nodes, values, keys)
}

fn assert_completed_identity(before: trace::MemoSnapshot, after: trace::MemoSnapshot) {
    assert!(after.final_ && after.has_value);
    assert_eq!(after.identity, before.identity);
    assert_eq!(after.execution_revision, before.execution_revision);
    assert_eq!(after.iteration, before.iteration);
    assert_eq!(after.changed_at, before.changed_at);
    assert_eq!(after.durability, before.durability);
    assert_eq!(after.heads.entries, before.heads.entries);
    assert_eq!(after.heads.overflow, before.heads.overflow);
    assert_eq!(after.converged, before.converged);
    let lineage = |support: Option<trace::SupportSnapshot>| {
        support.map(|support| {
            (
                support.owner,
                support.revision,
                support.cancellation,
                support.explicitly_incomplete,
            )
        })
    };
    assert_eq!(lineage(after.support), lineage(before.support));
}

#[derive(Debug)]
struct FallbackRevalidationSnapshot {
    memo: trace::MemoSnapshot,
    value: u32,
    executions: usize,
}

fn fallback_revalidation_snapshots(
    db: &Db,
    nodes: [super::Node; 2],
    head_first: bool,
    phase: &str,
) -> [FallbackRevalidationSnapshot; 2] {
    let ingredient = super::fallback::fn_ingredient_(db, db.zalsa());
    // Inspecting the table avoids finalizing the participant before its requested read.
    let snapshots = nodes.map(|node| {
        let memo = super::memo(db, ingredient, node).expect("fallback has a memo");
        FallbackRevalidationSnapshot {
            memo: memo.header.transfer_test_snapshot(memo.value().is_some()),
            value: *memo.value().expect("fallback has a value"),
            executions: executions(ingredient.database_key_index(node.as_id())),
        }
    });
    eprintln!(
        "HEAD_REVALIDATION head_first={head_first} phase={phase} head={:?} participant={:?}",
        snapshots[0], snapshots[1]
    );
    snapshots
}

fn ordinary_fallback_revalidation(head_first: bool, prior_cancellation: bool, write: bool) {
    let _reset = reset();
    let mut db = Db::default();
    let head = super::Node::builder(None, 10)
        .durability(Durability::HIGH)
        .new(&db);
    let participant = super::Node::builder(Some(head), 20)
        .durability(Durability::HIGH)
        .new(&db);
    head.set_next(&mut db)
        .with_durability(Durability::HIGH)
        .to(Some(participant));
    let nodes = [head, participant];
    if prior_cancellation {
        db.trigger_cancellation();
    }
    assert_eq!(super::fallback(&db, head), 10);
    let original = fallback_revalidation_snapshots(&db, nodes, head_first, "original");
    let revision = db.zalsa().current_revision();
    let cancellation = db.zalsa().runtime().cancellation_count();
    let high_changed = db.zalsa().last_changed_revision(Durability::HIGH);
    assert_eq!([original[0].value, original[1].value], [10, 20]);
    assert!(original[0].memo.final_);
    assert!(!original[1].memo.final_);
    let head_key =
        super::fallback::fn_ingredient_(&db, db.zalsa()).database_key_index(head.as_id());
    assert_eq!(
        original[1].memo.heads.entries,
        [Some((head_key, original[0].memo.iteration)), None]
    );
    for snapshot in &original {
        assert_eq!(snapshot.memo.verified_at, revision);
        assert_eq!(snapshot.memo.execution_revision, Some(revision));
        assert_eq!(snapshot.memo.durability, Durability::HIGH);
        assert_eq!(snapshot.memo.iteration.cancellation_count(), cancellation);
        assert!(snapshot.memo.support.is_none());
        assert!(!snapshot.memo.heads.overflow);
        assert!(snapshot.executions > 0);
    }
    clean(&db);

    if write {
        db.synthetic_write(Durability::LOW);
    } else {
        db.trigger_cancellation();
    }
    let current_revision = db.zalsa().current_revision();
    if write {
        assert!(current_revision > revision);
        assert_eq!(
            db.zalsa().last_changed_revision(Durability::LOW),
            current_revision
        );
        assert_eq!(db.zalsa().runtime().cancellation_count(), 0);
    } else {
        assert_eq!(current_revision, revision);
        assert!(db.zalsa().runtime().cancellation_count() > cancellation);
    }
    assert_eq!(cancellation > 0, prior_cancellation);
    assert_eq!(
        db.zalsa().last_changed_revision(Durability::HIGH),
        high_changed
    );
    assert_eq!([head.seed(&db), participant.seed(&db)], [10, 20]);
    assert!([head.next(&db), participant.next(&db)] == [Some(participant), Some(head)]);
    let after_write = fallback_revalidation_snapshots(&db, nodes, head_first, "after_low_write");
    for (before, after) in original.iter().zip(&after_write) {
        assert_eq!(after.memo.identity, before.memo.identity);
        assert_eq!(after.memo.iteration, before.memo.iteration);
        assert_eq!(after.memo.verified_at, before.memo.verified_at);
        assert_eq!(
            after.memo.execution_revision,
            before.memo.execution_revision
        );
        assert_eq!(after.memo.changed_at, before.memo.changed_at);
        assert_eq!(after.memo.final_, before.memo.final_);
        assert_eq!(after.memo.durability, before.memo.durability);
        assert_eq!(after.memo.heads.entries, before.memo.heads.entries);
        assert_eq!(after.memo.heads.overflow, before.memo.heads.overflow);
        assert_eq!(after.memo.support, before.memo.support);
        assert_eq!(after.value, before.value);
        assert_eq!(after.executions, before.executions);
    }

    let first = if head_first { 0 } else { 1 };
    let second = 1 - first;
    let mut values = [0; 2];
    values[first] = super::fallback(&db, nodes[first]);
    let after_first = fallback_revalidation_snapshots(&db, nodes, head_first, "after_first_read");
    assert_eq!(after_first[first].memo.verified_at, current_revision);
    assert_eq!(after_first[second].memo.verified_at, revision);
    assert_eq!(
        after_first[first].memo.identity,
        original[first].memo.identity
    );
    assert_eq!(
        after_first[first].memo.iteration,
        original[first].memo.iteration
    );
    assert!(after_first[first].memo.final_);
    assert_eq!(after_first[first].executions, original[first].executions);
    if head_first {
        assert!(!after_first[1].memo.final_);
    }

    values[second] = super::fallback(&db, nodes[second]);
    let final_memos = fallback_revalidation_snapshots(&db, nodes, head_first, "after_both_reads");
    eprintln!("HEAD_REVALIDATION head_first={head_first} returned={values:?}");
    clean(&db);
    assert_eq!(
        values,
        [10, 20],
        "revalidation changed the accepted fallback"
    );
    for (before, after) in original.iter().zip(&final_memos) {
        assert_eq!(after.memo.identity, before.memo.identity);
        assert_eq!(after.memo.iteration, before.memo.iteration);
        assert_eq!(after.memo.changed_at, before.memo.changed_at);
        assert_eq!(after.memo.verified_at, current_revision);
        assert_eq!(
            after.memo.execution_revision,
            before.memo.execution_revision
        );
        assert_eq!(after.memo.durability, Durability::HIGH);
        assert!(after.memo.final_ && after.memo.support.is_none());
        assert_eq!(after.value, before.value);
        assert_eq!(after.executions, before.executions);
    }
}

#[test]
fn ordinary_fallback_survives_participant_first_revalidation() {
    ordinary_fallback_revalidation(false, false, true);
}

#[test]
fn ordinary_fallback_survives_head_first_revalidation() {
    ordinary_fallback_revalidation(true, false, true);
}

#[test]
fn accepted_fallback_survives_historical_cancellation_epochs() {
    for head_first in [false, true] {
        ordinary_fallback_revalidation(head_first, true, true);
        ordinary_fallback_revalidation(head_first, false, false);
    }
}

#[test]
fn accepted_unowned_participant_keeps_its_fallback_for_either_caller() {
    for controlled in [false, true] {
        for reverse in [false, true] {
            let _reset = reset();
            let mut db = Db::default();
            let (nodes, values, [_, key]) = fallback_pair(&mut db, reverse);
            assert_eq!(super::fallback(&db, nodes[0]), values[0]);
            let before = header(&db, key).transfer_test_snapshot(true);
            let inputs = header(&db, key).origin().inputs().collect::<Vec<_>>();
            assert!(!before.final_ && before.support.is_none());
            let work = executions(key);
            let fetch = || {
                classify(
                    &db,
                    key,
                    if controlled {
                        MemoReuse::Stale
                    } else {
                        MemoReuse::Ordinary
                    },
                );
                super::fallback(&db, nodes[1])
            };
            if controlled {
                assert_eq!(
                    try_with_attempt(&db, 100, fetch),
                    Ok(AttemptOutcome::Complete(values[1]))
                );
            } else {
                assert_eq!(fetch(), values[1]);
            }
            assert_eq!(executions(key), work, "accepted participant reexecuted");
            assert_completed_identity(before, header(&db, key).transfer_test_snapshot(true));
            assert_eq!(
                header(&db, key).origin().inputs().collect::<Vec<_>>(),
                inputs
            );
            clean(&db);
        }
    }
}

#[test]
fn current_provisional_can_finish_without_changing_its_predecessor() {
    let _reset = reset();
    let mut db = Db::default();
    let (a, b, key) = pair(&mut db);
    let mut identity = 0;
    assert_eq!(
        try_with_attempt(&db, 100, || {
            assert_eq!(super::fixpoint(&db, a), 3);
            let before = header(&db, key).transfer_test_snapshot(true);
            assert!(!before.final_ && before.support.unwrap().state == 0);
            identity = before.identity;
            classify(&db, key, MemoReuse::Ordinary);
            let work = executions(key);
            assert_eq!(super::fixpoint(&db, b), 3);
            assert_eq!(executions(key), work);
            let after = header(&db, key).transfer_test_snapshot(true);
            assert!(after.final_);
            assert_eq!(after.identity, identity);
        }),
        Ok(AttemptOutcome::Complete(()))
    );
    let work = executions(key);
    assert_eq!(super::fixpoint(&db, b), 3);
    assert_eq!(
        try_with_attempt(&db, 0, || super::fixpoint(&db, b)),
        Ok(AttemptOutcome::Complete(3))
    );
    assert_eq!(executions(key), work);
    assert_eq!(
        header(&db, key).transfer_test_snapshot(true).identity,
        identity
    );
    clean(&db);
}

#[derive(Clone, Copy, Debug)]
enum Finish {
    Complete,
    Refuse(Incomplete),
    Abandon,
}

#[test]
fn completed_component_survives_its_owners_later_outcome() {
    for finish in [
        Finish::Complete,
        Finish::Refuse(Incomplete::Allowance),
        Finish::Refuse(Incomplete::Interrupted),
        Finish::Abandon,
    ] {
        for controlled in [false, true] {
            for reverse in [false, true] {
                let _reset = reset();
                let mut db = Db::default();
                let (nodes, values, [root, key]) = fallback_pair(&mut db, reverse);
                let identity = Arc::new(());
                let result = catch_unwind(AssertUnwindSafe(|| {
                    try_with_attempt(&db, 100, || {
                        assert_eq!(super::fallback(&db, nodes[0]), values[0]);
                        assert!(!header(&db, key).transfer_test_snapshot(true).final_);
                        classify(&db, key, MemoReuse::Ordinary);
                        match finish {
                            Finish::Complete => {}
                            Finish::Refuse(reason) => {
                                attempt_probe::report_incomplete(&db, reason);
                                classify(&db, key, MemoReuse::Incomplete);
                            }
                            Finish::Abandon => panic_any(NativeFailure(identity.clone())),
                        }
                    })
                }));
                match finish {
                    Finish::Complete => {
                        assert_eq!(result.unwrap(), Ok(AttemptOutcome::Complete(())))
                    }
                    Finish::Refuse(reason) => {
                        assert_eq!(result.unwrap(), Ok(AttemptOutcome::Incomplete(reason)))
                    }
                    Finish::Abandon => assert!(Arc::ptr_eq(
                        &result
                            .unwrap_err()
                            .downcast_ref::<NativeFailure>()
                            .unwrap()
                            .0,
                        &identity
                    )),
                }
                clean(&db);
                let independent = header(&db, root).transfer_test_snapshot(true);
                assert!(independent.final_ && !independent.support.unwrap().explicitly_incomplete);
                classify(&db, root, MemoReuse::Ordinary);
                let root_work = executions(root);
                assert_eq!(super::fallback(&db, nodes[0]), values[0]);
                assert_eq!(
                    try_with_attempt(&db, 0, || super::fallback(&db, nodes[0])),
                    Ok(AttemptOutcome::Complete(values[0]))
                );
                assert_eq!(executions(root), root_work);
                assert_completed_identity(
                    independent,
                    header(&db, root).transfer_test_snapshot(true),
                );
                let before = header(&db, key).transfer_test_snapshot(true);
                assert!(!before.final_);
                assert_eq!(
                    before.support.unwrap().state,
                    match finish {
                        Finish::Complete => 2,
                        Finish::Refuse(Incomplete::Allowance) => 1,
                        Finish::Refuse(Incomplete::Interrupted) => 4,
                        Finish::Refuse(Incomplete::RequestedAllocation) => 5,
                        Finish::Abandon => 3,
                    }
                );
                let work = executions(key);
                let fetch = || {
                    classify(&db, key, MemoReuse::Stale);
                    super::fallback(&db, nodes[1])
                };
                if controlled {
                    assert_eq!(
                        try_with_attempt(&db, 100, fetch),
                        Ok(AttemptOutcome::Complete(values[1]))
                    );
                } else {
                    assert_eq!(fetch(), values[1]);
                }
                assert_eq!(
                    executions(key),
                    work,
                    "completed reads must not execute callbacks"
                );
                assert_completed_identity(before, header(&db, key).transfer_test_snapshot(true));
                clean(&db);
            }
        }
    }
}

#[test]
fn completed_participant_keeps_its_value_inside_a_refused_owner() {
    for reason in [Incomplete::Allowance, Incomplete::Interrupted] {
        let _reset = reset();
        let mut db = Db::default();
        let (nodes, values, [_, key]) = fallback_pair(&mut db, false);
        assert_eq!(
            try_with_attempt(&db, 100, || {
                assert_eq!(super::fallback(&db, nodes[0]), values[0]);
                let before = header(&db, key).transfer_test_snapshot(true);
                assert!(!before.final_);
                attempt_probe::report_incomplete(&db, reason);
                classify(&db, key, MemoReuse::Incomplete);
                let work = executions(key);
                assert_eq!(super::fallback(&db, nodes[1]), values[1]);
                assert_eq!(executions(key), work);
                assert_completed_identity(before, header(&db, key).transfer_test_snapshot(true));
                classify(&db, key, MemoReuse::Ordinary);
            }),
            Ok(AttemptOutcome::Incomplete(reason))
        );
        clean(&db);
    }
}

#[test]
fn completed_foreign_participant_keeps_its_fallback_while_owner_runs() {
    for controlled in [false, true] {
        for reverse in [false, true] {
            let mut db = Db::default();
            let (nodes, values, [_, key]) = fallback_pair(&mut db, reverse);
            let (ready_tx, ready_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            let (left, right) = run_pair(
                &db,
                move |db, participant| {
                    participant.run(&db, 100, || {
                        let _reset = reset();
                        assert_eq!(super::fallback(&db, nodes[0]), values[0]);
                        classify(&db, key, MemoReuse::Ordinary);
                        let before = header(&db, key).transfer_test_snapshot(true);
                        assert!(!before.final_ && before.support.unwrap().state == 0);
                        ready_tx.send(before).unwrap();
                        done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    })
                },
                move |db, participant| {
                    let inspect = || {
                        let _reset = reset();
                        let before = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        classify(&db, key, MemoReuse::Stale);
                        assert_eq!(
                            header(&db, key).transfer_test_snapshot(true).identity,
                            before.identity
                        );
                        assert_eq!(super::fallback(&db, nodes[1]), values[1]);
                        assert_eq!(executions(key), 0, "foreign proof executed a body");
                        assert_completed_identity(
                            before,
                            header(&db, key).transfer_test_snapshot(true),
                        );
                        done_tx.send(()).unwrap();
                    };
                    if controlled {
                        assert_eq!(
                            participant.run(&db, 100, inspect),
                            Ok(AttemptOutcome::Complete(()))
                        );
                    } else {
                        assert_eq!(participant.run_ordinary(&db, inspect), Ok(()));
                    }
                },
            )
            .unwrap();
            assert_eq!(left.unwrap(), Ok(AttemptOutcome::Complete(())));
            right.unwrap();
            clean(&db);
        }
    }
}

#[test]
fn an_edit_cannot_reuse_a_previous_owner_provisional_stamp() {
    for controlled in [false, true] {
        let _reset = reset();
        let mut db = Db::default();
        let (a, b, key) = pair(&mut db);
        assert_eq!(
            try_with_attempt(&db, 100, || super::fixpoint(&db, a)),
            Ok(AttemptOutcome::Complete(3))
        );
        let before = header(&db, key).transfer_test_snapshot(true);
        assert!(!before.final_);
        assert_eq!(before.execution_revision, Some(before.verified_at));
        a.set_next(&mut db).to(None);
        assert_ne!(
            before.support.unwrap().revision,
            db.zalsa().current_revision()
        );
        let work = executions(key);
        let fetch = || {
            classify(&db, key, MemoReuse::Stale);
            super::fixpoint(&db, b)
        };
        if controlled {
            assert_eq!(
                try_with_attempt(&db, 100, fetch),
                Ok(AttemptOutcome::Complete(2))
            );
        } else {
            assert_eq!(fetch(), 2);
        }
        assert!(executions(key) > work);
        let after = header(&db, key).transfer_test_snapshot(true);
        assert_ne!(after.identity, before.identity);
        assert_ne!(after.execution_revision, before.execution_revision);
        assert_eq!(after.execution_revision, None);
        assert!(after.heads.entries.iter().all(Option::is_none));
        assert!(!after.heads.overflow);
        assert!(after.support.is_none());
        assert!(after.final_);
        assert_eq!(after.verified_at, db.zalsa().current_revision());
        clean(&db);
    }
}

#[test]
fn unfinished_participant_keeps_typed_transport_without_becoming_final() {
    for reason in [Incomplete::Allowance, Incomplete::Interrupted] {
        let _reset = reset();
        let mut db = Db::default();
        let (a, b, key) = pair(&mut db);
        let ingredient = super::fixpoint::fn_ingredient_(&db, db.zalsa());
        let root = ingredient.database_key_index(a.as_id());
        arm(root, Point::Iterate, reason, CallbackUse::Report);
        assert_eq!(
            try_with_attempt(&db, 100, || {
                let _ = super::fixpoint(&db, a);
                assert!(STATE.with_borrow(|state| state.fired));
                let head = super::memo(&db, ingredient, a)
                    .unwrap()
                    .transfer_test_snapshot();
                assert!(head.final_ && head.support.unwrap().explicitly_incomplete);
                let retained = super::memo(&db, ingredient, b).unwrap();
                let before = retained.transfer_test_snapshot();
                let value = *retained.value().unwrap();
                assert!(!before.final_ && !before.support.unwrap().explicitly_incomplete);
                classify(&db, key, MemoReuse::Incomplete);
                let work = executions(key);
                let (delivered, observations) = trace::collect(
                    TraceConfig {
                        worker: 0,
                        ordinal: Arc::new(AtomicUsize::new(0)),
                    },
                    || super::fixpoint(&db, b),
                );
                assert_eq!(delivered, value);
                assert_eq!(executions(key), work);
                let after = retained.transfer_test_snapshot();
                assert_eq!(after.identity, before.identity);
                assert!(!after.final_);
                assert!(!observations.broken);
                assert!(observations.records.iter().any(|record| {
                    record.event.phase == Some("finality.head") && !record.event.decision
                }));
                assert!(
                    !observations
                        .records
                        .iter()
                        .any(|record| record.event.phase == Some("finality.published"))
                );
            }),
            Ok(AttemptOutcome::Incomplete(reason))
        );
        clean(&db);
        let old = super::memo(&db, ingredient, b).unwrap();
        let work = executions(key);
        assert_eq!(
            try_with_attempt(&db, 100, || {
                ingredient
                    .maybe_changed_after(&db, b.as_id(), old.header.verified_at.load())
                    .is_unchanged()
            }),
            Ok(AttemptOutcome::Complete(false))
        );
        assert!(old.header.may_be_provisional());
        assert_eq!(executions(key), work);
        clean(&db);
    }
}

#[test]
fn poisoned_head_cannot_certify_its_abandoned_participant() {
    let _reset = reset();
    let mut db = Db::default();
    let (a, b, key) = pair(&mut db);
    let ingredient = super::fixpoint::fn_ingredient_(&db, db.zalsa());
    let root = ingredient.database_key_index(a.as_id());
    let identity = Arc::new(());
    arm(
        root,
        Point::Iterate,
        Incomplete::Interrupted,
        CallbackUse::Report,
    );
    STATE.with_borrow_mut(|state| state.panic = Some(identity.clone()));
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        try_with_attempt(&db, 100, || super::fixpoint(&db, a))
    }));
    assert!(Arc::ptr_eq(
        &outcome
            .unwrap_err()
            .downcast_ref::<NativeFailure>()
            .unwrap()
            .0,
        &identity
    ));
    clean(&db);
    let head = super::memo(&db, ingredient, a)
        .unwrap()
        .transfer_test_snapshot();
    assert!(!head.final_ && !head.has_value);
    let old = super::memo(&db, ingredient, b).unwrap();
    assert!(old.header.may_be_provisional());
    let work = executions(key);
    assert_eq!(
        try_with_attempt(&db, 100, || {
            ingredient
                .maybe_changed_after(&db, b.as_id(), old.header.verified_at.load())
                .is_unchanged()
        }),
        Ok(AttemptOutcome::Complete(false))
    );
    assert!(old.header.may_be_provisional());
    assert_eq!(executions(key), work);
    clean(&db);
}

#[test]
fn selected_completed_head_survives_its_owners_concurrent_outcome() {
    for finish in [
        Finish::Complete,
        Finish::Refuse(Incomplete::Allowance),
        Finish::Refuse(Incomplete::Interrupted),
        Finish::Abandon,
    ] {
        for controlled in [false, true] {
            let mut db = Db::default();
            let (nodes, values, [root, key]) = fallback_pair(&mut db, false);
            let (ready_tx, ready_rx) = mpsc::channel();
            let (control_tx, control_rx) = mpsc::channel::<trace::FinalityPauseControl>();
            let (left, right) = run_pair(
                &db,
                move |db, participant| {
                    let _reset = reset();
                    let identity = Arc::new(());
                    let mut release = None;
                    let outcome = catch_unwind(AssertUnwindSafe(|| {
                        participant.run(&db, 100, || {
                            assert_eq!(super::fallback(&db, nodes[0]), values[0]);
                            let before = header(&db, key).transfer_test_snapshot(true);
                            let head = header(&db, root).transfer_test_snapshot(true);
                            assert!(!before.final_ && head.final_);
                            assert_eq!(before.support.unwrap().state, 0);
                            ready_tx.send((before, head)).unwrap();
                            release =
                                Some(control_rx.recv_timeout(Duration::from_secs(5)).unwrap());
                            release.as_ref().unwrap().wait_for_selection();
                            assert!(
                                db.zalsa()
                                    .lookup_ingredient(key.ingredient_index())
                                    .as_function()
                                    .unwrap()
                                    .sync_table()
                                    .test_transfer_state(key.key_index())
                                    .is_some()
                            );
                            match finish {
                                Finish::Refuse(reason) => {
                                    attempt_probe::report_incomplete(&db, reason);
                                }
                                Finish::Abandon => panic_any(NativeFailure(identity.clone())),
                                Finish::Complete => {}
                            }
                        })
                    }));
                    match finish {
                        Finish::Refuse(reason) => {
                            assert_eq!(outcome.unwrap(), Ok(AttemptOutcome::Incomplete(reason)))
                        }
                        Finish::Abandon => assert!(Arc::ptr_eq(
                            &outcome
                                .unwrap_err()
                                .downcast_ref::<NativeFailure>()
                                .unwrap()
                                .0,
                            &identity
                        )),
                        Finish::Complete => {
                            assert_eq!(outcome.unwrap(), Ok(AttemptOutcome::Complete(())))
                        }
                    }
                    assert_eq!(
                        header(&db, root)
                            .transfer_test_snapshot(true)
                            .support
                            .unwrap()
                            .state,
                        match finish {
                            Finish::Refuse(Incomplete::Allowance) => 1,
                            Finish::Refuse(Incomplete::Interrupted) => 4,
                            Finish::Refuse(Incomplete::RequestedAllocation) => 5,
                            Finish::Abandon => 3,
                            Finish::Complete => 2,
                        }
                    );
                    // The owner has already left its attempt when the retained proof resumes.
                    drop(release);
                },
                move |db, participant| {
                    let inspect = || {
                        let _reset = reset();
                        let (before, head) = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        let (pause, control) =
                            trace::finality_pause(key, before.identity, root, head.identity);
                        let _pause = trace::install_finality_pause(pause);
                        control_tx.send(control).unwrap();
                        let (value, observations) = trace::collect(
                            TraceConfig {
                                worker: 1,
                                ordinal: Arc::new(AtomicUsize::new(0)),
                            },
                            || super::fallback(&db, nodes[1]),
                        );
                        assert_eq!(value, values[1]);
                        assert_eq!(executions(key), 0);
                        assert_completed_identity(
                            before,
                            header(&db, key).transfer_test_snapshot(true),
                        );
                        assert!(!observations.broken);
                        let proof = observations
                            .records
                            .iter()
                            .find(|record| record.event.phase == Some("finality.head"))
                            .unwrap();
                        assert!(proof.event.decision);
                        assert_eq!(proof.event.memo.unwrap().identity, before.identity);
                        let selected = proof.event.other_memo.unwrap();
                        assert_eq!(selected.identity, head.identity);
                        assert!(
                            selected.final_ && !selected.support.unwrap().explicitly_incomplete
                        );
                        assert_eq!(
                            selected.support.unwrap().state,
                            match finish {
                                Finish::Refuse(Incomplete::Allowance) => 1,
                                Finish::Refuse(Incomplete::Interrupted) => 4,
                                Finish::Refuse(Incomplete::RequestedAllocation) => 5,
                                Finish::Abandon => 3,
                                Finish::Complete => 2,
                            }
                        );
                        let published = observations
                            .records
                            .iter()
                            .find(|record| record.event.phase == Some("finality.published"))
                            .unwrap();
                        assert!(published.ordinal > proof.ordinal);
                        assert_eq!(published.event.memo.unwrap().identity, before.identity);
                    };
                    if controlled {
                        assert_eq!(
                            participant.run(&db, 100, inspect),
                            Ok(AttemptOutcome::Complete(()))
                        );
                    } else {
                        assert_eq!(participant.run_ordinary(&db, inspect), Ok(()));
                    }
                },
            )
            .unwrap();
            left.unwrap();
            right.unwrap();
            clean(&db);
        }
    }
}

#[test]
fn unfinished_foreign_participant_waits_for_its_transferred_claim() {
    for controlled in [false, true] {
        let mut db = Db::default();
        let (a, b, key) = pair(&mut db);
        let root = super::fixpoint::fn_ingredient_(&db, db.zalsa()).database_key_index(a.as_id());
        let (ready_tx, ready_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (left, right) = run_pair(
            &db,
            move |db, participant| {
                participant.run(&db, 100, || {
                    let _reset = reset();
                    arm(
                        root,
                        Point::Iterate,
                        Incomplete::Interrupted,
                        CallbackUse::Quiet,
                    );
                    STATE.with_borrow_mut(|state| {
                        state.iteration_pause = Some((ready_tx, resume_rx))
                    });
                    assert_eq!(super::fixpoint(&db, a), 3);
                    assert!(
                        STATE.with_borrow(|state| state.fired && state.iteration_pause.is_none())
                    );
                })
            },
            move |db, participant| {
                let inspect = || {
                    let _reset = reset();
                    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    let ingredient = super::fixpoint::fn_ingredient_(&db, db.zalsa());
                    let old = super::memo(&db, ingredient, b).unwrap();
                    let before = old.transfer_test_snapshot();
                    let head = header(&db, root).transfer_test_snapshot(true);
                    assert!(!before.final_ && !head.final_);
                    assert_eq!(before.support.unwrap().state, 0);
                    assert_eq!(head.support.unwrap().state, 0);
                    let ownership = ingredient
                        .sync_table
                        .test_transfer_state(b.as_id())
                        .unwrap();
                    assert!(matches!(
                        ownership.owner,
                        crate::function::SyncOwner::Transferred
                    ));
                    let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
                    assert!(!graph.transferred.overflow);
                    assert!(
                        graph
                            .transferred
                            .entries
                            .into_iter()
                            .flatten()
                            .any(|(query, _, owner)| query == key && owner == root)
                    );
                    STATE.with_borrow_mut(|state| state.wait_release = Some((key, resume_tx)));
                    let (value, observations) = trace::collect(
                        TraceConfig {
                            worker: 1,
                            ordinal: Arc::new(AtomicUsize::new(0)),
                        },
                        || {
                            classify(&db, key, MemoReuse::Stale);
                            super::fixpoint(&db, b)
                        },
                    );
                    assert_eq!(value, 3);
                    assert!(STATE.with_borrow(|state| state.wait_release.is_none()));
                    assert!(!observations.broken);
                    let wait = observations
                        .records
                        .iter()
                        .find(|record| record.event.phase == Some("unfinished participant wait"))
                        .unwrap();
                    assert_eq!(wait.event.key, Some(key));
                    let before_wait = || {
                        observations
                            .records
                            .iter()
                            .filter(|record| record.ordinal < wait.ordinal)
                    };
                    assert!(before_wait().any(|record| record.event.kind == Kind::Reuse
                        && record.event.identity == before.identity
                        && record.event.reuse == Some(MemoReuse::Stale)));
                    assert!(
                        before_wait().any(|record| record.event.kind == Kind::SeedAllowed
                            && record.event.identity == before.identity
                            && !record.event.decision)
                    );
                    // The transferred claim excludes proof and execution until the producer
                    // releases it. Merely seeing a candidate cannot bypass this ownership.
                    assert!(!before_wait().any(|record| matches!(
                        record.event.phase,
                        Some("finality.head" | "finality.published")
                    )));
                    assert!(!before_wait().any(|record| matches!(
                        record.event.kind,
                        Kind::BodyValue | Kind::InitialValue | Kind::RecoveryValue | Kind::Executed
                    )));
                    assert!(
                        !before_wait().any(|record| record.event.kind == Kind::SeedAllowed
                            && record.event.decision)
                    );
                    assert!(!observations.records.iter().any(|record| {
                        record.event.phase == Some("finality.published")
                            && record
                                .event
                                .memo
                                .is_some_and(|memo| memo.identity == before.identity)
                    }));
                    assert!(old.header.may_be_provisional());
                    assert_ne!(
                        header(&db, key).transfer_test_snapshot(true).identity,
                        before.identity
                    );
                };
                if controlled {
                    assert_eq!(
                        participant.run(&db, 100, inspect),
                        Ok(AttemptOutcome::Complete(()))
                    );
                } else {
                    assert_eq!(participant.run_ordinary(&db, inspect), Ok(()));
                }
            },
        )
        .unwrap();
        assert_eq!(left.unwrap(), Ok(AttemptOutcome::Complete(())));
        right.unwrap();
        assert_eq!(super::fixpoint(&db, a), 3);
        assert_eq!(super::fixpoint(&db, b), 3);
        clean(&db);
    }
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = skipped_initial, cycle_fn = skipped_recover)]
fn skipping_head(db: &dyn Database, node: Node) -> u32 {
    let previous = skipping_head(db, node);
    if previous == 0 {
        let _ = skipped_participant(db, node);
    }
    (previous + 1).min(2)
}

#[crate::tracked(returns(copy), attempt = ReturnOnly, cycle_initial = skipped_initial, cycle_fn = skipped_recover)]
fn skipped_participant(db: &dyn Database, node: Node) -> u32 {
    skipping_head(db, node) + 10
}

fn skipped_initial(db: &dyn Database, _id: Id, node: Node) -> u32 {
    node.seed(db)
}

fn skipped_recover(
    _db: &dyn Database,
    _cycle: &Cycle<'_>,
    _last: &u32,
    value: u32,
    _node: Node,
) -> u32 {
    value
}

#[test]
fn a_final_head_cannot_certify_a_participant_from_its_skipped_iteration() {
    let _reset = reset();
    let db = Db::default();
    let node = super::Node::new(&db, None, 0);
    let head_ingredient = skipping_head::fn_ingredient_(&db, db.zalsa());
    let participant_ingredient = skipped_participant::fn_ingredient_(&db, db.zalsa());
    let head_key = head_ingredient.database_key_index(node.as_id());
    let participant_key = participant_ingredient.database_key_index(node.as_id());
    assert_eq!(
        try_with_attempt(&db, 100, || {
            assert_eq!(skipping_head(&db, node), 2);
            let old = super::memo(&db, participant_ingredient, node).unwrap();
            let head = super::memo(&db, head_ingredient, node).unwrap();
            assert_eq!(old.value(), Some(&10));
            assert_eq!(head.value(), Some(&2));
            let before = old.transfer_test_snapshot();
            let accepted_head = head.transfer_test_snapshot();
            assert!(!before.final_ && before.has_value);
            assert!(accepted_head.final_ && accepted_head.has_value);
            assert!(!old.header.has_incomplete_attempt() && !head.header.has_incomplete_attempt());
            assert!(old.header.same_attempt_owner(&head.header));
            assert_eq!(
                before.support.unwrap().owner,
                accepted_head.support.unwrap().owner
            );
            assert_eq!(
                before.execution_revision,
                Some(db.zalsa().current_revision())
            );
            assert_eq!(before.execution_revision, accepted_head.execution_revision);
            assert!(old.header.is_finality_candidate());
            assert!(!before.heads.overflow);
            let [Some((recorded_key, recorded_iteration)), None] = before.heads.entries else {
                panic!("skipped participant must retain exactly its initial head: {before:?}");
            };
            assert_eq!(recorded_key, head_key);
            assert_eq!(recorded_iteration.iteration(), 0);
            assert_eq!(accepted_head.iteration.iteration(), 2);
            assert_eq!(
                recorded_iteration.cancellation_count(),
                accepted_head.iteration.cancellation_count()
            );
            assert_ne!(recorded_iteration, accepted_head.iteration);
            let head_work = executions(head_key);
            let participant_work = executions(participant_key);
            let (value, observations) = trace::collect(
                TraceConfig {
                    worker: 0,
                    ordinal: Arc::new(AtomicUsize::new(0)),
                },
                || skipped_participant(&db, node),
            );
            assert_eq!(value, 12);
            assert_eq!(executions(head_key), head_work);
            assert_eq!(executions(participant_key), participant_work + 1);
            assert!(!observations.broken);
            let rejected = observations
                .records
                .iter()
                .find(|record| {
                    record.event.phase == Some("finality.head")
                        && record
                            .event
                            .memo
                            .is_some_and(|memo| memo.identity == before.identity)
                })
                .expect("native fetch checked the retained participant's head");
            assert!(!rejected.event.decision);
            assert_eq!(rejected.event.key, Some(participant_key));
            assert_eq!(rejected.event.other_key, Some(head_key));
            assert_eq!(
                rejected.event.other_memo.unwrap().identity,
                accepted_head.identity
            );
            assert!(rejected.event.other_memo.unwrap().final_);
            assert!(!observations.records.iter().any(|record| {
                record.event.phase == Some("finality.published")
                    && record
                        .event
                        .memo
                        .is_some_and(|memo| memo.identity == before.identity)
            }));
            assert!(old.header.may_be_provisional());
            assert_eq!(old.value(), Some(&10));
            let replacement = super::memo(&db, participant_ingredient, node).unwrap();
            assert_ne!(
                replacement.transfer_test_snapshot().identity,
                before.identity
            );
            assert!(!replacement.header.may_be_provisional());
            assert_eq!(replacement.value(), Some(&12));
            assert_eq!(
                super::memo(&db, head_ingredient, node)
                    .unwrap()
                    .transfer_test_snapshot()
                    .identity,
                accepted_head.identity
            );
        }),
        Ok(AttemptOutcome::Complete(()))
    );
    clean(&db);
}
