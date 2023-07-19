use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{
    criterion_group, criterion_main, BenchmarkGroup, BenchmarkId, Criterion, Throughput,
};

use ruff::linter::lint_only;
use ruff::settings::{flags, Settings};
use ruff::RuleSelector;
use ruff_benchmark::{TestCase, TestCaseSpeed, TestFile, TestFileDownloadError};

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
        TestCase::fast(TestFile::try_download("numpy/globals.py", "https://raw.githubusercontent.com/numpy/numpy/89d64415e349ca75a25250f22b874aa16e5c0973/numpy/_globals.py")?),
        TestCase::normal(TestFile::try_download(
            "pydantic/types.py",
            "https://raw.githubusercontent.com/pydantic/pydantic/83b3c49e99ceb4599d9286a3d793cea44ac36d4b/pydantic/types.py",
        )?),
        TestCase::normal(TestFile::try_download("numpy/ctypeslib.py", "https://raw.githubusercontent.com/numpy/numpy/e42c9503a14d66adfd41356ef5640c6975c45218/numpy/ctypeslib.py")?),
        TestCase::slow(TestFile::try_download(
            "large/dataset.py",
            "https://raw.githubusercontent.com/DHI/mikeio/b7d26418f4db2909b0aa965253dbe83194d7bb5b/tests/test_dataset.py",
        )?),
    ])
}

fn benchmark_linter(mut group: BenchmarkGroup<WallTime>, settings: &Settings) {
    let test_cases = create_test_cases().unwrap();

    for case in test_cases {
        group.throughput(Throughput::Bytes(case.code().len() as u64));
        group.measurement_time(match case.speed() {
            TestCaseSpeed::Fast => Duration::from_secs(10),
            TestCaseSpeed::Normal => Duration::from_secs(20),
            TestCaseSpeed::Slow => Duration::from_secs(45),
        });

        group.bench_with_input(
            BenchmarkId::from_parameter(case.name()),
            &case,
            |b, case| {
                b.iter(|| {
                    let result = lint_only(
                        case.code(),
                        &case.path(),
                        None,
                        settings,
                        flags::Noqa::Enabled,
                        None,
                    );

                    // Assert that file contains no parse errors
                    assert_eq!(result.error, None);
                });
            },
        );
    }

    group.finish();
}

fn benchmark_default_rules(criterion: &mut Criterion) {
    let group = criterion.benchmark_group("linter/default-rules");
    benchmark_linter(group, &Settings::default());
}

fn benchmark_all_rules(criterion: &mut Criterion) {
    let settings = Settings {
        rules: RuleSelector::All.into_iter().collect(),
        ..Settings::default()
    };

    let group = criterion.benchmark_group("linter/all-rules");
    benchmark_linter(group, &settings);
}

criterion_group!(default_rules, benchmark_default_rules);
criterion_group!(all_rules, benchmark_all_rules);
criterion_main!(default_rules, all_rules);
