use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for declared assignments to the same variable multiple times
/// in the same assignment.
///
/// ## Why is this bad?
/// Assigning a variable multiple times in the same assignment is redundant,
/// as the final assignment to the variable is what the value will be.
///
/// ## Example
/// ```python
/// a, b, a = (1, 2, 3)
/// print(a)  # 3
/// ```
///
/// Use instead:
/// ```python
/// # this is assuming you want to assign 3 to `a`
/// _, b, a = (1, 2, 3)
/// print(a)  # 3
/// ```
///
#[violation]
pub struct RedeclaredAssignedName {
    name: String,
}

impl Violation for RedeclaredAssignedName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedeclaredAssignedName { name } = self;
        format!("Redeclared variable `{name}` in assignment")
    }
}

/// PLW0128
pub(crate) fn redeclared_assigned_name(checker: &mut Checker, targets: &Vec<Expr>) {
    let mut names: Vec<String> = Vec::new();

    for target in targets {
        check_expr(checker, target, &mut names);
    }
}

fn check_expr(checker: &mut Checker, expr: &Expr, names: &mut Vec<String>) {
    match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            for target in elts {
                check_expr(checker, target, names);
            }
        }
        Expr::Name(ast::ExprName { id, .. }) => {
            if checker.settings.dummy_variable_rgx.is_match(id) {
                // Ignore dummy variable assignments
                return;
            }
            if names.contains(id) {
                checker.diagnostics.push(Diagnostic::new(
                    RedeclaredAssignedName {
                        name: id.to_string(),
                    },
                    expr.range(),
                ));
            }
            names.push(id.to_string());
        }
        _ => {}
    }
}
