use ruff_python_ast::{self as ast, Constant, Expr, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_stdlib::identifiers::{is_identifier, is_mangled_private};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of `getattr` that take a constant attribute value as an
/// argument (e.g., `getattr(obj, "foo")`).
///
/// ## Why is this bad?
/// `getattr` is used to access attributes dynamically. If the attribute is
/// defined as a constant, it is no safer than a typical property access. When
/// possible, prefer property access over `getattr` calls, as the former is
/// more concise and idiomatic.
///
///
/// ## Example
/// ```python
/// getattr(obj, "foo")
/// ```
///
/// Use instead:
/// ```python
/// obj.foo
/// ```
///
/// ## References
/// - [Python documentation: `getattr`](https://docs.python.org/3/library/functions.html#getattr)
#[violation]
pub struct GetAttrWithConstant;

impl AlwaysAutofixableViolation for GetAttrWithConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Do not call `getattr` with a constant attribute value. It is not any safer than \
             normal property access."
        )
    }

    fn autofix_title(&self) -> String {
        "Replace `getattr` with attribute access".to_string()
    }
}

/// B009
pub(crate) fn getattr_with_constant(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Expr::Name(ast::ExprName { id, .. }) = func else {
        return;
    };
    if id != "getattr" {
        return;
    }
    let [obj, arg] = args else {
        return;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(value),
        ..
    }) = arg
    else {
        return;
    };
    if !is_identifier(value) {
        return;
    }
    if is_mangled_private(value) {
        return;
    }

    let mut diagnostic = Diagnostic::new(GetAttrWithConstant, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            format!("{}.{}", checker.locator().slice(obj.range()), value),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
