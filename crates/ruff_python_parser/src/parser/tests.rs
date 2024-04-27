use crate::{lex, parse, parse_expression, parse_suite, parse_tokens, Mode};

#[test]
fn test_modes() {
    let source = "a[0][1][2][3][4]";

    assert!(parse(source, Mode::Expression).is_ok());
    assert!(parse(source, Mode::Module).is_ok());
}

#[test]
fn test_expr_mode_invalid_syntax1() {
    let source = "first second";
    let error = parse_expression(source).unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_expr_mode_invalid_syntax2() {
    let source = r"first

second
";
    let error = parse_expression(source).unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_expr_mode_invalid_syntax3() {
    let source = r"first

second

third
";
    let error = parse_expression(source).unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_expr_mode_valid_syntax() {
    let source = "first

";
    let expr = parse_expression(source).unwrap();

    insta::assert_debug_snapshot!(expr);
}

#[test]
fn test_unicode_aliases() {
    // https://github.com/RustPython/RustPython/issues/4566
    let source = r#"x = "\N{BACKSPACE}another cool trick""#;
    let parse_ast = parse_suite(source).unwrap();

    insta::assert_debug_snapshot!(parse_ast);
}

#[test]
fn test_ipython_escape_commands() {
    let parse_ast = parse(
        r"
# Normal Python code
(
    a
    %
    b
)

# Dynamic object info
??a.foo
?a.foo
?a.foo?
??a.foo()??

# Line magic
%timeit a = b
%timeit foo(b) % 3
%alias showPath pwd && ls -a
%timeit a =\
  foo(b); b = 2
%matplotlib --inline
%matplotlib \
    --inline

# System shell access
!pwd && ls -a | sed 's/^/\    /'
!pwd \
  && ls -a | sed 's/^/\\    /'
!!cd /Users/foo/Library/Application\ Support/

# Let's add some Python code to make sure that earlier escapes were handled
# correctly and that we didn't consume any of the following code as a result
# of the escapes.
def foo():
    return (
        a
        !=
        b
    )

# Transforms into `foo(..)`
/foo 1 2
;foo 1 2
,foo 1 2

# Indented escape commands
for a in range(5):
    !ls

p1 = !pwd
p2: str = !pwd
foo = %foo \
    bar

% foo
foo = %foo  # comment

# Help end line magics
foo?
foo.bar??
foo.bar.baz?
foo[0]??
foo[0][1]?
foo.bar[0].baz[1]??
foo.bar[0].baz[2].egg??
"
        .trim(),
        Mode::Ipython,
    )
    .unwrap();
    insta::assert_debug_snapshot!(parse_ast);
}

#[test]
fn test_ipython_escape_command_parse_error() {
    let source = r"
a = 1
%timeit a == 1
    "
    .trim();
    let lxr = lex(source, Mode::Ipython);
    let parse_err = parse_tokens(lxr.collect(), source, Mode::Module).unwrap_err();
    assert_eq!(
        parse_err.to_string(),
        "IPython escape commands are only allowed in `Mode::Ipython` at byte range 6..20"
            .to_string()
    );
}
