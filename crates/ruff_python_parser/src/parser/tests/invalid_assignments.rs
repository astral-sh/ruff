#[cfg(test)]
mod tests {
    use crate::parse_suite;

    // First we test, broadly, that various kinds of assignments are now
    // rejected by the parser. e.g., `5 = 3`, `5 += 3`, `(5): int = 3`.

    // Regression test: https://github.com/astral-sh/ruff/issues/6895
    #[test]
    fn err_literal_assignment() {
        let ast = parse_suite(r"5 = 3");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..1,
            },
        )
        "###);
    }

    // This test previously passed before the assignment operator checking
    // above, but we include it here for good measure.
    #[test]
    fn err_assignment_expr() {
        let ast = parse_suite(r"(5 := 3)");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: UnrecognizedToken(
                    ColonEqual,
                    None,
                ),
                location: 3..5,
            },
        )
        "###);
    }

    #[test]
    fn err_literal_augment_assignment() {
        let ast = parse_suite(r"5 += 3");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..1,
            },
        )
        "###);
    }

    #[test]
    fn err_literal_annotation_assignment() {
        let ast = parse_suite(r"(5): int = 3");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 1..2,
            },
        )
        "###);
    }

    // Now we exhaustively test all possible cases where assignment can fail.

    #[test]
    fn err_bool_op() {
        let ast = parse_suite(r"x or y = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..6,
            },
        )
        "###);
    }

    #[test]
    fn err_named_expr() {
        let ast = parse_suite(r"(x := 5) = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 1..7,
            },
        )
        "###);
    }

    #[test]
    fn err_bin_op() {
        let ast = parse_suite(r"x + y = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..5,
            },
        )
        "###);
    }

    #[test]
    fn err_unary_op() {
        let ast = parse_suite(r"-x = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..2,
            },
        )
        "###);
    }

    #[test]
    fn err_lambda() {
        let ast = parse_suite(r"(lambda _: 1) = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 1..12,
            },
        )
        "###);
    }

    #[test]
    fn err_if_exp() {
        let ast = parse_suite(r"a if b else c = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..13,
            },
        )
        "###);
    }

    #[test]
    fn err_dict() {
        let ast = parse_suite(r"{'a':5} = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..7,
            },
        )
        "###);
    }

    #[test]
    fn err_set() {
        let ast = parse_suite(r"{a} = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..3,
            },
        )
        "###);
    }

    #[test]
    fn err_list_comp() {
        let ast = parse_suite(r"[x for x in xs] = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..15,
            },
        )
        "###);
    }

    #[test]
    fn err_set_comp() {
        let ast = parse_suite(r"{x for x in xs} = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..15,
            },
        )
        "###);
    }

    #[test]
    fn err_dict_comp() {
        let ast = parse_suite(r"{x: x*2 for x in xs} = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..20,
            },
        )
        "###);
    }

    #[test]
    fn err_generator_exp() {
        let ast = parse_suite(r"(x for x in xs) = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..15,
            },
        )
        "###);
    }

    #[test]
    fn err_await() {
        let ast = parse_suite(r"await x = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..7,
            },
        )
        "###);
    }

    #[test]
    fn err_yield() {
        let ast = parse_suite(r"(yield x) = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 1..8,
            },
        )
        "###);
    }

    #[test]
    fn err_yield_from() {
        let ast = parse_suite(r"(yield from xs) = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 1..14,
            },
        )
        "###);
    }

    #[test]
    fn err_compare() {
        let ast = parse_suite(r"a < b < c = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..9,
            },
        )
        "###);
    }

    #[test]
    fn err_call() {
        let ast = parse_suite(r"foo() = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..5,
            },
        )
        "###);
    }

    #[test]
    fn err_formatted_value() {
        // N.B. It looks like the parser can't generate a top-level
        // FormattedValue, where as the official Python AST permits
        // representing a single f-string containing just a variable as a
        // FormattedValue directly.
        //
        // Bottom line is that because of this, this test is (at present)
        // duplicative with the `fstring` test. That is, in theory these tests
        // could fail independently, but in practice their failure or success
        // is coupled.
        //
        // See: https://docs.python.org/3/library/ast.html#ast.FormattedValue
        let ast = parse_suite(r#"f"{quux}" = 42"#);
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..9,
            },
        )
        "###);
    }

    #[test]
    fn err_fstring() {
        let ast = parse_suite(r#"f"{foo} and {bar}" = 42"#);
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..18,
            },
        )
        "###);
    }

    #[test]
    fn err_string_literal() {
        let ast = parse_suite(r#""foo" = 42"#);
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..5,
            },
        )
        "###);
    }

    #[test]
    fn err_bytes_literal() {
        let ast = parse_suite(r#"b"foo" = 42"#);
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..6,
            },
        )
        "###);
    }

    #[test]
    fn err_number_literal() {
        let ast = parse_suite(r"123 = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..3,
            },
        )
        "###);
    }

    #[test]
    fn err_boolean_literal() {
        let ast = parse_suite(r"True = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..4,
            },
        )
        "###);
    }

    #[test]
    fn err_none_literal() {
        let ast = parse_suite(r"None = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..4,
            },
        )
        "###);
    }

    #[test]
    fn err_ellipsis_literal() {
        let ast = parse_suite(r"... = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 0..3,
            },
        )
        "###);
    }

    #[test]
    fn err_starred() {
        let ast = parse_suite(r"*foo() = 42");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 1..6,
            },
        )
        "###);
    }

    #[test]
    fn err_list() {
        let ast = parse_suite(r"[x, foo(), y] = [42, 42, 42]");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 4..9,
            },
        )
        "###);
    }

    #[test]
    fn err_list_nested() {
        let ast = parse_suite(r"[[a, b], [[42]], d] = [[1, 2], [[3]], 4]");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 11..13,
            },
        )
        "###);
    }

    #[test]
    fn err_tuple() {
        let ast = parse_suite(r"(x, foo(), y) = (42, 42, 42)");
        insta::assert_debug_snapshot!(ast, @r###"
        Err(
            ParseError {
                error: Lexical(
                    AssignmentError,
                ),
                location: 4..9,
            },
        )
        "###);
    }

    // This last group of tests checks that assignments we expect to be parsed
    // (including some interesting ones) continue to be parsed successfully.

    #[test]
    fn ok_starred() {
        let ast = parse_suite(r"*foo = 42");
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn ok_list() {
        let ast = parse_suite(r"[x, y, z] = [1, 2, 3]");
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn ok_tuple() {
        let ast = parse_suite(r"(x, y, z) = (1, 2, 3)");
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn ok_subscript_normal() {
        let ast = parse_suite(r"x[0] = 42");
        insta::assert_debug_snapshot!(ast);
    }

    // This is actually a type error, not a syntax error. So check that it
    // doesn't fail parsing.
    #[test]
    fn ok_subscript_weird() {
        let ast = parse_suite(r"5[0] = 42");
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn ok_slice_normal() {
        let ast = parse_suite(r"x[1:2] = [42]");
        insta::assert_debug_snapshot!(ast);
    }

    // This is actually a type error, not a syntax error. So check that it
    // doesn't fail parsing.
    #[test]
    fn ok_slice_weird() {
        let ast = parse_suite(r"5[1:2] = [42]");
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn ok_attribute_normal() {
        let ast = parse_suite(r"foo.bar = 42");
        insta::assert_debug_snapshot!(ast);
    }

    // This is actually an attribute error, not a syntax error. So check that
    // it doesn't fail parsing.
    #[test]
    fn ok_attribute_weird() {
        let ast = parse_suite(r#""foo".y = 42"#);
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn ok_name() {
        let ast = parse_suite(r"foo = 42");
        insta::assert_debug_snapshot!(ast);
    }

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

    #[test]
    fn ok_assignment_expr() {
        let ast = parse_suite(r"(x := 5)");
        insta::assert_debug_snapshot!(ast);
    }
}
