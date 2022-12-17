use rustpython_ast::{Arguments, Expr, ExprKind, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::CheckKind;
use crate::Check;

/// PLR0206
pub fn property_with_parameters(
    checker: &mut Checker,
    stmt: &Stmt,
    decorator_list: &[Expr],
    args: &Arguments,
) {
    if !decorator_list
        .iter()
        .any(|d| matches!(&d.node, ExprKind::Name { id, .. } if id == "property"))
    {
        return;
    }
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
