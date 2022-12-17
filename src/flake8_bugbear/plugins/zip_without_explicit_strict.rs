use rustpython_ast::{Expr, ExprKind, Keyword};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// B905
pub fn zip_without_explicit_strict(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    kwargs: &[Keyword],
) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "zip"
            && checker.is_builtin("zip")
            && !kwargs.iter().any(|keyword| {
                keyword
                    .node
                    .arg
                    .as_ref()
                    .map_or(false, |name| name == "strict")
            })
        {
            checker.add_check(Check::new(
                CheckKind::ZipWithoutExplicitStrict,
                Range::from_located(expr),
            ));
        }
    }
}
