use ruff_python_ast::identifier;
use ruff_python_ast::name::QualifiedName;
use ruff_python_parser::{ParseError, parse_expression, parse_module};
use ruff_text_size::{TextRange, TextSize};

#[test]
fn qualify_attribute_chain() {
    let name = QualifiedName::user_defined("numpy.linalg");
    for (source, expected) in [
        ("la", Some(name.clone())),
        (
            "la.norm.__name__",
            Some(QualifiedName::user_defined("numpy.linalg.norm.__name__")),
        ),
        ("None.member", None),
    ] {
        let parsed = parse_expression(source).unwrap();
        assert_eq!(name.qualify(parsed.expr()), expected, "{source}");
    }
}

#[test]
fn extract_else_range() -> Result<(), ParseError> {
    let contents = r"
for x in y:
    pass
else:
    pass
"
    .trim();
    let parsed = parse_module(contents)?;
    let stmt = parsed.suite().first().unwrap();
    let range = identifier::else_(stmt, contents).unwrap();
    assert_eq!(&contents[range], "else");
    assert_eq!(
        range,
        TextRange::new(TextSize::from(21), TextSize::from(25))
    );
    Ok(())
}
