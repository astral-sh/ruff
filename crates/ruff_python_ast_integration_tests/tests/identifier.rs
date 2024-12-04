use ruff_python_ast::identifier;
use ruff_python_parser::{parse_module, ParseError};
use ruff_text_size::TextRange;

macro_rules! test {
    ($func:expr, $contents:expr, $expected:expr, $start:expr, $end:expr) => {{
        let contents = $contents.trim();

        let stmts = parse_module(contents)?.into_suite();
        let stmt = stmts.first().unwrap();
        let range = $func(stmt, contents).unwrap();

        assert_eq!(&contents[range], $expected);
        assert_eq!(range, TextRange::new($start.into(), $end.into()));

        Ok(())
    }};
}

#[test]
fn extract_else_range_loop() -> Result<(), ParseError> {
    let contents = r"
for x in y:
    pass
else:
    pass
";

    test!(identifier::else_loop, contents, "else", 21, 25)
}

#[test]
fn extract_else_range_try() -> Result<(), ParseError> {
    let contents = r"
try:
    pass
except:
    pass
else:
    pass
";

    test!(identifier::else_try, contents, "else", 31, 35)
}

#[test]
fn extract_finally_range_try() -> Result<(), ParseError> {
    let contents = r"
try:
    pass
except:
    pass
else:
    pass
finally:
    pass
";

    test!(identifier::finally, contents, "finally", 46, 53)
}
