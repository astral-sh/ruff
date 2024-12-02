use std::sync::LazyLock;

use regex::Regex;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_index::Indexer;
use ruff_python_trivia::CommentRanges;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;

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
#[derive(ViolationMetadata)]
pub(crate) struct UTF8EncodingDeclaration;

impl AlwaysFixableViolation for UTF8EncodingDeclaration {
    #[derive_message_formats]
    fn message(&self) -> String {
        "UTF-8 encoding declaration is unnecessary".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove unnecessary coding comment".to_string()
    }
}

// Regex from PEP263.
static CODING_COMMENT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[ \t\f]*#.*?coding[:=][ \t]*(?<name>[-_.a-zA-Z0-9]+)").unwrap());

enum CodingComment {
    UTF8(CodingCommentRanges),
    Other,
}

struct CodingCommentRanges {
    self_range: TextRange,
    line_range: TextRange,
}

/// UP009
pub(crate) fn unnecessary_coding_comment(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    indexer: &Indexer,
    comment_ranges: &CommentRanges,
) {
    // The coding comment must be on one of the first two lines. Since each comment spans at least
    // one line, we only need to check the first two comments at most.
    let coding_comments = comment_ranges
        .iter()
        .take(2)
        .map(|comment_range| coding_comment(locator, indexer, *comment_range))
        .collect::<Vec<_>>();

    match &coding_comments[..] {
        [Some(CodingComment::UTF8(ranges))]
        | [Some(CodingComment::UTF8(ranges)), None]
        | [None, Some(CodingComment::UTF8(ranges))] => {
            report(diagnostics, &ranges.line_range, &ranges.self_range);
        }

        [Some(CodingComment::UTF8(ranges_1)), Some(CodingComment::UTF8(ranges_2))] => {
            report(diagnostics, &ranges_1.line_range, &ranges_1.self_range);
            report(diagnostics, &ranges_2.line_range, &ranges_2.self_range);
        }

        _ => {}
    }
}

fn report(diagnostics: &mut Vec<Diagnostic>, line_range: &TextRange, comment_range: &TextRange) {
    let edit = Edit::deletion(line_range.start(), line_range.end());
    let fix = Fix::safe_edit(edit);

    let diagnostic = Diagnostic::new(UTF8EncodingDeclaration, *comment_range);

    diagnostics.push(diagnostic.with_fix(fix));
}

fn coding_comment(
    locator: &Locator,
    indexer: &Indexer,
    self_range: TextRange,
) -> Option<CodingComment> {
    // If leading content is not whitespace then it's not a valid coding comment e.g.
    // ```
    // print(x) # coding=utf8
    // ```
    let line_range = locator.full_line_range(self_range.start());
    if !locator
        .slice(TextRange::new(line_range.start(), self_range.start()))
        .trim()
        .is_empty()
    {
        return None;
    }

    // If the line is after a continuation then it's not a valid coding comment e.g.
    // ```
    // x = 1 \
    //    # coding=utf8
    // x = 2
    // ```
    if indexer
        .preceded_by_continuations(line_range.start(), locator.contents())
        .is_some()
    {
        return None;
    }

    let part_of_interest = CODING_COMMENT_REGEX.captures(locator.slice(line_range))?;
    let coding_name = part_of_interest.name("name")?.as_str();

    #[allow(deprecated)]
    let index = locator.compute_line_index(line_range.start());

    if index.to_zero_indexed() > 1 {
        return None;
    }

    let ranges = CodingCommentRanges {
        self_range,
        line_range,
    };

    match coding_name {
        "utf8" | "utf-8" => Some(CodingComment::UTF8(ranges)),
        _ => Some(CodingComment::Other),
    }
}
