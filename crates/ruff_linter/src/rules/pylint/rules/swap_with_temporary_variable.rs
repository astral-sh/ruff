use itertools::Itertools;
use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_python_ast::name::Name;
use ruff_python_ast::traversal::EnclosingSuite;
use ruff_python_ast::{Stmt, traversal};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::{Ranged, TextRange, TextSize};

use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;
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
    is_temp_var_reused: bool,
}

impl Violation for SwapWithTemporaryVariable<'_> {
    const FIX_AVAILABILITY: crate::FixAvailability = crate::FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let SwapWithTemporaryVariable {
            first_var,
            second_var,
            is_temp_var_reused: _,
        } = self;
        format!("Consider swapping `{first_var}` and `{second_var}` by using tuple unpacking")
    }

    fn fix_title(&self) -> Option<String> {
        let SwapWithTemporaryVariable {
            first_var,
            second_var,
            is_temp_var_reused,
        } = self;

        if !is_temp_var_reused {
            Some(format!(
                "Use `{first_var}, {second_var} = {second_var}, {first_var}` instead"
            ))
        } else {
            None
        }
    }
}

pub(crate) fn swap_with_temporary_variable(checker: &Checker, assignment: &Stmt) {
    let Some(consecutive_assignments) =
        match_consecutive_assignments(assignment, checker.semantic())
    else {
        return;
    };
    for (stmt_a, stmt_b, stmt_c) in consecutive_assignments.tuple_windows() {
        // Detect patterns like:
        // temp = x
        // x = y
        // y = temp
        if stmt_a.value == stmt_b.target
            && stmt_b.value == stmt_c.target
            && stmt_a.target == stmt_c.value
        {
            // check whether there is any later read reference to the temporary variable -
            // in this case the automatic hotfix would result in broken code, because
            // this later read would attempt to read from a variable that no longer exists
            let is_variable_reused_later =
                is_variable_read_after(checker, &stmt_a, stmt_c.range.end());
            let diagnostic = SwapWithTemporaryVariable {
                first_var: stmt_b.target,
                second_var: stmt_c.target,
                is_temp_var_reused: is_variable_reused_later,
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

            // no hotfix available in this case, see explanation above
            if is_variable_reused_later {
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

/// Match consecutive assignment statements.
///
/// Also see the `repeated_append` rule for a similar use case.
fn match_consecutive_assignments<'a>(
    stmt: &'a Stmt,
    semantic: &'a SemanticModel,
) -> Option<impl Iterator<Item = VarToVarAssignment<'a>>> {
    let root_assignment = VarToVarAssignment::from_stmt(stmt)?;

    // In order to match consecutive statements, we need to go to the tree ancestor of the
    // given statement, find its position there, and match all 'appends' from there.
    let suite = if semantic.at_top_level() {
        // If the statement is at the top level, we should go to the parent module.
        // Module is available in the definitions list.
        EnclosingSuite::new(semantic.definitions.python_ast()?, stmt.into())?
    } else {
        // Otherwise, go to the parent, and take its body as a sequence of siblings.
        semantic
            .current_statement_parent()
            .and_then(|parent| traversal::suite(stmt, parent))?
    };

    // We shouldn't repeat the same work for many 'assignments' that go in a row. Let's check
    // that this statement is at the beginning of such a group.
    if suite
        .previous_sibling()
        .is_some_and(|previous_stmt| VarToVarAssignment::from_stmt(previous_stmt).is_some())
    {
        return None;
    }

    Some(
        std::iter::once(root_assignment).chain(
            suite
                .next_siblings()
                .iter()
                .map_while(VarToVarAssignment::from_stmt),
        ),
    )
}

#[derive(Eq, PartialEq, Debug, Clone)]
struct VarToVarAssignment<'a> {
    target: &'a Name,
    value: &'a Name,
    range: TextRange,
}

impl<'a> VarToVarAssignment<'a> {
    fn from_stmt(stmt: &'a Stmt) -> Option<VarToVarAssignment<'a>> {
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
        if let (Some(target_expr), Some(value_expr)) = (target.as_name_expr(), value.as_name_expr())
        {
            Some(Self {
                target: &target_expr.id,
                value: &value_expr.id,
                range: stmt.range(),
            })
        } else {
            None
        }
    }
}

/// Check whether a variable is read after a given position.
///
/// Returns `true` if the variable assigned to in `variable_assignment` is read anywhere after `after_position`.
fn is_variable_read_after(
    checker: &Checker,
    variable_assignment: &VarToVarAssignment,
    after_position: TextSize,
) -> bool {
    // Get the variable binding for the variable assigned to in this statement,
    // e.g., in the example `a = b` this would be the binding to the variable `a`.
    let variable_binding = checker
        .semantic()
        .bindings
        .iter()
        .find(|binding| variable_assignment.range.contains_range(binding.range))
        .unwrap();

    // If the variable is global (e.g., `global VARNAME`) or nonlocal (e.g., `nonlocal VARNAME`),
    // then it is intended to also be used elsewhere outside our scope and hence it's likely
    // to be used in other contexts as well.
    if variable_binding.is_global() || variable_binding.is_nonlocal() {
        return true;
    }

    // Check if there's any read reference to the variable in the consecutive statements after
    // the provided `after_position`.
    if variable_binding
        .references()
        .map(|reference| checker.semantic().reference(reference))
        .any(|other_reference| after_position < other_reference.start())
    {
        return true;
    }

    false
}
