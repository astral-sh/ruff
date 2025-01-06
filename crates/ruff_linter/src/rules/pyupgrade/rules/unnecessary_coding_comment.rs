use std::iter::FusedIterator;
use std::sync::LazyLock;

use regex::Regex;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextRange, TextSize};

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

#[derive(Debug)]
enum CodingComment {
    /// A UTF-8 encoding declaration.
    UTF8(CodingCommentRange),
    /// A declaration for any non utf8 encoding
    OtherEncoding,
    /// Any other comment
    NoEncoding,
}

#[derive(Debug)]
struct CodingCommentRange {
    comment: TextRange,
    line: TextRange,
}

/// UP009
pub(crate) fn unnecessary_coding_comment(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
) {
    let mut iter = CodingCommentIterator::new(locator, comment_ranges)
        .skip_while(|comment| matches!(comment, CodingComment::NoEncoding));

    let Some(CodingComment::UTF8(range)) = iter.next() else {
        return;
    };

    let line_index = locator.count_lines(TextRange::up_to(range.comment.start()));

    // Comment must be on the first or second line
    if line_index > 1 {
        return;
    }

    // ```python
    // # -*- coding: utf-8 -*-
    // # -*- coding: latin-1 -*-
    // ```
    // or
    // ```python
    // # -*- coding: utf-8 -*-
    // # comment
    // # -*- coding: latin-1 -*-
    // ```
    if matches!(
        (iter.next(), iter.next()),
        (Some(CodingComment::OtherEncoding), _)
            | (
                Some(CodingComment::NoEncoding),
                Some(CodingComment::OtherEncoding)
            )
    ) {
        return;
    }

    let fix = Fix::safe_edit(Edit::range_deletion(range.line));
    let diagnostic = Diagnostic::new(UTF8EncodingDeclaration, range.comment);

    diagnostics.push(diagnostic.with_fix(fix));
}

struct CodingCommentIterator<'a> {
    /// End offset of the last comment trivia or `None` if there
    /// was any non-comment comment since the iterator started (e.g. a print statement)
    last_trivia_end: Option<TextSize>,
    locator: &'a Locator<'a>,
    comments: std::slice::Iter<'a, TextRange>,
}

impl<'a> CodingCommentIterator<'a> {
    fn new(locator: &'a Locator<'a>, comments: &'a CommentRanges) -> Self {
        Self {
            last_trivia_end: Some(locator.bom_start_offset()),
            locator,
            comments: comments.iter(),
        }
    }
}

impl Iterator for CodingCommentIterator<'_> {
    type Item = CodingComment;

    fn next(&mut self) -> Option<Self::Item> {
        let comment = self.comments.next()?;
        let line_range = self.locator.full_line_range(comment.start());

        // If leading content is not whitespace then it's not a valid coding comment e.g.
        // ```py
        // print(x) # coding=utf8
        // ```
        // or
        // ```python
        // print(test)
        // # -*- coding: utf-8 -*-
        // ```
        let last_trivia_end = self.last_trivia_end.take()?;
        let before_hash_sign = self
            .locator
            .slice(TextRange::new(last_trivia_end, comment.start()));

        if !before_hash_sign.trim().is_empty() {
            return None;
        }

        self.last_trivia_end = Some(comment.end());

        let result = if let Some(parts_of_interest) =
            CODING_COMMENT_REGEX.captures(self.locator.slice(line_range))
        {
            let coding_name = parts_of_interest.name("name").unwrap();

            match coding_name.as_str() {
                "utf8" | "utf-8" => CodingComment::UTF8(CodingCommentRange {
                    comment: comment.range(),
                    line: line_range,
                }),
                _ => CodingComment::OtherEncoding,
            }
        } else {
            CodingComment::NoEncoding
        };

        Some(result)
    }
}

impl FusedIterator for CodingCommentIterator<'_> {}
