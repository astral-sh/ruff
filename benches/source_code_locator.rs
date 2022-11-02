use std::path::Path;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ruff::fs;
use ruff::source_code_locator::compute_offsets;

fn criterion_benchmark(c: &mut Criterion) {
    let contents = fs::read_file(Path::new("resources/test/fixtures/D.py")).unwrap();
    c.bench_function("compute_offsets", |b| {
        b.iter(|| compute_offsets(black_box(&contents)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
