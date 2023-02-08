#![allow(dead_code)]

use rustpython_parser::ast::Location;
use rustpython_parser::lexer::Tok;

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::registry::DiagnosticKind;
use crate::source_code::Locator;
use crate::violation::Violation;

define_violation!(
    pub struct TooFewSpacesBeforeInlineComment;
);
impl Violation for TooFewSpacesBeforeInlineComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Insert at least two spaces before an inline comment")
    }
}

define_violation!(
    pub struct NoSpaceAfterInlineComment;
);
impl Violation for NoSpaceAfterInlineComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Inline comment should start with `# `")
    }
}

define_violation!(
    pub struct NoSpaceAfterBlockComment;
);
impl Violation for NoSpaceAfterBlockComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Block comment should start with `# `")
    }
}

define_violation!(
    pub struct MultipleLeadingHashesForBlockComment;
);
impl Violation for MultipleLeadingHashesForBlockComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Too many leading `#` before block comment")
    }
}

/// E261, E262, E265, E266
#[cfg(feature = "logical_lines")]
pub fn whitespace_before_comment(
    tokens: &[(Location, &Tok, Location)],
    locator: &Locator,
) -> Vec<(Range, DiagnosticKind)> {
    let mut diagnostics = vec![];
    let mut prev_end = Location::new(0, 0);
    for (start, tok, end) in tokens {
        if let Tok::Comment(text) = tok {
            let line = locator.slice_source_code_range(&Range::new(
                Location::new(start.row(), 0),
                Location::new(start.row(), start.column()),
            ));

            let is_inline_comment = !line.trim().is_empty();
            if is_inline_comment {
                if prev_end.row() == start.row() && start.column() < prev_end.column() + 2 {
                    diagnostics.push((
                        Range::new(prev_end, *start),
                        TooFewSpacesBeforeInlineComment.into(),
                    ));
                }
            }

            // Split into the portion before and after the first space.
            let mut parts = text.splitn(2, ' ');
            let symbol = parts.next().unwrap_or("");
            let comment = parts.next().unwrap_or("");

            let bad_prefix = if symbol != "#" && symbol != "#:" {
                Some(symbol.trim_start_matches('#').chars().next().unwrap_or('#'))
            } else {
                None
            };

            if is_inline_comment {
                if bad_prefix.is_some() || comment.chars().next().map_or(false, char::is_whitespace)
                {
                    diagnostics.push((Range::new(*start, *end), NoSpaceAfterInlineComment.into()));
                }
            } else if let Some(bad_prefix) = bad_prefix {
                if bad_prefix != '!' || start.row() > 1 {
                    if bad_prefix != '#' {
                        diagnostics
                            .push((Range::new(*start, *end), NoSpaceAfterBlockComment.into()));
                    } else if !comment.is_empty() {
                        diagnostics.push((
                            Range::new(*start, *end),
                            MultipleLeadingHashesForBlockComment.into(),
                        ));
                    }
                }
            }
        } else if !matches!(tok, Tok::NonLogicalNewline) {
            prev_end = *end;
        }
    }
    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn whitespace_before_comment(
    _tokens: &[(Location, &Tok, Location)],
    _locator: &Locator,
) -> Vec<(Range, DiagnosticKind)> {
    vec![]
}
