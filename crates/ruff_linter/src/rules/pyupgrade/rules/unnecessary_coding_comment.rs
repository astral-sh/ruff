use std::sync::LazyLock;

use regex::Regex;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::LineRanges;
use ruff_text_size::TextRange;

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
    comment_ranges: &CommentRanges,
) {
    let first_line_trimmed = locator.full_line_str(0.into()).trim();

    if !first_line_trimmed.is_empty() && !first_line_trimmed.starts_with('#') {
        return;
    }

    // The coding comment must be on one of the first two lines.
    // Since each comment spans at least one line,
    // we only need to check the first two comments,
    // plus a third to make sure it would not become a new coding comment.
    let mut coding_comments = comment_ranges
        .iter()
        .take(3)
        .map(|comment_range| coding_comment(locator, *comment_range));

    let first = coding_comments.next().flatten();
    let second = coding_comments.next().flatten();
    let third = coding_comments.next().flatten();

    // Table: https://github.com/astral-sh/ruff/pull/14728#issuecomment-2518114454
    match (first, second, third) {
        (Some(CodingComment::UTF8(ranges)), None | Some(CodingComment::UTF8(..)), _)
        | (None, Some(CodingComment::UTF8(ranges)), None | Some(CodingComment::UTF8(..))) => {
            report(diagnostics, ranges.line_range, ranges.self_range);
        }
        _ => {}
    }
}

fn coding_comment(locator: &Locator, self_range: TextRange) -> Option<CodingComment> {
    let line_range = locator.full_line_range(self_range.start());

    // If leading content is not whitespace then it's not a valid coding comment e.g.
    // ```
    // print(x) # coding=utf8
    // ```
    let before_hash_sign = locator.slice(TextRange::new(line_range.start(), self_range.start()));

    if !before_hash_sign.trim().is_empty() {
        return None;
    }

    let line_index = locator.count_lines(TextRange::up_to(line_range.start()));

    if line_index > 2 {
        return None;
    }

    let part_of_interest = CODING_COMMENT_REGEX.captures(locator.slice(line_range))?;
    let coding_name = part_of_interest.name("name")?.as_str();

    let ranges = CodingCommentRanges {
        self_range,
        line_range,
    };

    match coding_name {
        "utf8" | "utf-8" => Some(CodingComment::UTF8(ranges)),
        _ => Some(CodingComment::Other),
    }
}

fn report(diagnostics: &mut Vec<Diagnostic>, line_range: TextRange, comment_range: TextRange) {
    let edit = Edit::deletion(line_range.start(), line_range.end());
    let fix = Fix::safe_edit(edit);

    let diagnostic = Diagnostic::new(UTF8EncodingDeclaration, comment_range);

    diagnostics.push(diagnostic.with_fix(fix));
}
