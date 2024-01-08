#[cfg(test)]
mod tests {
    use crate::lexer::LexicalErrorType;
    use crate::parse_suite;
    use crate::ParseErrorType;

    macro_rules! function_and_lambda {
        ($($name:ident: $code:expr,)*) => {
            $(
                #[test]
                fn $name() {
                    let parse_ast = crate::parse_suite($code);
                    insta::assert_debug_snapshot!(parse_ast);
                }
            )*
        }
    }

    function_and_lambda! {
        test_function_no_args_with_ranges: "def f(): pass",
        test_function_pos_args_with_ranges: "def f(a, b, c): pass",
    }

    function_and_lambda! {
        test_function_no_args: "def f(): pass",
        test_function_pos_args: "def f(a, b, c): pass",
        test_function_pos_args_with_defaults: "def f(a, b=20, c=30): pass",
        test_function_kw_only_args: "def f(*, a, b, c): pass",
        test_function_kw_only_args_with_defaults: "def f(*, a, b=20, c=30): pass",
        test_function_pos_and_kw_only_args: "def f(a, b, c, *, d, e, f): pass",
        test_function_pos_and_kw_only_args_with_defaults: "def f(a, b, c, *, d, e=20, f=30): pass",
        test_function_pos_and_kw_only_args_with_defaults_and_varargs: "def f(a, b, c, *args, d, e=20, f=30): pass",
        test_function_pos_and_kw_only_args_with_defaults_and_varargs_and_kwargs: "def f(a, b, c, *args, d, e=20, f=30, **kwargs): pass",
        test_lambda_no_args: "lambda: 1",
        test_lambda_pos_args: "lambda a, b, c: 1",
        test_lambda_pos_args_with_defaults: "lambda a, b=20, c=30: 1",
        test_lambda_kw_only_args: "lambda *, a, b, c: 1",
        test_lambda_kw_only_args_with_defaults: "lambda *, a, b=20, c=30: 1",
        test_lambda_pos_and_kw_only_args: "lambda a, b, c, *, d, e: 0",
    }

    fn function_parse_error(src: &str) -> ParseErrorType {
        let parse_ast = parse_suite(src);
        parse_ast.map_err(|e| e.error).expect_err("Expected error")
    }

    macro_rules! function_and_lambda_error {
        ($($name:ident: $code:expr, $error:expr,)*) => {
            $(
                #[test]
                fn $name() {
                    let error = function_parse_error($code);
                    assert_eq!(error, $error);
                }
            )*
        }
    }

    function_and_lambda_error! {
        // Check definitions
        test_duplicates_f1: "def f(a, a): pass", ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string())),
        test_duplicates_f2: "def f(a, *, a): pass", ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string())),
        test_duplicates_f3: "def f(a, a=20): pass", ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string())),
        test_duplicates_f4: "def f(a, *a): pass", ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string())),
        test_duplicates_f5: "def f(a, *, **a): pass", ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string())),
        test_duplicates_l1: "lambda a, a: 1", ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string())),
        test_duplicates_l2: "lambda a, *, a: 1", ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string())),
        test_duplicates_l3: "lambda a, a=20: 1", ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string())),
        test_duplicates_l4: "lambda a, *a: 1", ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string())),
        test_duplicates_l5: "lambda a, *, **a: 1", ParseErrorType::Lexical(LexicalErrorType::DuplicateArgumentError("a".to_string())),
        test_default_arg_error_f: "def f(a, b=20, c): pass", ParseErrorType::Lexical(LexicalErrorType::DefaultArgumentError),
        test_default_arg_error_l: "lambda a, b=20, c: 1", ParseErrorType::Lexical(LexicalErrorType::DefaultArgumentError),

        // Check some calls.
        test_positional_arg_error_f: "f(b=20, c)", ParseErrorType::Lexical(LexicalErrorType::PositionalArgumentError),
        test_unpacked_arg_error_f: "f(**b, *c)", ParseErrorType::Lexical(LexicalErrorType::UnpackedArgumentError),
        test_duplicate_kw_f1: "f(a=20, a=30)", ParseErrorType::Lexical(LexicalErrorType::DuplicateKeywordArgumentError("a".to_string())),
    }
}
