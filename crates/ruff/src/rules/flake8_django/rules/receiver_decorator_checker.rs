use rustpython_parser::ast::{Expr, ExprKind, Located};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct ReceiverDecoratorChecker;
);
impl Violation for ReceiverDecoratorChecker {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`@receiver` decorator must be on top of all the other decorators")
    }
}

/// DJ013
pub fn receiver_decorator_checker(decorator_list: &[Expr]) -> Option<Diagnostic> {
    let Some(Located {node: ExprKind::Call{ func, ..}, ..}) = decorator_list.first() else {
        return None;
    };
    let ExprKind::Name {id, ..} = &func.node else {
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
