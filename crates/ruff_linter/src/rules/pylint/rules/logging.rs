use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::logging;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::pyflakes::cformat::CFormatSummary;

/// ## What it does
/// Checks for too few positional arguments for a `logging` format string.
///
/// ## Why is this bad?
/// A `TypeError` will be raised if the statement is run.
///
/// ## Example
/// ```python
/// import logging
///
/// try:
///     function()
/// except Exception as e:
///     logging.error("%s error occurred: %s", e)
///     raise
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// try:
///     function()
/// except Exception as e:
///     logging.error("%s error occurred: %s", type(e), e)
///     raise
/// ```
#[violation]
pub struct LoggingTooFewArgs;

impl Violation for LoggingTooFewArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Not enough arguments for `logging` format string")
    }
}

/// ## What it does
/// Checks for too many positional arguments for a `logging` format string.
///
/// ## Why is this bad?
/// A `TypeError` will be raised if the statement is run.
///
/// ## Example
/// ```python
/// import logging
///
/// try:
///     function()
/// except Exception as e:
///     logging.error("Error occurred: %s", type(e), e)
///     raise
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
/// try:
///     function()
/// except Exception as e:
///     logging.error("%s error occurred: %s", type(e), e)
///     raise
/// ```
#[violation]
pub struct LoggingTooManyArgs;

impl Violation for LoggingTooManyArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Too many arguments for `logging` format string")
    }
}

/// PLE1205
/// PLE1206
pub(crate) fn logging_call(checker: &mut Checker, call: &ast::ExprCall) {
    // If there are any starred arguments, abort.
    if call.arguments.args.iter().any(Expr::is_starred_expr) {
        return;
    }

    // If there are any starred keyword arguments, abort.
    if call
        .arguments
        .keywords
        .iter()
        .any(|keyword| keyword.arg.is_none())
    {
        return;
    }

    match call.func.as_ref() {
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
            if LoggingLevel::from_attribute(attr).is_none() {
                return;
            };
            if !logging::is_logger_candidate(
                &call.func,
                checker.semantic(),
                &checker.settings.logger_objects,
            ) {
                return;
            }
        }
        Expr::Name(_) => {
            let Some(qualified_name) = checker
                .semantic()
                .resolve_qualified_name(call.func.as_ref())
            else {
                return;
            };
            let ["logging", attribute] = qualified_name.segments() else {
                return;
            };
            if LoggingLevel::from_attribute(attribute).is_none() {
                return;
            };
        }
        _ => return,
    };

    let Some(Expr::StringLiteral(ast::ExprStringLiteral { value, .. })) =
        call.arguments.find_positional(0)
    else {
        return;
    };

    let Ok(summary) = CFormatSummary::try_from(value.to_str()) else {
        return;
    };

    if summary.starred {
        return;
    }

    if !summary.keywords.is_empty() {
        return;
    }

    let num_message_args = call.arguments.args.len() - 1;
    let num_keywords = call.arguments.keywords.len();

    if checker.enabled(Rule::LoggingTooManyArgs) {
        if summary.num_positional < num_message_args {
            checker
                .diagnostics
                .push(Diagnostic::new(LoggingTooManyArgs, call.func.range()));
        }
    }

    if checker.enabled(Rule::LoggingTooFewArgs) {
        if num_message_args > 0 && num_keywords == 0 && summary.num_positional > num_message_args {
            checker
                .diagnostics
                .push(Diagnostic::new(LoggingTooFewArgs, call.func.range()));
        }
    }
}
