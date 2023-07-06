use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

use ruff::noqa::ParsedFileExemption;

pub fn directive_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Directive");
    for i in [
        "# noqa: F401",
        "# noqa: F401, F841",
        "# flake8: noqa: F401, F841",
        "# ruff: noqa: F401, F841",
        "# flake8: noqa",
        "# ruff: noqa",
        "# noqa",
        "# type: ignore # noqa: E501",
        "# type: ignore # nosec",
        "# some very long comment that # is interspersed with characters but # no directive",
    ]
    .iter()
    {
        group.bench_with_input(BenchmarkId::new("Regex", i), i, |b, _i| {
            b.iter(|| ParsedFileExemption::try_regex(black_box(i)))
        });
        group.bench_with_input(BenchmarkId::new("Lexer", i), i, |b, _i| {
            b.iter(|| ParsedFileExemption::try_extract(black_box(i)))
        });
    }
    group.finish();
}

criterion_group!(benches, directive_benchmark);
criterion_main!(benches);
