use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

pub fn rewrite_unicode_literal(
    checker: &mut Checker,
    expr: &Expr,
    value: &str,
    kind: &Option<String>,
) {
    if let Some(const_kind) = kind {
        if const_kind == "u" {
            let mut check = Check::new(CheckKind::RewriteUnicodeLiteral, Range::from_located(expr));
            if checker.patch(check.kind.code()) {
                let mut new_str = value.to_string();
                new_str.insert(0, '"');
                new_str.push('"');
                check.amend(Fix::replacement(
                    new_str,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.add_check(check);
        }
    }
}
