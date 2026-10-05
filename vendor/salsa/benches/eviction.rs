//! Compares eviction policies across cache-hit overhead and eviction cost.
//!
//! Large multi-revision workloads and concurrent hit-path contention are
//! measured by `eviction_walltime`, letting these bounded workloads use CPU and
//! memory instrumentation.
//!
//! Every benchmark includes `NoEviction` as the policy-free baseline for how
//! cheap the workload could be without eviction bookkeeping or capacity-driven
//! recomputation.
//!
//! Database-owning benchmarks use single-iteration samples to avoid batching
//! multiple databases, and 20 samples to keep their setup cost bounded.

use std::hint::black_box;

use salsa::Database as _;

#[path = "eviction/support.rs"]
mod support;

use support::{Policy, access_all, new_items, prewarm};

fn main() {
    divan::main();
}

/// Measures the overhead each eviction policy adds to repeated cache hits.
///
/// ```text
/// prewarm: A B C ... (1,024 entries)
/// timed:   A B C ... | A B C ... | ...
/// ```
///
/// Hot query results are often reused many times within one revision, so even
/// small per-hit costs can add up. Every result remains resident, which isolates
/// hit-path bookkeeping. `NoEviction` shows the tracked-query cost without an
/// eviction policy.
#[divan::bench(args = [Policy::NoEviction, Policy::Lru])]
fn fast_path(bencher: divan::Bencher, policy: Policy) {
    const ITEMS: usize = 1_024;

    let mut db = salsa::DatabaseImpl::new();
    policy.set_capacity(&mut db, ITEMS);
    let items = new_items(&db, ITEMS);
    prewarm(policy, &db, &items);

    bencher.bench_local(|| black_box(access_all(policy, &db, items.iter().copied())));
}

/// Measures cache-hit overhead after disabling eviction at runtime.
///
/// ```text
/// capacity: 0
/// prewarm:  A B C ... (1,024 entries)
/// timed:    A B C ... | A B C ... | ...
/// ```
///
/// Setting capacity to zero disables an eviction policy without changing the
/// tracked function. This measures the remaining disabled-policy branch on the
/// hit path. `NoEviction` is the policy-free reference for how cheap this path
/// could be.
#[divan::bench(args = [Policy::NoEviction, Policy::Lru])]
fn disabled_eviction(bencher: divan::Bencher, policy: Policy) {
    const ITEMS: usize = 1_024;

    let mut db = salsa::DatabaseImpl::new();
    policy.set_capacity(&mut db, 0);
    let items = new_items(&db, ITEMS);
    prewarm(policy, &db, &items);

    bencher.bench_local(|| black_box(access_all(policy, &db, items.iter().copied())));
}

/// Measures the cost of enforcing capacity when the working set already fits.
///
/// ```text
/// prewarm: A B C ... (1,024 entries)
/// timed:   A B C ... -> enforce capacity 1,024 -> no victim
/// ```
///
/// A capacity check should be cheap when no victim is needed. The benchmark
/// reads a fully resident working set and then triggers a sweep, separating
/// steady-state sweep bookkeeping from victim selection and destruction.
#[divan::bench(
    args = [Policy::NoEviction, Policy::Lru],
    sample_count = 20,
    sample_size = 1
)]
fn fast_path_and_sweep(bencher: divan::Bencher, policy: Policy) {
    const ITEMS: usize = 1_024;

    bencher
        .with_inputs(|| {
            let mut db = salsa::DatabaseImpl::new();
            policy.set_capacity(&mut db, ITEMS);
            let items = new_items(&db, ITEMS);
            prewarm(policy, &db, &items);
            (db, items)
        })
        .bench_local_refs(|(db, items)| {
            let result = access_all(policy, &*db, items.iter().copied());
            db.trigger_lru_eviction();
            black_box(result)
        });
}

/// Measures the cost of exceeding capacity and evicting the excess entries.
///
/// ```text
/// timed: empty -> A B C ... (320 entries) -> enforce capacity 256
///                                             `-> evict 64
/// ```
///
/// The cache starts empty, admits 320 entries into a capacity of 256, and then
/// enforces that capacity. This includes admission, eager or lazy victim
/// selection, and value destruction. `NoEviction` provides the computation
/// baseline; setup and final database destruction remain outside timing.
#[divan::bench(
    args = [Policy::NoEviction, Policy::Lru],
    sample_count = 20,
    sample_size = 1
)]
fn fill_and_evict(bencher: divan::Bencher, policy: Policy) {
    const CAPACITY: usize = 256;
    const ITEMS: usize = 320;

    bencher
        .with_inputs(|| {
            let mut db = salsa::DatabaseImpl::new();
            policy.set_capacity(&mut db, CAPACITY);
            let items = new_items(&db, ITEMS);
            (db, items)
        })
        .bench_local_refs(|(db, items)| {
            let result = access_all(policy, &*db, items.iter().copied());
            db.trigger_lru_eviction();
            black_box(result)
        });
}
