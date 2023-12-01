use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use super::super::helpers::at_last_top_level_expression_in_cell;

/// ## What it does
/// Checks for useless comparisons.
///
/// ## Why is this bad?
/// Useless comparisons have no effect on the program, and are often included
/// by mistake. If the comparison is intended to enforce an invariant, prepend
/// the comparison with an `assert`. Otherwise, remove it entirely.
///
/// ## Example
/// ```python
/// foo == bar
/// ```
///
/// Use instead:
/// ```python
/// assert foo == bar, "`foo` and `bar` should be equal."
/// ```
///
/// ## References
/// - [Python documentation: `assert` statement](https://docs.python.org/3/reference/simple_stmts.html#the-assert-statement)
#[violation]
pub struct UselessComparison;

impl Violation for UselessComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Pointless comparison. Did you mean to assign a value? \
             Otherwise, prepend `assert` or remove it."
        )
    }
}

/// B015
pub(crate) fn useless_comparison(checker: &mut Checker, expr: &Expr) {
    if expr.is_compare_expr() {
        // For Jupyter Notebooks, ignore the last top-level expression for each cell.
        // This is because it's common to have a cell that ends with an expression
        // to display it's value.
        if checker.source_type.is_ipynb()
            && at_last_top_level_expression_in_cell(
                checker.semantic(),
                checker.locator(),
                checker.cell_offsets(),
            )
        {
            return;
        }

        checker
            .diagnostics
            .push(Diagnostic::new(UselessComparison, expr.range()));
    }
}
