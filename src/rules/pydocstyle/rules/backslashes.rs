use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::rules::regexes::BACKSLASH_REGEX;
use crate::violations;

/// D301
pub fn backslashes(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;

    // Docstring is already raw.
    if contents.starts_with('r') || contents.starts_with("ur") {
        return;
    }

    if BACKSLASH_REGEX.is_match(contents) {
        checker.diagnostics.push(Diagnostic::new(
            violations::UsesRPrefixForBackslashedContent,
            Range::from_located(docstring.expr),
        ));
    }
}
