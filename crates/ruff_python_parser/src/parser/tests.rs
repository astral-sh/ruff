use crate::parser::Parser;
use crate::token_source::TokenSource;
use crate::Mode;

mod parser;
mod suite;

// This is a sanity test for what looks like an ipython directive being
// assigned to. Although this doesn't actually parse as an assignment
// statement, but rather, a directive whose value is `foo = 42`.
#[test]
fn ok_ipy_escape_command() {
    use crate::Mode;

    let src = r"!foo = 42";
    let tokens = crate::lexer::lex(src, Mode::Ipython).collect();
    let ast = crate::parse_tokens(tokens, src, Mode::Ipython);
    insta::assert_debug_snapshot!(ast);
}

// Test that is intentionally ignored by default.
// Use it for quickly debugging a parser issue.
#[test]
#[ignore]
fn parser_quick_test() {
    let src = r"if True:
    1
    ...
if x < 1:
    ...
else:
    pass

if a:
    pass
elif b:
    ...
";

    let tokens = crate::lexer::lex(src, Mode::Module).collect();
    let ast = Parser::new(src, Mode::Module, TokenSource::new(tokens)).parse_program();

    assert_eq!(&ast.parse_errors, &[]);
}
