use ruff_python_parser::parse_module;

#[test]
fn pattern_is_wildcard() {
    let source_code = r"
match subject:
    case _ as x: ...
    case _ | _: ...
    case _: ...
";
    let parsed = parse_module(source_code).unwrap();
    let cases = &parsed.syntax().body[0].as_match_stmt().unwrap().cases;
    for case in cases {
        assert!(case.pattern.is_wildcard());
    }
}
