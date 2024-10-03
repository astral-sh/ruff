use red_knot_test::run;
use std::path::PathBuf;

#[rstest::rstest]
fn mdtest(#[files("resources/mdtest/**/*.md")] path: PathBuf) {
    run(&path);
}
