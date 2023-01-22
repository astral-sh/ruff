use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violations;

/// UP020
pub fn open_alias(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if checker
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["io", "open"])
    {
        let mut diagnostic = Diagnostic::new(violations::OpenAlias, Range::from_located(expr));
        if checker.patch(&Rule::OpenAlias) {
            diagnostic.amend(Fix::replacement(
                "open".to_string(),
                func.location,
                func.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
