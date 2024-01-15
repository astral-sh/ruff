/*!
Defines some helper routines for rejecting invalid Python programs.

These routines are named in a way that supports qualified use. For example,
`invalid::assignment_targets`.
*/

use {ruff_python_ast::Expr, ruff_text_size::TextSize};

use crate::lexer::{LexicalError, LexicalErrorType};

/// Returns an error for invalid assignment targets.
///
/// # Errors
///
/// This returns an error when any of the given expressions are themselves
/// or contain an expression that is invalid on the left hand side of an
/// assignment. For example, all literal expressions are invalid assignment
/// targets.
pub(crate) fn assignment_targets(targets: &[Expr]) -> Result<(), LexicalError> {
    for t in targets {
        assignment_target(t)?;
    }
    Ok(())
}

/// Returns an error if the given target is invalid for the left hand side of
/// an assignment.
///
/// # Errors
///
/// This returns an error when the given expression is itself or contains an
/// expression that is invalid on the left hand side of an assignment. For
/// example, all literal expressions are invalid assignment targets.
pub(crate) fn assignment_target(target: &Expr) -> Result<(), LexicalError> {
    // Allowing a glob import here because of its limited scope.
    #[allow(clippy::enum_glob_use)]
    use self::Expr::*;

    let err = |location: TextSize| -> LexicalError {
        let error = LexicalErrorType::AssignmentError;
        LexicalError { error, location }
    };
    match *target {
        BoolOp(ref e) => Err(err(e.range.start())),
        NamedExpr(ref e) => Err(err(e.range.start())),
        BinOp(ref e) => Err(err(e.range.start())),
        UnaryOp(ref e) => Err(err(e.range.start())),
        Lambda(ref e) => Err(err(e.range.start())),
        IfExp(ref e) => Err(err(e.range.start())),
        Dict(ref e) => Err(err(e.range.start())),
        Set(ref e) => Err(err(e.range.start())),
        ListComp(ref e) => Err(err(e.range.start())),
        SetComp(ref e) => Err(err(e.range.start())),
        DictComp(ref e) => Err(err(e.range.start())),
        GeneratorExp(ref e) => Err(err(e.range.start())),
        Await(ref e) => Err(err(e.range.start())),
        Yield(ref e) => Err(err(e.range.start())),
        YieldFrom(ref e) => Err(err(e.range.start())),
        Compare(ref e) => Err(err(e.range.start())),
        Call(ref e) => Err(err(e.range.start())),
        // FString is recursive, but all its forms are invalid as an
        // assignment target, so we can reject it without exploring it.
        FString(ref e) => Err(err(e.range.start())),
        StringLiteral(ref e) => Err(err(e.range.start())),
        BytesLiteral(ref e) => Err(err(e.range.start())),
        NumberLiteral(ref e) => Err(err(e.range.start())),
        BooleanLiteral(ref e) => Err(err(e.range.start())),
        NoneLiteral(ref e) => Err(err(e.range.start())),
        EllipsisLiteral(ref e) => Err(err(e.range.start())),
        // This isn't in the Python grammar but is Jupyter notebook specific.
        // It seems like this should be an error. It does also seem like the
        // parser prevents this from ever appearing as an assignment target
        // anyway. ---AG
        IpyEscapeCommand(ref e) => Err(err(e.range.start())),
        // The only nested expressions allowed as an assignment target
        // are star exprs, lists and tuples.
        Starred(ref e) => assignment_target(&e.value),
        List(ref e) => assignment_targets(&e.elts),
        Tuple(ref e) => assignment_targets(&e.elts),
        // Subscript is recursive and can be invalid, but aren't syntax errors.
        // For example, `5[1] = 42` is a type error.
        Subscript(_) => Ok(()),
        // Similar to Subscript, e.g., `5[1:2] = [42]` is a type error.
        Slice(_) => Ok(()),
        // Similar to Subscript, e.g., `"foo".y = 42` is an attribute error.
        Attribute(_) => Ok(()),
        // These are always valid as assignment targets.
        Name(_) => Ok(()),
    }
}

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
                offset: 0,
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
                offset: 3,
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
                offset: 0,
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
                offset: 1,
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
                offset: 0,
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
                offset: 1,
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
                offset: 0,
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
                offset: 0,
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
                offset: 1,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 1,
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
                offset: 1,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 0,
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
                offset: 1,
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
                offset: 4,
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
                offset: 11,
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
                offset: 4,
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
        let tokens = crate::lexer::lex(src, Mode::Ipython);
        let ast = crate::parse_tokens(tokens.collect(), src, Mode::Ipython);
        insta::assert_debug_snapshot!(ast);
    }

    #[test]
    fn ok_assignment_expr() {
        let ast = parse_suite(r"(x := 5)");
        insta::assert_debug_snapshot!(ast);
    }
}
