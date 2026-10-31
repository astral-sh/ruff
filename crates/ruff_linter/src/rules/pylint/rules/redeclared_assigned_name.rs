use ruff_python_ast::Expr;
use ruff_python_ast::flat;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::Name;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
/// ## Options
///
/// The rule ignores assignments to dummy variables, as specified by:
///
/// - `lint.dummy-variable-rgx`
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.5.0", category = Category::Correctness)]
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
pub(crate) fn redeclared_assigned_name(checker: &Checker, targets: flat::NodeList<'_, Expr>) {
    let mut names: Vec<Name> = Vec::new();

    for target in targets {
        check_expr(checker, target, &mut names);
    }
}

fn check_expr(checker: &Checker, expr: flat::View<'_, Expr>, names: &mut Vec<Name>) {
    match expr.classify() {
        flat::ExprKind::Tuple(tuple) => {
            for target in tuple.elts() {
                check_expr(checker, target, names);
            }
        }
        flat::ExprKind::List(list) => {
            for target in list.elts() {
                check_expr(checker, target, names);
            }
        }
        flat::ExprKind::Starred(starred) => {
            check_expr(checker, starred.value(), names);
        }
        flat::ExprKind::Name(expr_name) => {
            let id = expr_name.id();

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
