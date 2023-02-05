use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::rules::regexes::BACKSLASH_REGEX;
use crate::violation::Violation;

use crate::define_violation;
use ruff_macros::derive_message_formats;

define_violation!(
    pub struct UsesRPrefixForBackslashedContent;
);
impl Violation for UsesRPrefixForBackslashedContent {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(r#"Use r""" if any backslashes in a docstring"#)
    }
}

/// D301
pub fn backslashes(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;

    // Docstring is already raw.
    if contents.starts_with('r') || contents.starts_with("ur") {
        return;
    }

    if BACKSLASH_REGEX.is_match(contents) {
        checker.diagnostics.push(Diagnostic::new(
            UsesRPrefixForBackslashedContent,
            Range::from_located(docstring.expr),
        ));
    }
}
