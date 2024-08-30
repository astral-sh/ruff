use std::str::FromStr;

use anyhow::{anyhow, Result};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_parser::{TokenKind, Tokens};
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for redundant `open` mode parameters.
///
/// ## Why is this bad?
/// Redundant `open` mode parameters are unnecessary and should be removed to
/// avoid confusion.
///
/// ## Example
/// ```python
/// with open("foo.txt", "r") as f:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// with open("foo.txt") as f:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `open`](https://docs.python.org/3/library/functions.html#open)
#[violation]
pub struct RedundantOpenModes {
    replacement: Option<String>,
}

impl AlwaysFixableViolation for RedundantOpenModes {
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

    fn fix_title(&self) -> String {
        let RedundantOpenModes { replacement } = self;
        match replacement {
            None => "Remove open mode parameters".to_string(),
            Some(replacement) => {
                format!("Replace with \"{replacement}\"")
            }
        }
    }
}

/// UP015
pub(crate) fn redundant_open_modes(checker: &mut Checker, call: &ast::ExprCall) {
    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["" | "builtins" | "aiofiles", "open"]
            )
        })
    {
        return;
    }

    match call.arguments.find_argument("mode", 1) {
        None => {
            if !call.arguments.is_empty() {
                if let Some(keyword) = call.arguments.find_keyword("mode") {
                    if let Expr::StringLiteral(ast::ExprStringLiteral {
                        value: mode_param_value,
                        ..
                    }) = &keyword.value
                    {
                        if let Ok(mode) = OpenMode::from_str(mode_param_value.to_str()) {
                            checker.diagnostics.push(create_diagnostic(
                                call,
                                &keyword.value,
                                mode.replacement_value(),
                                checker.tokens(),
                            ));
                        }
                    }
                }
            }
        }
        Some(mode_param) => {
            if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = &mode_param {
                if let Ok(mode) = OpenMode::from_str(value.to_str()) {
                    checker.diagnostics.push(create_diagnostic(
                        call,
                        mode_param,
                        mode.replacement_value(),
                        checker.tokens(),
                    ));
                }
            }
        }
    }
}

#[derive(Debug, Copy, Clone)]
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
    fn replacement_value(self) -> Option<&'static str> {
        match self {
            Self::U => None,
            Self::Ur => None,
            Self::Ub => Some("\"rb\""),
            Self::RUb => Some("\"rb\""),
            Self::R => None,
            Self::Rt => None,
            Self::Wt => Some("\"w\""),
        }
    }
}

fn create_diagnostic(
    call: &ast::ExprCall,
    mode_param: &Expr,
    replacement_value: Option<&str>,
    tokens: &Tokens,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        RedundantOpenModes {
            replacement: replacement_value.map(ToString::to_string),
        },
        call.range(),
    );

    if let Some(content) = replacement_value {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            content.to_string(),
            mode_param.range(),
        )));
    } else {
        diagnostic
            .try_set_fix(|| create_remove_param_fix(call, mode_param, tokens).map(Fix::safe_edit));
    }

    diagnostic
}

fn create_remove_param_fix(
    call: &ast::ExprCall,
    mode_param: &Expr,
    tokens: &Tokens,
) -> Result<Edit> {
    // Find the last comma before mode_param and create a deletion fix
    // starting from the comma and ending after mode_param.
    let mut fix_start: Option<TextSize> = None;
    let mut fix_end: Option<TextSize> = None;
    let mut is_first_arg: bool = false;
    let mut delete_first_arg: bool = false;

    for token in tokens.in_range(call.range()) {
        if token.start() == mode_param.start() {
            if is_first_arg {
                delete_first_arg = true;
                continue;
            }
            fix_end = Some(token.end());
            break;
        }
        match token.kind() {
            TokenKind::Name if delete_first_arg => {
                fix_end = Some(token.start());
                break;
            }
            TokenKind::Lpar => {
                is_first_arg = true;
                fix_start = Some(token.end());
            }
            TokenKind::Comma => {
                is_first_arg = false;
                if !delete_first_arg {
                    fix_start = Some(token.start());
                }
            }
            _ => {}
        }
    }

    match (fix_start, fix_end) {
        (Some(start), Some(end)) => Ok(Edit::deletion(start, end)),
        _ => Err(anyhow::anyhow!(
            "Failed to locate start and end parentheses"
        )),
    }
}
