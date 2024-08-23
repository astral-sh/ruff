use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, Stmt};
use ruff_python_semantic::ScopeKind;
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
/// ## Notebook behavior
/// For Jupyter Notebooks, this rule is not applied to the last top-level expression in a cell.
/// This is because it's common to have a notebook cell that ends with an expression,
/// which will result in the `repr` of the evaluated expression being printed as the cell's output.
///
/// ## References
/// - [Python documentation: `assert` statement](https://docs.python.org/3/reference/simple_stmts.html#the-assert-statement)
#[violation]
pub struct UselessComparison {
    at: ComparisonLocationAt,
}

impl Violation for UselessComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.at {
            ComparisonLocationAt::MiddleBody => format!(
                "Pointless comparison. Did you mean to assign a value? \
                Otherwise, prepend `assert` or remove it."
            ),
            ComparisonLocationAt::EndOfFunction => format!(
                "Pointless comparison at end of function scope. Did you mean \
                to return the expression result?"
            ),
        }
    }
}

/// B015
pub(crate) fn useless_comparison(checker: &mut Checker, expr: &Expr) {
    if expr.is_compare_expr() {
        let semantic = checker.semantic();

        if checker.source_type.is_ipynb()
            && at_last_top_level_expression_in_cell(
                semantic,
                checker.locator(),
                checker.cell_offsets(),
            )
        {
            return;
        }

        if let ScopeKind::Function(func_def) = semantic.current_scope().kind {
            if func_def
                .body
                .last()
                .and_then(Stmt::as_expr_stmt)
                .is_some_and(|last_stmt| &*last_stmt.value == expr)
            {
                checker.diagnostics.push(Diagnostic::new(
                    UselessComparison {
                        at: ComparisonLocationAt::EndOfFunction,
                    },
                    expr.range(),
                ));
                return;
            }
        }

        checker.diagnostics.push(Diagnostic::new(
            UselessComparison {
                at: ComparisonLocationAt::MiddleBody,
            },
            expr.range(),
        ));
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum ComparisonLocationAt {
    MiddleBody,
    EndOfFunction,
}
