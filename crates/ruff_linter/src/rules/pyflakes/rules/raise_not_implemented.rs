use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `raise` statements that raise `NotImplemented`.
///
/// ## Why is this bad?
/// `NotImplemented` is an exception used by binary special methods to indicate
/// that an operation is not implemented with respect to a particular type.
///
/// `NotImplemented` should not be raised directly. Instead, raise
/// `NotImplementedError`, which is used to indicate that the method is
/// abstract or not implemented in the derived class.
///
/// ## Example
/// ```python
/// class Foo:
///     def bar(self):
///         raise NotImplemented
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     def bar(self):
///         raise NotImplementedError
/// ```
///
/// ## References
/// - [Python documentation: `NotImplemented`](https://docs.python.org/3/library/constants.html#NotImplemented)
/// - [Python documentation: `NotImplementedError`](https://docs.python.org/3/library/exceptions.html#NotImplementedError)
#[derive(ViolationMetadata)]
pub(crate) struct RaiseNotImplemented;

impl Violation for RaiseNotImplemented {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`raise NotImplemented` should be `raise NotImplementedError`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use `raise NotImplementedError`".to_string())
    }
}

fn match_not_implemented(expr: &Expr) -> Option<&Expr> {
    match expr {
        Expr::Call(ast::ExprCall { func, .. }) => {
            if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                if id == "NotImplemented" {
                    return Some(func);
                }
            }
        }
        Expr::Name(ast::ExprName { id, .. }) => {
            if id == "NotImplemented" {
                return Some(expr);
            }
        }
        _ => {}
    }
    None
}

/// F901
pub(crate) fn raise_not_implemented(checker: &Checker, expr: &Expr) {
    let Some(expr) = match_not_implemented(expr) else {
        return;
    };
    let mut diagnostic = Diagnostic::new(RaiseNotImplemented, expr.range());
    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
            "NotImplementedError",
            expr.start(),
            checker.semantic(),
        )?;
        Ok(Fix::safe_edits(
            Edit::range_replacement(binding, expr.range()),
            import_edit,
        ))
    });
    checker.report_diagnostic(diagnostic);
}
