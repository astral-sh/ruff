use rustpython_ast::Expr;

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, DiagnosticCode};
use crate::violations;

/// UP020
pub fn open_alias(checker: &mut Checker, expr: &Expr, func: &Expr) {
    let call_path = dealias_call_path(collect_call_paths(expr), &checker.import_aliases);

    if match_call_path(&call_path, "io", "open", &checker.from_imports) {
        let mut check = Diagnostic::new(violations::OpenAlias, Range::from_located(expr));
        if checker.patch(&DiagnosticCode::UP020) {
            check.amend(Fix::replacement(
                "open".to_string(),
                func.location,
                func.end_location.unwrap(),
            ));
        }
        checker.checks.push(check);
    }
}
