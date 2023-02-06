use crate::ast::types::Range;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::derive_message_formats;
use rustpython_ast::ExprKind::{Call, Name};
use rustpython_ast::{Expr, Located};
define_violation!(
    pub struct ReceiverDecoratorChecker;
);
impl Violation for ReceiverDecoratorChecker {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("@receiver decorator must be on top of all the other decorators")
    }
}

pub fn receiver_decorator_checker(decorator_list: &[Expr]) -> Option<Diagnostic> {
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
