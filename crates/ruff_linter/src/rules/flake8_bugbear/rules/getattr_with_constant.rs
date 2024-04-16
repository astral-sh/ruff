use crate::fix::edits::pad;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_stdlib::identifiers::{is_identifier, is_mangled_private};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

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

impl AlwaysFixableViolation for GetAttrWithConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Do not call `getattr` with a constant attribute value. It is not any safer than \
             normal property access."
        )
    }

    fn fix_title(&self) -> String {
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
    let [obj, arg] = args else {
        return;
    };
    if obj.is_starred_expr() {
        return;
    }
    let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = arg else {
        return;
    };
    if !is_identifier(value.to_str()) {
        return;
    }
    if is_mangled_private(value.to_str()) {
        return;
    }
    if !checker.semantic().match_builtin_expr(func, "getattr") {
        return;
    }

    let mut diagnostic = Diagnostic::new(GetAttrWithConstant, expr.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        pad(
            if matches!(
                obj,
                Expr::Name(_) | Expr::Attribute(_) | Expr::Subscript(_) | Expr::Call(_)
            ) && !checker.locator().contains_line_break(obj.range())
            {
                format!("{}.{}", checker.locator().slice(obj), value)
            } else {
                // Defensively parenthesize any other expressions. For example, attribute accesses
                // on `int` literals must be parenthesized, e.g., `getattr(1, "real")` becomes
                // `(1).real`. The same is true for named expressions and others.
                format!("({}).{}", checker.locator().slice(obj), value)
            },
            expr.range(),
            checker.locator(),
        ),
        expr.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
