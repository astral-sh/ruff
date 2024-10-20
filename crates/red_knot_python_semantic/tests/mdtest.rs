use dir_test::{dir_test, Fixture};
use std::path::PathBuf;

#[dir_test(
    dir: "$CARGO_MANIFEST_DIR/resources/mdtest",
    glob: "**/*.md"
)]
fn mdtest(fixture: Fixture<&str>) {
    // Get the file path as a string
    let path = fixture.path();

    // Convert the string path and strip the directory prefix (e.g., "resources/mdtest")
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("mdtest")
        .canonicalize()
        .unwrap();

    path.strip_prefix(crate_dir.to_str().unwrap()).unwrap_or(path).to_string();
}
