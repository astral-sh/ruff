use ruff_python_parser::{ParseError, parse_module};
use ruff_python_trivia::has_trailing_content;
use ruff_text_size::Ranged;

#[test]
fn trailing_content() -> Result<(), ParseError> {
    let contents = "x = 1";
    let suite = parse_module(contents)?.into_suite();
    let stmt = suite.first().unwrap();
    assert!(!has_trailing_content(stmt.end(), contents));

    let contents = "x = 1; y = 2";
    let suite = parse_module(contents)?.into_suite();
    let stmt = suite.first().unwrap();
    assert!(has_trailing_content(stmt.end(), contents));

    let contents = "x = 1  ";
    let suite = parse_module(contents)?.into_suite();
    let stmt = suite.first().unwrap();
    assert!(!has_trailing_content(stmt.end(), contents));

    let contents = "x = 1  # Comment";
    let suite = parse_module(contents)?.into_suite();
    let stmt = suite.first().unwrap();
    assert!(!has_trailing_content(stmt.end(), contents));

    let contents = r"
x = 1
y = 2
"
    .trim();
    let suite = parse_module(contents)?.into_suite();
    let stmt = suite.first().unwrap();
    assert!(!has_trailing_content(stmt.end(), contents));

    Ok(())
}
