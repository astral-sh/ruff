use std::hint::black_box;

use salsa::Accumulator;

const INPUTS: usize = 128;

fn main() {
    divan::main();
}

#[salsa::input]
struct Input {
    #[returns(copy)]
    expressions: usize,
}

#[allow(dead_code)]
#[salsa::accumulator]
struct Diagnostic(String);

#[salsa::interned]
struct Expression<'db> {
    #[returns(copy)]
    number: usize,
}

#[salsa::tracked(returns(deref))]
#[inline(never)]
fn root(db: &dyn salsa::Database, input: Input) -> Vec<usize> {
    (0..input.expressions(db))
        .map(|i| infer_expression(db, Expression::new(db, i)))
        .collect()
}

#[salsa::tracked(returns(copy))]
#[inline(never)]
fn infer_expression<'db>(db: &'db dyn salsa::Database, expression: Expression<'db>) -> usize {
    let number = expression.number(db);

    if number % 10 == 0 {
        Diagnostic(format!("Number is {number}")).accumulate(db);
    }

    if number != 0 && number % 2 == 0 {
        let sub_expression = Expression::new(db, number / 2);
        let _ = infer_expression(db, sub_expression);
    }

    number
}

/// Collects accumulated diagnostics from cached query results.
#[divan::bench(name = "benches::accumulator::accumulator")]
fn accumulator(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            let db = salsa::DatabaseImpl::new();
            let expressions = 1_000;
            let expected_diagnostics = expressions / 10;
            let inputs = (0..INPUTS)
                .map(|_| Input::new(black_box(&db), black_box(expressions)))
                .collect::<Vec<_>>();

            // Pre-warm
            for input in inputs.iter().copied() {
                let result = root(black_box(&db), black_box(input));
                assert!(!black_box(result).is_empty());
            }

            (db, inputs, expected_diagnostics)
        })
        .bench_local_refs(|(db, inputs, expected_diagnostics)| {
            // Measure the cost of collecting accumulators ignoring the cost of running the
            // query itself.
            for input in inputs.iter().copied() {
                let diagnostics = root::accumulated::<Diagnostic>(black_box(db), black_box(input));
                assert_eq!(black_box(diagnostics).len(), *expected_diagnostics);
            }
        });
}
