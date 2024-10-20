use dir_test::{dir_test, Fixture};
use red_knot_test::run;
use std::path::Path;

/// See `crates/red_knot_test/README.md` for documentation on these tests.
#[dir_test(
    dir: "$CARGO_MANIFEST_DIR/resources/mdtest",
    glob: "**/*.md"
)]
#[allow(clippy::needless_pass_by_value)]
fn mdtest(fixture: Fixture<&str>) {
    let path = fixture.path();

    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources/mdtest")
        .canonicalize()
        .unwrap();

    let relative_path = path
        .strip_prefix(crate_dir.to_str().unwrap())
        .unwrap_or(path);

    run(Path::new(path), relative_path);
}
