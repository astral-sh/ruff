use std::path::PathBuf;

use ruff_benchmark::{criterion, TestFile};

use criterion::{
    criterion_group, criterion_main, BenchmarkGroup, BenchmarkId, Criterion, Throughput,
};
use ruff_benchmark::TestCase;
use ruff_linter::linter::{lint_only, ParseSource};
use ruff_linter::settings::rule_table::RuleTable;
use ruff_linter::settings::{flags, LinterSettings};
use ruff_linter::source_kind::SourceKind;
use ruff_linter::{registry::Linter::Flake8Executable, RuleSelector};
use ruff_python_ast::PySourceType;
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

// Disable decay after 10s because it can show up as *random* slow allocations
// in benchmarks. We don't need purging in benchmarks because it isn't important
// to give unallocated pages back to the OS.
// https://jemalloc.net/jemalloc.3.html#opt.dirty_decay_ms
#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64"
    )
))]
#[allow(non_upper_case_globals)]
#[export_name = "_rjem_malloc_conf"]
#[allow(unsafe_code)]
pub static _rjem_malloc_conf: &[u8] = b"dirty_decay_ms:-1,muzzy_decay_ms:-1\0";

static EXE_NO_SHEBANG: TestFile = TestFile::new(
    "flake8_executable/ExeNoShebang.py",
    include_str!("../resources/flake8_executable/ExeNoShebang.py"),
);

static EXE_WITH_SHEBANG: TestFile = TestFile::new(
    "flake8_executable/ExeWithShebang.py",
    include_str!("../resources/flake8_executable/ExeWithShebang.py"),
);

static NOEXE_NO_SHEBANG: TestFile = TestFile::new(
    "flake8_executable/NotExeNoShebang.py",
    include_str!("../resources/flake8_executable/NotExeNoShebang.py"),
);

static NOEXE_WITH_SHEBANG: TestFile = TestFile::new(
    "flake8_executable/NotExeWithShebang.py",
    include_str!("../resources/flake8_executable/NotExeWithShebang.py"),
);

fn create_test_cases() -> Vec<TestCase> {
    vec![
        TestCase::normal(EXE_NO_SHEBANG.clone()),
        TestCase::normal(EXE_WITH_SHEBANG.clone()),
        TestCase::normal(NOEXE_NO_SHEBANG.clone()),
        TestCase::normal(NOEXE_WITH_SHEBANG.clone()),
    ]
}

fn benchmark_linter(mut group: BenchmarkGroup, settings: &LinterSettings) {
    let test_cases = create_test_cases();

    for case in test_cases {
        group.throughput(Throughput::Bytes(case.code().len() as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(case.name()),
            &case,
            |b, case| {
                // Parse the source.
                let parsed =
                    parse_module(case.code()).expect("Input should be a valid Python code");

                b.iter_batched(
                    || parsed.clone(),
                    |parsed| {
                        let path = case.path();
                        lint_only(
                            &path,
                            None,
                            settings,
                            flags::Noqa::Enabled,
                            &SourceKind::Python(case.code().to_string()),
                            PySourceType::from(path.as_path()),
                            ParseSource::Precomputed(parsed),
                        );
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn benchmark_flake8_executable_rules(criterion: &mut Criterion) {
    // Collect only flake8-executable rules from the registry
    let rules: RuleTable = RuleSelector::Linter(Flake8Executable).all_rules().collect();

    let settings = LinterSettings {
        rules,
        project_root: PathBuf::from(file!())
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("resources")
            .join("flake8_executable"),
        ..LinterSettings::default()
    };

    let mut group = criterion.benchmark_group("linter/flake8-executable");
    group.measurement_time(std::time::Duration::from_secs(20));
    group.sample_size(1000);
    benchmark_linter(group, &settings);
}

criterion_group!(flake8_executable_rules, benchmark_flake8_executable_rules);
criterion_main!(flake8_executable_rules);
