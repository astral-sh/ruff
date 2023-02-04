use crate::ast::types::Range;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::Constant::Bool;
use rustpython_ast::ExprKind::{Attribute, Constant, Name};
use rustpython_ast::StmtKind::{Assign, ClassDef, FunctionDef};
use rustpython_ast::{Expr, Stmt};

define_violation!(
    pub struct ModelDunderStr;
);
impl Violation for ModelDunderStr {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Model does not define __str__ method")
    }
}

impl ModelDunderStr {
    pub fn check_dunder_str(
        bases: &[Expr],
        body: &[Stmt],
        class_location: &Stmt,
    ) -> Option<Diagnostic> {
        if !ModelDunderStr::checker_applies(bases, body) {
            return None;
        }
        if !ModelDunderStr::has_dunder_method(body) {
            return Some(Diagnostic::new(
                ModelDunderStr,
                Range::from_located(class_location),
            ));
        }
        None
    }

    fn has_dunder_method(body: &[Stmt]) -> bool {
        body.iter().any(|val| match &val.node {
            FunctionDef { name, .. } => {
                if name == "__str__" {
                    return true;
                }
                false
            }
            _ => false,
        })
    }

    fn checker_applies(bases: &[Expr], body: &[Stmt]) -> bool {
        for base in bases.iter() {
            if ModelDunderStr::is_abstract(body) {
                continue;
            }
            if ModelDunderStr::is_model(base) {
                return true;
            }
        }
        false
    }

    fn is_abstract(body: &[Stmt]) -> bool {
        for element in body.iter() {
            let ClassDef{name, body, ..} = &element.node else {
              continue
            };
            if name != "Meta" {
                continue;
            }
            for element in body.iter() {
                let Assign{targets, value, ..} = &element.node else {
                    continue
                };
                for target in targets.iter() {
                    let Name {id , ..} = &target.node else {continue};
                    if id != "abstract" {
                        continue;
                    }
                    let Constant{value: Bool(true), ..} = &value.node else {
                        continue;
                    };
                    return true;
                }
            }
        }
        false
    }
    fn is_model(base: &Expr) -> bool {
        match &base.node {
            Name { id, .. } => {
                if id == "Model" {
                    return true;
                }
                false
            }
            Attribute { value, attr, .. } => {
                let Name{id, ..} = &value.node else {
                    return false;
                };
                if attr == "Model" && id == "models" {
                    return true;
                }
                false
            }
            _ => false,
        }
    }
}
