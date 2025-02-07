use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

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
#[derive(ViolationMetadata)]
pub(crate) struct UnreliableCallableCheck;

impl Violation for UnreliableCallableCheck {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Using `hasattr(x, \"__call__\")` to test if x is callable is unreliable. Use \
             `callable(x)` for consistent results."
            .to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `callable()`".to_string())
    }
}

/// B004
pub(crate) fn unreliable_callable_check(
    checker: &Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let [obj, attr, ..] = args else {
        return;
    };
    let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = attr else {
        return;
    };
    if value != "__call__" {
        return;
    }
    let Some(builtins_function) = checker.semantic().resolve_builtin_symbol(func) else {
        return;
    };
    if !matches!(builtins_function, "hasattr" | "getattr") {
        return;
    }

    let mut diagnostic = Diagnostic::new(UnreliableCallableCheck, expr.range());
    if builtins_function == "hasattr" {
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_builtin_symbol(
                "callable",
                expr.start(),
                checker.semantic(),
            )?;
            let binding_edit = Edit::range_replacement(
                format!("{binding}({})", checker.locator().slice(obj)),
                expr.range(),
            );
            Ok(Fix::safe_edits(binding_edit, import_edit))
        });
    }
    checker.report_diagnostic(diagnostic);
}
