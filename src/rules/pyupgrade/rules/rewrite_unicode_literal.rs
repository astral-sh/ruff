use rustpython_ast::{Expr, Location};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;

/// UP025
pub fn rewrite_unicode_literal(checker: &mut Checker, expr: &Expr, kind: Option<&str>) {
    if let Some(const_kind) = kind {
        if const_kind.to_lowercase() == "u" {
            let mut diagnostic =
                Diagnostic::new(violations::RewriteUnicodeLiteral, Range::from_located(expr));
            if checker.patch(diagnostic.kind.rule()) {
                diagnostic.amend(Fix::deletion(
                    expr.location,
                    Location::new(expr.location.row(), expr.location.column() + 1),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
