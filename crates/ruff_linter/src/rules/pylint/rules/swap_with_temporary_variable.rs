use itertools::Itertools;
use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_python_ast::Stmt;
use ruff_python_ast::name::Name;
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::AlwaysFixableViolation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for code that swaps two variables using a temporary variable.
///
/// ## Why is this bad?
/// Variables can be swapped by using tuple unpacking instead of using a
/// temporary variable. That also makes the intention of the swapping logic
/// more clear.
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
#[violation_metadata(preview_since = "0.14.11")]
pub(crate) struct SwapWithTemporaryVariable<'a> {
    first_var: &'a Name,
    second_var: &'a Name,
}

impl AlwaysFixableViolation for SwapWithTemporaryVariable<'_> {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SwapWithTemporaryVariable {
            first_var,
            second_var,
        } = self;
        format!("Consider swapping `{first_var}` and `{second_var}` by using tuple unpacking")
    }

    fn fix_title(&self) -> String {
        let SwapWithTemporaryVariable {
            first_var,
            second_var,
        } = self;
        format!("Use `{first_var}, {second_var} = {second_var}, {first_var}` instead")
    }
}

pub(crate) fn swap_with_temporary_variable(checker: &Checker, stmts: &[Stmt]) {
    for stmt_sequence in stmts
        .iter()
        .map(VarToVarAssignment::from_stmt)
        .tuple_windows()
    {
        // if unwrapping fails, one of the statements hasn't been a var to var assignment
        let (Some(stmt_a), Some(stmt_b), Some(stmt_c)) = stmt_sequence else {
            continue;
        };

        // Detect patterns like:
        // temp = x
        // x = y
        // y = temp
        if stmt_a.value == stmt_b.target
            && stmt_b.value == stmt_c.target
            && stmt_a.target == stmt_c.value
        {
            let diagnostic = SwapWithTemporaryVariable {
                first_var: &stmt_b.target,
                second_var: &stmt_c.target,
            };
            let edit_range = TextRange::new(stmt_a.range.start(), stmt_c.range.end());
            let edit = Edit::range_replacement(
                format!(
                    "{0}, {1} = {1}, {0}",
                    &diagnostic.first_var, &diagnostic.second_var
                ),
                edit_range,
            );
            let mut diagnostic_guard = checker.report_diagnostic(diagnostic, edit_range);

            // Get the variable binding of the temporary variable that's used to swap the variables,
            // e.g. in the example above, this would be the `temp` variable.
            let temporary_variable_binding = checker
                .semantic()
                .bindings
                .iter()
                .find(|binding| stmt_a.range.contains_range(binding.range))
                .unwrap();

            // If the temporary variable is global (e.g., `global SWAP_VAR`) or nonlocal (e.g., `nonlocal SWAP_VAR`),
            // then it is intended to also be used elsewhere outside our scope and hence can not be easily removed
            // by applying a quick fix.
            if temporary_variable_binding.is_global() || temporary_variable_binding.is_nonlocal() {
                continue;
            }

            // In case there's any later reference to the temporary variable, the quick fix would also not be applicable
            // because it would remove the temporary variable declaration, but not its use later in the code.
            if temporary_variable_binding
                .references()
                .map(|reference| checker.semantic().reference(reference))
                .any(|other_reference| edit_range.end() < other_reference.start())
            {
                continue;
            }

            // The quick fix would remove comments, hence it's unsafe if there are any comments in the relevant code part.
            let applicability = if checker.comment_ranges().intersects(edit.range()) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            };
            diagnostic_guard.set_fix(Fix::applicable_edit(edit, applicability));
        }
    }
}

#[derive(Eq, PartialEq, Debug, Clone)]
struct VarToVarAssignment {
    target: Name,
    value: Name,
    range: TextRange,
}

impl VarToVarAssignment {
    fn from_stmt(stmt: &Stmt) -> Option<VarToVarAssignment> {
        let (target, value) = match stmt {
            Stmt::Assign(stmt_assign) => {
                // only one variable is expected for matching the pattern
                let [target_variable] = stmt_assign.targets.as_slice() else {
                    return None;
                };

                (target_variable, &stmt_assign.value)
            }
            Stmt::AnnAssign(stmt_ann_assign) => {
                // only assignments that actually assign a value are relevant here
                let Some(value) = &stmt_ann_assign.value else {
                    return None;
                };

                (&*stmt_ann_assign.target, value)
            }
            // Stmt::AugAssign is not relevant because it modifies the content
            // of a variable based on its existing value, so it can't swap variables
            _ => return None,
        };

        // assignment value is more complex than just a simple variable, skip such cases.
        if let (Some(target_expr), Some(value_expr)) =
            (target.clone().name_expr(), value.clone().name_expr())
        {
            Some(Self {
                target: target_expr.id,
                value: value_expr.id,
                range: stmt.range(),
            })
        } else {
            None
        }
    }
}
