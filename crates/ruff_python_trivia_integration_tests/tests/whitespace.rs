use ruff_python_parser::{parse_module, ParseError};
use ruff_python_trivia::has_trailing_content;
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

#[test]
fn trailing_content() -> Result<(), ParseError> {
    let contents = "x = 1";
    let suite = parse_module(contents)?.into_suite();
    let stmt = suite.first().unwrap();
    let locator = Locator::new(contents);
    assert!(!has_trailing_content(stmt.end(), &locator));

    let contents = "x = 1; y = 2";
    let suite = parse_module(contents)?.into_suite();
    let stmt = suite.first().unwrap();
    let locator = Locator::new(contents);
    assert!(has_trailing_content(stmt.end(), &locator));

    let contents = "x = 1  ";
    let suite = parse_module(contents)?.into_suite();
    let stmt = suite.first().unwrap();
    let locator = Locator::new(contents);
    assert!(!has_trailing_content(stmt.end(), &locator));

    let contents = "x = 1  # Comment";
    let suite = parse_module(contents)?.into_suite();
    let stmt = suite.first().unwrap();
    let locator = Locator::new(contents);
    assert!(!has_trailing_content(stmt.end(), &locator));

    let contents = r"
x = 1
y = 2
"
    .trim();
    let suite = parse_module(contents)?.into_suite();
    let stmt = suite.first().unwrap();
    let locator = Locator::new(contents);
    assert!(!has_trailing_content(stmt.end(), &locator));

    Ok(())
}
