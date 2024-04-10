use std::hash::BuildHasherDefault;
// Contains functions that perform validation and parsing of arguments and parameters.
// Checks apply both to functions and to lambdas.
use crate::lexer::{LexicalError, LexicalErrorType};
use ruff_python_ast::{self as ast};
use ruff_text_size::{Ranged, TextRange, TextSize};
use rustc_hash::FxHashSet;

pub(crate) struct ArgumentList {
    pub(crate) args: Vec<ast::Expr>,
    pub(crate) keywords: Vec<ast::Keyword>,
}

// Perform validation of function/lambda arguments in a function definition.
pub(crate) fn validate_arguments(arguments: &ast::Parameters) -> Result<(), LexicalError> {
    let mut all_arg_names = FxHashSet::with_capacity_and_hasher(
        arguments.posonlyargs.len()
            + arguments.args.len()
            + usize::from(arguments.vararg.is_some())
            + arguments.kwonlyargs.len()
            + usize::from(arguments.kwarg.is_some()),
        BuildHasherDefault::default(),
    );

    let posonlyargs = arguments.posonlyargs.iter();
    let args = arguments.args.iter();
    let kwonlyargs = arguments.kwonlyargs.iter();

    let vararg: Option<&ast::Parameter> = arguments.vararg.as_deref();
    let kwarg: Option<&ast::Parameter> = arguments.kwarg.as_deref();

    for arg in posonlyargs
        .chain(args)
        .chain(kwonlyargs)
        .map(|arg| &arg.parameter)
        .chain(vararg)
        .chain(kwarg)
    {
        let range = arg.range;
        let arg_name = arg.name.as_str();
        if !all_arg_names.insert(arg_name) {
            return Err(LexicalError::new(
                LexicalErrorType::DuplicateArgumentError(arg_name.to_string().into_boxed_str()),
                range.start(),
            ));
        }
    }

    Ok(())
}

pub(crate) fn validate_pos_params(
    args: &(
        Vec<ast::ParameterWithDefault>,
        Vec<ast::ParameterWithDefault>,
    ),
) -> Result<(), LexicalError> {
    let (posonlyargs, args) = args;
    #[allow(clippy::skip_while_next)]
    let first_invalid = posonlyargs
        .iter()
        .chain(args.iter()) // for all args
        .skip_while(|arg| arg.default.is_none()) // starting with args without default
        .skip_while(|arg| arg.default.is_some()) // and then args with default
        .next(); // there must not be any more args without default
    if let Some(invalid) = first_invalid {
        return Err(LexicalError::new(
            LexicalErrorType::DefaultArgumentError,
            invalid.parameter.start(),
        ));
    }
    Ok(())
}

type FunctionArgument = (
    Option<(TextSize, TextSize, Option<ast::Identifier>)>,
    ast::Expr,
);

// Parse arguments as supplied during a function/lambda *call*.
pub(crate) fn parse_arguments(
    function_arguments: Vec<FunctionArgument>,
) -> Result<ArgumentList, LexicalError> {
    // First, run through the comments to determine the number of positional and keyword arguments.
    let mut keyword_names = FxHashSet::with_capacity_and_hasher(
        function_arguments.len(),
        BuildHasherDefault::default(),
    );
    let mut double_starred = false;
    let mut num_args = 0;
    let mut num_keywords = 0;
    for (name, value) in &function_arguments {
        if let Some((start, _end, name)) = name {
            // Check for duplicate keyword arguments in the call.
            if let Some(keyword_name) = &name {
                if !keyword_names.insert(keyword_name.to_string()) {
                    return Err(LexicalError::new(
                        LexicalErrorType::DuplicateKeywordArgumentError(
                            keyword_name.to_string().into_boxed_str(),
                        ),
                        *start,
                    ));
                }
            } else {
                double_starred = true;
            }

            num_keywords += 1;
        } else {
            // Positional arguments mustn't follow keyword arguments.
            if num_keywords > 0 && !is_starred(value) {
                return Err(LexicalError::new(
                    LexicalErrorType::PositionalArgumentError,
                    value.start(),
                ));
                // Allow starred arguments after keyword arguments but
                // not after double-starred arguments.
            } else if double_starred {
                return Err(LexicalError::new(
                    LexicalErrorType::UnpackedArgumentError,
                    value.start(),
                ));
            }

            num_args += 1;
        }
    }

    // Second, push the arguments into vectors of exact capacity. This avoids a vector resize later
    // on when these vectors are boxed into slices.
    let mut args = Vec::with_capacity(num_args);
    let mut keywords = Vec::with_capacity(num_keywords);
    for (name, value) in function_arguments {
        if let Some((start, end, name)) = name {
            keywords.push(ast::Keyword {
                arg: name,
                value,
                range: TextRange::new(start, end),
            });
        } else {
            args.push(value);
        }
    }

    Ok(ArgumentList { args, keywords })
}

// Check if an expression is a starred expression.
const fn is_starred(exp: &ast::Expr) -> bool {
    exp.is_starred_expr()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_suite;
    use crate::ParseErrorType;

    macro_rules! function_and_lambda {
        ($($name:ident: $code:expr,)*) => {
            $(
                #[test]
                fn $name() {
                    let parse_ast = crate::parser::parse_suite($code, );
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
        test_function_posonly_and_pos_args: "def f(a, /, b, c): pass",
        test_function_pos_args_with_defaults: "def f(a, b=20, /, c=30): pass",
        test_function_pos_args_with_defaults_and_varargs_and_kwargs: "def f(a, b=20, /, c=30, *args, **kwargs): pass",
        test_function_kw_only_args: "def f(*, a, b, c): pass",
        test_function_kw_only_args_with_defaults: "def f(*, a, b=20, c=30): pass",
        test_function_kw_only_args_with_defaults_and_varargs: "def f(*args, a, b=20, c=30): pass",
        test_function_kw_only_args_with_defaults_and_kwargs: "def f(*, a, b=20, c=30, **kwargs): pass",
        test_function_kw_only_args_with_defaults_and_varargs_and_kwargs: "def f(*args, a, b=20, c=30, **kwargs): pass",
        test_function_pos_and_kw_only_args: "def f(a, b, /, c, *, d, e, f): pass",
        test_function_pos_and_kw_only_args_with_defaults: "def f(a, b, /, c, *, d, e=20, f=30): pass",
        test_function_pos_and_kw_only_args_with_defaults_and_varargs: "def f(a, b, /, c, *args, d, e=20, f=30): pass",
        test_function_pos_and_kw_only_args_with_defaults_and_kwargs: "def f(a, b, /, c, *, d, e=20, f=30, **kwargs): pass",
        test_function_pos_and_kw_only_args_with_defaults_and_varargs_and_kwargs: "def f(a, b, /, c, *args, d, e=20, f=30, **kwargs): pass",
        test_lambda_no_args: "lambda: 1",
        test_lambda_pos_args: "lambda a, b, c: 1",
        test_lambda_posonly_args: "lambda a, b, /, c: 1",
        test_lambda_pos_args_with_defaults: "lambda a, b=20, /, c=30: 1",
        test_lambda_kw_only_args: "lambda *, a, b, c: 1",
        test_lambda_kw_only_args_with_defaults: "lambda *, a, b=20, c=30: 1",
        test_lambda_pos_and_kw_only_args: "lambda a, b, /, c, *, d, e: 0",
        test_lambda_pos_and_kw_only_args_and_vararg_and_kwarg: "lambda a, b, /, c, *d, e, **f: 0",
    }

    fn function_parse_error(src: &str) -> LexicalErrorType {
        let parse_ast = parse_suite(src);
        parse_ast
            .map_err(|e| match e.error {
                ParseErrorType::Lexical(e) => e,
                _ => panic!("Expected LexicalError"),
            })
            .expect_err("Expected error")
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
        test_duplicates_f1: "def f(a, a): pass", LexicalErrorType::DuplicateArgumentError("a".to_string().into_boxed_str()),
        test_duplicates_f2: "def f(a, *, a): pass", LexicalErrorType::DuplicateArgumentError("a".to_string().into_boxed_str()),
        test_duplicates_f3: "def f(a, a=20): pass", LexicalErrorType::DuplicateArgumentError("a".to_string().into_boxed_str()),
        test_duplicates_f4: "def f(a, *a): pass", LexicalErrorType::DuplicateArgumentError("a".to_string().into_boxed_str()),
        test_duplicates_f5: "def f(a, *, b, **a): pass", LexicalErrorType::DuplicateArgumentError("a".to_string().into_boxed_str()),
        test_duplicates_l1: "lambda a, a: 1", LexicalErrorType::DuplicateArgumentError("a".to_string().into_boxed_str()),
        test_duplicates_l2: "lambda a, *, a: 1", LexicalErrorType::DuplicateArgumentError("a".to_string().into_boxed_str()),
        test_duplicates_l3: "lambda a, a=20: 1", LexicalErrorType::DuplicateArgumentError("a".to_string().into_boxed_str()),
        test_duplicates_l4: "lambda a, *a: 1", LexicalErrorType::DuplicateArgumentError("a".to_string().into_boxed_str()),
        test_duplicates_l5: "lambda a, *, b, **a: 1", LexicalErrorType::DuplicateArgumentError("a".to_string().into_boxed_str()),
        test_default_arg_error_f: "def f(a, b=20, c): pass", LexicalErrorType::DefaultArgumentError,
        test_default_arg_error_l: "lambda a, b=20, c: 1", LexicalErrorType::DefaultArgumentError,
        test_named_arguments_follow_bare_star_1: "def f(*): pass", LexicalErrorType::OtherError("named arguments must follow bare *".to_string().into_boxed_str()),
        test_named_arguments_follow_bare_star_2: "def f(*, **kwargs): pass", LexicalErrorType::OtherError("named arguments must follow bare *".to_string().into_boxed_str()),

        // Check some calls.
        test_positional_arg_error_f: "f(b=20, c)", LexicalErrorType::PositionalArgumentError,
        test_unpacked_arg_error_f: "f(**b, *c)", LexicalErrorType::UnpackedArgumentError,
        test_duplicate_kw_f1: "f(a=20, a=30)", LexicalErrorType::DuplicateKeywordArgumentError("a".to_string().into_boxed_str()),
    }
}
