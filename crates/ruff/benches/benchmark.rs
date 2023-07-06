use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruff_text_size::TextSize;

use ruff::noqa::{Directive, ParsedFileExemption};

pub fn directive_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Directive");
    // for i in [
    //     "# noqa: F401",
    //     "# noqa: F401, F841",
    //     "# noqa",
    //     "# type: ignore # noqa: E501",
    //     "# type: ignore # nosec",
    //     "# some very long comment that # is interspersed with characters but # no directive",
    // ]
    // .iter()
    // {
    //     group.bench_with_input(BenchmarkId::new("Regex", i), i, |b, _i| {
    //         b.iter(|| Directive::try_extract(black_box(i), TextSize::default()))
    //     });
    //     group.bench_with_input(BenchmarkId::new("Find", i), i, |b, _i| {
    //         b.iter(|| Directive::try_parse(black_box(i), TextSize::default()))
    //     });
    //     group.bench_with_input(BenchmarkId::new("AhoCorasick", i), i, |b, _i| {
    //         b.iter(|| Directive::try_parse_aho_corasick(black_box(i), TextSize::default()))
    //     });
    //     group.bench_with_input(BenchmarkId::new("Memchr", i), i, |b, _i| {
    //         b.iter(|| Directive::try_parse_memchr(black_box(i), TextSize::default()))
    //     });
    // }

    for i in [
        "# ruff: noqa",
        "# flake8: NOQA",
        "# noqa: F401, F841",
        "# noqa",
        "# type: ignore # noqa: E501",
        "# type: ignore # nosec",
        "# some very long comment that # is interspersed with characters but # no directive",
    ]
    .iter()
    {
        group.bench_with_input(BenchmarkId::new("Regex", i), i, |b, _i| {
            b.iter(|| ParsedFileExemption::extract(black_box(i)))
        });
        group.bench_with_input(BenchmarkId::new("Parser", i), i, |b, _i| {
            b.iter(|| ParsedFileExemption::parse(black_box(i)))
        });
        group.bench_with_input(BenchmarkId::new("Matches", i), i, |b, _i| {
            b.iter(|| ParsedFileExemption::matches(black_box(i)))
        });
    }

    group.finish();
}

criterion_group!(benches, directive_benchmark);
criterion_main!(benches);
