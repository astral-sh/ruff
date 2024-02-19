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
pub(super) fn validate_arguments(arguments: &ast::Parameters) -> Result<(), LexicalError> {
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
                range,
            ));
        }
    }

    Ok(())
}

pub(super) fn validate_pos_params(
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
            invalid.parameter.range(),
        ));
    }
    Ok(())
}

type FunctionArgument = (
    Option<(TextSize, TextSize, Option<ast::Identifier>)>,
    ast::Expr,
);

// Parse arguments as supplied during a function/lambda *call*.
pub(super) fn parse_arguments(
    function_arguments: Vec<FunctionArgument>,
) -> Result<ArgumentList, LexicalError> {
    let mut args = vec![];
    let mut keywords = vec![];

    let mut keyword_names = FxHashSet::with_capacity_and_hasher(
        function_arguments.len(),
        BuildHasherDefault::default(),
    );
    let mut double_starred = false;
    for (name, value) in function_arguments {
        if let Some((start, end, name)) = name {
            // Check for duplicate keyword arguments in the call.
            if let Some(keyword_name) = &name {
                if !keyword_names.insert(keyword_name.to_string()) {
                    return Err(LexicalError::new(
                        LexicalErrorType::DuplicateKeywordArgumentError(
                            keyword_name.to_string().into_boxed_str(),
                        ),
                        TextRange::new(start, end),
                    ));
                }
            } else {
                double_starred = true;
            }

            keywords.push(ast::Keyword {
                arg: name,
                value,
                range: TextRange::new(start, end),
            });
        } else {
            // Positional arguments mustn't follow keyword arguments.
            if !keywords.is_empty() && !is_starred(&value) {
                return Err(LexicalError::new(
                    LexicalErrorType::PositionalArgumentError,
                    value.range(),
                ));
                // Allow starred arguments after keyword arguments but
                // not after double-starred arguments.
            } else if double_starred {
                return Err(LexicalError::new(
                    LexicalErrorType::UnpackedArgumentError,
                    value.range(),
                ));
            }

            args.push(value);
        }
    }
    Ok(ArgumentList { args, keywords })
}

// Check if an expression is a starred expression.
const fn is_starred(exp: &ast::Expr) -> bool {
    exp.is_starred_expr()
}
