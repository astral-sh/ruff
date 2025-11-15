use crate::checkers::ast::Checker;
use crate::rules::ruff::is_final_annotation;
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, Stmt};
use ruff_python_semantic::{Binding, BindingId};
use ruff_text_size::Ranged;
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

/// Conservative prototype: warn when a binding is a single assignment in its scope and
/// has no Final annotation. The implementation is intentionally conservative and only
/// considers simple assignment bindings.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.0.0")]
pub(crate) struct SingleAssignmentMissingFinal {
    name: String,
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
    // Conservative checks
    // Only assignments
    if !binding.kind.is_assignment() {
        return;
    }

    if binding.is_global() || binding.is_nonlocal() {
        return;
    }

    // Skip unpacking assignments like: a, b = (1, 2)
    // The is_unpacked_assignment() method doesn't catch all cases,
    // so we also check if the binding is within a tuple target
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
        // If any delayed annotation exists, skip (conservative)
        if !delayed.is_empty() {
            return;
        }
    }

    // Skip if already annotated with Final
    if let Some(stmt) = binding.statement(semantic) {
        if let Stmt::AnnAssign(ann_assign) = stmt {
            if is_final_annotation(&ann_assign.annotation, semantic) {
                return;
            }
        }
    }

    // For a conservative prototype we don't attempt to analyze the RHS immutability.
    // Emit a diagnostic suggesting `Final`.
    checker.report_diagnostic(SingleAssignmentMissingFinal { name }, binding.range());
}
