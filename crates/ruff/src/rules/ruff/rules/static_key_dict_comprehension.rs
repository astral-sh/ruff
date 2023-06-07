use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use rustpython_parser::ast::{self, Expr, Ranged};

#[violation]
pub struct StaticKeyDictComprehension;

impl Violation for StaticKeyDictComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Static key value in dict comprehension")
    }
}

// Tuples are special cases, they're constant but to violate the rule their elements also need to be constant
fn is_constant_tuple(expr: &Expr) -> bool {
    let Expr::Tuple(ast::ExprTuple{elts, ..}) = expr else {
        // It's not a constant tuple if it's not a tuple
        return false;
    };
    elts.iter().all(|item| item.is_constant_expr())
}

/// RUF011
pub(crate) fn static_key_dict_comprehension(checker: &mut Checker, expr: &Expr, key: &Expr) {
    if key.is_constant_expr() || is_constant_tuple(key) {
        checker
            .diagnostics
            .push(Diagnostic::new(StaticKeyDictComprehension, expr.range()));
    }
}
