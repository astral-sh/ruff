use ahash::RandomState;
use std::collections::HashSet;

use crate::ast;
use crate::error::{LexicalError, LexicalErrorType};

pub struct ArgumentList {
    pub args: Vec<ast::Expr>,
    pub keywords: Vec<ast::Keyword>,
}

type ParameterDefs = (Vec<ast::Arg>, Vec<ast::Arg>, Vec<ast::Expr>);
type ParameterDef = (ast::Arg, Option<ast::Expr>);

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

type FunctionArgument = (Option<(ast::Location, Option<String>)>, ast::Expr);

pub fn parse_args(func_args: Vec<FunctionArgument>) -> Result<ArgumentList, LexicalError> {
    let mut args = vec![];
    let mut keywords = vec![];

    let mut keyword_names = HashSet::with_capacity_and_hasher(func_args.len(), RandomState::new());
    for (name, value) in func_args {
        match name {
            Some((location, name)) => {
                if let Some(keyword_name) = &name {
                    if keyword_names.contains(keyword_name) {
                        return Err(LexicalError {
                            error: LexicalErrorType::DuplicateKeywordArgumentError,
                            location,
                        });
                    }

                    keyword_names.insert(keyword_name.clone());
                }

                keywords.push(ast::Keyword::new(
                    location,
                    ast::KeywordData {
                        arg: name,
                        value: Box::new(value),
                    },
                ));
            }
            None => {
                // Allow starred args after keyword arguments.
                if !keywords.is_empty() && !is_starred(&value) {
                    return Err(LexicalError {
                        error: LexicalErrorType::PositionalArgumentError,
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
