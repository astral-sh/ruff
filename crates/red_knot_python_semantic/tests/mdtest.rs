use red_knot_test::run;
use std::path::PathBuf;

#[rstest::rstest]
fn mdtest(#[files("resources/mdtest/**/*.md")] path: PathBuf) {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("mdtest")
        .canonicalize()
        .unwrap();
    let title = path.strip_prefix(crate_dir).unwrap();
    run(&path, title.as_os_str().to_str().unwrap());
}
