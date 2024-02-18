//! Struct used to index source code, to enable efficient lookup of tokens that
//! are omitted from the AST (e.g., commented lines).

use ruff_python_ast::Stmt;
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;
use ruff_python_trivia::{
    has_leading_content, has_trailing_content, is_python_whitespace, CommentRanges,
};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::fstring_ranges::{FStringRanges, FStringRangesBuilder};
use crate::multiline_ranges::{MultilineRanges, MultilineRangesBuilder};
use crate::CommentRangesBuilder;

pub struct Indexer {
    comment_ranges: CommentRanges,

    /// Stores the start offset of continuation lines.
    continuation_lines: Vec<TextSize>,

    /// The range of all f-string in the source document.
    fstring_ranges: FStringRanges,

    /// The range of all multiline strings in the source document.
    multiline_ranges: MultilineRanges,
}

impl Indexer {
    pub fn from_tokens(tokens: &[LexResult], locator: &Locator) -> Self {
        assert!(TextSize::try_from(locator.contents().len()).is_ok());

        let mut comment_ranges_builder = CommentRangesBuilder::default();
        let mut fstring_ranges_builder = FStringRangesBuilder::default();
        let mut multiline_ranges_builder = MultilineRangesBuilder::default();
        let mut continuation_lines = Vec::new();
        // Token, end
        let mut prev_end = TextSize::default();
        let mut prev_token: Option<&Tok> = None;
        let mut line_start = TextSize::default();

        for (tok, range) in tokens.iter().flatten() {
            let trivia = locator.slice(TextRange::new(prev_end, range.start()));

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
            fstring_ranges_builder.visit_token(tok, *range);
            multiline_ranges_builder.visit_token(tok, *range);

            match tok {
                Tok::Newline | Tok::NonLogicalNewline => {
                    line_start = range.end();
                }
                Tok::String { .. } => {
                    // If the previous token was a string, find the start of the line that contains
                    // the closing delimiter, since the token itself can span multiple lines.
                    line_start = locator.line_start(range.end());
                }
                _ => {}
            }

            prev_token = Some(tok);
            prev_end = range.end();
        }

        Self {
            comment_ranges: comment_ranges_builder.finish(),
            continuation_lines,
            fstring_ranges: fstring_ranges_builder.finish(),
            multiline_ranges: multiline_ranges_builder.finish(),
        }
    }

    /// Returns the byte offset ranges of comments
    pub const fn comment_ranges(&self) -> &CommentRanges {
        &self.comment_ranges
    }

    /// Returns the byte offset ranges of f-strings.
    pub const fn fstring_ranges(&self) -> &FStringRanges {
        &self.fstring_ranges
    }

    /// Returns the byte offset ranges of multiline strings.
    pub const fn multiline_ranges(&self) -> &MultilineRanges {
        &self.multiline_ranges
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

    /// Returns `true` if a statement or expression includes at least one comment.
    pub fn has_comments<T>(&self, node: &T, locator: &Locator) -> bool
    where
        T: Ranged,
    {
        let start = if has_leading_content(node.start(), locator) {
            node.start()
        } else {
            locator.line_start(node.start())
        };
        let end = if has_trailing_content(node.end(), locator) {
            node.end()
        } else {
            locator.line_end(node.end())
        };

        self.comment_ranges().intersects(TextRange::new(start, end))
    }

    /// Given an offset at the end of a line (including newlines), return the offset of the
    /// continuation at the end of that line.
    fn find_continuation(&self, offset: TextSize, locator: &Locator) -> Option<TextSize> {
        let newline_pos = usize::from(offset).saturating_sub(1);

        // Skip the newline.
        let newline_len = match locator.contents().as_bytes()[newline_pos] {
            b'\n' => {
                if locator
                    .contents()
                    .as_bytes()
                    .get(newline_pos.saturating_sub(1))
                    == Some(&b'\r')
                {
                    2
                } else {
                    1
                }
            }
            b'\r' => 1,
            // No preceding line.
            _ => return None,
        };

        self.is_continuation(offset - TextSize::from(newline_len), locator)
            .then(|| offset - TextSize::from(newline_len) - TextSize::from(1))
    }

    /// If the node starting at the given [`TextSize`] is preceded by at least one continuation line
    /// (i.e., a line ending in a backslash), return the starting offset of the first such continuation
    /// character.
    ///
    /// For example, given:
    /// ```python
    /// x = 1; \
    ///    y = 2
    /// ```
    ///
    /// When passed the offset of `y`, this function will return the offset of the backslash at the end
    /// of the first line.
    ///
    /// Similarly, given:
    /// ```python
    /// x = 1; \
    ///        \
    ///   y = 2;
    /// ```
    ///
    /// When passed the offset of `y`, this function will again return the offset of the backslash at
    /// the end of the first line.
    pub fn preceded_by_continuations(
        &self,
        offset: TextSize,
        locator: &Locator,
    ) -> Option<TextSize> {
        // Find the first preceding continuation. If the offset isn't the first non-whitespace
        // character on the line, then we can't have a continuation.
        let previous_line_end = locator.line_start(offset);
        if !locator
            .slice(TextRange::new(previous_line_end, offset))
            .chars()
            .all(is_python_whitespace)
        {
            return None;
        }

        let mut continuation = self.find_continuation(previous_line_end, locator)?;

        // Continue searching for continuations, in the unlikely event that we have multiple
        // continuations in a row.
        loop {
            let previous_line_end = locator.line_start(continuation);
            if locator
                .slice(TextRange::new(previous_line_end, continuation))
                .chars()
                .all(is_python_whitespace)
            {
                if let Some(next_continuation) = self.find_continuation(previous_line_end, locator)
                {
                    continuation = next_continuation;
                    continue;
                }
            }
            break;
        }

        Some(continuation)
    }

    /// Return `true` if a [`Stmt`] appears to be preceded by other statements in a multi-statement
    /// line.
    pub fn preceded_by_multi_statement_line(&self, stmt: &Stmt, locator: &Locator) -> bool {
        has_leading_content(stmt.start(), locator)
            || self
                .preceded_by_continuations(stmt.start(), locator)
                .is_some()
    }

    /// Return `true` if a [`Stmt`] appears to be followed by other statements in a multi-statement
    /// line.
    pub fn followed_by_multi_statement_line(&self, stmt: &Stmt, locator: &Locator) -> bool {
        has_trailing_content(stmt.end(), locator)
    }

    /// Return `true` if a [`Stmt`] appears to be part of a multi-statement line.
    pub fn in_multi_statement_line(&self, stmt: &Stmt, locator: &Locator) -> bool {
        self.followed_by_multi_statement_line(stmt, locator)
            || self.preceded_by_multi_statement_line(stmt, locator)
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_parser::lexer::LexResult;
    use ruff_python_parser::{lexer, Mode};
    use ruff_source_file::Locator;
    use ruff_text_size::{TextRange, TextSize};

    use crate::Indexer;

    #[test]
    fn continuation() {
        let contents = r"x = 1";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(&lxr, &Locator::new(contents));
        assert_eq!(indexer.continuation_line_starts(), &[]);

        let contents = r"
        # Hello, world!

x = 1

y = 2
        "
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

        let contents = r"
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
"
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

        let contents = r"
f'foo { 'str1' \
    'str2' \
    'str3'
    f'nested { 'str4'
        'str5' \
        'str6'
    }'
}'
"
        .trim();
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(lxr.as_slice(), &Locator::new(contents));
        assert_eq!(
            indexer.continuation_line_starts(),
            [
                // row 1
                TextSize::new(0),
                // row 2
                TextSize::new(17),
                // row 5
                TextSize::new(63),
            ]
        );
    }

    #[test]
    fn test_f_string_ranges() {
        let contents = r#"
f"normal f-string"
f"start {f"inner {f"another"}"} end"
f"implicit " f"concatenation"
"#
        .trim();
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(lxr.as_slice(), &Locator::new(contents));
        assert_eq!(
            indexer
                .fstring_ranges()
                .values()
                .copied()
                .collect::<Vec<_>>(),
            &[
                TextRange::new(TextSize::from(0), TextSize::from(18)),
                TextRange::new(TextSize::from(19), TextSize::from(55)),
                TextRange::new(TextSize::from(28), TextSize::from(49)),
                TextRange::new(TextSize::from(37), TextSize::from(47)),
                TextRange::new(TextSize::from(56), TextSize::from(68)),
                TextRange::new(TextSize::from(69), TextSize::from(85)),
            ]
        );
    }

    #[test]
    fn test_triple_quoted_f_string_ranges() {
        let contents = r#"
f"""
this is one
multiline f-string
"""
f'''
and this is
another
'''
f"""
this is a {f"""nested multiline
f-string"""}
"""
"#
        .trim();
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(lxr.as_slice(), &Locator::new(contents));
        assert_eq!(
            indexer
                .fstring_ranges()
                .values()
                .copied()
                .collect::<Vec<_>>(),
            &[
                TextRange::new(TextSize::from(0), TextSize::from(39)),
                TextRange::new(TextSize::from(40), TextSize::from(68)),
                TextRange::new(TextSize::from(69), TextSize::from(122)),
                TextRange::new(TextSize::from(85), TextSize::from(117)),
            ]
        );
    }

    #[test]
    fn test_fstring_innermost_outermost() {
        let contents = r#"
f"no nested f-string"

if True:
    f"first {f"second {f"third"} second"} first"
    foo = "normal string"

f"implicit " f"concatenation"

f"first line {
    foo + f"second line {bar}"
} third line"

f"""this is a
multi-line {f"""nested
f-string"""}
the end"""
"#
        .trim();
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer = Indexer::from_tokens(lxr.as_slice(), &Locator::new(contents));

        // For reference, the ranges of the f-strings in the above code are as
        // follows where the ones inside parentheses are nested f-strings:
        //
        // [0..21, (36..80, 45..72, 55..63), 108..120, 121..137, (139..198, 164..184), (200..260, 226..248)]

        for (offset, innermost_range, outermost_range) in [
            // Inside a normal f-string
            (
                TextSize::new(130),
                TextRange::new(TextSize::new(121), TextSize::new(137)),
                TextRange::new(TextSize::new(121), TextSize::new(137)),
            ),
            // Left boundary
            (
                TextSize::new(121),
                TextRange::new(TextSize::new(121), TextSize::new(137)),
                TextRange::new(TextSize::new(121), TextSize::new(137)),
            ),
            // Right boundary
            (
                TextSize::new(136), // End offsets are exclusive
                TextRange::new(TextSize::new(121), TextSize::new(137)),
                TextRange::new(TextSize::new(121), TextSize::new(137)),
            ),
            // "first" left
            (
                TextSize::new(40),
                TextRange::new(TextSize::new(36), TextSize::new(80)),
                TextRange::new(TextSize::new(36), TextSize::new(80)),
            ),
            // "second" left
            (
                TextSize::new(50),
                TextRange::new(TextSize::new(45), TextSize::new(72)),
                TextRange::new(TextSize::new(36), TextSize::new(80)),
            ),
            // "third"
            (
                TextSize::new(60),
                TextRange::new(TextSize::new(55), TextSize::new(63)),
                TextRange::new(TextSize::new(36), TextSize::new(80)),
            ),
            // "second" right
            (
                TextSize::new(70),
                TextRange::new(TextSize::new(45), TextSize::new(72)),
                TextRange::new(TextSize::new(36), TextSize::new(80)),
            ),
            // "first" right
            (
                TextSize::new(75),
                TextRange::new(TextSize::new(36), TextSize::new(80)),
                TextRange::new(TextSize::new(36), TextSize::new(80)),
            ),
            // Single-quoted f-strings spanning across multiple lines
            (
                TextSize::new(160),
                TextRange::new(TextSize::new(139), TextSize::new(198)),
                TextRange::new(TextSize::new(139), TextSize::new(198)),
            ),
            (
                TextSize::new(170),
                TextRange::new(TextSize::new(164), TextSize::new(184)),
                TextRange::new(TextSize::new(139), TextSize::new(198)),
            ),
            // Multi-line f-strings
            (
                TextSize::new(220),
                TextRange::new(TextSize::new(200), TextSize::new(260)),
                TextRange::new(TextSize::new(200), TextSize::new(260)),
            ),
            (
                TextSize::new(240),
                TextRange::new(TextSize::new(226), TextSize::new(248)),
                TextRange::new(TextSize::new(200), TextSize::new(260)),
            ),
        ] {
            assert_eq!(
                indexer.fstring_ranges().innermost(offset).unwrap(),
                innermost_range
            );
            assert_eq!(
                indexer.fstring_ranges().outermost(offset).unwrap(),
                outermost_range
            );
        }
    }
}
