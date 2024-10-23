use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Display;
use std::fs;
use std::ops::Add;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
use log::warn;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Edit};
use ruff_python_trivia::{indentation_at_offset, CommentRanges};
use ruff_source_file::{LineEnding, Locator};

use crate::codes::NoqaCode;
use crate::fs::relativize_path;
use crate::registry::{AsRule, Rule, RuleSet};
use crate::rule_redirects::get_redirect_target;

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
    let file_directives =
        FileNoqaDirectives::extract(locator.contents(), comment_ranges, external, path, locator);
    let exemption = FileExemption::from(&file_directives);
    let directives = NoqaDirectives::from_commented_ranges(comment_ranges, path, locator);
    let comments = find_noqa_comments(diagnostics, locator, &exemption, &directives, noqa_line_for);
    build_noqa_edits_by_diagnostic(comments, locator, line_ending)
}

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
                        codes_end += leading_space;
                        codes.push(Code {
                            code,
                            range: TextRange::at(
                                TextSize::try_from(codes_end).unwrap(),
                                code.text_len(),
                            )
                            .add(offset),
                        });

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
    pub(crate) fn lex_code(line: &str) -> Option<&str> {
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

impl<'a> Ranged for Code<'a> {
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

impl<'a> Codes<'a> {
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
    locator: &Locator,
) -> bool {
    let offset = noqa_line_for.resolve(offset);
    let line_range = locator.line_range(offset);
    match Directive::try_extract(locator.slice(line_range), line_range.start()) {
        Ok(Some(Directive::All(_))) => true,
        Ok(Some(Directive::Codes(codes))) => codes.includes(code),
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

impl<'a> FileExemption<'a> {
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
            .any(|line| ParsedFileExemption::All == line.parsed_file_exemption)
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
    pub(crate) parsed_file_exemption: ParsedFileExemption<'a>,
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
        contents: &'a str,
        comment_ranges: &CommentRanges,
        external: &[String],
        path: &Path,
        locator: &Locator,
    ) -> Self {
        let mut lines = vec![];

        for range in comment_ranges {
            match ParsedFileExemption::try_extract(&contents[range]) {
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

                    let matches = match &exemption {
                        ParsedFileExemption::All => {
                            vec![]
                        }
                        ParsedFileExemption::Codes(codes) => {
                            codes.iter().filter_map(|code| {
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
                        parsed_file_exemption: exemption,
                        matches,
                    });
                }
                Ok(None) => {}
            }
        }

        Self(lines)
    }

    pub(crate) fn lines(&self) -> &[FileNoqaDirectiveLine] {
        &self.0
    }
}

/// An individual file-level exemption (e.g., `# ruff: noqa` or `# ruff: noqa: F401, F841`). Like
/// [`FileNoqaDirectives`], but only for a single line, as opposed to an aggregated set of exemptions
/// across a source file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ParsedFileExemption<'a> {
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
    let directives =
        FileNoqaDirectives::extract(locator.contents(), comment_ranges, external, path, locator);
    let exemption = FileExemption::from(&directives);

    let directives = NoqaDirectives::from_commented_ranges(comment_ranges, path, locator);

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

impl<'a> NoqaEdit<'a> {
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

impl<'a> Ranged for NoqaEdit<'a> {
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
    // Whether the directive applies to range.end
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
            match Directive::try_extract(locator.slice(range), range.start()) {
                Err(err) => {
                    #[allow(deprecated)]
                    let line = locator.compute_line_index(range.start());
                    let path_display = relativize_path(path);
                    warn!("Invalid `# noqa` directive on {path_display}:{line}: {err}");
                }
                Ok(Some(directive)) => {
                    // noqa comments are guaranteed to be single line.
                    let range = locator.line_range(range.start());
                    directives.push(NoqaDirectiveLine {
                        range,
                        directive,
                        matches: Vec::new(),
                        includes_end: range.end() == locator.contents().text_len(),
                    });
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
    use ruff_text_size::{TextRange, TextSize};

    use ruff_diagnostics::{Diagnostic, Edit};
    use ruff_python_trivia::CommentRanges;
    use ruff_source_file::{LineEnding, Locator};

    use crate::generate_noqa_edits;
    use crate::noqa::{add_noqa_inner, Directive, NoqaMapping, ParsedFileExemption};
    use crate::rules::pycodestyle::rules::{AmbiguousVariableName, UselessSemicolon};
    use crate::rules::pyflakes::rules::UnusedVariable;
    use crate::rules::pyupgrade::rules::PrintfStringFormatting;

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
