use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, Keyword, Location};

use crate::ast::helpers::find_keyword;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct ReplaceUniversalNewlines;
);
impl AlwaysAutofixableViolation for ReplaceUniversalNewlines {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`universal_newlines` is deprecated, use `text`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `text` keyword argument".to_string()
    }
}

/// UP021
pub fn replace_universal_newlines(checker: &mut Checker, func: &Expr, kwargs: &[Keyword]) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["subprocess", "run"]
    }) {
        let Some(kwarg) = find_keyword(kwargs, "universal_newlines") else { return; };
        let range = Range::new(
            kwarg.location,
            Location::new(
                kwarg.location.row(),
                kwarg.location.column() + "universal_newlines".len(),
            ),
        );
        let mut diagnostic = Diagnostic::new(ReplaceUniversalNewlines, range);
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.amend(Fix::replacement(
                "text".to_string(),
                range.location,
                range.end_location,
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}
