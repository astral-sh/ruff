use compact_str::CompactString;
use core::fmt;
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_ast::whitespace::indentation;
use std::{error::Error, fmt::Formatter};
use thiserror::Error;

use ruff_python_trivia::Cursor;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize, TextSlice};
use smallvec::{SmallVec, smallvec};

#[allow(unused)]
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

#[allow(unused)]
impl SuppressionComment {
    /// Return the suppressed codes as strings
    fn codes_as_str<'src>(&self, source: &'src str) -> impl Iterator<Item = &'src str> {
        self.codes.iter().map(|range| source.slice(range))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingSuppressionComment<'a> {
    /// How indented an own-line comment is, or None for trailing comments
    indent: &'a str,

    /// The suppression comment
    comment: SuppressionComment,
}

#[allow(unused)]
impl PendingSuppressionComment<'_> {
    /// Whether the comment "matches" another comment, based on indentation and suppressed codes
    /// Expects a "forward search" for matches, ie, will only match if the current comment is a
    /// "disable" comment and other is the matching "enable" comment.
    fn matches(&self, other: &PendingSuppressionComment, source: &str) -> bool {
        self.comment.action == SuppressionAction::Disable
            && other.comment.action == SuppressionAction::Enable
            && self.indent == other.indent
            && self
                .comment
                .codes_as_str(source)
                .eq(other.comment.codes_as_str(source))
    }
}

#[allow(unused)]
#[derive(Clone, Debug)]
pub(crate) struct Suppression {
    /// The lint code being suppressed
    code: CompactString,

    /// Range for which the suppression applies
    range: TextRange,

    /// Any comments associated with the suppression
    comments: SmallVec<[SuppressionComment; 2]>,
}

#[allow(unused)]
#[derive(Copy, Clone, Debug)]
pub(crate) enum InvalidSuppressionKind {
    /// Trailing suppression not supported
    Trailing,

    /// No matching enable or disable suppression found
    Unmatched,

    /// Suppression does not match surrounding indentation
    Indentation,
}

#[allow(unused)]
#[derive(Clone, Debug)]
pub(crate) struct InvalidSuppression {
    kind: InvalidSuppressionKind,
    comment: SuppressionComment,
}

#[allow(unused)]
#[derive(Debug)]
pub(crate) struct Suppressions {
    /// Valid suppression ranges with associated comments
    valid: Vec<Suppression>,

    /// Invalid suppression comments
    invalid: Vec<InvalidSuppression>,

    /// Parse errors from suppression comments
    errors: Vec<ParseError>,
}

#[allow(unused)]
impl Suppressions {
    pub(crate) fn from_tokens(source: &str, tokens: &Tokens) -> Suppressions {
        let builder = SuppressionsBuilder::new(source);
        builder.load_from_tokens(tokens)
    }
}

#[derive(Default)]
pub(crate) struct SuppressionsBuilder<'a> {
    source: &'a str,

    valid: Vec<Suppression>,
    invalid: Vec<InvalidSuppression>,
    errors: Vec<ParseError>,

    pending: Vec<PendingSuppressionComment<'a>>,
}

impl<'a> SuppressionsBuilder<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            ..Default::default()
        }
    }

    pub(crate) fn load_from_tokens(mut self, tokens: &Tokens) -> Suppressions {
        let default_indent = "";
        let mut indents: Vec<&str> = vec![];

        // Iterate through tokens, tracking indentation, filtering trailing comments, and then
        // looking for matching comments from the previous block when reaching a dedent token.
        for (token_index, token) in tokens.iter().enumerate() {
            match token.kind() {
                TokenKind::Indent => {
                    indents.push(self.source.slice(token));
                }
                TokenKind::Dedent => {
                    self.match_comments(indents.last().copied().unwrap_or_default(), token.range());
                    indents.pop();
                }
                TokenKind::Comment => {
                    let mut parser = SuppressionParser::new(self.source, token.range());
                    match parser.parse_comment() {
                        Ok(comment) => {
                            let indent = indentation(self.source, &comment.range);

                            let Some(indent) = indent else {
                                // trailing suppressions are not supported
                                self.invalid.push(InvalidSuppression {
                                    kind: InvalidSuppressionKind::Trailing,
                                    comment,
                                });
                                continue;
                            };

                            // comment matches current block's indentation, or precedes an indent/dedent token
                            if indent == indents.last().copied().unwrap_or_default()
                                || tokens[token_index..]
                                    .iter()
                                    .find(|t| !t.kind().is_trivia())
                                    .is_some_and(|t| {
                                        matches!(t.kind(), TokenKind::Dedent | TokenKind::Indent)
                                    })
                            {
                                self.pending
                                    .push(PendingSuppressionComment { indent, comment });
                            } else {
                                // weirdly indented? ¯\_(ツ)_/¯
                                self.invalid.push(InvalidSuppression {
                                    kind: InvalidSuppressionKind::Indentation,
                                    comment,
                                });
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

        self.match_comments(default_indent, TextRange::up_to(self.source.text_len()));

        Suppressions {
            valid: self.valid,
            invalid: self.invalid,
            errors: self.errors,
        }
    }

    fn match_comments(&mut self, current_indent: &str, dedent_range: TextRange) {
        let mut comment_index = 0;

        // for each pending comment, search for matching comments at the same indentation level,
        // generate range suppressions for any matches, and then discard any unmatched comments
        // from the outgoing indentation block
        while comment_index < self.pending.len() {
            let comment = &self.pending[comment_index];

            // skip comments from an outer indentation level
            if comment.indent.text_len() < current_indent.text_len() {
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

                // record a combined range suppression from the matching comments
                let combined_range =
                    TextRange::new(comment.comment.range.start(), other.comment.range.end());
                for code in comment.comment.codes_as_str(self.source) {
                    self.valid.push(Suppression {
                        code: code.into(),
                        range: combined_range,
                        comments: smallvec![comment.comment.clone(), other.comment.clone()],
                    });
                }

                // remove both comments from further consideration
                self.pending.remove(other_index);
                self.pending.remove(comment_index);
            } else if matches!(comment.comment.action, SuppressionAction::Disable) {
                // treat "disable" comments without a matching "enable" as *implicitly* matched
                // to the end of the current indentation level
                let implicit_range =
                    TextRange::new(comment.comment.range.start(), dedent_range.end());
                for code in comment.comment.codes_as_str(self.source) {
                    self.valid.push(Suppression {
                        code: code.into(),
                        range: implicit_range,
                        comments: smallvec![comment.comment.clone()],
                    });
                }
                self.pending.remove(comment_index);
            } else {
                self.invalid.push(InvalidSuppression {
                    kind: InvalidSuppressionKind::Unmatched,
                    comment: self.pending.remove(comment_index).comment.clone(),
                });
            }
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, Error, PartialEq)]
enum ParseErrorKind {
    #[error("not a suppression comment")]
    NotASuppression,

    #[error("comment doesn't start with `#`")]
    CommentWithoutHash,

    #[error("unknown ruff directive")]
    UnknownAction,

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
        } else if self.cursor.as_str().starts_with("noqa") {
            // file-level "noqa" variant, ignore for now
            self.error(ParseErrorKind::NotASuppression)
        } else {
            self.error(ParseErrorKind::UnknownAction)
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
        Err(ParseError::new(kind, self.range))
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::{self, Formatter};

    use insta::assert_debug_snapshot;
    use itertools::Itertools;
    use ruff_python_parser::{Mode, ParseOptions, parse};
    use ruff_text_size::{TextRange, TextSize};
    use similar::DiffableStr;

    use crate::suppression::{
        InvalidSuppression, ParseError, Suppression, SuppressionAction, SuppressionComment,
        SuppressionParser, Suppressions,
    };

    #[test]
    fn no_suppression() {
        let source = "
# this is a comment
print('hello')
";
        assert_debug_snapshot!(
            Suppressions::debug(source),
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
    fn file_level_suppression() {
        let source = "
# ruff: noqa F401
print('hello')
";
        assert_debug_snapshot!(
            Suppressions::debug(source),
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
    fn single_range_suppression() {
        let source = "
# ruff: disable[foo]
print('hello')
# ruff: enable[foo]
";
        assert_debug_snapshot!(
            Suppressions::debug(source),
            @r##"
        Suppressions {
            valid: [
                Suppression {
                    covered_source: "# ruff: disable[foo]\nprint('hello')\n# ruff: enable[foo]",
                    code: "foo",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[foo]",
                            action: Disable,
                            codes: [
                                "foo",
                            ],
                            reason: "",
                        },
                        SuppressionComment {
                            text: "# ruff: enable[foo]",
                            action: Enable,
                            codes: [
                                "foo",
                            ],
                            reason: "",
                        },
                    ],
                },
            ],
            invalid: [],
            errors: [],
        }
        "##,
        );
    }

    #[test]
    fn single_range_suppression_implicit_match() {
        let source = "
# ruff: disable[foo]
print('hello')

def foo():
    # ruff: disable[bar]
    print('hello')

";
        assert_debug_snapshot!(
            Suppressions::debug(source),
            @r##"
        Suppressions {
            valid: [
                Suppression {
                    covered_source: "# ruff: disable[bar]\n    print('hello')\n\n",
                    code: "bar",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[bar]",
                            action: Disable,
                            codes: [
                                "bar",
                            ],
                            reason: "",
                        },
                    ],
                },
                Suppression {
                    covered_source: "# ruff: disable[foo]\nprint('hello')\n\ndef foo():\n    # ruff: disable[bar]\n    print('hello')\n\n",
                    code: "foo",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[foo]",
                            action: Disable,
                            codes: [
                                "foo",
                            ],
                            reason: "",
                        },
                    ],
                },
            ],
            invalid: [],
            errors: [],
        }
        "##,
        );
    }

    #[test]
    fn nested_range_suppressions() {
        let source = "
class Foo:
    # ruff: disable[foo]
    def bar(self):
        # ruff: disable[bar]
        print('hello')
        # ruff: enable[bar]
    # ruff: enable[foo]
";
        assert_debug_snapshot!(
            Suppressions::debug(source),
            @r##"
        Suppressions {
            valid: [
                Suppression {
                    covered_source: "# ruff: disable[bar]\n        print('hello')\n        # ruff: enable[bar]",
                    code: "bar",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[bar]",
                            action: Disable,
                            codes: [
                                "bar",
                            ],
                            reason: "",
                        },
                        SuppressionComment {
                            text: "# ruff: enable[bar]",
                            action: Enable,
                            codes: [
                                "bar",
                            ],
                            reason: "",
                        },
                    ],
                },
                Suppression {
                    covered_source: "# ruff: disable[foo]\n    def bar(self):\n        # ruff: disable[bar]\n        print('hello')\n        # ruff: enable[bar]\n    # ruff: enable[foo]",
                    code: "foo",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[foo]",
                            action: Disable,
                            codes: [
                                "foo",
                            ],
                            reason: "",
                        },
                        SuppressionComment {
                            text: "# ruff: enable[foo]",
                            action: Enable,
                            codes: [
                                "foo",
                            ],
                            reason: "",
                        },
                    ],
                },
            ],
            invalid: [],
            errors: [],
        }
        "##,
        );
    }

    #[test]
    fn interleaved_range_suppressions() {
        let source = "
def foo():
    # ruff: disable[foo]
    print('hello')
    # ruff: disable[bar]
    print('hello')
    # ruff: enable[foo]
    print('hello')
    # ruff: enable[bar]
";
        assert_debug_snapshot!(
            Suppressions::debug(source),
            @r##"
        Suppressions {
            valid: [
                Suppression {
                    covered_source: "# ruff: disable[foo]\n    print('hello')\n    # ruff: disable[bar]\n    print('hello')\n    # ruff: enable[foo]",
                    code: "foo",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[foo]",
                            action: Disable,
                            codes: [
                                "foo",
                            ],
                            reason: "",
                        },
                        SuppressionComment {
                            text: "# ruff: enable[foo]",
                            action: Enable,
                            codes: [
                                "foo",
                            ],
                            reason: "",
                        },
                    ],
                },
                Suppression {
                    covered_source: "# ruff: disable[bar]\n    print('hello')\n    # ruff: enable[foo]\n    print('hello')\n    # ruff: enable[bar]",
                    code: "bar",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[bar]",
                            action: Disable,
                            codes: [
                                "bar",
                            ],
                            reason: "",
                        },
                        SuppressionComment {
                            text: "# ruff: enable[bar]",
                            action: Enable,
                            codes: [
                                "bar",
                            ],
                            reason: "",
                        },
                    ],
                },
            ],
            invalid: [],
            errors: [],
        }
        "##,
        );
    }

    #[test]
    fn range_suppression_two_codes() {
        let source = "
# ruff: disable[foo, bar]
print('hello')
# ruff: enable[foo, bar]
";
        assert_debug_snapshot!(
            Suppressions::debug(source),
            @r##"
        Suppressions {
            valid: [
                Suppression {
                    covered_source: "# ruff: disable[foo, bar]\nprint('hello')\n# ruff: enable[foo, bar]",
                    code: "foo",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[foo, bar]",
                            action: Disable,
                            codes: [
                                "foo",
                                "bar",
                            ],
                            reason: "",
                        },
                        SuppressionComment {
                            text: "# ruff: enable[foo, bar]",
                            action: Enable,
                            codes: [
                                "foo",
                                "bar",
                            ],
                            reason: "",
                        },
                    ],
                },
                Suppression {
                    covered_source: "# ruff: disable[foo, bar]\nprint('hello')\n# ruff: enable[foo, bar]",
                    code: "bar",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[foo, bar]",
                            action: Disable,
                            codes: [
                                "foo",
                                "bar",
                            ],
                            reason: "",
                        },
                        SuppressionComment {
                            text: "# ruff: enable[foo, bar]",
                            action: Enable,
                            codes: [
                                "foo",
                                "bar",
                            ],
                            reason: "",
                        },
                    ],
                },
            ],
            invalid: [],
            errors: [],
        }
        "##,
        );
    }

    #[test]
    fn range_suppression_unmatched() {
        let source = "
# ruff: disable[foo]
print('hello')
# ruff: enable[bar]
print('world')
";
        assert_debug_snapshot!(
            Suppressions::debug(source),
            @r##"
        Suppressions {
            valid: [
                Suppression {
                    covered_source: "# ruff: disable[foo]\nprint('hello')\n# ruff: enable[bar]\nprint('world')\n",
                    code: "foo",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[foo]",
                            action: Disable,
                            codes: [
                                "foo",
                            ],
                            reason: "",
                        },
                    ],
                },
            ],
            invalid: [
                InvalidSuppression {
                    kind: Unmatched,
                    comment: SuppressionComment {
                        text: "# ruff: enable[bar]",
                        action: Enable,
                        codes: [
                            "bar",
                        ],
                        reason: "",
                    },
                },
            ],
            errors: [],
        }
        "##,
        );
    }

    #[test]
    fn range_suppression_unordered() {
        let source = "
# ruff: disable[foo, bar]
print('hello')
# ruff: enable[bar, foo]
";
        assert_debug_snapshot!(
            Suppressions::debug(source),
            @r##"
        Suppressions {
            valid: [
                Suppression {
                    covered_source: "# ruff: disable[foo, bar]\nprint('hello')\n# ruff: enable[bar, foo]\n",
                    code: "foo",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[foo, bar]",
                            action: Disable,
                            codes: [
                                "foo",
                                "bar",
                            ],
                            reason: "",
                        },
                    ],
                },
                Suppression {
                    covered_source: "# ruff: disable[foo, bar]\nprint('hello')\n# ruff: enable[bar, foo]\n",
                    code: "bar",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[foo, bar]",
                            action: Disable,
                            codes: [
                                "foo",
                                "bar",
                            ],
                            reason: "",
                        },
                    ],
                },
            ],
            invalid: [
                InvalidSuppression {
                    kind: Unmatched,
                    comment: SuppressionComment {
                        text: "# ruff: enable[bar, foo]",
                        action: Enable,
                        codes: [
                            "bar",
                            "foo",
                        ],
                        reason: "",
                    },
                },
            ],
            errors: [],
        }
        "##,
        );
    }

    #[test]
    fn range_suppression_extra_disable() {
        let source = "
# ruff: disable[foo] first
print('hello')
# ruff: disable[foo] second
print('hello')
# ruff: enable[foo]
";
        assert_debug_snapshot!(
            Suppressions::debug(source),
            @r##"
        Suppressions {
            valid: [
                Suppression {
                    covered_source: "# ruff: disable[foo] first\nprint('hello')\n# ruff: disable[foo] second\nprint('hello')\n# ruff: enable[foo]",
                    code: "foo",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[foo] first",
                            action: Disable,
                            codes: [
                                "foo",
                            ],
                            reason: "first",
                        },
                        SuppressionComment {
                            text: "# ruff: enable[foo]",
                            action: Enable,
                            codes: [
                                "foo",
                            ],
                            reason: "",
                        },
                    ],
                },
                Suppression {
                    covered_source: "# ruff: disable[foo] second\nprint('hello')\n# ruff: enable[foo]\n",
                    code: "foo",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[foo] second",
                            action: Disable,
                            codes: [
                                "foo",
                            ],
                            reason: "second",
                        },
                    ],
                },
            ],
            invalid: [],
            errors: [],
        }
        "##,
        );
    }

    #[test]
    fn combined_range_suppressions() {
        let source = "
# ruff: noqa  # ignored

# comment here
print('hello')  # ruff: disable[phi] trailing

# ruff: disable[alpha]
def foo():
    # ruff: disable[beta,gamma]
    if True:
        # ruff: disable[delta] unmatched
        pass
    # ruff: enable[beta,gamma]
# ruff: enable[alpha]

# ruff: disable  # parse error!
def bar():
    # ruff: disable[zeta] unmatched
    pass
# ruff: enable[zeta] underindented
    pass
";
        assert_debug_snapshot!(
            Suppressions::debug(source),
            @r##"
        Suppressions {
            valid: [
                Suppression {
                    covered_source: "# ruff: disable[delta] unmatched\n        pass\n    # ruff: enable[beta,gamma]\n# ruff: enable[alpha]\n\n# ruff: disable  # parse error!\n",
                    code: "delta",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[delta] unmatched",
                            action: Disable,
                            codes: [
                                "delta",
                            ],
                            reason: "unmatched",
                        },
                    ],
                },
                Suppression {
                    covered_source: "# ruff: disable[beta,gamma]\n    if True:\n        # ruff: disable[delta] unmatched\n        pass\n    # ruff: enable[beta,gamma]",
                    code: "beta",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[beta,gamma]",
                            action: Disable,
                            codes: [
                                "beta",
                                "gamma",
                            ],
                            reason: "",
                        },
                        SuppressionComment {
                            text: "# ruff: enable[beta,gamma]",
                            action: Enable,
                            codes: [
                                "beta",
                                "gamma",
                            ],
                            reason: "",
                        },
                    ],
                },
                Suppression {
                    covered_source: "# ruff: disable[beta,gamma]\n    if True:\n        # ruff: disable[delta] unmatched\n        pass\n    # ruff: enable[beta,gamma]",
                    code: "gamma",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[beta,gamma]",
                            action: Disable,
                            codes: [
                                "beta",
                                "gamma",
                            ],
                            reason: "",
                        },
                        SuppressionComment {
                            text: "# ruff: enable[beta,gamma]",
                            action: Enable,
                            codes: [
                                "beta",
                                "gamma",
                            ],
                            reason: "",
                        },
                    ],
                },
                Suppression {
                    covered_source: "# ruff: disable[zeta] unmatched\n    pass\n# ruff: enable[zeta] underindented\n    pass\n",
                    code: "zeta",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[zeta] unmatched",
                            action: Disable,
                            codes: [
                                "zeta",
                            ],
                            reason: "unmatched",
                        },
                    ],
                },
                Suppression {
                    covered_source: "# ruff: disable[alpha]\ndef foo():\n    # ruff: disable[beta,gamma]\n    if True:\n        # ruff: disable[delta] unmatched\n        pass\n    # ruff: enable[beta,gamma]\n# ruff: enable[alpha]",
                    code: "alpha",
                    comments: [
                        SuppressionComment {
                            text: "# ruff: disable[alpha]",
                            action: Disable,
                            codes: [
                                "alpha",
                            ],
                            reason: "",
                        },
                        SuppressionComment {
                            text: "# ruff: enable[alpha]",
                            action: Enable,
                            codes: [
                                "alpha",
                            ],
                            reason: "",
                        },
                    ],
                },
            ],
            invalid: [
                InvalidSuppression {
                    kind: Trailing,
                    comment: SuppressionComment {
                        text: "# ruff: disable[phi] trailing",
                        action: Disable,
                        codes: [
                            "phi",
                        ],
                        reason: "trailing",
                    },
                },
                InvalidSuppression {
                    kind: Indentation,
                    comment: SuppressionComment {
                        text: "# ruff: enable[zeta] underindented",
                        action: Enable,
                        codes: [
                            "zeta",
                        ],
                        reason: "underindented",
                    },
                },
            ],
            errors: [
                ParseError {
                    text: "# ruff: disable  # parse error!",
                    kind: MissingCodes,
                },
            ],
        }
        "##,
        );
    }

    #[test]
    fn parse_unrelated_comment() {
        assert_debug_snapshot!(
            parse_suppression_comment("# hello world"),
            @r"
        Err(
            ParseError {
                kind: NotASuppression,
                range: 0..13,
            },
        )
        ",
        );
    }

    #[test]
    fn parse_invalid_action() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: lol[hi]"),
            @r"
        Err(
            ParseError {
                kind: UnknownAction,
                range: 0..15,
            },
        )
        ",
        );
    }

    #[test]
    fn parse_missing_codes() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable"),
            @r"
        Err(
            ParseError {
                kind: MissingCodes,
                range: 0..15,
            },
        )
        ",
        );
    }

    #[test]
    fn parse_empty_codes() {
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
    fn parse_missing_bracket() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo"),
            @r"
        Err(
            ParseError {
                kind: MissingBracket,
                range: 0..19,
            },
        )
        ",
        );
    }

    #[test]
    fn parse_missing_comma() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo bar]"),
            @r"
        Err(
            ParseError {
                kind: MissingComma,
                range: 0..24,
            },
        )
        ",
        );
    }

    #[test]
    fn disable_single_code() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo]"),
            @r##"
        Ok(
            SuppressionComment {
                text: "# ruff: disable[foo]",
                action: Disable,
                codes: [
                    "foo",
                ],
                reason: "",
            },
        )
        "##,
        );
    }

    #[test]
    fn disable_single_code_with_reason() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo] I like bar better"),
            @r##"
        Ok(
            SuppressionComment {
                text: "# ruff: disable[foo] I like bar better",
                action: Disable,
                codes: [
                    "foo",
                ],
                reason: "I like bar better",
            },
        )
        "##,
        );
    }

    #[test]
    fn disable_multiple_codes() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: disable[foo, bar]"),
            @r##"
        Ok(
            SuppressionComment {
                text: "# ruff: disable[foo, bar]",
                action: Disable,
                codes: [
                    "foo",
                    "bar",
                ],
                reason: "",
            },
        )
        "##,
        );
    }

    #[test]
    fn enable_single_code() {
        assert_debug_snapshot!(
            parse_suppression_comment("# ruff: enable[some-thing]"),
            @r##"
        Ok(
            SuppressionComment {
                text: "# ruff: enable[some-thing]",
                action: Enable,
                codes: [
                    "some-thing",
                ],
                reason: "",
            },
        )
        "##,
        );
    }

    #[test]
    fn trailing_comment() {
        let source = "print('hello world')  # ruff: enable[some-thing]";
        let comment = parse_suppression_comment(source);
        assert_debug_snapshot!(
            comment,
            @r##"
        Ok(
            SuppressionComment {
                text: "# ruff: enable[some-thing]",
                action: Enable,
                codes: [
                    "some-thing",
                ],
                reason: "",
            },
        )
        "##,
        );
    }

    #[test]
    fn indented_comment() {
        let source = "    # ruff: enable[some-thing]";
        let comment = parse_suppression_comment(source);
        assert_debug_snapshot!(
            comment,
            @r##"
        Ok(
            SuppressionComment {
                text: "# ruff: enable[some-thing]",
                action: Enable,
                codes: [
                    "some-thing",
                ],
                reason: "",
            },
        )
        "##,
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

    /// Parse a single suppression comment for testing
    fn parse_suppression_comment(
        source: &'_ str,
    ) -> Result<DebugSuppressionComment<'_>, ParseError> {
        let offset = TextSize::new(source.find('#').unwrap_or(0).try_into().unwrap());
        let mut parser = SuppressionParser::new(
            source,
            TextRange::new(offset, TextSize::try_from(source.len()).unwrap()),
        );
        match parser.parse_comment() {
            Ok(comment) => Ok(DebugSuppressionComment { source, comment }),
            Err(error) => Err(error),
        }
    }

    impl Suppressions {
        /// Parse all suppressions and errors in a module for testing
        fn debug(source: &'_ str) -> DebugSuppressions<'_> {
            let parsed = parse(source, ParseOptions::from(Mode::Module)).unwrap();
            let suppressions = Suppressions::from_tokens(source, parsed.tokens());
            DebugSuppressions {
                source,
                suppressions,
            }
        }
    }

    struct DebugSuppressions<'a> {
        source: &'a str,
        suppressions: Suppressions,
    }

    impl fmt::Debug for DebugSuppressions<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.debug_struct("Suppressions")
                .field(
                    "valid",
                    &self
                        .suppressions
                        .valid
                        .iter()
                        .map(|suppression| DebugSuppression {
                            source: self.source,
                            suppression,
                        })
                        .collect_vec(),
                )
                .field(
                    "invalid",
                    &self
                        .suppressions
                        .invalid
                        .iter()
                        .map(|invalid| DebugInvalidSuppression {
                            source: self.source,
                            invalid,
                        })
                        .collect_vec(),
                )
                .field(
                    "errors",
                    &self
                        .suppressions
                        .errors
                        .iter()
                        .map(|error| DebugParseError {
                            source: self.source,
                            error,
                        })
                        .collect_vec(),
                )
                .finish()
        }
    }

    struct DebugSuppression<'a> {
        source: &'a str,
        suppression: &'a Suppression,
    }

    impl fmt::Debug for DebugSuppression<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.debug_struct("Suppression")
                .field("covered_source", &&self.source[self.suppression.range])
                .field("code", &self.suppression.code)
                .field(
                    "comments",
                    &self
                        .suppression
                        .comments
                        .iter()
                        .map(|comment| DebugSuppressionComment {
                            source: self.source,
                            comment: comment.clone(),
                        })
                        .collect_vec(),
                )
                .finish()
        }
    }

    struct DebugInvalidSuppression<'a> {
        source: &'a str,
        invalid: &'a InvalidSuppression,
    }

    impl fmt::Debug for DebugInvalidSuppression<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.debug_struct("InvalidSuppression")
                .field("kind", &self.invalid.kind)
                .field(
                    "comment",
                    &DebugSuppressionComment {
                        source: self.source,
                        comment: self.invalid.comment.clone(),
                    },
                )
                .finish()
        }
    }

    struct DebugParseError<'a> {
        source: &'a str,
        error: &'a ParseError,
    }

    impl fmt::Debug for DebugParseError<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.debug_struct("ParseError")
                .field("text", &&self.source[self.error.range])
                .field("kind", &self.error.kind)
                .finish()
        }
    }

    struct DebugSuppressionComment<'a> {
        source: &'a str,
        comment: SuppressionComment,
    }

    impl fmt::Debug for DebugSuppressionComment<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            f.debug_struct("SuppressionComment")
                .field("text", &&self.source[self.comment.range])
                .field("action", &self.comment.action)
                .field(
                    "codes",
                    &DebugCodes {
                        source: self.source,
                        codes: &self.comment.codes,
                    },
                )
                .field("reason", &&self.source[self.comment.reason])
                .finish()
        }
    }

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
}
