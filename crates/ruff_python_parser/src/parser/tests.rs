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
