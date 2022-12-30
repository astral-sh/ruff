use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::pydocstyle::helpers::leading_quote;

pub fn rewrite_unicode_literal(
    checker: &mut Checker,
    expr: &Expr,
    value: &str,
    kind: &Option<String>,
) {
    if let Some(const_kind) = kind {
        if const_kind.to_lowercase() == "u" {
            let range = Range::from_located(expr);
            let content = checker.locator.slice_source_code_range(&range);
            // We need to skip the first item since it is always a u
            let quotes = leading_quote(&content).unwrap_or("u\"")[1..].to_string();
            let mut check = Check::new(CheckKind::RewriteUnicodeLiteral, range);
            if checker.patch(check.kind.code()) {
                let mut contents = String::new();
                contents.push_str(&quotes);
                contents.push_str(value);
                contents.push_str(&quotes);
                check.amend(Fix::replacement(
                    contents,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.add_check(check);
        }
    }
}
