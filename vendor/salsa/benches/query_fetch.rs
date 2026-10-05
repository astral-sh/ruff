use std::hint::black_box;

use salsa::{Database, Durability, Setter};

// Use many independent root query keys so each benchmark measures a batch of the same path
// instead of a single tiny query fetch dominated by loop and harness noise.
const ROOTS: usize = 256;

fn main() {
    divan::main();
}

/// Fetches cached query results in the same revision (fetch_hot)
#[divan::bench]
fn cached_same_revision(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let inputs = inputs(&db, Durability::LOW);
            let expected = compute_roots(&db, &inputs);

            (db, inputs, expected)
        })
        .bench_local_refs(|(db, inputs, expected)| {
            let sum = compute_roots(db, inputs);
            assert_eq!(black_box(sum), *expected);
        });
}

/// Executes a cold query without a cached value.
#[divan::bench]
fn cold_execute(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let inputs = inputs(&db, Durability::LOW);
            let expected = expected_sum();

            (db, inputs, expected)
        })
        .bench_local_refs(|(db, inputs, expected)| {
            let sum = compute_roots(db, inputs);
            assert_eq!(black_box(sum), *expected);
        });
}

/// Deeply verifies cached memos after an unrelated same-durability revision.
#[divan::bench]
fn deep_verify_memo(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let mut db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let inputs = inputs(&db, Durability::LOW);
            let expected = compute_roots(&db, &inputs);
            db.synthetic_write(Durability::LOW);

            (db, inputs, expected)
        })
        .bench_local_refs(|(db, inputs, expected)| {
            let sum = compute_roots(db, inputs);
            assert_eq!(black_box(sum), *expected);
        });
}

/// Deeply verifies cached memos, finds changed inputs, and re-executes the query tree.
#[divan::bench]
fn deep_verify_then_execute(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let mut db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let inputs = inputs(&db, Durability::LOW);
            let old_expected = compute_roots(&db, &inputs);
            assert_eq!(black_box(old_expected), expected_sum());

            for (value, input) in inputs.iter().copied().enumerate() {
                input.set_value(black_box(&mut db)).to(black_box(value * 2));
            }

            (db, inputs, expected_sum_doubled_values())
        })
        .bench_local_refs(|(db, inputs, expected)| {
            let sum = compute_roots(db, inputs);
            assert_eq!(black_box(sum), *expected);
        });
}

/// Fetches cached query trees whose inputs outlive a lower-durability revision.
#[divan::bench]
fn validate_high_durability(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let mut db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let inputs = inputs(&db, Durability::HIGH);
            let expected = compute_roots(&db, &inputs);
            db.synthetic_write(Durability::LOW);

            (db, inputs, expected)
        })
        .bench_local_refs(|(db, inputs, expected)| {
            let sum = compute_roots(db, inputs);
            assert_eq!(black_box(sum), *expected);
        });
}

fn inputs(db: &salsa::DatabaseImpl, durability: Durability) -> Vec<QueryInput> {
    (0..ROOTS)
        .map(|value| QueryInput::builder(value).durability(durability).new(db))
        .collect()
}

fn compute_roots(db: &salsa::DatabaseImpl, inputs: &[QueryInput]) -> usize {
    let mut sum = 0;

    for input in inputs.iter().copied() {
        sum += root(black_box(db), black_box(input));
    }

    sum
}

fn expected_sum() -> usize {
    (0..ROOTS).map(expected_root).sum()
}

fn expected_sum_doubled_values() -> usize {
    (0..ROOTS).map(|value| expected_root(value * 2)).sum()
}

fn expected_root(value: usize) -> usize {
    4 * value + 10
}

fn warm_db(db: &salsa::DatabaseImpl) {
    let input = WarmupInput::new(black_box(db), black_box(13));
    let value = warmup_query(black_box(db), black_box(input));
    assert_eq!(black_box(value), 13);
}

#[salsa::input]
struct WarmupInput {
    #[returns(copy)]
    value: usize,
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn warmup_query(db: &dyn salsa::Database, input: WarmupInput) -> usize {
    input.value(db)
}

#[salsa::input]
struct QueryInput {
    #[returns(copy)]
    value: usize,
}

/// Root of a 7-query tree with 10 dependency edges per input:
/// root -> 2 branches -> 4 leaves -> 4 input field reads.
#[salsa::tracked(returns(copy))]
#[inline(never)]
fn root(db: &dyn salsa::Database, input: QueryInput) -> usize {
    branch_a(db, input) + branch_b(db, input)
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn branch_a(db: &dyn salsa::Database, input: QueryInput) -> usize {
    leaf_a1(db, input) + leaf_a2(db, input)
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn branch_b(db: &dyn salsa::Database, input: QueryInput) -> usize {
    leaf_b1(db, input) + leaf_b2(db, input)
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn leaf_a1(db: &dyn salsa::Database, input: QueryInput) -> usize {
    input.value(db) + 1
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn leaf_a2(db: &dyn salsa::Database, input: QueryInput) -> usize {
    input.value(db) + 2
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn leaf_b1(db: &dyn salsa::Database, input: QueryInput) -> usize {
    input.value(db) + 3
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn leaf_b2(db: &dyn salsa::Database, input: QueryInput) -> usize {
    input.value(db) + 4
}
