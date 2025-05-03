use camino::Utf8Path;
use dir_test::{dir_test, Fixture};
use red_knot_test::OutputFormat;

/// See `crates/red_knot_test/README.md` for documentation on these tests.
#[dir_test(
    dir: "$CARGO_MANIFEST_DIR/resources/mdtest",
    glob: "**/*.md"
)]
#[allow(clippy::needless_pass_by_value)]
fn mdtest(fixture: Fixture<&str>) {
    let absolute_fixture_path = Utf8Path::new(fixture.path());
    let crate_dir = Utf8Path::new(env!("CARGO_MANIFEST_DIR"));
    let snapshot_path = crate_dir.join("resources").join("mdtest").join("snapshots");
    let workspace_root = crate_dir.ancestors().nth(2).unwrap();

    let relative_fixture_path = absolute_fixture_path.strip_prefix(workspace_root).unwrap();
    let short_title = absolute_fixture_path.file_name().unwrap();

    let test_name = test_name("mdtest", absolute_fixture_path);

    let output_format = if std::env::var("MDTEST_GITHUB_ANNOTATIONS_FORMAT").is_ok() {
        OutputFormat::GitHub
    } else {
        OutputFormat::Cli
    };

    red_knot_test::run(
        absolute_fixture_path,
        relative_fixture_path,
        &snapshot_path,
        short_title,
        &test_name,
        output_format,
    );
}

/// Constructs the test name used for individual markdown files
///
/// This code is copied from <https://github.com/fe-lang/dir-test/blob/1c0f41c480a3490bc2653a043ff6e3f8085a1f47/macros/src/lib.rs#L104-L138>
/// and should be updated if they diverge
fn test_name(test_func_name: &str, fixture_path: &Utf8Path) -> String {
    assert!(fixture_path.is_file());

    let dir_path = format!("{}/resources/mdtest", std::env!("CARGO_MANIFEST_DIR"));
    let rel_path = fixture_path.strip_prefix(dir_path).unwrap();
    assert!(rel_path.is_relative());

    let mut test_name = test_func_name.to_owned();
    test_name.push_str("__");

    for component in rel_path.parent().unwrap().components() {
        let component = component
            .as_str()
            .replace(|c: char| c.is_ascii_punctuation(), "_");
        test_name.push_str(&component);
        test_name.push('_');
    }

    test_name.push_str(
        &rel_path
            .file_stem()
            .unwrap()
            .replace(|c: char| c.is_ascii_punctuation(), "_"),
    );

    test_name
}
