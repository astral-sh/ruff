use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::contains_effect;
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::super::helpers::at_last_top_level_expression_in_cell;

/// ## What it does
/// Checks for useless expressions.
///
/// For Jupyter Notebooks, this rule ignores to check the last top-level expression for each cell.
/// This is because it's common to have a cell that ends with an expression to display it's value.
///
/// ## Why is this bad?
/// Useless expressions have no effect on the program, and are often included
/// by mistake. Assign a useless expression to a variable, or remove it
/// entirely.
///
/// ## Example
/// ```python
/// 1 + 1
/// ```
///
/// Use instead:
/// ```python
/// foo = 1 + 1
/// ```
///
/// ## Known problems
/// This rule ignores expression types that are commonly used for their side
/// effects, such as function calls.
///
/// However, if a seemingly useless expression (like an attribute access) is
/// needed to trigger a side effect, consider assigning it to an anonymous
/// variable, to indicate that the return value is intentionally ignored.
///
/// For example, given:
/// ```python
/// with errors.ExceptionRaisedContext():
///     obj.attribute
/// ```
///
/// Use instead:
/// ```python
/// with errors.ExceptionRaisedContext():
///     _ = obj.attribute
/// ```
#[violation]
pub struct UselessExpression {
    kind: Kind,
}

impl Violation for UselessExpression {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.kind {
            Kind::Expression => {
                format!("Found useless expression. Either assign it to a variable or remove it.")
            }
            Kind::Attribute => {
                format!(
                    "Found useless attribute access. Either assign it to a variable or remove it."
                )
            }
        }
    }
}

/// B018
pub(crate) fn useless_expression(checker: &mut Checker, value: &Expr) {
    // Ignore comparisons, as they're handled by `useless_comparison`.
    if value.is_compare_expr() {
        return;
    }

    // Ignore strings, to avoid false positives with docstrings.
    if matches!(
        value,
        Expr::FString(_) | Expr::StringLiteral(_) | Expr::EllipsisLiteral(_)
    ) {
        return;
    }

    if checker.source_type.is_ipynb()
        && at_last_top_level_expression_in_cell(
            checker.semantic(),
            checker.locator(),
            checker.cell_offsets(),
        )
    {
        return;
    }

    // Ignore statements that have side effects.
    if contains_effect(value, |id| checker.semantic().has_builtin_binding(id)) {
        // Flag attributes as useless expressions, even if they're attached to calls or other
        // expressions.
        if value.is_attribute_expr() {
            checker.diagnostics.push(Diagnostic::new(
                UselessExpression {
                    kind: Kind::Attribute,
                },
                value.range(),
            ));
        }
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        UselessExpression {
            kind: Kind::Expression,
        },
        value.range(),
    ));
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum Kind {
    Expression,
    Attribute,
}
