use rustpython_ast::Expr;

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// UP020
pub fn open_alias(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, func: &Expr) {
    let call_path = dealias_call_path(collect_call_paths(expr), &xxxxxxxx.import_aliases);

    if match_call_path(&call_path, "io", "open", &xxxxxxxx.from_imports) {
        let mut check = Diagnostic::new(violations::OpenAlias, Range::from_located(expr));
        if xxxxxxxx.patch(&RuleCode::UP020) {
            check.amend(Fix::replacement(
                "open".to_string(),
                func.location,
                func.end_location.unwrap(),
            ));
        }
        xxxxxxxx.diagnostics.push(check);
    }
}
