use ruff_benchmark::criterion;

use criterion::{
    criterion_group, criterion_main, measurement::WallTime, BenchmarkId, Criterion, Throughput,
};
use ruff_benchmark::{
    TestCase, LARGE_DATASET, NUMPY_CTYPESLIB, NUMPY_GLOBALS, PYDANTIC_TYPES, UNICODE_PYPINYIN,
};
use ruff_python_parser::{lexer, Mode, TokenKind};

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

fn benchmark_lexer(criterion: &mut Criterion<WallTime>) {
    let test_cases = create_test_cases();
    let mut group = criterion.benchmark_group("lexer");

    for case in test_cases {
        group.throughput(Throughput::Bytes(case.code().len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(case.name()),
            &case,
            |b, case| {
                b.iter(|| {
                    let mut lexer = lexer::lex(case.code(), Mode::Module);
                    loop {
                        let token = lexer.next_token();
                        match token {
                            TokenKind::EndOfFile => break,
                            TokenKind::Unknown => panic!("Input to be a valid Python source code"),
                            _ => {}
                        }
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(lexer, benchmark_lexer);
criterion_main!(lexer);
