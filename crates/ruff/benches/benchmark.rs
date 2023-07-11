use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ruff::ShebangDirective;

pub fn directive_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Directive");
    for i in [
        "not a match",
        "print('test')  #!/usr/bin/python",
        "#!/usr/bin/env python",
        "  #!/usr/bin/env python",
        "# flake8: noqa: F401, F841",
        "# ruff: noqa: F401, F841",
        "# some very long comment that # is interspersed with characters but # no directive",
    ]
    .iter()
    {
        group.bench_with_input(BenchmarkId::new("Regex", i), i, |b, _i| {
            b.iter(|| ShebangDirective::try_extract(black_box(i)))
        });
        group.bench_with_input(BenchmarkId::new("Lexer", i), i, |b, _i| {
            b.iter(|| ShebangDirective::try_lex(black_box(i)))
        });
    }
    group.finish();
}

criterion_group!(benches, directive_benchmark);
criterion_main!(benches);
