use ruff_python_ast::Stmt;

use crate::{
    Mode, ParseErrorType, ParseOptions, parse, parse_expression, parse_module, parse_unchecked,
};

#[test]
fn test_modes() {
    let source = "a[0][1][2][3][4]";

    assert!(parse(source, ParseOptions::from(Mode::Expression)).is_ok());
    assert!(parse(source, ParseOptions::from(Mode::Module)).is_ok());
}

#[test]
fn deeply_nested_parens() {
    let source = format!("{}1{}", "(".repeat(1_000), ")".repeat(1_000));

    parse_module(&source).unwrap();
}

#[test]
fn deeply_nested_later_tuple_elements() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("(0, ");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(')');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_starred_tuples() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("(*");
    }
    src.push('1');
    for _ in 0..depth {
        src.push_str(",)");
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_parens_continuations() {
    for source in [
        "((1) + 2)",
        "((1) if True else 2)",
        "((x), y)",
        "((x) for x in y)",
        "((f)(1).x[0])",
    ] {
        parse_expression(source).unwrap();
    }
}

#[test]
fn nested_expression_continuations() {
    for source in [
        "f(g(1), 2)",
        "f(g(1) for _ in xs)",
        "f(g(1) + 2)",
        "a[b[c], d]",
        "a[b[c]:d]",
        "a[b[c] + d]",
        "[[][0]]",
        "[[1] for _ in xs]",
        "{{1}.pop()}",
        "{{1} for _ in xs}",
        "{{**0}}",
        "1 + (2 + (3) * 4)",
        "(1 + (lambda: 2))",
    ] {
        parse_expression(source).unwrap();
    }
}

#[test]
fn moderately_nested_parens() {
    // 50 levels is well above what real-world Python ever produces. The parser should handle it
    // without taking the nested parenthesized-expression slow path into exceptional behavior.
    let src = format!("x = {}1{}", "(".repeat(50), ")".repeat(50));
    parse_module(&src).unwrap();
}

#[test]
fn deeply_nested_def_blocks() {
    let depth = 400;
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&"\t".repeat(i));
        src.push_str("def f():\n");
    }
    src.push_str(&"\t".repeat(depth));
    src.push_str("pass\n");
    parse_module(&src).unwrap();
}

#[test]
fn deeply_nested_if_blocks() {
    let depth = 3_000;
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&"\t".repeat(i));
        src.push_str("if x:\n");
    }
    src.push_str(&"\t".repeat(depth));
    src.push_str("pass\n");

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_else_if_blocks() {
    let depth = 3_000;
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&"\t".repeat(i));
        src.push_str("if x:\n");
        src.push_str(&"\t".repeat(i + 1));
        src.push_str("pass\n");
        src.push_str(&"\t".repeat(i));
        src.push_str("else:\n");
    }
    src.push_str(&"\t".repeat(depth));
    src.push_str("pass\n");

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_compound_statement_blocks() {
    const HEADERS: &[&str] = &[
        "if x:\n",
        "for x in xs:\n",
        "while x:\n",
        "try:\n",
        "def f():\n",
        "class C:\n",
        "with x:\n",
        "async def f():\n",
        "async for x in xs:\n",
        "async with x:\n",
    ];

    let depth = 3_000;
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&"\t".repeat(i));
        src.push_str(HEADERS[i % HEADERS.len()]);
    }
    src.push_str(&"\t".repeat(depth));
    src.push_str("pass\n");

    for i in (0..depth).rev() {
        if HEADERS[i % HEADERS.len()] == "try:\n" {
            src.push_str(&"\t".repeat(i));
            src.push_str("except Exception:\n");
            src.push_str(&"\t".repeat(i + 1));
            src.push_str("pass\n");
        }
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_match_statement_blocks() {
    let depth = 3_000;
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&"\t".repeat(i * 2));
        src.push_str("match x:\n");
        src.push_str(&"\t".repeat(i * 2 + 1));
        src.push_str("case _:\n");
    }
    src.push_str(&"\t".repeat(depth * 2));
    src.push_str("pass\n");

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_decorated_def_blocks() {
    let depth = 3_000;
    let mut src = String::new();
    for i in 0..depth {
        src.push_str(&"\t".repeat(i));
        src.push_str("@decorator\n");
        src.push_str(&"\t".repeat(i));
        src.push_str("def f():\n");
    }
    src.push_str(&"\t".repeat(depth));
    src.push_str("pass\n");

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_lists() {
    let src = format!("{}1{}", "[".repeat(5_000), "]".repeat(5_000));

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_later_lists() {
    let depth = 5_000;
    let mut src = String::new();
    for _ in 0..depth {
        src.push_str("[0, ");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(']');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_list_comprehension_iters() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("[x for x in ");
    }
    src.push_str("xs");
    for _ in 0..depth {
        src.push(']');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_starred_lists() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("[*");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(']');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_sets() {
    let src = format!("{}1{}", "{".repeat(5_000), "}".repeat(5_000));

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_later_sets() {
    let depth = 5_000;
    let mut src = String::new();
    for _ in 0..depth {
        src.push_str("{0, ");
    }
    src.push('1');
    for _ in 0..depth {
        src.push('}');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_set_comprehension_iters() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("{x for x in ");
    }
    src.push_str("xs");
    for _ in 0..depth {
        src.push('}');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_starred_sets() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("{*");
    }
    src.push('1');
    for _ in 0..depth {
        src.push('}');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_dict_values() {
    let depth = 5_000;
    let mut src = String::new();
    for _ in 0..depth {
        src.push_str("{0: ");
    }
    src.push('1');
    for _ in 0..depth {
        src.push('}');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_later_dict_keys() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("{0: 0, ");
    }
    src.push_str("{}");
    for _ in 0..depth {
        src.push_str(": 0}");
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_dict_unpackings() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("{**");
    }
    src.push_str("{}");
    for _ in 0..depth {
        src.push('}');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_later_dict_unpackings() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("{**x, **");
    }
    src.push_str("{}");
    for _ in 0..depth {
        src.push('}');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_dict_comprehension_iters() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("{x: x for x in ");
    }
    src.push_str("xs");
    for _ in 0..depth {
        src.push('}');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_generator_expression_iters() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("(x for x in ");
    }
    src.push_str("xs");
    for _ in 0..depth {
        src.push(')');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_unary() {
    let src = format!("{}1", "+".repeat(20_000));

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_expression(&src).unwrap());
}

#[test]
fn deeply_nested_await_chain() {
    let src = format!("async def f():\n    return {}x\n", "await ".repeat(20_000));

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_unchecked(&src, ParseOptions::from(Mode::Module)));
}

#[test]
fn deeply_nested_yield_chain() {
    let depth = 5_000;
    let mut src = String::from("def f():\n    yield ");
    for _ in 0..depth {
        src.push_str("(yield ");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(')');
    }
    src.push('\n');

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_yield_from_chain() {
    let depth = 5_000;
    let mut src = String::from("def f():\n    yield from ");
    for _ in 0..depth {
        src.push_str("(yield from ");
    }
    src.push('x');
    for _ in 0..depth {
        src.push(')');
    }
    src.push('\n');

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_named_expression_values() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("(y := ");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(')');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_invalid_async_prefixes() {
    let src = format!("{}def f(): pass\n", "async ".repeat(20_000));

    std::mem::forget(parse_unchecked(&src, ParseOptions::from(Mode::Module)));
}

#[test]
fn nested_calls() {
    let src = format!("x = {}1{}", "f(".repeat(1_000), ")".repeat(1_000));
    parse_module(&src).unwrap();
}

#[test]
fn deeply_nested_later_calls() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("f(0, ");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(')');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_keyword_call_values() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("f(x=");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(')');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_keyword_unpacking_call_values() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("f(**");
    }
    src.push_str("{}");
    for _ in 0..depth {
        src.push(')');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_starred_call_values() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("f(*");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(')');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_subscripts() {
    let src = format!("x = {}1{}", "a[".repeat(1_000), "]".repeat(1_000));
    parse_module(&src).unwrap();
}

#[test]
fn deeply_nested_later_subscripts() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("a[0, ");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(']');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_starred_subscripts() {
    let depth = 5_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("a[*");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(']');
    }

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_match_patterns() {
    let mut src = String::from("match x:\n case ");
    for _ in 0..5_000 {
        src.push('(');
    }
    src.push('y');
    for _ in 0..5_000 {
        src.push(')');
    }
    src.push_str(": pass\n");

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_class_patterns() {
    let depth = 5_000;
    let mut src = String::from("match x:\n case ");
    for _ in 0..depth {
        src.push_str("C(");
    }
    src.push('y');
    for _ in 0..depth {
        src.push(')');
    }
    src.push_str(": pass\n");

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_mapping_patterns() {
    let depth = 5_000;
    let mut src = String::from("match x:\n case ");
    for _ in 0..depth {
        src.push_str("{1: ");
    }
    src.push('y');
    for _ in 0..depth {
        src.push('}');
    }
    src.push_str(": pass\n");

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_later_mapping_patterns() {
    let depth = 5_000;
    let mut src = String::from("match x:\n case ");
    for _ in 0..depth {
        src.push_str("{0: _, 1: ");
    }
    src.push('_');
    for _ in 0..depth {
        src.push('}');
    }
    src.push_str(": pass\n");

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_keyword_class_patterns() {
    let depth = 5_000;
    let mut src = String::from("match x:\n case ");
    for _ in 0..depth {
        src.push_str("C(x=");
    }
    src.push('y');
    for _ in 0..depth {
        src.push(')');
    }
    src.push_str(": pass\n");

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_later_class_patterns() {
    let depth = 5_000;
    let mut src = String::from("match x:\n case ");
    for _ in 0..depth {
        src.push_str("C(_, ");
    }
    src.push('y');
    for _ in 0..depth {
        src.push(')');
    }
    src.push_str(": pass\n");

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn deeply_nested_later_sequence_patterns() {
    let depth = 5_000;
    let mut src = String::from("match x:\n case ");
    for _ in 0..depth {
        src.push_str("[_, ");
    }
    src.push('y');
    for _ in 0..depth {
        src.push(']');
    }
    src.push_str(": pass\n");

    // Keep this focused on parser recursion rather than recursive AST destruction.
    std::mem::forget(parse_module(&src).unwrap());
}

#[test]
fn binary_paren_interplay() {
    let depth = 2_000;
    let mut src = String::new();
    for _ in 0..depth {
        src.push_str("1+(");
    }
    src.push('1');
    for _ in 0..depth {
        src.push(')');
    }
    parse_module(&src).unwrap();
}

#[test]
fn deeply_nested_right_assoc_pow_chain() {
    let depth = 2_000;
    let mut src = String::with_capacity(depth * 3 + 1);
    for _ in 0..depth {
        src.push_str("1**");
    }
    src.push('1');
    parse_module(&src).unwrap();
}

#[test]
fn deeply_nested_ternary_else_chain() {
    let depth = 2_000;
    let mut src = String::with_capacity(depth * 12 + 1);
    for _ in 0..depth {
        src.push_str("1 if 1 else ");
    }
    src.push('1');
    parse_module(&src).unwrap();
}

#[test]
fn deeply_nested_lambda_chain() {
    let depth = 2_000;
    let mut src = String::from("x = ");
    for _ in 0..depth {
        src.push_str("lambda: ");
    }
    src.push('1');
    parse_module(&src).unwrap();
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
