use ruff_python_ast::stmt_if::elif_else_range;
use ruff_python_parser::{parse_module, ParseError};
use ruff_text_size::TextSize;

#[test]
fn extract_elif_else_range() -> Result<(), ParseError> {
    let contents = "if a:
    ...
elif b:
    ...
";
    let parsed = parse_module(contents)?;
    let if_stmt = parsed
        .suite()
        .first()
        .expect("module should contain at least one statement")
        .as_if_stmt()
        .expect("first statement should be an `if` statement");
    let range = elif_else_range(&if_stmt.elif_else_clauses[0], contents).unwrap();
    assert_eq!(range.start(), TextSize::from(14));
    assert_eq!(range.end(), TextSize::from(18));

    let contents = "if a:
    ...
else:
    ...
";
    let parsed = parse_module(contents)?;
    let if_stmt = parsed
        .suite()
        .first()
        .expect("module should contain at least one statement")
        .as_if_stmt()
        .expect("first statement should be an `if` statement");
    let range = elif_else_range(&if_stmt.elif_else_clauses[0], contents).unwrap();
    assert_eq!(range.start(), TextSize::from(14));
    assert_eq!(range.end(), TextSize::from(18));

    Ok(())
}
