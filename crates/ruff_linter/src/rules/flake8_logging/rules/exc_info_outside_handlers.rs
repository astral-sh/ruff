use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::Truthiness;
use ruff_python_ast::{Expr, ExprCall};
use ruff_python_semantic::analyze::logging::is_logger_candidate;
use ruff_python_semantic::Modules;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{remove_argument, Parentheses};
use crate::rules::flake8_logging::rules::helpers::outside_handlers;

/// ## What it does
/// Checks for logging calls with `exc_info=` outside exception handlers.
///
/// ## Why is this bad?
///
///
/// ## Example
///
/// ```python
///
/// ```
///
/// Use instead:
///
/// ```python
///
/// ```
///
/// ## Fix safety
/// The fix will always be marked as unsafe, as it changes runtime behavior.
#[derive(ViolationMetadata)]
pub(crate) struct ExcInfoOutsideHandlers;

impl AlwaysFixableViolation for ExcInfoOutsideHandlers {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`exc_info=` outside exception handlers".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove `exc_info=`".to_string()
    }
}

pub(crate) fn exc_info_outside_handlers(checker: &mut Checker, call: &ExprCall) {
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::LOGGING) {
        return;
    }

    if !outside_handlers(call.start(), semantic) {
        return;
    }

    match call.func.as_ref() {
        func @ Expr::Attribute(_) => {
            if !is_logger_candidate(func, semantic, &checker.settings.logger_objects) {
                return;
            }
        }

        name @ Expr::Name(_) => {
            let Some(qualified_name) = semantic.resolve_qualified_name(name) else {
                return;
            };

            let ["logging", attr] = qualified_name.segments() else {
                return;
            };

            if *attr != "log" && LoggingLevel::from_attribute(attr).is_none() {
                return;
            }
        }

        _ => return,
    }

    let Some(exc_info) = call.arguments.find_keyword("exc_info") else {
        return;
    };

    let truthiness = Truthiness::from_expr(&exc_info.value, |id| semantic.has_builtin_binding(id));

    if !truthiness.is_true() && !truthiness.is_truthy() {
        return;
    }

    let arguments = &call.arguments;
    let source = checker.source();

    let Ok(edit) = remove_argument(exc_info, arguments, Parentheses::Preserve, source) else {
        unreachable!("Failed to remove `exc_info=`");
    };
    let fix = Fix::unsafe_edit(edit);

    let diagnostic = Diagnostic::new(ExcInfoOutsideHandlers, exc_info.range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}
