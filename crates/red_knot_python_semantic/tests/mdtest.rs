use std::ffi::OsStr;
use std::path::Path;

use dir_test::{dir_test, Fixture};

/// Constructs the test name used for individual markdown files
///
/// This code is copied from <https://github.com/fe-lang/dir-test/blob/1c0f41c480a3490bc2653a043ff6e3f8085a1f47/macros/src/lib.rs#L104-L138>
/// and should be updated if they diverge
fn test_name(test_func_name: &str, fixture_path: &Path) -> String {
    assert!(fixture_path.is_file());

    let dir_path = format!("{}/resources/mdtest", std::env!("CARGO_MANIFEST_DIR"));
    let rel_path = fixture_path.strip_prefix(dir_path).unwrap();
    assert!(rel_path.is_relative());

    let mut test_name = test_func_name.to_owned();
    test_name.push_str("__");

    for component in rel_path.parent().unwrap().components() {
        let component = component
            .as_os_str()
            .to_string_lossy()
            .replace(|c: char| c.is_ascii_punctuation(), "_");
        test_name.push_str(&component);
        test_name.push('_');
    }

    test_name.push_str(
        &rel_path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .replace(|c: char| c.is_ascii_punctuation(), "_"),
    );

    test_name
}

/// See `crates/red_knot_test/README.md` for documentation on these tests.
#[dir_test(
    dir: "$CARGO_MANIFEST_DIR/resources/mdtest",
    glob: "**/*.md"
)]
#[allow(clippy::needless_pass_by_value)]
fn mdtest(fixture: Fixture<&str>) {
    let fixture_path = Path::new(fixture.path());
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_dir.parent().and_then(Path::parent).unwrap();

    let long_title = fixture_path
        .strip_prefix(workspace_root)
        .unwrap()
        .to_str()
        .unwrap();
    let short_title = fixture_path.file_name().and_then(OsStr::to_str).unwrap();

    let test_name = test_name("mdtest", fixture_path);

    red_knot_test::run(fixture_path, long_title, short_title, &test_name);
}
