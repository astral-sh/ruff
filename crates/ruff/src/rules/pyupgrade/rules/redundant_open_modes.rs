use std::str::FromStr;

use anyhow::{anyhow, Result};
use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword, Location};
use rustpython_parser::lexer;
use rustpython_parser::token::Tok;

use crate::ast::helpers::find_keyword;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::source_code::Locator;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct RedundantOpenModes {
        pub replacement: Option<String>,
    }
);
impl AlwaysAutofixableViolation for RedundantOpenModes {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedundantOpenModes { replacement } = self;
        match replacement {
            None => format!("Unnecessary open mode parameters"),
            Some(replacement) => {
                format!("Unnecessary open mode parameters, use \"{replacement}\"")
            }
        }
    }

    fn autofix_title(&self) -> String {
        let RedundantOpenModes { replacement } = self;
        match replacement {
            None => "Remove open mode parameters".to_string(),
            Some(replacement) => {
                format!("Replace with \"{replacement}\"")
            }
        }
    }
}

const OPEN_FUNC_NAME: &str = "open";
const MODE_KEYWORD_ARGUMENT: &str = "mode";

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
            "U" => Ok(Self::U),
            "Ur" => Ok(Self::Ur),
            "Ub" => Ok(Self::Ub),
            "rUb" => Ok(Self::RUb),
            "r" => Ok(Self::R),
            "rt" => Ok(Self::Rt),
            "wt" => Ok(Self::Wt),
            _ => Err(anyhow!("Unknown open mode: {}", string)),
        }
    }
}

impl OpenMode {
    fn replacement_value(&self) -> Option<String> {
        match *self {
            Self::U => None,
            Self::Ur => None,
            Self::Ub => Some(String::from("\"rb\"")),
            Self::RUb => Some(String::from("\"rb\"")),
            Self::R => None,
            Self::Rt => None,
            Self::Wt => Some(String::from("\"w\"")),
        }
    }
}

fn match_open(expr: &Expr) -> (Option<&Expr>, Vec<Keyword>) {
    if let ExprKind::Call {
        func,
        args,
        keywords,
    } = &expr.node
    {
        if matches!(&func.node, ExprKind::Name {id, ..} if id == OPEN_FUNC_NAME) {
            // Return the "open mode" parameter and keywords.
            return (args.get(1), keywords.clone());
        }
    }
    (None, vec![])
}

fn create_check(
    expr: &Expr,
    mode_param: &Expr,
    replacement_value: Option<String>,
    locator: &Locator,
    patch: bool,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        RedundantOpenModes {
            replacement: replacement_value.clone(),
        },
        Range::from_located(expr),
    );
    if patch {
        if let Some(content) = replacement_value {
            diagnostic.amend(Fix::replacement(
                content,
                mode_param.location,
                mode_param.end_location.unwrap(),
            ));
        } else {
            match create_remove_param_fix(locator, expr, mode_param) {
                Ok(fix) => {
                    diagnostic.amend(fix);
                }
                Err(e) => error!("Failed to remove parameter: {e}"),
            }
        }
    }
    diagnostic
}

fn create_remove_param_fix(locator: &Locator, expr: &Expr, mode_param: &Expr) -> Result<Fix> {
    let content =
        locator.slice_source_code_range(&Range::new(expr.location, expr.end_location.unwrap()));
    // Find the last comma before mode_param and create a deletion fix
    // starting from the comma and ending after mode_param.
    let mut fix_start: Option<Location> = None;
    let mut fix_end: Option<Location> = None;
    let mut is_first_arg: bool = false;
    let mut delete_first_arg: bool = false;
    for (start, tok, end) in lexer::make_tokenizer_located(content, expr.location).flatten() {
        if start == mode_param.location {
            if is_first_arg {
                delete_first_arg = true;
                continue;
            }
            fix_end = Some(end);
            break;
        }
        if delete_first_arg && matches!(tok, Tok::Name { .. }) {
            fix_end = Some(start);
            break;
        }
        if matches!(tok, Tok::Lpar) {
            is_first_arg = true;
            fix_start = Some(end);
        }
        if matches!(tok, Tok::Comma) {
            is_first_arg = false;
            if !delete_first_arg {
                fix_start = Some(start);
            }
        }
    }
    match (fix_start, fix_end) {
        (Some(start), Some(end)) => Ok(Fix::deletion(start, end)),
        _ => Err(anyhow::anyhow!(
            "Failed to locate start and end parentheses"
        )),
    }
}

/// UP015
pub fn redundant_open_modes(checker: &mut Checker, expr: &Expr) {
    // If `open` has been rebound, skip this check entirely.
    if !checker.is_builtin(OPEN_FUNC_NAME) {
        return;
    }
    let (mode_param, keywords): (Option<&Expr>, Vec<Keyword>) = match_open(expr);
    if mode_param.is_none() && !keywords.is_empty() {
        if let Some(keyword) = find_keyword(&keywords, MODE_KEYWORD_ARGUMENT) {
            if let ExprKind::Constant {
                value: Constant::Str(mode_param_value),
                ..
            } = &keyword.node.value.node
            {
                if let Ok(mode) = OpenMode::from_str(mode_param_value.as_str()) {
                    checker.diagnostics.push(create_check(
                        expr,
                        &keyword.node.value,
                        mode.replacement_value(),
                        checker.locator,
                        checker.patch(&Rule::RedundantOpenModes),
                    ));
                }
            }
        }
    } else if let Some(mode_param) = mode_param {
        if let ExprKind::Constant {
            value: Constant::Str(mode_param_value),
            ..
        } = &mode_param.node
        {
            if let Ok(mode) = OpenMode::from_str(mode_param_value.as_str()) {
                checker.diagnostics.push(create_check(
                    expr,
                    mode_param,
                    mode.replacement_value(),
                    checker.locator,
                    checker.patch(&Rule::RedundantOpenModes),
                ));
            }
        }
    }
}
