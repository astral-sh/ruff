use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Write};
use std::fs;
use std::ops::Add;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
use log::warn;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use ruff_diagnostics::Diagnostic;
use ruff_python_trivia::{indentation_at_offset, CommentRanges};
use ruff_source_file::{LineEnding, Locator};

use crate::codes::NoqaCode;
use crate::fs::relativize_path;
use crate::registry::{AsRule, Rule, RuleSet};
use crate::rule_redirects::get_redirect_target;

/// A directive to ignore a set of rules for a given line of Python source code (e.g.,
/// `# noqa: F401, F841`).
#[derive(Debug)]
pub(crate) enum Directive<'a> {
    /// The `noqa` directive ignores all rules (e.g., `# noqa`).
    All(All),
    /// The `noqa` directive ignores specific rules (e.g., `# noqa: F401, F841`).
    Codes(Codes<'a>),
}

impl<'a> Directive<'a> {
    /// Extract the noqa `Directive` from a line of Python source code.
    pub(crate) fn try_extract(text: &'a str, offset: TextSize) -> Result<Option<Self>, ParseError> {
        for (char_index, char) in text.char_indices() {
            // Only bother checking for the `noqa` literal if the character is `n` or `N`.
            if !matches!(char, 'n' | 'N') {
                continue;
            }

            // Determine the start of the `noqa` literal.
            if !matches!(
                text[char_index..].as_bytes(),
                [b'n' | b'N', b'o' | b'O', b'q' | b'Q', b'a' | b'A', ..]
            ) {
                continue;
            }

            let noqa_literal_start = char_index;
            let noqa_literal_end = noqa_literal_start + "noqa".len();

            // Determine the start of the comment.
            let mut comment_start = noqa_literal_start;

            // Trim any whitespace between the `#` character and the `noqa` literal.
            comment_start = text[..comment_start].trim_end().len();

            // The next character has to be the `#` character.
            if text[..comment_start]
                .chars()
                .last()
                .map_or(true, |c| c != '#')
            {
                continue;
            }
            comment_start -= '#'.len_utf8();

            // If the next character is `:`, then it's a list of codes. Otherwise, it's a directive
            // to ignore all rules.
            let directive = match text[noqa_literal_end..].chars().next() {
                Some(':') => {
                    // E.g., `# noqa: F401, F841`.
                    let mut codes_start = noqa_literal_end;

                    // Skip the `:` character.
                    codes_start += ':'.len_utf8();

                    // Skip any whitespace between the `:` and the codes.
                    codes_start += text[codes_start..]
                        .find(|c: char| !c.is_whitespace())
                        .unwrap_or(0);

                    // Extract the comma-separated list of codes.
                    let mut codes = vec![];
                    let mut codes_end = codes_start;
                    let mut leading_space = 0;
                    while let Some(code) = Self::lex_code(&text[codes_end + leading_space..]) {
                        codes.push(code);
                        codes_end += leading_space;
                        codes_end += code.len();

                        // Codes can be comma- or whitespace-delimited. Compute the length of the
                        // delimiter, but only add it in the next iteration, once we find the next
                        // code.
                        if let Some(space_between) =
                            text[codes_end..].find(|c: char| !(c.is_whitespace() || c == ','))
                        {
                            leading_space = space_between;
                        } else {
                            break;
                        }
                    }

                    // If we didn't identify any codes, warn.
                    if codes.is_empty() {
                        return Err(ParseError::MissingCodes);
                    }

                    let range = TextRange::new(
                        TextSize::try_from(comment_start).unwrap(),
                        TextSize::try_from(codes_end).unwrap(),
                    );

                    Self::Codes(Codes {
                        range: range.add(offset),
                        codes,
                    })
                }
                None | Some('#') => {
                    // E.g., `# noqa` or `# noqa# ignore`.
                    let range = TextRange::new(
                        TextSize::try_from(comment_start).unwrap(),
                        TextSize::try_from(noqa_literal_end).unwrap(),
                    );
                    Self::All(All {
                        range: range.add(offset),
                    })
                }
                Some(c) if c.is_whitespace() => {
                    // E.g., `# noqa # ignore`.
                    let range = TextRange::new(
                        TextSize::try_from(comment_start).unwrap(),
                        TextSize::try_from(noqa_literal_end).unwrap(),
                    );
                    Self::All(All {
                        range: range.add(offset),
                    })
                }
                _ => return Err(ParseError::InvalidSuffix),
            };

            return Ok(Some(directive));
        }

        Ok(None)
    }

    /// Lex an individual rule code (e.g., `F401`).
    #[inline]
    fn lex_code(line: &str) -> Option<&str> {
        // Extract, e.g., the `F` in `F401`.
        let prefix = line.chars().take_while(char::is_ascii_uppercase).count();
        // Extract, e.g., the `401` in `F401`.
        let suffix = line[prefix..]
            .chars()
            .take_while(char::is_ascii_digit)
            .count();
        if prefix > 0 && suffix > 0 {
            Some(&line[..prefix + suffix])
        } else {
            None
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

#[derive(Debug)]
pub(crate) struct Codes<'a> {
    range: TextRange,
    codes: Vec<&'a str>,
}

impl Codes<'_> {
    /// The codes that are ignored by the `noqa` directive.
    pub(crate) fn codes(&self) -> &[&str] {
        &self.codes
    }
}

impl Ranged for Codes<'_> {
    /// The range of the `noqa` directive.
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Returns `true` if the string list of `codes` includes `code` (or an alias
/// thereof).
pub(crate) fn includes(needle: Rule, haystack: &[&str]) -> bool {
    let needle = needle.noqa_code();
    haystack
        .iter()
        .any(|candidate| needle == get_redirect_target(candidate).unwrap_or(candidate))
}

/// Returns `true` if the given [`Rule`] is ignored at the specified `lineno`.
pub(crate) fn rule_is_ignored(
    code: Rule,
    offset: TextSize,
    noqa_line_for: &NoqaMapping,
    locator: &Locator,
) -> bool {
    let offset = noqa_line_for.resolve(offset);
    let line_range = locator.line_range(offset);
    match Directive::try_extract(locator.slice(line_range), line_range.start()) {
        Ok(Some(Directive::All(_))) => true,
        Ok(Some(Directive::Codes(Codes { codes, range: _ }))) => includes(code, &codes),
        _ => false,
    }
}

/// The file-level exemptions extracted from a given Python file.
#[derive(Debug)]
pub(crate) enum FileExemption {
    /// The file is exempt from all rules.
    All,
    /// The file is exempt from the given rules.
    Codes(Vec<NoqaCode>),
}

impl FileExemption {
    /// Extract the [`FileExemption`] for a given Python source file, enumerating any rules that are
    /// globally ignored within the file.
    pub(crate) fn try_extract(
        contents: &str,
        comment_ranges: &CommentRanges,
        path: &Path,
        locator: &Locator,
    ) -> Option<Self> {
        let mut exempt_codes: Vec<NoqaCode> = vec![];

        for range in comment_ranges {
            match ParsedFileExemption::try_extract(&contents[*range]) {
                Err(err) => {
                    #[allow(deprecated)]
                    let line = locator.compute_line_index(range.start());
                    let path_display = relativize_path(path);
                    warn!("Invalid `# ruff: noqa` directive at {path_display}:{line}: {err}");
                }
                Ok(Some(exemption)) => {
                    if indentation_at_offset(range.start(), locator).is_none() {
                        #[allow(deprecated)]
                        let line = locator.compute_line_index(range.start());
                        let path_display = relativize_path(path);
                        warn!("Unexpected `# ruff: noqa` directive at {path_display}:{line}. File-level suppression comments must appear on their own line. For line-level suppression, omit the `ruff:` prefix.");
                        continue;
                    }

                    match exemption {
                        ParsedFileExemption::All => {
                            return Some(Self::All);
                        }
                        ParsedFileExemption::Codes(codes) => {
                            exempt_codes.extend(codes.into_iter().filter_map(|code| {
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
                            }));
                        }
                    }
                }
                Ok(None) => {}
            }
        }

        if exempt_codes.is_empty() {
            None
        } else {
            Some(Self::Codes(exempt_codes))
        }
    }
}

/// An individual file-level exemption (e.g., `# ruff: noqa` or `# ruff: noqa: F401, F841`). Like
/// [`FileExemption`], but only for a single line, as opposed to an aggregated set of exemptions
/// across a source file.
#[derive(Debug)]
enum ParsedFileExemption<'a> {
    /// The file-level exemption ignores all rules (e.g., `# ruff: noqa`).
    All,
    /// The file-level exemption ignores specific rules (e.g., `# ruff: noqa: F401, F841`).
    Codes(Vec<&'a str>),
}

impl<'a> ParsedFileExemption<'a> {
    /// Return a [`ParsedFileExemption`] for a given comment line.
    fn try_extract(line: &'a str) -> Result<Option<Self>, ParseError> {
        let line = Self::lex_whitespace(line);
        let Some(line) = Self::lex_char(line, '#') else {
            return Ok(None);
        };
        let line = Self::lex_whitespace(line);

        let Some(line) = Self::lex_flake8(line).or_else(|| Self::lex_ruff(line)) else {
            return Ok(None);
        };

        let line = Self::lex_whitespace(line);
        let Some(line) = Self::lex_char(line, ':') else {
            return Ok(None);
        };
        let line = Self::lex_whitespace(line);
        let Some(line) = Self::lex_noqa(line) else {
            return Ok(None);
        };
        let line = Self::lex_whitespace(line);

        Ok(Some(if line.is_empty() {
            // Ex) `# ruff: noqa`
            Self::All
        } else {
            // Ex) `# ruff: noqa: F401, F841`
            let Some(line) = Self::lex_char(line, ':') else {
                return Err(ParseError::InvalidSuffix);
            };
            let line = Self::lex_whitespace(line);

            // Extract the codes from the line (e.g., `F401, F841`).
            let mut codes = vec![];
            let mut line = line;
            while let Some(code) = Self::lex_code(line) {
                codes.push(code);
                line = &line[code.len()..];

                // Codes can be comma- or whitespace-delimited.
                if let Some(rest) = Self::lex_delimiter(line).map(Self::lex_whitespace) {
                    line = rest;
                } else {
                    break;
                }
            }

            // If we didn't identify any codes, warn.
            if codes.is_empty() {
                return Err(ParseError::MissingCodes);
            }

            Self::Codes(codes)
        }))
    }

    /// Lex optional leading whitespace.
    #[inline]
    fn lex_whitespace(line: &str) -> &str {
        line.trim_start()
    }

    /// Lex a specific character, or return `None` if the character is not the first character in
    /// the line.
    #[inline]
    fn lex_char(line: &str, c: char) -> Option<&str> {
        let mut chars = line.chars();
        if chars.next() == Some(c) {
            Some(chars.as_str())
        } else {
            None
        }
    }

    /// Lex the "flake8" prefix of a `noqa` directive.
    #[inline]
    fn lex_flake8(line: &str) -> Option<&str> {
        line.strip_prefix("flake8")
    }

    /// Lex the "ruff" prefix of a `noqa` directive.
    #[inline]
    fn lex_ruff(line: &str) -> Option<&str> {
        line.strip_prefix("ruff")
    }

    /// Lex a `noqa` directive with case-insensitive matching.
    #[inline]
    fn lex_noqa(line: &str) -> Option<&str> {
        match line.as_bytes() {
            [b'n' | b'N', b'o' | b'O', b'q' | b'Q', b'a' | b'A', ..] => Some(&line["noqa".len()..]),
            _ => None,
        }
    }

    /// Lex a code delimiter, which can either be a comma or whitespace.
    #[inline]
    fn lex_delimiter(line: &str) -> Option<&str> {
        let mut chars = line.chars();
        if let Some(c) = chars.next() {
            if c == ',' || c.is_whitespace() {
                Some(chars.as_str())
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Lex an individual rule code (e.g., `F401`).
    #[inline]
    fn lex_code(line: &str) -> Option<&str> {
        // Extract, e.g., the `F` in `F401`.
        let prefix = line.chars().take_while(char::is_ascii_uppercase).count();
        // Extract, e.g., the `401` in `F401`.
        let suffix = line[prefix..]
            .chars()
            .take_while(char::is_ascii_digit)
            .count();
        if prefix > 0 && suffix > 0 {
            Some(&line[..prefix + suffix])
        } else {
            None
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
}

impl Display for ParseError {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::MissingCodes => fmt.write_str("expected a comma-separated list of codes (e.g., `# noqa: F401, F841`)."),
            ParseError::InvalidSuffix => {
                fmt.write_str("expected `:` followed by a comma-separated list of codes (e.g., `# noqa: F401, F841`).")
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
    noqa_line_for: &NoqaMapping,
    line_ending: LineEnding,
) -> Result<usize> {
    let (count, output) = add_noqa_inner(
        path,
        diagnostics,
        locator,
        comment_ranges,
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
    noqa_line_for: &NoqaMapping,
    line_ending: LineEnding,
) -> (usize, String) {
    // Map of line start offset to set of (non-ignored) diagnostic codes that are triggered on that line.
    let mut matches_by_line: BTreeMap<TextSize, (RuleSet, Option<&Directive>)> =
        BTreeMap::default();

    // Whether the file is exempted from all checks.
    // Codes that are globally exempted (within the current file).
    let exemption = FileExemption::try_extract(locator.contents(), comment_ranges, path, locator);
    let directives = NoqaDirectives::from_commented_ranges(comment_ranges, path, locator);

    // Mark any non-ignored diagnostics.
    for diagnostic in diagnostics {
        match &exemption {
            Some(FileExemption::All) => {
                // If the file is exempted, don't add any noqa directives.
                continue;
            }
            Some(FileExemption::Codes(codes)) => {
                // If the diagnostic is ignored by a global exemption, don't add a noqa directive.
                if codes.contains(&diagnostic.kind.rule().noqa_code()) {
                    continue;
                }
            }
            None => {}
        }

        // Is the violation ignored by a `noqa` directive on the parent line?
        if let Some(parent) = diagnostic.parent {
            if let Some(directive_line) =
                directives.find_line_with_directive(noqa_line_for.resolve(parent))
            {
                match &directive_line.directive {
                    Directive::All(_) => {
                        continue;
                    }
                    Directive::Codes(Codes { codes, range: _ }) => {
                        if includes(diagnostic.kind.rule(), codes) {
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
                    continue;
                }
                Directive::Codes(Codes { codes, range: _ }) => {
                    let rule = diagnostic.kind.rule();
                    if !includes(rule, codes) {
                        matches_by_line
                            .entry(directive_line.start())
                            .or_insert_with(|| {
                                (RuleSet::default(), Some(&directive_line.directive))
                            })
                            .0
                            .insert(rule);
                    }
                    continue;
                }
            }
        }

        // There's no existing noqa directive that suppresses the diagnostic.
        matches_by_line
            .entry(locator.line_start(noqa_offset))
            .or_insert_with(|| (RuleSet::default(), None))
            .0
            .insert(diagnostic.kind.rule());
    }

    let mut count = 0;
    let mut output = String::with_capacity(locator.len());
    let mut prev_end = TextSize::default();

    for (offset, (rules, directive)) in matches_by_line {
        output.push_str(locator.slice(TextRange::new(prev_end, offset)));

        let line = locator.full_line(offset);

        match directive {
            None => {
                // Add existing content.
                output.push_str(line.trim_end());

                // Add `noqa` directive.
                output.push_str("  # noqa: ");

                // Add codes.
                push_codes(&mut output, rules.iter().map(|rule| rule.noqa_code()));
                output.push_str(&line_ending);
                count += 1;
            }
            Some(Directive::All(_)) => {
                // Does not get inserted into the map.
            }
            Some(Directive::Codes(Codes { range, codes })) => {
                // Reconstruct the line based on the preserved rule codes.
                // This enables us to tally the number of edits.
                let output_start = output.len();

                // Add existing content.
                output.push_str(
                    locator
                        .slice(TextRange::new(offset, range.start()))
                        .trim_end(),
                );

                // Add `noqa` directive.
                output.push_str("  # noqa: ");

                // Add codes.
                push_codes(
                    &mut output,
                    rules
                        .iter()
                        .map(|rule| rule.noqa_code().to_string())
                        .chain(codes.iter().map(ToString::to_string))
                        .sorted_unstable(),
                );

                // Only count if the new line is an actual edit.
                if &output[output_start..] != line.trim_end() {
                    count += 1;
                }

                output.push_str(&line_ending);
            }
        }

        prev_end = offset + line.text_len();
    }

    output.push_str(locator.after(prev_end));

    (count, output)
}

fn push_codes<I: Display>(str: &mut String, codes: impl Iterator<Item = I>) {
    let mut first = true;
    for code in codes {
        if !first {
            str.push_str(", ");
        }
        write!(str, "{code}").unwrap();
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
            match Directive::try_extract(locator.slice(*range), range.start()) {
                Err(err) => {
                    #[allow(deprecated)]
                    let line = locator.compute_line_index(range.start());
                    let path_display = relativize_path(path);
                    warn!("Invalid `# noqa` directive on {path_display}:{line}: {err}");
                }
                Ok(Some(directive)) => {
                    // noqa comments are guaranteed to be single line.
                    directives.push(NoqaDirectiveLine {
                        range: locator.line_range(range.start()),
                        directive,
                        matches: Vec::new(),
                    });
                }
                Ok(None) => {}
            }
        }

        // Extend a mapping at the end of the file to also include the EOF token.
        if let Some(last) = directives.last_mut() {
            if last.range.end() == locator.contents().text_len() {
                last.range = last.range.add_end(TextSize::from(1));
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
                } else if directive.range.contains(offset) {
                    std::cmp::Ordering::Equal
                } else {
                    std::cmp::Ordering::Greater
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
    use ruff_text_size::{TextRange, TextSize};

    use ruff_diagnostics::Diagnostic;
    use ruff_python_trivia::CommentRanges;
    use ruff_source_file::{LineEnding, Locator};

    use crate::noqa::{add_noqa_inner, Directive, NoqaMapping, ParsedFileExemption};
    use crate::rules::pycodestyle::rules::AmbiguousVariableName;
    use crate::rules::pyflakes::rules::UnusedVariable;

    #[test]
    fn noqa_all() {
        let source = "# noqa";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_code() {
        let source = "# noqa: F401";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_codes() {
        let source = "# noqa: F401, F841";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_all_case_insensitive() {
        let source = "# NOQA";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_code_case_insensitive() {
        let source = "# NOQA: F401";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_codes_case_insensitive() {
        let source = "# NOQA: F401, F841";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_leading_space() {
        let source = "#   # noqa: F401";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_trailing_space() {
        let source = "# noqa: F401   #";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_all_no_space() {
        let source = "#noqa";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_code_no_space() {
        let source = "#noqa:F401";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_codes_no_space() {
        let source = "#noqa:F401,F841";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_all_multi_space() {
        let source = "#  noqa";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_code_multi_space() {
        let source = "#  noqa: F401";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_codes_multi_space() {
        let source = "#  noqa: F401,  F841";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_all_leading_comment() {
        let source = "# Some comment describing the noqa # noqa";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_code_leading_comment() {
        let source = "# Some comment describing the noqa # noqa: F401";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_codes_leading_comment() {
        let source = "# Some comment describing the noqa # noqa: F401, F841";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_all_trailing_comment() {
        let source = "# noqa # Some comment describing the noqa";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_code_trailing_comment() {
        let source = "# noqa: F401 # Some comment describing the noqa";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_codes_trailing_comment() {
        let source = "# noqa: F401, F841 # Some comment describing the noqa";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_invalid_codes() {
        let source = "# noqa: unused-import, F401, some other code";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn noqa_invalid_suffix() {
        let source = "# noqa[F401]";
        assert_debug_snapshot!(Directive::try_extract(source, TextSize::default()));
    }

    #[test]
    fn flake8_exemption_all() {
        let source = "# flake8: noqa";
        assert_debug_snapshot!(ParsedFileExemption::try_extract(source));
    }

    #[test]
    fn ruff_exemption_all() {
        let source = "# ruff: noqa";
        assert_debug_snapshot!(ParsedFileExemption::try_extract(source));
    }

    #[test]
    fn flake8_exemption_all_no_space() {
        let source = "#flake8:noqa";
        assert_debug_snapshot!(ParsedFileExemption::try_extract(source));
    }

    #[test]
    fn ruff_exemption_all_no_space() {
        let source = "#ruff:noqa";
        assert_debug_snapshot!(ParsedFileExemption::try_extract(source));
    }

    #[test]
    fn flake8_exemption_codes() {
        // Note: Flake8 doesn't support this; it's treated as a blanket exemption.
        let source = "# flake8: noqa: F401, F841";
        assert_debug_snapshot!(ParsedFileExemption::try_extract(source));
    }

    #[test]
    fn ruff_exemption_codes() {
        let source = "# ruff: noqa: F401, F841";
        assert_debug_snapshot!(ParsedFileExemption::try_extract(source));
    }

    #[test]
    fn flake8_exemption_all_case_insensitive() {
        let source = "# flake8: NoQa";
        assert_debug_snapshot!(ParsedFileExemption::try_extract(source));
    }

    #[test]
    fn ruff_exemption_all_case_insensitive() {
        let source = "# ruff: NoQa";
        assert_debug_snapshot!(ParsedFileExemption::try_extract(source));
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
            &noqa_line_for,
            LineEnding::Lf,
        );
        assert_eq!(count, 0);
        assert_eq!(output, "x = 1  # noqa");
    }
}
