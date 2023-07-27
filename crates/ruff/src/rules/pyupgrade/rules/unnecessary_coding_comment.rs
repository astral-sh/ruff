use once_cell::sync::Lazy;
use regex::Regex;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_source_file::Locator;

use crate::registry::AsRule;
use crate::settings::Settings;

/// ## What it does
/// Checks for unnecessary UTF-8 encoding declarations.
///
/// ## Why is this bad?
/// [PEP 3120] makes UTF-8 the default encoding, so a UTF-8 encoding
/// declaration is unnecessary.
///
/// ## Example
/// ```python
/// # -*- coding: utf-8 -*-
/// print("Hello, world!")
/// ```
///
/// Use instead:
/// ```python
/// print("Hello, world!")
/// ```
///
/// [PEP 3120]: https://peps.python.org/pep-3120/
#[violation]
pub struct UTF8EncodingDeclaration;

impl AlwaysAutofixableViolation for UTF8EncodingDeclaration {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("UTF-8 encoding declaration is unnecessary")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary coding comment".to_string()
    }
}

// Regex from PEP263.
static CODING_COMMENT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[ \t\f]*#.*?coding[:=][ \t]*utf-?8").unwrap());

/// UP009
pub(crate) fn unnecessary_coding_comment(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    indexer: &Indexer,
    settings: &Settings,
) {
    // The coding comment must be on one of the first two lines. Since each comment spans at least
    // one line, we only need to check the first two comments at most.
    for range in indexer.comment_ranges().iter().take(2) {
        let line = locator.slice(*range);
        if CODING_COMMENT_REGEX.is_match(line) {
            #[allow(deprecated)]
            let line = locator.compute_line_index(range.start());
            if line.to_zero_indexed() > 1 {
                continue;
            }

            let mut diagnostic = Diagnostic::new(UTF8EncodingDeclaration, *range);
            if settings.rules.should_fix(diagnostic.kind.rule()) {
                diagnostic.set_fix(Fix::automatic(Edit::deletion(
                    range.start(),
                    locator.full_line_end(range.end()),
                )));
            }
            diagnostics.push(diagnostic);
        }
    }
}
