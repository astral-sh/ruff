use crate::ast::types::Range;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::Constant::Bool;
use rustpython_ast::ExprKind::{Attribute, Call, Constant, Name};
use rustpython_ast::StmtKind::{Assign, ClassDef, FunctionDef};
use rustpython_ast::{Expr, Located, Stmt};

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
            if is_model(base) {
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
}

define_violation!(
    pub struct ReceiverDecoratorChecker;
);
impl Violation for ReceiverDecoratorChecker {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("@receiver decorator must be on top of all the other decorators")
    }
}

impl ReceiverDecoratorChecker {
    pub fn check_decorator(decorator_list: &[Expr]) -> Option<Diagnostic> {
        let Some(Located {node: Call{ func, ..}, ..}) = decorator_list.first() else {
            return None;
        };
        let Name {id, ..} = &func.node else {
            return None;
        };
        if id == "receiver" {
            return Some(Diagnostic::new(
                ReceiverDecoratorChecker,
                Range::from_located(func),
            ));
        }
        None
    }
}

define_violation!(
    pub struct ModelStringFieldNullable(pub String);
);
impl Violation for ModelStringFieldNullable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let ModelStringFieldNullable(field) = self;
        format!("Avoid using null=True on string-based fields such as {field}.")
    }
}
const NOT_NULL_TRUE_FIELDS: [&str; 6] = [
    "CharField",
    "TextField",
    "SlugField",
    "EmailField",
    "FilePathField",
    "URLField",
];
impl ModelStringFieldNullable {
    pub fn check(bases: &[Expr], body: &[Stmt]) -> Vec<Diagnostic> {
        let mut errors = Vec::new();
        if !bases.iter().any(is_model) {
            return errors;
        }
        for statement in body.iter() {
            let Assign {value, ..} = &statement.node else {
                continue
            };
            if let Some(field_name) = ModelStringFieldNullable::check_nullable_field(value) {
                errors.push(Diagnostic::new(
                    ModelStringFieldNullable(field_name.to_string()),
                    Range::from_located(value),
                ));
            }
        }
        errors
    }

    fn check_nullable_field(value: &Expr) -> Option<&str> {
        let Call {func, keywords, ..} = &value.node else {
           return None;
        };
        let valid_field_name = match &func.node {
            Attribute { attr, .. } => {
                if !NOT_NULL_TRUE_FIELDS.contains(&&**attr) {
                    return None;
                }
                Some(attr)
            }
            Name { id, .. } => {
                if !NOT_NULL_TRUE_FIELDS.contains(&&**id) {
                    return None;
                }
                Some(id)
            }
            _ => None,
        };
        let Some(valid_field_name) = valid_field_name else {
            return None;
        };

        let mut null_key = false;
        let mut blank_key = false;
        let mut unique_key = false;
        for keyword in keywords.iter() {
            let Constant {value: Bool(true), ..} = &keyword.node.value.node else {
                continue
            };
            let Some(argument) = &keyword.node.arg else {
                continue
            };
            match argument.as_str() {
                "blank" => blank_key = true,
                "null" => null_key = true,
                "unique" => unique_key = true,
                _ => continue,
            }
        }
        if blank_key && unique_key {
            return None;
        }
        if null_key {
            return Some(valid_field_name);
        }
        None
    }
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
