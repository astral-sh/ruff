use crate::checkers::ast::Checker;
use crate::rules::ruff::is_final_annotation;
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, Stmt};
use ruff_python_semantic::{Binding, BindingId};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for variables that are assigned once and never reassigned, but are
/// missing a [`typing.Final`][] type annotation.
///
/// ## Why is this bad?
/// Variables that are assigned once and never reassigned can be annotated with
/// [`typing.Final`][] to indicate that they are intended to be constants. This
/// improves maintainability.
///
/// Note: This rule currently does not try to catch more advanced cases of
/// single assignment, like an assignment within a loop iteration that would
/// never occur, or unpacked assignments.
///
/// ## Example
///
/// ```python
/// X = 1
/// for i in range(8):
///     if i == 9:
///         X = 2
/// ```
///
/// and
///
/// ```python
/// def foo():
///     a, b = (1, 2)
/// ```
///
/// do not currently warn, but this could be improved in the future.
///
/// ## Non-ideal
/// ```python
/// X = 1
/// print(X)
/// ```
///
/// ## Fixed
/// ```python
/// from typing import Final
///
/// X: Final = 1
/// print(X)
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.0.0")]
pub(crate) struct SingleAssignmentMissingFinal {
    name: String,
}

impl Violation for SingleAssignmentMissingFinal {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Variable `{}` is assigned once and is missing `Final`",
            self.name
        )
    }
}

/// Check if a binding is part of an unpacking/tuple assignment pattern.
/// This detects cases like: `a, b = (1, 2)` or `(x, y) = some_tuple`,
/// so we can avoid them.
fn is_tuple_assignment_target(binding: &Binding, checker: &Checker) -> bool {
    let Some(source) = binding.source else {
        return false;
    };

    let semantic = checker.semantic();
    let stmt = semantic.statement(source);

    // Check if this is an assignment with tuple/list/set targets
    match stmt {
        Stmt::Assign(assign) => {
            // Any target that is a tuple/list/set indicates unpacking
            assign
                .targets
                .iter()
                .any(|target| matches!(target, Expr::Tuple(_) | Expr::List(_) | Expr::Set(_)))
        }
        _ => false,
    }
}

pub(crate) fn single_assignment_missing_final(
    checker: &Checker,
    binding: &Binding,
    binding_id: BindingId,
) {
    if !binding.kind.is_assignment() {
        return;
    }

    // For simplicity, ignore unpacked assignments for the initial implementation.
    if binding.is_unpacked_assignment() || is_tuple_assignment_target(binding, checker) {
        return;
    }

    let semantic = checker.semantic();
    let scope = &semantic.scopes[binding.scope];
    let name = binding.name(checker.source()).to_string();
    let mut assignment_count = 0usize;

    for id in scope.get_all(name.as_str()) {
        let b = &semantic.bindings[id];
        if b.kind.is_assignment() {
            assignment_count += 1;
        }
        if assignment_count > 1 {
            break;
        }
    }

    if assignment_count != 1 {
        return;
    }

    if let Some(delayed) = semantic.delayed_annotations(binding_id) {
        // For simplicity in the initial implementation, skip delayed annotations.
        if !delayed.is_empty() {
            return;
        }
    }

    // Skip if already annotated with Final
    if let Some(Stmt::AnnAssign(ann_assign)) = binding.statement(semantic) {
        if is_final_annotation(&ann_assign.annotation, semantic) {
            return;
        }
    }

    // Emit a diagnostic suggesting `Final`.
    checker.report_diagnostic(SingleAssignmentMissingFinal { name }, binding.range());
}
