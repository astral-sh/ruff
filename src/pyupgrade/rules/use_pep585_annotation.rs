use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;

/// UP006
pub fn use_pep585_annotation(checker: &mut Checker, expr: &Expr, id: &str) {
    let replacement = *checker.import_aliases.get(id).unwrap_or(&id);
    let mut diagnostic = Diagnostic::new(
        violations::UsePEP585Annotation(replacement.to_string()),
        Range::from_located(expr),
    );
    if checker.patch(diagnostic.kind.code()) {
        diagnostic.amend(Fix::replacement(
            replacement.to_lowercase(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
