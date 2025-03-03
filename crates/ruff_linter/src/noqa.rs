use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Display;
use std::fs;
use std::ops::Add;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::Result;
use itertools::Itertools;
use log::warn;

use regex::Regex;
use ruff_diagnostics::{Diagnostic, Edit};
use ruff_python_trivia::{indentation_at_offset, CommentRanges};
use ruff_source_file::{LineEnding, LineRanges};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::codes::NoqaCode;
use crate::fs::relativize_path;
use crate::registry::{AsRule, Rule, RuleSet};
use crate::rule_redirects::get_redirect_target;
use crate::Locator;

/// Generates an array of edits that matches the length of `diagnostics`.
/// Each potential edit in the array is paired, in order, with the associated diagnostic.
/// Each edit will add a `noqa` comment to the appropriate line in the source to hide
/// the diagnostic. These edits may conflict with each other and should not be applied
/// simultaneously.
pub fn generate_noqa_edits(
    path: &Path,
    diagnostics: &[Diagnostic],
    locator: &Locator,
    comment_ranges: &CommentRanges,
    external: &[String],
    noqa_line_for: &NoqaMapping,
    line_ending: LineEnding,
) -> Vec<Option<Edit>> {
    let file_directives = FileNoqaDirectives::extract(locator, comment_ranges, external, path);
    let exemption = FileExemption::from(&file_directives);
    let directives = NoqaDirectives::from_commented_ranges(comment_ranges, external, path, locator);
    let comments = find_noqa_comments(diagnostics, locator, &exemption, &directives, noqa_line_for);
    build_noqa_edits_by_diagnostic(comments, locator, line_ending)
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
    pub(crate) fn iter(&self) -> std::slice::Iter<Code> {
        self.codes.iter()
    }

    /// Returns `true` if the string list of `codes` includes `code` (or an alias
    /// thereof).
    pub(crate) fn includes(&self, needle: Rule) -> bool {
        let needle = needle.noqa_code();
        self.iter()
            .any(|code| needle == get_redirect_target(code.as_str()).unwrap_or(code.as_str()))
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
    match parse_inline_noqa(comment_range, locator.contents()) {
        Ok(Some(ParsedNoqa {
            directive: Directive::All(_),
            ..
        })) => true,
        Ok(Some(ParsedNoqa {
            directive: Directive::Codes(codes),
            ..
        })) => codes.includes(code),
        _ => false,
    }
}

/// A summary of the file-level exemption as extracted from [`FileNoqaDirectives`].
#[derive(Debug)]
pub(crate) enum FileExemption<'a> {
    /// The file is exempt from all rules.
    All(Vec<&'a NoqaCode>),
    /// The file is exempt from the given rules.
    Codes(Vec<&'a NoqaCode>),
}

impl FileExemption<'_> {
    /// Returns `true` if the file is exempt from the given rule.
    pub(crate) fn includes(&self, needle: Rule) -> bool {
        let needle = needle.noqa_code();
        match self {
            FileExemption::All(_) => true,
            FileExemption::Codes(codes) => codes.iter().any(|code| needle == **code),
        }
    }

    /// Returns `true` if the file exemption lists the rule directly, rather than via a blanket
    /// exemption.
    pub(crate) fn enumerates(&self, needle: Rule) -> bool {
        let needle = needle.noqa_code();
        let codes = match self {
            FileExemption::All(codes) => codes,
            FileExemption::Codes(codes) => codes,
        };
        codes.iter().any(|code| needle == **code)
    }
}

impl<'a> From<&'a FileNoqaDirectives<'a>> for FileExemption<'a> {
    fn from(directives: &'a FileNoqaDirectives) -> Self {
        let codes = directives
            .lines()
            .iter()
            .flat_map(|line| &line.matches)
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
    pub(crate) matches: Vec<NoqaCode>,
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
            let parsed = parse_file_exemption(range, locator.contents());
            match parsed {
                Ok(Some(ParsedNoqa {
                    warnings,
                    directive,
                })) => {
                    let no_indentation_at_offset =
                        indentation_at_offset(range.start(), locator.contents()).is_none();
                    if !warnings.is_empty() || no_indentation_at_offset {
                        #[allow(deprecated)]
                        let line = locator.compute_line_index(range.start());
                        let path_display = relativize_path(path);

                        for warning in warnings {
                            warn!("Missing or joined rule code(s) at {path_display}:{line}: {warning}");
                        }

                        if no_indentation_at_offset {
                            warn!("Unexpected `# ruff: noqa` directive at {path_display}:{line}. File-level suppression comments must appear on their own line. For line-level suppression, omit the `ruff:` prefix.");
                            continue;
                        }
                    }

                    let matches = match &directive {
                        Directive::All(_) => {
                            vec![]
                        }
                        Directive::Codes(codes) => {
                            codes.iter().filter_map(|code| {
                                let code = code.as_str();
                                // Ignore externally-defined rules.
                                if external.iter().any(|external| code.starts_with(external)) {
                                    return None;
                                }

                                if let Ok(rule) = Rule::from_code(get_redirect_target(code).unwrap_or(code))
                                {
                                    Some(rule.noqa_code())
                                } else {
                                    #[allow(deprecated)]
                                    let line = locator.compute_line_index(range.start());
                                    let path_display = relativize_path(path);
                                    warn!("Invalid rule code provided to `# ruff: noqa` at {path_display}:{line}: {code}");
                                    None
                                }
                            }).collect()
                        }
                    };

                    lines.push(FileNoqaDirectiveLine {
                        range,
                        parsed_file_exemption: directive,
                        matches,
                    });
                }
                Err(err) => {
                    #[allow(deprecated)]
                    let line = locator.compute_line_index(range.start());
                    let path_display = relativize_path(path);
                    warn!("Invalid `# ruff: noqa` directive at {path_display}:{line}: {err}");
                }
                Ok(None) => {}
            };
        }
        Self(lines)
    }

    pub(crate) fn lines(&self) -> &[FileNoqaDirectiveLine] {
        &self.0
    }
}

/// Output of parsed `noqa` directive.
#[derive(Debug)]
pub(crate) struct ParsedNoqa<'a> {
    warnings: Vec<ParseWarning>,
    directive: Directive<'a>,
}

/// Marks the beginning of an in-line `noqa` directive
static NOQA_DIRECTIVE_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#\s*(?i)noqa").unwrap());

/// Marks the beginning of a file-level exemption comment
static FILE_EXEMPTION_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#\s*(?:ruff|flake8)\s*:\s*(?i)noqa").unwrap());

/// Parses in-line `noqa` comment, e.g. `# noqa: F401`
pub(crate) fn parse_inline_noqa(
    comment_range: TextRange,
    source: &str,
) -> Result<Option<ParsedNoqa<'_>>, ParseError> {
    parse_noqa_with_prefix(comment_range, source, &NOQA_DIRECTIVE_PREFIX)
}

/// Parses file-level exemption comment, e.g. `# ruff: noqa: F401`
pub(crate) fn parse_file_exemption(
    comment_range: TextRange,
    source: &str,
) -> Result<Option<ParsedNoqa<'_>>, ParseError> {
    parse_noqa_with_prefix(comment_range, source, &FILE_EXEMPTION_PREFIX)
}

/// Parses noqa comment beginning with specified prefix.
/// Used internally to align parsing for both file-level and
/// in-line suppression comments.
fn parse_noqa_with_prefix<'a>(
    comment_range: TextRange,
    source: &'a str,
    prefix_regex: &LazyLock<Regex>,
) -> Result<Option<ParsedNoqa<'a>>, ParseError> {
    let line = &source[comment_range];
    let offset = comment_range.start();

    let Some(prefix_match) = prefix_regex.find(line) else {
        return Ok(None);
    };

    let comment_start = TextSize::try_from(prefix_match.start()).unwrap();
    let noqa_literal_end = TextSize::try_from(prefix_match.end()).unwrap();

    let line = &line[noqa_literal_end.to_usize()..];

    let parser = NoqaParser::default();
    Ok(Some(parser.parse(
        line,
        offset,
        comment_start,
        noqa_literal_end,
    )?))
}

#[derive(Default)]
pub(crate) struct NoqaParser<'a> {
    codes: Vec<Code<'a>>,
    warnings: Vec<ParseWarning>,
}

impl<'a> NoqaParser<'a> {
    /// Parses a generic `noqa` comment line.
    ///
    /// # Arguments
    ///
    /// * `line` - Line beginning at end of `noqa` literal
    /// * `offset` - Offset of `noqa` comment start
    /// * `comment_start` - Start of comment, relative to offset
    /// * `noqa_literal_end` - End of `noqa` literal, relative to offset
    fn parse(
        mut self,
        line: &'a str,
        offset: TextSize,
        comment_start: TextSize,
        noqa_literal_end: TextSize,
    ) -> Result<ParsedNoqa<'a>, ParseError> {
        let directive = match line.chars().next() {
            None => {
                // Ex) `# noqa`
                let range = TextRange::new(comment_start, noqa_literal_end).add(offset);
                Directive::All(All { range })
            }
            Some(c) if c.is_ascii_whitespace() || c == '#' => {
                // Ex) `# noqa#comment` or `# noqa comment`
                let range = TextRange::new(comment_start, noqa_literal_end).add(offset);
                Directive::All(All { range })
            }
            Some(':') => {
                // Ex) `# noqa: F401,F841`
                let line = &line[1..];
                let line_offset = noqa_literal_end + offset + TextSize::new(1); // Add 1 for ':'
                self.parse_items(line, line_offset)?;

                let codes = self.codes;
                let Some(last_code) = codes.last() else {
                    return Err(ParseError::MissingCodes);
                };
                let codes_end = last_code.range.end();
                let range = TextRange::new(comment_start + offset, codes_end);
                Directive::Codes(Codes { range, codes })
            }
            _ => return Err(ParseError::InvalidSuffix),
        };

        Ok(ParsedNoqa {
            warnings: self.warnings,
            directive,
        })
    }

    /// Parses list of potential codes, separated by commas or whitespace
    ///
    /// The core logic is as follows:
    /// - split into comma-separated segments,
    /// - split each of these into whitespace-separated items,
    /// - parse each item and push codes found
    /// - stop when we find `#`, no codes in an item, or an error
    ///
    /// The complexity is mainly introduced to keep track of the
    /// ranges of each code in the source text and for error recovery.
    fn parse_items(&mut self, source: &'a str, initial_offset: TextSize) -> Result<(), ParseError> {
        let mut remaining = source;
        let mut curr = initial_offset;

        // Process each comma-separated segment
        while !remaining.is_empty() {
            // Ex) `F401  F841, F842` -> (`F401  F841`,true,`F842`)
            //
            // Note: `next_remaining` is guaranteed to be smaller than
            // `remaining`, so the surrounding loop will terminate.
            let (segment, has_comma, next_remaining) = Self::extract_next_segment(remaining);
            remaining = next_remaining;

            let segment_start = curr;

            // Handle empty segments (just whitespace between commas)
            // Ex) `F401,  ,F841`
            if Self::is_empty_segment(segment) {
                let segment_len = TextSize::of(segment);
                self.add_missing_item_warning(segment_start, segment_len);
                curr = Self::advance_position(curr, segment_len, has_comma);
                continue;
            }

            // Process the items within this segment
            // Ex) `F401  F841`
            // If an empty code list is found, stop parsing entirely
            match self.parse_segment(segment, segment_start, has_comma)? {
                Some(new_curr) => curr = new_curr,
                None => return Ok(()), // Empty code list found, stop parsing
            }
        }

        Ok(())
    }

    /// Processes a single comma-separated segment
    /// Returns Some(position) if processing should continue, or None if parsing should stop
    fn parse_segment(
        &mut self,
        segment: &'a str,
        segment_start: TextSize,
        has_comma: bool,
    ) -> Result<Option<TextSize>, ParseError> {
        let mut item_remaining = segment;
        let mut segment_curr = segment_start;

        // Process each whitespace-separated item in the segment
        while !item_remaining.is_empty() {
            // Skip leading whitespace
            let (non_ws_text, ws_len) = Self::skip_leading_whitespace(item_remaining);
            item_remaining = non_ws_text;
            segment_curr += ws_len;

            if item_remaining.is_empty() {
                break;
            }

            // Extract the next item
            //
            // Note: `next_remaining` is guaranteed to be have
            // smaller size than `item_remaining`, so the surrounding
            // loop will terminate.
            let (item, next_remaining) = Self::extract_next_item(item_remaining);

            // Parse the item into codes
            match Self::parse_item(item)? {
                ParsedItem::Continue(codes) => {
                    segment_curr = self.process_codes(&codes, segment_curr);
                }
                // Ex) `F401#Comment`
                ParsedItem::StopAtComment(codes) => {
                    self.process_codes(&codes, segment_curr);
                    return Ok(None);
                }
                // Ex) If we reach "Comment" in `F401 Comment`
                ParsedItem::StopEmptyCodes => return Ok(None),
            };

            item_remaining = next_remaining;
        }

        // Calculate the final position after processing this segment
        let segment_len = TextSize::of(segment);
        Ok(Some(Self::advance_position(
            segment_start,
            segment_len,
            has_comma,
        )))
    }

    /// Parses single item in comma and whitespace delimited list.
    ///
    /// Returns code(s) found wrapped in parser state that dictates
    /// whether to continue parsing.
    ///
    /// It is expected that this returns a single code, but for purposes
    /// of error recovery we will parse, e.g., `F401F841` as two separate
    /// codes and record a warning.
    fn parse_item(item: &'a str) -> Result<ParsedItem<'a>, ParseError> {
        let mut res = Vec::new();
        let mut line = item;
        while let Some((code, end)) = Self::parse_code(line) {
            res.push(code);
            line = &line[end..];
        }
        if res.is_empty() {
            return Ok(ParsedItem::StopEmptyCodes);
        };
        match line.chars().next() {
            // Ex) `# noqa: F401#Some comment`
            Some('#') => Ok(ParsedItem::StopAtComment(res)),
            // Ex) `# noqa: F401abc`
            Some(_) if !res.is_empty() => Err(ParseError::InvalidCodeSuffix),
            _ => Ok(ParsedItem::Continue(res)),
        }
    }

    /// Parse a single code, e.g. `F401` and record location of end of code
    ///
    /// Returns `None` if beginning of text does not match the regex
    /// `[A-Z]+[0-9]+`.
    #[inline]
    pub(crate) fn parse_code(text: &'a str) -> Option<(&'a str, usize)> {
        let prefix = text.chars().take_while(char::is_ascii_uppercase).count();
        let suffix = text[prefix..]
            .chars()
            .take_while(char::is_ascii_digit)
            .count();
        if prefix > 0 && suffix > 0 {
            Some((&text[..prefix + suffix], prefix + suffix))
        } else {
            None
        }
    }

    /// Processes a list of codes, adding them to the result and updating the current position
    fn process_codes(&mut self, codes: &[&'a str], mut curr: TextSize) -> TextSize {
        for (i, code) in codes.iter().enumerate() {
            let code_len = TextSize::of(*code);

            if i > 0 {
                // Ex) `F401F841`
                self.warnings
                    .push(ParseWarning::MissingDelimiter(TextRange::at(
                        curr, code_len,
                    )));
            }

            self.codes.push(Code {
                code,
                range: TextRange::at(curr, code_len),
            });

            curr += code_len;
        }

        curr
    }

    /// Extracts the next comma-separated segment from the input
    #[inline]
    fn extract_next_segment(input: &'a str) -> (&'a str, bool, &'a str) {
        if let Some(pos) = input.find(',') {
            let segment = &input[..pos];
            let next_remaining = &input[pos + 1..];
            (segment, true, next_remaining)
        } else {
            (input, false, "")
        }
    }

    /// Extracts the next whitespace-separated item from the input
    #[inline]
    fn extract_next_item(text: &'a str) -> (&'a str, &'a str) {
        let item_end = text
            .find(|c: char| char::is_ascii_whitespace(&c))
            .unwrap_or(text.len());

        let item = &text[..item_end];
        let next_remaining = &text[item_end..];

        (item, next_remaining)
    }

    /// Advances the current position based on segment length and comma presence
    #[inline]
    fn advance_position(curr: TextSize, segment_len: TextSize, has_comma: bool) -> TextSize {
        let mut new_pos = curr + segment_len;
        if has_comma {
            new_pos += TextSize::new(1); // Add 1 for the comma
        }
        new_pos
    }

    /// Skips leading whitespace in a string and returns the remaining text and the length of whitespace skipped
    #[inline]
    fn skip_leading_whitespace(text: &'a str) -> (&'a str, TextSize) {
        // Find the position of the first non-whitespace character (if any)
        let non_ws_pos = text
            .find(|c: char| !char::is_ascii_whitespace(&c))
            .unwrap_or(text.len());

        // Calculate whitespace length and return remaining text
        let ws_len = TextSize::of(&text[..non_ws_pos]);
        (&text[non_ws_pos..], ws_len)
    }

    /// Checks if a segment is empty (contains only whitespace)
    #[inline]
    fn is_empty_segment(segment: &str) -> bool {
        segment.chars().all(|x| char::is_ascii_whitespace(&x))
    }

    /// Adds a warning for a missing item (empty segment between commas)
    #[inline]
    fn add_missing_item_warning(&mut self, start: TextSize, len: TextSize) {
        self.warnings
            .push(ParseWarning::MissingItem(TextRange::at(start, len)));
    }
}

/// Represents result of parsing item in comma and whitespace-separated list.
///
/// Contains text of codes found wrapped in instruction on whether to continue.
enum ParsedItem<'a> {
    Continue(Vec<&'a str>),
    StopAtComment(Vec<&'a str>),
    StopEmptyCodes,
}

#[derive(Debug, Clone, Copy)]
enum ParseWarning {
    MissingItem(TextRange),
    MissingDelimiter(TextRange),
}

impl Display for ParseWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingItem(_) => f.write_str("expected rule code between commas"),
            Self::MissingDelimiter(_) => {
                f.write_str("expected comma or space delimiter between codes")
            }
        }
    }
}

impl Ranged for ParseWarning {
    fn range(&self) -> TextRange {
        match *self {
            ParseWarning::MissingItem(text_range) => text_range,
            ParseWarning::MissingDelimiter(text_range) => text_range,
        }
    }
}

/// The result of an [`Importer::get_or_import_symbol`] call.
#[derive(Debug)]
pub(crate) enum ParseError {
    /// The `noqa` directive was missing valid codes (e.g., `# noqa: unused-import` instead of `# noqa: F401`).
    MissingCodes,
    /// The `noqa` directive used an invalid suffix (e.g., `# noqa; F401` instead of `# noqa: F401`).
    InvalidSuffix,
    /// The `noqa` code matched the regex "[A-Z]+[0-9]+" with text remaining
    /// (e.g. `# noqa: F401abc`)
    InvalidCodeSuffix,
}

impl Display for ParseError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::MissingCodes => fmt.write_str("expected a comma-separated list of codes (e.g., `# noqa: F401, F841`)."),
            ParseError::InvalidSuffix => {
                fmt.write_str("expected `:` followed by a comma-separated list of codes (e.g., `# noqa: F401, F841`).")
            }
            ParseError::InvalidCodeSuffix => {
                fmt.write_str("expected code to consist of uppercase letters followed by digits only (e.g. `F401`)")
            }

        }
    }
}

impl Error for ParseError {}

/// Adds noqa comments to suppress all diagnostics of a file.
pub(crate) fn add_noqa(
    path: &Path,
    diagnostics: &[Diagnostic],
    locator: &Locator,
    comment_ranges: &CommentRanges,
    external: &[String],
    noqa_line_for: &NoqaMapping,
    line_ending: LineEnding,
) -> Result<usize> {
    let (count, output) = add_noqa_inner(
        path,
        diagnostics,
        locator,
        comment_ranges,
        external,
        noqa_line_for,
        line_ending,
    );

    fs::write(path, output)?;
    Ok(count)
}

fn add_noqa_inner(
    path: &Path,
    diagnostics: &[Diagnostic],
    locator: &Locator,
    comment_ranges: &CommentRanges,
    external: &[String],
    noqa_line_for: &NoqaMapping,
    line_ending: LineEnding,
) -> (usize, String) {
    let mut count = 0;

    // Whether the file is exempted from all checks.
    let directives = FileNoqaDirectives::extract(locator, comment_ranges, external, path);
    let exemption = FileExemption::from(&directives);

    let directives = NoqaDirectives::from_commented_ranges(comment_ranges, external, path, locator);

    let comments = find_noqa_comments(diagnostics, locator, &exemption, &directives, noqa_line_for);

    let edits = build_noqa_edits_by_line(comments, locator, line_ending);

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

fn build_noqa_edits_by_diagnostic(
    comments: Vec<Option<NoqaComment>>,
    locator: &Locator,
    line_ending: LineEnding,
) -> Vec<Option<Edit>> {
    let mut edits = Vec::default();
    for comment in comments {
        match comment {
            Some(comment) => {
                if let Some(noqa_edit) = generate_noqa_edit(
                    comment.directive,
                    comment.line,
                    RuleSet::from_rule(comment.diagnostic.kind.rule()),
                    locator,
                    line_ending,
                ) {
                    edits.push(Some(noqa_edit.into_edit()));
                }
            }
            None => edits.push(None),
        }
    }
    edits
}

fn build_noqa_edits_by_line<'a>(
    comments: Vec<Option<NoqaComment<'a>>>,
    locator: &Locator,
    line_ending: LineEnding,
) -> BTreeMap<TextSize, NoqaEdit<'a>> {
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
        if let Some(edit) = generate_noqa_edit(
            directive,
            offset,
            matches
                .into_iter()
                .map(|NoqaComment { diagnostic, .. }| diagnostic.kind.rule())
                .collect(),
            locator,
            line_ending,
        ) {
            edits.insert(offset, edit);
        }
    }
    edits
}

struct NoqaComment<'a> {
    line: TextSize,
    diagnostic: &'a Diagnostic,
    directive: Option<&'a Directive<'a>>,
}

fn find_noqa_comments<'a>(
    diagnostics: &'a [Diagnostic],
    locator: &'a Locator,
    exemption: &'a FileExemption,
    directives: &'a NoqaDirectives,
    noqa_line_for: &NoqaMapping,
) -> Vec<Option<NoqaComment<'a>>> {
    // List of noqa comments, ordered to match up with `diagnostics`
    let mut comments_by_line: Vec<Option<NoqaComment<'a>>> = vec![];

    // Mark any non-ignored diagnostics.
    for diagnostic in diagnostics {
        match &exemption {
            FileExemption::All(_) => {
                // If the file is exempted, don't add any noqa directives.
                comments_by_line.push(None);
                continue;
            }
            FileExemption::Codes(codes) => {
                // If the diagnostic is ignored by a global exemption, don't add a noqa directive.
                if codes.contains(&&diagnostic.kind.rule().noqa_code()) {
                    comments_by_line.push(None);
                    continue;
                }
            }
        }

        // Is the violation ignored by a `noqa` directive on the parent line?
        if let Some(parent) = diagnostic.parent {
            if let Some(directive_line) =
                directives.find_line_with_directive(noqa_line_for.resolve(parent))
            {
                match &directive_line.directive {
                    Directive::All(_) => {
                        comments_by_line.push(None);
                        continue;
                    }
                    Directive::Codes(codes) => {
                        if codes.includes(diagnostic.kind.rule()) {
                            comments_by_line.push(None);
                            continue;
                        }
                    }
                }
            }
        }

        let noqa_offset = noqa_line_for.resolve(diagnostic.start());

        // Or ignored by the directive itself?
        if let Some(directive_line) = directives.find_line_with_directive(noqa_offset) {
            match &directive_line.directive {
                Directive::All(_) => {
                    comments_by_line.push(None);
                    continue;
                }
                directive @ Directive::Codes(codes) => {
                    let rule = diagnostic.kind.rule();
                    if !codes.includes(rule) {
                        comments_by_line.push(Some(NoqaComment {
                            line: directive_line.start(),
                            diagnostic,
                            directive: Some(directive),
                        }));
                    }
                    continue;
                }
            }
        }

        // There's no existing noqa directive that suppresses the diagnostic.
        comments_by_line.push(Some(NoqaComment {
            line: locator.line_start(noqa_offset),
            diagnostic,
            directive: None,
        }));
    }

    comments_by_line
}

struct NoqaEdit<'a> {
    edit_range: TextRange,
    rules: RuleSet,
    codes: Option<&'a Codes<'a>>,
    line_ending: LineEnding,
}

impl NoqaEdit<'_> {
    fn into_edit(self) -> Edit {
        let mut edit_content = String::new();
        self.write(&mut edit_content);

        Edit::range_replacement(edit_content, self.edit_range)
    }

    fn write(&self, writer: &mut impl std::fmt::Write) {
        write!(writer, "  # noqa: ").unwrap();
        match self.codes {
            Some(codes) => {
                push_codes(
                    writer,
                    self.rules
                        .iter()
                        .map(|rule| rule.noqa_code().to_string())
                        .chain(codes.iter().map(ToString::to_string))
                        .sorted_unstable(),
                );
            }
            None => {
                push_codes(
                    writer,
                    self.rules.iter().map(|rule| rule.noqa_code().to_string()),
                );
            }
        }
        write!(writer, "{}", self.line_ending.as_str()).unwrap();
    }
}

impl Ranged for NoqaEdit<'_> {
    fn range(&self) -> TextRange {
        self.edit_range
    }
}

fn generate_noqa_edit<'a>(
    directive: Option<&'a Directive>,
    offset: TextSize,
    rules: RuleSet,
    locator: &Locator,
    line_ending: LineEnding,
) -> Option<NoqaEdit<'a>> {
    let line_range = locator.full_line_range(offset);

    let edit_range;
    let codes;

    // Add codes.
    match directive {
        None => {
            let trimmed_line = locator.slice(line_range).trim_end();
            edit_range = TextRange::new(TextSize::of(trimmed_line), line_range.len()) + offset;
            codes = None;
        }
        Some(Directive::Codes(existing_codes)) => {
            // find trimmed line without the noqa
            let trimmed_line = locator
                .slice(TextRange::new(line_range.start(), existing_codes.start()))
                .trim_end();
            edit_range = TextRange::new(TextSize::of(trimmed_line), line_range.len()) + offset;
            codes = Some(existing_codes);
        }
        Some(Directive::All(_)) => return None,
    };

    Some(NoqaEdit {
        edit_range,
        rules,
        codes,
        line_ending,
    })
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
    pub(crate) matches: Vec<NoqaCode>,
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
        external: &[String],
        path: &Path,
        locator: &'a Locator<'a>,
    ) -> Self {
        let mut directives = Vec::new();

        for range in comment_ranges {
            let parsed = parse_inline_noqa(range, locator.contents());

            match parsed {
                Ok(Some(ParsedNoqa {
                    warnings,
                    directive,
                })) => {
                    if !warnings.is_empty() {
                        #[allow(deprecated)]
                        let line = locator.compute_line_index(range.start());
                        let path_display = relativize_path(path);
                        for warning in warnings {
                            warn!("Missing or joined rule code(s) at {path_display}:{line}: {warning}");
                        }
                    }
                    if let Directive::Codes(codes) = &directive {
                        // Warn on invalid rule codes.
                        for code in &codes.codes {
                            // Ignore externally-defined rules.
                            if !external
                                .iter()
                                .any(|external| code.as_str().starts_with(external))
                            {
                                if Rule::from_code(
                                    get_redirect_target(code.as_str()).unwrap_or(code.as_str()),
                                )
                                .is_err()
                                {
                                    #[allow(deprecated)]
                                    let line = locator.compute_line_index(range.start());
                                    let path_display = relativize_path(path);
                                    warn!("Invalid rule code provided to `# noqa` at {path_display}:{line}: {code}");
                                }
                            }
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
                    #[allow(deprecated)]
                    let line = locator.compute_line_index(range.start());
                    let path_display = relativize_path(path);
                    warn!("Invalid `# noqa` directive on {path_display}:{line}: {err}");
                }
                Ok(None) => {}
            }
        }

        Self { inner: directives }
    }

    pub(crate) fn find_line_with_directive(&self, offset: TextSize) -> Option<&NoqaDirectiveLine> {
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

    pub(crate) fn lines(&self) -> &[NoqaDirectiveLine] {
        &self.inner
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
    use std::path::Path;

    use insta::assert_debug_snapshot;

    use ruff_diagnostics::{Diagnostic, Edit};
    use ruff_python_trivia::CommentRanges;
    use ruff_source_file::LineEnding;
    use ruff_text_size::{TextLen, TextRange, TextSize};

    use crate::noqa::{
        add_noqa_inner, parse_file_exemption, parse_inline_noqa, Directive, NoqaMapping, ParsedNoqa,
    };
    use crate::rules::pycodestyle::rules::{AmbiguousVariableName, UselessSemicolon};
    use crate::rules::pyflakes::rules::UnusedVariable;
    use crate::rules::pyupgrade::rules::PrintfStringFormatting;
    use crate::{generate_noqa_edits, Locator};

    fn assert_codes_match_slices(codes: &crate::noqa::Codes, source: &str) {
        for code in codes.iter() {
            assert_eq!(&source[code.range], code.code);
        }
    }

    #[test]
    fn noqa_range() {
        let source = "# noqa: F401     F841, F841 # wiggle";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);

        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_all() {
        let source = "# noqa";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_code() {
        let source = "# noqa: F401";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_codes() {
        let source = "# noqa: F401, F841";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_all_case_insensitive() {
        let source = "# NOQA";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_code_case_insensitive() {
        let source = "# NOQA: F401";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_codes_case_insensitive() {
        let source = "# NOQA: F401, F841";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_leading_space() {
        let source = "#   # noqa: F401";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_trailing_space() {
        let source = "# noqa: F401   #";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_all_no_space() {
        let source = "#noqa";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_code_no_space() {
        let source = "#noqa:F401";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_codes_no_space() {
        let source = "#noqa:F401,F841";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_all_multi_space() {
        let source = "#  noqa";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_code_multi_space() {
        let source = "#  noqa: F401";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_codes_multi_space() {
        let source = "#  noqa: F401,  F841";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_code_leading_hashes() {
        let source = "###noqa: F401";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_all_leading_comment() {
        let source = "# Some comment describing the noqa # noqa";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_code_leading_comment() {
        let source = "# Some comment describing the noqa # noqa: F401";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_codes_leading_comment() {
        let source = "# Some comment describing the noqa # noqa: F401, F841";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_all_trailing_comment() {
        let source = "# noqa # Some comment describing the noqa";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_code_trailing_comment() {
        let source = "# noqa: F401 # Some comment describing the noqa";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_codes_trailing_comment() {
        let source = "# noqa: F401, F841 # Some comment describing the noqa";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_invalid_codes() {
        let source = "# noqa: unused-import, F401, some other code";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_squashed_codes() {
        let source = "# noqa: F401F841";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_empty_comma() {
        let source = "# noqa: F401,,F841";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_empty_comma_space() {
        let source = "# noqa: F401, ,F841";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_non_code() {
        let source = "# noqa: F401 We're ignoring an import";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_code_invalid_code_suffix() {
        let source = "# noqa: F401abc";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn noqa_invalid_suffix() {
        let source = "# noqa[F401]";
        let directive = parse_inline_noqa(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(directive);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = directive
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn flake8_exemption_all() {
        let source = "# flake8: noqa";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_all() {
        let source = "# ruff: noqa";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn flake8_exemption_all_no_space() {
        let source = "#flake8:noqa";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_all_no_space() {
        let source = "#ruff:noqa";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn flake8_exemption_codes() {
        // Note: Flake8 doesn't support this; it's treated as a blanket exemption.
        let source = "# flake8: noqa: F401, F841";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_codes() {
        let source = "# ruff: noqa: F401, F841";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }
    #[test]
    fn ruff_exemption_codes_leading_hashes() {
        let source = "#### ruff: noqa: F401, F841";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_squashed_codes() {
        let source = "# ruff: noqa: F401F841";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_empty_comma() {
        let source = "# ruff: noqa: F401,,F841";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_empty_comma_space() {
        let source = "# ruff: noqa: F401, ,F841";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_invalid_code_suffix() {
        let source = "# ruff: noqa: F401abc";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_code_leading_comment() {
        let source = "# Leading comment # ruff: noqa: F401";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_code_trailing_comment() {
        let source = "# ruff: noqa: F401 # Trailing comment";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_all_leading_comment() {
        let source = "# Leading comment # ruff: noqa";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_all_trailing_comment() {
        let source = "# ruff: noqa # Trailing comment";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_code_trailing_comment_no_space() {
        let source = "# ruff: noqa: F401# And another comment";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_all_trailing_comment_no_space() {
        let source = "# ruff: noqa# Trailing comment";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_all_trailing_comment_no_hash() {
        let source = "# ruff: noqa Trailing comment";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_code_trailing_comment_no_hash() {
        let source = "# ruff: noqa: F401 Trailing comment";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn flake8_exemption_all_case_insensitive() {
        let source = "# flake8: NoQa";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn ruff_exemption_all_case_insensitive() {
        let source = "# ruff: NoQa";
        let exemption = parse_file_exemption(TextRange::up_to(source.text_len()), source);
        assert_debug_snapshot!(exemption);
        if let Ok(Some(ParsedNoqa {
            warnings: _,
            directive: Directive::Codes(codes),
        })) = exemption
        {
            assert_codes_match_slices(&codes, source);
        }
    }

    #[test]
    fn modification() {
        let path = Path::new("/tmp/foo.txt");

        let contents = "x = 1";
        let noqa_line_for = NoqaMapping::default();
        let (count, output) = add_noqa_inner(
            path,
            &[],
            &Locator::new(contents),
            &CommentRanges::default(),
            &[],
            &noqa_line_for,
            LineEnding::Lf,
        );
        assert_eq!(count, 0);
        assert_eq!(output, format!("{contents}"));

        let diagnostics = [Diagnostic::new(
            UnusedVariable {
                name: "x".to_string(),
            },
            TextRange::new(TextSize::from(0), TextSize::from(0)),
        )];

        let contents = "x = 1";
        let noqa_line_for = NoqaMapping::default();
        let (count, output) = add_noqa_inner(
            path,
            &diagnostics,
            &Locator::new(contents),
            &CommentRanges::default(),
            &[],
            &noqa_line_for,
            LineEnding::Lf,
        );
        assert_eq!(count, 1);
        assert_eq!(output, "x = 1  # noqa: F841\n");

        let diagnostics = [
            Diagnostic::new(
                AmbiguousVariableName("x".to_string()),
                TextRange::new(TextSize::from(0), TextSize::from(0)),
            ),
            Diagnostic::new(
                UnusedVariable {
                    name: "x".to_string(),
                },
                TextRange::new(TextSize::from(0), TextSize::from(0)),
            ),
        ];
        let contents = "x = 1  # noqa: E741\n";
        let noqa_line_for = NoqaMapping::default();
        let comment_ranges =
            CommentRanges::new(vec![TextRange::new(TextSize::from(7), TextSize::from(19))]);
        let (count, output) = add_noqa_inner(
            path,
            &diagnostics,
            &Locator::new(contents),
            &comment_ranges,
            &[],
            &noqa_line_for,
            LineEnding::Lf,
        );
        assert_eq!(count, 1);
        assert_eq!(output, "x = 1  # noqa: E741, F841\n");

        let diagnostics = [
            Diagnostic::new(
                AmbiguousVariableName("x".to_string()),
                TextRange::new(TextSize::from(0), TextSize::from(0)),
            ),
            Diagnostic::new(
                UnusedVariable {
                    name: "x".to_string(),
                },
                TextRange::new(TextSize::from(0), TextSize::from(0)),
            ),
        ];
        let contents = "x = 1  # noqa";
        let noqa_line_for = NoqaMapping::default();
        let comment_ranges =
            CommentRanges::new(vec![TextRange::new(TextSize::from(7), TextSize::from(13))]);
        let (count, output) = add_noqa_inner(
            path,
            &diagnostics,
            &Locator::new(contents),
            &comment_ranges,
            &[],
            &noqa_line_for,
            LineEnding::Lf,
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
        let diagnostics = [Diagnostic::new(
            PrintfStringFormatting,
            TextRange::new(12.into(), 79.into()),
        )];
        let comment_ranges = CommentRanges::default();
        let edits = generate_noqa_edits(
            path,
            &diagnostics,
            &Locator::new(source),
            &comment_ranges,
            &[],
            &noqa_line_for,
            LineEnding::Lf,
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
        let diagnostics = [Diagnostic::new(
            UselessSemicolon,
            TextRange::new(4.into(), 5.into()),
        )];
        let noqa_line_for = NoqaMapping::default();
        let comment_ranges = CommentRanges::default();
        let edits = generate_noqa_edits(
            path,
            &diagnostics,
            &Locator::new(source),
            &comment_ranges,
            &[],
            &noqa_line_for,
            LineEnding::Lf,
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
