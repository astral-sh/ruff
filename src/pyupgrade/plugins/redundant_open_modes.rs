use std::str::FromStr;

use anyhow::{anyhow, Result};
use log::error;
use rustpython_ast::{Constant, Expr, ExprKind, Located, Location};
use rustpython_parser::lexer;
use rustpython_parser::token::Tok;

use crate::ast::helpers::{self, match_name_or_attr};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::source_code_locator::SourceCodeLocator;

const OPEN_FUNC_NAME: &str = "open";

enum OpenMode {
    U,
    Ur,
    Ub,
    RUb,
    R,
    Rt,
    Wt,
}

impl FromStr for OpenMode {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        match string {
            "U" => Ok(OpenMode::U),
            "Ur" => Ok(OpenMode::Ur),
            "Ub" => Ok(OpenMode::Ub),
            "rUb" => Ok(OpenMode::RUb),
            "r" => Ok(OpenMode::R),
            "rt" => Ok(OpenMode::Rt),
            "wt" => Ok(OpenMode::Wt),
            _ => Err(anyhow!("Unknown open mode: {}", string)),
        }
    }
}

impl OpenMode {
    fn replacement_value(&self) -> Option<String> {
        match *self {
            OpenMode::U => None,
            OpenMode::Ur => None,
            OpenMode::Ub => Some(String::from("\"rb\"")),
            OpenMode::RUb => Some(String::from("\"rb\"")),
            OpenMode::R => None,
            OpenMode::Rt => None,
            OpenMode::Wt => Some(String::from("\"w\"")),
        }
    }
}

fn match_open(expr: &Expr) -> Option<&Expr> {
    if let ExprKind::Call { func, args, .. } = &expr.node {
        // TODO(andberger): Verify that "open" is still bound to the built-in function.
        if match_name_or_attr(func, OPEN_FUNC_NAME) {
            // Return the "open mode" parameter.
            return args.get(1);
        }
    }
    None
}

fn create_check(
    expr: &Expr,
    mode_param: &Expr,
    replacement_value: Option<String>,
    locator: &SourceCodeLocator,
    patch: bool,
) -> Check {
    let mut check = Check::new(CheckKind::RedundantOpenModes, Range::from_located(expr));
    if patch {
        if let Some(content) = replacement_value {
            check.amend(Fix::replacement(
                content,
                mode_param.location,
                mode_param.end_location.unwrap(),
            ));
        } else {
            match create_remove_param_fix(locator, expr, mode_param) {
                Ok(fix) => check.amend(fix),
                Err(e) => error!("Failed to remove parameter: {}", e),
            }
        }
    }
    check
}

fn create_remove_param_fix(
    locator: &SourceCodeLocator,
    expr: &Expr,
    mode_param: &Expr,
) -> Result<Fix> {
    let content = locator.slice_source_code_range(&Range {
        location: expr.location,
        end_location: expr.end_location.unwrap(),
    });
    // Find the last comma before mode_param
    // and delete that comma as well as mode_param.
    let mut fix_start: Option<Location> = None;
    let mut fix_end: Option<Location> = None;
    for (start, tok, end) in lexer::make_tokenizer(&content).flatten() {
        let start = helpers::to_absolute(start, expr.location);
        let end = helpers::to_absolute(end, expr.location);
        if start == mode_param.location {
            fix_end = Some(end);
            break;
        }
        if matches!(tok, Tok::Comma) {
            fix_start = Some(start);
        }
    }
    match (fix_start, fix_end) {
        (Some(start), Some(end)) => Ok(Fix::deletion(start, end)),
        _ => Err(anyhow::anyhow!(
            "Failed to locate start and end parentheses."
        )),
    }
}

/// U015
pub fn redundant_open_modes(checker: &mut Checker, expr: &Expr) {
    // TODO(andberger): Add "mode" keyword argument handling to handle invocations
    // on the following formats:
    // - `open("foo", mode="U")`
    // - `open(name="foo", mode="U")`
    // - `open(mode="U", name="foo")`
    if let Some(mode_param) = match_open(expr) {
        if let Located {
            node:
                ExprKind::Constant {
                    value: Constant::Str(mode_param_value),
                    ..
                },
            ..
        } = mode_param
        {
            if let Ok(mode) = OpenMode::from_str(mode_param_value.as_str()) {
                checker.add_check(create_check(
                    expr,
                    mode_param,
                    mode.replacement_value(),
                    checker.locator,
                    checker.patch(&CheckCode::U015),
                ));
            }
        }
    }
}
