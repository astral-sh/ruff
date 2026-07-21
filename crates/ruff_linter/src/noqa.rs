use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Display;
use std::fs;
use std::ops::Add;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
use log::warn;

use ruff_db::diagnostic::{Diagnostic, SecondaryCode};
use ruff_python_trivia::PythonWhitespace;
use ruff_python_trivia::{CommentRanges, Cursor, indentation_at_offset};
use ruff_source_file::{LineEnding, LineRanges};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use rustc_hash::FxHashSet;

use crate::Edit;
use crate::Locator;
use crate::fs::relativize_path;
use crate::registry::Rule;
use crate::rule_redirects::get_redirect_target;
use crate::suppression::{self, Suppressions};

/// Generates an array of edits that matches the length of `diagnostics`.
/// Each potential edit in the array is paired, in order, with the associated diagnostic.
/// Each edit will add a suppression comment to the appropriate line in the source to hide
/// the diagnostic. These edits may conflict with each other and should not be applied
/// simultaneously.
#[expect(clippy::too_many_arguments)]
pub fn generate_suppression_edits(
    path: &Path,
    diagnostics: &[Diagnostic],
    locator: &Locator,
    comment_ranges: &CommentRanges,
    external: &[String],
    noqa_line_for: &NoqaMapping,
    line_ending: LineEnding,
    suppressions: &Suppressions,
    suppression_kind: SuppressionKind,
) -> Vec<Option<Edit>> {
    let file_directives = FileNoqaDirectives::extract(locator, comment_ranges, external, path);
    let exemption = FileExemption::from(&file_directives);
    let directives = NoqaDirectives::from_commented_ranges(comment_ranges, path, locator);
    let comments = find_suppression_comments(
        diagnostics,
        locator,
        &exemption,
        &directives,
        noqa_line_for,
        suppressions,
        suppression_kind,
    );
    build_suppression_edits_by_diagnostic(comments, locator, line_ending, None, suppression_kind)
}

/// A directive to ignore a set of rules either for a given line of Python source code or an entire file (e.g.,
/// `# noqa: F401, F841` or `#ruff: noqa: F401, F841`).
#[derive(Debug)]
pub(crate) enum Directive<'a> {
    /// The `noqa` directive ignores all rules (e.g., `# noqa`).
    All(All),
    /// The `noqa` directive ignores specific rules (e.g., `# noqa: F401, F841`).
    Codes(Codes<'a>),
}

impl Ranged for Directive<'_> {
    fn range(&self) -> TextRange {
        match self {
            Directive::All(all) => all.range(),
            Directive::Codes(codes) => codes.range(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct All {
    range: TextRange,
}

impl Ranged for All {
    /// The range of the `noqa` directive.
    fn range(&self) -> TextRange {
        self.range
    }
}

/// An individual rule code in a `noqa` directive (e.g., `F401`).
#[derive(Debug)]
pub(crate) struct Code<'a> {
    code: &'a str,
    range: TextRange,
}

impl<'a> Code<'a> {
    /// The code that is ignored by the `noqa` directive.
    pub(crate) fn as_str(&self) -> &'a str {
        self.code
    }
}

impl Display for Code<'_> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt.write_str(self.code)
    }
}

impl Ranged for Code<'_> {
    /// The range of the rule code.
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Debug)]
pub(crate) struct Codes<'a> {
    range: TextRange,
    codes: Vec<Code<'a>>,
}

impl Codes<'_> {
    /// Returns an iterator over the [`Code`]s in the `noqa` directive.
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Code<'_>> {
        self.codes.iter()
    }

    /// Returns `true` if the string list of `codes` includes `code` (or an alias
    /// thereof).
    pub(crate) fn includes<T: for<'a> PartialEq<&'a str>>(&self, needle: &T) -> bool {
        self.iter()
            .any(|code| *needle == get_redirect_target(code.as_str()).unwrap_or(code.as_str()))
    }

    pub(crate) fn len(&self) -> usize {
        self.codes.len()
    }
}

impl Ranged for Codes<'_> {
    /// The range of the `noqa` directive.
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Returns `true` if the given [`Rule`] is ignored at the specified `lineno`.
pub(crate) fn rule_is_ignored(
    code: Rule,
    offset: TextSize,
    noqa_line_for: &NoqaMapping,
    comment_ranges: &CommentRanges,
    locator: &Locator,
) -> bool {
    let offset = noqa_line_for.resolve(offset);
    let line_range = locator.line_range(offset);
    let &[comment_range] = comment_ranges.comments_in_range(line_range) else {
        return false;
    };
    match lex_inline_noqa(comment_range, locator.contents()) {
        Ok(Some(NoqaLexerOutput {
            directive: Directive::All(_),
            ..
        })) => true,
        Ok(Some(NoqaLexerOutput {
            directive: Directive::Codes(codes),
            ..
        })) => codes.includes(&code.noqa_code()),
        _ => false,
    }
}

/// A summary of the file-level exemption as extracted from [`FileNoqaDirectives`].
#[derive(Debug)]
pub(crate) enum FileExemption {
    /// The file is exempt from all rules.
    All(Vec<Rule>),
    /// The file is exempt from the given rules.
    Codes(Vec<Rule>),
}

impl FileExemption {
    /// Returns `true` if the file is exempt from the given rule, as identified by its noqa code.
    pub(crate) fn contains_secondary_code(&self, needle: &SecondaryCode) -> bool {
        match self {
            FileExemption::All(_) => true,
            FileExemption::Codes(codes) => codes.iter().any(|code| *needle == code.noqa_code()),
        }
    }

    /// Returns `true` if the file is exempt from the given rule.
    pub(crate) fn includes(&self, needle: Rule) -> bool {
        match self {
            FileExemption::All(_) => true,
            FileExemption::Codes(codes) => codes.contains(&needle),
        }
    }

    /// Returns `true` if the file exemption lists the rule directly, rather than via a blanket
    /// exemption.
    pub(crate) fn enumerates(&self, needle: Rule) -> bool {
        let codes = match self {
            FileExemption::All(codes) => codes,
            FileExemption::Codes(codes) => codes,
        };
        codes.contains(&needle)
    }
}

impl<'a> From<&'a FileNoqaDirectives<'a>> for FileExemption {
    fn from(directives: &'a FileNoqaDirectives) -> Self {
        let codes = directives
            .lines()
            .iter()
            .flat_map(|line| &line.matches)
            .copied()
            .collect();
        if directives
            .lines()
            .iter()
            .any(|line| matches!(line.parsed_file_exemption, Directive::All(_)))
        {
            FileExemption::All(codes)
        } else {
            FileExemption::Codes(codes)
        }
    }
}

/// The directive for a file-level exemption (e.g., `# ruff: noqa`) from an individual line.
#[derive(Debug)]
pub(crate) struct FileNoqaDirectiveLine<'a> {
    /// The range of the text line for which the noqa directive applies.
    pub(crate) range: TextRange,
    /// The blanket noqa directive.
    pub(crate) parsed_file_exemption: Directive<'a>,
    /// The codes that are ignored by the parsed exemptions.
    pub(crate) matches: Vec<Rule>,
}

impl Ranged for FileNoqaDirectiveLine<'_> {
    /// The range of the `noqa` directive.
    fn range(&self) -> TextRange {
        self.range
    }
}

/// All file-level exemptions (e.g., `# ruff: noqa`) from a given Python file.
#[derive(Debug)]
pub(crate) struct FileNoqaDirectives<'a>(Vec<FileNoqaDirectiveLine<'a>>);

impl<'a> FileNoqaDirectives<'a> {
    /// Extract the [`FileNoqaDirectives`] for a given Python source file, enumerating any rules
    /// that are globally ignored within the file.
    pub(crate) fn extract(
        locator: &Locator<'a>,
        comment_ranges: &CommentRanges,
        external: &[String],
        path: &Path,
    ) -> Self {
        let mut lines = vec![];

        for range in comment_ranges {
            let lexed = lex_file_exemption(range, locator.contents());
            match lexed {
                Ok(Some(NoqaLexerOutput {
                    warnings,
                    directive,
                })) => {
                    let no_indentation_at_offset =
                        indentation_at_offset(range.start(), locator.contents()).is_none();
                    if !warnings.is_empty() || no_indentation_at_offset {
                        #[expect(deprecated)]
                        let line = locator.compute_line_index(range.start());
                        let path_display = relativize_path(path);

                        for warning in warnings {
                            warn!(
                                "Missing or joined rule code(s) at {path_display}:{line}: {warning}"
                            );
                        }

                        if no_indentation_at_offset {
                            warn!(
                                "Unexpected `# ruff: noqa` directive at {path_display}:{line}. File-level suppression comments must appear on their own line. For line-level suppression, omit the `ruff:` prefix."
                            );
                            continue;
                        }
                    }

                    let matches = match &directive {
                        Directive::All(_) => {
                            vec![]
                        }
                        Directive::Codes(codes) => {
                            codes
                                .iter()
                                .filter_map(|code| {
                                    let code = code.as_str();
                                    // Ignore externally-defined rules.
                                    if external.iter().any(|external| code.starts_with(external)) {
                                        return None;
                                    }

                                    Rule::from_code(get_redirect_target(code).unwrap_or(code)).ok()
                                })
                                .collect()
                        }
                    };

                    lines.push(FileNoqaDirectiveLine {
                        range,
                        parsed_file_exemption: directive,
                        matches,
                    });
                }
                Err(err) => {
                    #[expect(deprecated)]
                    let line = locator.compute_line_index(range.start());
                    let path_display = relativize_path(path);
                    warn!("Invalid `# ruff: noqa` directive at {path_display}:{line}: {err}");
                }
                Ok(None) => {}
            }
        }
        Self(lines)
    }

    pub(crate) fn lines(&self) -> &[FileNoqaDirectiveLine<'_>] {
        &self.0
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Output of lexing a `noqa` directive.
#[derive(Debug)]
pub(crate) struct NoqaLexerOutput<'a> {
    warnings: Vec<LexicalWarning>,
    directive: Directive<'a>,
}

/// Lexes in-line `noqa` comment, e.g. `# noqa: F401`
fn lex_inline_noqa(
    comment_range: TextRange,
    source: &str,
) -> Result<Option<NoqaLexerOutput<'_>>, LexicalError> {
    let lexer = NoqaLexer::in_range(comment_range, source);
    lexer.lex_inline_noqa()
}

/// Return the range of the rule identifier at `offset` within `comment_range`, if it exists and is
/// part of a file-level or line-level `noqa` comment.
pub(crate) fn rule_identifier_range_at_offset(
    source: &str,
    comment_range: TextRange,
    offset: TextSize,
) -> Option<TextRange> {
    let Some(NoqaLexerOutput {
        directive: Directive::Codes(codes),
        ..
    }) = lex_inline_noqa(comment_range, source)
        .ok()
        .flatten()
        .or_else(|| lex_file_exemption(comment_range, source).ok().flatten())
    else {
        return None;
    };

    codes
        .iter()
        .map(Ranged::range)
        .find(|range| range.contains(offset))
}

/// Lexes file-level exemption comment, e.g. `# ruff: noqa: F401`
fn lex_file_exemption(
    comment_range: TextRange,
    source: &str,
) -> Result<Option<NoqaLexerOutput<'_>>, LexicalError> {
    let lexer = NoqaLexer::in_range(comment_range, source);
    lexer.lex_file_exemption()
}

pub(crate) fn lex_codes(text: &str) -> Result<Vec<Code<'_>>, LexicalError> {
    let mut lexer = NoqaLexer::in_range(TextRange::new(TextSize::new(0), text.text_len()), text);
    lexer.lex_codes()?;
    Ok(lexer.codes)
}

/// Lexer for `noqa` comment lines.
///
/// Assumed to be initialized with range or offset starting
/// at the `#` at the beginning of a `noqa` comment.
#[derive(Debug)]
struct NoqaLexer<'a> {
    /// Length between offset and end of comment range
    line_length: TextSize,
    /// Contains convenience methods for lexing
    cursor: Cursor<'a>,
    /// Byte offset of the start of putative `noqa` comment
    ///
    /// Note: This is updated in the course of lexing in the case
    /// where there are multiple comments in a comment range.
    /// Ex) `# comment # noqa: F401`
    /// start^         ^-- changed to here during lexing
    offset: TextSize,
    /// Non-fatal warnings collected during lexing
    warnings: Vec<LexicalWarning>,
    /// Tracks whether we are lexing in a context with a missing delimiter
    /// e.g. at `C` in `F401C402`.
    missing_delimiter: bool,
    /// Rule codes collected during lexing
    codes: Vec<Code<'a>>,
}

impl<'a> NoqaLexer<'a> {
    /// Initialize [`NoqaLexer`] in the given range of source text.
    fn in_range(range: TextRange, source: &'a str) -> Self {
        Self {
            line_length: range.len(),
            offset: range.start(),
            cursor: Cursor::new(&source[range]),
            warnings: Vec::new(),
            missing_delimiter: false,
            codes: Vec::new(),
        }
    }

    fn lex_inline_noqa(mut self) -> Result<Option<NoqaLexerOutput<'a>>, LexicalError> {
        while !self.cursor.is_eof() {
            // Skip over any leading content.
            self.cursor.eat_while(|c| c != '#');

            // This updates the offset and line length in the case of
            // multiple comments in a range, e.g.
            // `# First # Second # noqa`
            self.offset += self.cursor.token_len();
            self.line_length -= self.cursor.token_len();
            self.cursor.start_token();

            if !self.cursor.eat_char('#') {
                continue;
            }

            self.eat_whitespace();

            if !is_noqa_uncased(self.cursor.as_str()) {
                continue;
            }

            self.cursor.skip_bytes("noqa".len());

            return Ok(Some(self.lex_directive()?));
        }

        Ok(None)
    }

    fn lex_file_exemption(mut self) -> Result<Option<NoqaLexerOutput<'a>>, LexicalError> {
        while !self.cursor.is_eof() {
            // Skip over any leading content
            self.cursor.eat_while(|c| c != '#');

            // This updates the offset and line length in the case of
            // multiple comments in a range, e.g.
            // # First # Second # noqa
            self.offset += self.cursor.token_len();
            self.line_length -= self.cursor.token_len();
            self.cursor.start_token();

            // The remaining logic implements a simple regex

            // `#\s*(?:ruff|flake8)\s*:\s*(?i)noqa`
            //  ^
            if !self.cursor.eat_char('#') {
                continue;
            }

            // `#\s*(?:ruff|flake8)\s*:\s*(?i)noqa`
            //   ^^^
            self.eat_whitespace();

            // `#\s*(?:ruff|flake8)\s*:\s*(?i)noqa`
            //       ^^^^^^^^^^^^^
            if self.cursor.as_str().starts_with("ruff") {
                self.cursor.skip_bytes("ruff".len());
            } else if self.cursor.as_str().starts_with("flake8") {
                self.cursor.skip_bytes("flake8".len());
            } else {
                continue;
            }

            // `#\s*(?:ruff|flake8)\s*:\s*(?i)noqa`
            //                     ^^^
            self.eat_whitespace();

            // `#\s*(?:ruff|flake8)\s*:\s*(?i)noqa`
            //                        ^
            if !self.cursor.eat_char(':') {
                continue;
            }

            // `#\s*(?:ruff|flake8)\s*:\s*(?i)noqa`
            //                         ^^^
            self.eat_whitespace();

            // `#\s*(?:ruff|flake8)\s*:\s*(?i)noqa`
            //                             ^^^^^^^
            if !is_noqa_uncased(self.cursor.as_str()) {
                continue;
            }
            self.cursor.skip_bytes("noqa".len());

            return Ok(Some(self.lex_directive()?));
        }
        Ok(None)
    }

    /// Collect codes in `noqa` comment.
    fn lex_directive(mut self) -> Result<NoqaLexerOutput<'a>, LexicalError> {
        let range = TextRange::at(self.offset, self.position());

        match self.cursor.bump() {
            // End of comment
            // Ex) # noqa# A comment
            None | Some('#') => {
                return Ok(NoqaLexerOutput {
                    warnings: self.warnings,
                    directive: Directive::All(All { range }),
                });
            }
            // Ex) # noqa: F401,F842
            Some(':') => self.lex_codes()?,
            // Ex) # noqa A comment
            // Ex) # noqa   : F401
            Some(c) if c.is_whitespace() => {
                self.eat_whitespace();
                match self.cursor.bump() {
                    Some(':') => self.lex_codes()?,
                    _ => {
                        return Ok(NoqaLexerOutput {
                            warnings: self.warnings,
                            directive: Directive::All(All { range }),
                        });
                    }
                }
            }
            // Ex) #noqaA comment
            // Ex) #noqaF401
            _ => {
                return Err(LexicalError::InvalidSuffix);
            }
        }

        let Some(last) = self.codes.last() else {
            return Err(LexicalError::MissingCodes);
        };

        Ok(NoqaLexerOutput {
            warnings: self.warnings,
            directive: Directive::Codes(Codes {
                range: TextRange::new(self.offset, last.range.end()),
                codes: self.codes,
            }),
        })
    }

    fn lex_codes(&mut self) -> Result<(), LexicalError> {
        // SAFETY: Every call to `lex_code` advances the cursor at least once.
        while !self.cursor.is_eof() {
            self.lex_code()?;
        }
        Ok(())
    }

    fn lex_code(&mut self) -> Result<(), LexicalError> {
        self.cursor.start_token();
        self.eat_whitespace();

        // Ex) # noqa: F401,    ,F841
        //                  ^^^^
        if self.cursor.first() == ',' {
            self.warnings.push(LexicalWarning::MissingItem(
                self.token_range().add(self.offset),
            ));
            self.cursor.eat_char(',');
            return Ok(());
        }

        let before_code = self.cursor.as_str();
        // Reset start of token so it does not include whitespace
        self.cursor.start_token();
        match self.cursor.bump() {
            // Ex) # noqa: F401
            //             ^
            Some(c) if c.is_ascii_uppercase() => {
                self.cursor.eat_while(|chr| chr.is_ascii_uppercase());
                if !self.cursor.eat_if(|c| c.is_ascii_digit()) {
                    // Fail hard if we're already attempting
                    // to lex squashed codes, e.g. `F401Fabc`
                    if self.missing_delimiter {
                        return Err(LexicalError::InvalidCodeSuffix);
                    }
                    // Otherwise we've reached the first invalid code,
                    // so it could be a comment and we just stop parsing.
                    // Ex) #noqa: F401 A comment
                    //       we're here^
                    self.cursor.skip_bytes(self.cursor.as_str().len());
                    return Ok(());
                }
                self.cursor.eat_while(|chr| chr.is_ascii_digit());

                let code = &before_code[..self.cursor.token_len().to_usize()];

                self.push_code(code);

                if self.cursor.is_eof() {
                    return Ok(());
                }

                self.missing_delimiter = match self.cursor.first() {
                    ',' => {
                        self.cursor.eat_char(',');
                        false
                    }

                    // Whitespace is an allowed delimiter or the end of the `noqa`.
                    c if c.is_whitespace() => false,

                    // e.g. #noqa:F401F842
                    //                ^
                    // Push a warning and update the `missing_delimiter`
                    // state but don't consume the character since it's
                    // part of the next code.
                    c if c.is_ascii_uppercase() => {
                        self.warnings
                            .push(LexicalWarning::MissingDelimiter(TextRange::empty(
                                self.offset + self.position(),
                            )));
                        true
                    }
                    // Start of a new comment
                    // e.g. #noqa: F401#A comment
                    //                 ^
                    '#' => false,
                    _ => return Err(LexicalError::InvalidCodeSuffix),
                };
            }
            Some(_) => {
                // The first time we hit an evidently invalid code,
                // it's probably a trailing comment. So stop lexing,
                // but don't push an error.
                // Ex)
                // # noqa: F401 we import this for a reason
                //              ^----- stop lexing but no error
                self.cursor.skip_bytes(self.cursor.as_str().len());
            }
            None => {}
        }
        Ok(())
    }

    /// Push current token to stack of [`Code`] objects
    fn push_code(&mut self, code: &'a str) {
        self.codes.push(Code {
            code,
            range: self.token_range().add(self.offset),
        });
    }

    /// Consume whitespace
    #[inline]
    fn eat_whitespace(&mut self) {
        self.cursor.eat_while(char::is_whitespace);
    }

    /// Current token range relative to offset
    #[inline]
    fn token_range(&self) -> TextRange {
        let end = self.position();
        let len = self.cursor.token_len();

        TextRange::at(end - len, len)
    }

    /// Retrieves the current position of the cursor within the line.
    #[inline]
    fn position(&self) -> TextSize {
        self.line_length - self.cursor.text_len()
    }
}

/// Helper to check if "noqa" (case-insensitive) appears at the given position
#[inline]
fn is_noqa_uncased(text: &str) -> bool {
    matches!(
        text.as_bytes(),
        [b'n' | b'N', b'o' | b'O', b'q' | b'Q', b'a' | b'A', ..]
    )
}

/// Indicates recoverable error encountered while lexing with [`NoqaLexer`]
#[derive(Debug, Clone, Copy)]
enum LexicalWarning {
    MissingItem(TextRange),
    MissingDelimiter(TextRange),
}

impl Display for LexicalWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingItem(_) => f.write_str("expected rule code between commas"),
            Self::MissingDelimiter(_) => {
                f.write_str("expected comma or space delimiter between codes")
            }
        }
    }
}

impl Ranged for LexicalWarning {
    fn range(&self) -> TextRange {
        match *self {
            LexicalWarning::MissingItem(text_range) => text_range,
            LexicalWarning::MissingDelimiter(text_range) => text_range,
        }
    }
}

/// Fatal error occurring while lexing a `noqa` comment as in [`NoqaLexer`]
#[derive(Debug)]
pub(crate) enum LexicalError {
    /// The `noqa` directive was missing valid codes (e.g., `# noqa: unused-import` instead of `# noqa: F401`).
    MissingCodes,
    /// The `noqa` directive used an invalid suffix (e.g., `# noqa; F401` instead of `# noqa: F401`).
    InvalidSuffix,
    /// The `noqa` code matched the regex "[A-Z]+[0-9]+" with text remaining
    /// (e.g. `# noqa: F401abc`)
    InvalidCodeSuffix,
}

impl Display for LexicalError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LexicalError::MissingCodes => fmt.write_str("expected a comma-separated list of codes (e.g., `# noqa: F401, F841`)."),
            LexicalError::InvalidSuffix => {
                fmt.write_str("expected `:` followed by a comma-separated list of codes (e.g., `# noqa: F401, F841`).")
            }
            LexicalError::InvalidCodeSuffix => {
                fmt.write_str("expected code to consist of uppercase letters followed by digits only (e.g. `F401`)")
            }

        }
    }
}

impl Error for LexicalError {}

/// The kind of suppression comment to be added to suppress a diagnostic.
#[derive(Copy, Clone, Debug)]
pub enum SuppressionKind {
    /// A `noqa` comment
    Noqa,
    /// A `ruff:ignore` comment
    Ignore,
}

/// Adds suppression comments to suppress all messages of a file.
#[expect(clippy::too_many_arguments)]
pub(crate) fn add_suppression(
    path: &Path,
    diagnostics: &[Diagnostic],
    locator: &Locator,
    comment_ranges: &CommentRanges,
    external: &[String],
    noqa_line_for: &NoqaMapping,
    line_ending: LineEnding,
    reason: Option<&str>,
    suppressions: &Suppressions,
    suppression_kind: SuppressionKind,
) -> Result<usize> {
    let (count, output) = add_suppression_inner(
        path,
        diagnostics,
        locator,
        comment_ranges,
        external,
        noqa_line_for,
        line_ending,
        reason,
        suppressions,
        suppression_kind,
    );

    fs::write(path, output)?;
    Ok(count)
}

#[expect(clippy::too_many_arguments)]
fn add_suppression_inner(
    path: &Path,
    diagnostics: &[Diagnostic],
    locator: &Locator,
    comment_ranges: &CommentRanges,
    external: &[String],
    noqa_line_for: &NoqaMapping,
    line_ending: LineEnding,
    reason: Option<&str>,
    suppressions: &Suppressions,
    suppression_kind: SuppressionKind,
) -> (usize, String) {
    let mut count = 0;

    // Whether the file is exempted from all checks.
    let directives = FileNoqaDirectives::extract(locator, comment_ranges, external, path);
    let exemption = FileExemption::from(&directives);

    let directives = NoqaDirectives::from_commented_ranges(comment_ranges, path, locator);

    let comments = find_suppression_comments(
        diagnostics,
        locator,
        &exemption,
        &directives,
        noqa_line_for,
        suppressions,
        suppression_kind,
    );

    let edits =
        build_suppression_edits_by_line(comments, locator, line_ending, reason, suppression_kind);

    let contents = locator.contents();

    let mut output = String::with_capacity(contents.len());
    let mut last_append = TextSize::default();

    for (_, edit) in edits {
        output.push_str(&contents[TextRange::new(last_append, edit.start())]);

        edit.write(&mut output);

        count += 1;

        last_append = edit.end();
    }

    output.push_str(&contents[TextRange::new(last_append, TextSize::of(contents))]);

    (count, output)
}

fn build_suppression_edits_by_diagnostic(
    comments: Vec<Option<SuppressionComment>>,
    locator: &Locator,
    line_ending: LineEnding,
    reason: Option<&str>,
    suppression_kind: SuppressionKind,
) -> Vec<Option<Edit>> {
    let mut edits = Vec::default();
    for comment in comments {
        match comment {
            Some(comment) => {
                let suppression_edit = generate_suppression_edit(
                    comment.directive,
                    comment.line,
                    FxHashSet::from_iter([comment.identifier]),
                    locator,
                    line_ending,
                    reason,
                    suppression_kind,
                );
                edits.push(Some(suppression_edit.into_edit()));
            }
            None => edits.push(None),
        }
    }
    edits
}

fn build_suppression_edits_by_line<'a>(
    comments: Vec<Option<SuppressionComment<'a>>>,
    locator: &Locator<'a>,
    line_ending: LineEnding,
    reason: Option<&'a str>,
    suppression_kind: SuppressionKind,
) -> BTreeMap<TextSize, SuppressionEdit<'a>> {
    let mut comments_by_line = BTreeMap::default();
    for comment in comments.into_iter().flatten() {
        comments_by_line
            .entry(comment.line)
            .or_insert_with(Vec::default)
            .push(comment);
    }
    let mut edits = BTreeMap::default();
    for (offset, matches) in comments_by_line {
        let Some(first_match) = matches.first() else {
            continue;
        };
        let directive = first_match.directive;
        let suppression_edit = generate_suppression_edit(
            directive,
            offset,
            matches
                .into_iter()
                .map(|comment| comment.identifier)
                .collect(),
            locator,
            line_ending,
            reason,
            suppression_kind,
        );
        edits.insert(offset, suppression_edit);
    }
    edits
}

struct SuppressionComment<'a> {
    line: TextSize,
    identifier: &'a str,
    directive: Option<ExistingDirective<'a>>,
}

#[derive(Copy, Clone)]
enum ExistingDirective<'a> {
    Noqa(&'a Codes<'a>),
    Ignore(&'a suppression::SuppressionComment),
}

impl Ranged for ExistingDirective<'_> {
    fn range(&self) -> TextRange {
        match self {
            ExistingDirective::Noqa(codes) => codes.range(),
            ExistingDirective::Ignore(comment) => comment.range(),
        }
    }
}

fn find_suppression_comments<'a>(
    diagnostics: &'a [Diagnostic],
    locator: &'a Locator,
    exemption: &'a FileExemption,
    directives: &'a NoqaDirectives,
    noqa_line_for: &NoqaMapping,
    suppressions: &'a Suppressions,
    suppression_kind: SuppressionKind,
) -> Vec<Option<SuppressionComment<'a>>> {
    // List of suppression comments, ordered to match up with `messages`
    let mut comments_by_line: Vec<Option<SuppressionComment<'a>>> = vec![];

    // Mark any non-ignored diagnostics.
    for message in diagnostics {
        let Some(code) = message.secondary_code() else {
            comments_by_line.push(None);
            continue;
        };

        if exemption.contains_secondary_code(code) {
            comments_by_line.push(None);
            continue;
        }

        // Apply ranged suppressions next
        if suppressions.check_diagnostic(message) {
            comments_by_line.push(None);
            continue;
        }

        // Is the violation ignored by a `noqa` directive on the parent line?
        if let Some(parent) = message.parent() {
            if let Some(directive_line) =
                directives.find_line_with_directive(noqa_line_for.resolve(parent))
            {
                match &directive_line.directive {
                    Directive::All(_) => {
                        comments_by_line.push(None);
                        continue;
                    }
                    Directive::Codes(codes) => {
                        if codes.includes(code) {
                            comments_by_line.push(None);
                            continue;
                        }
                    }
                }
            }
        }

        let noqa_offset = message
            .range()
            .map(|range| noqa_line_for.resolve(range.start()))
            .unwrap_or_default();

        // Or ignored by a `noqa` directive on the diagnostic's line itself?
        let existing_noqa =
            if let Some(directive_line) = directives.find_line_with_directive(noqa_offset) {
                match &directive_line.directive {
                    Directive::All(_) => {
                        comments_by_line.push(None);
                        continue;
                    }
                    Directive::Codes(codes) => {
                        if codes.includes(code) {
                            comments_by_line.push(None);
                            continue;
                        }
                        Some(ExistingDirective::Noqa(codes))
                    }
                }
            } else {
                None
            };

        // Reuse an existing directive that matches the requested suppression style.
        let directive = match suppression_kind {
            SuppressionKind::Noqa => existing_noqa,
            SuppressionKind::Ignore => suppressions
                .find_applicable_ignore(message)
                .map(ExistingDirective::Ignore),
        };

        let identifier = match suppression_kind {
            SuppressionKind::Noqa => code.as_str(),
            SuppressionKind::Ignore => message.name(),
        };

        comments_by_line.push(Some(SuppressionComment {
            line: locator.line_start(directive.map_or(noqa_offset, |directive| directive.start())),
            identifier,
            directive,
        }));
    }

    comments_by_line
}

struct SuppressionEdit<'a> {
    edit_range: TextRange,
    noqa_codes: FxHashSet<&'a str>,
    existing_codes: Vec<&'a str>,
    line_ending: LineEnding,
    reason: Option<&'a str>,
    blank_line: bool,
    suppression_kind: SuppressionKind,
}

impl SuppressionEdit<'_> {
    fn into_edit(self) -> Edit {
        let mut edit_content = String::new();
        self.write(&mut edit_content);

        Edit::range_replacement(edit_content, self.edit_range)
    }

    fn write(&self, writer: &mut impl std::fmt::Write) {
        if !self.blank_line {
            write!(writer, "  ").unwrap();
        }
        match self.suppression_kind {
            SuppressionKind::Noqa => write!(writer, "# noqa: ").unwrap(),
            SuppressionKind::Ignore => write!(writer, "# ruff:ignore[").unwrap(),
        }
        push_codes(
            writer,
            self.noqa_codes
                .iter()
                .chain(self.existing_codes.iter())
                .sorted_unstable(),
        );
        match self.suppression_kind {
            SuppressionKind::Noqa => {}
            SuppressionKind::Ignore => write!(writer, "]").unwrap(),
        }
        if let Some(reason) = self.reason {
            write!(writer, " {reason}").unwrap();
        }
        write!(writer, "{}", self.line_ending.as_str()).unwrap();
    }
}

impl Ranged for SuppressionEdit<'_> {
    fn range(&self) -> TextRange {
        self.edit_range
    }
}

fn generate_suppression_edit<'a>(
    directive: Option<ExistingDirective<'a>>,
    offset: TextSize,
    noqa_codes: FxHashSet<&'a str>,
    locator: &Locator<'a>,
    line_ending: LineEnding,
    reason: Option<&'a str>,
    suppression_kind: SuppressionKind,
) -> SuppressionEdit<'a> {
    let line_range = locator.full_line_range(offset);

    let edit_range;
    let mut existing_codes = Vec::new();
    let blank_line;

    match (directive, suppression_kind) {
        // Add additional rule codes to an existing `noqa` comment.
        (Some(ExistingDirective::Noqa(codes)), SuppressionKind::Noqa) => {
            (edit_range, blank_line) = suppression_edit_range(locator, line_range, codes.start());
            existing_codes.extend(codes.iter().map(Code::as_str));
        }
        // Add additional rule names to an existing `ruff:ignore` comment.
        (Some(ExistingDirective::Ignore(comment)), SuppressionKind::Ignore) => {
            (edit_range, blank_line) = suppression_edit_range(locator, line_range, comment.start());
            existing_codes.extend(comment.codes_as_str(locator.contents()));
        }
        // Add a new comment when one doesn't exist, or the "wrong" kind is present. In either case,
        // append a new comment.
        (None | Some(_), _) => {
            let trimmed_line = locator.slice(line_range).trim_end();
            blank_line = trimmed_line.trim_whitespace_start().is_empty();
            edit_range = TextRange::new(TextSize::of(trimmed_line), line_range.len()) + offset;
        }
    }

    SuppressionEdit {
        edit_range,
        noqa_codes,
        existing_codes,
        line_ending,
        reason,
        blank_line,
        suppression_kind,
    }
}

/// Returns the range to replace when updating an existing suppression directive, along with whether
/// the directive is on an otherwise blank line.
fn suppression_edit_range(
    locator: &Locator,
    line_range: TextRange,
    directive_start: TextSize,
) -> (TextRange, bool) {
    let prefix = locator.slice(TextRange::new(line_range.start(), directive_start));
    if prefix.trim_whitespace().is_empty() {
        (TextRange::new(directive_start, line_range.end()), true)
    } else {
        let edit_start = line_range.start() + prefix.trim_end().text_len();
        (TextRange::new(edit_start, line_range.end()), false)
    }
}

fn push_codes<I: Display>(writer: &mut dyn std::fmt::Write, codes: impl Iterator<Item = I>) {
    let mut first = true;
    for code in codes {
        if !first {
            write!(writer, ", ").unwrap();
        }
        write!(writer, "{code}").unwrap();
        first = false;
    }
}

#[derive(Debug)]
pub(crate) struct NoqaDirectiveLine<'a> {
    /// The range of the text line for which the noqa directive applies.
    pub(crate) range: TextRange,
    /// The noqa directive.
    pub(crate) directive: Directive<'a>,
    /// The codes that are ignored by the directive.
    pub(crate) matches: Vec<Rule>,
    /// Whether the directive applies to `range.end`.
    pub(crate) includes_end: bool,
}

impl Ranged for NoqaDirectiveLine<'_> {
    /// The range of the `noqa` directive.
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Debug, Default)]
pub(crate) struct NoqaDirectives<'a> {
    inner: Vec<NoqaDirectiveLine<'a>>,
}

impl<'a> NoqaDirectives<'a> {
    pub(crate) fn from_commented_ranges(
        comment_ranges: &CommentRanges,
        path: &Path,
        locator: &'a Locator<'a>,
    ) -> Self {
        let mut directives = Vec::new();

        for range in comment_ranges {
            let lexed = lex_inline_noqa(range, locator.contents());

            match lexed {
                Ok(Some(NoqaLexerOutput {
                    warnings,
                    directive,
                })) => {
                    if !warnings.is_empty() {
                        #[expect(deprecated)]
                        let line = locator.compute_line_index(range.start());
                        let path_display = relativize_path(path);
                        for warning in warnings {
                            warn!(
                                "Missing or joined rule code(s) at {path_display}:{line}: {warning}"
                            );
                        }
                    }
                    // noqa comments are guaranteed to be single line.
                    let range = locator.line_range(range.start());
                    directives.push(NoqaDirectiveLine {
                        range,
                        directive,
                        matches: Vec::new(),
                        includes_end: range.end() == locator.contents().text_len(),
                    });
                }
                Err(err) => {
                    #[expect(deprecated)]
                    let line = locator.compute_line_index(range.start());
                    let path_display = relativize_path(path);
                    warn!("Invalid `# noqa` directive on {path_display}:{line}: {err}");
                }
                Ok(None) => {}
            }
        }

        Self { inner: directives }
    }

    pub(crate) fn find_line_with_directive(
        &self,
        offset: TextSize,
    ) -> Option<&NoqaDirectiveLine<'_>> {
        self.find_line_index(offset).map(|index| &self.inner[index])
    }

    pub(crate) fn find_line_with_directive_mut(
        &mut self,
        offset: TextSize,
    ) -> Option<&mut NoqaDirectiveLine<'a>> {
        if let Some(index) = self.find_line_index(offset) {
            Some(&mut self.inner[index])
        } else {
            None
        }
    }

    fn find_line_index(&self, offset: TextSize) -> Option<usize> {
        self.inner
            .binary_search_by(|directive| {
                if directive.range.end() < offset {
                    std::cmp::Ordering::Less
                } else if directive.range.start() > offset {
                    std::cmp::Ordering::Greater
                }
                // At this point, end >= offset, start <= offset
                else if !directive.includes_end && directive.range.end() == offset {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()
    }

    pub(crate) fn lines(&self) -> &[NoqaDirectiveLine<'_>] {
        &self.inner
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Remaps offsets falling into one of the ranges to instead check for a noqa comment on the
/// line specified by the offset.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct NoqaMapping {
    ranges: Vec<TextRange>,
}

impl NoqaMapping {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            ranges: Vec::with_capacity(capacity),
        }
    }

    /// Returns the re-mapped position or `position` if no mapping exists.
    pub(crate) fn resolve(&self, offset: TextSize) -> TextSize {
        let index = self.ranges.binary_search_by(|range| {
            if range.end() < offset {
                std::cmp::Ordering::Less
            } else if range.contains(offset) {
                std::cmp::Ordering::Equal
            } else {
                std::cmp::Ordering::Greater
            }
        });

        if let Ok(index) = index {
            self.ranges[index].end()
        } else {
            offset
        }
    }

    pub(crate) fn push_mapping(&mut self, range: TextRange) {
        if let Some(last_range) = self.ranges.last_mut() {
            // Strictly sorted insertion
            if last_range.end() < range.start() {
                // OK
            } else if range.end() < last_range.start() {
                // Incoming range is strictly before the last range which violates
                // the function's contract.
                panic!("Ranges must be inserted in sorted order")
            } else {
                // Here, it's guaranteed that `last_range` and `range` overlap
                // in some way. We want to merge them into a single range.
                *last_range = last_range.cover(range);
                return;
            }
        }

        self.ranges.push(range);
    }
}

impl FromIterator<TextRange> for NoqaMapping {
    fn from_iter<T: IntoIterator<Item = TextRange>>(iter: T) -> Self {
        let mut mappings = NoqaMapping::default();

        for range in iter {
            mappings.push_mapping(range);
        }

        mappings
    }
}

#[cfg(test)]
mod tests {

    use std::fmt::Write;
    use std::path::Path;

    use anyhow::{Result, anyhow};
    use insta::{assert_debug_snapshot, assert_snapshot};

    use ruff_diagnostics::SourceMap;
    use ruff_python_ast::{PySourceType, SourceType};
    use ruff_python_codegen::Stylist;
    use ruff_python_index::Indexer;
    use ruff_python_trivia::{CommentRanges, textwrap::dedent};
    use ruff_source_file::{LineEnding, SourceFileBuilder};
    use ruff_text_size::{TextLen, TextRange, TextSize};

    use crate::codes::Rule;
    use crate::linter::{check_path, parse_unchecked_source};
    use crate::noqa::{
        Directive, LexicalError, NoqaLexerOutput, NoqaMapping, SuppressionKind,
        add_suppression_inner, lex_codes, lex_file_exemption, lex_inline_noqa,
    };
    use crate::rules::pycodestyle::rules::{AmbiguousVariableName, UselessSemicolon};
    use crate::rules::pyflakes::rules::UnusedVariable;
    use crate::rules::pyupgrade::rules::PrintfStringFormatting;
    use crate::settings::{LinterSettings, flags};
    use crate::source_kind::SourceKind;
    use crate::suppression::Suppressions;
    use crate::test::{print_messages, test_contents};
    use crate::{Edit, Violation, directives};
    use crate::{Locator, generate_suppression_edits};

    fn add_suppressions(
        path: &Path,
        source_kind: &SourceKind,
        settings: &LinterSettings,
        suppression_kind: SuppressionKind,
    ) -> (usize, String) {
        let source_type = source_kind.py_source_type();
        let target_version = settings.resolve_target_version(path);
        let parsed =
            parse_unchecked_source(source_kind, source_type, target_version.parser_version());
        let locator = Locator::new(source_kind.source_code());
        let stylist = Stylist::from_tokens(parsed.tokens(), locator.contents());
        let indexer = Indexer::from_tokens(parsed.tokens(), locator.contents());
        let directives = directives::extract_directives(
            parsed.tokens(),
            directives::Flags::from_settings(settings),
            &locator,
            &indexer,
        );
        let suppressions =
            Suppressions::from_tokens(locator.contents(), parsed.tokens(), &indexer, settings);
        let diagnostics = check_path(
            path,
            None,
            &locator,
            &stylist,
            &indexer,
            &directives,
            settings,
            flags::Noqa::Disabled,
            source_kind,
            source_type,
            &parsed,
            target_version,
            &suppressions,
        );

        add_suppression_inner(
            path,
            &diagnostics,
            &locator,
            indexer.comment_ranges(),
            &settings.external,
            &directives.noqa_line_for,
            stylist.line_ending(),
            None,
            &suppressions,
            suppression_kind,
        )
    }

    #[track_caller]
    fn add_suppressions_in(
        source: &str,
        suppression_kind: SuppressionKind,
        settings: &LinterSettings,
    ) -> Result<String> {
        let path = Path::new("<filename>");
        let source_map = SourceMap::default();
        let source_kind = SourceKind::from_source_code(
            dedent(source).trim().to_string(),
            SourceType::Python(PySourceType::Python),
        )?
        .ok_or_else(|| anyhow!("test file should be Python"))?;

        let (count, fixed) = add_suppressions(path, &source_kind, settings, suppression_kind);
        let plural = if count == 1 { "" } else { "s" };
        let mut output = String::new();
        writeln!(
            output,
            "Added {count} suppression{plural}\n\n## Fixed source\n\n```py\n{fixed}\n```\n"
        )?;

        let source_kind = source_kind.updated(fixed, &source_map);
        let (second_count, fixed) =
            add_suppressions(path, &source_kind, settings, suppression_kind);
        if second_count > 0 {
            writeln!(
                output,
                "## Additional suppressions added on a second pass: {second_count}\n\n```py\n{fixed}\n```\n"
            )?;
        }

        let source_kind = source_kind.updated(fixed, &source_map);
        let (diagnostics, _) = test_contents(&source_kind, path, settings);
        if !diagnostics.is_empty() {
            writeln!(
                output,
                "## New diagnostics after re-checking file\n\n{diagnostics}\n",
                diagnostics = print_messages(&diagnostics)
            )?;
        }

        Ok(output)
    }

    fn assert_lexed_ranges_match_slices(
        directive: Result<Option<NoqaLexerOutput>, LexicalError>,
        source: &str,
    ) {
        if let Ok(Some(NoqaLexerOutput {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            for code in codes.iter() {
                assert_eq!(&source[code.range], code.code);
            }
        }
    }

    #[test]
    fn noqa_lex_codes() {
        let source = " F401,,F402F403 # and so on";
        assert_debug_snapshot!(lex_codes(source), @r#"
        Ok(
            [
                Code {
                    code: "F401",
                    range: 1..5,
                },
                Code {
                    code: "F402",
                    range: 7..11,
                },
                Code {
                    code: "F403",
                    range: 11..15,
                },
            ],
        )
        "#);
    }

    #[test]
    fn noqa_all() {
        let source = "# noqa";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..6,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_no_code() {
        let source = "# noqa:";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_no_code_invalid_suffix() {
        let source = "# noqa: foo";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_no_code_trailing_content() {
        let source = "# noqa:  # Foo";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn malformed_code_1() {
        let source = "# noqa: F";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn malformed_code_2() {
        let source = "# noqa: RUF001, F";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..14,
                            codes: [
                                Code {
                                    code: "RUF001",
                                    range: 8..14,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn malformed_code_3() {
        let source = "# noqa: RUF001F";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            InvalidCodeSuffix,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_code() {
        let source = "# noqa: F401";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..12,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 8..12,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_codes() {
        let source = "# noqa: F401, F841";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..18,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 8..12,
                                },
                                Code {
                                    code: "F841",
                                    range: 14..18,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_all_case_insensitive() {
        let source = "# NOQA";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..6,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_code_case_insensitive() {
        let source = "# NOQA: F401";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..12,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 8..12,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_codes_case_insensitive() {
        let source = "# NOQA: F401, F841";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..18,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 8..12,
                                },
                                Code {
                                    code: "F841",
                                    range: 14..18,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_leading_space() {
        let source = "#   # noqa: F401";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 4..16,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 12..16,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_trailing_space() {
        let source = "# noqa: F401   #";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..12,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 8..12,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_all_no_space() {
        let source = "#noqa";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..5,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_code_no_space() {
        let source = "#noqa:F401";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..10,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 6..10,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_codes_no_space() {
        let source = "#noqa:F401,F841";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..15,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 6..10,
                                },
                                Code {
                                    code: "F841",
                                    range: 11..15,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_all_multi_space() {
        let source = "#  noqa";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..7,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_code_multi_space() {
        let source = "#  noqa: F401";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..13,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 9..13,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_codes_multi_space() {
        let source = "#  noqa: F401,  F841";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..20,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 9..13,
                                },
                                Code {
                                    code: "F841",
                                    range: 16..20,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_code_leading_hashes() {
        let source = "###noqa: F401";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 2..13,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 9..13,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_code_leading_hashes_with_spaces() {
        let source = "#  #  # noqa: F401";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 6..18,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 14..18,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_all_leading_comment() {
        let source = "# Some comment describing the noqa # noqa";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 35..41,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_code_leading_comment() {
        let source = "# Some comment describing the noqa # noqa: F401";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 35..47,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 43..47,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_codes_leading_comment() {
        let source = "# Some comment describing the noqa # noqa: F401, F841";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 35..53,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 43..47,
                                },
                                Code {
                                    code: "F841",
                                    range: 49..53,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_all_trailing_comment() {
        let source = "# noqa # Some comment describing the noqa";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..6,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_code_trailing_comment() {
        let source = "# noqa: F401 # Some comment describing the noqa";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..12,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 8..12,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_codes_trailing_comment() {
        let source = "# noqa: F401, F841 # Some comment describing the noqa";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..18,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 8..12,
                                },
                                Code {
                                    code: "F841",
                                    range: 14..18,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_invalid_codes() {
        let source = "# noqa: unused-import, F401, some other code";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_squashed_codes() {
        let source = "# noqa: F401F841";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [
                        MissingDelimiter(
                            12..12,
                        ),
                    ],
                    directive: Codes(
                        Codes {
                            range: 0..16,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 8..12,
                                },
                                Code {
                                    code: "F841",
                                    range: 12..16,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_empty_comma() {
        let source = "# noqa: F401,,F841";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [
                        MissingItem(
                            13..13,
                        ),
                    ],
                    directive: Codes(
                        Codes {
                            range: 0..18,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 8..12,
                                },
                                Code {
                                    code: "F841",
                                    range: 14..18,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_empty_comma_space() {
        let source = "# noqa: F401, ,F841";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [
                        MissingItem(
                            13..14,
                        ),
                    ],
                    directive: Codes(
                        Codes {
                            range: 0..19,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 8..12,
                                },
                                Code {
                                    code: "F841",
                                    range: 15..19,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_non_code() {
        let source = "# noqa: F401 We're ignoring an import";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..12,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 8..12,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_code_invalid_code_suffix() {
        let source = "# noqa: F401abc";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            InvalidCodeSuffix,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn noqa_invalid_suffix() {
        let source = "# noqa[F401]";
        let directive = lex_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            InvalidSuffix,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn flake8_exemption_all() {
        let source = "# flake8: noqa";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..14,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn flake8_noqa_no_code() {
        let source = "# flake8: noqa:";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn flake8_noqa_no_code_invalid_suffix() {
        let source = "# flake8: noqa: foo";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn flake8_noqa_no_code_trailing_content() {
        let source = "# flake8: noqa:  # Foo";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn flake8_malformed_code_1() {
        let source = "# flake8: noqa: F";
        let directive = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn flake8_malformed_code_2() {
        let source = "# flake8: noqa: RUF001, F";
        let directive = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..22,
                            codes: [
                                Code {
                                    code: "RUF001",
                                    range: 16..22,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn flake8_malformed_code_3() {
        let source = "# flake8: noqa: RUF001F";
        let directive = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            InvalidCodeSuffix,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn ruff_exemption_all() {
        let source = "# ruff: noqa";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..12,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_noqa_no_code() {
        let source = "# ruff: noqa:";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_noqa_no_code_invalid_suffix() {
        let source = "# ruff: noqa: foo";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_noqa_no_code_trailing_content() {
        let source = "# ruff: noqa:  # Foo";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_malformed_code_1() {
        let source = "# ruff: noqa: F";
        let directive = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            MissingCodes,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn ruff_malformed_code_2() {
        let source = "# ruff: noqa: RUF001, F";
        let directive = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..20,
                            codes: [
                                Code {
                                    code: "RUF001",
                                    range: 14..20,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn ruff_malformed_code_3() {
        let source = "# ruff: noqa: RUF001F";
        let directive = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive, @"
        Err(
            InvalidCodeSuffix,
        )
        ");
        assert_lexed_ranges_match_slices(directive, source);
    }

    #[test]
    fn flake8_exemption_all_no_space() {
        let source = "#flake8:noqa";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..12,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_all_no_space() {
        let source = "#ruff:noqa";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..10,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn flake8_exemption_codes() {
        // Note: Flake8 doesn't support this; it's treated as a blanket exemption.
        let source = "# flake8: noqa: F401, F841";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..26,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 16..20,
                                },
                                Code {
                                    code: "F841",
                                    range: 22..26,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_codes() {
        let source = "# ruff: noqa: F401, F841";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..24,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 14..18,
                                },
                                Code {
                                    code: "F841",
                                    range: 20..24,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(exemption, source);
    }
    #[test]
    fn ruff_exemption_codes_leading_hashes() {
        let source = "#### ruff: noqa: F401, F841";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 3..27,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 17..21,
                                },
                                Code {
                                    code: "F841",
                                    range: 23..27,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_squashed_codes() {
        let source = "# ruff: noqa: F401F841";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [
                        MissingDelimiter(
                            18..18,
                        ),
                    ],
                    directive: Codes(
                        Codes {
                            range: 0..22,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 14..18,
                                },
                                Code {
                                    code: "F841",
                                    range: 18..22,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_empty_comma() {
        let source = "# ruff: noqa: F401,,F841";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [
                        MissingItem(
                            19..19,
                        ),
                    ],
                    directive: Codes(
                        Codes {
                            range: 0..24,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 14..18,
                                },
                                Code {
                                    code: "F841",
                                    range: 20..24,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_empty_comma_space() {
        let source = "# ruff: noqa: F401, ,F841";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [
                        MissingItem(
                            19..20,
                        ),
                    ],
                    directive: Codes(
                        Codes {
                            range: 0..25,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 14..18,
                                },
                                Code {
                                    code: "F841",
                                    range: 21..25,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_invalid_code_suffix() {
        let source = "# ruff: noqa: F401abc";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Err(
            InvalidCodeSuffix,
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_code_leading_comment() {
        let source = "# Leading comment # ruff: noqa: F401";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 18..36,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 32..36,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_code_trailing_comment() {
        let source = "# ruff: noqa: F401 # Trailing comment";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..18,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 14..18,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_all_leading_comment() {
        let source = "# Leading comment # ruff: noqa";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 18..30,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_all_trailing_comment() {
        let source = "# ruff: noqa # Trailing comment";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..12,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_code_trailing_comment_no_space() {
        let source = "# ruff: noqa: F401# And another comment";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..18,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 14..18,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_all_trailing_comment_no_space() {
        let source = "# ruff: noqa# Trailing comment";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..12,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_all_trailing_comment_no_hash() {
        let source = "# ruff: noqa Trailing comment";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..12,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_code_trailing_comment_no_hash() {
        let source = "# ruff: noqa: F401 Trailing comment";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @r#"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: Codes(
                        Codes {
                            range: 0..18,
                            codes: [
                                Code {
                                    code: "F401",
                                    range: 14..18,
                                },
                            ],
                        },
                    ),
                },
            ),
        )
        "#);
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn flake8_exemption_all_case_insensitive() {
        let source = "# flake8: NoQa";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..14,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn ruff_exemption_all_case_insensitive() {
        let source = "# ruff: NoQa";
        let exemption = lex_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption, @"
        Ok(
            Some(
                NoqaLexerOutput {
                    warnings: [],
                    directive: All(
                        All {
                            range: 0..12,
                        },
                    ),
                },
            ),
        )
        ");
        assert_lexed_ranges_match_slices(exemption, source);
    }

    #[test]
    fn add_noqa_to_existing_ignore() -> Result<()> {
        assert_snapshot!(
            add_suppressions_in(
                r#"
                def unused(x):  # ruff:ignore[ANN001, ARG001, D103]
                    pass
                "#,
                SuppressionKind::Noqa,
                &LinterSettings::for_rules([
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::UnusedFunctionArgument,
                    Rule::UndocumentedPublicFunction,
                ]),
            )?,
            @"
        Added 1 suppression

        ## Fixed source

        ```py
        def unused(x):  # ruff:ignore[ANN001, ARG001, D103]  # noqa: ANN201
            pass
        ```
        "
        );
        Ok(())
    }

    #[test]
    fn add_noqa_to_existing_ignore_preview() -> Result<()> {
        assert_snapshot!(
            add_suppressions_in(
                r#"
                def unused(x):  # ruff:ignore[ANN001, ARG001, D103]
                    pass
                "#,
                SuppressionKind::Noqa,
                &LinterSettings::for_rules([
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::UnusedFunctionArgument,
                    Rule::UndocumentedPublicFunction,
                ])
                .with_preview_mode(),
            )?,
            @"
        Added 1 suppression

        ## Fixed source

        ```py
        def unused(x):  # ruff:ignore[ANN001, ARG001, D103]  # noqa: ANN201
            pass
        ```
        "
        );
        Ok(())
    }

    #[test]
    fn add_ignore_to_existing_noqa() -> Result<()> {
        assert_snapshot!(
            add_suppressions_in(
                r#"
                def unused(x):  # noqa: ANN001, ARG001, D103
                    pass
                "#,
                SuppressionKind::Ignore,
                &LinterSettings::for_rules([
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::UnusedFunctionArgument,
                    Rule::UndocumentedPublicFunction,
                ])
                .with_preview_mode(),
            )?,
            @"
        Added 1 suppression

        ## Fixed source

        ```py
        def unused(x):  # noqa: ANN001, ARG001, D103  # ruff:ignore[missing-return-type-undocumented-public-function]
            pass
        ```
        "
        );
        Ok(())
    }

    #[test]
    fn add_noqa_ruf105() -> Result<()> {
        let settings =
            LinterSettings::for_rules([Rule::NoqaComments, Rule::UnusedImport]).with_preview_mode();

        assert_snapshot!(
            add_suppressions_in(
                "import math  # noqa: F401",
                SuppressionKind::Noqa,
                &settings,
            )?,
            @"
        Added 1 suppression

        ## Fixed source

        ```py
        import math  # noqa: F401, RUF105

        ```
        "
        );
        Ok(())
    }

    #[test]
    fn add_ignore_ruf105() -> Result<()> {
        let settings =
            LinterSettings::for_rules([Rule::NoqaComments, Rule::UnusedImport]).with_preview_mode();

        assert_snapshot!(
            add_suppressions_in(
                "import math  # noqa: F401",
                SuppressionKind::Ignore,
                &settings,
            )?,
            @"
        Added 1 suppression

        ## Fixed source

        ```py
        import math  # noqa: F401  # ruff:ignore[noqa-comments]

        ```
        "
        );
        Ok(())
    }

    #[test]
    fn add_ignore_to_existing_ignore() -> Result<()> {
        assert_snapshot!(
            add_suppressions_in(
                r#"
                def unused(x):  # ruff:ignore[ANN001, ARG001, D103]
                    pass
                "#,
                SuppressionKind::Ignore,
                &LinterSettings::for_rules([
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::UnusedFunctionArgument,
                    Rule::UndocumentedPublicFunction,
                ])
                .with_preview_mode(),
            )?,
            @"
        Added 1 suppression

        ## Fixed source

        ```py
        def unused(x):  # ruff:ignore[ANN001, ARG001, D103, missing-return-type-undocumented-public-function]
            pass
        ```
        "
        );
        Ok(())
    }

    #[test]
    fn add_ignore_multiple_codes() -> Result<()> {
        assert_snapshot!(
            add_suppressions_in(
                r#"
                def unused(x):
                    pass
                "#,
                SuppressionKind::Ignore,
                &LinterSettings::for_rules([
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                    Rule::UndocumentedPublicFunction,
                ])
                .with_preview_mode(),
            )?,
            @"
        Added 1 suppression

        ## Fixed source

        ```py
        def unused(x):  # ruff:ignore[missing-return-type-undocumented-public-function, missing-type-function-argument, undocumented-public-function]
            pass
        ```
        "
        );
        Ok(())
    }

    #[test]
    fn add_ignore_multiline_diagnostic() -> Result<()> {
        assert_snapshot!(
            add_suppressions_in(
                r#"
                import z
                import c
                import a
                "#,
                SuppressionKind::Ignore,
                &LinterSettings::for_rules([Rule::UnsortedImports]).with_preview_mode(),
            )?,
            @"
        Added 1 suppression

        ## Fixed source

        ```py
        import z  # ruff:ignore[unsorted-imports]
        import c
        import a
        ```
        "
        );
        Ok(())
    }

    #[test]
    fn add_ignore_existing_own_line_ignore() -> Result<()> {
        assert_snapshot!(
            add_suppressions_in(
                r#"
                # ruff:ignore[ANN001]
                def public(x):
                    """Return x."""
                    return x
                "#,
                SuppressionKind::Ignore,
                &LinterSettings::for_rules([
                    Rule::MissingTypeFunctionArgument,
                    Rule::MissingReturnTypeUndocumentedPublicFunction,
                ])
                .with_preview_mode(),
            )?,
            @r#"
        Added 1 suppression

        ## Fixed source

        ```py
        # ruff:ignore[ANN001, missing-return-type-undocumented-public-function]
        def public(x):
            """Return x."""
            return x
        ```
        "#
        );
        Ok(())
    }

    #[test]
    fn modification() {
        let path = Path::new("/tmp/foo.txt");

        let contents = "x = 1";
        let noqa_line_for = NoqaMapping::default();
        let (count, output) = add_suppression_inner(
            path,
            &[],
            &Locator::new(contents),
            &CommentRanges::default(),
            &[],
            &noqa_line_for,
            LineEnding::Lf,
            None,
            &Suppressions::default(),
            SuppressionKind::Noqa,
        );
        assert_eq!(count, 0);
        assert_eq!(output, format!("{contents}"));

        let source_file = SourceFileBuilder::new(path.to_string_lossy(), contents).finish();
        let messages = [UnusedVariable {
            name: "x".to_string(),
        }
        .into_diagnostic(
            TextRange::new(TextSize::from(0), TextSize::from(0)),
            &source_file,
        )];

        let contents = "x = 1";
        let noqa_line_for = NoqaMapping::default();
        let (count, output) = add_suppression_inner(
            path,
            &messages,
            &Locator::new(contents),
            &CommentRanges::default(),
            &[],
            &noqa_line_for,
            LineEnding::Lf,
            None,
            &Suppressions::default(),
            SuppressionKind::Noqa,
        );
        assert_eq!(count, 1);
        assert_eq!(output, "x = 1  # noqa: F841\n");

        let source_file = SourceFileBuilder::new(path.to_string_lossy(), contents).finish();
        let messages = [
            AmbiguousVariableName("x".to_string()).into_diagnostic(
                TextRange::new(TextSize::from(0), TextSize::from(0)),
                &source_file,
            ),
            UnusedVariable {
                name: "x".to_string(),
            }
            .into_diagnostic(
                TextRange::new(TextSize::from(0), TextSize::from(0)),
                &source_file,
            ),
        ];
        let contents = "x = 1  # noqa: E741\n";
        let noqa_line_for = NoqaMapping::default();
        let comment_ranges =
            CommentRanges::new(vec![TextRange::new(TextSize::from(7), TextSize::from(19))]);
        let (count, output) = add_suppression_inner(
            path,
            &messages,
            &Locator::new(contents),
            &comment_ranges,
            &[],
            &noqa_line_for,
            LineEnding::Lf,
            None,
            &Suppressions::default(),
            SuppressionKind::Noqa,
        );
        assert_eq!(count, 1);
        assert_eq!(output, "x = 1  # noqa: E741, F841\n");

        let source_file = SourceFileBuilder::new(path.to_string_lossy(), contents).finish();
        let messages = [
            AmbiguousVariableName("x".to_string()).into_diagnostic(
                TextRange::new(TextSize::from(0), TextSize::from(0)),
                &source_file,
            ),
            UnusedVariable {
                name: "x".to_string(),
            }
            .into_diagnostic(
                TextRange::new(TextSize::from(0), TextSize::from(0)),
                &source_file,
            ),
        ];
        let contents = "x = 1  # noqa";
        let noqa_line_for = NoqaMapping::default();
        let comment_ranges =
            CommentRanges::new(vec![TextRange::new(TextSize::from(7), TextSize::from(13))]);
        let (count, output) = add_suppression_inner(
            path,
            &messages,
            &Locator::new(contents),
            &comment_ranges,
            &[],
            &noqa_line_for,
            LineEnding::Lf,
            None,
            &Suppressions::default(),
            SuppressionKind::Noqa,
        );
        assert_eq!(count, 0);
        assert_eq!(output, "x = 1  # noqa");
    }

    #[test]
    fn multiline_comment() {
        let path = Path::new("/tmp/foo.txt");
        let source = r#"
print(
    """First line
    second line
    third line
      %s"""
    % name
)
"#;
        let noqa_line_for = [TextRange::new(8.into(), 68.into())].into_iter().collect();
        let source_file = SourceFileBuilder::new(path.to_string_lossy(), source).finish();
        let messages = [PrintfStringFormatting
            .into_diagnostic(TextRange::new(12.into(), 79.into()), &source_file)];
        let comment_ranges = CommentRanges::default();
        let suppressions = Suppressions::default();
        let edits = generate_suppression_edits(
            path,
            &messages,
            &Locator::new(source),
            &comment_ranges,
            &[],
            &noqa_line_for,
            LineEnding::Lf,
            &suppressions,
            SuppressionKind::Noqa,
        );
        assert_eq!(
            edits,
            vec![Some(Edit::replacement(
                "  # noqa: UP031\n".to_string(),
                68.into(),
                69.into()
            ))]
        );
    }

    #[test]
    fn syntax_error() {
        let path = Path::new("/tmp/foo.txt");
        let source = "\
foo;
bar =
";
        let source_file = SourceFileBuilder::new(path.to_string_lossy(), source).finish();
        let messages =
            [UselessSemicolon.into_diagnostic(TextRange::new(4.into(), 5.into()), &source_file)];
        let noqa_line_for = NoqaMapping::default();
        let comment_ranges = CommentRanges::default();
        let suppressions = Suppressions::default();
        let edits = generate_suppression_edits(
            path,
            &messages,
            &Locator::new(source),
            &comment_ranges,
            &[],
            &noqa_line_for,
            LineEnding::Lf,
            &suppressions,
            SuppressionKind::Noqa,
        );
        assert_eq!(
            edits,
            vec![Some(Edit::replacement(
                "  # noqa: E703\n".to_string(),
                4.into(),
                5.into()
            ))]
        );
    }
}
