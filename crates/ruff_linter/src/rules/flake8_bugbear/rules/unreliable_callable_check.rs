use ruff_python_ast::{self as ast, Constant, Expr};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of `hasattr` to test if an object is callable (e.g.,
/// `hasattr(obj, "__call__")`).
///
/// ## Why is this bad?
/// Using `hasattr` is an unreliable mechanism for testing if an object is
/// callable. If `obj` implements a custom `__getattr__`, or if its `__call__`
/// is itself not callable, you may get misleading results.
///
/// Instead, use `callable(obj)` to test if `obj` is callable.
///
/// ## Example
/// ```python
/// hasattr(obj, "__call__")
/// ```
///
/// Use instead:
/// ```python
/// callable(obj)
/// ```
///
/// ## References
/// - [Python documentation: `callable`](https://docs.python.org/3/library/functions.html#callable)
/// - [Python documentation: `hasattr`](https://docs.python.org/3/library/functions.html#hasattr)
/// - [Python documentation: `__getattr__`](https://docs.python.org/3/reference/datamodel.html#object.__getattr__)
/// - [Python documentation: `__call__`](https://docs.python.org/3/reference/datamodel.html#object.__call__)
#[violation]
pub struct UnreliableCallableCheck;

impl Violation for UnreliableCallableCheck {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Using `hasattr(x, \"__call__\")` to test if x is callable is unreliable. Use \
             `callable(x)` for consistent results."
        )
    }

    fn autofix_title(&self) -> Option<String> {
        Some(format!("Replace with `callable()`"))
    }
}

/// B004
pub(crate) fn unreliable_callable_check(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        return;
    };
    if !matches!(id.as_str(), "hasattr" | "getattr") {
        return;
    }
    let [obj, attr, ..] = args else {
        return;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(ast::StringConstant { value, .. }),
        ..
    }) = attr
    else {
        return;
    };
    if value != "__call__" {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnreliableCallableCheck, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        if id == "hasattr" {
            if checker.semantic().is_builtin("callable") {
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    format!("callable({})", checker.locator().slice(obj)),
                    expr.range(),
                )));
            }
        }
    }
    checker.diagnostics.push(diagnostic);
}
