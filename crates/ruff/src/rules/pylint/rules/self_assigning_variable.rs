use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for self-assignment of variables.
///
/// ## Why is this bad?
/// Self-assignment of variables is redundant and likely a mistake.
///
/// ## Example
/// ```python
/// country = "Poland"
/// country = country
/// ```
///
/// Use instead:
/// ```python
/// country = "Poland"
/// ```
#[violation]
pub struct SelfAssigningVariable {
    name: String,
}

impl Violation for SelfAssigningVariable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SelfAssigningVariable { name } = self;
        format!("Self-assignment of variable `{name}`")
    }
}

/// PLW0127
pub(crate) fn self_assigning_variable(checker: &mut Checker, target: &Expr, value: &Expr) {
    fn inner(left: &Expr, right: &Expr, diagnostics: &mut Vec<Diagnostic>) {
        match (left, right) {
            (
                Expr::Tuple(ast::ExprTuple { elts: lhs_elts, .. }),
                Expr::Tuple(ast::ExprTuple { elts: rhs_elts, .. }),
            ) if lhs_elts.len() == rhs_elts.len() => lhs_elts
                .iter()
                .zip(rhs_elts.iter())
                .for_each(|(lhs, rhs)| inner(lhs, rhs, diagnostics)),
            (
                Expr::Name(ast::ExprName { id: lhs_name, .. }),
                Expr::Name(ast::ExprName { id: rhs_name, .. }),
            ) if lhs_name == rhs_name => {
                diagnostics.push(Diagnostic::new(
                    SelfAssigningVariable {
                        name: lhs_name.to_string(),
                    },
                    left.range(),
                ));
            }
            _ => {}
        }
    }

    // Assignments in class bodies are attributes (e.g., `x = x` assigns `x` to `self.x`, and thus
    // is not a self-assignment).
    if checker.semantic().current_scope().kind.is_class() {
        return;
    }

    inner(target, value, &mut checker.diagnostics);
}
