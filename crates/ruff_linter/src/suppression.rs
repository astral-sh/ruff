use compact_str::CompactString;
use core::fmt;
use itertools::Itertools;
use ruff_db::diagnostic::Diagnostic;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_index::Indexer;
use rustc_hash::FxHashSet;
use std::cell::Cell;
use std::{error::Error, fmt::Formatter};
use thiserror::Error;

use ruff_python_trivia::{Cursor, indentation_at_offset};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize, TextSlice};
use smallvec::{SmallVec, smallvec};

use crate::checkers::ast::LintContext;
use crate::codes::Rule;
use crate::fix::edits::delete_comment;
use crate::preview::is_range_suppressions_enabled;
use crate::rule_redirects::get_redirect_target;
use crate::rules::ruff::rules::{
    InvalidRuleCode, InvalidRuleCodeKind, InvalidSuppressionComment, InvalidSuppressionCommentKind,
    UnmatchedSuppressionComment, UnusedCodes, UnusedNOQA, UnusedNOQAKind, code_is_valid,
};
use crate::settings::LinterSettings;
use crate::{Locator, Violation};

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

#[derive(Debug)]
pub(crate) struct Suppression {
    /// The lint code being suppressed
    code: CompactString,

    /// Range for which the suppression applies
    range: TextRange,

    /// Whether this suppression actually suppressed a diagnostic
    used: Cell<bool>,

    comments: DisableEnableComments,
}

impl Suppression {
    fn codes(&self) -> &[TextRange] {
        &self.comments.disable_comment().codes
    }
}

#[derive(Debug)]
pub(crate) enum DisableEnableComments {
    /// An implicitly closed disable comment without a matching enable comment.
    Disable(SuppressionComment),
    /// A matching pair of disable and enable comments.
    DisableEnable(SuppressionComment, SuppressionComment),
}

impl DisableEnableComments {
    pub(crate) fn disable_comment(&self) -> &SuppressionComment {
        match self {
            DisableEnableComments::Disable(comment) => comment,
            DisableEnableComments::DisableEnable(disable, _) => disable,
        }
    }
    pub(crate) fn enable_comment(&self) -> Option<&SuppressionComment> {
        match self {
            DisableEnableComments::Disable(_) => None,
            DisableEnableComments::DisableEnable(_, enable) => Some(enable),
        }
    }
}

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
#[derive(Debug, Default)]
pub struct Suppressions {
    /// Valid suppression ranges with associated comments
    valid: Vec<Suppression>,

    /// Invalid suppression comments
    invalid: Vec<InvalidSuppression>,

    /// Parse errors from suppression comments
    errors: Vec<ParseError>,
}

#[derive(Debug)]
struct SuppressionDiagnostic<'a> {
    suppression: &'a Suppression,
    invalid_codes: Vec<&'a str>,
    duplicated_codes: Vec<&'a str>,
    disabled_codes: Vec<&'a str>,
    unused_codes: Vec<&'a str>,
}

impl<'a> SuppressionDiagnostic<'a> {
    fn new(suppression: &'a Suppression) -> Self {
        Self {
            suppression,
            invalid_codes: Vec::new(),
            duplicated_codes: Vec::new(),
            disabled_codes: Vec::new(),
            unused_codes: Vec::new(),
        }
    }

    fn any_invalid(&self) -> bool {
        !self.invalid_codes.is_empty()
    }

    fn any_unused(&self) -> bool {
        !self.disabled_codes.is_empty()
            || !self.duplicated_codes.is_empty()
            || !self.unused_codes.is_empty()
    }
}

impl Suppressions {
    pub fn from_tokens(
        settings: &LinterSettings,
        source: &str,
        tokens: &Tokens,
        indexer: &Indexer,
    ) -> Suppressions {
        if is_range_suppressions_enabled(settings) {
            let builder = SuppressionsBuilder::new(source);
            builder.load_from_tokens(tokens, indexer)
        } else {
            Suppressions::default()
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.valid.is_empty() && self.invalid.is_empty() && self.errors.is_empty()
    }

    /// Check if a diagnostic is suppressed by any known range suppressions
    pub(crate) fn check_diagnostic(&self, diagnostic: &Diagnostic) -> bool {
        if self.valid.is_empty() {
            return false;
        }

        let Some(code) = diagnostic.secondary_code() else {
            return false;
        };
        let Some(span) = diagnostic.primary_span() else {
            return false;
        };
        let Some(range) = span.range() else {
            return false;
        };

        for suppression in &self.valid {
            let suppression_code =
                get_redirect_target(suppression.code.as_str()).unwrap_or(suppression.code.as_str());
            if *code == suppression_code && suppression.range.contains_range(range) {
                suppression.used.set(true);
                return true;
            }
        }
        false
    }

    pub(crate) fn check_suppressions(&self, context: &LintContext, locator: &Locator) {
        let mut grouped_diagnostic: Option<(TextRange, SuppressionDiagnostic)> = None;
        let mut unmatched_ranges = FxHashSet::default();
        for suppression in &self.valid {
            let key = suppression.comments.disable_comment().range;

            // Process any pending grouped diagnostics
            if let Some((group_key, ref group)) = grouped_diagnostic
                && key != group_key
            {
                if group.any_invalid() {
                    Suppressions::report_suppression_codes(
                        context,
                        locator,
                        group.suppression,
                        &group.invalid_codes,
                        true,
                        InvalidRuleCode {
                            rule_code: group.invalid_codes.iter().join(", "),
                            kind: InvalidRuleCodeKind::Suppression,
                            whole_comment: group.suppression.codes().len()
                                == group.invalid_codes.len(),
                        },
                    );
                }
                if group.any_unused() {
                    let mut codes = group.disabled_codes.clone();
                    codes.extend(group.unused_codes.clone());
                    Suppressions::report_suppression_codes(
                        context,
                        locator,
                        group.suppression,
                        &codes,
                        false,
                        UnusedNOQA {
                            codes: Some(UnusedCodes {
                                disabled: group
                                    .disabled_codes
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect_vec(),
                                duplicated: group
                                    .duplicated_codes
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect_vec(),
                                unmatched: group
                                    .unused_codes
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect_vec(),
                                ..Default::default()
                            }),
                            kind: UnusedNOQAKind::Suppression,
                        },
                    );
                }
                grouped_diagnostic = None;
            }

            let code_str = suppression.code.as_str();

            if !code_is_valid(&suppression.code, &context.settings().external) {
                // InvalidRuleCode
                let (_key, group) = grouped_diagnostic
                    .get_or_insert_with(|| (key, SuppressionDiagnostic::new(suppression)));
                group.invalid_codes.push(code_str);
            } else if !suppression.used.get() {
                // UnusedNOQA
                let Ok(rule) = Rule::from_code(
                    get_redirect_target(&suppression.code).unwrap_or(&suppression.code),
                ) else {
                    continue; // "external" lint code, don't treat it as unused
                };

                let (_key, group) = grouped_diagnostic
                    .get_or_insert_with(|| (key, SuppressionDiagnostic::new(suppression)));

                if context.is_rule_enabled(rule) {
                    if suppression
                        .comments
                        .disable_comment()
                        .codes_as_str(locator.contents())
                        .filter(|code| *code == code_str)
                        .count()
                        > 1
                    {
                        group.duplicated_codes.push(code_str);
                    } else {
                        group.unused_codes.push(code_str);
                    }
                } else {
                    group.disabled_codes.push(code_str);
                }
            } else if let DisableEnableComments::Disable(comment) = &suppression.comments {
                // UnmatchedSuppressionComment
                if unmatched_ranges.insert(comment.range) {
                    context.report_diagnostic_if_enabled(
                        UnmatchedSuppressionComment {},
                        comment.range,
                    );
                }
            }
        }

        if context.is_rule_enabled(Rule::InvalidSuppressionComment) {
            for error in &self.errors {
                context
                    .report_diagnostic(
                        InvalidSuppressionComment {
                            kind: InvalidSuppressionCommentKind::Error(error.kind),
                        },
                        error.range,
                    )
                    .set_fix(Fix::unsafe_edit(delete_comment(error.range, locator)));
            }
        }

        if context.is_rule_enabled(Rule::InvalidSuppressionComment) {
            for invalid in &self.invalid {
                context
                    .report_diagnostic(
                        InvalidSuppressionComment {
                            kind: InvalidSuppressionCommentKind::Invalid(invalid.kind),
                        },
                        invalid.comment.range,
                    )
                    .set_fix(Fix::unsafe_edit(delete_comment(
                        invalid.comment.range,
                        locator,
                    )));
            }
        }
    }

    fn report_suppression_codes<T: Violation>(
        context: &LintContext,
        locator: &Locator,
        suppression: &Suppression,
        remove_codes: &[&str],
        highlight_only_code: bool,
        kind: T,
    ) {
        let disable_comment = suppression.comments.disable_comment();
        let (range, edit) = Suppressions::delete_codes_or_comment(
            locator,
            disable_comment,
            remove_codes,
            highlight_only_code,
        );
        if let Some(mut diagnostic) = context.report_diagnostic_if_enabled(kind, range) {
            if let Some(enable_comment) = suppression.comments.enable_comment() {
                let (enable_range, enable_range_edit) = Suppressions::delete_codes_or_comment(
                    locator,
                    enable_comment,
                    remove_codes,
                    highlight_only_code,
                );
                diagnostic.secondary_annotation("", enable_range);
                diagnostic.set_fix(Fix::safe_edits(edit, [enable_range_edit]));
            } else {
                diagnostic.set_fix(Fix::safe_edit(edit));
            }
        }
    }

    fn delete_codes_or_comment(
        locator: &Locator<'_>,
        comment: &SuppressionComment,
        remove_codes: &[&str],
        highlight_only_code: bool,
    ) -> (TextRange, Edit) {
        let mut range = comment.range;
        let edit = if comment.codes.len() == 1 {
            if highlight_only_code {
                range = comment.codes[0];
            }
            delete_comment(comment.range, locator)
        } else if remove_codes.len() == 1 {
            let code_index = comment
                .codes
                .iter()
                .position(|range| locator.slice(range) == remove_codes[0])
                .unwrap();
            if highlight_only_code {
                range = comment.codes[code_index];
            }
            let code_range = if code_index < (comment.codes.len() - 1) {
                TextRange::new(
                    comment.codes[code_index].start(),
                    comment.codes[code_index + 1].start(),
                )
            } else {
                TextRange::new(
                    comment.codes[code_index - 1].end(),
                    comment.codes[code_index].end(),
                )
            };
            Edit::range_deletion(code_range)
        } else {
            let first = comment
                .codes
                .first()
                .expect("suppression comment without codes");
            let last = comment
                .codes
                .last()
                .expect("suppression comment without codes");
            let code_range = TextRange::new(first.start(), last.end());
            let remaining = comment
                .codes_as_str(locator.contents())
                .filter(|code| !remove_codes.contains(code))
                .dedup()
                .join(", ");

            if remaining.is_empty() {
                delete_comment(comment.range, locator)
            } else {
                Edit::range_replacement(remaining, code_range)
            }
        };
        (range, edit)
    }
}

#[derive(Default)]
pub(crate) struct SuppressionsBuilder<'a> {
    source: &'a str,

    valid: Vec<Suppression>,
    invalid: Vec<InvalidSuppression>,

    pending: Vec<PendingSuppressionComment<'a>>,
}

impl<'a> SuppressionsBuilder<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            ..Default::default()
        }
    }

    pub(crate) fn load_from_tokens(mut self, tokens: &Tokens, indexer: &Indexer) -> Suppressions {
        let mut indents: Vec<&str> = vec![];
        let mut errors = Vec::new();

        let mut suppressions = indexer
            .comment_ranges()
            .iter()
            .copied()
            .filter_map(|comment_range| {
                let mut parser = SuppressionParser::new(self.source, comment_range);
                match parser.parse_comment() {
                    Ok(comment) => Some(comment),
                    Err(ParseError {
                        kind: ParseErrorKind::NotASuppression,
                        ..
                    }) => None,
                    Err(error) => {
                        errors.push(error);
                        None
                    }
                }
            })
            .peekable();

        'comments: while let Some(suppression) = suppressions.peek() {
            indents.clear();

            let (before, after) = tokens.split_at(suppression.range.start());
            let last_indent = before
                .iter()
                .rfind(|token| token.kind() == TokenKind::Indent)
                .map(|token| self.source.slice(token))
                .unwrap_or_default();

            indents.push(last_indent);

            // Iterate through tokens, tracking indentation, filtering trailing comments, and then
            // looking for matching comments from the previous block when reaching a dedent token.
            for (token_index, token) in after.iter().enumerate() {
                let current_indent = indents.last().copied().unwrap_or_default();
                match token.kind() {
                    TokenKind::Indent => {
                        indents.push(self.source.slice(token));
                    }
                    TokenKind::Dedent => {
                        self.match_comments(current_indent, token.range());

                        indents.pop();

                        if indents.is_empty() || self.pending.is_empty() {
                            continue 'comments;
                        }
                    }
                    TokenKind::Comment => {
                        let Some(suppression) =
                            suppressions.next_if(|suppression| suppression.range == token.range())
                        else {
                            continue;
                        };

                        let Some(indent) =
                            indentation_at_offset(suppression.range.start(), self.source)
                        else {
                            // trailing suppressions are not supported
                            self.invalid.push(InvalidSuppression {
                                kind: InvalidSuppressionKind::Trailing,
                                comment: suppression,
                            });
                            continue;
                        };

                        // comment matches current block's indentation, or precedes an indent/dedent token
                        if indent == current_indent
                            || after[token_index..]
                                .iter()
                                .find(|t| !t.kind().is_trivia())
                                .is_some_and(|t| {
                                    matches!(t.kind(), TokenKind::Dedent | TokenKind::Indent)
                                })
                        {
                            self.pending.push(PendingSuppressionComment {
                                indent,
                                comment: suppression,
                            });
                        } else {
                            // weirdly indented? ¯\_(ツ)_/¯
                            self.invalid.push(InvalidSuppression {
                                kind: InvalidSuppressionKind::Indentation,
                                comment: suppression,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        self.match_comments("", TextRange::up_to(self.source.text_len()));

        Suppressions {
            valid: self.valid,
            invalid: self.invalid,
            errors,
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
                        comments: DisableEnableComments::DisableEnable(
                            comment.comment.clone(),
                            other.comment.clone(),
                        ),
                        used: false.into(),
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
                        comments: DisableEnableComments::Disable(comment.comment.clone()),
                        used: false.into(),
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
pub(crate) enum ParseErrorKind {
    #[error("not a suppression comment")]
    NotASuppression,

    #[error("comment doesn't start with `#`")]
    CommentWithoutHash,

    #[error("unknown ruff directive")]
    UnknownAction,

    #[error("missing suppression codes like `[E501, ...]`")]
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
        } else if self.cursor.as_str().starts_with("noqa")
            || self.cursor.as_str().starts_with("isort")
        {
            // alternate suppression variants, ignore for now
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
    use ruff_python_index::Indexer;
    use ruff_python_parser::{Mode, ParseOptions, parse};
    use ruff_text_size::{TextLen, TextRange, TextSize};
    use similar::DiffableStr;

    use crate::{
        settings::LinterSettings,
        suppression::{
            InvalidSuppression, ParseError, Suppression, SuppressionAction, SuppressionComment,
            SuppressionParser, Suppressions,
        },
    };

    #[test]
    fn no_suppression() {
        let source = "
# this is a comment
print('hello')
";
        assert_debug_snapshot!(
            Suppressions::debug(source),
            @"
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
            @"
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
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[foo]",
                        action: Disable,
                        codes: [
                            "foo",
                        ],
                        reason: "",
                    },
                    enable_comment: SuppressionComment {
                        text: "# ruff: enable[foo]",
                        action: Enable,
                        codes: [
                            "foo",
                        ],
                        reason: "",
                    },
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
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[bar]",
                        action: Disable,
                        codes: [
                            "bar",
                        ],
                        reason: "",
                    },
                    enable_comment: None,
                },
                Suppression {
                    covered_source: "# ruff: disable[foo]\nprint('hello')\n\ndef foo():\n    # ruff: disable[bar]\n    print('hello')\n\n",
                    code: "foo",
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[foo]",
                        action: Disable,
                        codes: [
                            "foo",
                        ],
                        reason: "",
                    },
                    enable_comment: None,
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
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[bar]",
                        action: Disable,
                        codes: [
                            "bar",
                        ],
                        reason: "",
                    },
                    enable_comment: SuppressionComment {
                        text: "# ruff: enable[bar]",
                        action: Enable,
                        codes: [
                            "bar",
                        ],
                        reason: "",
                    },
                },
                Suppression {
                    covered_source: "# ruff: disable[foo]\n    def bar(self):\n        # ruff: disable[bar]\n        print('hello')\n        # ruff: enable[bar]\n    # ruff: enable[foo]",
                    code: "foo",
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[foo]",
                        action: Disable,
                        codes: [
                            "foo",
                        ],
                        reason: "",
                    },
                    enable_comment: SuppressionComment {
                        text: "# ruff: enable[foo]",
                        action: Enable,
                        codes: [
                            "foo",
                        ],
                        reason: "",
                    },
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
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[foo]",
                        action: Disable,
                        codes: [
                            "foo",
                        ],
                        reason: "",
                    },
                    enable_comment: SuppressionComment {
                        text: "# ruff: enable[foo]",
                        action: Enable,
                        codes: [
                            "foo",
                        ],
                        reason: "",
                    },
                },
                Suppression {
                    covered_source: "# ruff: disable[bar]\n    print('hello')\n    # ruff: enable[foo]\n    print('hello')\n    # ruff: enable[bar]",
                    code: "bar",
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[bar]",
                        action: Disable,
                        codes: [
                            "bar",
                        ],
                        reason: "",
                    },
                    enable_comment: SuppressionComment {
                        text: "# ruff: enable[bar]",
                        action: Enable,
                        codes: [
                            "bar",
                        ],
                        reason: "",
                    },
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
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[foo, bar]",
                        action: Disable,
                        codes: [
                            "foo",
                            "bar",
                        ],
                        reason: "",
                    },
                    enable_comment: SuppressionComment {
                        text: "# ruff: enable[foo, bar]",
                        action: Enable,
                        codes: [
                            "foo",
                            "bar",
                        ],
                        reason: "",
                    },
                },
                Suppression {
                    covered_source: "# ruff: disable[foo, bar]\nprint('hello')\n# ruff: enable[foo, bar]",
                    code: "bar",
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[foo, bar]",
                        action: Disable,
                        codes: [
                            "foo",
                            "bar",
                        ],
                        reason: "",
                    },
                    enable_comment: SuppressionComment {
                        text: "# ruff: enable[foo, bar]",
                        action: Enable,
                        codes: [
                            "foo",
                            "bar",
                        ],
                        reason: "",
                    },
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
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[foo]",
                        action: Disable,
                        codes: [
                            "foo",
                        ],
                        reason: "",
                    },
                    enable_comment: None,
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
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[foo, bar]",
                        action: Disable,
                        codes: [
                            "foo",
                            "bar",
                        ],
                        reason: "",
                    },
                    enable_comment: None,
                },
                Suppression {
                    covered_source: "# ruff: disable[foo, bar]\nprint('hello')\n# ruff: enable[bar, foo]\n",
                    code: "bar",
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[foo, bar]",
                        action: Disable,
                        codes: [
                            "foo",
                            "bar",
                        ],
                        reason: "",
                    },
                    enable_comment: None,
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
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[foo] first",
                        action: Disable,
                        codes: [
                            "foo",
                        ],
                        reason: "first",
                    },
                    enable_comment: SuppressionComment {
                        text: "# ruff: enable[foo]",
                        action: Enable,
                        codes: [
                            "foo",
                        ],
                        reason: "",
                    },
                },
                Suppression {
                    covered_source: "# ruff: disable[foo] second\nprint('hello')\n# ruff: enable[foo]\n",
                    code: "foo",
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[foo] second",
                        action: Disable,
                        codes: [
                            "foo",
                        ],
                        reason: "second",
                    },
                    enable_comment: None,
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
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[delta] unmatched",
                        action: Disable,
                        codes: [
                            "delta",
                        ],
                        reason: "unmatched",
                    },
                    enable_comment: None,
                },
                Suppression {
                    covered_source: "# ruff: disable[beta,gamma]\n    if True:\n        # ruff: disable[delta] unmatched\n        pass\n    # ruff: enable[beta,gamma]",
                    code: "beta",
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[beta,gamma]",
                        action: Disable,
                        codes: [
                            "beta",
                            "gamma",
                        ],
                        reason: "",
                    },
                    enable_comment: SuppressionComment {
                        text: "# ruff: enable[beta,gamma]",
                        action: Enable,
                        codes: [
                            "beta",
                            "gamma",
                        ],
                        reason: "",
                    },
                },
                Suppression {
                    covered_source: "# ruff: disable[beta,gamma]\n    if True:\n        # ruff: disable[delta] unmatched\n        pass\n    # ruff: enable[beta,gamma]",
                    code: "gamma",
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[beta,gamma]",
                        action: Disable,
                        codes: [
                            "beta",
                            "gamma",
                        ],
                        reason: "",
                    },
                    enable_comment: SuppressionComment {
                        text: "# ruff: enable[beta,gamma]",
                        action: Enable,
                        codes: [
                            "beta",
                            "gamma",
                        ],
                        reason: "",
                    },
                },
                Suppression {
                    covered_source: "# ruff: disable[zeta] unmatched\n    pass\n# ruff: enable[zeta] underindented\n    pass\n",
                    code: "zeta",
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[zeta] unmatched",
                        action: Disable,
                        codes: [
                            "zeta",
                        ],
                        reason: "unmatched",
                    },
                    enable_comment: None,
                },
                Suppression {
                    covered_source: "# ruff: disable[alpha]\ndef foo():\n    # ruff: disable[beta,gamma]\n    if True:\n        # ruff: disable[delta] unmatched\n        pass\n    # ruff: enable[beta,gamma]\n# ruff: enable[alpha]",
                    code: "alpha",
                    disable_comment: SuppressionComment {
                        text: "# ruff: disable[alpha]",
                        action: Disable,
                        codes: [
                            "alpha",
                        ],
                        reason: "",
                    },
                    enable_comment: SuppressionComment {
                        text: "# ruff: enable[alpha]",
                        action: Enable,
                        codes: [
                            "alpha",
                        ],
                        reason: "",
                    },
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
            @"
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
            @"
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
            @"
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
            @"
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
            @"
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
            @"
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
        let mut parser =
            SuppressionParser::new(source, TextRange::new(0.into(), source.text_len()));
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
        let mut parser = SuppressionParser::new(source, TextRange::new(offset, source.text_len()));
        match parser.parse_comment() {
            Ok(comment) => Ok(DebugSuppressionComment {
                source,
                comment: Some(comment),
            }),
            Err(error) => Err(error),
        }
    }

    impl Suppressions {
        /// Parse all suppressions and errors in a module for testing
        fn debug(source: &'_ str) -> DebugSuppressions<'_> {
            let parsed = parse(source, ParseOptions::from(Mode::Module)).unwrap();
            let indexer = Indexer::from_tokens(parsed.tokens(), source);
            let suppressions = Suppressions::from_tokens(
                &LinterSettings::default().with_preview_mode(),
                source,
                parsed.tokens(),
                &indexer,
            );
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
                    "disable_comment",
                    &DebugSuppressionComment {
                        source: self.source,
                        comment: Some(self.suppression.comments.disable_comment().clone()),
                    },
                )
                .field(
                    "enable_comment",
                    &DebugSuppressionComment {
                        source: self.source,
                        comment: self.suppression.comments.enable_comment().cloned(),
                    },
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
                        comment: Some(self.invalid.comment.clone()),
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
        comment: Option<SuppressionComment>,
    }

    impl fmt::Debug for DebugSuppressionComment<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            match &self.comment {
                Some(comment) => f
                    .debug_struct("SuppressionComment")
                    .field("text", &&self.source[comment.range])
                    .field("action", &comment.action)
                    .field(
                        "codes",
                        &DebugCodes {
                            source: self.source,
                            codes: &comment.codes,
                        },
                    )
                    .field("reason", &&self.source[comment.reason])
                    .finish(),
                None => f.debug_tuple("None").finish(),
            }
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
