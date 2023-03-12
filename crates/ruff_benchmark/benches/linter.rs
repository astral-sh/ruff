use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ruff::linter::lint_only;
use ruff::settings::{flags, Settings};
use ruff_benchmark::{TestCase, TestCaseSpeed, TestFile, TestFileDownloadError};
use std::time::Duration;

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

fn create_test_cases() -> Result<Vec<TestCase>, TestFileDownloadError> {
    Ok(vec![
        TestCase::fast(TestFile::try_download("numpy/globals.py", "https://github.com/numpy/numpy/blob/89d64415e349ca75a25250f22b874aa16e5c0973/numpy/_globals.py")?),
        TestCase::normal(TestFile::try_download(
            "pydantic/types.py",
            "https://raw.githubusercontent.com/pydantic/pydantic/main/pydantic/types.py",
        )?),
        TestCase::normal(TestFile::try_download("numpy/ctypeslib.py", "https://github.com/numpy/numpy/blob/main/numpy/ctypeslib.py")?),
        TestCase::slow(TestFile::try_download(
            "large/dataset.py",
            "https://raw.githubusercontent.com/DHI/mikeio/b7d26418f4db2909b0aa965253dbe83194d7bb5b/tests/test_dataset.py",
        )?),
    ])
}

fn benchmark_linter(criterion: &mut Criterion) {
    let test_cases = create_test_cases().unwrap();
    let mut group = criterion.benchmark_group("linter");

    for case in test_cases {
        group.throughput(Throughput::Bytes(case.code().len() as u64));
        group.measurement_time(match case.speed() {
            TestCaseSpeed::Fast => Duration::from_secs(10),
            TestCaseSpeed::Normal => Duration::from_secs(20),
            TestCaseSpeed::Slow => Duration::from_secs(30),
        });
        group.bench_with_input(
            BenchmarkId::from_parameter(case.name()),
            &case,
            |b, case| {
                b.iter(|| {
                    lint_only(
                        case.code(),
                        &case.path(),
                        None,
                        &black_box(Settings::default()),
                        flags::Noqa::Enabled,
                        flags::Autofix::Enabled,
                    )
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_linter);
criterion_main!(benches);
