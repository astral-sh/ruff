use std::hint::black_box;
use std::mem::transmute;

const MANY: usize = 256;

fn main() {
    divan::main();
}

/// Reads one input field per query across many values.
#[divan::bench]
fn input_field_read_many(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let inputs = (0..MANY)
                .map(|value| InputValue::new(black_box(&db), black_box(value)))
                .collect::<Vec<_>>();
            (db, inputs)
        })
        .bench_local_refs(|(db, inputs)| {
            let mut sum = 0;

            for input in inputs.iter().copied() {
                sum += read_input_field(black_box(db), black_box(input));
            }

            assert_eq!(black_box(sum), expected_sum(MANY));
        });
}

/// Reads one interned field per query across many values.
#[divan::bench]
fn interned_field_read_many(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let interned_values = (0..MANY)
                .map(|value| InternedValue::new(black_box(&db), black_box(value)))
                .collect::<Vec<_>>();
            // SAFETY: The interned values are returned together with the database they came from.
            // The benchmark only uses them while that database is still alive.
            let interned_values: Vec<InternedValue<'static>> =
                unsafe { transmute(interned_values) };
            (db, interned_values)
        })
        .bench_local_refs(|(db, interned_values)| {
            let mut sum = 0;

            for interned in interned_values.iter().copied() {
                sum += read_interned_field(black_box(db), black_box(interned));
            }

            assert_eq!(black_box(sum), expected_sum(MANY));
        });
}

/// Reads one tracked field per query across many values.
#[divan::bench]
fn tracked_field_read_many(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let input = TrackedInput::new(black_box(&db), black_box(MANY));
            // SAFETY: The tracked values are returned together with the database they came from.
            // The benchmark only uses them while that database is still alive.
            let tracked_values: Vec<TrackedValue<'static>> =
                unsafe { transmute(make_tracked_values(black_box(&db), black_box(input)).clone()) };
            (db, tracked_values)
        })
        .bench_local_refs(|(db, tracked_values)| {
            let mut sum = 0;

            for tracked in tracked_values.iter().copied() {
                sum += read_tracked_field(black_box(db), black_box(tracked));
            }

            assert_eq!(black_box(sum), expected_sum(MANY));
        });
}

/// Reads one untracked field per query across many tracked values.
#[divan::bench]
fn untracked_field_read_many(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::default();
            warm_db(&db);
            let input = TrackedInput::new(black_box(&db), black_box(MANY));
            // SAFETY: The tracked values are returned together with the database they came from.
            // The benchmark only uses them while that database is still alive.
            let tracked_values: Vec<TrackedValue<'static>> =
                unsafe { transmute(make_tracked_values(black_box(&db), black_box(input)).clone()) };
            (db, tracked_values)
        })
        .bench_local_refs(|(db, tracked_values)| {
            let mut sum = 0;

            for tracked in tracked_values.iter().copied() {
                sum += read_untracked_field(black_box(db), black_box(tracked));
            }

            assert_eq!(black_box(sum), expected_sum(MANY));
        });
}

fn warm_db(db: &salsa::DatabaseImpl) {
    let input = WarmupInput::new(black_box(db), black_box(13));
    let value = warmup_query(black_box(db), black_box(input));
    assert_eq!(black_box(value), 13);
}

fn expected_sum(count: usize) -> usize {
    (0..count).sum()
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

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn read_input_field(db: &dyn salsa::Database, input: InputValue) -> usize {
    input.value(db)
}

#[salsa::interned]
struct InternedValue<'db> {
    #[returns(copy)]
    value: usize,
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn read_interned_field<'db>(db: &'db dyn salsa::Database, interned: InternedValue<'db>) -> usize {
    interned.value(db)
}

#[salsa::input]
struct TrackedInput {
    #[returns(copy)]
    count: usize,
}

#[salsa::tracked]
struct TrackedValue<'db> {
    index: usize,
    #[tracked]
    #[returns(copy)]
    value: usize,
}

#[salsa::tracked(returns(ref))]
#[inline(never)]
fn make_tracked_values(db: &dyn salsa::Database, input: TrackedInput) -> Vec<TrackedValue<'_>> {
    let count = input.count(db);

    (0..count)
        .map(|value| TrackedValue::new(db, value, value))
        .collect()
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn read_tracked_field<'db>(db: &'db dyn salsa::Database, tracked: TrackedValue<'db>) -> usize {
    tracked.value(db)
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn read_untracked_field<'db>(db: &'db dyn salsa::Database, tracked: TrackedValue<'db>) -> usize {
    *tracked.index(db)
}
