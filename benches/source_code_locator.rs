use std::path::Path;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ropey::Rope;
use ruff::fs;

fn criterion_benchmark(c: &mut Criterion) {
    let contents = fs::read_file(Path::new("resources/test/fixtures/D.py")).unwrap();
    c.bench_function("rope", |b| {
        b.iter(|| {
            let rope = Rope::from_str(black_box(&contents));
            rope.line_to_char(black_box(4));
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
