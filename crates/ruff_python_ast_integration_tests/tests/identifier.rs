use ruff_python_ast::identifier;
use ruff_python_parser::{parse_module, ParseError};
use ruff_text_size::{TextRange, TextSize};

#[test]
fn extract_else_range() -> Result<(), ParseError> {
    let contents = r"
for x in y:
    pass
else:
    pass
"
    .trim();
    let stmts = parse_module(contents)?.into_suite();
    let stmt = stmts.first().unwrap();
    let range = identifier::else_(stmt, contents).unwrap();
    assert_eq!(&contents[range], "else");
    assert_eq!(
        range,
        TextRange::new(TextSize::from(21), TextSize::from(25))
    );
    Ok(())
}
