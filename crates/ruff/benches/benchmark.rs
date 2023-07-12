use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruff::comments::shebang::ShebangDirective;

pub fn directive_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Directive");
    for i in [
        "# noqa: F401",
        "# noqa: F401, F841",
        "# flake8: noqa: F401, F841",
        "# ruff: noqa: F401, F841",
        "# flake8: noqa",
        "#! /usr/bin/env python",
        "#! /usr/bin/env python3",
        "    #! /usr/bin/env python3.9",
        "# ruff: noqa",
        "# noqa",
        "# type: ignore # noqa: E501",
        "# type: ignore # nosec",
        "# some very long comment that # is interspersed with characters but # no directive",
    ]
    .iter()
    {
        group.bench_with_input(BenchmarkId::new("Extract", i), i, |b, _i| {
            b.iter(|| ShebangDirective::try_extract(black_box(i)))
        });
        group.bench_with_input(BenchmarkId::new("Cursor", i), i, |b, _i| {
            b.iter(|| ShebangDirective::try_cursor(black_box(i)))
        });
    }
    group.finish();
}

criterion_group!(benches, directive_benchmark);
criterion_main!(benches);
