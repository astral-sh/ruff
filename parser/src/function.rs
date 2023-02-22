// Contains functions that perform validation and parsing of arguments and parameters.
// Checks apply both to functions and to lambdas.
use crate::{
    ast,
    lexer::{LexicalError, LexicalErrorType},
};
use rustc_hash::FxHashSet;

pub(crate) struct ArgumentList {
    pub args: Vec<ast::Expr>,
    pub keywords: Vec<ast::Keyword>,
}

type ParameterDefs = (Vec<ast::Arg>, Vec<ast::Arg>, Vec<ast::Expr>);
type ParameterDef = (ast::Arg, Option<ast::Expr>);

// Perform validation of function/lambda arguments in a function definition.
pub(crate) fn validate_arguments(
    arguments: ast::Arguments,
) -> Result<ast::Arguments, LexicalError> {
    let mut all_args: Vec<&ast::Located<ast::ArgData>> = vec![];

    all_args.extend(arguments.posonlyargs.iter());
    all_args.extend(arguments.args.iter());

    if let Some(a) = &arguments.vararg {
        all_args.push(a);
    }

    all_args.extend(arguments.kwonlyargs.iter());

    if let Some(a) = &arguments.kwarg {
        all_args.push(a);
    }

    let mut all_arg_names = FxHashSet::with_hasher(Default::default());
    for arg in all_args {
        let arg_name = &arg.node.arg;
        // Check for duplicate arguments in the function definition.
        if !all_arg_names.insert(arg_name) {
            return Err(LexicalError {
                error: LexicalErrorType::DuplicateArgumentError(arg_name.to_string()),
                location: arg.location,
            });
        }
    }

    Ok(arguments)
}

// Parse parameters as supplied during a function/lambda *definition*.
pub(crate) fn parse_params(
    params: (Vec<ParameterDef>, Vec<ParameterDef>),
) -> Result<ParameterDefs, LexicalError> {
    let mut pos_only = Vec::with_capacity(params.0.len());
    let mut names = Vec::with_capacity(params.1.len());
    let mut defaults = vec![];

    let mut try_default = |name: &ast::Arg, default| {
        if let Some(default) = default {
            defaults.push(default);
        } else if !defaults.is_empty() {
            // Once we have started with defaults, all remaining arguments must
            // have defaults.
            return Err(LexicalError {
                error: LexicalErrorType::DefaultArgumentError,
                location: name.location,
            });
        }
        Ok(())
    };

    for (name, default) in params.0 {
        try_default(&name, default)?;
        pos_only.push(name);
    }

    for (name, default) in params.1 {
        try_default(&name, default)?;
        names.push(name);
    }

    Ok((pos_only, names, defaults))
}

type FunctionArgument = (
    Option<(ast::Location, ast::Location, Option<String>)>,
    ast::Expr,
);

// Parse arguments as supplied during a function/lambda *call*.
pub(crate) fn parse_args(func_args: Vec<FunctionArgument>) -> Result<ArgumentList, LexicalError> {
    let mut args = vec![];
    let mut keywords = vec![];

    let mut keyword_names =
        FxHashSet::with_capacity_and_hasher(func_args.len(), Default::default());
    let mut double_starred = false;
    for (name, value) in func_args {
        match name {
            Some((start, end, name)) => {
                // Check for duplicate keyword arguments in the call.
                if let Some(keyword_name) = &name {
                    if keyword_names.contains(keyword_name) {
                        return Err(LexicalError {
                            error: LexicalErrorType::DuplicateKeywordArgumentError(
                                keyword_name.to_string(),
                            ),
                            location: start,
                        });
                    }

                    keyword_names.insert(keyword_name.clone());
                } else {
                    double_starred = true;
                }

                keywords.push(ast::Keyword::new(
                    start,
                    end,
                    ast::KeywordData { arg: name, value },
                ));
            }
            None => {
                // Positional arguments mustn't follow keyword arguments.
                if !keywords.is_empty() && !is_starred(&value) {
                    return Err(LexicalError {
                        error: LexicalErrorType::PositionalArgumentError,
                        location: value.location,
                    });
                // Allow starred arguments after keyword arguments but
                // not after double-starred arguments.
                } else if double_starred {
                    return Err(LexicalError {
                        error: LexicalErrorType::UnpackedArgumentError,
                        location: value.location,
                    });
                }

                args.push(value);
            }
        }
    }
    Ok(ArgumentList { args, keywords })
}

// Check if an expression is a starred expression.
fn is_starred(exp: &ast::Expr) -> bool {
    matches!(exp.node, ast::ExprKind::Starred { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_program, ParseErrorType};

    macro_rules! function_and_lambda {
        ($($name:ident: $code:expr,)*) => {
            $(
                #[test]
                fn $name() {
                    let parse_ast = parse_program($code, "<test>");
                    insta::assert_debug_snapshot!(parse_ast);
                }
            )*
        }
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

    fn function_parse_error(src: &str) -> LexicalErrorType {
        let parse_ast = parse_program(src, "<test>");
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
        test_duplicates_f1: "def f(a, a): pass", LexicalErrorType::DuplicateArgumentError("a".to_string()),
        test_duplicates_f2: "def f(a, *, a): pass", LexicalErrorType::DuplicateArgumentError("a".to_string()),
        test_duplicates_f3: "def f(a, a=20): pass", LexicalErrorType::DuplicateArgumentError("a".to_string()),
        test_duplicates_f4: "def f(a, *a): pass", LexicalErrorType::DuplicateArgumentError("a".to_string()),
        test_duplicates_f5: "def f(a, *, **a): pass", LexicalErrorType::DuplicateArgumentError("a".to_string()),
        test_duplicates_l1: "lambda a, a: 1", LexicalErrorType::DuplicateArgumentError("a".to_string()),
        test_duplicates_l2: "lambda a, *, a: 1", LexicalErrorType::DuplicateArgumentError("a".to_string()),
        test_duplicates_l3: "lambda a, a=20: 1", LexicalErrorType::DuplicateArgumentError("a".to_string()),
        test_duplicates_l4: "lambda a, *a: 1", LexicalErrorType::DuplicateArgumentError("a".to_string()),
        test_duplicates_l5: "lambda a, *, **a: 1", LexicalErrorType::DuplicateArgumentError("a".to_string()),
        test_default_arg_error_f: "def f(a, b=20, c): pass", LexicalErrorType::DefaultArgumentError,
        test_default_arg_error_l: "lambda a, b=20, c: 1", LexicalErrorType::DefaultArgumentError,

        // Check some calls.
        test_positional_arg_error_f: "f(b=20, c)", LexicalErrorType::PositionalArgumentError,
        test_unpacked_arg_error_f: "f(**b, *c)", LexicalErrorType::UnpackedArgumentError,
        test_duplicate_kw_f1: "f(a=20, a=30)", LexicalErrorType::DuplicateKeywordArgumentError("a".to_string()),
    }
}
