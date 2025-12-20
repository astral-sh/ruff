use std::collections::HashSet;

use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Detects duplicate elements in `__all__` definitions.
///
/// ## Why is this bad?
/// Duplicate elements are usually mistakes.
///
/// ## Example
/// ```python
/// import sys
///
/// __all__ = [
///     "a",
///     "a",
///     "b",
/// ]
///
/// Use instead:
/// ```python
/// import sys
///
/// __all__ = [
///     "a",
///     "b",
/// ]
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.10")]
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

/// RUF069
/// This routine checks whether `__all__` contains duplicated entries, and emits
/// a violation if it does.
pub(crate) fn duplicate_entry_in_dunder_all(
    checker: &Checker,
    ast::StmtAssign { value, targets, .. }: &ast::StmtAssign,
) {
    let [target] = targets.as_slice() else { return };
    let ast::Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    if id != "__all__" {
        return;
    }

    // We're only interested in `__all__` in the global scope
    if !checker.semantic().current_scope().kind.is_module() {
        return;
    }

    let elts = match value.as_ref() {
        ast::Expr::List(ast::ExprList { elts, .. }) => elts,
        ast::Expr::Tuple(ast::ExprTuple { elts, .. }) => elts,
        _ => return,
    };

    // It's impossible to have duplicates if there is one or no element
    if elts.len() <= 1 {
        return;
    }

    let mut deduplicated_elts = HashSet::with_capacity(elts.len());
    let source = checker.locator().contents();

    for expr in elts {
        let Some(string_value) = expr.as_string_literal_expr() else {
            // If any elt we encounter is not an ExprStringLiteral AST value, that indicates at least
            // one item in the sequence is not a string literal, which means the sequence is out of
            // scope for RUF069.
            return;
        };

        if !deduplicated_elts.insert(string_value.value.to_str()) {
            let range = expr.range();
            let mut diagnostic = checker.report_diagnostic(DuplicateEntryInDunderAll, range);

            let leading_len: TextSize = source[..range.start().to_usize()]
                .chars()
                .rev()
                .take_while(|c| c.is_whitespace() || *c == ',')
                .map(TextLen::text_len)
                .sum();

            let fix_range = TextRange::new(range.start() - leading_len, range.end());

            let edit = Edit::range_deletion(fix_range);

            if checker.comment_ranges().intersects(fix_range) {
                diagnostic.set_fix(Fix::unsafe_edit(edit));
            } else {
                diagnostic.set_fix(Fix::safe_edit(edit));
            }
        }
    }
}
