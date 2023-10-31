use std::str::FromStr;

use anyhow::{anyhow, Result};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, PySourceType};
use ruff_python_parser::{lexer, AsMode};
use ruff_python_semantic::SemanticModel;
use ruff_source_file::Locator;
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
    if !is_open_builtin(call.func.as_ref(), checker.semantic()) {
        return;
    }

    match call.arguments.find_argument("mode", 1) {
        None => {
            if !call.arguments.is_empty() {
                if let Some(keyword) = call.arguments.find_keyword(MODE_KEYWORD_ARGUMENT) {
                    if let Expr::StringLiteral(ast::ExprStringLiteral {
                        value: mode_param_value,
                        ..
                    }) = &keyword.value
                    {
                        if let Ok(mode) = OpenMode::from_str(mode_param_value) {
                            checker.diagnostics.push(create_check(
                                call,
                                &keyword.value,
                                mode.replacement_value(),
                                checker.locator(),
                                checker.source_type,
                            ));
                        }
                    }
                }
            }
        }
        Some(mode_param) => {
            if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = &mode_param {
                if let Ok(mode) = OpenMode::from_str(value) {
                    checker.diagnostics.push(create_check(
                        call,
                        mode_param,
                        mode.replacement_value(),
                        checker.locator(),
                        checker.source_type,
                    ));
                }
            }
        }
    }
}

const OPEN_FUNC_NAME: &str = "open";
const MODE_KEYWORD_ARGUMENT: &str = "mode";

/// Returns `true` if the given `call` is a call to the `open` builtin.
fn is_open_builtin(func: &Expr, semantic: &SemanticModel) -> bool {
    let Some(ast::ExprName { id, .. }) = func.as_name_expr() else {
        return false;
    };
    id.as_str() == OPEN_FUNC_NAME && semantic.is_builtin(id)
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

fn create_check<T: Ranged>(
    expr: &T,
    mode_param: &Expr,
    replacement_value: Option<&str>,
    locator: &Locator,
    source_type: PySourceType,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        RedundantOpenModes {
            replacement: replacement_value.map(ToString::to_string),
        },
        expr.range(),
    );

    if let Some(content) = replacement_value {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            content.to_string(),
            mode_param.range(),
        )));
    } else {
        diagnostic.try_set_fix(|| {
            create_remove_param_fix(locator, expr, mode_param, source_type).map(Fix::safe_edit)
        });
    }

    diagnostic
}

fn create_remove_param_fix<T: Ranged>(
    locator: &Locator,
    expr: &T,
    mode_param: &Expr,
    source_type: PySourceType,
) -> Result<Edit> {
    let content = locator.slice(expr);
    // Find the last comma before mode_param and create a deletion fix
    // starting from the comma and ending after mode_param.
    let mut fix_start: Option<TextSize> = None;
    let mut fix_end: Option<TextSize> = None;
    let mut is_first_arg: bool = false;
    let mut delete_first_arg: bool = false;
    for (tok, range) in lexer::lex_starts_at(content, source_type.as_mode(), expr.start()).flatten()
    {
        if range.start() == mode_param.start() {
            if is_first_arg {
                delete_first_arg = true;
                continue;
            }
            fix_end = Some(range.end());
            break;
        }
        if delete_first_arg && tok.is_name() {
            fix_end = Some(range.start());
            break;
        }
        if tok.is_lpar() {
            is_first_arg = true;
            fix_start = Some(range.end());
        }
        if tok.is_comma() {
            is_first_arg = false;
            if !delete_first_arg {
                fix_start = Some(range.start());
            }
        }
    }
    match (fix_start, fix_end) {
        (Some(start), Some(end)) => Ok(Edit::deletion(start, end)),
        _ => Err(anyhow::anyhow!(
            "Failed to locate start and end parentheses"
        )),
    }
}
