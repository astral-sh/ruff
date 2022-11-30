use rustpython_ast::Expr;

use crate::ast::types::{FunctionScope, Range, ScopeKind};
use crate::check_ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLE1142
pub fn await_outside_async(checker: &mut Checker, expr: &Expr) {
    if !checker
        .current_scopes()
        .find_map(|scope| {
            if let ScopeKind::Function(FunctionScope { async_, .. }) = &scope.kind {
                Some(*async_)
            } else {
                None
            }
        })
        .unwrap_or(true)
    {
        checker.add_check(Check::new(
            CheckKind::AwaitOutsideAsync,
            Range::from_located(expr),
        ));
    }
}
