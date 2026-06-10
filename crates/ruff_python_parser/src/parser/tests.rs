use crate::{Mode, ParseOptions, RECURSIVE_AST_TEST_DEPTH, parse, parse_expression, parse_module};

#[test]
fn test_modes() {
    let source = "a[0][1][2][3][4]";

    assert!(parse(source, ParseOptions::from(Mode::Expression)).is_ok());
    assert!(parse(source, ParseOptions::from(Mode::Module)).is_ok());
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
    let parsed = parse_expression(source).unwrap();

    insta::assert_debug_snapshot!(parsed.expr());
}

#[test]
fn test_unicode_aliases() {
    // https://github.com/RustPython/RustPython/issues/4566
    let source = r#"x = "\N{BACKSPACE}another cool trick""#;
    let suite = parse_module(source).unwrap().into_suite();

    insta::assert_debug_snapshot!(suite);
}

#[test]
fn test_ipython_escape_commands() {
    let parsed = parse(
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
bar = %foo?
baz = !pwd?

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
        ParseOptions::from(Mode::Ipython),
    )
    .unwrap();
    insta::assert_debug_snapshot!(parsed.syntax());
}

#[test]
fn test_fstring_expr_inner_line_continuation_and_t_string() {
    let source = r#"f'{\t"i}'"#;

    let parsed = parse_expression(source);

    let error = parsed.unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_fstring_expr_inner_line_continuation_newline_t_string() {
    let source = r#"f'{\
t"i}'"#;

    let parsed = parse_expression(source);

    let error = parsed.unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_tstring_fstring_middle() {
    let source = "t'{:{F'{\0}F";
    let parsed = parse_expression(source);

    let error = parsed.unwrap_err();

    insta::assert_debug_snapshot!(error);
}

#[test]
fn test_tstring_fstring_middle_fuzzer() {
    let source = "A1[A\u{c}\0:+,>1t'{:f\0:{f\"f\0:\0{fm\0:{f:\u{10}\0\0\0:bb\0{@f>f\u{1}'\0f";
    let parsed = parse_expression(source);

    let error = parsed.unwrap_err();

    insta::assert_debug_snapshot!(error);
}

// `stacker` is a no-op on unsupported platforms, so only require stack
// growth where its native support is well established.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
mod stack_growth {
    use super::*;

    #[test]
    fn nested_parens() {
        let source = format!(
            "{}1{}",
            "(".repeat(RECURSIVE_AST_TEST_DEPTH),
            ")".repeat(RECURSIVE_AST_TEST_DEPTH)
        );
        parse_module(&source).unwrap();
    }

    #[test]
    fn nested_lists() {
        let source = format!(
            "{}1{}",
            "[".repeat(RECURSIVE_AST_TEST_DEPTH),
            "]".repeat(RECURSIVE_AST_TEST_DEPTH)
        );
        parse_module(&source).unwrap();
    }

    #[test]
    fn nested_dictionaries() {
        let source = format!(
            "{}1{}",
            "{'x':".repeat(RECURSIVE_AST_TEST_DEPTH),
            "}".repeat(RECURSIVE_AST_TEST_DEPTH)
        );
        parse_module(&source).unwrap();
    }

    #[test]
    fn unclosed_lists() {
        // Invalid input does not return a deeply nested AST that must be dropped
        // on the test thread, so this does not need the lower Windows limit.
        let source = "[".repeat(5_000);
        assert!(parse_module(&source).is_err());
    }

    #[test]
    fn nested_calls() {
        let source = format!(
            "x = {}1{}",
            "f(".repeat(RECURSIVE_AST_TEST_DEPTH),
            ")".repeat(RECURSIVE_AST_TEST_DEPTH)
        );
        parse_module(&source).unwrap();
    }

    #[test]
    fn nested_subscripts() {
        let source = format!(
            "x = {}1{}",
            "a[".repeat(RECURSIVE_AST_TEST_DEPTH),
            "]".repeat(RECURSIVE_AST_TEST_DEPTH)
        );
        parse_module(&source).unwrap();
    }

    #[test]
    fn binary_paren_interplay() {
        // `1+(1+(1+(1+...)))` — each level alternates a binary operator and a
        // parenthesised sub-expression, exactly like the pattern described in
        // the tracking issue.
        let depth = RECURSIVE_AST_TEST_DEPTH;
        let mut source = String::new();
        for _ in 0..depth {
            source.push_str("1+(");
        }
        source.push('1');
        for _ in 0..depth {
            source.push(')');
        }
        parse_module(&source).unwrap();
    }

    #[test]
    fn ternary_else_chain() {
        // `1 if 1 else 1 if 1 else ...` — the `else` operand recurses at the
        // conditional layer (`parse_if_expression` -> `orelse`).
        let depth = RECURSIVE_AST_TEST_DEPTH;
        let mut source = String::with_capacity(depth * 12 + 1);
        for _ in 0..depth {
            source.push_str("1 if 1 else ");
        }
        source.push('1');
        parse_module(&source).unwrap();
    }
}

#[test]
fn invalid_async_chain_is_iterative() {
    // Invalid repeated `async` prefixes are recovered iteratively.
    let source = format!("{}x = 1\n", "async ".repeat(5_000));
    let parsed = crate::parse_unchecked(&source, ParseOptions::from(Mode::Module));
    assert!(!parsed.errors().is_empty());
}

#[test]
fn nested_equal_precedence_unary_chains_are_iterative() {
    // Consecutive unary operators sharing precedence do not require stack growth.
    let source = format!("{}1\n", "-~+".repeat(RECURSIVE_AST_TEST_DEPTH));
    parse_module(&source).unwrap();

    let source = format!("{}True\n", "not ".repeat(RECURSIVE_AST_TEST_DEPTH));
    parse_module(&source).unwrap();
}
