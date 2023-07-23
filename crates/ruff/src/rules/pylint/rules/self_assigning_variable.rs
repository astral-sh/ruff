use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

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
pub(crate) fn self_assigning_variable(checker: &mut Checker, targets: &[Expr], value: &Expr) {
    let [target] = targets else {
        return;
    };
    match (target, value) {
        (
            Expr::Tuple(ast::ExprTuple { elts: lhs_elts, .. }),
            Expr::Tuple(ast::ExprTuple { elts: rhs_elts, .. }),
        ) if lhs_elts.len() == rhs_elts.len() => {
            lhs_elts
                .iter()
                .zip(rhs_elts.iter())
                .for_each(|(lhs, rhs)| self_assigning_variable(checker, &[lhs.clone()], rhs));
        }
        (
            Expr::Name(ast::ExprName { id: lhs_name, .. }),
            Expr::Name(ast::ExprName { id: rhs_name, .. }),
        ) if lhs_name == rhs_name => {
            checker.diagnostics.push(Diagnostic::new(
                SelfAssigningVariable {
                    name: lhs_name.to_string(),
                },
                target.range(),
            ));
        }
        _ => {}
    }
}
