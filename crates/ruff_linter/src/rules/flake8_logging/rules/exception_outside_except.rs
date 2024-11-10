use ruff_python_ast::ExprCall;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Checks for uses of `logging.exception()` outside of exception handlers.
///
/// ## Why is this bad?
/// Calling `exception()` outside of an exception handler attaches `None`
/// exception information, leading to confusing messages.
///
/// ## Example
/// ```python
/// logging.exception("example")
/// # Output:
/// # ERROR:root:example
/// # NoneType: None
/// ```
///
/// Use instead:
/// ```python
/// logging.error("example")
/// # Output:
/// # ERROR:root:example
/// ```
#[violation]
pub struct ExceptionOutsideExcept;

impl Violation for ExceptionOutsideExcept {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of `logging.exception` outside exception handler".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace `logging.exception` with `logging.error`".to_string())
    }
}

/// LOG004
pub(crate) fn exception_outside_except(checker: &mut Checker, expr: &ExprCall) {
    if !checker.semantic().seen_module(Modules::LOGGING) {
        return;
    }

    let parents = checker.semantic().current_statements();
    let mut acceptable_position = false;
    for parent in parents {
        if let ruff_python_ast::Stmt::Try(stmt_try) = parent {
            for handler in &stmt_try.handlers {
                if handler.range().contains_range(expr.range()) {
                    acceptable_position = true;
                    break;
                }
            }
        } else if let ruff_python_ast::Stmt::FunctionDef(_) = parent {
            acceptable_position = false;
            break;
        }
        if acceptable_position {
            break;
        }
    }

    if acceptable_position {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(&expr.func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["logging", "exception"]))
    {
        let mut diagnostic = Diagnostic::new(ExceptionOutsideExcept, expr.func.range());
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import("logging", "error"),
                expr.start(),
                checker.semantic(),
            )?;
            let reference_edit = Edit::range_replacement(binding, expr.func.range());
            Ok(Fix::safe_edits(import_edit, [reference_edit]))
        });
        checker.diagnostics.push(diagnostic);
    }
}
