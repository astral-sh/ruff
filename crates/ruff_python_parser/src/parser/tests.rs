use ruff_python_ast::name::Name;
use ruff_python_ast::{Expr, InterpolatedStringElement, IpyEscapeKind, Number, Stmt};

use super::NameInterner;
use crate::{Mode, ParseErrorType, ParseOptions, parse, parse_expression, parse_module};

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
fn nfkc_normalizes_names() {
    let parsed = parse_expression("𝒞").unwrap();
    let Expr::Name(name) = parsed.expr() else {
        panic!("expected name expression, got {:?}", parsed.expr());
    };

    assert_eq!(name.id.as_str(), "C");
}

#[test]
fn repeated_long_names_share_storage() {
    let long_name = "identifier_longer_than_inline_capacity";
    let source = format!("{long_name} = {long_name}");
    let parsed = parse_module(&source).unwrap();
    let [Stmt::Assign(assign)] = parsed.suite().as_slice() else {
        panic!("expected an assignment");
    };
    let [Expr::Name(target)] = assign.targets.as_slice() else {
        panic!("expected a name target");
    };
    let Expr::Name(value) = assign.value.as_ref() else {
        panic!("expected a name value");
    };

    assert!(std::ptr::eq(target.id.as_str(), value.id.as_str()));
}

#[test]
fn only_heap_allocated_names_are_interned() {
    let mut interner = NameInterner::default();
    interner.intern(&"x".repeat(Name::INLINE_CAPACITY));
    assert!(interner.names.is_empty());

    interner.intern(&"x".repeat(Name::INLINE_CAPACITY + 1));
    assert_eq!(interner.names.len(), 1);
}

#[test]
fn normalized_long_names_share_storage() {
    let normalized = "C".repeat(Name::INLINE_CAPACITY + 1);
    let source = format!("{} = {normalized}", "𝒞".repeat(Name::INLINE_CAPACITY + 1));
    let parsed = parse_module(&source).unwrap();
    let [Stmt::Assign(assign)] = parsed.suite().as_slice() else {
        panic!("expected an assignment");
    };
    let [Expr::Name(target)] = assign.targets.as_slice() else {
        panic!("expected a name target");
    };
    let Expr::Name(value) = assign.value.as_ref() else {
        panic!("expected a name value");
    };

    assert_eq!(target.id.as_str(), normalized);
    assert!(std::ptr::eq(target.id.as_str(), value.id.as_str()));
}

#[test]
fn number_values() {
    let cases = [
        ("1E400", Number::Float(f64::INFINITY)),
        (
            "1E400J",
            Number::Complex {
                real: 0.0,
                imag: f64::INFINITY,
            },
        ),
        (
            "123_456_789_123_456_789_123_456_789_123_456_789",
            Number::Int("123456789123456789123456789123456789".parse().unwrap()),
        ),
        (
            "000_123_456_789_123_456_789_123_456_789_123_456_789J",
            Number::Complex {
                real: 0.0,
                imag: 1.234_567_891_234_567_8e35,
            },
        ),
    ];

    for (source, expected) in cases {
        let parsed = parse_expression(source).unwrap();
        let Expr::NumberLiteral(number) = parsed.expr() else {
            panic!(
                "expected number expression for {source:?}, got {:?}",
                parsed.expr()
            );
        };

        assert_eq!(number.value, expected, "source: {source:?}");
    }
}

#[test]
fn malformed_radix_literals() {
    for source in ["0x", "0o", "0b", "0x_", "0x__1"] {
        assert!(parse_expression(source).is_err(), "source: {source:?}");
    }
}

#[test]
fn interpolated_string_escaped_brace_values() {
    let cases = [
        (r"f'\{{1}}'", r"\{1}"),
        (r"f'\}}'", r"\}"),
        (r"f'\\{{1}}'", r"\{1}"),
        (r"f'\\\{{1}}'", r"\\{1}"),
        (r"t'\{{1}}'", r"\{1}"),
        (r"t'\}}'", r"\}"),
        (r"t'\\{{1}}'", r"\{1}"),
        (r"t'\\\{{1}}'", r"\\{1}"),
        (r"rf'\{{1}}'", r"\{1}"),
        (r"rt'\{{1}}'", r"\{1}"),
    ];

    for (source, expected) in cases {
        let parsed = parse_expression(source).unwrap();
        let elements = match parsed.expr() {
            Expr::FString(string) => &string.as_single_part_fstring().unwrap().elements,
            Expr::TString(string) => &string.as_single_part_tstring().unwrap().elements,
            expression => panic!("expected interpolated string for {source:?}, got {expression:?}"),
        };
        let [InterpolatedStringElement::Literal(literal)] = &**elements else {
            panic!("expected one literal element for {source:?}");
        };

        assert_eq!(&*literal.value, expected, "source: {source:?}");
    }
}

#[test]
fn ipython_escape_command_values() {
    let cases = [
        ("?foo?", IpyEscapeKind::Help, "foo"),
        ("??   foo?", IpyEscapeKind::Help, "foo"),
        ("??   foo  ?", IpyEscapeKind::Help2, "   foo  ?"),
        ("?foo??", IpyEscapeKind::Help2, "foo"),
        ("%foo?", IpyEscapeKind::Help, "%foo"),
        ("%foo??", IpyEscapeKind::Help2, "%foo"),
        ("%%foo???", IpyEscapeKind::Magic2, "foo???"),
        ("!pwd?", IpyEscapeKind::Shell, "pwd?"),
        ("?? \\\n    foo?", IpyEscapeKind::Help, "foo"),
        ("?? \\\r    foo?", IpyEscapeKind::Help, "foo"),
        ("?? \\\r\n    foo?", IpyEscapeKind::Help, "foo"),
    ];

    for (source, expected_kind, expected_value) in cases {
        let suite = parse(source, ParseOptions::from(Mode::Ipython))
            .unwrap()
            .try_into_module()
            .unwrap()
            .into_suite();
        let [Stmt::IpyEscapeCommand(command)] = suite.as_slice() else {
            panic!("expected one IPython escape command for {source:?}, got {suite:?}");
        };

        assert_eq!(command.kind, expected_kind, "source: {source:?}");
        assert_eq!(&*command.value, expected_value, "source: {source:?}");
    }
}

#[test]
fn ipython_escape_command_expression_values() {
    let cases = [
        ("x = !!foo", IpyEscapeKind::Shell, "!foo"),
        ("x = %%foo", IpyEscapeKind::Magic, "%foo"),
    ];

    for (source, expected_kind, expected_value) in cases {
        let suite = parse(source, ParseOptions::from(Mode::Ipython))
            .unwrap()
            .try_into_module()
            .unwrap()
            .into_suite();
        let [Stmt::Assign(assign)] = suite.as_slice() else {
            panic!("expected one assignment for {source:?}, got {suite:?}");
        };
        let Expr::IpyEscapeCommand(command) = assign.value.as_ref() else {
            panic!(
                "expected an IPython escape command for {source:?}, got {:?}",
                assign.value
            );
        };

        assert_eq!(command.kind, expected_kind, "source: {source:?}");
        assert_eq!(&*command.value, expected_value, "source: {source:?}");
    }
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

#[test]
fn recursion_limit_nested_parens() {
    let src = format!("{}1{}", "(".repeat(1_000), ")".repeat(1_000));
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(100);
    let err = parse(&src, opts).unwrap_err();
    assert!(matches!(err.error, ParseErrorType::RecursionLimitExceeded));
}

#[test]
fn recursion_limit_normal_python_unaffected() {
    // 50 levels is well above what real-world Python ever produces and well
    // below the default cap — the point is to confirm the default doesn't
    // reject ordinary input.
    let src = format!("x = {}1{}", "(".repeat(50), ")".repeat(50));
    parse_module(&src).unwrap();
}

#[test]
fn recursion_limit_preserves_prior_statements() {
    // Recursion-limit recovery is limited for now: we drain the rest of the file but keep the
    // statements parsed before the overflowing statement.
    // TODO: Recover at the next newline so the trailing statement is preserved too.
    let src = format!(
        "before = 1\n{}1{}\nafter = 2\n",
        "(".repeat(1_000),
        ")".repeat(1_000),
    );
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(100);
    let parsed = crate::parse_unchecked(&src, opts)
        .try_into_module()
        .unwrap();

    assert!(matches!(
        parsed.errors().first().map(|error| &error.error),
        Some(ParseErrorType::RecursionLimitExceeded)
    ));
    assert!(matches!(parsed.suite().first(), Some(Stmt::Assign(_))));
}

#[test]
fn recursion_limit_nested_def_blocks() {
    // Nested function definitions exercise instrumentation on
    // `parse_statement` rather than `parse_lhs_expression`. Each level
    // needs one more leading tab to make indentation valid.
    let depth = 400;
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&"\t".repeat(i));
        src.push_str("def f():\n");
    }
    src.push_str(&"\t".repeat(depth));
    src.push_str("pass\n");
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(100);
    let err = parse(&src, opts).unwrap_err();
    assert!(matches!(err.error, ParseErrorType::RecursionLimitExceeded));
}

#[test]
fn recursion_limit_nested_lists() {
    let src = format!("{}1{}", "[".repeat(1_000), "]".repeat(1_000));
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(100);
    let err = parse(&src, opts).unwrap_err();
    assert!(matches!(err.error, ParseErrorType::RecursionLimitExceeded));
}

#[test]
fn recursion_limit_nested_calls() {
    let src = format!("x = {}1{}", "f(".repeat(1_000), ")".repeat(1_000));
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(100);
    let err = parse(&src, opts).unwrap_err();
    assert!(matches!(err.error, ParseErrorType::RecursionLimitExceeded));
}

#[test]
fn recursion_limit_nested_subscripts() {
    let src = format!("x = {}1{}", "a[".repeat(1_000), "]".repeat(1_000));
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(100);
    let err = parse(&src, opts).unwrap_err();
    assert!(matches!(err.error, ParseErrorType::RecursionLimitExceeded));
}

#[test]
fn recursion_limit_nested_match_patterns() {
    // Deeply parenthesised match patterns — exercises pattern-parsing
    // instrumentation in addition to statement / expression paths.
    let mut src = String::from("match x:\n case ");
    for _ in 0..600 {
        src.push('(');
    }
    src.push('y');
    for _ in 0..600 {
        src.push(')');
    }
    src.push_str(": pass\n");
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(100);
    let err = parse(&src, opts).unwrap_err();
    assert!(matches!(err.error, ParseErrorType::RecursionLimitExceeded));
}

#[test]
fn recursion_limit_binary_paren_interplay() {
    // `1+(1+(1+(1+...)))` — each level alternates a binary operator and a
    // parenthesised sub-expression, exactly like the pattern described in
    // the tracking issue.
    let depth = 2_000;
    let mut src = String::new();
    for _ in 0..depth {
        src.push_str("1+(");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(')');
    }
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(100);
    let err = parse(&src, opts).unwrap_err();
    assert!(matches!(err.error, ParseErrorType::RecursionLimitExceeded));
}

#[test]
fn recursion_limit_first_error_is_recursion_not_noise() {
    // When the limit is hit the outer parser frames will emit secondary
    // errors as they unwind. Callers read the first error via `into_result`
    // / `Parsed::errors()`, so `RecursionLimitExceeded` must come first, and
    // the drain-to-EOF after reporting the recursion limit should keep the total count
    // small rather than producing one noisy error per unwound frame.
    let src = format!("{}1{}", "(".repeat(2_000), ")".repeat(2_000));
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(50);
    let parsed = crate::parse_unchecked(&src, opts);
    let errors = parsed.errors();
    let first = errors.first().expect("expected at least one error");
    assert!(matches!(
        first.error,
        ParseErrorType::RecursionLimitExceeded
    ));
    // Exactly one `RecursionLimitExceeded` — guards against a regression
    // where the unwind loops and re-triggers the limit check.
    let recursion_errors = errors
        .iter()
        .filter(|e| matches!(e.error, ParseErrorType::RecursionLimitExceeded))
        .count();
    assert_eq!(recursion_errors, 1);
    // Small, bounded tail of follow-up errors from the unwinding frames.
    // Today this is 0; the generous cap is a regression gate, not a spec.
    assert!(
        errors.len() <= 8,
        "expected a small number of errors, got {}: {errors:?}",
        errors.len(),
    );
}

#[test]
fn recursion_limit_default_set() {
    let opts = ParseOptions::from(Mode::Module);
    // Guards against someone accidentally unsetting the default. Real-world
    // Python never approaches this depth, and the value must stay within the
    // threading stack's capacity — see the const's docs in `options.rs`.
    assert!(opts.max_recursion_depth() >= 200);
    assert!(opts.max_recursion_depth() <= 2000);
}

#[test]
fn recursion_limit_right_assoc_pow_chain() {
    // `1**1**1**...**1` — `**` is right-associative, so the right operand
    // is parsed by a recursive `parse_binary_expression_or_higher` call
    // *without* any intervening parentheses or atom nesting. This exercises
    // the binary-expression recursion path directly, unlike the
    // `1+(1+(...))` interplay test which recurses through parenthesised
    // atoms.
    let depth = 2_000;
    let mut src = String::with_capacity(depth * 3 + 1);
    for _ in 0..depth {
        src.push_str("1**");
    }
    src.push('1');
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(100);
    let err = parse(&src, opts).unwrap_err();
    assert!(
        matches!(err.error, ParseErrorType::RecursionLimitExceeded),
        "expected RecursionLimitExceeded, got {:?}",
        err.error
    );
}

#[test]
fn recursion_limit_ternary_else_chain() {
    // `1 if 1 else 1 if 1 else ...` — the `else` operand recurses at the
    // conditional layer (`parse_if_expression` -> `orelse`), which is not
    // covered by the `parse_lhs_expression` guard.
    let depth = 2_000;
    let mut src = String::with_capacity(depth * 12 + 1);
    for _ in 0..depth {
        src.push_str("1 if 1 else ");
    }
    src.push('1');
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(100);
    let err = parse(&src, opts).unwrap_err();
    assert!(
        matches!(err.error, ParseErrorType::RecursionLimitExceeded),
        "expected RecursionLimitExceeded, got {:?}",
        err.error
    );
}

#[test]
fn recursion_limit_nested_lambda_chain() {
    // `lambda: lambda: lambda: ...` — the lambda body recurses at the
    // conditional layer (`parse_lambda_expr` -> body), bypassing the
    // `parse_lhs_expression` guard entirely.
    let depth = 2_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("lambda: ");
    }
    src.push('1');
    let opts = ParseOptions::from(Mode::Module).with_max_recursion_depth(100);
    let err = parse(&src, opts).unwrap_err();
    assert!(
        matches!(err.error, ParseErrorType::RecursionLimitExceeded),
        "expected RecursionLimitExceeded, got {:?}",
        err.error
    );
}
