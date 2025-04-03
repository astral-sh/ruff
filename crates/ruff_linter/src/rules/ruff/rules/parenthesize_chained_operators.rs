use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct ParenthesizeChainedOperators;

impl AlwaysFixableViolation for ParenthesizeChainedOperators {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Parenthesize `a and b` expressions when chaining `and` and `or` together, to make the precedence clear".to_string()
    }

    fn fix_title(&self) -> String {
        "Parenthesize the `and` subexpression".to_string()
    }
}

/// RUF021
pub(crate) fn parenthesize_chained_logical_operators(checker: &Checker, expr: &ast::ExprBoolOp) {
    // We're only interested in `and` expressions inside `or` expressions:
    // - `a or b or c` => `BoolOp(values=[Name("a"), Name("b"), Name("c")], op=Or)`
    // - `a and b and c` => `BoolOp(values=[Name("a"), Name("b"), Name("c")], op=And)`
    // - `a or b and c` => `BoolOp(value=[Name("a"), BoolOp(values=[Name("b"), Name("c")], op=And), op=Or)`
    //
    // While it is *possible* to get an `Or` node inside an `And` node,
    // you can only achieve it by parenthesizing the `or` subexpression
    // (since normally, `and` always binds more tightly):
    // - `a and (b or c)` => `BoolOp(values=[Name("a"), BoolOp(values=[Name("b"), Name("c"), op=Or), op=And)`
    //
    // We only care about unparenthesized boolean subexpressions here
    // (if they're parenthesized already, that's great!),
    // so we can ignore all cases where an `Or` node
    // exists inside an `And` node.
    if expr.op.is_and() {
        return;
    }
    for condition in &expr.values {
        match condition {
            ast::Expr::BoolOp(
                bool_op @ ast::ExprBoolOp {
                    op: ast::BoolOp::And,
                    ..
                },
            ) => {
                let locator = checker.locator();
                let source_range = bool_op.range();
                if parenthesized_range(
                    bool_op.into(),
                    expr.into(),
                    checker.comment_ranges(),
                    locator.contents(),
                )
                .is_none()
                {
                    let new_source = format!("({})", locator.slice(source_range));
                    let edit = Edit::range_replacement(new_source, source_range);
                    checker.report_diagnostic(
                        Diagnostic::new(ParenthesizeChainedOperators, source_range)
                            .with_fix(Fix::safe_edit(edit)),
                    );
                }
            }
            _ => continue,
        }
    }
}
