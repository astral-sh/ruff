use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct TypingTextStrAlias;
);
impl AlwaysAutofixableViolation for TypingTextStrAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`typing.Text` is deprecated, use `str`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `str`".to_string()
    }
}

/// UP019
pub fn typing_text_str_alias(checker: &mut Checker, expr: &Expr) {
    if checker.resolve_call_path(expr).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "Text"]
    }) {
        let mut diagnostic = Diagnostic::new(TypingTextStrAlias, Range::from_located(expr));
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.amend(Fix::replacement(
                "str".to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
