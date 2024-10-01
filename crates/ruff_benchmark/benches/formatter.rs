use std::path::Path;

use ruff_benchmark::criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use ruff_benchmark::{TestCase, TestFile, TestFileDownloadError};
use ruff_python_formatter::{format_module_ast, PreviewMode, PyFormatOptions};
use ruff_python_parser::{parse, Mode};
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

fn create_test_cases() -> Result<Vec<TestCase>, TestFileDownloadError> {
    Ok(vec![
        TestCase::fast(TestFile::try_download("numpy/globals.py", "https://raw.githubusercontent.com/numpy/numpy/89d64415e349ca75a25250f22b874aa16e5c0973/numpy/_globals.py")?),
        TestCase::fast(TestFile::try_download("unicode/pypinyin.py", "https://raw.githubusercontent.com/mozillazg/python-pinyin/9521e47d96e3583a5477f5e43a2e82d513f27a3f/pypinyin/standard.py")?),
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

fn benchmark_formatter(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("formatter");
    let test_cases = create_test_cases().unwrap();

    for case in test_cases {
        group.throughput(Throughput::Bytes(case.code().len() as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(case.name()),
            &case,
            |b, case| {
                // Parse the source.
                let parsed =
                    parse(case.code(), Mode::Module).expect("Input should be a valid Python code");

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
