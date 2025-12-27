use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_python_ast::Stmt;
use ruff_python_ast::name::Name;
use ruff_text_size::{Ranged, TextRange, TextSize};

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::checkers::ast::Checker;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for code that swaps two variables using a temporary variable.
///
/// ## Why is this bad?
/// Variables can be swapped by using "tuple unpacking" instead of using a temporary variable. That also makes the intention of the swapping logic more clear.
///
/// ## Example
/// ```python
/// def function(x, y):
///     if x > y:
///         temp = x
///         x = y
///         y = temp
///     assert x <= y
/// ```
///
/// Use instead:
/// ```python
/// def function(x, y):
///     if x > y:
///         x, y = y, x
///     assert x <= y
/// ```
///
/// ## Fix safety
/// The rule's fix is marked as safe, unless it contains comments. In this
/// exception case, applying the quick fix would remove comments between the
/// assignment statements.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.0.0")]
pub(crate) struct ConsiderSwapVariables {
    var_a_name: Name,
    var_b_name: Name,
}

#[derive(Eq, PartialEq, Debug, Clone)]
struct VarToVarAssignment {
    target_var_name: Name,
    value_var_name: Name,
    start: TextSize,
    end: TextSize,
}

impl Violation for ConsiderSwapVariables {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            r#"Consider swapping `{first_var}` and `{second_var}` using "tuple unpacking": `{first_var}, {second_var} = {second_var}, {first_var}`"#,
            first_var = self.var_a_name,
            second_var = self.var_b_name
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Swap variables via tuple unpacking".to_string())
    }
}

pub(crate) fn consider_swap_variables(checker: &Checker, stmts: &[Stmt]) {
    for stmt_sequence in stmts.windows(3) {
        let stmt_sequence = stmt_sequence
            .iter()
            .filter_map(var_to_var_assignment)
            .collect::<Vec<_>>();

        // if unwrapping fails, one of the statements hasn't been a var to var assignment
        let [stmt_a, stmt_b, stmt_c] = stmt_sequence.as_slice() else {
            continue;
        };

        // Detect patterns like:
        // temp = x
        // x = y
        // y = temp
        if stmt_a.value_var_name == stmt_b.target_var_name
            && stmt_b.value_var_name == stmt_c.target_var_name
            && stmt_a.target_var_name == stmt_c.value_var_name
        {
            let diagnostic = ConsiderSwapVariables {
                var_a_name: stmt_b.target_var_name.clone(),
                var_b_name: stmt_c.target_var_name.clone(),
            };
            let edit = Edit::replacement(
                format!(
                    "{0}, {1} = {1}, {0}",
                    &diagnostic.var_a_name, &diagnostic.var_b_name
                ),
                stmt_a.start,
                stmt_c.end,
            );
            let mut diagnostic_guard =
                checker.report_diagnostic(diagnostic, TextRange::new(stmt_a.start, stmt_c.end));

            // the quick fix would remove comments, hence it's unsafe
            let applicability = if checker.comment_ranges().intersects(edit.range()) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            };
            diagnostic_guard.set_fix(Fix::applicable_edit(edit, applicability));
        }
    }
}

fn var_to_var_assignment(stmt: &Stmt) -> Option<VarToVarAssignment> {
    let (target, value) = match stmt {
        Stmt::Assign(stmt_assign) => {
            // only one variable is expected for matching the pattern
            let [target_variable] = stmt_assign.targets.as_slice() else {
                return None;
            };

            (target_variable.clone(), stmt_assign.value.clone())
        }
        Stmt::AnnAssign(stmt_ann_assign) => {
            // only assignments that actually assign a value are relevant here
            let Some(value) = &stmt_ann_assign.value else {
                return None;
            };

            ((*stmt_ann_assign.target).clone(), value.clone())
        }
        // Stmt::AugAssign is not relevant because it modifies the content
        // of a variable based on its existing value, so it can't swap variables
        _ => return None,
    };

    // assignment value is more complex than just a simple variable, skip such cases.
    if !target.is_name_expr() || !value.is_name_expr() {
        return None;
    }

    Some(VarToVarAssignment {
        target_var_name: target.expect_name_expr().id,
        value_var_name: value.expect_name_expr().id,
        start: stmt.start(),
        end: stmt.end(),
    })
}
