use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};

use crate::checkers::ast::Checker;

#[violation]
pub struct RedundantLiteralUnion {
    literal: String,
    constant_literal: String,
}

impl Violation for RedundantLiteralUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedundantLiteralUnion {
            literal,
            constant_literal,
        } = self;
        format!("`{literal}` is redundant in an union with `{constant_literal}`")
    }
}

/// PYI051
pub(crate) fn redudant_literal_union(checker: &mut Checker, expr: &Expr) {
    dbg!(expr);
    // TODO: compare the `value` with the literal builtins in `slice`
    if let Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
        if checker.semantic().match_typing_expr(value, "Literal") {
            dbg!(slice);
        }
    }
}
