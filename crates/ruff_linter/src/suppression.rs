use core::fmt;
use ruff_source_file::LineRanges;
use std::{error::Error, fmt::Formatter};
use thiserror::Error;

use ruff_python_trivia::{CommentRanges, Cursor, is_python_whitespace};
use ruff_text_size::{TextRange, TextSize};
use smallvec::{SmallVec, smallvec};

#[derive(Clone, Debug, Eq, PartialEq)]
enum SuppressionAction {
    Disable,
    Enable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

impl SuppressionComment {
    /// Return the suppressed codes as strings
    fn codes_as_str(&self, source: &str) -> Vec<String> {
        self.codes
            .iter()
            .map(|range| source[*range].to_string())
            .collect()
    }

    /// Whether the comment is an own-line comment, and how indented it is
    fn own_line_indent(&self, source: &str) -> Option<usize> {
        let before =
            &source[TextRange::new(source.line_start(self.range.start()), self.range.start())];
        let is_own_line = before.chars().all(is_python_whitespace);
        is_own_line.then(|| before.chars().count())
    }
}

#[allow(unused)]
#[derive(Debug)]
pub(crate) struct Suppression {
    /// The lint code being suppressed
    code: String,

    /// Range for which the suppression applies
    range: TextRange,

    /// Any comments associated with the suppression
    comments: SmallVec<[SuppressionComment; 2]>,
}

#[allow(unused)]
#[derive(Debug)]
pub(crate) struct Suppressions {
    valid: Vec<Suppression>,
    invalid: Vec<SuppressionComment>,
    errors: Vec<ParseError>,
}

impl Suppressions {
    pub(crate) fn load(source: &str, comment_ranges: &CommentRanges) -> Self {
        let mut suppression_comments = comment_ranges
            .iter()
            .map(|comment_range| {
                let mut parser = SuppressionParser::new(source, *comment_range);
                parser.parse_comment()
            })
            .collect::<Vec<_>>();

        let mut index = 0;
        let mut suppressions: Vec<Suppression> = Vec::new();
        let mut invalid: Vec<SuppressionComment> = Vec::new();
        let mut errors: Vec<ParseError> = Vec::new();

        // Process all parsed comments in order generating appropriate suppression ranges
        while index < suppression_comments.len() {
            let mut remove_index: Option<usize> = None;
            match &suppression_comments[index] {
                Ok(comment) => {
                    match comment.action {
                        SuppressionAction::Enable => {
                            let Some(_indent) = comment.own_line_indent(source) else {
                                invalid.push(comment.clone());
                                continue;
                            };

                            invalid.push(comment.clone());
                        }
                        SuppressionAction::Disable => {
                            let Some(indent) = comment.own_line_indent(source) else {
                                invalid.push(comment.clone());
                                continue;
                            };

                            // Look for matching "enable" comments.
                            // Match by indentation and suppressed codes.
                            // TODO: search only within the same scope
                            let codes = comment.codes_as_str(source);
                            if let Some(other_index) =
                                suppression_comments[index + 1..].iter().position(|k| {
                                    k.as_ref().is_ok_and(|other| {
                                        other.action == SuppressionAction::Enable
                                            && other.own_line_indent(source) == Some(indent)
                                            && other.codes_as_str(source) == codes
                                    })
                                })
                            {
                                // Offset from current position
                                let other_index = index + 1 + other_index;

                                // Create a suppression range spanning from the starting disable
                                // comment to the ending enable comment.
                                let other = suppression_comments[other_index].as_ref().unwrap();
                                let combined_range =
                                    TextRange::new(comment.range.start(), other.range.end());
                                for code in codes {
                                    suppressions.push(Suppression {
                                        code,
                                        range: combined_range,
                                        comments: smallvec![comment.clone(), other.clone()],
                                    });
                                }
                                // Mark the matched enable comment to be removed from the vector
                                // so that it doesn't get processed and treated as unmatched.
                                remove_index = Some(other_index);
                            } else {
                                invalid.push(comment.clone());
                            }
                        }
                    }
                }
                Err(error) => {
                    if error.kind != ParseErrorKind::NotASuppression {
                        errors.push(error.clone());
                    }
                }
            }
            // Remove a marked comment from the vector.
            if let Some(remove_index) = remove_index {
                suppression_comments.remove(remove_index).ok();
            }
            index += 1;
        }

        suppressions.shrink_to_fit();
        Self {
            valid: suppressions,
            invalid,
            errors,
        }
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
    fn new(source: &'src str, range: TextRange) -> Self {
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
    use ruff_python_trivia::CommentRanges;
    use ruff_text_size::{TextRange, TextSize};
    use similar::DiffableStr;

    use crate::suppression::{
        ParseError, SuppressionAction, SuppressionComment, SuppressionParser, Suppressions,
    };

    fn parse_suppression_comment(source: &str) -> Result<SuppressionComment, ParseError> {
        let offset = TextSize::new(source.find('#').unwrap_or(0).try_into().unwrap());
        let mut parser = SuppressionParser::new(
            source,
            TextRange::new(offset, TextSize::try_from(source.len()).unwrap()),
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
            parse_suppression_comment("# ruff: disable"),
            @r"
        Err(
            ParseError {
                kind: MissingCodes,
                range: 15..15,
            },
        )
        ",
        );
    }

    #[test]
    fn empty_codes() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[]"),
            @r"
        Err(
            ParseError {
                kind: MissingCodes,
                range: 0..17,
            },
        )
        ",
        );
    }

    #[test]
    fn missing_bracket() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo"),
            @r"
        Err(
            ParseError {
                kind: MissingBracket,
                range: 19..19,
            },
        )
        ",
        );
    }

    #[test]
    fn missing_comma() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo bar]"),
            @r"
        Err(
            ParseError {
                kind: MissingComma,
                range: 20..24,
            },
        )
        ",
        );
    }

    #[test]
    fn disable_single_code() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo]"),
            @r"
        Ok(
            SuppressionComment {
                range: 0..20,
                action: Disable,
                codes: [
                    16..19,
                ],
                reason: 20..20,
            },
        )
        ",
        );
    }

    #[test]
    fn disable_single_code_with_reason() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo] I like bar better"),
            @r"
        Ok(
            SuppressionComment {
                range: 0..38,
                action: Disable,
                codes: [
                    16..19,
                ],
                reason: 21..38,
            },
        )
        ",
        );
    }

    #[test]
    fn disable_multiple_codes() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo, bar]"),
            @r"
        Ok(
            SuppressionComment {
                range: 0..25,
                action: Disable,
                codes: [
                    16..19,
                    21..24,
                ],
                reason: 25..25,
            },
        )
        ",
        );
    }

    #[test]
    fn enable_single_code() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: enable[some-thing]"),
            @r"
        Ok(
            SuppressionComment {
                range: 0..26,
                action: Enable,
                codes: [
                    15..25,
                ],
                reason: 26..26,
            },
        )
        ",
        );
    }

    #[test]
    fn trailing_comment() {
        let source = "print('hello world')  # ruff: enable[some-thing]";
        let comment = parse_suppression_comment(source);
        assert_debug_snapshot!(
            comment,
            @r"
        Ok(
            SuppressionComment {
                range: 22..48,
                action: Enable,
                codes: [
                    37..47,
                ],
                reason: 48..48,
            },
        )
        ",
        );
        assert_debug_snapshot!(
            comment.unwrap().own_line_indent(source),
            @"None",
        );
    }

    #[test]
    fn indented_comment() {
        let source = "    # ruff: enable[some-thing]";
        let comment = parse_suppression_comment(source);
        assert_debug_snapshot!(
            comment,
            @r"
        Ok(
            SuppressionComment {
                range: 4..30,
                action: Enable,
                codes: [
                    19..29,
                ],
                reason: 30..30,
            },
        )
        ",
        );
        assert_debug_snapshot!(
            comment.unwrap().own_line_indent(source),
            @r"
        Some(
            4,
        )
        ",
        );
    }

    #[test]
    fn load_no_comments() {
        let source = "print('hello world')";
        let suppressions = Suppressions::load(source, &CommentRanges::new(vec![]));
        assert_debug_snapshot!(
            suppressions,
            @r"
        Suppressions {
            valid: [],
            invalid: [],
            errors: [],
        }
        ",
        );
    }

    #[test]
    fn load_matched_range() {
        let source = "
# ruff: disable[foo]
print('hello world')
# ruff: enable[foo]
";
        let ranges = vec![
            TextRange::at(1.into(), 20.into()),
            TextRange::at(43.into(), 19.into()),
        ];
        let suppressions = Suppressions::load(source, &CommentRanges::new(ranges));
        assert_debug_snapshot!(
            suppressions,
            @r#"
        Suppressions {
            valid: [
                Suppression {
                    code: "foo",
                    range: 1..62,
                    comments: [
                        SuppressionComment {
                            range: 1..21,
                            action: Disable,
                            codes: [
                                17..20,
                            ],
                            reason: 21..21,
                        },
                        SuppressionComment {
                            range: 43..62,
                            action: Enable,
                            codes: [
                                58..61,
                            ],
                            reason: 62..62,
                        },
                    ],
                },
            ],
            invalid: [],
            errors: [],
        }
        "#,
        );
    }

    #[test]
    fn load_unmatched_range() {
        let source = "
# ruff: disable[foo]
print('hello world')
# unrelated comment
";
        let ranges = vec![
            TextRange::at(1.into(), 20.into()),
            TextRange::at(43.into(), 19.into()),
        ];
        let suppressions = Suppressions::load(source, &CommentRanges::new(ranges));
        assert_debug_snapshot!(
            suppressions,
            @r"
        Suppressions {
            valid: [],
            invalid: [
                SuppressionComment {
                    range: 1..21,
                    action: Disable,
                    codes: [
                        17..20,
                    ],
                    reason: 21..21,
                },
            ],
            errors: [],
        }
        ",
        );
    }

    #[test]
    fn comment_attributes() {
        let source = "# ruff: disable[foo, bar] hello world";
        let mut parser = SuppressionParser::new(
            source,
            TextRange::new(0.into(), TextSize::try_from(source.len()).unwrap()),
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
