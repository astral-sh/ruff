use super::helpers;
use rustpython_parser::ast::Constant::Bool;
use rustpython_parser::ast::{Expr, StmtKind};
use rustpython_parser::ast::{ExprKind, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::{define_violation, derive_message_formats};

define_violation!(
    pub struct ModelDunderStr;
);
impl Violation for ModelDunderStr {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Model does not define `__str__` method")
    }
}

pub fn model_dunder_str(
    checker: &Checker,
    bases: &[Expr],
    body: &[Stmt],
    class_location: &Stmt,
) -> Option<Diagnostic> {
    if !checker_applies(checker, bases, body) {
        return None;
    }
    if !has_dunder_method(body) {
        return Some(Diagnostic::new(
            ModelDunderStr,
            Range::from_located(class_location),
        ));
    }
    None
}

fn has_dunder_method(body: &[Stmt]) -> bool {
    body.iter().any(|val| match &val.node {
        StmtKind::FunctionDef { name, .. } => {
            if name == "__str__" {
                return true;
            }
            false
        }
        _ => false,
    })
}

fn checker_applies(checker: &Checker, bases: &[Expr], body: &[Stmt]) -> bool {
    for base in bases.iter() {
        if is_model_abstract(body) {
            continue;
        }
        if helpers::is_model(checker, base) {
            return true;
        }
    }
    false
}

/// Check if class is abstract in terms of django model inheritance
fn is_model_abstract(body: &[Stmt]) -> bool {
    for element in body.iter() {
        let StmtKind::ClassDef{name, body, ..} = &element.node else {
            continue
        };
        if name != "Meta" {
            continue;
        }
        for element in body.iter() {
            let StmtKind::Assign{targets, value, ..} = &element.node else {
                continue
            };
            for target in targets.iter() {
                let ExprKind::Name {id , ..} = &target.node else {continue};
                if id != "abstract" {
                    continue;
                }
                let ExprKind::Constant{value: Bool(true), ..} = &value.node else {
                    continue;
                };
                return true;
            }
        }
    }
    false
}
