use rustpython_ast::{Arguments, Expr, ExprKind, Stmt};

use crate::ast::types::{FunctionScope, Range, ScopeKind};
use crate::check_ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLE0206
pub fn property_with_parameters(
    checker: &mut Checker,
    stmt: &Stmt,
    decorator_list: &[Expr],
    args: &Arguments,
) {
    if decorator_list
        .iter()
        .any(|d| matches!(&d.node, ExprKind::Name { id, .. } if id == "property"))
    {
        if checker.is_builtin("property")
            && args
                .args
                .iter()
                .chain(args.posonlyargs.iter())
                .chain(args.kwonlyargs.iter())
                .count()
                > 1
        {
            checker.add_check(Check::new(
                CheckKind::PropertyWithParameters,
                Range::from_located(stmt),
            ));
        }
    }
}

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
