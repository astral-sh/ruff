use rustpython_ast::Expr;

use crate::ast::types::{FunctionDef, Range, ScopeKind};
use crate::xxxxxxxxs::ast::xxxxxxxx;
use crate::{violations, Diagnostic};

/// PLE1142
pub fn await_outside_async(xxxxxxxx: &mut xxxxxxxx, expr: &Expr) {
    if !xxxxxxxx
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
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::AwaitOutsideAsync,
            Range::from_located(expr),
        ));
    }
}
