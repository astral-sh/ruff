use itertools::Itertools;
use ruff_python_ast::Expr;
use ruff_python_ast::flat;
use ruff_python_ast::flat::fields as ast;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.281", category = Category::Suspicious)]
pub(crate) struct SelfAssigningVariable {
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
pub(crate) fn self_assignment(checker: &Checker, assign: &ast::StmtAssign<'_>) {
    // Assignments in class bodies are attributes (e.g., `x = x` assigns `x` to `self.x`, and thus
    // is not a self-assignment).
    if checker.semantic().current_scope().kind.is_class() {
        return;
    }

    for [left, right] in assign
        .targets
        .iter()
        .chain(std::iter::once(assign.value))
        .array_combinations()
    {
        visit_assignments(checker, left, right);
    }
}

/// PLW0127
pub(crate) fn self_annotated_assignment(checker: &Checker, assign: &ast::StmtAnnAssign<'_>) {
    let Some(value) = assign.value else {
        return;
    };

    // Assignments in class bodies are attributes (e.g., `x = x` assigns `x` to `self.x`, and thus
    // is not a self-assignment).
    if checker.semantic().current_scope().kind.is_class() {
        return;
    }

    visit_assignments(checker, assign.target, value);
}

fn visit_assignments(checker: &Checker, left: flat::View<'_, Expr>, right: flat::View<'_, Expr>) {
    match (left.classify(), right.classify()) {
        (flat::ExprKind::Tuple(lhs), flat::ExprKind::Tuple(rhs))
            if lhs.elts().len() == rhs.elts().len() =>
        {
            for (lhs_elem, rhs_elem) in lhs.iter().zip(rhs) {
                visit_assignments(checker, lhs_elem, rhs_elem);
            }
        }
        (flat::ExprKind::Name(lhs), flat::ExprKind::Name(rhs)) if lhs.id() == rhs.id() => {
            checker.report_diagnostic(
                SelfAssigningVariable {
                    name: lhs.id().to_string(),
                },
                left.range(),
            );
        }
        _ => {}
    }
}
