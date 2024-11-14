use std::ffi::OsStr;
use std::path::Path;

use dir_test::{dir_test, Fixture};

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

    red_knot_test::run(fixture_path, long_title, short_title);
}
