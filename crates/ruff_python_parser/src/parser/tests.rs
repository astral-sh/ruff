mod parser;
mod suite;

use crate::{parse, parse_suite, Mode};

// This is a sanity test for what looks like an ipython directive being
// assigned to. Although this doesn't actually parse as an assignment
// statement, but rather, a directive whose value is `foo = 42`.
#[test]
fn ok_ipy_escape_command() {
    let source = r"!foo = 42";
    let ast = parse(source, Mode::Ipython);

    insta::assert_debug_snapshot!(ast);
}

#[test]
fn test_modes() {
    let source = "a[0][1][2][3][4]";

    assert!(parse(source, Mode::Expression).is_ok());
    assert!(parse(source, Mode::Module).is_ok());
}

#[test]
fn test_unicode_aliases() {
    // https://github.com/RustPython/RustPython/issues/4566
    let source = r#"x = "\N{BACKSPACE}another cool trick""#;
    let parse_ast = parse_suite(source).unwrap();

    insta::assert_debug_snapshot!(parse_ast);
}
