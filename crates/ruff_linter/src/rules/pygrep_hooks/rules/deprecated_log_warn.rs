use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::analyze::logging;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Check for usages of the deprecated `warn` method from the `logging` module.
///
/// ## Why is this bad?
/// The `warn` method is deprecated. Use `warning` instead.
///
/// ## Example
/// ```python
/// import logging
///
///
/// def foo():
///     logging.warn("Something happened")
/// ```
///
/// Use instead:
/// ```python
/// import logging
///
///
/// def foo():
///     logging.warning("Something happened")
/// ```
///
/// ## References
/// - [Python documentation: `logger.Logger.warning`](https://docs.python.org/3/library/logging.html#logging.Logger.warning)
#[violation]
pub struct DeprecatedLogWarn;

impl Violation for DeprecatedLogWarn {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`warn` is deprecated in favor of `warning`")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with `warning`"))
    }
}

/// PGH002
pub(crate) fn deprecated_log_warn(checker: &mut Checker, call: &ast::ExprCall) {
    match call.func.as_ref() {
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
            if !logging::is_logger_candidate(
                &call.func,
                checker.semantic(),
                &checker.settings.logger_objects,
            ) {
                return;
            }
            if !matches!(
                LoggingLevel::from_attribute(attr.as_str()),
                Some(LoggingLevel::Warn)
            ) {
                return;
            }
        }
        Expr::Name(_) => {
            if !checker
                .semantic()
                .resolve_call_path(call.func.as_ref())
                .is_some_and(|call_path| matches!(call_path.as_slice(), ["logging", "warn"]))
            {
                return;
            }
        }
        _ => return,
    }

    let mut diagnostic = Diagnostic::new(DeprecatedLogWarn, call.func.range());
    if checker.settings.preview.is_enabled() {
        match call.func.as_ref() {
            Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    "warning".to_string(),
                    attr.range(),
                )));
            }
            Expr::Name(_) => {
                diagnostic.try_set_fix(|| {
                    let (import_edit, binding) = checker.importer().get_or_import_symbol(
                        &ImportRequest::import("logging", "warning"),
                        call.start(),
                        checker.semantic(),
                    )?;
                    let name_edit = Edit::range_replacement(binding, call.func.range());
                    Ok(Fix::safe_edits(import_edit, [name_edit]))
                });
            }
            _ => {}
        }
    }
    checker.diagnostics.push(diagnostic);
}
