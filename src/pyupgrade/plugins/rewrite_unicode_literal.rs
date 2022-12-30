use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::pydocstyle::helpers::leading_quote;

/// Strip any leading kind prefixes (e..g. "u") from a quote string.
fn strip_kind(leading_quote: &str) -> &str {
    if let Some(index) = leading_quote.find('\'') {
        &leading_quote[index..]
    } else if let Some(index) = leading_quote.find('\"') {
        &leading_quote[index..]
    } else {
        unreachable!("Expected docstring to start with a valid triple- or single-quote prefix")
    }
}

pub fn rewrite_unicode_literal(
    checker: &mut Checker,
    expr: &Expr,
    value: &str,
    kind: &Option<String>,
) {
    if let Some(const_kind) = kind {
        if const_kind.to_lowercase() == "u" {
            let mut check = Check::new(CheckKind::RewriteUnicodeLiteral, Range::from_located(expr));
            if checker.patch(check.kind.code()) {
                let content = checker
                    .locator
                    .slice_source_code_range(&Range::from_located(expr));
                if let Some(leading_quote) = leading_quote(&content).map(strip_kind) {
                    let mut contents = String::with_capacity(value.len() + leading_quote.len() * 2);
                    contents.push_str(leading_quote);
                    contents.push_str(value);
                    contents.push_str(leading_quote);
                    check.amend(Fix::replacement(
                        contents,
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
            }
            checker.add_check(check);
        }
    }
}
