use anyhow::anyhow;
use camino::Utf8Path;

/// See `crates/ty_test/README.md` for documentation on these tests.
#[expect(clippy::needless_pass_by_value)]
fn mdtest(fixture_path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    let short_title = fixture_path
        .file_name()
        .ok_or_else(|| anyhow!("Expected fixture path to have a file name"))?;

    let crate_dir = Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
    let snapshot_path = crate_dir.join("resources").join("mdtest").join("snapshots");
    let absolute_fixture_path = crate_dir.join(fixture_path);
    let workspace_relative_fixture_path = Utf8Path::new("crates/ty_python_semantic")
        .join(fixture_path.strip_prefix(".").unwrap_or(fixture_path));

    let test_name = fixture_path
        .strip_prefix("./resources/mdtest")
        .unwrap_or(fixture_path)
        .as_str();

    // Limit multithreading in tests to avoid that they compete
    // for the same resources (tests are run concurrently most of the time).
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();

    pool.install(|| {
        ty_test::run(
            &absolute_fixture_path,
            &workspace_relative_fixture_path,
            &content,
            &snapshot_path,
            short_title,
            test_name,
            "ty_python_semantic",
        )
    })?;

    Ok(())
}

datatest_stable::harness! {
    { test = mdtest, root = "./resources/mdtest", pattern = r"\.md$" },
}
