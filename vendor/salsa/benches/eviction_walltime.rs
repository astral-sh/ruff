//! Compares eviction policies in workloads that require wall-time measurement.
//!
//! The multi-revision workloads perform enough query computation and allocation
//! to make eviction decisions observable, producing instruction traces that are
//! prohibitively expensive to simulate. The concurrent workload requires real
//! scheduling and contention.
//!
//! Every benchmark includes `NoEviction` as the policy-free baseline for how
//! cheap the workload could be without eviction bookkeeping or capacity-driven
//! recomputation.
//!
//! Database-owning benchmarks use single-iteration samples to avoid batching
//! multiple databases, and 20 samples to keep their setup cost bounded.

use std::hint::black_box;

use rayon::prelude::*;
use salsa::Database as _;

#[path = "eviction/support.rs"]
mod support;

use support::{
    Item, Policy, Value, access_all, access_all_with, lru_value, new_items, no_eviction_value,
    prewarm,
};

fn main() {
    rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build_global()
        .unwrap();

    divan::main();
}

/// Measures a large initial working set followed by small incremental working sets.
///
/// ```text
/// capacity: 256
/// Pn = entry n; repeat(Pn, 64) = 64 consecutive accesses to that entry
///
/// initial revision (1,024 distinct entries):
/// repeat(P0, 64), repeat(P1, 64), ..., repeat(P1023, 64)
///
/// ---- revision boundary ----
///
/// incremental revisions 1-3 (the same 8 entries in every revision):
/// repeat(P0, 64),   repeat(P128, 64), repeat(P256, 64), repeat(P384, 64),
/// repeat(P512, 64), repeat(P640, 64), repeat(P768, 64), repeat(P896, 64)
/// ```
///
/// This resembles checking an entire project once, followed by incremental
/// checks after edits that affect only a few files.
///
/// The initial phase accesses 1,024 entries, far exceeding the cache capacity,
/// and reuses each entry many times before moving to the next one. Later
/// revisions repeatedly access only eight entries. Spreading those entries
/// across the initial access order avoids biasing the result toward the recently
/// accessed tail.
///
/// The initial phase measures admission and eviction while repeated calls hit
/// the current entry. The incremental phases then measure the cost of loading
/// active entries that did not survive and how quickly the policy stabilizes
/// around the smaller working set.
#[divan::bench(
    args = [Policy::NoEviction, Policy::Lru],
    sample_count = 20,
    sample_size = 1
)]
fn project_check_then_incremental(bencher: divan::Bencher, policy: Policy) {
    const CAPACITY: usize = 256;
    const PROJECT_FILES: usize = 1_024;
    const ACTIVE_FILES: usize = 8;
    const CALLS_PER_FILE: usize = 64;
    const INCREMENTAL_REVISIONS: usize = 3;

    bencher
        .with_inputs(|| {
            let mut db = salsa::DatabaseImpl::new();
            policy.set_capacity(&mut db, CAPACITY);
            let files = new_items(&db, PROJECT_FILES);
            let active_files = files
                .iter()
                .step_by(PROJECT_FILES / ACTIVE_FILES)
                .copied()
                .collect::<Vec<_>>();
            (db, files, active_files)
        })
        .bench_local_refs(|(db, files, active_files)| {
            let mut sum = access_each_repeatedly(policy, &*db, files, CALLS_PER_FILE).0;

            for _ in 0..INCREMENTAL_REVISIONS {
                db.synthetic_write(salsa::Durability::LOW);
                sum = sum.wrapping_add(
                    access_each_repeatedly(policy, &*db, active_files, CALLS_PER_FILE).0,
                );
            }

            black_box(Value(sum))
        });
}

/// Measures resistance to cache pollution from an interleaved one-shot scan.
///
/// ```text
/// H = next recurring entry; Sr = next scan entry from round r
///
/// warmup:     [H H H S0 S0] x 64
///                         |
///                      revision
///                         v
/// rounds 1..4: [H H H Sr Sr] x 64  (revision after each round)
///                         |
///                         v
/// probe:       [H] x 192
/// ```
///
/// Each mixed round accesses the same 192 recurring entries exactly once and
/// interleaves 128 entries that are never used again. The recurring set leaves
/// 64 free slots in a cache of 256, so each 128-entry scan must displace cached
/// entries. Every round uses the same access order and provides no same-revision
/// frequency signal. After an untimed mixed round establishes cache state, four
/// measured rounds test which recurring entries survive the scans. A final
/// recurring-only round charges for entries displaced by the last scan.
/// `NoEviction` shows the cost when recurring values are never displaced.
#[divan::bench(
    args = [Policy::NoEviction, Policy::Lru],
    sample_count = 20,
    sample_size = 1
)]
fn scan_resistance(bencher: divan::Bencher, policy: Policy) {
    const CAPACITY: usize = 256;
    const HOT_ITEMS: usize = 192;
    const ITEMS_PER_ROUND: usize = 128;
    const ROUNDS: usize = 5;

    bencher
        .with_inputs(|| {
            let mut db = salsa::DatabaseImpl::new();
            policy.set_capacity(&mut db, CAPACITY);
            let items = new_items(&db, HOT_ITEMS + ITEMS_PER_ROUND * ROUNDS);
            let hot = &items[..HOT_ITEMS];

            let mut rounds = Vec::with_capacity(ROUNDS);

            for round in 0..ROUNDS {
                let cold_start = HOT_ITEMS + round * ITEMS_PER_ROUND;
                let cold_end = cold_start + ITEMS_PER_ROUND;
                let cold = &items[cold_start..cold_end];
                let mut accesses = Vec::with_capacity(HOT_ITEMS + ITEMS_PER_ROUND);

                // Mix three reusable entries with every two one-shot entries.
                for (hot_chunk, cold_chunk) in hot.chunks_exact(3).zip(cold.chunks_exact(2)) {
                    accesses.extend_from_slice(hot_chunk);
                    accesses.extend_from_slice(cold_chunk);
                }
                rounds.push(accesses);
            }

            prewarm(policy, &db, &rounds.remove(0));

            // Pay for recurring entries displaced by the final mixed round.
            rounds.push(hot.to_vec());

            Workload { db, rounds }
        })
        .bench_local_refs(|workload| black_box(workload.run(policy)));
}

/// Measures whether a policy retains popular queries over one-hit wonders.
///
/// ```text
/// round 0 (untimed): H0 C0 H0 | H1 C1 H1 | ... | H63 C63 H63 | C64 ... C191
///                                      |
///                                   revision
///                                      v
/// round 1:           H0 C192 H0 | H1 C193 H1 | ... | H63 C255 H63 | C256 ... C383
///                                      |
///                                   revision
///                                      v
/// round 2:           H0 C384 H0 | H1 C385 H1 | ... | H63 C447 H63 | C448 ... C575
///                                      |
///                                     ...
///                                      |
///                                   revision
///                                      v
/// final probe:       H0 H1 ... H63
/// ```
///
/// Each round accesses 64 popular entries twice, with a one-hit wonder between
/// each pair, then admits 128 more entries that are never used again. That final
/// cold tail fills the capacity, so LRU sees every one-hit wonder as more recent
/// than the popular entries and evicts the values needed by the next round.
/// A frequency-aware policy can use the second popular access to distinguish
/// reusable entries from one-hit wonders.
///
/// Round 0 establishes policy state outside the timed region. Four measured
/// rounds charge for previous eviction decisions, and the final probe charges
/// for popular entries displaced by the last one-hit-wonder batch. `NoEviction`
/// provides the computation baseline; retaining cold entries cannot create hits
/// because they are never accessed again.
#[divan::bench(
    args = [Policy::NoEviction, Policy::Lru],
    sample_count = 20,
    sample_size = 1
)]
fn one_hit_wonders(bencher: divan::Bencher, policy: Policy) {
    const CAPACITY: usize = 128;
    const HOT_ITEMS: usize = 64;
    const COLD_ITEMS_PER_ROUND: usize = 192;
    const ROUNDS: usize = 5;

    bencher
        .with_inputs(|| {
            let mut db = salsa::DatabaseImpl::new();
            policy.set_capacity(&mut db, CAPACITY);
            let items = new_items(&db, HOT_ITEMS + COLD_ITEMS_PER_ROUND * ROUNDS);
            let hot = &items[..HOT_ITEMS];
            let mut rounds = Vec::with_capacity(ROUNDS);

            for round in 0..ROUNDS {
                let cold_start = HOT_ITEMS + round * COLD_ITEMS_PER_ROUND;
                let cold_end = cold_start + COLD_ITEMS_PER_ROUND;
                let cold = &items[cold_start..cold_end];
                let mut accesses = Vec::with_capacity(HOT_ITEMS * 2 + COLD_ITEMS_PER_ROUND);

                for (hot_item, cold_item) in hot.iter().zip(cold) {
                    accesses.push(*hot_item);
                    accesses.push(*cold_item);
                    accesses.push(*hot_item);
                }
                accesses.extend_from_slice(&cold[HOT_ITEMS..]);
                rounds.push(accesses);
            }

            prewarm(policy, &db, &rounds.remove(0));

            rounds.push(hot.to_vec());

            Workload { db, rounds }
        })
        .bench_local_refs(|workload| black_box(workload.run(policy)));
}

/// Measures how quickly a policy abandons a working set that is no longer used.
///
/// ```text
/// [A x 192] --revision--> [B x 192] --revision--> [B x 192]
///                                      --revision--> [B x 192]
/// ```
///
/// The benchmark switches from 192 entries to a disjoint set of
/// 192 entries. Either set fits by itself, so retaining the old phase only
/// delays admission of useful results from the new phase. `NoEviction` provides
/// the policy-free baseline: it retains both phases, but only the second phase
/// is accessed after the switch.
#[divan::bench(
    args = [Policy::NoEviction, Policy::Lru],
    sample_count = 20,
    sample_size = 1
)]
fn phase_change(bencher: divan::Bencher, policy: Policy) {
    const CAPACITY: usize = 256;
    const ITEMS_PER_PHASE: usize = 192;
    const ROUNDS: usize = 3;

    bencher
        .with_inputs(|| {
            let mut db = salsa::DatabaseImpl::new();
            policy.set_capacity(&mut db, CAPACITY);
            let items = new_items(&db, ITEMS_PER_PHASE * 2);
            let first_phase = &items[..ITEMS_PER_PHASE];
            let second_phase = &items[ITEMS_PER_PHASE..];
            prewarm(policy, &db, first_phase);

            Workload {
                db,
                rounds: vec![second_phase.to_vec(); ROUNDS],
            }
        })
        .bench_local_refs(|workload| black_box(workload.run(policy)));
}

/// Measures synchronization overhead on concurrent cache hits.
///
/// ```text
/// worker 0: A A A ... (4,096 accesses)
/// worker 1: B B B ...
/// worker 2: C C C ...
/// worker 3: D D D ...
/// ```
///
/// Each worker repeatedly accesses a different prewarmed query, avoiding both
/// misses and contention on the query itself. This isolates contention in the
/// eviction policy's hit bookkeeping; `NoEviction` provides the baseline.
#[divan::bench(
    args = [Policy::NoEviction, Policy::Lru],
    sample_count = 20,
    sample_size = 1
)]
fn parallel_fast_path(bencher: divan::Bencher, policy: Policy) {
    const ACCESSES_PER_WORKER: usize = 4_096;

    bencher
        .with_inputs(|| {
            let worker_count = rayon::current_num_threads();
            let mut db = salsa::DatabaseImpl::new();
            policy.set_capacity(&mut db, worker_count);
            let items = new_items(&db, worker_count);
            prewarm(policy, &db, &items);

            items
                .into_iter()
                .map(|item| (db.clone(), item))
                .collect::<Vec<_>>()
        })
        .bench_local_refs(|jobs| {
            black_box(parallel_access_repeated(policy, jobs, ACCESSES_PER_WORKER))
        });
}

fn access_each_repeatedly(
    policy: Policy,
    db: &dyn salsa::Database,
    items: &[Item],
    count: usize,
) -> Value {
    access_all(
        policy,
        db,
        items
            .iter()
            .copied()
            .flat_map(|item| std::iter::repeat_n(item, count)),
    )
}

struct Workload {
    db: salsa::DatabaseImpl,
    /// Each round starts a new revision, applying evictions selected by the
    /// preceding round before performing the next batch of accesses.
    rounds: Vec<Vec<Item>>,
}

impl Workload {
    #[inline]
    fn run(&mut self, policy: Policy) -> Value {
        match policy {
            Policy::NoEviction => self.run_with(no_eviction_value),
            Policy::Lru => self.run_with(lru_value),
        }
    }

    #[inline]
    fn run_with(&mut self, fetch: impl Fn(&dyn salsa::Database, Item) -> Value + Copy) -> Value {
        let mut sum = 0u64;
        for round in &self.rounds {
            self.db.synthetic_write(salsa::Durability::LOW);
            sum = sum.wrapping_add(access_all_with(&self.db, round.iter().copied(), fetch).0);
        }
        Value(sum)
    }
}

fn parallel_access_repeated(
    policy: Policy,
    jobs: &mut [(salsa::DatabaseImpl, Item)],
    accesses_per_item: usize,
) -> Value {
    match policy {
        Policy::NoEviction => {
            parallel_access_repeated_with(jobs, accesses_per_item, no_eviction_value)
        }
        Policy::Lru => parallel_access_repeated_with(jobs, accesses_per_item, lru_value),
    }
}

fn parallel_access_repeated_with(
    jobs: &mut [(salsa::DatabaseImpl, Item)],
    accesses_per_item: usize,
    fetch: impl Fn(&dyn salsa::Database, Item) -> Value + Copy + Send + Sync,
) -> Value {
    let sum = jobs
        .par_iter_mut()
        .map(|(db, item)| {
            access_all_with(db, std::iter::repeat_n(*item, accesses_per_item), fetch).0
        })
        .reduce(|| 0, u64::wrapping_add);
    Value(sum)
}
