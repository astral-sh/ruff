//! Checked transfers use real claims and waits without executing the query bodies.

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, ThreadId};
use std::time::Duration;

use rustc_hash::FxHashMap;

use super::super::DependencyGraph;
use super::{assert_acyclic, assert_empty, measure_work};
use crate::attempt_probe::transfer_test_support::{
    self as trace, Action, Kind, Mode, TraceConfig, TransferTrace,
};
use crate::function::{ClaimGuard, ClaimResult, Reentrancy, SyncOwner};
use crate::hash::FxHashSet;
use crate::plumbing::AsId;
use crate::runtime::{RetirementQuote, TransferFailure, WaitResult};
use crate::zalsa::ZalsaDatabase;
use crate::{Database, DatabaseKeyIndex};

const STAGE_TIMEOUT: Duration = Duration::from_secs(5);

#[crate::db]
#[derive(Clone, Default)]
struct Db {
    storage: crate::Storage<Self>,
}

#[crate::db]
impl Database for Db {}

#[crate::input]
struct Input {
    #[returns(copy)]
    value: u32,
}

#[crate::tracked(returns(copy))]
fn query(db: &dyn Database, input: Input) -> u32 {
    input.value(db)
}

fn keys<const N: usize>(db: &Db) -> [DatabaseKeyIndex; N] {
    let ingredient = query::fn_ingredient_(db, db.zalsa());
    std::array::from_fn(|_| ingredient.database_key_index(Input::new(db, 0).as_id()))
}

fn claim(db: &Db, key: DatabaseKeyIndex, reentrancy: Reentrancy) -> ClaimGuard<'_> {
    let function = db
        .zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap();
    match function
        .sync_table()
        .try_claim(db.zalsa(), db.zalsa_local(), key.key_index(), reentrancy)
    {
        ClaimResult::Claimed(claim) => claim,
        _ => panic!("fixture query was not available: {key:?}"),
    }
}

fn traced_claim(db: &Db, key: DatabaseKeyIndex) -> (ClaimGuard<'_>, usize) {
    let (claim, observations) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: Arc::new(AtomicUsize::new(0)),
        },
        || claim(db, key, Reentrancy::Deny),
    );
    assert!(!observations.broken);
    let claims: Vec<_> = observations
        .records
        .iter()
        .filter(|record| record.event.kind == Kind::Claim)
        .collect();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].event.key, Some(key));
    assert_eq!(claims[0].event.mode, Some(Mode::Default));
    (claim, claims[0].event.serial.unwrap())
}

fn release_original(claim: ClaimGuard<'_>, key: DatabaseKeyIndex, serial: usize) {
    let (refetch, observations) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: Arc::new(AtomicUsize::new(0)),
        },
        || claim.drop(),
    );
    assert!(!refetch && !observations.broken);
    assert!(
        !observations
            .records
            .iter()
            .any(|record| record.event.kind == Kind::Claim)
    );
    let terminals: Vec<_> = observations
        .records
        .iter()
        .filter(|record| record.event.kind == Kind::Terminal)
        .collect();
    assert_eq!(terminals.len(), 1);
    assert_eq!(terminals[0].event.key, Some(key));
    assert_eq!(terminals[0].event.serial, Some(serial));
    assert_eq!(terminals[0].event.mode, Some(Mode::Default));
    assert_eq!(terminals[0].event.action, Some(Action::Drop));
    assert!(matches!(
        terminals[0].event.wait,
        Some(WaitResult::Completed)
    ));
}

fn wait_on(
    db: &Db,
    key: DatabaseKeyIndex,
    registered: &Sender<(ThreadId, DatabaseKeyIndex)>,
) -> bool {
    let function = db
        .zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap();
    let running = match function.sync_table().try_claim(
        db.zalsa(),
        db.zalsa_local(),
        key.key_index(),
        Reentrancy::Deny,
    ) {
        ClaimResult::Running(running) => running,
        _ => panic!("fixture query did not require a wait: {key:?}"),
    };
    // Running retains the graph mutex. The receiving thread can inspect the edge only
    // after block_on registers it and releases that mutex to wait.
    registered.send((thread::current().id(), key)).unwrap();
    running.block_on(db.zalsa())
}

fn registered(
    db: &Db,
    receiver: &Receiver<(ThreadId, DatabaseKeyIndex)>,
    key: DatabaseKeyIndex,
    owner: ThreadId,
) -> ThreadId {
    let (waiter, actual_key) = receiver.recv_timeout(STAGE_TIMEOUT).unwrap();
    assert_eq!(actual_key, key);
    let graph = db.zalsa().runtime().dependency_graph.lock();
    assert_eq!(graph.edges.0[&waiter].blocked_on_id, owner);
    assert!(graph.query_dependents[&key].contains(&waiter));
    assert_acyclic(&graph);
    waiter
}

fn transfer(claim: ClaimGuard<'_>, target: DatabaseKeyIndex) {
    let quote = claim.zalsa().runtime().retirement_quote(1).unwrap();
    match claim.retire_participant(target, false, &quote) {
        Ok(false) => {}
        _ => panic!("fixture transfer did not preserve the current dependency path"),
    }
}

fn retire<'db>(
    claim: ClaimGuard<'db>,
    target: DatabaseKeyIndex,
    must_refetch: bool,
    quote: &RetirementQuote,
) -> (
    Result<bool, (ClaimGuard<'db>, TransferFailure)>,
    TransferTrace,
) {
    let ((result, work), observations) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: Arc::new(AtomicUsize::new(0)),
        },
        || measure_work(|| claim.retire_participant(target, must_refetch, quote)),
    );
    assert!(!observations.broken);
    assert!(work.total() <= quote.units, "work exceeded its quote");
    if quote.transfers == 0 && quote.edges == 0 {
        assert_eq!(quote.scratch_bytes, 0);
        assert_eq!(
            work.total(),
            0,
            "empty retirement created a cursor or visited a graph loop"
        );
    }
    (result, observations)
}

fn assert_source_owned(db: &Db, key: DatabaseKeyIndex, reclaimed: bool) {
    let state = db
        .zalsa()
        .lookup_ingredient(key.ingredient_index())
        .as_function()
        .unwrap()
        .sync_table()
        .test_transfer_state(key.key_index())
        .unwrap();
    assert!(matches!(state.owner, SyncOwner::Thread(id) if id == thread::current().id()));
    assert_eq!(state.claimed_twice, reclaimed);
}

fn assert_no_retirement(observations: &TransferTrace) {
    assert!(!observations.records.iter().any(|record| matches!(
        record.event.kind,
        Kind::Terminal | Kind::TransferBegin | Kind::Mapping | Kind::Unblock
    )));
}

fn assert_committed_and_waited(observations: &TransferTrace, source: DatabaseKeyIndex) {
    for kind in [
        Kind::Terminal,
        Kind::TransferBegin,
        Kind::TransferWaitBegin,
        Kind::TransferWaitEnd,
    ] {
        assert_eq!(
            observations
                .records
                .iter()
                .filter(|record| { record.event.kind == kind && record.event.key == Some(source) })
                .count(),
            1,
            "source must have exactly one {kind:?} transition",
        );
    }
    assert!(observations.records.iter().any(|record| {
        record.event.kind == Kind::TransferEnd
            && record.event.key == Some(source)
            && record.event.decision
    }));
    assert!(observations.records.iter().any(|record| {
        record.event.kind == Kind::TransferWaitEnd
            && matches!(record.event.wait, Some(WaitResult::Completed))
    }));
}

fn assert_ownership(
    graph: &DependencyGraph,
    expected: &[(DatabaseKeyIndex, ThreadId, DatabaseKeyIndex)],
) {
    let expected: FxHashMap<_, _> = expected
        .iter()
        .map(|&(key, thread, parent)| (key, (thread, parent)))
        .collect();
    assert_eq!(graph.transferred, expected);
    let memberships: usize = graph
        .transferred_dependents
        .values()
        .map(|children| children.0.len())
        .sum();
    assert_eq!(memberships, expected.len());
    for (&child, &(_, parent)) in &expected {
        assert_eq!(
            graph.transferred_dependents[&parent]
                .iter()
                .filter(|&&key| key == child)
                .count(),
            1,
            "each forward mapping has exactly one reverse membership",
        );
    }
    for &start in graph.transferred.keys() {
        let mut seen = FxHashSet::default();
        let mut at = start;
        while let Some(&(_, parent)) = graph.transferred.get(&at) {
            assert!(seen.insert(at), "ownership rotation introduced a cycle");
            at = parent;
        }
    }
    assert_acyclic(graph);
}

#[test]
fn empty_graph_same_thread_commit_uses_no_cursor_or_graph_loop() {
    let db = Db::default();
    let [source_key, target_key] = keys(&db);
    let (source, serial) = traced_claim(&db, source_key);
    let target = claim(&db, target_key, Reentrancy::Deny);
    let quote = db.zalsa().runtime().retirement_quote(1).unwrap();
    assert_eq!(
        (
            quote.transfers,
            quote.edges,
            quote.units,
            quote.scratch_bytes
        ),
        (0, 0, 176, 0)
    );
    let (checked, work) = measure_work(|| source.check_participant_head(target_key, &quote));
    assert!(checked.is_ok());
    assert_eq!(work.total(), 0);
    let (result, observations) = retire(source, target_key, false, &quote);
    assert!(matches!(result, Ok(false)));
    let terminals: Vec<_> = observations
        .records
        .iter()
        .filter(|record| record.event.kind == Kind::Terminal)
        .collect();
    assert_eq!(terminals.len(), 1);
    assert_eq!(terminals[0].event.key, Some(source_key));
    assert_eq!(terminals[0].event.serial, Some(serial));
    for kind in [Kind::TransferBegin, Kind::TransferEnd, Kind::Mapping] {
        assert_eq!(
            observations
                .records
                .iter()
                .filter(|record| record.event.kind == kind && record.event.key == Some(source_key))
                .count(),
            1
        );
    }
    assert!(!observations.records.iter().any(|record| matches!(
        record.event.kind,
        Kind::TransferWaitBegin | Kind::TransferWaitEnd | Kind::EdgeRemap
    )));
    assert_ownership(
        &db.zalsa().runtime().dependency_graph.lock(),
        &[(source_key, thread::current().id(), target_key)],
    );
    assert!(!target.drop());
    assert_empty(&db.zalsa().runtime().dependency_graph.lock());
}

#[test]
fn empty_graph_mandatory_refetch_retains_the_original_claim() {
    let db = Db::default();
    let [source_key, target_key] = keys(&db);
    let (source, serial) = traced_claim(&db, source_key);
    let target = claim(&db, target_key, Reentrancy::Deny);
    let quote = db.zalsa().runtime().retirement_quote(1).unwrap();
    let (result, observations) = retire(source, target_key, true, &quote);
    let source = match result {
        Err((source, TransferFailure::Unavailable)) => source,
        _ => panic!("an empty graph cannot supply a mandatory-refetch waiter"),
    };
    assert!(!source.is_reclaimed_transfer());
    assert_source_owned(&db, source_key, false);
    assert_no_retirement(&observations);
    assert_empty(&db.zalsa().runtime().dependency_graph.lock());
    release_original(source, source_key, serial);
    assert!(!target.drop());
}

#[test]
fn empty_graph_remote_target_retains_the_original_claim() {
    let db = Db::default();
    let [source_key, target_key] = keys(&db);
    let (source, serial) = traced_claim(&db, source_key);
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    thread::scope(|scope| {
        let target_db = db.clone();
        let target = scope.spawn(move || {
            let target = claim(&target_db, target_key, Reentrancy::Deny);
            ready_tx.send(()).unwrap();
            release_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
            assert!(!target.drop());
        });
        ready_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        let quote = db.zalsa().runtime().retirement_quote(1).unwrap();
        let (result, observations) = retire(source, target_key, false, &quote);
        let source = match result {
            Err((source, TransferFailure::Unavailable)) => source,
            _ => panic!("a remote target without a dependency path is unavailable"),
        };
        assert!(!source.is_reclaimed_transfer());
        assert_source_owned(&db, source_key, false);
        assert_no_retirement(&observations);
        assert_empty(&db.zalsa().runtime().dependency_graph.lock());
        release_original(source, source_key, serial);
        release_tx.send(()).unwrap();
        target.join().unwrap();
    });
    assert_empty(&db.zalsa().runtime().dependency_graph.lock());
}

#[test]
fn empty_quote_transfer_growth_retains_the_original_claim() {
    let db = Db::default();
    let [source_key, target_key, additional_key] = keys(&db);
    let (source, serial) = traced_claim(&db, source_key);
    let target = claim(&db, target_key, Reentrancy::Deny);
    let quote = db.zalsa().runtime().retirement_quote(1).unwrap();
    transfer(claim(&db, additional_key, Reentrancy::Deny), target_key);
    let before = format!("{:?}", db.zalsa().runtime().dependency_graph.lock());
    let (result, observations) = retire(source, target_key, false, &quote);
    let source = match result {
        Err((source, TransferFailure::Requote)) => source,
        _ => panic!("the first transfer membership invalidates an empty quote"),
    };
    assert!(!source.is_reclaimed_transfer());
    assert_source_owned(&db, source_key, false);
    assert_no_retirement(&observations);
    assert_eq!(
        before,
        format!("{:?}", db.zalsa().runtime().dependency_graph.lock())
    );
    assert_ownership(
        &db.zalsa().runtime().dependency_graph.lock(),
        &[(additional_key, thread::current().id(), target_key)],
    );
    release_original(source, source_key, serial);
    assert!(!target.drop());
    assert_empty(&db.zalsa().runtime().dependency_graph.lock());
}

#[test]
fn empty_quote_wait_edge_growth_retains_the_original_claim() {
    let db = Db::default();
    let [source_key, target_key] = keys(&db);
    let (source, serial) = traced_claim(&db, source_key);
    let target = claim(&db, target_key, Reentrancy::Deny);
    let quote = db.zalsa().runtime().retirement_quote(1).unwrap();
    let (wait_tx, wait_rx) = mpsc::channel();
    thread::scope(|scope| {
        let waiter_db = db.clone();
        let waiter = scope.spawn(move || assert!(wait_on(&waiter_db, source_key, &wait_tx)));
        registered(&db, &wait_rx, source_key, thread::current().id());
        let before = format!("{:?}", db.zalsa().runtime().dependency_graph.lock());
        let (result, observations) = retire(source, target_key, false, &quote);
        let source = match result {
            Err((source, TransferFailure::Requote)) => source,
            _ => panic!("the first wait edge invalidates an empty quote"),
        };
        assert!(!source.is_reclaimed_transfer());
        assert_source_owned(&db, source_key, false);
        assert_no_retirement(&observations);
        assert_eq!(
            before,
            format!("{:?}", db.zalsa().runtime().dependency_graph.lock())
        );
        release_original(source, source_key, serial);
        waiter.join().unwrap();
        assert!(!target.drop());
    });
    assert_empty(&db.zalsa().runtime().dependency_graph.lock());
}

#[test]
fn unchanged_mapping_still_waits_and_refetches() {
    let db = Db::default();
    let [source_key, target_key, gate_key] = keys(&db);
    let source = claim(&db, source_key, Reentrancy::Deny);
    let gate = claim(&db, gate_key, Reentrancy::Deny);
    let current = thread::current().id();
    let (wait_tx, wait_rx) = mpsc::channel();
    thread::scope(|scope| {
        let target_db = db.clone();
        let target = scope.spawn(move || {
            let target = claim(&target_db, target_key, Reentrancy::Deny);
            assert!(wait_on(&target_db, gate_key, &wait_tx));
            assert!(wait_on(&target_db, source_key, &wait_tx));
            {
                let graph = target_db.zalsa().runtime().dependency_graph.lock();
                assert_ownership(&graph, &[(source_key, thread::current().id(), target_key)]);
                assert_eq!(graph.edges.0.len(), 1);
                assert_eq!(
                    graph.edges.0[&current].blocked_on_id,
                    thread::current().id()
                );
                assert_eq!(graph.query_dependents[&target_key].as_slice(), [current]);
            }
            assert!(!target.drop());
        });
        let target_thread = registered(&db, &wait_rx, gate_key, current);
        transfer(source, target_key);
        let source = claim(&db, source_key, Reentrancy::Allow);
        assert!(source.is_reclaimed_transfer());
        assert!(!gate.drop());
        assert_eq!(
            registered(&db, &wait_rx, source_key, current),
            target_thread
        );
        let quote = db.zalsa().runtime().retirement_quote(1).unwrap();
        let (result, observations) = retire(source, target_key, true, &quote);
        if !matches!(result, Ok(true)) {
            db.zalsa()
                .runtime()
                .unblock_queries_blocked_on(source_key, WaitResult::Cancelled);
        }
        assert!(matches!(result, Ok(true)));
        target.join().unwrap();
        assert_committed_and_waited(&observations, source_key);
    });
    assert_empty(&db.zalsa().runtime().dependency_graph.lock());
}

#[test]
fn dependent_target_without_a_source_subtree_waiter_retains_the_claim() {
    let db = Db::default();
    let [source_key, target_key, outside_key] = keys(&db);
    let source = claim(&db, source_key, Reentrancy::Deny);
    let outside = claim(&db, outside_key, Reentrancy::Deny);
    let current = thread::current().id();
    let (wait_tx, wait_rx) = mpsc::channel();
    thread::scope(|scope| {
        let target_db = db.clone();
        let target = scope.spawn(move || {
            let target = claim(&target_db, target_key, Reentrancy::Deny);
            assert!(wait_on(&target_db, outside_key, &wait_tx));
            assert!(!target.drop());
        });
        registered(&db, &wait_rx, outside_key, current);
        let before = format!("{:?}", db.zalsa().runtime().dependency_graph.lock());
        let quote = db.zalsa().runtime().retirement_quote(1).unwrap();
        let (result, observations) = retire(source, target_key, true, &quote);
        let source = match result {
            Err((source, TransferFailure::Unavailable)) => source,
            _ => panic!("a dependency outside the source subtree cannot supply its cut"),
        };
        assert!(!source.is_reclaimed_transfer());
        assert_source_owned(&db, source_key, false);
        assert_no_retirement(&observations);
        assert_eq!(
            before,
            format!("{:?}", db.zalsa().runtime().dependency_graph.lock())
        );
        assert!(!source.drop());
        assert!(!outside.drop());
        target.join().unwrap();
    });
    assert_empty(&db.zalsa().runtime().dependency_graph.lock());
}

#[test]
fn quote_growth_retains_the_original_reclaimed_source() {
    let db = Db::default();
    let [source_key, target_key, additional_key] = keys(&db);
    let target = claim(&db, target_key, Reentrancy::Deny);
    transfer(claim(&db, source_key, Reentrancy::Deny), target_key);
    let source = claim(&db, source_key, Reentrancy::Allow);
    let quote = db.zalsa().runtime().retirement_quote(1).unwrap();
    transfer(claim(&db, additional_key, Reentrancy::Deny), target_key);
    let before = format!("{:?}", db.zalsa().runtime().dependency_graph.lock());
    let (result, observations) = retire(source, target_key, false, &quote);
    let source = match result {
        Err((source, TransferFailure::Requote)) => source,
        _ => panic!("a new transfer membership invalidates the quote"),
    };
    assert!(source.is_reclaimed_transfer());
    assert_source_owned(&db, source_key, true);
    assert_no_retirement(&observations);
    assert_eq!(
        before,
        format!("{:?}", db.zalsa().runtime().dependency_graph.lock())
    );
    assert!(!source.drop());
    assert_ownership(
        &db.zalsa().runtime().dependency_graph.lock(),
        &[
            (source_key, thread::current().id(), target_key),
            (additional_key, thread::current().id(), target_key),
        ],
    );
    assert!(!target.drop());
    assert_empty(&db.zalsa().runtime().dependency_graph.lock());
}

#[test]
fn rotation_cannot_cut_a_waiter_in_the_detached_subtree() {
    let db = Db::default();
    let [source_key, branch_key, descendant_key, outer_key, gate_key] = keys(&db);
    let source = claim(&db, source_key, Reentrancy::Deny);
    let branch = claim(&db, branch_key, Reentrancy::Deny);
    let descendant = claim(&db, descendant_key, Reentrancy::Deny);
    let gate = claim(&db, gate_key, Reentrancy::Deny);
    let current = thread::current().id();
    let (wait_tx, wait_rx) = mpsc::channel();
    thread::scope(|scope| {
        let outer_db = db.clone();
        let outer = scope.spawn(move || {
            let outer = claim(&outer_db, outer_key, Reentrancy::Deny);
            assert!(wait_on(&outer_db, gate_key, &wait_tx));
            assert!(!wait_on(&outer_db, descendant_key, &wait_tx));
            assert!(!outer.drop());
        });
        registered(&db, &wait_rx, gate_key, current);
        transfer(descendant, branch_key);
        transfer(branch, source_key);
        transfer(source, outer_key);
        let source = claim(&db, source_key, Reentrancy::Allow);
        let descendant = claim(&db, descendant_key, Reentrancy::Allow);
        assert!(!gate.drop());
        registered(&db, &wait_rx, descendant_key, current);
        let before = format!("{:?}", db.zalsa().runtime().dependency_graph.lock());
        let quote = db.zalsa().runtime().retirement_quote(1).unwrap();
        let (result, observations) = retire(source, branch_key, true, &quote);
        let source = match result {
            Err((source, TransferFailure::Unavailable)) => source,
            _ => panic!("rotation must exclude the detached branch and all its descendants"),
        };
        assert!(source.is_reclaimed_transfer());
        assert_source_owned(&db, source_key, true);
        assert_no_retirement(&observations);
        assert_eq!(
            before,
            format!("{:?}", db.zalsa().runtime().dependency_graph.lock())
        );
        assert!(!source.drop());
        // Cancelling the descendant removes its reclaimed mapping and releases its
        // waiter. The outer query can then complete and release the retained source.
        descendant.abort();
        outer.join().unwrap();
    });
    assert_empty(&db.zalsa().runtime().dependency_graph.lock());
}

#[test]
fn occupied_source_rotation_preserves_the_detached_wait_edge() {
    // Gates let the target first lend its transferred queries to this thread, then
    // lend the detached descendant to the waiter that the rotation must release.
    let db = Db::default();
    let [
        source_key,
        branch_key,
        descendant_key,
        retained_key,
        outer_key,
        initial_gate_key,
        waiter_gate_key,
    ] = keys(&db);
    let source = claim(&db, source_key, Reentrancy::Deny);
    let branch = claim(&db, branch_key, Reentrancy::Deny);
    let descendant = claim(&db, descendant_key, Reentrancy::Deny);
    let retained = claim(&db, retained_key, Reentrancy::Deny);
    let initial_gate = claim(&db, initial_gate_key, Reentrancy::Deny);
    let current = thread::current().id();
    let (target_tx, target_rx) = mpsc::channel();
    let (waiter_tx, waiter_rx) = mpsc::channel();
    let (retained_tx, retained_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (reclaim_tx, reclaim_rx) = mpsc::channel();
    thread::scope(|scope| {
        let outer_db = db.clone();
        let outer = scope.spawn(move || {
            let outer = claim(&outer_db, outer_key, Reentrancy::Deny);
            assert!(wait_on(&outer_db, initial_gate_key, &target_tx));
            assert!(wait_on(&outer_db, waiter_gate_key, &target_tx));
            assert!(!wait_on(&outer_db, descendant_key, &target_tx));
            assert!(!outer.drop());
        });
        let target_thread = registered(&db, &target_rx, initial_gate_key, current);
        transfer(descendant, branch_key);
        transfer(branch, source_key);
        transfer(retained, source_key);
        transfer(source, outer_key);
        let source = claim(&db, source_key, Reentrancy::Allow);
        let retained = claim(&db, retained_key, Reentrancy::Allow);
        let waiter_db = db.clone();
        let waiter = scope.spawn(move || {
            let gate = claim(&waiter_db, waiter_gate_key, Reentrancy::Deny);
            ready_tx.send(thread::current().id()).unwrap();
            let retained_thread = reclaim_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
            let descendant = claim(&waiter_db, descendant_key, Reentrancy::Allow);
            assert!(!gate.drop());
            assert!(wait_on(&waiter_db, source_key, &waiter_tx));
            {
                let graph = waiter_db.zalsa().runtime().dependency_graph.lock();
                assert_ownership(
                    &graph,
                    &[
                        (source_key, target_thread, branch_key),
                        (branch_key, target_thread, outer_key),
                        (descendant_key, current, branch_key),
                        (retained_key, current, source_key),
                    ],
                );
                assert_eq!(graph.edges.0.len(), 3);
                assert_eq!(
                    graph.edges.0[&target_thread].blocked_on_id,
                    thread::current().id()
                );
                assert_eq!(graph.edges.0[&retained_thread].blocked_on_id, target_thread);
                assert_eq!(graph.edges.0[&current].blocked_on_id, target_thread);
                assert!(!graph.edges.contains_key(&thread::current().id()));
                assert_eq!(
                    graph.query_dependents[&descendant_key].as_slice(),
                    [target_thread]
                );
                assert_eq!(graph.query_dependents[&branch_key].as_slice(), [current]);
                assert!(graph.wait_results.is_empty());
            }
            descendant.abort();
        });
        let waiter_thread = ready_rx.recv_timeout(STAGE_TIMEOUT).unwrap();
        let retained_db = db.clone();
        let retained_waiter =
            scope.spawn(move || assert!(wait_on(&retained_db, retained_key, &retained_tx)));
        let retained_thread = registered(&db, &retained_rx, retained_key, current);
        assert!(!retained.drop());
        assert!(!initial_gate.drop());
        assert_eq!(
            registered(&db, &target_rx, waiter_gate_key, waiter_thread),
            target_thread
        );
        reclaim_tx.send(retained_thread).unwrap();
        assert_eq!(
            registered(&db, &waiter_rx, source_key, current),
            waiter_thread
        );
        assert_eq!(
            registered(&db, &target_rx, descendant_key, waiter_thread),
            target_thread
        );
        let quote = db.zalsa().runtime().retirement_quote(1).unwrap();
        let (result, observations) = retire(source, branch_key, true, &quote);
        if !matches!(result, Ok(true)) {
            db.zalsa()
                .runtime()
                .unblock_queries_blocked_on(source_key, WaitResult::Cancelled);
        }
        assert!(matches!(result, Ok(true)));
        waiter.join().unwrap();
        outer.join().unwrap();
        retained_waiter.join().unwrap();
        assert_committed_and_waited(&observations, source_key);
        let remapped: Vec<_> = observations
            .records
            .iter()
            .filter(|record| record.event.kind == Kind::EdgeRemap)
            .map(|record| (record.event.key, record.event.from, record.event.peer))
            .collect();
        assert_eq!(
            remapped,
            [(
                Some(retained_key),
                Some(retained_thread),
                Some(target_thread)
            )]
        );
    });
    assert_empty(&db.zalsa().runtime().dependency_graph.lock());
}
