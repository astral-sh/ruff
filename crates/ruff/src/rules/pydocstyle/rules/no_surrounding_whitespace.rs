use ruff_text_size::{TextLen, TextRange};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::NewlineWithTrailingNewline;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::AsRule;
use crate::rules::pydocstyle::helpers::ends_with_backslash;

#[violation]
pub struct SurroundingWhitespace;

impl Violation for SurroundingWhitespace {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No whitespaces allowed surrounding docstring text")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Trim surrounding whitespace".to_string())
    }
}

/// D210
pub(crate) fn no_surrounding_whitespace(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body();

    let mut lines = NewlineWithTrailingNewline::from(body.as_str());
    let Some(line) = lines.next() else {
        return;
    };
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if line == trimmed {
        return;
    }
    let mut diagnostic = Diagnostic::new(SurroundingWhitespace, docstring.range());
    if checker.patch(diagnostic.kind.rule()) {
        let quote = docstring.contents.chars().last().unwrap();
        // If removing whitespace would lead to an invalid string of quote
        // characters, avoid applying the fix.
        if !trimmed.ends_with(quote) && !trimmed.starts_with(quote) && !ends_with_backslash(trimmed)
        {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                trimmed.to_string(),
                TextRange::at(body.start(), line.text_len()),
            )));
        }
    }
    checker.diagnostics.push(diagnostic);
}
