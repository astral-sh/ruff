use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::{Expr, Keyword};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::find_keyword;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct ReplaceUniversalNewlines;

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
    if checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["subprocess", "run"]
        })
    {
        let Some(kwarg) = find_keyword(kwargs, "universal_newlines") else { return; };
        let range = TextRange::at(kwarg.start(), "universal_newlines".text_len());
        let mut diagnostic = Diagnostic::new(ReplaceUniversalNewlines, range);
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.set_fix(Edit::range_replacement("text".to_string(), range));
        }
        checker.diagnostics.push(diagnostic);
    }
}
