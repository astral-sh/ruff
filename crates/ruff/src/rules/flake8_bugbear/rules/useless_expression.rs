use ruff_python_ast::{self as ast, Constant, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::contains_effect;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for useless expressions.
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
    if matches!(value, Expr::Compare(_)) {
        return;
    }

    // Ignore strings, to avoid false positives with docstrings.
    if matches!(
        value,
        Expr::FString(_)
            | Expr::Constant(ast::ExprConstant {
                value: Constant::Str(..) | Constant::Ellipsis,
                ..
            })
    ) {
        return;
    }

    // Ignore statements that have side effects.
    if contains_effect(value, |id| checker.semantic().is_builtin(id)) {
        // Flag attributes as useless expressions, even if they're attached to calls or other
        // expressions.
        if matches!(value, Expr::Attribute(_)) {
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
