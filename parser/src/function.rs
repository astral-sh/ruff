use crate::ast;
use crate::error::{LexicalError, LexicalErrorType};
use rustc_hash::FxHashSet;

pub struct ArgumentList {
    pub args: Vec<ast::Expr>,
    pub keywords: Vec<ast::Keyword>,
}

type ParameterDefs = (Vec<ast::Arg>, Vec<ast::Arg>, Vec<ast::Expr>);
type ParameterDef = (ast::Arg, Option<ast::Expr>);

pub fn validate_arguments(arguments: ast::Arguments) -> Result<ast::Arguments, LexicalError> {
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
        if !all_arg_names.insert(arg_name) {
            return Err(LexicalError {
                error: LexicalErrorType::DuplicateArgumentError(arg_name.to_string()),
                location: arg.location,
            });
        }
    }

    Ok(arguments)
}

pub fn parse_params(
    params: (Vec<ParameterDef>, Vec<ParameterDef>),
) -> Result<ParameterDefs, LexicalError> {
    let mut posonly = Vec::with_capacity(params.0.len());
    let mut names = Vec::with_capacity(params.1.len());
    let mut defaults = vec![];

    let mut try_default = |name: &ast::Arg, default| {
        if let Some(default) = default {
            defaults.push(default);
        } else if !defaults.is_empty() {
            // Once we have started with defaults, all remaining arguments must
            // have defaults
            return Err(LexicalError {
                error: LexicalErrorType::DefaultArgumentError,
                location: name.location,
            });
        }
        Ok(())
    };

    for (name, default) in params.0 {
        try_default(&name, default)?;
        posonly.push(name);
    }

    for (name, default) in params.1 {
        try_default(&name, default)?;
        names.push(name);
    }

    Ok((posonly, names, defaults))
}

type FunctionArgument = (
    Option<(ast::Location, ast::Location, Option<String>)>,
    ast::Expr,
);

pub fn parse_args(func_args: Vec<FunctionArgument>) -> Result<ArgumentList, LexicalError> {
    let mut args = vec![];
    let mut keywords = vec![];

    let mut keyword_names =
        FxHashSet::with_capacity_and_hasher(func_args.len(), Default::default());
    let mut double_starred = false;
    for (name, value) in func_args {
        match name {
            Some((start, end, name)) => {
                if let Some(keyword_name) = &name {
                    if keyword_names.contains(keyword_name) {
                        return Err(LexicalError {
                            error: LexicalErrorType::DuplicateKeywordArgumentError(keyword_name.to_string()),
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
                // Allow starred arguments after keyword arguments but
                // not after double-starred arguments.
                if !keywords.is_empty() && !is_starred(&value) {
                    return Err(LexicalError {
                        error: LexicalErrorType::PositionalArgumentError,
                        location: value.location,
                    });
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

fn is_starred(exp: &ast::Expr) -> bool {
    matches!(exp.node, ast::ExprKind::Starred { .. })
}
