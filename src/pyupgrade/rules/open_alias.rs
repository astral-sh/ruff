use rustpython_ast::Expr;

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;

/// UP020
pub fn open_alias(checker: &mut Checker, expr: &Expr, func: &Expr) {
    let call_path = dealias_call_path(collect_call_paths(expr), &checker.import_aliases);

    if match_call_path(&call_path, "io", "open", &checker.from_imports) {
        let mut diagnostic = Diagnostic::new(violations::OpenAlias, Range::from_located(expr));
        if checker.patch(&RuleCode::UP020) {
            diagnostic.amend(Fix::replacement(
                "open".to_string(),
                func.location,
                func.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
