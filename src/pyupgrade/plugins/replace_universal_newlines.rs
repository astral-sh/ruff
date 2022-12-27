use rustpython_ast::{Expr, Keyword, Location};

use crate::ast::helpers::{find_keyword, match_module_member};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// UP021
pub fn replace_universal_newlines(checker: &mut Checker, expr: &Expr, kwargs: &[Keyword]) {
    if match_module_member(
        expr,
        "subprocess",
        "run",
        &checker.from_imports,
        &checker.import_aliases,
    ) {
        let Some(kwarg) = find_keyword(kwargs, "universal_newlines") else { return; };
        let range = Range {
            location: kwarg.location,
            end_location: Location::new(
                kwarg.location.row(),
                kwarg.location.column() + "universal_newlines".len(),
            ),
        };
        let mut check = Check::new(CheckKind::ReplaceUniversalNewlines, range);
        if checker.patch(check.kind.code()) {
            check.amend(Fix::replacement(
                "text".to_string(),
                range.location,
                range.end_location,
            ));
        }
        checker.add_check(check);
    }
}
