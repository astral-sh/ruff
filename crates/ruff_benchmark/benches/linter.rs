use ruff_benchmark::criterion;

use criterion::{
    criterion_group, criterion_main, BenchmarkGroup, BenchmarkId, Criterion, Throughput,
};
use ruff_benchmark::{
    TestCase, LARGE_DATASET, NUMPY_CTYPESLIB, NUMPY_GLOBALS, PYDANTIC_TYPES, UNICODE_PYPINYIN,
};
use ruff_linter::linter::{lint_only, ParseSource};
use ruff_linter::rule_selector::PreviewOptions;
use ruff_linter::settings::rule_table::RuleTable;
use ruff_linter::settings::types::PreviewMode;
use ruff_linter::settings::{flags, LinterSettings};
use ruff_linter::source_kind::SourceKind;
use ruff_linter::{registry::Rule, RuleSelector};
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

fn create_test_cases() -> Vec<TestCase> {
    vec![
        TestCase::fast(NUMPY_GLOBALS.clone()),
        TestCase::fast(UNICODE_PYPINYIN.clone()),
        TestCase::normal(PYDANTIC_TYPES.clone()),
        TestCase::normal(NUMPY_CTYPESLIB.clone()),
        TestCase::slow(LARGE_DATASET.clone()),
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
                        let result = lint_only(
                            &path,
                            None,
                            settings,
                            flags::Noqa::Enabled,
                            &SourceKind::Python(case.code().to_string()),
                            PySourceType::from(path.as_path()),
                            ParseSource::Precomputed(parsed),
                        );

                        // Assert that file contains no parse errors
                        assert!(!result.has_syntax_error);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn benchmark_default_rules(criterion: &mut Criterion) {
    let group = criterion.benchmark_group("linter/default-rules");
    benchmark_linter(group, &LinterSettings::default());
}

/// Disables IO based rules because they are a source of flakiness
fn disable_io_rules(rules: &mut RuleTable) {
    rules.disable(Rule::ShebangMissingExecutableFile);
    rules.disable(Rule::ShebangNotExecutable);
}

fn benchmark_all_rules(criterion: &mut Criterion) {
    let mut rules: RuleTable = RuleSelector::All
        .rules(&PreviewOptions {
            mode: PreviewMode::Disabled,
            require_explicit: false,
        })
        .collect();

    disable_io_rules(&mut rules);

    let settings = LinterSettings {
        rules,
        ..LinterSettings::default()
    };

    let group = criterion.benchmark_group("linter/all-rules");
    benchmark_linter(group, &settings);
}

fn benchmark_preview_rules(criterion: &mut Criterion) {
    let mut rules: RuleTable = RuleSelector::All.all_rules().collect();

    disable_io_rules(&mut rules);

    let settings = LinterSettings {
        rules,
        preview: PreviewMode::Enabled,
        ..LinterSettings::default()
    };

    let group = criterion.benchmark_group("linter/all-with-preview-rules");
    benchmark_linter(group, &settings);
}

criterion_group!(default_rules, benchmark_default_rules);
criterion_group!(all_rules, benchmark_all_rules);
criterion_group!(preview_rules, benchmark_preview_rules);
criterion_main!(default_rules, all_rules, preview_rules);
