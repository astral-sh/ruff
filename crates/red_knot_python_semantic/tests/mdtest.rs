use dir_test::{dir_test, Fixture};
use red_knot_test::run;
use std::path::PathBuf;

/// See `crates/red_knot_test/README.md` for documentation on these tests.
#[dir_test(
    dir: "$CARGO_MANIFEST_DIR/resources/mdtest",
    glob: "**/*.md"
)]
fn mdtest(fixture: Fixture<&str>) {
    let path = fixture.path();

    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("mdtest")
        .canonicalize()
        .unwrap();

    let relative_path = path
        .strip_prefix(crate_dir.to_str().unwrap())
        .unwrap_or(path)
        .to_string();

    let path_buf = PathBuf::from(path);
    run(&path_buf, &relative_path);
}
