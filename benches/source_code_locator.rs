use std::path::Path;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use ruff::ast::operations::{compute_offsets, compute_offsets_v0};
use ruff::fs;

fn criterion_benchmark(c: &mut Criterion) {
    let contents = fs::read_file(Path::new("resources/test/fixtures/D.py")).unwrap();
    c.bench_function("compute_offsets", |b| {
        b.iter(|| compute_offsets(black_box(&contents)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
