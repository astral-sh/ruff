use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// UP006
pub fn use_pep585_annotation(xxxxxxxx: &mut xxxxxxxx, expr: &Expr, id: &str) {
    let replacement = *xxxxxxxx.import_aliases.get(id).unwrap_or(&id);
    let mut check = Diagnostic::new(
        violations::UsePEP585Annotation(replacement.to_string()),
        Range::from_located(expr),
    );
    if xxxxxxxx.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            replacement.to_lowercase(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    xxxxxxxx.diagnostics.push(check);
}
