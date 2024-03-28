#[cfg(test)]
mod tests {
    use crate::{lexer, parse, parse_suite, parse_tokens, Mode};

    #[test]
    fn test_with_statement() {
        let source = "\
with 0: pass
with 0 as x: pass
with 0, 1: pass
with 0 as x, 1 as y: pass
with 0 if 1 else 2: pass
with 0 if 1 else 2 as x: pass
with (): pass
with () as x: pass
with (0): pass
with (0) as x: pass
with (0,): pass
with (0,) as x: pass
with (0, 1): pass
with (0, 1) as x: pass
with (*a,): pass
with (*a,) as x: pass
with (0, *a): pass
with (0, *a) as x: pass
with (a := 0): pass
with (a := 0) as x: pass
with (a := 0, b := 1): pass
with (a := 0, b := 1) as x: pass
with (0 as a): pass
with (0 as a,): pass
with (0 as a, 1 as b): pass
with (0 as a, 1 as b,): pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_parenthesized_with_statement() {
        let source = "\
with ((a), (b)): pass
with ((a), (b), c as d, (e)): pass
with (a, b): pass
with (a, b) as c: pass
with ((a, b) as c): pass
with (a as b): pass
with (a): pass
with (a := 0): pass
with (a := 0) as x: pass
with ((a)): pass
with ((a := 0)): pass
with (a as b, (a := 0)): pass
with (a, (a := 0)): pass
with (yield): pass
with (yield from a): pass
with ((yield)): pass
with ((yield from a)): pass
";
        insta::assert_debug_snapshot!(parse_suite(source).unwrap());
    }

    #[test]
    fn test_with_statement_invalid() {
        for source in [
            "with 0,: pass",
            "with 0 as x,: pass",
            "with *a: pass",
            "with *a as x: pass",
            "with (*a): pass",
            "with (*a) as x: pass",
            "with *a, 0 as x: pass",
            "with (*a, 0 as x): pass",
            "with 0 as x, *a: pass",
            "with (0 as x, *a): pass",
            "with (0 as x) as y: pass",
            "with (0 as x), 1: pass",
            "with ((0 as x)): pass",
            "with a := 0 as x: pass",
            "with (a := 0 as x): pass",
        ] {
            assert!(parse_suite(source).is_err());
        }
    }

    #[test]
    fn test_invalid_type() {
        // TODO(dhruvmanila): Check error recovery for these cases
        assert!(parse_suite("a: type X = int").is_err());
        assert!(parse_suite("lambda: type X = int").is_err());
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
        let lxr = lexer::lex(source, Mode::Ipython);
        let parse_err = parse_tokens(lxr.collect(), source, Mode::Module).unwrap_err();
        assert_eq!(
            parse_err.to_string(),
            "IPython escape commands are only allowed in `Mode::Ipython` at byte range 6..20"
                .to_string()
        );
    }
}
