use std::hint::black_box;

use ruff_benchmark::criterion;
use ty_module_resolver::ModuleName;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

const VALID: &[(&str, &[&str])] = &[
    ("1", &["typing"]),
    ("2", &["importlib", "resources"]),
    ("4", &["some_package", "submodule", "utilities", "helpers"]),
];

const INVALID: &[(&str, &[&str])] = &[
    (
        "first",
        &["invalid-name", "submodule", "utilities", "helpers"],
    ),
    (
        "last",
        &["some_package", "submodule", "utilities", "invalid-name"],
    ),
];

fn from_components(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("module_name/from_components");

    for &(component_count, components) in VALID {
        group.bench_with_input(
            BenchmarkId::new("valid", component_count),
            components,
            |b, components| {
                b.iter(|| {
                    black_box(ModuleName::from_components(
                        black_box(components).iter().copied(),
                    ))
                });
            },
        );
    }

    for &(invalid_position, components) in INVALID {
        group.bench_with_input(
            BenchmarkId::new("invalid", invalid_position),
            components,
            |b, components| {
                b.iter(|| {
                    black_box(ModuleName::from_components(
                        black_box(components).iter().copied(),
                    ))
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, from_components);
criterion_main!(benches);
