use rustpython_ast::{Arguments, Expr, ExprKind, Stmt};

use crate::ast::types::Range;
use crate::xxxxxxxxs::ast::xxxxxxxx;
use crate::{violations, Diagnostic};

/// PLR0206
pub fn property_with_parameters(
    xxxxxxxx: &mut xxxxxxxx,
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
    if xxxxxxxx.is_builtin("property")
        && args
            .args
            .iter()
            .chain(args.posonlyargs.iter())
            .chain(args.kwonlyargs.iter())
            .count()
            > 1
    {
        xxxxxxxx.diagnostics.push(Diagnostic::new(
            violations::PropertyWithParameters,
            Range::from_located(stmt),
        ));
    }
}
