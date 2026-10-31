use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprCall;
use ruff_python_ast::flat;
use ruff_python_ast::helpers::Truthiness;
use ruff_python_semantic::analyze::logging;
use ruff_python_stdlib::logging::LoggingLevel;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for uses of `logging.exception()` with `exc_info` set to `False`.
///
/// ## Why is this bad?
/// The `logging.exception()` method captures the exception automatically, but
/// accepts an optional `exc_info` argument to override this behavior. Setting
/// `exc_info` to `False` disables the automatic capture of the exception and
/// stack trace.
///
/// Instead of setting `exc_info` to `False`, prefer `logging.error()`, which
/// has equivalent behavior to `logging.exception()` with `exc_info` set to
/// `False`, but is clearer in intent.
///
/// ## Example
/// ```python
/// logging.exception("...", exc_info=False)
/// ```
///
/// Use instead:
/// ```python
/// logging.error("...")
/// ```
///
/// ## Options
///
/// - `lint.logger-objects`
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.2.0", category = Category::Pedantic)]
pub(crate) struct ExceptionWithoutExcInfo;

impl Violation for ExceptionWithoutExcInfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of `logging.exception` with falsy `exc_info`".to_string()
    }
}

/// LOG007
pub(crate) fn exception_without_exc_info(checker: &Checker, call: flat::View<'_, ExprCall>) {
    match call.func().classify() {
        flat::ExprKind::Attribute(expr_attribute) => {
            let attr = expr_attribute.attr();

            if !matches!(
                LoggingLevel::from_attribute(attr.as_str()),
                Some(LoggingLevel::Exception)
            ) {
                return;
            }

            if !logging::is_logger_candidate(
                call.func(),
                checker.semantic(),
                &checker.settings().logger_objects,
            ) {
                return;
            }
        }
        flat::ExprKind::Name(_) => {
            if !checker
                .semantic()
                .resolve_qualified_name(call.func())
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["logging", "exception"])
                })
            {
                return;
            }
        }
        _ => return,
    }

    if exc_info_arg_is_falsey(call, checker) {
        checker.report_diagnostic(ExceptionWithoutExcInfo, call.range());
    }
}

fn exc_info_arg_is_falsey(call: flat::View<'_, ExprCall>, checker: &Checker) -> bool {
    call.arguments()
        .find_keyword("exc_info")
        .map(flat::View::<ruff_python_ast::Keyword>::value)
        .is_some_and(|value| {
            let truthiness =
                Truthiness::from_expr(value, |id| checker.semantic().has_builtin_binding(id));
            truthiness.into_bool() == Some(false)
        })
}
