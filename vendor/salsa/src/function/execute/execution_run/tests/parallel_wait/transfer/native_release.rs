//! Real native claims establish the transfer depth before a small-stack worker releases it.
//! Query bodies are deliberately not executed; the parent test covers semantic restart/refusal.

use std::cell::Cell;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use super::{Db, Input, a, assert_graph_clean};
use crate::attempt_probe::transfer_test_support::{
    self as trace, Action, Event, Kind, Record, TraceConfig, TransferTrace,
};
use crate::attempt_probe::{self, AttemptOutcome, Incomplete};
use crate::function::sync::ReleaseMode;
use crate::function::{ClaimGuard, ClaimResult, Reentrancy, SyncOwner};
use crate::plumbing::AsId;
use crate::runtime::WaitResult;
use crate::zalsa::ZalsaDatabase;
use crate::{Cancelled, Database, DatabaseKeyIndex};

const STACK_BYTES: usize = 256 * 1024;
const DEPTH_KEYS: usize = 2_048;
const STAGE_TIMEOUT: Duration = Duration::from_secs(5);
const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const CHILD_MARKER: &str = "SALSA_NATIVE_TRANSFER_RELEASE_CHILD";
const CASE_MARKER: &str = "SALSA_NATIVE_TRANSFER_RELEASE_CASE";
const RELEASE_TEST: &str = "function::execute::execution_run::tests::parallel_wait::transfer::native_release::deep_native_transfer_release";
const HANDOFF_TEST: &str = "function::execute::execution_run::tests::parallel_wait::transfer::native_release::deep_native_transfer_handoff";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Finish {
    Complete,
    Refuse,
    Reclaimed,
    Panic,
    Local,
}

impl Finish {
    fn parse(value: &str) -> Self {
        match value {
            "complete" => Self::Complete,
            "refuse" => Self::Refuse,
            "reclaimed" => Self::Reclaimed,
            "panic" => Self::Panic,
            "local" => Self::Local,
            _ => panic!("unknown native release case: {value}"),
        }
    }
}

#[derive(Debug)]
struct NativeFailure(Arc<()>);

fn allocate_keys(db: &Db, count: usize) -> Vec<DatabaseKeyIndex> {
    let ingredient = a::fn_ingredient_(db, db.zalsa());
    (0..count)
        .map(|_| ingredient.database_key_index(Input::new(db, 3, 17).as_id()))
        .collect()
}

fn claim<'db>(db: &'db Db, key: DatabaseKeyIndex, reentrancy: Reentrancy) -> ClaimGuard<'db> {
    let function = db
        .zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap();
    match function
        .sync_table()
        .try_claim(db.zalsa(), db.zalsa_local(), key.key_index(), reentrancy)
    {
        ClaimResult::Claimed(guard) => guard,
        _ => panic!("fixture key was not available: {key:?}"),
    }
}

fn claim_all<'db>(db: &'db Db, keys: &[DatabaseKeyIndex]) -> Vec<ClaimGuard<'db>> {
    keys.iter()
        .map(|&key| claim(db, key, Reentrancy::Deny))
        .collect()
}

fn transfer_chain<'db>(mut guards: Vec<ClaimGuard<'db>>) -> ClaimGuard<'db> {
    // Every immediate parent is still thread-owned. Resolving an already-transferred
    // destination here would hide quadratic owner-chain lookup in fixture construction.
    while guards.len() > 1 {
        let mut child = guards.pop().unwrap();
        let parent = guards.last().unwrap().database_key_index();
        child.set_release_mode(ReleaseMode::TransferTo(parent));
        assert!(!child.drop(), "same-thread setup unexpectedly waited");
    }
    guards.pop().unwrap()
}

fn transfer_star<'db>(mut guards: Vec<ClaimGuard<'db>>) -> ClaimGuard<'db> {
    let root = guards[0].database_key_index();
    while guards.len() > 1 {
        let mut child = guards.pop().unwrap();
        child.set_release_mode(ReleaseMode::TransferTo(root));
        assert!(!child.drop());
    }
    guards.pop().unwrap()
}

fn marker(phase: &'static str) {
    let mut event = Event::new(Kind::Gate);
    event.phase = Some(phase);
    trace::record(event);
}

fn assert_chain_history(records: &[Record], keys: &[DatabaseKeyIndex]) {
    let ready = records
        .iter()
        .find(|record| record.event.phase == Some("release-ready"))
        .unwrap();
    let positions: HashMap<_, _> = keys
        .iter()
        .copied()
        .enumerate()
        .map(|(i, key)| (key, i))
        .collect();
    let mut seen = vec![false; keys.len()];
    let mut mappings = 0;
    for record in records
        .iter()
        .filter(|record| record.ordinal < ready.ordinal)
    {
        assert!(!matches!(
            record.event.kind,
            Kind::Undo | Kind::MappingRemoved
        ));
        if record.event.kind == Kind::Mapping {
            let index = positions[&record.event.key.unwrap()];
            assert!(
                index > 0 && !seen[index],
                "duplicate or root transfer: {record:?}"
            );
            assert_eq!(record.event.other_key, Some(keys[index - 1]));
            assert_eq!(record.event.peer, Some(ready.thread));
            seen[index] = true;
            mappings += 1;
        }
    }
    assert_eq!(mappings, keys.len() - 1);
    assert!(seen[1..].iter().all(|seen| *seen));
    eprintln!(
        "NATIVE_RELEASE proven actual transfer depth {}",
        keys.len() - 1
    );
}

fn assert_claim_terminals(records: &[Record]) {
    let mut claims = HashMap::new();
    for record in records {
        if record.event.kind == Kind::Claim {
            assert!(
                claims
                    .insert(record.event.serial.unwrap(), (record.event.key, 0))
                    .is_none()
            );
        } else if record.event.kind == Kind::Terminal {
            let entry = claims.get_mut(&record.event.serial.unwrap()).unwrap();
            assert_eq!(entry.0, record.event.key);
            entry.1 += 1;
        }
    }
    assert!(!claims.is_empty());
    assert!(claims.values().all(|(_, count)| *count == 1));
    assert!(!records.iter().any(|record| matches!(
        record.event.kind,
        Kind::BodyValue
            | Kind::InitialValue
            | Kind::RecoveryValue
            | Kind::RootPublished
            | Kind::TargetPublished
    )));
}

fn assert_released(db: &Db, keys: &[DatabaseKeyIndex]) {
    assert_graph_clean(db.zalsa().runtime().test_transfer_graph_snapshot());
    for key in keys {
        let function = db
            .zalsa()
            .lookup_ingredient(key.ingredient_index())
            .as_function()
            .unwrap();
        if let Some(state) = function.sync_table().test_transfer_state(key.key_index()) {
            assert!(
                matches!(state.owner, SyncOwner::Transferred),
                "live claim: {key:?} {state:?}"
            );
            assert!(!state.claimed_twice);
        }
    }
    assert!(db.zalsa_local().active_query().is_none());
    assert_eq!(attempt_probe::stack_depths(), (0, 0));
    assert_eq!(db.counts.snapshot(), [0; 6]);
}

fn wait_until_registered(db: &Db, waiter: ThreadId, key: DatabaseKeyIndex) {
    let deadline = Instant::now() + STAGE_TIMEOUT;
    loop {
        let graph = db.zalsa().runtime().test_transfer_graph_snapshot();
        assert!(!graph.edges.overflow && !graph.dependents.overflow);
        if graph
            .edges
            .entries
            .iter()
            .flatten()
            .any(|(from, _)| *from == waiter)
            && graph
                .dependents
                .entries
                .iter()
                .flatten()
                .any(|(query, threads)| {
                    assert!(!threads.overflow);
                    *query == key
                        && threads
                            .entries
                            .iter()
                            .flatten()
                            .any(|thread| *thread == waiter)
                })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "wait edge was not installed: {waiter:?} {key:?}"
        );
        thread::yield_now();
    }
}

struct WaiterReport {
    result: thread::Result<bool>,
    trace: TransferTrace,
}

fn waiter(db: Db, key: DatabaseKeyIndex, config: TraceConfig) -> thread::JoinHandle<WaiterReport> {
    thread::spawn(move || {
        let (result, trace) = trace::collect(config, || {
            catch_unwind(AssertUnwindSafe(|| {
                let function = db
                    .zalsa()
                    .lookup_ingredient(key.ingredient_index())
                    .as_function()
                    .unwrap();
                let ClaimResult::Running(running) = function.sync_table().try_claim(
                    db.zalsa(),
                    db.zalsa_local(),
                    key.key_index(),
                    Reentrancy::Deny,
                ) else {
                    panic!("waiter did not encounter the real claim");
                };
                running.block_on(db.zalsa())
            }))
        });
        WaiterReport { result, trace }
    })
}

fn release_body(
    db: &Db,
    keys: &[DatabaseKeyIndex],
    finish: Finish,
    identity: &Arc<()>,
    ordinal: &Arc<AtomicUsize>,
) -> (thread::Result<()>, Vec<WaiterReport>, TransferTrace) {
    let setup = Instant::now();
    let ((root, reclaimed, waiters), mut observations) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: ordinal.clone(),
        },
        || {
            marker("setup-start");
            eprintln!("NATIVE_RELEASE {finish:?} keys={} setup-start", keys.len());
            let root = transfer_chain(claim_all(db, keys));
            let mut waiters = Vec::new();
            for (worker, key) in [keys[0], *keys.last().unwrap()].into_iter().enumerate() {
                let handle = waiter(
                    db.clone(),
                    key,
                    TraceConfig {
                        worker: worker + 1,
                        ordinal: ordinal.clone(),
                    },
                );
                wait_until_registered(db, handle.thread().id(), key);
                waiters.push(handle);
            }
            let reclaimed = (finish == Finish::Reclaimed)
                .then(|| claim(db, keys[keys.len() / 2], Reentrancy::Allow));
            marker("release-ready");
            (root, reclaimed, waiters)
        },
    );
    assert!(!observations.broken);
    assert_chain_history(&observations.records, keys);
    eprintln!(
        "NATIVE_RELEASE {finish:?} keys={} setup={:?} release-ready",
        keys.len(),
        setup.elapsed()
    );
    let ((result, reports), cleanup) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: ordinal.clone(),
        },
        || {
            let started = Cell::new(None);
            let result = catch_unwind(AssertUnwindSafe(|| {
                let root = root;
                match finish {
                    Finish::Complete => {
                        started.set(Some(Instant::now()));
                        assert!(!root.drop());
                    }
                    Finish::Refuse | Finish::Reclaimed => {
                        assert_eq!(attempt_probe::charge(db, 1), Err(Incomplete::Allowance));
                        let refused = trace::session_snapshot().unwrap();
                        assert_eq!(refused.remaining, 0);
                        started.set(Some(Instant::now()));
                        if let Some(reclaimed) = reclaimed {
                            reclaimed.abort();
                        }
                        root.abort();
                        assert_eq!(
                            trace::session_snapshot(),
                            Some(refused),
                            "cleanup changed its refused session"
                        );
                    }
                    Finish::Panic => {
                        started.set(Some(Instant::now()));
                        let _root = root;
                        panic_any(NativeFailure(identity.clone()));
                    }
                    Finish::Local => {
                        db.cancellation_token().cancel();
                        started.set(Some(Instant::now()));
                        let _root = root;
                        db.zalsa().unwind_if_revision_cancelled(db.zalsa_local());
                        panic!("local cancellation did not unwind");
                    }
                }
            }));
            let cleanup_elapsed = started.get().unwrap().elapsed();
            marker("release-finished");
            eprintln!(
                "NATIVE_RELEASE {finish:?} cleanup-or-unwind={cleanup_elapsed:?} release-finished"
            );
            let reports = waiters
                .into_iter()
                .map(|waiter| waiter.join().unwrap())
                .collect();
            (result, reports)
        },
    );
    observations.broken |= cleanup.broken;
    observations.records.extend(cleanup.records);
    (result, reports, observations)
}

fn release_case(finish: Finish, count: usize) {
    let db = Db::default();
    let keys = allocate_keys(&db, count);
    let identity = Arc::new(());
    let ordinal = Arc::new(AtomicUsize::new(0));
    let owner_trace = crate::attach(&db, || {
        let (result, reports, owner_trace) = if matches!(finish, Finish::Refuse | Finish::Reclaimed)
        {
            let mut terminal = None;
            let outcome = attempt_probe::try_with_attempt(&db, 0, || {
                terminal = Some(release_body(&db, &keys, finish, &identity, &ordinal));
            })
            .unwrap();
            assert_eq!(outcome, AttemptOutcome::Incomplete(Incomplete::Allowance));
            terminal.unwrap()
        } else {
            release_body(&db, &keys, finish, &identity, &ordinal)
        };
        match finish {
            Finish::Panic => assert!(Arc::ptr_eq(
                &result.unwrap_err().downcast::<NativeFailure>().unwrap().0,
                &identity
            )),
            Finish::Local => assert!(matches!(
                *result.unwrap_err().downcast::<Cancelled>().unwrap(),
                Cancelled::Local
            )),
            _ => result.unwrap(),
        }
        for report in reports {
            assert!(!report.trace.broken);
            let expected = match finish {
                Finish::Complete => WaitResult::Completed,
                Finish::Panic => WaitResult::Panicked,
                _ => WaitResult::Cancelled,
            };
            let consumed: Vec<_> = report
                .trace
                .records
                .iter()
                .filter(|record| record.event.kind == Kind::WaitConsumed)
                .collect();
            assert_eq!(consumed.len(), 1);
            assert!(matches!(
                (consumed[0].event.wait, expected),
                (Some(WaitResult::Completed), WaitResult::Completed)
                    | (Some(WaitResult::Cancelled), WaitResult::Cancelled)
                    | (Some(WaitResult::Panicked), WaitResult::Panicked)
            ));
            if finish == Finish::Panic {
                assert!(matches!(
                    *report.result.unwrap_err().downcast::<Cancelled>().unwrap(),
                    Cancelled::PropagatedPanic
                ));
            } else {
                assert_eq!(report.result.unwrap(), finish == Finish::Complete);
            }
        }
        owner_trace
    });
    assert!(!owner_trace.broken);
    assert_claim_terminals(&owner_trace.records);
    let root_terminal = owner_trace
        .records
        .iter()
        .find(|record| record.event.kind == Kind::Terminal && record.event.key == Some(keys[0]))
        .unwrap();
    let expected_action = match finish {
        Finish::Complete => Action::Drop,
        Finish::Refuse | Finish::Reclaimed => Action::Abort,
        Finish::Panic | Finish::Local => Action::Panic,
    };
    assert_eq!(root_terminal.event.action, Some(expected_action));
    if finish == Finish::Reclaimed {
        assert!(owner_trace.records.iter().any(|record| {
            record.event.kind == Kind::Undo && record.event.key == Some(keys[keys.len() / 2])
        }));
    }
    let ready = owner_trace
        .records
        .iter()
        .find(|record| record.event.phase == Some("release-ready"))
        .unwrap();
    assert!(
        !owner_trace
            .records
            .iter()
            .any(|record| record.ordinal > ready.ordinal
                && matches!(
                    record.event.kind,
                    Kind::DebitAccepted
                        | Kind::Admission
                        | Kind::BodyValue
                        | Kind::InitialValue
                        | Kind::RecoveryValue
                ))
    );
    assert_released(&db, &keys);
    assert!(attempt_probe::current().is_none());
    let followup = db.clone();
    for &key in &keys {
        assert!(!claim(&followup, key, Reentrancy::Deny).drop());
    }
    assert_released(&followup, &keys);
    assert_eq!(
        attempt_probe::try_with_attempt(&followup, 0, || ()),
        Ok(AttemptOutcome::Complete(()))
    );
}

fn handoff_case(count: usize) {
    let db = Db::default();
    let all_keys = allocate_keys(&db, count + 1);
    let (keys, extra) = all_keys.split_at(count);
    let target = extra[0];
    let source_thread = thread::current().id();
    let ordinal = Arc::new(AtomicUsize::new(0));
    let ((root, receiver, observer), mut owner_trace) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: ordinal.clone(),
        },
        || {
            marker("setup-start");
            let setup = Instant::now();
            eprintln!("NATIVE_HANDOFF keys={count} setup-start");
            let root = transfer_chain(claim_all(&db, keys));
            let middle = keys[count / 2];
            let leaf = *keys.last().unwrap();
            let observer = waiter(
                db.clone(),
                middle,
                TraceConfig {
                    worker: 2,
                    ordinal: ordinal.clone(),
                },
            );
            let observer_thread = observer.thread().id();
            wait_until_registered(&db, observer_thread, middle);
            let receiver_db = db.clone();
            let receiver_ordinal = ordinal.clone();
            let receiver = thread::Builder::new()
                .stack_size(STACK_BYTES)
                .spawn(move || {
                    trace::collect(
                        TraceConfig {
                            worker: 1,
                            ordinal: receiver_ordinal,
                        },
                        || {
                            let target_claim = claim(&receiver_db, target, Reentrancy::Deny);
                            let function = receiver_db
                                .zalsa()
                                .lookup_ingredient(leaf.ingredient_index())
                                .as_function()
                                .unwrap();
                            let ClaimResult::Running(running) = function.sync_table().try_claim(
                                receiver_db.zalsa(),
                                receiver_db.zalsa_local(),
                                leaf.key_index(),
                                Reentrancy::Deny,
                            ) else {
                                panic!("new owner did not block on the transferred leaf");
                            };
                            assert!(running.block_on(receiver_db.zalsa()));
                            wait_until_registered(&receiver_db, source_thread, target);
                            let graph =
                                receiver_db.zalsa().runtime().test_transfer_graph_snapshot();
                            assert!(!graph.edges.overflow);
                            let receiver_thread = thread::current().id();
                            for expected in [source_thread, observer_thread] {
                                assert!(
                                    graph.edges.entries.iter().flatten().any(|&(from, to)| from
                                        == expected
                                        && to == receiver_thread)
                                );
                            }
                            eprintln!("NATIVE_HANDOFF actual donor and observer edges remapped");
                            assert!(!target_claim.drop());
                        },
                    )
                })
                .unwrap();
            wait_until_registered(&db, receiver.thread().id(), leaf);
            marker("release-ready");
            eprintln!("NATIVE_HANDOFF keys={count} setup={:?}", setup.elapsed());
            (root, receiver, observer)
        },
    );
    assert!(!owner_trace.broken);
    assert_chain_history(&owner_trace.records, keys);
    eprintln!("NATIVE_HANDOFF keys={count} release-ready");
    let (reports, handoff_trace) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: ordinal.clone(),
        },
        || {
            let started = Instant::now();
            let mut root = root;
            root.set_release_mode(ReleaseMode::TransferTo(target));
            assert!(root.drop(), "blocked donor must refetch");
            eprintln!(
                "NATIVE_HANDOFF transfer-and-wait={:?} release-finished",
                started.elapsed()
            );
            marker("release-finished");
            (receiver.join().unwrap().1, observer.join().unwrap())
        },
    );
    owner_trace.broken |= handoff_trace.broken;
    owner_trace.records.extend(handoff_trace.records);
    let (receiver_trace, observer) = reports;
    assert!(!owner_trace.broken && !receiver_trace.broken && !observer.trace.broken);
    assert!(observer.result.unwrap());
    for (trace, key) in [
        (&receiver_trace, *keys.last().unwrap()),
        (&observer.trace, keys[count / 2]),
    ] {
        let consumed: Vec<_> = trace
            .records
            .iter()
            .filter(|record| record.event.kind == Kind::WaitConsumed)
            .collect();
        assert_eq!(consumed.len(), 1);
        assert_eq!(consumed[0].event.key, Some(key));
        assert!(matches!(
            consumed[0].event.wait,
            Some(WaitResult::Completed)
        ));
        assert!(
            trace
                .records
                .iter()
                .all(|record| record.event.session.is_none())
        );
    }
    let mut records = owner_trace.records;
    records.extend(receiver_trace.records);
    records.extend(observer.trace.records);
    records.sort_by_key(|record| record.ordinal);
    assert_claim_terminals(&records);
    assert_eq!(
        records
            .iter()
            .filter(|record| record.event.kind == Kind::MappingRemoved)
            .count(),
        count
    );
    assert!(
        records
            .iter()
            .any(|record| record.event.kind == Kind::EdgeRemap
                && record.event.key == Some(keys[count / 2]))
    );
    assert!(
        records
            .iter()
            .any(|record| record.event.kind == Kind::TransferWaitEnd
                && record.event.decision
                && matches!(record.event.wait, Some(WaitResult::Completed)))
    );
    assert_released(&db, &all_keys);
}

fn untraced_scaling() {
    let db = Db::default();
    let keys = allocate_keys(&db, 8_192);
    // These timings exclude trace collection, fixture construction, result checking, and
    // input allocation. Debug duplicate checks make wide-star construction quadratic.
    // Reusing the same database also exercises large-then-small retained table capacity.
    for count in [256, 2_048, 8_192, 8] {
        for star in [false, true] {
            let setup = Instant::now();
            let guards = claim_all(&db, &keys[..count]);
            let root = if star {
                transfer_star(guards)
            } else {
                transfer_chain(guards)
            };
            let setup_elapsed = setup.elapsed();
            eprintln!(
                "NATIVE_SCALING keys={count} star={star} setup={setup_elapsed:?} release-ready"
            );
            let release = Instant::now();
            let refetch = root.drop();
            let release_elapsed = release.elapsed();
            assert!(!refetch);
            eprintln!(
                "NATIVE_SCALING keys={count} star={star} release={release_elapsed:?} release-finished"
            );
            assert_released(&db, &keys);
        }
    }
}

fn isolated_case(test: &str, case: &str) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env(CHILD_MARKER, test)
        .env(CASE_MARKER, case)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + PROCESS_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let output = child.wait_with_output().unwrap();
            let output = String::from_utf8(output.stdout).unwrap();
            print!("{output}");
            assert!(
                status.success(),
                "native transfer child {case} failed: {status}"
            );
            assert!(
                output.contains(&format!("NATIVE_RELEASE_CHILD_FINISHED {case}")),
                "child did not run its requested fixture: {case}"
            );
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let status = child.wait().unwrap();
            panic!("native transfer child {case} exceeded {PROCESS_TIMEOUT:?}: {status}");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn deep_native_transfer_release() {
    if std::env::var(CHILD_MARKER).as_deref() == Ok(RELEASE_TEST) {
        let case = std::env::var(CASE_MARKER).unwrap();
        if case == "scaling" {
            thread::Builder::new()
                .stack_size(STACK_BYTES)
                .spawn(untraced_scaling)
                .unwrap()
                .join()
                .unwrap();
            println!("NATIVE_RELEASE_CHILD_FINISHED {case}");
            return;
        }
        let (finish, count) = case.split_once(':').unwrap();
        let finish = Finish::parse(finish);
        let count = count.parse().unwrap();
        thread::Builder::new()
            .stack_size(STACK_BYTES)
            .spawn(move || release_case(finish, count))
            .unwrap()
            .join()
            .unwrap();
        println!("NATIVE_RELEASE_CHILD_FINISHED {case}");
        return;
    }
    for count in [8, DEPTH_KEYS] {
        for finish in ["complete", "refuse", "reclaimed", "panic", "local"] {
            isolated_case(RELEASE_TEST, &format!("{finish}:{count}"));
        }
    }
    isolated_case(RELEASE_TEST, "scaling");
}

#[test]
fn deep_native_transfer_handoff() {
    if std::env::var(CHILD_MARKER).as_deref() == Ok(HANDOFF_TEST) {
        let case = std::env::var(CASE_MARKER).unwrap();
        let count = case.parse().unwrap();
        thread::Builder::new()
            .stack_size(STACK_BYTES)
            .spawn(move || handoff_case(count))
            .unwrap()
            .join()
            .unwrap();
        println!("NATIVE_RELEASE_CHILD_FINISHED {case}");
        return;
    }
    for count in [8, DEPTH_KEYS] {
        isolated_case(HANDOFF_TEST, &count.to_string());
    }
}
