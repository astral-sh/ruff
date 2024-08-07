// In case we want to change the boolean setting for
// this rule to an enum, this will make the code change
// just a little simpler.
#![allow(clippy::match_bool)]

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::ExprSubscript;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for use or omission of parentheses around tuples in subscripts,
/// depending on the setting [`lint.ruff.parenthesize-tuple-in-getitem`]. By default, the use of parentheses
/// is considered a violation.
///
/// ## Why is this bad?
/// It is good to be consistent and, depending on the codebase, one or the other
/// convention may be preferred.
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
pub struct IncorrectlyParenthesizedTupleInSubscript {
    prefer_parentheses: bool,
}

impl AlwaysFixableViolation for IncorrectlyParenthesizedTupleInSubscript {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.prefer_parentheses {
            true => format!("Use parentheses for tuples in subscripts."),
            false => format!("Avoid parentheses for tuples in scubscripts."),
        }
    }

    fn fix_title(&self) -> String {
        match self.prefer_parentheses {
            true => "Add parentheses around tuple in subscript.".to_string(),
            false => "Remove parentheses from tuple in subscript.".to_string(),
        }
    }
}

/// RUF031
pub(crate) fn subscript_with_parenthesized_tuple(checker: &mut Checker, subscript: &ExprSubscript) {
    let prefer_parentheses = checker.settings.ruff.parenthesize_tuple_in_subscript;
    let Some(tuple_index) = subscript.slice.as_tuple_expr() else {
        return;
    };
    if tuple_index.parenthesized != prefer_parentheses {
        let locator = checker.locator();
        let source_range = subscript.slice.range();
        let new_source = match prefer_parentheses {
            true => {
                format!("({})", locator.slice(source_range))
            }
            false => locator.slice(source_range)[1..source_range.len().to_usize() - 1].to_string(),
        };
        let edit = Edit::range_replacement(new_source, source_range);
        checker.diagnostics.push(
            Diagnostic::new(
                IncorrectlyParenthesizedTupleInSubscript { prefer_parentheses },
                source_range,
            )
            .with_fix(Fix::safe_edit(edit)),
        );
    }
}
