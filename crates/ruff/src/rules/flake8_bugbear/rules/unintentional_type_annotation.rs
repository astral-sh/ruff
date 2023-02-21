use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

use rustpython_parser::ast::{Expr, ExprKind, Stmt};

define_violation!(
    pub struct UnintentionalTypeAnnotation;
);
impl Violation for UnintentionalTypeAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Possible unintentional type annotation (using :). Did you mean to assign (using =)?"
        )
    }
}

/// B032
pub fn unintentional_type_annotation(
    checker: &mut Checker,
    target: &Expr,
    value: &Expr,
    stmt: &Stmt,
) {
    checker.diagnostics.push(Diagnostic::new(
        UnintentionalTypeAnnotation,
        Range::from_located(stmt),
    ));
}
