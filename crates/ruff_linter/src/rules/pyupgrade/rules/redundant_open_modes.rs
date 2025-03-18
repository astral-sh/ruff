use anyhow::Result;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_parser::{TokenKind, Tokens};
use ruff_python_stdlib::open_mode::OpenMode;
use ruff_text_size::{Ranged, TextSize};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for redundant `open` mode arguments.
///
/// ## Why is this bad?
/// Redundant `open` mode arguments are unnecessary and should be removed to
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
#[derive(ViolationMetadata)]
pub(crate) struct RedundantOpenModes {
    replacement: String,
}

impl AlwaysFixableViolation for RedundantOpenModes {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedundantOpenModes { replacement } = self;
        if replacement.is_empty() {
            "Unnecessary mode argument".to_string()
        } else {
            format!("Unnecessary modes, use `{replacement}`")
        }
    }

    fn fix_title(&self) -> String {
        let RedundantOpenModes { replacement } = self;
        if replacement.is_empty() {
            "Remove mode argument".to_string()
        } else {
            format!("Replace with `{replacement}`")
        }
    }
}

/// UP015
pub(crate) fn redundant_open_modes(checker: &Checker, call: &ast::ExprCall) {
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

    let Some(mode_arg) = call.arguments.find_argument_value("mode", 1) else {
        return;
    };
    let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = &mode_arg else {
        return;
    };
    let Ok(mode) = OpenMode::from_chars(value.chars()) else {
        return;
    };
    let reduced = mode.reduce();
    if reduced != mode {
        checker.report_diagnostic(create_diagnostic(call, mode_arg, reduced, checker));
    }
}

fn create_diagnostic(
    call: &ast::ExprCall,
    mode_arg: &Expr,
    mode: OpenMode,
    checker: &Checker,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        RedundantOpenModes {
            replacement: mode.to_string(),
        },
        mode_arg.range(),
    );

    if mode.is_empty() {
        diagnostic.try_set_fix(|| {
            create_remove_argument_fix(call, mode_arg, checker.tokens()).map(Fix::safe_edit)
        });
    } else {
        let stylist = checker.stylist();
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            format!("{}{mode}{}", stylist.quote(), stylist.quote()),
            mode_arg.range(),
        )));
    }

    diagnostic
}

fn create_remove_argument_fix(
    call: &ast::ExprCall,
    mode_arg: &Expr,
    tokens: &Tokens,
) -> Result<Edit> {
    // Find the last comma before mode_arg and create a deletion fix
    // starting from the comma and ending after mode_arg.
    let mut fix_start: Option<TextSize> = None;
    let mut fix_end: Option<TextSize> = None;
    let mut is_first_arg: bool = false;
    let mut delete_first_arg: bool = false;

    for token in tokens.in_range(call.range()) {
        if token.start() == mode_arg.start() {
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
