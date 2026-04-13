use anyhow::anyhow;
use camino::Utf8Path;
use ruff_test::OutputFormat;
use ty_static::EnvVars;

/// See `crates/ty_test/README.md` for documentation on these tests.
#[expect(clippy::needless_pass_by_value)]
fn mdtest(fixture_path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    let short_title = fixture_path
        .file_name()
        .ok_or_else(|| anyhow!("Expected fixture path to have a file name"))?;

    let crate_dir = Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
    let snapshot_path = crate_dir.join("resources").join("mdtest").join("snapshots");
    let workspace_relative_fixture_path = Utf8Path::new("crates/ruff_linter")
        .join(fixture_path.strip_prefix(".").unwrap_or(fixture_path));

    let test_name = fixture_path
        .strip_prefix("./resources/mdtest")
        .unwrap_or(fixture_path)
        .as_str();

    let output_format = if std::env::var(EnvVars::MDTEST_GITHUB_ANNOTATIONS_FORMAT).is_ok() {
        OutputFormat::GitHub
    } else {
        OutputFormat::Cli
    };

    ruff_test::run(
        &workspace_relative_fixture_path,
        &content,
        &snapshot_path,
        short_title,
        test_name,
        output_format,
    )?;

    Ok(())
}

datatest_stable::harness! {
    { test = mdtest, root = "./resources/mdtest", pattern = r"\.md$" },
}
