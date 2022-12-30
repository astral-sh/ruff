use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

pub fn rewrite_unicode_literal(checker: &mut Checker, expr: &Expr, kind: &Option<String>) {
    if let Some(const_kind) = kind {
        if const_kind.to_lowercase() == "u" {
            let mut check = Check::new(CheckKind::RewriteUnicodeLiteral, Range::from_located(expr));
            if checker.patch(check.kind.code()) {
                let mut content = checker
                    .locator
                    .slice_source_code_range(&Range::from_located(expr))
                    .to_string();
                let first_char = content.remove(0);
                if first_char == 'u' || first_char == 'U' {
                    check.amend(Fix::replacement(
                        content,
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
            }
            checker.add_check(check);
        }
    }
}
