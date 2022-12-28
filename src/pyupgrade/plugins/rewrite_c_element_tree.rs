use rustpython_ast::Expr;

use crate::ast::helpers::match_module_member;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// UP023
pub fn replace_c_element_tree(checker: &mut Checker, expr: &Expr) {
    if match_module_member(
        expr,
        "etree",
        "cElementTree",
        &checker.from_imports,
        &checker.import_aliases,
    ) {
        let mut check = Check::new(CheckKind::TypingTextStrAlias, Range::from_located(expr));
        if checker.patch(check.kind.code()) {
            check.amend(Fix::replacement(
                "str".to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.add_check(check);
    }
}
