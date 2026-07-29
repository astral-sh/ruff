use anyhow::anyhow;
use camino::Utf8Path;

thread_local! {
    // Restrict each fixture's Rayon work to one thread so concurrent tests do not compete for the
    // same resources. When fixtures share a process, the harness reuses worker threads, so cache
    // one pool per worker.
    static RAYON_POOL: rayon::ThreadPool = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
}

/// See `crates/ty_test/README.md` for documentation on these tests.
#[expect(clippy::needless_pass_by_value)]
fn mdtest(fixture_path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    run(
        fixture_path,
        &content,
        "./resources/mdtest",
        ty_test::RunOptions::default(),
    )
}

#[expect(clippy::needless_pass_by_value)]
fn lint_doc(fixture_path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    let rule_name = fixture_path
        .file_stem()
        .ok_or_else(|| anyhow!("Expected fixture path to have a file stem"))?;
    let rule = ty_python_semantic::default_lint_registry()
        .get(rule_name)
        .map_err(|err| anyhow!("Expected `{rule_name}` to name a lint rule: {err}"))?;

    run(
        fixture_path,
        &content,
        "./resources/lint_docs",
        ty_test::RunOptions {
            default_error_rule: Some(rule.name.as_str()),
        },
    )
}

fn run(
    fixture_path: &Utf8Path,
    content: &str,
    resource_root: &str,
    options: ty_test::RunOptions,
) -> datatest_stable::Result<()> {
    let short_title = fixture_path
        .file_name()
        .ok_or_else(|| anyhow!("Expected fixture path to have a file name"))?;

    let crate_dir = Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
    let snapshot_path = crate_dir.join("resources").join("mdtest").join("snapshots");
    let absolute_fixture_path = crate_dir.join(fixture_path);
    let workspace_relative_fixture_path = Utf8Path::new("crates/ty_python_semantic")
        .join(fixture_path.strip_prefix(".").unwrap_or(fixture_path));

    let test_name = fixture_path
        .strip_prefix(resource_root)
        .unwrap_or(fixture_path)
        .as_str();

    RAYON_POOL.with(|pool| {
        pool.install(|| {
            ty_test::run(
                &absolute_fixture_path,
                &workspace_relative_fixture_path,
                content,
                &snapshot_path,
                short_title,
                test_name,
                options,
            )
        })
    })?;

    Ok(())
}

datatest_stable::harness! {
    { test = mdtest, root = "./resources/mdtest", pattern = r"\.md$" },
    { test = lint_doc, root = "./resources/lint_docs", pattern = r"\.md$" },
}
