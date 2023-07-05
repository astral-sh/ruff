use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruff_text_size::TextSize;

use ruff::noqa::Directive;

pub fn directive_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Directive");
    for i in [
        "# noqa: F401",
        "# noqa: F401, F841",
        "# noqa",
        "# type: ignore # noqa: E501",
        "# type: ignore # nosec",
        "# some very long comment that # is interspersed with characters but # no directive",
    ]
    .iter()
    {
        group.bench_with_input(BenchmarkId::new("Regex", i), i, |b, _i| {
            b.iter(|| Directive::try_extract(black_box(i), TextSize::default()))
        });
        group.bench_with_input(BenchmarkId::new("Parser", i), i, |b, _i| {
            b.iter(|| Directive::try_parse(black_box(i), TextSize::default()))
        });
    }
    group.finish();
}

criterion_group!(benches, directive_benchmark);
criterion_main!(benches);
