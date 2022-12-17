use rustpython_ast::Expr;

use crate::ast::types::{FunctionDef, Range, ScopeKind};
use crate::checkers::ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLE1142
pub fn await_outside_async(checker: &mut Checker, expr: &Expr) {
    if !checker
        .current_scopes()
        .find_map(|scope| {
            if let ScopeKind::Function(FunctionDef { async_, .. }) = &scope.kind {
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
