use core::fmt;
use itertools::Itertools;
use std::{error::Error, fmt::Formatter};
use thiserror::Error;

use ruff_python_trivia::{CommentRanges, Cursor};
use ruff_text_size::{TextRange, TextSize};
use smallvec::{SmallVec, smallvec};

#[derive(Clone, Debug, Eq, PartialEq)]
enum SuppressionAction {
    Disable,
    Enable,
    Ignore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SuppressionKind {
    /// multi-line range suppression
    Range,

    /// next-line suppression
    NextLine,

    /// end-of-line suppression
    EndOfLine,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct SuppressionComment {
    /// Range containing the entire suppression comment
    range: TextRange,

    /// The action directive
    action: SuppressionAction,

    /// Ranges containing the lint codes being suppressed
    codes: SmallVec<[TextRange; 2]>,

    /// Range containing the reason for the suppression
    reason: TextRange,
}

#[derive(Debug)]
pub(crate) struct Suppression {
    kind: SuppressionKind,

    /// The lint code being suppressed
    code: String,

    /// Range for which the suppression applies
    range: TextRange,

    /// Any comments associated with the suppression
    comments: Vec<SuppressionComment>,
}

impl Suppression {
    pub(crate) fn load(source: &str, comment_ranges: &CommentRanges) -> Vec<SuppressionComment> {
        comment_ranges
            .iter()
            .flat_map(|comment_range| {
                let mut parser = SuppressionParser::new(source, comment_range);
                parser.parse_comment()
            })
            .collect::<Vec<_>>()
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
enum ParseErrorKind {
    #[error("not a suppression comment")]
    NotASuppression,

    #[error("comment doesn't start with `#`")]
    CommentWithoutHash,

    #[error("unknown ruff directive")]
    UnknownDirective,

    #[error("missing suppression codes")]
    MissingCodes,

    #[error("missing closing bracket")]
    MissingBracket,

    #[error("missing comma between codes")]
    MissingComma,

    #[error("invalid error code")]
    InvalidCode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParseError {
    kind: ParseErrorKind,
    range: TextRange,
}

impl ParseError {
    fn new(kind: ParseErrorKind, range: TextRange) -> Self {
        Self { kind, range }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.kind.fmt(f)
    }
}

impl Error for ParseError {}

struct SuppressionParser<'src> {
    cursor: Cursor<'src>,
    range: TextRange,
}

impl<'src> SuppressionParser<'src> {
    fn new(source: &'src str, range: &TextRange) -> Self {
        let range = range.clone();
        let cursor = Cursor::new(&source[range]);
        Self { cursor, range }
    }

    fn parse_comment(&mut self) -> Result<SuppressionComment, ParseError> {
        self.cursor.start_token();

        if !self.cursor.eat_char('#') {
            return self.error(ParseErrorKind::CommentWithoutHash);
        }

        self.eat_whitespace();

        let action = self.eat_action()?;
        let codes = self.eat_codes()?;
        if codes.is_empty() {
            return Err(ParseError::new(ParseErrorKind::MissingCodes, self.range));
        }

        self.eat_whitespace();
        let reason = TextRange::new(self.offset(), self.range.end());

        Ok(SuppressionComment {
            range: self.range,
            action,
            codes,
            reason,
        })
    }

    fn eat_action(&mut self) -> Result<SuppressionAction, ParseError> {
        if !self.cursor.as_str().starts_with("ruff") {
            return self.error(ParseErrorKind::NotASuppression);
        }

        self.cursor.skip_bytes("ruff".len());
        self.eat_whitespace();

        if !self.cursor.eat_char(':') {
            return self.error(ParseErrorKind::NotASuppression);
        }
        self.eat_whitespace();

        if self.cursor.as_str().starts_with("disable") {
            self.cursor.skip_bytes("disable".len());
            Ok(SuppressionAction::Disable)
        } else if self.cursor.as_str().starts_with("enable") {
            self.cursor.skip_bytes("enable".len());
            Ok(SuppressionAction::Enable)
        } else if self.cursor.as_str().starts_with("ignore") {
            self.cursor.skip_bytes("ignore".len());
            Ok(SuppressionAction::Ignore)
        } else {
            self.error(ParseErrorKind::UnknownDirective)
        }
    }

    fn eat_codes(&mut self) -> Result<SmallVec<[TextRange; 2]>, ParseError> {
        self.eat_whitespace();
        if !self.cursor.eat_char('[') {
            return self.error(ParseErrorKind::MissingCodes);
        }

        let mut codes: SmallVec<[TextRange; 2]> = smallvec![];

        loop {
            if self.cursor.is_eof() {
                return self.error(ParseErrorKind::MissingBracket);
            }

            self.eat_whitespace();

            if self.cursor.eat_char(']') {
                break Ok(codes);
            }

            let code_start = self.offset();
            if !self.eat_word() {
                return self.error(ParseErrorKind::InvalidCode);
            }

            codes.push(TextRange::new(code_start, self.offset()));

            self.eat_whitespace();
            if !self.cursor.eat_char(',') {
                if self.cursor.eat_char(']') {
                    break Ok(codes);
                }

                return if self.cursor.is_eof() {
                    self.error(ParseErrorKind::MissingBracket)
                } else {
                    self.error(ParseErrorKind::MissingComma)
                };
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

    fn offset(&self) -> TextSize {
        self.range.start() + self.range.len() - self.cursor.text_len()
    }

    fn error<T>(&self, kind: ParseErrorKind) -> Result<T, ParseError> {
        Err(ParseError::new(
            kind,
            TextRange::new(self.offset(), self.range.end()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use ruff_text_size::{TextRange, TextSize};
    use similar::DiffableStr;

    use crate::suppression::{
        ParseError, SuppressionAction, SuppressionComment, SuppressionParser,
    };

    fn parse_suppression_comment(source: &str) -> Result<SuppressionComment, ParseError> {
        let mut parser = SuppressionParser::new(
            source,
            &TextRange::new(0.into(), TextSize::try_from(source.len()).unwrap()),
        );
        parser.parse_comment()
    }

    #[test]
    fn unrelated_comment() {
        assert_debug_snapshot!(
            parse_suppression_comment("# hello world"),
            @r"
        Err(
            ParseError {
                kind: NotASuppression,
                range: 2..13,
            },
        )
        ",
        );
    }

    #[test]
    fn invalid_action() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: lol[hi]"),
            @r"
        Err(
            ParseError {
                kind: UnknownDirective,
                range: 8..15,
            },
        )
        ",
        );
    }

    #[test]
    fn missing_codes() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: ignore"),
            @r"
        Err(
            ParseError {
                kind: MissingCodes,
                range: 14..14,
            },
        )
        ",
        );
    }

    #[test]
    fn empty_codes() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: ignore[]"),
            @r"
        Err(
            ParseError {
                kind: MissingCodes,
                range: 0..16,
            },
        )
        ",
        );
    }

    #[test]
    fn missing_bracket() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: ignore[foo"),
            @r"
        Err(
            ParseError {
                kind: MissingBracket,
                range: 18..18,
            },
        )
        ",
        );
    }

    #[test]
    fn missing_comma() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: ignore[foo bar]"),
            @r"
        Err(
            ParseError {
                kind: MissingComma,
                range: 19..23,
            },
        )
        ",
        );
    }

    #[test]
    fn ignore_single_code() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: ignore[foo]"),
            @r"
        Ok(
            SuppressionComment {
                range: 0..19,
                action: Ignore,
                codes: [
                    15..18,
                ],
                reason: 19..19,
            },
        )
        ",
        );
    }

    #[test]
    fn ignore_single_code_with_reason() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: ignore[foo] I like bar better"),
            @r"
        Ok(
            SuppressionComment {
                range: 0..37,
                action: Ignore,
                codes: [
                    15..18,
                ],
                reason: 20..37,
            },
        )
        ",
        );
    }

    #[test]
    fn ignore_multiple_codes() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: ignore[foo, bar]"),
            @r"
        Ok(
            SuppressionComment {
                range: 0..24,
                action: Ignore,
                codes: [
                    15..18,
                    20..23,
                ],
                reason: 24..24,
            },
        )
        ",
        );
    }

    #[test]
    fn disable_single_code() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[some-thing]"),
            @r"
        Ok(
            SuppressionComment {
                range: 0..27,
                action: Disable,
                codes: [
                    16..26,
                ],
                reason: 27..27,
            },
        )
        ",
        );
    }

    #[test]
    fn comment_attributes() {
        let source = "# ruff: disable[foo, bar] hello world";
        let mut parser = SuppressionParser::new(
            source,
            &TextRange::new(0.into(), TextSize::try_from(source.len()).unwrap()),
        );
        let comment = parser.parse_comment().unwrap();
        assert_eq!(comment.action, SuppressionAction::Disable);
        assert_eq!(
            comment
                .codes
                .into_iter()
                .map(|range| { source.slice(range.into()) })
                .collect::<Vec<_>>(),
            ["foo", "bar"]
        );
        assert_eq!(source.slice(comment.reason.into()), "hello world");
    }
}
