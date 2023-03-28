use rustpython_parser::ast::Expr;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct TypingTextStrAlias;

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
    if checker
        .ctx
        .resolve_call_path(expr)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["typing", "Text"]
        })
    {
        let mut diagnostic = Diagnostic::new(TypingTextStrAlias, Range::from(expr));
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.set_fix(Edit::replacement(
                "str".to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
