use std::path::Path;

use ruff_benchmark::criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};

use ruff_benchmark::{
    TestCase, LARGE_DATASET, NUMPY_CTYPESLIB, NUMPY_GLOBALS, PYDANTIC_TYPES, UNICODE_PYPINYIN,
};
use ruff_python_formatter::{format_module_ast, PreviewMode, PyFormatOptions};
use ruff_python_parser::{parse, Mode, ParseOptions};
use ruff_python_trivia::CommentRanges;

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64"
    )
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn create_test_cases() -> Vec<TestCase> {
    vec![
        TestCase::fast(NUMPY_GLOBALS.clone()),
        TestCase::fast(UNICODE_PYPINYIN.clone()),
        TestCase::normal(PYDANTIC_TYPES.clone()),
        TestCase::normal(NUMPY_CTYPESLIB.clone()),
        TestCase::slow(LARGE_DATASET.clone()),
    ]
}

fn benchmark_formatter(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("formatter");

    for case in create_test_cases() {
        group.throughput(Throughput::Bytes(case.code().len() as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(case.name()),
            &case,
            |b, case| {
                // Parse the source.
                let parsed = parse(case.code(), ParseOptions::from(Mode::Module))
                    .expect("Input should be a valid Python code");

                let comment_ranges = CommentRanges::from(parsed.tokens());

                b.iter(|| {
                    let options = PyFormatOptions::from_extension(Path::new(case.name()))
                        .with_preview(PreviewMode::Enabled);
                    let formatted =
                        format_module_ast(&parsed, &comment_ranges, case.code(), options)
                            .expect("Formatting to succeed");

                    formatted.print().expect("Printing to succeed")
                });
            },
        );
    }

    group.finish();
}

criterion_group!(formatter, benchmark_formatter);
criterion_main!(formatter);
