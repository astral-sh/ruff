use ruff_text_size::TextLen;
use strum::IntoEnumIterator;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::{StrExt, UniversalNewlineIterator};

use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::docstrings::sections::SectionKind;
use crate::registry::AsRule;
use crate::rules::pydocstyle::helpers::logical_line;

#[violation]
pub struct EndsInPunctuation;

impl AlwaysAutofixableViolation for EndsInPunctuation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("First line should end with a period, question mark, or exclamation point")
    }

    fn autofix_title(&self) -> String {
        "Add closing punctuation".to_string()
    }
}

/// D415
pub fn ends_with_punctuation(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body();

    if let Some(first_line) = body.trim().universal_newlines().next() {
        let trimmed = first_line.trim();

        // Avoid false-positives: `:param`, etc.
        for prefix in [":param", ":type", ":raises", ":return", ":rtype"] {
            if trimmed.starts_with(prefix) {
                return;
            }
        }

        // Avoid false-positives: `Args:`, etc.
        for section_kind in SectionKind::iter() {
            if let Some(suffix) = trimmed.strip_suffix(section_kind.as_str()) {
                if suffix.is_empty() {
                    return;
                }
                if suffix == ":" {
                    return;
                }
            }
        }
    }

    if let Some(index) = logical_line(body.as_str()) {
        let mut lines = UniversalNewlineIterator::with_offset(&body, body.start()).skip(index);
        let line = lines.next().unwrap();
        let trimmed = line.trim_end();

        if !trimmed.ends_with(['.', '!', '?']) {
            let mut diagnostic = Diagnostic::new(EndsInPunctuation, docstring.range());
            // Best-effort autofix: avoid adding a period after other punctuation marks.
            if checker.patch(diagnostic.kind.rule()) && !trimmed.ends_with([':', ';']) {
                diagnostic.set_fix(Edit::insertion(
                    ".".to_string(),
                    line.start() + trimmed.text_len(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        };
    }
}
