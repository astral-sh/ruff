use std::hint::black_box;

use salsa::Setter;

const MANY: usize = 256;
const TRACKED_FEW: usize = 32;

fn main() {
    divan::main();
}

/// Creates many input values directly.
#[divan::bench(args = [32, 64, 128, 256, 512, 1024, 10240])]
fn input_create_many(bencher: divan::Bencher, inputs: usize) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            db
        })
        .bench_local_refs(|db| {
            for value in 0..inputs {
                let input = InputValue::new(black_box(&*db), black_box(value));
                black_box(input);
            }
        });
}

/// Interns many distinct values inside a query.
#[divan::bench]
fn intern_distinct_many(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let input = InternInput::new(black_box(&db), black_box(MANY), black_box(1));
            (db, input)
        })
        .bench_local_refs(|(db, input)| {
            let sum = intern_distinct_values(black_box(db), black_box(*input));
            assert_eq!(black_box(sum), MANY);
        });
}

/// Interns the same value many times inside a query.
#[divan::bench]
fn intern_same_many(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let warm_input = InternInput::new(black_box(&db), black_box(MANY), black_box(1));
            let sum = intern_same_value(black_box(&db), black_box(warm_input));

            assert_eq!(black_box(sum), MANY);
            // Create a new input, so that we get a fresh `intern_same_value` query key.
            let input = InternInput::new(black_box(&db), black_box(MANY), black_box(1));
            (db, input)
        })
        .bench_local_refs(|(db, input)| {
            let sum = intern_same_value(black_box(db), black_box(*input));
            assert_eq!(black_box(sum), MANY);
        });
}

/// Creates many tracked structs inside a query.
#[divan::bench]
fn tracked_create_many(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let input = TrackedInput::new(black_box(&db), black_box(MANY), black_box(1));
            (db, input)
        })
        .bench_local_refs(|(db, input)| {
            let sum = create_tracked(black_box(db), black_box(*input));
            assert_eq!(black_box(sum), MANY);
        });
}

/// Recreates many tracked structs and reuses their identities.
#[divan::bench]
fn tracked_reuse_many(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let input = TrackedInput::new(black_box(&db), black_box(MANY), black_box(1));
            let sum = create_tracked(black_box(&db), black_box(input));
            assert_eq!(black_box(sum), MANY);
            (db, input)
        })
        .bench_local_refs(|(db, input)| {
            input.set_value(black_box(db)).to(black_box(2));
            let sum = create_tracked(black_box(db), black_box(*input));
            assert_eq!(black_box(sum), 2 * MANY);
        });
}

/// Recreates a small prefix of previously-created tracked structs.
#[divan::bench]
fn tracked_reuse_few(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let input = TrackedInput::new(black_box(&db), black_box(MANY), black_box(1));
            let sum = create_tracked(black_box(&db), black_box(input));
            assert_eq!(black_box(sum), MANY);
            (db, input)
        })
        .bench_local_refs(|(db, input)| {
            // Recreate only a small prefix of the original outputs. This measures the case where
            // some identities are reused and the rest become stale outputs.
            input.set_count(black_box(db)).to(black_box(TRACKED_FEW));
            let sum = create_tracked(black_box(db), black_box(*input));
            assert_eq!(black_box(sum), TRACKED_FEW);
        });
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
struct InputValue {
    #[returns(copy)]
    value: usize,
}

#[salsa::input]
struct InternInput {
    #[returns(copy)]
    count: usize,
    #[returns(copy)]
    value: usize,
}

#[salsa::interned]
struct InternedValue<'db> {
    #[returns(copy)]
    value: usize,
}

#[salsa::input]
struct TrackedInput {
    #[returns(copy)]
    count: usize,
    #[returns(copy)]
    value: usize,
}

#[salsa::tracked]
struct TrackedValue<'db> {
    #[returns(copy)]
    index: usize,
    #[tracked]
    #[returns(copy)]
    value: usize,
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn create_tracked(db: &dyn salsa::Database, input: TrackedInput) -> usize {
    let count = input.count(db);
    let value = input.value(db);
    let mut sum = 0;

    for index in 0..count {
        let tracked = TrackedValue::new(db, index, value);
        black_box(tracked);
        sum += value;
    }

    sum
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn intern_distinct_values(db: &dyn salsa::Database, input: InternInput) -> usize {
    let count = input.count(db);
    let value = input.value(db);
    let mut sum = 0;

    for offset in 0..count {
        let interned = InternedValue::new(db, value + offset);
        black_box(interned);
        sum += value;
    }

    sum
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn intern_same_value(db: &dyn salsa::Database, input: InternInput) -> usize {
    let count = input.count(db);
    let value = input.value(db);
    let mut sum = 0;

    for _ in 0..count {
        let interned = InternedValue::new(db, value);
        black_box(interned);
        sum += value;
    }

    sum
}
