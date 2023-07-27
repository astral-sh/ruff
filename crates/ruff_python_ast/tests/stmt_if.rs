use ruff_python_ast::stmt_if::elif_else_range;
use ruff_python_ast::Stmt;
use ruff_python_parser::{Parse, ParseError};
use ruff_text_size::TextSize;

#[test]
fn extract_elif_else_range() -> Result<(), ParseError> {
    let contents = "if a:
    ...
elif b:
    ...
";
    let stmt = Stmt::parse(contents, "<filename>")?;
    let stmt = Stmt::as_if_stmt(&stmt).unwrap();
    let range = elif_else_range(&stmt.elif_else_clauses[0], contents).unwrap();
    assert_eq!(range.start(), TextSize::from(14));
    assert_eq!(range.end(), TextSize::from(18));

    let contents = "if a:
    ...
else:
    ...
";
    let stmt = Stmt::parse(contents, "<filename>")?;
    let stmt = Stmt::as_if_stmt(&stmt).unwrap();
    let range = elif_else_range(&stmt.elif_else_clauses[0], contents).unwrap();
    assert_eq!(range.start(), TextSize::from(14));
    assert_eq!(range.end(), TextSize::from(18));

    Ok(())
}
