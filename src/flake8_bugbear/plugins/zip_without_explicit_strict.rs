use rustpython_ast::{Expr, ExprKind, Keyword};

use crate::ast::types::Range;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// B905
pub fn zip_without_explicit_strict(
    xxxxxxxx: &mut xxxxxxxx,
    expr: &Expr,
    func: &Expr,
    kwargs: &[Keyword],
) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "zip"
            && xxxxxxxx.is_builtin("zip")
            && !kwargs.iter().any(|keyword| {
                keyword
                    .node
                    .arg
                    .as_ref()
                    .map_or(false, |name| name == "strict")
            })
        {
            xxxxxxxx.diagnostics.push(Diagnostic::new(
                violations::ZipWithoutExplicitStrict,
                Range::from_located(expr),
            ));
        }
    }
}
