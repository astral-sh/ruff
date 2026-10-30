use ruff_python_ast::flat;
use ruff_python_ast::flat::fields;
use rustc_hash::{FxBuildHasher, FxHashMap};

use ruff_diagnostics::{Applicability, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Detects duplicate elements in `__all__` definitions.
///
/// ## Why is this bad?
/// Duplicate elements in `__all__` serve no purpose and can indicate copy-paste errors or
/// incomplete refactoring.
///
/// ## Example
/// ```python
/// __all__ = [
///     "DatabaseConnection",
///     "Product",
///     "User",
///     "DatabaseConnection",  # Duplicate
/// ]
/// ```
///
/// Use instead:
/// ```python
/// __all__ = [
///     "DatabaseConnection",
///     "Product",
///     "User",
/// ]
/// ```
///
/// ## Fix Safety
/// This rule's fix is marked as unsafe if the replacement would remove comments attached to the
/// original expression, potentially losing important context or documentation.
///
/// For example:
/// ```python
/// __all__ = [
///     "PublicAPI",
///     # TODO: Remove this in v2.0
///     "PublicAPI",  # Deprecated alias
/// ]
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.16.0", category = Category::Correctness)]
pub(crate) struct DuplicateEntryInDunderAll;

impl Violation for DuplicateEntryInDunderAll {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`__all__` contains duplicate entries".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove duplicate entries from `__all__`".to_string())
    }
}

/// Apply RUF068 to `StmtAssign` AST node. For example: `__all__ = ["a", "b", "a"]`.
pub(crate) fn duplicate_entry_in_dunder_all_assign(
    checker: &Checker,
    stmt: &fields::StmtAssign<'_>,
) {
    let value = stmt.value;
    if let Some([expr]) = stmt.targets.as_array() {
        duplicate_entry_in_dunder_all(checker, expr, value);
    }
}

/// Apply RUF068 to `StmtAugAssign` AST node. For example: `__all__ += ["a", "b", "a"]`.
pub(crate) fn duplicate_entry_in_dunder_all_aug_assign(
    checker: &Checker,
    node: &fields::StmtAugAssign<'_>,
) {
    if node.op.is_add() {
        duplicate_entry_in_dunder_all(checker, node.target, node.value);
    }
}

/// Apply RUF068 to `__all__.extend()`.
pub(crate) fn duplicate_entry_in_dunder_all_extend_call(
    checker: &Checker,
    call: &fields::ExprCall<'_>,
) {
    let func = call.func;
    let (Some([value_passed]), true) = (
        call.arguments.args().as_array(),
        call.arguments.keywords().is_empty(),
    ) else {
        return;
    };
    let flat::ExprKind::Attribute(attribute) = func.classify() else {
        return;
    };
    let value = attribute.value();
    let attr = attribute.attr();
    if attr == "extend" {
        duplicate_entry_in_dunder_all(checker, value, value_passed);
    }
}

/// Apply RUF068 to a `StmtAnnAssign` AST node.
/// For example: `__all__: list[str] = ["a", "b", "a"]`.
pub(crate) fn duplicate_entry_in_dunder_all_ann_assign(
    checker: &Checker,
    node: &fields::StmtAnnAssign<'_>,
) {
    if let Some(value) = node.value {
        duplicate_entry_in_dunder_all(checker, node.target, value);
    }
}

/// RUF068
/// This routine checks whether `__all__` contains duplicated entries, and emits
/// a violation if it does.
fn duplicate_entry_in_dunder_all(
    checker: &Checker,
    target: flat::View<'_, ast::Expr>,
    value: flat::View<'_, ast::Expr>,
) {
    let flat::ExprKind::Name(name_node) = target.classify() else {
        return;
    };
    let id = name_node.id();

    if id != "__all__" {
        return;
    }

    // We're only interested in `__all__` in the global scope
    if !checker.semantic().current_scope().kind.is_module() {
        return;
    }

    let elts = match value.classify() {
        flat::ExprKind::List(expr_list) => expr_list.elts(),
        flat::ExprKind::Tuple(expr_tuple) => expr_tuple.elts(),
        _ => return,
    };

    // It's impossible to have duplicates if there is one or no element
    if elts.len() <= 1 {
        return;
    }

    let mut deduplicated_elts = FxHashMap::with_capacity_and_hasher(elts.len(), FxBuildHasher);
    let source = checker.locator().contents();

    for (index, expr) in elts.iter().enumerate() {
        let Some(string_value) = expr.as_string_literal_expr() else {
            // In the example below we're ignoring `foo`:
            // __all__ = [foo, "bar", "bar"]
            continue;
        };

        let name = string_value.value().to_str();

        if let Some(previous_expr) = deduplicated_elts.insert(name, expr) {
            let mut diagnostic = checker.report_diagnostic(DuplicateEntryInDunderAll, expr.range());

            diagnostic.secondary_annotation(
                format_args!("previous occurrence of `{name}` here"),
                previous_expr,
            );

            diagnostic.set_primary_annotation_message(format_args!("`{name}` duplicated here"));

            diagnostic.try_set_fix(|| {
                edits::remove_member(elts, index, source).map(|edit| {
                    let applicability = if checker.comment_ranges().intersects(edit.range()) {
                        Applicability::Unsafe
                    } else {
                        Applicability::Safe
                    };
                    Fix::applicable_edit(edit, applicability)
                })
            });
        }
    }
}
