use rustpython_ast::Expr;

use crate::ast::helpers::match_module_member;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::registry::Diagnostic;
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// UP019
pub fn typing_text_str_alias(xxxxxxxx: &mut xxxxxxxx, expr: &Expr) {
    if match_module_member(
        expr,
        "typing",
        "Text",
        &xxxxxxxx.from_imports,
        &xxxxxxxx.import_aliases,
    ) {
        let mut check = Diagnostic::new(violations::TypingTextStrAlias, Range::from_located(expr));
        if xxxxxxxx.patch(check.kind.code()) {
            check.amend(Fix::replacement(
                "str".to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        xxxxxxxx.diagnostics.push(check);
    }
}
