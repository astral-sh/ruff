use rustpython_ast::{Constant, Expr, ExprKind, Located, Location};
use rustpython_parser::lexer;
use rustpython_parser::token::Tok;

use crate::ast::helpers::{self, match_name_or_attr};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
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

impl OpenMode {
    fn from_str(input: &str) -> Option<OpenMode> {
        match input {
            "U" => Some(OpenMode::U),
            "Ur" => Some(OpenMode::Ur),
            "Ub" => Some(OpenMode::Ub),
            "rUb" => Some(OpenMode::RUb),
            "r" => Some(OpenMode::R),
            "rt" => Some(OpenMode::Rt),
            "wt" => Some(OpenMode::Wt),
            _ => None,
        }
    }

    fn replacement_value(&self) -> String {
        match *self {
            OpenMode::U => String::from(""),
            OpenMode::Ur => String::from(""),
            OpenMode::Ub => String::from("\"rb\""),
            OpenMode::RUb => String::from("\"rb\""),
            OpenMode::R => String::from(""),
            OpenMode::Rt => String::from(""),
            OpenMode::Wt => String::from("\"w\""),
        }
    }
}

fn match_open(expr: &Expr) -> Option<&Located<ExprKind>> {
    if let ExprKind::Call { func, args, .. } = &expr.node {
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
    replacement_value: String,
    locator: &SourceCodeLocator,
    patch: bool,
) -> Check {
    let mut check = Check::new(CheckKind::RedundantOpenModes, Range::from_located(expr));
    if patch {
        if replacement_value.is_empty() {
            if let Some(fix) = create_remove_param_fix(locator, expr, mode_param) {
                check.amend(fix);
            }
        } else {
            check.amend(Fix::replacement(
                replacement_value,
                mode_param.location,
                mode_param.end_location.unwrap(),
            ))
        }
    }
    check
}

fn create_remove_param_fix(
    locator: &SourceCodeLocator,
    expr: &Located<ExprKind>,
    mode_param: &Located<ExprKind>,
) -> Option<Fix> {
    let content = locator.slice_source_code_range(&Range {
        location: expr.location,
        end_location: expr.end_location.unwrap(),
    });
    let mut fix_start: Option<Location> = None;
    for (start, tok, _) in lexer::make_tokenizer(&content).flatten() {
        let start = helpers::to_absolute(&start, &expr.location);
        if matches!(tok, Tok::Comma) {
            fix_start = Some(start);
            break;
        }
    }
    match (fix_start, mode_param.end_location) {
        (Some(start), Some(end)) => Some(Fix::deletion(start, end)),
        _ => None,
    }
}

/// U013
pub fn redundant_open_modes(checker: &mut Checker, expr: &Expr) {
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
            if let Some(mode) = OpenMode::from_str(mode_param_value.as_str()) {
                checker.add_check(create_check(
                    expr,
                    mode_param,
                    mode.replacement_value(),
                    checker.locator,
                    checker.patch(),
                ));
            }
        }
    }
}
