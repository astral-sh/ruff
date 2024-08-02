use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::ExprSubscript;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for use or omission of parentheses around tuples of length at least two in calls to `__getitem__`,
/// depending on the setting [`lint.ruff.prefer-parentheses-getitem-tuple`]. By default, the use of parentheses
/// is considered a violation.
///
/// ## Why is this bad?
/// It is good to be consistent and, depending on the codebase, one or the other
/// convention may be more readable.
///
/// ## Example
///
/// ```python
/// directions = {(0, 1): "North", (-1, 0): "East", (0, -1): "South", (1, 0): "West"}
/// directions[(0, 1)]
/// ```
///
/// Use instead (with default setting):
///
/// ```python
/// directions = {(0, 1): "North", (-1, 0): "East", (0, -1): "South", (1, 0): "West"}
/// directions[0, 1]
/// ```

#[violation]
pub struct ParenthesesInTupleSlices {
    prefer_parentheses: bool,
}

impl AlwaysFixableViolation for ParenthesesInTupleSlices {
    #[derive_message_formats]
    fn message(&self) -> String {
        if self.prefer_parentheses {
            format!("Use preferred format when evaluating `__getitem__` at a tuple.")
        } else {
            format!("Use preferred format when evaluating `__getitem__` at a tuple.")
        }
    }

    fn fix_title(&self) -> String {
        "Remove parentheses from tuple argument in `__getitem__` call.".to_string()
    }
}

/// RUF031
pub(crate) fn getitem_with_parenthesized_tuple(checker: &mut Checker, subscript: &ExprSubscript) {
    if let Some(tuple_index) = subscript.slice.as_tuple_expr() {
        if tuple_index.parenthesized && tuple_index.elts.len() > 1 {
            let locator = checker.locator();
            let source_range = subscript.slice.range();
            let new_source = format!("{}", locator.slice(source_range));
            let edit = Edit::range_replacement(
                new_source[1..new_source.len() - 1].to_string(),
                source_range,
            );
            checker.diagnostics.push(
                Diagnostic::new(ParenthesesInTupleSlices, source_range)
                    .with_fix(Fix::safe_edit(edit)),
            );
        }
    }
}
