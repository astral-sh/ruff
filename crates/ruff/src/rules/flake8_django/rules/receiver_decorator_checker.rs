use rustpython_parser::ast::{Expr, ExprKind};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::{CallPath, Range};
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
pub fn receiver_decorator_checker<'a, F>(
    decorator_list: &'a [Expr],
    resolve_call_path: F,
) -> Option<Diagnostic>
where
    F: Fn(&'a Expr) -> Option<CallPath<'a>>,
{
    for (i, decorator) in decorator_list.iter().enumerate() {
        if i == 0 {
            continue;
        }
        let ExprKind::Call{ func, ..} = &decorator.node else {
            continue;
        };
        if resolve_call_path(func).map_or(false, |call_path| {
            call_path.as_slice() == ["django", "dispatch", "receiver"]
        }) {
            return Some(Diagnostic::new(
                ReceiverDecoratorChecker,
                Range::from_located(decorator),
            ));
        }
    }
    None
}
