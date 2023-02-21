use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

use rustpython_parser::ast::{Expr, ExprKind, Located, Stmt};

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
    value: &Option<Box<Located<ExprKind>>>,
    stmt: &Stmt,
) {
    if let Some(value) = value {
        return;
    }

    let is_target_subscript = match &target.node {
        ExprKind::Subscript { .. } => true,
        _ => false,
    };

    let is_target_attribute = match &target.node {
        ExprKind::Attribute { .. } => true,
        _ => false,
    };

    if is_target_subscript || is_target_attribute {
        let target_value = match &target.node {
            ExprKind::Subscript { value, .. } => value,
            ExprKind::Attribute { value, .. } => value,
            _ => return,
        };

        let is_value_name = match &target_value.node {
            ExprKind::Name { .. } => true,
            _ => false,
        };
        let mut err = false;
        if is_value_name {
            if is_target_subscript {
                err = true;
            } else {
                let value_id = match &target_value.node {
                    ExprKind::Name { id, .. } => id,
                    _ => return,
                };

                if value_id != "self" {
                    err = true;
                }
            }
        }
        if err {
            checker.diagnostics.push(Diagnostic::new(
                UnintentionalTypeAnnotation,
                Range::from_located(stmt),
            ));
        }
    }
}
