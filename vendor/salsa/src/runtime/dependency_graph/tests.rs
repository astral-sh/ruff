//! Structural controls inspect this graph directly. Real claim/transfer lifetimes are
//! exercised separately by the existing transfer fixture's `native_release` module.

mod checked_retirement;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::thread;

use super::{DependencyGraph, TransferredQueries, edge::EdgeCondvar};
use crate::attempt_probe::transfer_test_support::{self as trace, Kind, TraceConfig};
use crate::function::SyncOwner;
use crate::runtime::{TransferFailure, WaitResult};
use crate::zalsa::IngredientIndex;
use crate::{DatabaseKeyIndex, Id};

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct ReleaseCounts {
    pub(super) queries: usize,
    pub(super) children: usize,
    pub(super) waiters: usize,
}

fn key(index: u32) -> DatabaseKeyIndex {
    // SAFETY: These structural fixtures use only small, explicitly bounded indices.
    DatabaseKeyIndex::new(IngredientIndex::new(0), unsafe { Id::from_index(index) })
}

fn transfer(graph: &mut DependencyGraph, child: u32, parent: u32) {
    assert!(
        graph
            .transferred
            .insert(key(child), (thread::current().id(), key(parent)))
            .is_none()
    );
    graph
        .transferred_dependents
        .entry(key(parent))
        .or_default()
        .push(key(child));
}

fn assert_empty(graph: &DependencyGraph) {
    assert!(graph.transferred.is_empty());
    assert!(graph.transferred_dependents.is_empty());
    assert!(graph.edges.0.is_empty());
    assert!(graph.query_dependents.is_empty());
    assert!(graph.wait_results.is_empty());
}

fn inspect_tree(graph: &DependencyGraph, count: usize) {
    let memberships: usize = graph
        .transferred_dependents
        .values()
        .map(|set| set.0.len())
        .sum();
    assert_eq!(memberships, count - 1);
    assert_eq!(graph.transferred.len(), memberships);
    for (&parent, children) in &graph.transferred_dependents {
        for &child in children {
            assert_eq!(graph.transferred[&child].1, parent);
        }
    }
    let mut walk = TransferredQueries::new(&graph.transferred_dependents, key(0));
    let mut visited = 0;
    let mut frames = 0;
    let mut frame_capacity = 0;
    while walk.next().is_some() {
        visited += 1;
        frames = frames.max(walk.ancestors.len());
        frame_capacity = frame_capacity.max(walk.ancestors.capacity());
    }
    assert_eq!(visited, count);
    assert!(frames <= count);
    let reverse_capacity: usize = graph
        .transferred_dependents
        .values()
        .map(|set| set.0.capacity())
        .sum();
    eprintln!(
        "NATIVE_RELEASE structural nodes={count} forward={}/{} reverse={}/{} memberships={memberships}/{reverse_capacity} iterator-frames={frames}/{frame_capacity}",
        graph.transferred.len(),
        graph.transferred.capacity(),
        graph.transferred_dependents.len(),
        graph.transferred_dependents.capacity(),
    );
    eprintln!(
        "NATIVE_RELEASE structural wait-edges={}/{} query-dependents={}/{} pending-results={}/{}",
        graph.edges.0.len(),
        graph.edges.0.capacity(),
        graph.query_dependents.len(),
        graph.query_dependents.capacity(),
        graph.wait_results.len(),
        graph.wait_results.capacity(),
    );
}

#[test]
fn release_shapes_and_sparse_capacity() {
    let mut graph = DependencyGraph::default();
    let parents: [fn(u32) -> u32; 3] = [|child| child - 1, |_| 0, |child| (child - 1) / 2];
    for count in [1, 8, 256, 2_048] {
        for parent in parents {
            for child in 1..count {
                transfer(&mut graph, child, parent(child));
            }
            inspect_tree(&graph, count as usize);
            graph.release_counts = ReleaseCounts::default();
            graph.unblock_runtimes_blocked_on_transferred_queries_owned_by(
                key(0),
                WaitResult::Completed,
            );
            assert_eq!(
                graph.release_counts,
                ReleaseCounts {
                    queries: count as usize,
                    children: count as usize - 1,
                    waiters: 0,
                }
            );
            assert_empty(&graph);
            assert_eq!(
                graph
                    .transferred_dependents
                    .values()
                    .map(|set| set.0.capacity())
                    .sum::<usize>(),
                0
            );
        }
    }

    // A small live tree in large retained tables must not repeatedly scan empty buckets.
    graph.transferred.reserve(32_768);
    graph.transferred_dependents.reserve(32_768);
    for _ in 0..16 {
        transfer(&mut graph, 1, 0);
        transfer(&mut graph, 2, 1);
        inspect_tree(&graph, 3);
        graph.release_counts = ReleaseCounts::default();
        graph.unblock_runtimes_blocked_on_transferred_queries_owned_by(
            key(0),
            WaitResult::Cancelled,
        );
        assert_eq!(graph.release_counts.queries, 3);
        assert_eq!(graph.release_counts.children, 2);
        assert_empty(&graph);
        assert!(graph.transferred.capacity() >= 16_384);
        assert!(graph.transferred_dependents.capacity() >= 16_384);
    }

    // Undo leaves this live reverse set with only three children but its wide allocation.
    // Release work must follow the remaining memberships, not the retained capacity.
    for child in 1..=2_048 {
        transfer(&mut graph, child, 0);
    }
    for child in 4..=2_048 {
        graph.undo_transfer_lock(key(child));
        graph.unblock_runtimes_blocked_on_transferred_queries_owned_by(
            key(child),
            WaitResult::Completed,
        );
    }
    let surviving = &graph.transferred_dependents[&key(0)].0;
    assert_eq!(surviving.len(), 3);
    assert!(surviving.capacity() >= 2_048);
    inspect_tree(&graph, 4);
    graph.release_counts = ReleaseCounts::default();
    graph.unblock_runtimes_blocked_on_transferred_queries_owned_by(key(0), WaitResult::Cancelled);
    assert_eq!(
        graph.release_counts,
        ReleaseCounts {
            queries: 4,
            children: 3,
            waiters: 0,
        }
    );
    assert_empty(&graph);

    // Releasing an inner root detaches it from its former parent, leaving siblings owned.
    transfer(&mut graph, 1, 0);
    transfer(&mut graph, 2, 0);
    transfer(&mut graph, 3, 1);
    graph.unblock_runtimes_blocked_on_transferred_queries_owned_by(key(1), WaitResult::Panicked);
    assert_eq!(graph.transferred.len(), 1);
    assert_eq!(graph.transferred[&key(2)].1, key(0));
    assert_eq!(
        graph.transferred_dependents[&key(0)]
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [key(2)]
    );
    graph.unblock_runtimes_blocked_on_transferred_queries_owned_by(key(0), WaitResult::Completed);
    assert_empty(&graph);

    // An earlier reentrant-claim undo must not prevent release of that query's children.
    transfer(&mut graph, 1, 0);
    transfer(&mut graph, 2, 1);
    graph.undo_transfer_lock(key(1));
    graph.unblock_runtimes_blocked_on_transferred_queries_owned_by(key(1), WaitResult::Cancelled);
    assert!(graph.transferred.is_empty());
    assert!(
        graph.transferred_dependents[&key(0)]
            .iter()
            .next()
            .is_none()
    );
    graph.unblock_runtimes_blocked_on_transferred_queries_owned_by(key(0), WaitResult::Completed);
    assert_empty(&graph);
}

#[test]
fn transfer_walk_order() {
    let threads: Vec<_> = (0..4)
        .map(|_| thread::spawn(|| thread::current().id()).join().unwrap())
        .collect();
    let condvars: Vec<_> = (0..3).map(|_| Box::pin(EdgeCondvar::default())).collect();
    // Declared after the condvars so even an assertion unwind drops every edge first.
    let mut graph = DependencyGraph::default();
    transfer(&mut graph, 1, 0);
    transfer(&mut graph, 2, 0);
    transfer(&mut graph, 3, 1);
    let preorder = [key(0), key(1), key(3), key(2)];
    assert_eq!(
        TransferredQueries::new(&graph.transferred_dependents, key(0)).collect::<Vec<_>>(),
        preorder
    );
    assert_eq!(
        TransferredQueries::new(&graph.transferred_dependents, key(0))
            .take(2)
            .collect::<Vec<_>>(),
        &preorder[..2]
    );

    // DFS reaches threads[0] first, but cutting it would leave the target itself as a
    // subtree waiter. Remapping that remaining waiter to the target creates a self-edge.
    unsafe {
        // SAFETY: The pinned condvars outlive graph, including assertion unwinding.
        graph.add_edge(threads[0], key(1), threads[3], condvars[0].as_ref());
        graph.add_edge(threads[1], key(2), threads[0], condvars[1].as_ref());
    }
    assert!(graph.query_dependents[&key(1)].contains(&threads[0]));
    assert!(graph.query_dependents[&key(2)].contains(&threads[1]));
    let mut old_remap = [(threads[0], threads[3]), (threads[1], threads[0])]
        .into_iter()
        .collect::<super::FxHashMap<_, _>>();
    old_remap.remove(&threads[0]);
    old_remap.insert(threads[1], threads[1]);
    assert_eq!(old_remap[&threads[1]], threads[1]);

    graph.unblock_transfer_target(key(0), threads[1]);
    assert!(matches!(
        graph.wait_results.remove(&threads[1]),
        Some(WaitResult::Completed)
    ));
    assert!(!graph.edges.contains_key(&threads[1]));
    assert!(graph.edges.contains_key(&threads[0]));
    graph.update_transferred_edges(key(0), threads[1]);
    assert_eq!(graph.edges.0[&threads[0]].blocked_on_id, threads[1]);
    assert_acyclic(&graph);
    graph.unblock_runtimes_blocked_on(key(1), WaitResult::Completed);
    assert!(matches!(
        graph.wait_results.remove(&threads[0]),
        Some(WaitResult::Completed)
    ));

    for (index, query) in [key(1), key(3), key(2)].into_iter().enumerate() {
        // SAFETY: The pinned condvars outlive graph, including assertion unwinding.
        unsafe {
            graph.add_edge(
                threads[index],
                query,
                thread::current().id(),
                condvars[index].as_ref(),
            );
        }
    }
    let (_, observations) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: Arc::new(AtomicUsize::new(0)),
        },
        || graph.update_transferred_edges(key(0), threads[3]),
    );
    assert!(!observations.broken);
    let remapped: Vec<_> = observations
        .records
        .iter()
        .filter(|record| record.event.kind == Kind::EdgeRemap)
        .map(|record| {
            assert_eq!(record.event.peer, Some(threads[3]));
            record.event.key.unwrap()
        })
        .collect();
    assert_eq!(remapped, &preorder[1..]);
    assert!(
        graph
            .edges
            .0
            .values()
            .all(|edge| edge.blocked_on_id == threads[3])
    );

    graph.release_counts = ReleaseCounts::default();
    let (_, observations) = trace::collect(
        TraceConfig {
            worker: 0,
            ordinal: Arc::new(AtomicUsize::new(0)),
        },
        || {
            graph.unblock_runtimes_blocked_on_transferred_queries_owned_by(
                key(0),
                WaitResult::Cancelled,
            )
        },
    );
    assert!(!observations.broken);
    let removed: Vec<_> = observations
        .records
        .iter()
        .filter(|record| record.event.kind == Kind::MappingRemoved)
        .map(|record| record.event.key.unwrap())
        .collect();
    // Destructive release pops siblings from the tail and removes a parent on ascent.
    assert_eq!(removed, [key(2), key(3), key(1)]);
    assert_eq!(
        graph.release_counts,
        ReleaseCounts {
            queries: 4,
            children: 3,
            waiters: 3
        }
    );
    for waiter in &threads[..3] {
        assert!(matches!(
            graph.wait_results.remove(waiter),
            Some(WaitResult::Cancelled)
        ));
    }
    assert_empty(&graph);
}

fn assert_acyclic(graph: &DependencyGraph) {
    for &start in graph.edges.0.keys() {
        let mut seen = crate::hash::FxHashSet::default();
        let mut at = start;
        while let Some(edge) = graph.edges.0.get(&at) {
            assert!(seen.insert(at), "thread remapping introduced a cycle");
            at = edge.blocked_on_id;
        }
    }
}

#[test]
fn checked_target_rejects_an_unmapped_source_descendant() {
    let mut graph = DependencyGraph::default();
    transfer(&mut graph, 1, 0);
    let quote = graph.retirement_quote(1).unwrap();
    let result = graph.check_transfer_target(
        key(0),
        key(1),
        SyncOwner::Transferred,
        thread::current().id(),
        &quote,
    );
    assert!(matches!(result, Err(TransferFailure::Unavailable)));
    assert_eq!(graph.transferred.len(), 1);
    assert_eq!(graph.transferred[&key(1)].1, key(0));
    assert_eq!(
        graph.transferred_dependents[&key(0)]
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [key(1)]
    );

    // With Q -> outer, the old outer owner survives the occupied-source rotation.
    transfer(&mut graph, 0, 2);
    let quote = graph.retirement_quote(1).unwrap();
    assert!(
        graph
            .check_transfer_target(
                key(0),
                key(1),
                SyncOwner::Transferred,
                thread::current().id(),
                &quote
            )
            .is_ok()
    );
    transfer(&mut graph, 3, 2);
    assert!(matches!(
        graph.check_transfer_target(
            key(0),
            key(1),
            SyncOwner::Transferred,
            thread::current().id(),
            &quote
        ),
        Err(TransferFailure::Requote)
    ));
}

#[test]
fn nearest_waiter_excludes_the_entire_rotated_branch() {
    let threads: Vec<_> = (0..4)
        .map(|_| thread::spawn(|| thread::current().id()).join().unwrap())
        .collect();
    let condvars: Vec<_> = (0..3).map(|_| Box::pin(EdgeCondvar::default())).collect();
    let mut graph = DependencyGraph::default();
    transfer(&mut graph, 1, 0);
    transfer(&mut graph, 2, 1);
    transfer(&mut graph, 3, 0);
    unsafe {
        // SAFETY: All pinned condvars outlive their edges, including assertion unwinding.
        graph.add_edge(threads[0], key(2), threads[1], condvars[0].as_ref());
        graph.add_edge(threads[1], key(3), threads[2], condvars[1].as_ref());
        graph.add_edge(threads[2], key(0), threads[3], condvars[2].as_ref());
    }
    assert_eq!(
        graph.transfer_waiter(key(0), threads[0], None),
        Some((key(2), 0))
    );
    assert_eq!(
        graph.transfer_waiter(key(0), threads[0], Some(key(1))),
        Some((key(3), 0))
    );
    assert_eq!(
        TransferredQueries::excluding(&graph.transferred_dependents, key(0), Some(key(1)))
            .collect::<Vec<_>>(),
        [key(0), key(3)]
    );
    graph.unblock_transfer_waiter((key(3), 0));
    assert!(matches!(
        graph.wait_results.remove(&threads[1]),
        Some(WaitResult::Completed)
    ));
    // After rotation the excluded branch retains its old owner. Only root's remaining
    // waiter is remapped, and the target's path stops at the waiter just released.
    graph
        .transferred_dependents
        .get_mut(&key(0))
        .unwrap()
        .remove(&key(1));
    graph.update_transferred_edges(key(0), threads[0]);
    assert_eq!(graph.edges.0[&threads[2]].blocked_on_id, threads[0]);
    assert_acyclic(&graph);
}

#[test]
fn nearest_waiter_is_independent_of_subtree_order() {
    let threads: Vec<_> = (0..4)
        .map(|_| thread::spawn(|| thread::current().id()).join().unwrap())
        .collect();
    for reverse in [false, true] {
        let condvars: Vec<_> = (0..4).map(|_| Box::pin(EdgeCondvar::default())).collect();
        let mut graph = DependencyGraph::default();
        for child in if reverse { [2, 1] } else { [1, 2] } {
            transfer(&mut graph, child, 0);
        }
        // The target reaches U then V then the current thread. Both U and V wait on
        // this source subtree. Cutting V would leave U pointing back to the target.
        unsafe {
            // SAFETY: The pinned condvars outlive every stored edge.
            graph.add_edge(threads[0], key(99), threads[1], condvars[0].as_ref());
            graph.add_edge(threads[1], key(2), threads[2], condvars[1].as_ref());
            graph.add_edge(threads[2], key(1), threads[3], condvars[2].as_ref());
        }
        assert_eq!(
            graph.transfer_waiter(key(0), threads[0], None),
            Some((key(2), 0))
        );
        graph.unblock_transfer_target(key(0), threads[0]);
        assert!(matches!(
            graph.wait_results.remove(&threads[1]),
            Some(WaitResult::Completed)
        ));
        assert!(!graph.wait_results.contains_key(&threads[2]));
        graph.update_transferred_edges(key(0), threads[0]);
        unsafe {
            // SAFETY: The pinned condvars outlive every stored edge.
            graph.add_edge(threads[3], key(99), threads[0], condvars[3].as_ref());
        }
        assert_eq!(graph.edges.0[&threads[0]].blocked_on_id, threads[1]);
        assert_eq!(graph.edges.0[&threads[2]].blocked_on_id, threads[0]);
        assert_eq!(graph.edges.0[&threads[3]].blocked_on_id, threads[0]);
        assert_acyclic(&graph);
    }
}

/// Explicit graph traversal/membership visits and cursor creation/growth requests.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct WorkCounts([usize; 7]);
impl WorkCounts {
    fn total(self) -> usize {
        self.0.into_iter().sum()
    }
}
#[derive(Clone, Copy)]
pub(super) enum WorkKind {
    Chain,
    Tree,
    Waiter,
    Dependency,
    Membership,
    Cursor,
    CursorGrowth,
}
std::thread_local! { static WORK_COUNTS: std::cell::Cell<Option<WorkCounts>> = const { std::cell::Cell::new(None) }; }
pub(super) fn count_work(kind: WorkKind, units: usize) {
    WORK_COUNTS.with(|slot| {
        if let Some(mut counts) = slot.get() {
            counts.0[kind as usize] += units;
            slot.set(Some(counts));
        }
    });
}
fn measure_work<T>(f: impl FnOnce() -> T) -> (T, WorkCounts) {
    struct Restore(Option<WorkCounts>);
    impl Drop for Restore {
        fn drop(&mut self) {
            WORK_COUNTS.set(self.0);
        }
    }
    let _restore = Restore(WORK_COUNTS.replace(Some(WorkCounts::default())));
    let value = f();
    (value, WORK_COUNTS.get().unwrap())
}

#[test]
fn retirement_quote_bounds_live_chain_and_wide_tree_walks() {
    let threads: Vec<_> = (0..5)
        .map(|_| thread::spawn(|| thread::current().id()).join().unwrap())
        .collect();
    let condvars: Vec<_> = (0..4).map(|_| Box::pin(EdgeCondvar::default())).collect();
    for wide in [false, true] {
        let mut graph = DependencyGraph::default();
        for child in 1..=512 {
            transfer(&mut graph, child, if wide { 0 } else { child - 1 });
        }
        unsafe {
            // SAFETY: All pinned condvars outlive the graph.
            for index in 0..4 {
                graph.add_edge(
                    threads[index],
                    key((index + 1) as u32),
                    threads[index + 1],
                    condvars[index].as_ref(),
                );
            }
        }
        let quote = graph.retirement_quote(8).unwrap();
        let check = |graph: &DependencyGraph| {
            measure_work(|| {
                for _ in 0..8 {
                    graph
                        .check_transfer_target(
                            key(600),
                            key(512),
                            SyncOwner::Transferred,
                            thread::current().id(),
                            &quote,
                        )
                        .unwrap_or_else(|_| panic!("tree ends at the locally held root"));
                    assert!(graph.transfer_waiter(key(0), threads[0], None).is_some());
                }
            })
            .1
        };
        let dense = check(&graph);
        assert!(
            dense.total() <= quote.units,
            "{dense:?} exceeds {}",
            quote.units
        );
        graph.transferred.reserve(32_768);
        graph.transferred_dependents.reserve(32_768);
        graph.query_dependents.reserve(32_768);
        graph.edges.0.reserve(32_768);
        let sparse = check(&graph);
        assert_eq!(dense.0, sparse.0);
        assert!(sparse.total() <= quote.units);
        eprintln!(
            "RETIREMENT_WORK wide={wide} live={} edges={} counted={dense:?} quote={}",
            graph.transferred.len(),
            graph.edges.0.len(),
            quote.units
        );
    }
}

#[test]
fn retirement_scratch_quote_covers_minimum_and_cumulative_cursors() {
    let mut graph = DependencyGraph::default();
    transfer(&mut graph, 1, 0);
    let quote = graph.retirement_quote(1).unwrap();
    let frame = size_of::<std::slice::Iter<'_, DatabaseKeyIndex>>();
    assert!(quote.scratch_bytes >= 3 * 4 * frame);
    assert!(graph.retirement_quote(usize::MAX).is_none());
    let (_, work) = measure_work(|| {
        assert_eq!(
            TransferredQueries::new(&graph.transferred_dependents, key(0)).collect::<Vec<_>>(),
            [key(0), key(1)]
        );
    });
    assert_eq!(work.0[WorkKind::Cursor as usize], 1);
    assert_eq!(work.0[WorkKind::CursorGrowth as usize], 1);
}

#[test]
fn empty_graph_quotes_no_cursor_bytes_and_keeps_fixed_units() {
    let mut graph = DependencyGraph::default();
    for retained_capacity in [false, true] {
        if retained_capacity {
            graph.transferred.reserve(32);
            graph.edges.0.reserve(32);
            graph
                .transferred_dependents
                .entry(key(0))
                .or_default()
                .0
                .reserve(32);
        }
        for candidates in [0, 1, 8] {
            let quote = graph.retirement_quote(candidates).unwrap();
            assert_eq!(quote.transfers, 0);
            assert_eq!(quote.edges, 0);
            assert_eq!(quote.units, 88 * (candidates + 1));
            assert_eq!(quote.scratch_bytes, 0);
            let (result, work) = measure_work(|| {
                graph.check_transfer_target(
                    key(1),
                    key(0),
                    SyncOwner::Thread(thread::current().id()),
                    thread::current().id(),
                    &quote,
                )
            });
            assert!(result.is_ok());
            assert_eq!(work.total(), 0);
        }
        assert!(graph.retirement_quote(usize::MAX).is_none());
        assert!(graph.retirement_quote(usize::MAX / 88).is_none());
    }
}

#[test]
fn nonempty_graph_quotes_keep_the_general_units_and_cursor_bytes() {
    let threads: Vec<_> = (0..3)
        .map(|_| thread::spawn(|| thread::current().id()).join().unwrap())
        .collect();
    let condvars: Vec<_> = (0..2).map(|_| Box::pin(EdgeCondvar::default())).collect();
    for (transfers, edges) in [(1, 0), (0, 1), (1, 1), (3, 2)] {
        let mut graph = DependencyGraph::default();
        for child in 1..=transfers {
            transfer(&mut graph, child as u32, 0);
        }
        for index in 0..edges {
            // SAFETY: Each pinned condvar outlives the graph storing its edge.
            unsafe {
                graph.add_edge(
                    threads[index],
                    key(99),
                    threads[index + 1],
                    condvars[index].as_ref(),
                );
            }
        }
        for candidates in [0, 1, 8] {
            let quote = graph.retirement_quote(candidates).unwrap();
            assert_eq!(quote.transfers, transfers);
            assert_eq!(quote.edges, edges);
            assert_eq!(
                quote.units,
                (candidates + 1) * (16 * (transfers + 1) + 8 * (edges + 1).pow(2) + 64)
            );
            assert_eq!(
                quote.scratch_bytes,
                (candidates + 2)
                    * (4 * (transfers + 1) + 8)
                    * size_of::<std::slice::Iter<'_, DatabaseKeyIndex>>()
            );
        }
        assert!(graph.retirement_quote(usize::MAX).is_none());
    }
}
