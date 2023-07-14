use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::BindingId;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for the presence of unnecessary quotes in type annotations.
///
/// ## Why is this bad?
/// In Python, type annotations can be quoted to avoid forward references.
/// However, if `from __future__ import annotations` is present, Python
/// will always evaluate type annotations in a deferred manner, making
/// the quotes unnecessary.
///
/// ## Example
/// ```python
/// from __future__ import annotations
///
///
/// def foo(bar: "Bar") -> "Bar":
///     ...
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
///
/// def foo(bar: Bar) -> Bar:
///     ...
/// ```
///
/// ## References
/// - [PEP 563](https://peps.python.org/pep-0563/)
/// - [Python documentation: `__future__`](https://docs.python.org/3/library/__future__.html#module-__future__)
#[violation]
pub struct UnquotedAnnotation {
    name: String,
}

impl Violation for UnquotedAnnotation {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnquotedAnnotation { name } = self;
        format!("Typing-only variable referenced in runtime annotation: `{name}`")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Add quotes".to_string())
    }
}

/// TCH200
pub(crate) fn unquoted_annotation(checker: &mut Checker, binding_id: BindingId, expr: &Expr) {
    // If we're already in a quoted annotation, skip.
    if checker.semantic().in_deferred_type_definition() {
        return;
    }

    // If we're in a typing-only context, skip.
    if checker.semantic().execution_context().is_typing() {
        return;
    }

    // If the reference resolved to a typing-only import, flag.
    if checker.semantic().bindings[binding_id].context.is_typing() {
        // Expand any attribute chains (e.g., flag `typing.List` in `typing.List[int]`).
        let mut expr = expr;
        for parent in checker.semantic().expr_ancestors() {
            if parent.is_attribute_expr() {
                expr = parent;
            } else {
                break;
            }
        }

        let mut diagnostic = Diagnostic::new(
            UnquotedAnnotation {
                name: checker.locator.slice(expr.range()).to_string(),
            },
            expr.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            // We can only _fix_ this if we're in a type annotation.
            if checker.semantic().in_runtime_annotation() {
                diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
                    format!("\"{}\"", checker.locator.slice(expr.range()).to_string()),
                    expr.range(),
                )));
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
