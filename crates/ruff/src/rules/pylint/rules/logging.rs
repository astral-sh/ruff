use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::helpers::{is_logger_candidate, SimpleCallArgs};
use crate::ast::logging::LoggingLevel;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pyflakes::cformat::CFormatSummary;
use crate::violation::Violation;

define_violation!(
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
    ///     logging.error('%s error occurred: %s', e)  # [logging-too-few-args]
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
    ///     logging.error('%s error occurred: %s', type(e), e)
    ///     raise
    /// ```
    pub struct LoggingTooFewArgs;
);
impl Violation for LoggingTooFewArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Not enough arguments for `logging` format string")
    }
}

define_violation!(
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
    ///     logging.error('Error occurred: %s', type(e), e)  # [logging-too-many-args]
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
    ///     logging.error('%s error occurred: %s', type(e), e)
    ///     raise
    /// ```
    pub struct LoggingTooManyArgs;
);
impl Violation for LoggingTooManyArgs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Too many arguments for `logging` format string")
    }
}

/// Check logging calls for violations.
pub fn logging_call(checker: &mut Checker, func: &Expr, args: &[Expr], keywords: &[Keyword]) {
    if !is_logger_candidate(func) {
        return;
    }

    if let ExprKind::Attribute { attr, .. } = &func.node {
        if LoggingLevel::from_str(attr.as_str()).is_some() {
            let call_args = SimpleCallArgs::new(args, keywords);

            // E1205 - E1206
            if let Some(msg) = call_args.get_argument("msg", Some(0)) {
                if let ExprKind::Constant {
                    value: Constant::Str(value),
                    ..
                } = &msg.node
                {
                    if let Ok(summary) = CFormatSummary::try_from(value.as_str()) {
                        if summary.starred {
                            return;
                        }

                        if !call_args.kwargs.is_empty() {
                            // Keyword checking on logging strings is complicated by
                            // special keywords - out of scope.
                            return;
                        }

                        let message_args = call_args.args.len() - 1;

                        if checker.settings.rules.enabled(&Rule::LoggingTooManyArgs)
                            && summary.num_positional < message_args
                        {
                            checker.diagnostics.push(Diagnostic::new(
                                LoggingTooManyArgs,
                                Range::from_located(func),
                            ));
                        }

                        if checker.settings.rules.enabled(&Rule::LoggingTooFewArgs)
                            && summary.num_positional > message_args
                        {
                            checker.diagnostics.push(Diagnostic::new(
                                LoggingTooFewArgs,
                                Range::from_located(func),
                            ));
                        }
                    }
                }
            }
        }
    }
}
