use ruff_python_ast::{self as ast, Expr, UnaryOp};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the attempted use of the unary prefix increment (`++`) or
/// decrement operator (`--`).
///
/// ## Why is this bad?
/// Python does not support the unary prefix increment or decrement operator.
/// Writing `++n` is equivalent to `+(+(n))` and writing `--n` is equivalent to
/// `-(-(n))`. In both cases, it is equivalent to `n`.
///
/// ## Example
/// ```python
/// ++x
/// --y
/// ```
///
/// Use instead:
/// ```python
/// x += 1
/// y -= 1
/// ```
///
/// ## References
/// - [Python documentation: Unary arithmetic and bitwise operations](https://docs.python.org/3/reference/expressions.html#unary-arithmetic-and-bitwise-operations)
/// - [Python documentation: Augmented assignment statements](https://docs.python.org/3/reference/simple_stmts.html#augmented-assignment-statements)
#[violation]
pub struct UnaryPrefixIncrementDecrement {
    operator: UnaryPrefixOperatorType,
}

impl Violation for UnaryPrefixIncrementDecrement {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnaryPrefixIncrementDecrement { operator } = self;
        match operator {
            UnaryPrefixOperatorType::Increment => {
                format!("Python does not support the unary prefix increment operator (`++`)")
            }
            UnaryPrefixOperatorType::Decrement => {
                format!("Python does not support the unary prefix decrement operator (`--`)")
            }
        }
    }
}

/// B002
pub(crate) fn unary_prefix_increment_decrement(
    checker: &mut Checker,
    expr: &Expr,
    op: UnaryOp,
    operand: &Expr,
) {
    let Expr::UnaryOp(ast::ExprUnaryOp { op: nested_op, .. }) = operand else {
        return;
    };
    match (op, nested_op) {
        (UnaryOp::UAdd, UnaryOp::UAdd) => {
            checker.diagnostics.push(Diagnostic::new(
                UnaryPrefixIncrementDecrement {
                    operator: UnaryPrefixOperatorType::Increment,
                },
                expr.range(),
            ));
        }
        (UnaryOp::USub, UnaryOp::USub) => {
            checker.diagnostics.push(Diagnostic::new(
                UnaryPrefixIncrementDecrement {
                    operator: UnaryPrefixOperatorType::Decrement,
                },
                expr.range(),
            ));
        }
        _ => {}
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum UnaryPrefixOperatorType {
    Increment,
    Decrement,
}
