use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for chained operators where adding parentheses could improve the
/// clarity of the code.
///
/// ## Why is this bad?
/// `and` always binds more tightly than `or` when chaining the two together,
/// but this can be hard to remember (and sometimes surprising).
/// Adding parentheses in these situations can greatly improve code readability,
/// with no change to semantics or performance.
///
/// For example:
/// ```python
/// a, b, c = 1, 0, 2
/// x = a or b and c
///
/// d, e, f = 0, 1, 2
/// y = d and e or f
/// ```
///
/// Use instead:
/// ```python
/// a, b, c = 1, 0, 2
/// x = a or (b and c)
///
/// d, e, f = 0, 1, 2
/// y = (d and e) or f
/// ````
#[violation]
pub struct ParenthesizeChainedOperators;

impl Violation for ParenthesizeChainedOperators {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Parenthesize `a and b` expressions when chaining `and` and `or` together, to make the precedence clear"
        )
    }
}

/// RUF021
pub(crate) fn parenthesize_chained_logical_operators(
    checker: &mut Checker,
    expr: &ast::ExprBoolOp,
) {
    for condition in &expr.values {
        match condition {
            // If we find a BoolOp expression inside a BoolOp expression,
            // it means a different operator is being used for the subexpression
            // than in the superexpression:
            // `a or b or c` => `BoolOp(values=[Name('a'), Name('b'), Name('c')], op=Or)`
            // `a or b and c` => `BoolOp(values=[Name('a'), BoolOp(values=[Name('b'), Name('c')], op=And)], op=Or)`
            ast::Expr::BoolOp(bool_op) => {
                // `and` binds more tightly than `or`, and the AST reflects this:
                // the BoolOp subexpression *must*, logically, be an `And`
                // (and the superexpression *must*, logically, be an `Or`)
                assert!(bool_op.op.is_and());
                if parenthesized_range(
                    bool_op.into(),
                    expr.into(),
                    checker.indexer().comment_ranges(),
                    checker.locator().contents(),
                )
                .is_none()
                {
                    checker.diagnostics.push(Diagnostic::new(
                        ParenthesizeChainedOperators,
                        bool_op.range(),
                    ));
                }
            }
            _ => continue,
        };
    }
}
