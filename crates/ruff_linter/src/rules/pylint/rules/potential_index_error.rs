use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for potential hard-coded IndexErrors, which occurs when accessing
/// a list or tuple with an index that is known to be out of bounds.
///
/// ## Why is this bad?
/// This will cause a runtime error.
///
/// ## Example
/// ```python
/// print([1, 2, 3][123])
/// ```
///
#[violation]
pub struct PotentialIndexError;

impl Violation for PotentialIndexError {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Potential IndexError")
    }
}

/// PLE0643
pub(crate) fn potential_index_error(checker: &mut Checker, value: &Expr, slice: &Expr) {
    let length = match value {
        Expr::Tuple(ast::ExprTuple { elts, .. }) | Expr::List(ast::ExprList { elts, .. }) => {
            match i64::try_from(elts.len()) {
                Ok(length) => length,
                Err(_) => return,
            }
        }
        _ => {
            return;
        }
    };

    let (number_value, range) = match slice {
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(number_value),
            range,
        }) => match number_value.as_i64() {
            Some(value) => (-value, *range),
            None => return,
        },
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::USub,
            operand,
            range,
        }) => match operand.as_ref() {
            Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(number_value),
                ..
            }) => match number_value.as_i64() {
                Some(value) => (-value, *range),
                None => return,
            },
            _ => return,
        },
        _ => return,
    };

    if number_value >= length || number_value < -length {
        checker
            .diagnostics
            .push(Diagnostic::new(PotentialIndexError, range));
    }
}
