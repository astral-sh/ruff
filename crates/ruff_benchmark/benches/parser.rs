use ruff_benchmark::criterion;

use criterion::{
    criterion_group, criterion_main, measurement::WallTime, BenchmarkId, Criterion, Throughput,
};
use ruff_benchmark::{
    TestCase, LARGE_DATASET, NUMPY_CTYPESLIB, NUMPY_GLOBALS, PYDANTIC_TYPES, UNICODE_PYPINYIN,
};
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_python_ast::Stmt;
use ruff_python_parser::parse_module;

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

struct CountVisitor {
    count: usize,
}

impl<'a> StatementVisitor<'a> for CountVisitor {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
        self.count += 1;
    }
}

fn benchmark_parser(criterion: &mut Criterion<WallTime>) {
    let test_cases = create_test_cases();
    let mut group = criterion.benchmark_group("parser");

    for case in test_cases {
        group.throughput(Throughput::Bytes(case.code().len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(case.name()),
            &case,
            |b, case| {
                b.iter(|| {
                    let parsed = parse_module(case.code())
                        .expect("Input should be a valid Python code")
                        .into_suite();

                    let mut visitor = CountVisitor { count: 0 };
                    visitor.visit_body(&parsed);
                    visitor.count
                });
            },
        );
    }

    group.finish();
}

criterion_group!(parser, benchmark_parser);
criterion_main!(parser);
