use rustpython_ast::{Expr, Location};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// UP025
pub fn rewrite_unicode_literal(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, kind: Option<&str>) {
    if let Some(const_kind) = kind {
        if const_kind.to_lowercase() == "u" {
            let mut check =
                Diagnostic::new(violations::RewriteUnicodeLiteral, Range::from_located(expr));
            if xxxxxxxx.patch(check.kind.code()) {
                check.amend(Fix::deletion(
                    expr.location,
                    Location::new(expr.location.row(), expr.location.column() + 1),
                ));
            }
            xxxxxxxx.diagnostics.push(check);
        }
    }
}
