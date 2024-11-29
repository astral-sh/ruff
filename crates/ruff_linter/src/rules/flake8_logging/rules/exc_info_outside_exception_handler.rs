use crate::checkers::ast::Checker;
use crate::rules::flake8_logging::helpers;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, ExprCall, Stmt};
use ruff_python_semantic::analyze::logging;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for cases where a logging call is made with `exc_info` set to `True`,
/// outside of an exception handling block.
///
/// ## Why is this bad?
/// When outside of an exception handling block, the variable holding the
/// exception information will be assigned to `None` as there is no active
/// exception, which then causes the final line of the logging call to contain
/// `NoneType: None`, which is meaningless
///
/// ## Example
/// ```python
/// logging.error("...", exc_info=True)
/// ```
///
/// Either add an exception handling block:
/// ```python
/// try:
///     logging.error("...", exc_info=True)
/// except ...:
///     ...
/// ```
///
/// Or don't set `exc_info` to `True`:
/// logging.error("...")
#[derive(ViolationMetadata)]
pub(crate) struct ExcInfoOutsideExceptionHandler;

impl Violation for ExcInfoOutsideExceptionHandler {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of `exc_info` outside an exception handler".to_string()
    }
}

/// LOG014
pub(crate) fn exc_info_outside_exception_handler(checker: &mut Checker, call: &ExprCall) {
    if is_logging_call(checker, call)
        && helpers::exc_info_arg_is_truey(checker, call)
        && currently_outside_exception_handler(checker)
    {
        checker.diagnostics.push(Diagnostic::new(
            ExcInfoOutsideExceptionHandler,
            call.range(),
        ));
    }
}

fn currently_outside_exception_handler(checker: &mut Checker) -> bool {
    for parent_stmt in checker.semantic().current_statements() {
        if let Stmt::Try(_) = parent_stmt {
            return false;
        }
    }

    true
}

fn is_logging_call(checker: &mut Checker, call: &ExprCall) -> bool {
    match call.func.as_ref() {
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
            // Match any logging level
            if LoggingLevel::from_attribute(attr.as_str()).is_none() {
                return false;
            }

            if !logging::is_logger_candidate(
                &call.func,
                checker.semantic(),
                &checker.settings.logger_objects,
            ) {
                return false;
            }
        }
        Expr::Name(_) => {
            if !checker
                .semantic()
                .resolve_qualified_name(call.func.as_ref())
                .is_some_and(|qualified_name| {
                    matches!(
                        qualified_name.segments(),
                        ["logging", attr] if helpers::is_logger_method_name(attr)
                    )
                })
            {
                return false;
            }
        }
        _ => return false,
    }

    true
}
