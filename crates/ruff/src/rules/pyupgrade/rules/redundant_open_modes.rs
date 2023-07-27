use std::str::FromStr;

use anyhow::{anyhow, Result};
use ruff_python_ast::{self as ast, Constant, Expr, Keyword, Ranged};
use ruff_python_parser::{lexer, Mode};
use ruff_text_size::TextSize;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::find_keyword;
use ruff_python_semantic::SemanticModel;
use ruff_source_file::Locator;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

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

/// UP015
pub(crate) fn redundant_open_modes(checker: &mut Checker, expr: &Expr) {
    let Some((mode_param, keywords)) = match_open(expr, checker.semantic()) else {
        return;
    };
    match mode_param {
        None => {
            if !keywords.is_empty() {
                if let Some(keyword) = find_keyword(keywords, MODE_KEYWORD_ARGUMENT) {
                    if let Expr::Constant(ast::ExprConstant {
                        value: Constant::Str(mode_param_value),
                        ..
                    }) = &keyword.value
                    {
                        if let Ok(mode) = OpenMode::from_str(mode_param_value.as_str()) {
                            checker.diagnostics.push(create_check(
                                expr,
                                &keyword.value,
                                mode.replacement_value(),
                                checker.locator(),
                                checker.patch(Rule::RedundantOpenModes),
                            ));
                        }
                    }
                }
            }
        }
        Some(mode_param) => {
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Str(value),
                ..
            }) = &mode_param
            {
                if let Ok(mode) = OpenMode::from_str(value.as_str()) {
                    checker.diagnostics.push(create_check(
                        expr,
                        mode_param,
                        mode.replacement_value(),
                        checker.locator(),
                        checker.patch(Rule::RedundantOpenModes),
                    ));
                }
            }
        }
    }
}

const OPEN_FUNC_NAME: &str = "open";
const MODE_KEYWORD_ARGUMENT: &str = "mode";

fn match_open<'a>(
    expr: &'a Expr,
    model: &SemanticModel,
) -> Option<(Option<&'a Expr>, &'a [Keyword])> {
    let ast::ExprCall {
        func,
        args,
        keywords,
        range: _,
    } = expr.as_call_expr()?;

    let ast::ExprName { id, .. } = func.as_name_expr()?;

    if id.as_str() == OPEN_FUNC_NAME && model.is_builtin(id) {
        // Return the "open mode" parameter and keywords.
        Some((args.get(1), keywords))
    } else {
        None
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

fn create_check(
    expr: &Expr,
    mode_param: &Expr,
    replacement_value: Option<&str>,
    locator: &Locator,
    patch: bool,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        RedundantOpenModes {
            replacement: replacement_value.map(ToString::to_string),
        },
        expr.range(),
    );
    if patch {
        if let Some(content) = replacement_value {
            diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                content.to_string(),
                mode_param.range(),
            )));
        } else {
            diagnostic.try_set_fix(|| {
                create_remove_param_fix(locator, expr, mode_param).map(Fix::automatic)
            });
        }
    }
    diagnostic
}

fn create_remove_param_fix(locator: &Locator, expr: &Expr, mode_param: &Expr) -> Result<Edit> {
    let content = locator.slice(expr.range());
    // Find the last comma before mode_param and create a deletion fix
    // starting from the comma and ending after mode_param.
    let mut fix_start: Option<TextSize> = None;
    let mut fix_end: Option<TextSize> = None;
    let mut is_first_arg: bool = false;
    let mut delete_first_arg: bool = false;
    for (tok, range) in lexer::lex_starts_at(content, Mode::Module, expr.start()).flatten() {
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
