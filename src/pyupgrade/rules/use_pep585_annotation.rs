use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;

/// UP006
pub fn use_pep585_annotation(checker: &mut Checker, expr: &Expr) {
    if let Some(call_path) = checker.resolve_call_path(expr) {
        let mut diagnostic = Diagnostic::new(
            violations::UsePEP585Annotation(call_path[call_path.len() - 1].to_string()),
            Range::from_located(expr),
        );
        if checker.patch(diagnostic.kind.code()) {
            diagnostic.amend(Fix::replacement(
                call_path[call_path.len() - 1].to_lowercase(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
