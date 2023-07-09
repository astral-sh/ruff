//! Struct used to index source code, to enable efficient lookup of tokens that
//! are omitted from the AST (e.g., commented lines).

use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::lexer::LexResult;
use rustpython_parser::{StringKind, Tok};

use crate::source_code::comment_ranges::{CommentRanges, CommentRangesBuilder};
use crate::source_code::Locator;

pub struct Indexer {
    comment_ranges: CommentRanges,

    /// Stores the start offset of continuation lines.
    continuation_lines: Vec<TextSize>,

    /// The range of all triple quoted strings in the source document. The ranges are sorted by their
    /// [`TextRange::start`] position in increasing order. No two ranges are overlapping.
    triple_quoted_string_ranges: Vec<TextRange>,

    /// The range of all f-string in the source document. The ranges are sorted by their
    /// [`TextRange::start`] position in increasing order. No two ranges are overlapping.
    f_string_ranges: Vec<TextRange>,
}

impl Indexer {
    pub fn from_tokens(tokens: &[LexResult], locator: &Locator) -> Self {
        assert!(TextSize::try_from(locator.contents().len()).is_ok());

        let mut comment_ranges_builder = CommentRangesBuilder::default();
        let mut continuation_lines = Vec::new();
        let mut triple_quoted_string_ranges = Vec::new();
        let mut f_string_ranges = Vec::new();
        // Token, end
        let mut prev_end = TextSize::default();
        let mut prev_token: Option<&Tok> = None;
        let mut line_start = TextSize::default();

        for (tok, range) in tokens.iter().flatten() {
            let trivia = &locator.contents()[TextRange::new(prev_end, range.start())];

            // Get the trivia between the previous and the current token and detect any newlines.
            // This is necessary because `RustPython` doesn't emit `[Tok::Newline]` tokens
            // between any two tokens that form a continuation. That's why we have to extract the
            // newlines "manually".
            for (index, text) in trivia.match_indices(['\n', '\r']) {
                if text == "\r" && trivia.as_bytes().get(index + 1) == Some(&b'\n') {
                    continue;
                }

                // Newlines after a newline never form a continuation.
                if !matches!(prev_token, Some(Tok::Newline | Tok::NonLogicalNewline)) {
                    continuation_lines.push(line_start);
                }

                // SAFETY: Safe because of the len assertion at the top of the function.
                #[allow(clippy::cast_possible_truncation)]
                {
                    line_start = prev_end + TextSize::new((index + 1) as u32);
                }
            }

            comment_ranges_builder.visit_token(tok, *range);

            match tok {
                Tok::Newline | Tok::NonLogicalNewline => {
                    line_start = range.end();
                }
                Tok::String {
                    triple_quoted: true,
                    ..
                } => {
                    triple_quoted_string_ranges.push(*range);
                }
                Tok::String {
                    kind: StringKind::FString | StringKind::RawFString,
                    ..
                } => {
                    f_string_ranges.push(*range);
                }
                _ => {}
            }

            prev_token = Some(tok);
            prev_end = range.end();
        }
        Self {
            comment_ranges: comment_ranges_builder.finish(),
            continuation_lines,
            triple_quoted_string_ranges,
            f_string_ranges,
        }
    }

    /// Returns the byte offset ranges of comments
    pub fn comment_ranges(&self) -> &CommentRanges {
        &self.comment_ranges
    }

    /// Returns the line start positions of continuations (backslash).
    pub fn continuation_line_starts(&self) -> &[TextSize] {
        &self.continuation_lines
    }

    /// Returns `true` if the given offset is part of a continuation line.
    pub fn is_continuation(&self, offset: TextSize, locator: &Locator) -> bool {
        let line_start = locator.line_start(offset);
        self.continuation_lines.binary_search(&line_start).is_ok()
    }

    /// Return the [`TextRange`] of the triple-quoted-string containing a given offset.
    pub fn triple_quoted_string_range(&self, offset: TextSize) -> Option<TextRange> {
        let Ok(string_range_index) = self.triple_quoted_string_ranges.binary_search_by(|range| {
            if offset < range.start() {
                std::cmp::Ordering::Greater
            } else if range.contains(offset) {
                std::cmp::Ordering::Equal
            } else {
                std::cmp::Ordering::Less
            }
        }) else {
            return None;
        };
        Some(self.triple_quoted_string_ranges[string_range_index])
    }

    /// Return the [`TextRange`] of the f-string containing a given offset.
    pub fn f_string_range(&self, offset: TextSize) -> Option<TextRange> {
        let Ok(string_range_index) = self.f_string_ranges.binary_search_by(|range| {
            if offset < range.start() {
                std::cmp::Ordering::Greater
            } else if range.contains(offset) {
                std::cmp::Ordering::Equal
            } else {
                std::cmp::Ordering::Less
            }
        }) else {
            return None;
        };
        Some(self.f_string_ranges[string_range_index])
    }
}

#[cfg(test)]
mod tests {
    use ruff_text_size::{TextRange, TextSize};
    use rustpython_parser::lexer::LexResult;
    use rustpython_parser::{lexer, Mode};

    use crate::source_code::{Indexer, Locator};

    #[test]
    fn continuation() {
        let contents = r#"x = 1"#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(&lxr, &Locator::new(contents));
        assert_eq!(indexer.continuation_line_starts(), &[]);

        let contents = r#"
        # Hello, world!

x = 1

y = 2
        "#
        .trim();

        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(&lxr, &Locator::new(contents));
        assert_eq!(indexer.continuation_line_starts(), &[]);

        let contents = r#"
x = \
    1

if True:
    z = \
        \
        2

(
    "abc" # Foo
    "def" \
    "ghi"
)
"#
        .trim();
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(lxr.as_slice(), &Locator::new(contents));
        assert_eq!(
            indexer.continuation_line_starts(),
            [
                // row 1
                TextSize::from(0),
                // row 5
                TextSize::from(22),
                // row 6
                TextSize::from(32),
                // row 11
                TextSize::from(71),
            ]
        );

        let contents = r#"
x = 1; import sys
import os

if True:
    x = 1; import sys
    import os

if True:
    x = 1; \
        import os

x = 1; \
import os
"#
        .trim();
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(lxr.as_slice(), &Locator::new(contents));
        assert_eq!(
            indexer.continuation_line_starts(),
            [
                // row 9
                TextSize::from(84),
                // row 12
                TextSize::from(116)
            ]
        );
    }

    #[test]
    fn string_ranges() {
        let contents = r#""this is a single-quoted string""#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(lxr.as_slice(), &Locator::new(contents));
        assert_eq!(indexer.triple_quoted_string_ranges, []);

        let contents = r#"
            """
            this is a multiline string
            """
            "#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(lxr.as_slice(), &Locator::new(contents));
        assert_eq!(
            indexer.triple_quoted_string_ranges,
            [TextRange::new(TextSize::from(13), TextSize::from(71))]
        );

        let contents = r#"
            """
            '''this is a multiline string with multiple delimiter types'''
            """
            "#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(lxr.as_slice(), &Locator::new(contents));
        assert_eq!(
            indexer.triple_quoted_string_ranges,
            [TextRange::new(TextSize::from(13), TextSize::from(107))]
        );

        let contents = r#"
            """
            this is one
            multiline string
            """
            """
            and this is
            another
            """
            "#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(lxr.as_slice(), &Locator::new(contents));
        assert_eq!(
            indexer.triple_quoted_string_ranges,
            &[
                TextRange::new(TextSize::from(13), TextSize::from(85)),
                TextRange::new(TextSize::from(98), TextSize::from(161))
            ]
        );
    }
}
