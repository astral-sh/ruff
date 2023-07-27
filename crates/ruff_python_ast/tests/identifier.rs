use ruff_python_ast::Stmt;
use ruff_python_parser::{Parse, ParseError};
use ruff_text_size::{TextRange, TextSize};

use ruff_python_ast::identifier;

#[test]
fn extract_else_range() -> Result<(), ParseError> {
    let contents = r#"
for x in y:
    pass
else:
    pass
"#
    .trim();
    let stmt = Stmt::parse(contents, "<filename>")?;
    let range = identifier::else_(&stmt, contents).unwrap();
    assert_eq!(&contents[range], "else");
    assert_eq!(
        range,
        TextRange::new(TextSize::from(21), TextSize::from(25))
    );
    Ok(())
}
