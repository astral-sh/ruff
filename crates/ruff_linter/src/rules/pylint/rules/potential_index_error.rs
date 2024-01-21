use std::str::FromStr;

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
        }) => (number_value.to_owned(), *range),
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::USub,
            operand,
            range,
        }) => match operand.as_ref() {
            Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(number_value),
                ..
            }) => (
                ast::Int::from_str(&format!("-{number_value}")).unwrap(),
                *range,
            ),
            _ => return,
        },
        _ => return,
    };

    let emit = if let Some(number) = number_value.as_i64() {
        number >= length || number < -length
    } else {
        // this should be impossible
        true
    };

    if emit {
        checker
            .diagnostics
            .push(Diagnostic::new(PotentialIndexError, range));
    }
}
