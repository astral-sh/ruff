use once_cell::sync::Lazy;
use regex::Regex;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_python_trivia::CommentRanges;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

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

impl AlwaysFixableViolation for UTF8EncodingDeclaration {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("UTF-8 encoding declaration is unnecessary")
    }

    fn fix_title(&self) -> String {
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
    comment_ranges: &CommentRanges,
) {
    // The coding comment must be on one of the first two lines. Since each comment spans at least
    // one line, we only need to check the first two comments at most.
    for comment_range in comment_ranges.iter().take(2) {
        // If leading content is not whitespace then it's not a valid coding comment e.g.
        // ```
        // print(x) # coding=utf8
        // ```
        let line_range = locator.full_line_range(comment_range.start());
        if !locator
            .slice(TextRange::new(line_range.start(), comment_range.start()))
            .trim()
            .is_empty()
        {
            continue;
        }

        // If the line is after a continuation then it's not a valid coding comment e.g.
        // ```
        // x = 1 \
        //    # coding=utf8
        // x = 2
        // ```
        if indexer
            .preceded_by_continuations(line_range.start(), locator)
            .is_some()
        {
            continue;
        }

        if CODING_COMMENT_REGEX.is_match(locator.slice(line_range)) {
            #[allow(deprecated)]
            let index = locator.compute_line_index(line_range.start());
            if index.to_zero_indexed() > 1 {
                continue;
            }

            let mut diagnostic = Diagnostic::new(UTF8EncodingDeclaration, *comment_range);
            diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
                line_range.start(),
                line_range.end(),
            )));
            diagnostics.push(diagnostic);
        }
    }
}
