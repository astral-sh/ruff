use std::error::Error;

use crate::suppression::SuppressionKind;
use ruff_python_trivia::Cursor;
use ruff_text_size::{TextLen, TextRange, TextSize};
use smallvec::{SmallVec, smallvec};
use thiserror::Error;

pub(super) struct SuppressionParser<'src> {
    cursor: Cursor<'src>,
    range: TextRange,
}

impl<'src> SuppressionParser<'src> {
    pub(super) fn new(source: &'src str, range: TextRange) -> Self {
        let cursor = Cursor::new(&source[range]);

        Self { cursor, range }
    }

    fn parse_comment(&mut self) -> Result<SuppressionComment, ParseError> {
        let comment_start = self.offset();
        self.cursor.start_token();

        if !self.cursor.eat_char('#') {
            return self.syntax_error(ParseErrorKind::CommentWithoutHash);
        }

        self.eat_whitespace();

        // type: ignore[code]
        // ^^^^^^^^^^^^
        let Some(kind) = self.eat_kind() else {
            return Err(ParseError::new(
                ParseErrorKind::NotASuppression,
                TextRange::new(comment_start, self.offset()),
            ));
        };

        let has_trailing_whitespace = self.eat_whitespace();

        // type: ignore[code1, code2]
        //             ^^^^^^
        let codes = self.eat_codes(kind)?;

        if self.cursor.is_eof() || codes.is_some() || has_trailing_whitespace {
            // Consume the comment until its end or until the next "sub-comment" starts.
            self.cursor.eat_while(|c| c != '#');
            Ok(SuppressionComment {
                kind,
                codes,
                range: TextRange::at(comment_start, self.cursor.token_len()),
            })
        } else {
            self.syntax_error(ParseErrorKind::NoWhitespaceAfterIgnore(kind))
        }
    }

    fn eat_kind(&mut self) -> Option<SuppressionKind> {
        let kind = if self.cursor.as_str().starts_with("type") {
            SuppressionKind::TypeIgnore
        } else if self.cursor.as_str().starts_with("ty") {
            SuppressionKind::Ty
        } else {
            return None;
        };

        self.cursor.skip_bytes(kind.len_utf8());

        self.eat_whitespace();

        if !self.cursor.eat_char(':') {
            return None;
        }

        self.eat_whitespace();

        if !self.cursor.as_str().starts_with("ignore") {
            return None;
        }

        self.cursor.skip_bytes("ignore".len());

        Some(kind)
    }

    fn eat_codes(
        &mut self,
        kind: SuppressionKind,
    ) -> Result<Option<SmallVec<[TextRange; 2]>>, ParseError> {
        if !self.cursor.eat_char('[') {
            return Ok(None);
        }

        let mut codes: SmallVec<[TextRange; 2]> = smallvec![];

        loop {
            if self.cursor.is_eof() {
                return self.syntax_error(ParseErrorKind::CodesMissingClosingBracket(kind));
            }

            self.eat_whitespace();

            // `ty: ignore[]` or `ty: ignore[a,]`
            if self.cursor.eat_char(']') {
                break Ok(Some(codes));
            }

            let code_start = self.offset();
            if !self.eat_word() {
                return self.syntax_error(ParseErrorKind::InvalidCode(kind));
            }

            codes.push(TextRange::new(code_start, self.offset()));

            self.eat_whitespace();

            if !self.cursor.eat_char(',') {
                if self.cursor.eat_char(']') {
                    break Ok(Some(codes));
                }
                // `ty: ignore[a b]
                return self.syntax_error(ParseErrorKind::CodesMissingComma(kind));
            }
        }
    }

    fn eat_whitespace(&mut self) -> bool {
        if self.cursor.eat_if(char::is_whitespace) {
            self.cursor.eat_while(char::is_whitespace);
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self) -> bool {
        if self.cursor.eat_if(char::is_alphabetic) {
            // Allow `:` for better error recovery when someone uses `lint:code` instead of just `code`.
            self.cursor
                .eat_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ':'));
            true
        } else {
            false
        }
    }

    fn syntax_error<T>(&self, kind: ParseErrorKind) -> Result<T, ParseError> {
        let len = if self.cursor.is_eof() {
            TextSize::default()
        } else {
            self.cursor.first().text_len()
        };

        Err(ParseError::new(kind, TextRange::at(self.offset(), len)))
    }

    fn offset(&self) -> TextSize {
        self.range.start() + self.range.len() - self.cursor.text_len()
    }
}

impl Iterator for SuppressionParser<'_> {
    type Item = Result<SuppressionComment, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor.is_eof() {
            return None;
        }

        match self.parse_comment() {
            Ok(result) => Some(Ok(result)),
            Err(error) => {
                self.cursor.eat_while(|c| c != '#');
                Some(Err(error))
            }
        }
    }
}

/// A single parsed suppression comment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SuppressionComment {
    /// The range of the suppression comment.
    ///
    /// This can be a sub-range of the comment token if the comment token contains multiple `#` tokens:
    /// ```py
    /// # fmt: off # type: ignore
    ///            ^^^^^^^^^^^^^^
    /// ```
    range: TextRange,

    kind: SuppressionKind,

    /// The ranges of the codes in the optional `[...]`.
    /// `None` for comments that don't specify any code.
    ///
    /// ```py
    /// # type: ignore[unresolved-reference, invalid-exception-caught]
    ///                ^^^^^^^^^^^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    codes: Option<SmallVec<[TextRange; 2]>>,
}

impl SuppressionComment {
    pub(super) fn kind(&self) -> SuppressionKind {
        self.kind
    }

    pub(super) fn codes(&self) -> Option<&[TextRange]> {
        self.codes.as_deref()
    }

    pub(super) fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Debug, Eq, PartialEq, Clone, get_size2::GetSize)]
pub(super) struct ParseError {
    pub(super) kind: ParseErrorKind,

    /// The position/range at which the parse error occurred.
    pub(super) range: TextRange,
}

impl ParseError {
    fn new(kind: ParseErrorKind, range: TextRange) -> Self {
        Self { kind, range }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.kind.fmt(f)
    }
}

impl Error for ParseError {}

#[derive(Debug, Eq, PartialEq, Clone, Error, get_size2::GetSize)]
pub(super) enum ParseErrorKind {
    /// The comment isn't a suppression comment.
    #[error("not a suppression comment")]
    NotASuppression,

    #[error("the comment doesn't start with a `#`")]
    CommentWithoutHash,

    /// A valid suppression `type: ignore` but it misses a whitespaces after the `ignore` keyword.
    ///
    /// ```py
    /// type: ignoree
    /// ```
    #[error("no whitespace after `ignore`")]
    NoWhitespaceAfterIgnore(SuppressionKind),

    /// Missing comma between two codes
    #[error("expected a comma separating the rule codes")]
    CodesMissingComma(SuppressionKind),

    /// `ty: ignore[*.*]`
    #[error("expected a alphanumeric character or `-` or `_` as code")]
    InvalidCode(SuppressionKind),

    /// `ty: ignore[a, b`
    #[error("expected a closing bracket")]
    CodesMissingClosingBracket(SuppressionKind),
}

#[cfg(test)]
mod tests {
    use crate::suppression::{SuppressionComment, SuppressionParser};
    use insta::assert_debug_snapshot;
    use ruff_text_size::{TextLen, TextRange};
    use std::fmt;
    use std::fmt::Formatter;

    #[test]
    fn type_ignore_no_codes() {
        assert_debug_snapshot!(
            SuppressionComments::new(
                "# type: ignore",
            ),
            @r##"
        [
            SuppressionComment {
                text: "# type: ignore",
                kind: TypeIgnore,
                codes: [],
            },
        ]
        "##
        );
    }

    #[test]
    fn type_ignore_explanation() {
        assert_debug_snapshot!(
            SuppressionComments::new(
                "# type: ignore I tried but couldn't figure out the proper type",
            ),
            @r##"
        [
            SuppressionComment {
                text: "# type: ignore I tried but couldn't figure out the proper type",
                kind: TypeIgnore,
                codes: [],
            },
        ]
        "##
        );
    }

    #[test]
    fn fmt_comment_before_type_ignore() {
        assert_debug_snapshot!(
            SuppressionComments::new(
                "# fmt: off   # type: ignore",
            ),
            @r##"
        [
            SuppressionComment {
                text: "# type: ignore",
                kind: TypeIgnore,
                codes: [],
            },
        ]
        "##
        );
    }

    #[test]
    fn type_ignore_before_fmt_off() {
        assert_debug_snapshot!(
            SuppressionComments::new(
                "# type: ignore  # fmt: off",
            ),
            @r##"
        [
            SuppressionComment {
                text: "# type: ignore  ",
                kind: TypeIgnore,
                codes: [],
            },
        ]
        "##
        );
    }

    #[test]
    fn multiple_type_ignore_comments() {
        assert_debug_snapshot!(
            SuppressionComments::new(
                "# type: ignore[a]  # type: ignore[b]",
            ),
            @r##"
        [
            SuppressionComment {
                text: "# type: ignore[a]  ",
                kind: TypeIgnore,
                codes: [
                    "a",
                ],
            },
            SuppressionComment {
                text: "# type: ignore[b]",
                kind: TypeIgnore,
                codes: [
                    "b",
                ],
            },
        ]
        "##
        );
    }

    #[test]
    fn invalid_type_ignore_valid_type_ignore() {
        assert_debug_snapshot!(
            SuppressionComments::new(
                "# type: ignore[a  # type: ignore[b]",
            ),
            @r##"
        [
            SuppressionComment {
                text: "# type: ignore[b]",
                kind: TypeIgnore,
                codes: [
                    "b",
                ],
            },
        ]
        "##
        );
    }

    #[test]
    fn valid_type_ignore_invalid_type_ignore() {
        assert_debug_snapshot!(
            SuppressionComments::new(
                "# type: ignore[a]  # type: ignoreeee",
            ),
            @r##"
        [
            SuppressionComment {
                text: "# type: ignore[a]  ",
                kind: TypeIgnore,
                codes: [
                    "a",
                ],
            },
        ]
        "##
        );
    }

    #[test]
    fn type_ignore_multiple_codes() {
        assert_debug_snapshot!(
            SuppressionComments::new(
                "# type: ignore[invalid-exception-raised, invalid-exception-caught]",
            ),
            @r##"
        [
            SuppressionComment {
                text: "# type: ignore[invalid-exception-raised, invalid-exception-caught]",
                kind: TypeIgnore,
                codes: [
                    "invalid-exception-raised",
                    "invalid-exception-caught",
                ],
            },
        ]
        "##
        );
    }

    #[test]
    fn type_ignore_single_code() {
        assert_debug_snapshot!(
            SuppressionComments::new("# type: ignore[invalid-exception-raised]",),
            @r##"
        [
            SuppressionComment {
                text: "# type: ignore[invalid-exception-raised]",
                kind: TypeIgnore,
                codes: [
                    "invalid-exception-raised",
                ],
            },
        ]
        "##
        );
    }

    struct SuppressionComments<'a> {
        source: &'a str,
    }

    impl<'a> SuppressionComments<'a> {
        fn new(source: &'a str) -> Self {
            Self { source }
        }
    }

    impl fmt::Debug for SuppressionComments<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let mut list = f.debug_list();

            for comment in SuppressionParser::new(
                self.source,
                TextRange::new(0.into(), self.source.text_len()),
            )
            .flatten()
            {
                list.entry(&comment.debug(self.source));
            }

            list.finish()
        }
    }

    impl SuppressionComment {
        fn debug<'a>(&'a self, source: &'a str) -> DebugSuppressionComment<'a> {
            DebugSuppressionComment {
                source,
                comment: self,
            }
        }
    }

    struct DebugSuppressionComment<'a> {
        source: &'a str,
        comment: &'a SuppressionComment,
    }

    impl fmt::Debug for DebugSuppressionComment<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            struct DebugCodes<'a> {
                source: &'a str,
                codes: &'a [TextRange],
            }

            impl fmt::Debug for DebugCodes<'_> {
                fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                    let mut f = f.debug_list();

                    for code in self.codes {
                        f.entry(&&self.source[*code]);
                    }

                    f.finish()
                }
            }

            f.debug_struct("SuppressionComment")
                .field("text", &&self.source[self.comment.range])
                .field("kind", &self.comment.kind)
                .field(
                    "codes",
                    &DebugCodes {
                        source: self.source,
                        codes: self.comment.codes.as_deref().unwrap_or_default(),
                    },
                )
                .finish()
        }
    }
}
