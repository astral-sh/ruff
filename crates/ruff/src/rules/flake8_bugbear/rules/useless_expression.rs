use rustpython_parser::ast::{self, Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::contains_effect;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum Kind {
    Expression,
    Attribute,
}

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
    if matches!(value.node, ExprKind::Compare(_)) {
        return;
    }

    // Ignore strings, to avoid false positives with docstrings.
    if matches!(
        value.node,
        ExprKind::JoinedStr(_)
            | ExprKind::Constant(ast::ExprConstant {
                value: Constant::Str(..) | Constant::Ellipsis,
                ..
            })
    ) {
        return;
    }

    // Ignore statements that have side effects.
    if contains_effect(value, |id| checker.ctx.is_builtin(id)) {
        // Flag attributes as useless expressions, even if they're attached to calls or other
        // expressions.
        if matches!(value.node, ExprKind::Attribute(_)) {
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
