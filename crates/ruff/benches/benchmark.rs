use criterion::{black_box, criterion_group, criterion_main, Criterion};
use smallvec::{smallvec, SmallVec};

pub type CallPath<'a> = smallvec::SmallVec<[&'a str; 8]>;

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("call_path");

    group.bench_function("v1", |b| {
        b.iter(|| {
            let name = black_box("foo.bar.baz");
            let call_path: CallPath = smallvec![black_box("typing"), black_box("List")];
            if let Some(..) = call_path.first() {
                let mut source_path: CallPath = name.split('.').collect();
                source_path.extend(call_path.into_iter().skip(1));
                black_box(source_path);
            }
        })
    });

    group.bench_function("v2", |b| {
        b.iter(|| {
            let name = black_box("foo.bar.baz");
            let call_path: CallPath = smallvec![black_box("typing"), black_box("List")];
            if let Some(..) = call_path.first() {
                let parts = name.split('.');
                let mut source_path: CallPath =
                    SmallVec::with_capacity(parts.count() + call_path.len() - 1);
                let parts = name.split('.');
                source_path.extend(parts);
                source_path.extend(call_path.into_iter().skip(1));
                black_box(source_path);
            }
        })
    });

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
