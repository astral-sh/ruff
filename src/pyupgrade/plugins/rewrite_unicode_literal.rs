use rustpython_ast::{Expr, Location};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

pub fn rewrite_unicode_literal(checker: &mut Checker, expr: &Expr, kind: &Option<String>) {
    if let Some(const_kind) = kind {
        if const_kind.to_lowercase() == "u" {
            let mut check = Check::new(CheckKind::RewriteUnicodeLiteral, Range::from_located(expr));
            if checker.patch(check.kind.code()) {
                check.amend(Fix::deletion(
                    expr.location,
                    Location::new(expr.location.row(), expr.location.column() + 1),
                ));
            }
            checker.add_check(check);
        }
    }
}
