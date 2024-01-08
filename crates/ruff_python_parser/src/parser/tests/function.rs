#[cfg(test)]
mod tests {
    use crate::lexer::LexicalErrorType;
    use crate::ParseError;
    use crate::ParseErrorType;
    use ruff_python_ast::Suite;

    fn parse_program(code: &str) -> Result<Suite, ParseError> {
        crate::parse_suite(code)
    }

    #[test]
    fn test_function_no_args_with_ranges() {
        let parse_ast = parse_program("def f(): pass");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_function_pos_args_with_ranges() {
        let parse_ast = parse_program("def f(a, b, c): pass");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_function_no_args() {
        let parse_ast = parse_program("def f(): pass");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_function_pos_args() {
        let parse_ast = parse_program("def f(a, b, c): pass");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_function_pos_args_with_defaults() {
        let parse_ast = parse_program("def f(a, b=20, c=30): pass");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_function_kw_only_args() {
        let parse_ast = parse_program("def f(*, a, b, c): pass");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_function_kw_only_args_with_defaults() {
        let parse_ast = parse_program("def f(*, a, b=20, c=30): pass");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_function_pos_and_kw_only_args() {
        let parse_ast = parse_program("def f(a, b, c, *, d, e, f): pass");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_function_pos_and_kw_only_args_with_defaults() {
        let parse_ast = parse_program("def f(a, b, c, *, d, e=20, f=30): pass");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_function_pos_and_kw_only_args_with_defaults_and_varargs() {
        let parse_ast = parse_program("def f(a, b, c, *args, d, e=20, f=30): pass");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_function_pos_and_kw_only_args_with_defaults_and_varargs_and_kwargs() {
        let parse_ast = parse_program("def f(a, b, c, *args, d, e=20, f=30, **kwargs): pass");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_lambda_no_args() {
        let parse_ast = parse_program("lambda: 1");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_lambda_pos_args() {
        let parse_ast = parse_program("lambda a, b, c: 1");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_lambda_pos_args_with_defaults() {
        let parse_ast = parse_program("lambda a, b=20, c=30: 1");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_lambda_kw_only_args() {
        let parse_ast = parse_program("lambda *, a, b, c: 1");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_lambda_kw_only_args_with_defaults() {
        let parse_ast = parse_program("lambda *, a, b=20, c=30: 1");
        insta::assert_debug_snapshot!(parse_ast);
    }

    #[test]
    fn test_lambda_pos_and_kw_only_args() {
        let parse_ast = parse_program("lambda a, b, c, *, d, e: 0");
        insta::assert_debug_snapshot!(parse_ast);
    }

    fn function_parse_error(src: &str) -> ParseErrorType {
        let parse_ast = parse_program(src);
        parse_ast.map_err(|e| e.error).expect_err("Expected error")
    }

    #[test]
    fn test_duplicates_f1() {
        let error = function_parse_error("def f(a, a): pass");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string()))
        );
    }

    #[test]
    fn test_duplicates_f2() {
        let error = function_parse_error("def f(a, *, a): pass");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string()))
        );
    }

    #[test]
    fn test_duplicates_f3() {
        let error = function_parse_error("def f(a, a=20): pass");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string()))
        );
    }

    #[test]
    fn test_duplicates_f4() {
        let error = function_parse_error("def f(a, *a): pass");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string()))
        );
    }

    #[test]
    fn test_duplicates_f5() {
        let error = function_parse_error("def f(a, *, **a): pass");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string()))
        );
    }

    #[test]
    fn test_duplicates_l1() {
        let error = function_parse_error("lambda a, a: 1");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string()))
        );
    }

    #[test]
    fn test_duplicates_l2() {
        let error = function_parse_error("lambda a, *, a: 1");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string()))
        );
    }

    #[test]
    fn test_duplicates_l3() {
        let error = function_parse_error("lambda a, a=20: 1");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string()))
        );
    }

    #[test]
    fn test_duplicates_l4() {
        let error = function_parse_error("lambda a, *a: 1");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string()))
        );
    }

    #[test]
    fn test_duplicates_l5() {
        let error = function_parse_error("lambda a, *, **a: 1");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string()))
        );
    }

    #[test]
    fn test_default_arg_error_f() {
        let error = function_parse_error("def f(a, b=20, c): pass");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DefaultArgumentError)
        );
    }

    #[test]
    fn test_default_arg_error_l() {
        let error = function_parse_error("lambda a, b=20, c: 1");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DefaultArgumentError)
        );
    }

    #[test]
    fn test_positional_arg_error_f() {
        let error = function_parse_error("f(b=20, c)");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::PositionalArgumentError)
        );
    }

    #[test]
    fn test_unpacked_arg_error_f() {
        let error = function_parse_error("f(**b, *c)");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::UnpackedArgumentError)
        );
    }

    #[test]
    fn test_duplicate_kw_f1() {
        let error = function_parse_error("f(a=20, a=30)");
        assert_eq!(
            error,
            ParseErrorType::Lexical(LexicalErrorType::DuplicateKeywordArgumentError(
                "a".to_string(),
            ))
        );
    }
}
