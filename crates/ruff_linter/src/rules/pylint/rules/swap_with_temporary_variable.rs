use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_python_ast::name::Name;
use ruff_python_ast::{Stmt, traversal};
use ruff_python_semantic::{BindingId, Scope, ScopeId, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

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
/// The rule's fix is marked as safe, unless the replacement range contains comments
/// that would be removed.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct SwapWithTemporaryVariable<'a> {
    first: &'a Name,
    second: &'a Name,
}

impl Violation for SwapWithTemporaryVariable<'_> {
    const FIX_AVAILABILITY: crate::FixAvailability = crate::FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary temporary variable".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let SwapWithTemporaryVariable { first, second } = self;

        Some(format!(
            "Use `{first}, {second} = {second}, {first}` instead"
        ))
    }
}

pub(crate) fn swap_with_temporary_variable(checker: &Checker, scope_id: ScopeId, scope: &Scope) {
    let consecutive_assignments = scope.binding_ids().filter_map(|binding_id| {
        match_consecutive_assignments(checker.semantic(), scope_id, binding_id)
    });

    for (stmt_a, stmt_b, stmt_c) in consecutive_assignments {
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
            let is_variable_reused_later = is_variable_read_after(checker, &stmt_a);
            if is_variable_reused_later {
                continue;
            }

            let first = stmt_b.target;
            let second = stmt_c.target;
            let edit_range = TextRange::new(stmt_a.start(), stmt_c.end());
            let edit = Edit::range_replacement(
                format!("{first}, {second} = {second}, {first}"),
                edit_range,
            );

            // The quick fix would remove comments, hence it's unsafe if there are any comments in the relevant code part.
            let applicability = if checker.comment_ranges().intersects(edit.range()) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            };

            checker
                .report_diagnostic(SwapWithTemporaryVariable { first, second }, edit_range)
                .set_fix(Fix::applicable_edit(edit, applicability));
        }
    }
}

/// Match consecutive assignment statements.
///
/// Also see the `repeated_append` rule for a similar use case.
fn match_consecutive_assignments<'a>(
    semantic: &'a SemanticModel<'a>,
    scope_id: ScopeId,
    binding_id: BindingId,
) -> Option<(
    VarToVarAssignment<'a>,
    VarToVarAssignment<'a>,
    VarToVarAssignment<'a>,
)> {
    let binding = semantic.binding(binding_id);

    // Only consider simple assignments (no imports, function defs, etc.)
    if !binding.kind.is_assignment() {
        return None;
    }

    let node_id = binding.source?;

    let stmt = binding.statement(semantic)?;
    let stmt_a = VarToVarAssignment::from_stmt(stmt)?;

    // Find the enclosing suite so we can look at the next siblings.
    // For the global scope, use the module body; otherwise, find the parent statement.
    let suite = if scope_id.is_global() {
        traversal::EnclosingSuite::new(semantic.definitions.python_ast()?, stmt.into())
    } else {
        traversal::suite(stmt, semantic.parent_statement(node_id)?)
    }?;

    let stmt_b = VarToVarAssignment::from_stmt(suite.next_sibling()?)?;
    let stmt_c = VarToVarAssignment::from_stmt(suite.next_siblings().get(1)?)?;

    Some((stmt_a, stmt_b, stmt_c))
}

#[derive(Eq, PartialEq, Debug, Clone)]
struct VarToVarAssignment<'a> {
    target: &'a Name,
    value: &'a Name,
    range: TextRange,
}

impl Ranged for VarToVarAssignment<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
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
/// Returns `true` if the variable assigned to in `variable_assignment` is read anywhere other than the swap statement.
fn is_variable_read_after(checker: &Checker, variable_assignment: &VarToVarAssignment) -> bool {
    // Get the variable binding for the variable assigned to in this statement,
    // e.g., in the example `a = b` this would be the binding to the variable `a`.
    let Some(variable_binding) = checker
        .semantic()
        .bindings
        .iter()
        .find(|binding| variable_assignment.range.contains_range(binding.range))
    else {
        return true;
    };

    // If the variable is global (e.g., `global VARNAME`) or nonlocal (e.g., `nonlocal VARNAME`),
    // then it is intended to also be used elsewhere outside our scope and hence it's likely
    // to be used in other contexts as well.
    if variable_binding.is_global() || variable_binding.is_nonlocal() {
        return true;
    }

    // Check if there's any read reference to the variable other than the one from the swap statement
    // We already confirmed that there is at least one reference (i.e. `y = temp`), so the variable is
    // only re-used if there is any other reference than this one (i.e. reference count > 1).
    variable_binding.references().count() > 1
}
