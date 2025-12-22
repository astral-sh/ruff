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
