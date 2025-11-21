use core::fmt;
use ruff_python_parser::{TokenKind, Tokens};
use ruff_source_file::LineRanges;
use std::{error::Error, fmt::Formatter};
use thiserror::Error;

use ruff_python_trivia::{Cursor, is_python_whitespace};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize, TextSlice};
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

    /// How indented an own-line comment is, or None for trailing comments
    indent: Option<String>,

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
            .map(|range| source.slice(range).to_string())
            .collect()
    }

    /// Whether the comment "matches" another comment, based on indentation and suppressed codes
    fn matches(&self, other: &SuppressionComment, source: &str) -> bool {
        ((self.action == SuppressionAction::Enable && other.action == SuppressionAction::Disable)
            || (self.action == SuppressionAction::Disable
                && other.action == SuppressionAction::Enable))
            && self.indent == other.indent
            && self.codes_as_str(source) == other.codes_as_str(source)
    }
}

#[allow(unused)]
#[derive(Clone, Debug)]
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
    /// Valid suppression ranges with associated comments
    valid: Vec<Suppression>,

    /// Invalid suppression comments
    invalid: Vec<SuppressionComment>,

    /// Parse errors from suppression comments
    errors: Vec<ParseError>,
}

impl Suppressions {
    pub(crate) fn from_tokens(source: &str, tokens: &Tokens) -> Suppressions {
        let mut builder = SuppressionsBuilder::new(source);
        builder.load_from_tokens(tokens)
    }
}

#[derive(Default)]
pub(crate) struct SuppressionsBuilder<'a> {
    source: &'a str,

    valid: Vec<Suppression>,
    invalid: Vec<SuppressionComment>,
    errors: Vec<ParseError>,

    pending: Vec<SuppressionComment>,
}

impl<'a> SuppressionsBuilder<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            ..Default::default()
        }
    }

    fn match_comments(&mut self, current_indent: &String) {
        let mut comment_index = 0;
        while comment_index < self.pending.len() {
            let comment = &self.pending[comment_index];

            if comment
                .indent
                .as_ref()
                .is_some_and(|indent| indent.text_len() < current_indent.text_len())
            {
                comment_index += 1;
                continue;
            }

            // find the first matching comment
            if let Some(other_index) = self.pending[comment_index + 1..]
                .iter()
                .position(|other| comment.matches(other, self.source))
            {
                // offset from current candidate
                let other_index = comment_index + 1 + other_index;
                let other = &self.pending[other_index];

                let combined_range = TextRange::new(comment.range.start(), other.range.end());

                for code in comment.codes_as_str(self.source) {
                    self.valid.push(Suppression {
                        code,
                        range: combined_range,
                        comments: smallvec![comment.clone(), other.clone()],
                    });
                }

                // remove both comments from further consideration
                self.pending.remove(other_index);
                self.pending.remove(comment_index);
            } else {
                self.invalid
                    .push(self.pending.remove(comment_index).clone());
            }
        }
    }

    pub(crate) fn load_from_tokens(&mut self, tokens: &Tokens) -> Suppressions {
        let default_indent = String::new();
        let mut current_indent: &String = &default_indent;
        let mut indents: Vec<String> = vec![];

        for (token_index, token) in tokens.iter().enumerate() {
            match token.kind() {
                TokenKind::Indent => {
                    indents.push(self.source.slice(token).to_string());
                    current_indent = indents.last().unwrap_or(&default_indent);
                }
                TokenKind::Dedent => {
                    self.match_comments(current_indent);

                    indents.pop();
                    current_indent = indents.last().unwrap_or(&default_indent);
                }
                TokenKind::Comment => {
                    let mut parser = SuppressionParser::new(self.source, token.range());
                    match parser.parse_comment() {
                        Ok(comment) => {
                            let Some(indent) = &comment.indent else {
                                // trailing suppressions are not supported
                                self.invalid.push(comment);
                                continue;
                            };

                            // comment matches current block's indentation, or precedes a dedent
                            if indent == current_indent
                                || tokens[token_index..]
                                    .iter()
                                    .find(|t| !t.kind().is_trivia())
                                    .is_some_and(|t| {
                                        t.kind() == TokenKind::Dedent
                                            || t.kind() == TokenKind::Indent
                                    })
                            {
                                self.pending.push(comment);
                            } else {
                                // weirdly indented? ¯\_(ツ)_/¯
                                self.invalid.push(comment);
                            }
                        }
                        Err(ParseError {
                            kind: ParseErrorKind::NotASuppression,
                            ..
                        }) => {}
                        Err(error) => {
                            self.errors.push(error);
                        }
                    }
                }
                _ => {}
            }
        }

        self.match_comments(&default_indent);

        Suppressions {
            valid: self.valid.clone(),
            invalid: self.invalid.clone(),
            errors: self.errors.clone(),
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
    source: &'src str,
    range: TextRange,
}

impl<'src> SuppressionParser<'src> {
    fn new(source: &'src str, range: TextRange) -> Self {
        let cursor = Cursor::new(&source[range]);
        Self {
            cursor,
            source,
            range,
        }
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
        let text_before = self.source.slice(TextRange::new(
            self.source.line_start(self.range.start()),
            self.range.start(),
        ));
        let is_own_line = text_before.chars().all(is_python_whitespace);
        let indent = is_own_line.then(|| text_before.to_string());

        Ok(SuppressionComment {
            range: self.range,
            indent,
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
    use ruff_python_parser::{Mode, ParseOptions, parse};
    use ruff_text_size::{TextRange, TextSize};
    use similar::DiffableStr;

    use crate::suppression::{
        ParseError, SuppressionAction, SuppressionComment, SuppressionParser, Suppressions,
    };

    #[test]
    fn suppressions_from_tokens() {
        let source = "
# comment here
print('hello')

# ruff: disable[alpha]
def foo():
    # ruff: disable[beta]
    if True:
        # ruff: disable[gamma]  # unmatched!
        pass
    # ruff: enable[beta]
# ruff: enable[alpha]

# ruff: disable  # parse error!
def bar():
    pass
";
        let parsed = parse(source, ParseOptions::from(Mode::Module)).unwrap();
        let suppressions = Suppressions::from_tokens(source, parsed.tokens());
        assert_debug_snapshot!(
            suppressions,
            @r#"
        Suppressions {
            valid: [
                Suppression {
                    code: "beta",
                    range: 70..187,
                    comments: [
                        SuppressionComment {
                            range: 70..91,
                            indent: Some(
                                "    ",
                            ),
                            action: Disable,
                            codes: [
                                86..90,
                            ],
                            reason: 91..91,
                        },
                        SuppressionComment {
                            range: 167..187,
                            indent: Some(
                                "    ",
                            ),
                            action: Enable,
                            codes: [
                                182..186,
                            ],
                            reason: 187..187,
                        },
                    ],
                },
                Suppression {
                    code: "alpha",
                    range: 32..209,
                    comments: [
                        SuppressionComment {
                            range: 32..54,
                            indent: Some(
                                "",
                            ),
                            action: Disable,
                            codes: [
                                48..53,
                            ],
                            reason: 54..54,
                        },
                        SuppressionComment {
                            range: 188..209,
                            indent: Some(
                                "",
                            ),
                            action: Enable,
                            codes: [
                                203..208,
                            ],
                            reason: 209..209,
                        },
                    ],
                },
            ],
            invalid: [
                SuppressionComment {
                    range: 113..149,
                    indent: Some(
                        "        ",
                    ),
                    action: Disable,
                    codes: [
                        129..134,
                    ],
                    reason: 137..149,
                },
            ],
            errors: [
                ParseError {
                    kind: MissingCodes,
                    range: 228..242,
                },
            ],
        }
        "#,
        );
    }

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
            @r#"
        Ok(
            SuppressionComment {
                range: 0..20,
                indent: Some(
                    "",
                ),
                action: Disable,
                codes: [
                    16..19,
                ],
                reason: 20..20,
            },
        )
        "#,
        );
    }

    #[test]
    fn disable_single_code_with_reason() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo] I like bar better"),
            @r#"
        Ok(
            SuppressionComment {
                range: 0..38,
                indent: Some(
                    "",
                ),
                action: Disable,
                codes: [
                    16..19,
                ],
                reason: 21..38,
            },
        )
        "#,
        );
    }

    #[test]
    fn disable_multiple_codes() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo, bar]"),
            @r#"
        Ok(
            SuppressionComment {
                range: 0..25,
                indent: Some(
                    "",
                ),
                action: Disable,
                codes: [
                    16..19,
                    21..24,
                ],
                reason: 25..25,
            },
        )
        "#,
        );
    }

    #[test]
    fn enable_single_code() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: enable[some-thing]"),
            @r#"
        Ok(
            SuppressionComment {
                range: 0..26,
                indent: Some(
                    "",
                ),
                action: Enable,
                codes: [
                    15..25,
                ],
                reason: 26..26,
            },
        )
        "#,
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
                indent: None,
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
            comment.unwrap().indent,
            @"None",
        );
    }

    #[test]
    fn indented_comment() {
        let source = "    # ruff: enable[some-thing]";
        let comment = parse_suppression_comment(source);
        assert_debug_snapshot!(
            comment,
            @r#"
        Ok(
            SuppressionComment {
                range: 4..30,
                indent: Some(
                    "    ",
                ),
                action: Enable,
                codes: [
                    19..29,
                ],
                reason: 30..30,
            },
        )
        "#,
        );
        assert_debug_snapshot!(
            comment.unwrap().indent,
            @r#"
        Some(
            "    ",
        )
        "#,
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
