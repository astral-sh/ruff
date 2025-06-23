use ruff_python_ast::{self as ast, Expr};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::Name;
use ruff_text_size::Ranged;

use crate::Violation;
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
#[derive(ViolationMetadata)]
pub(crate) struct RedeclaredAssignedName {
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
pub(crate) fn redeclared_assigned_name(checker: &Checker, targets: &Vec<Expr>) {
    let mut names: Vec<Name> = Vec::new();

    for target in targets {
        check_expr(checker, target, &mut names);
    }
}

fn check_expr(checker: &Checker, expr: &Expr, names: &mut Vec<Name>) {
    match expr {
        Expr::Tuple(tuple) => {
            for target in tuple {
                check_expr(checker, target, names);
            }
        }
        Expr::List(list) => {
            for target in list {
                check_expr(checker, target, names);
            }
        }
        Expr::Starred(starred) => {
            check_expr(checker, &starred.value, names);
        }
        Expr::Name(ast::ExprName { id, .. }) => {
            if checker.settings().dummy_variable_rgx.is_match(id) {
                // Ignore dummy variable assignments
                return;
            }
            if names.contains(id) {
                checker.report_diagnostic(
                    RedeclaredAssignedName {
                        name: id.to_string(),
                    },
                    expr.range(),
                );
            }
            names.push(id.clone());
        }
        _ => {}
    }
}
