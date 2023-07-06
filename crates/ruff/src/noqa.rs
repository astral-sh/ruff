use std::collections::BTreeMap;
use std::fmt::{Display, Write};
use std::fs;
use std::ops::Add;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
use log::warn;
use once_cell::sync::Lazy;
use regex::Regex;
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_parser::ast::Ranged;

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::source_code::Locator;
use ruff_python_whitespace::{LineEnding, PythonWhitespace};

use crate::codes::NoqaCode;
use crate::registry::{AsRule, Rule, RuleSet};
use crate::rule_redirects::get_redirect_target;

static NOQA_LINE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?P<noqa>(?i:# noqa)(?::\s?(?P<codes>[A-Z]+[0-9]+(?:[,\s]+[A-Z]+[0-9]+)*))?)")
        .unwrap()
});

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
    pub(crate) fn try_extract(text: &'a str, offset: TextSize) -> Option<Self> {
        let caps = NOQA_LINE_REGEX.captures(text)?;
        match (caps.name("noqa"), caps.name("codes")) {
            (Some(noqa), Some(codes)) => {
                let codes = codes
                    .as_str()
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .map(str::trim)
                    .filter(|code| !code.is_empty())
                    .collect_vec();
                if codes.is_empty() {
                    warn!("Expected rule codes on `noqa` directive: \"{text}\"");
                }
                let range = TextRange::at(
                    TextSize::try_from(noqa.start()).unwrap().add(offset),
                    noqa.as_str().text_len(),
                );
                Some(Self::Codes(Codes { range, codes }))
            }
            (Some(noqa), None) => {
                let range = TextRange::at(
                    TextSize::try_from(noqa.start()).unwrap().add(offset),
                    noqa.as_str().text_len(),
                );
                Some(Self::All(All { range }))
            }
            _ => None,
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
        Some(Directive::All(_)) => true,
        Some(Directive::Codes(Codes { codes, range: _ })) => includes(code, &codes),
        None => false,
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
    pub(crate) fn try_extract(contents: &str, comment_ranges: &[TextRange]) -> Option<Self> {
        let mut exempt_codes: Vec<NoqaCode> = vec![];

        for range in comment_ranges {
            match ParsedFileExemption::extract(&contents[*range]) {
                ParsedFileExemption::All => {
                    return Some(Self::All);
                }
                ParsedFileExemption::Codes(codes) => {
                    exempt_codes.extend(codes.into_iter().filter_map(|code| {
                        if let Ok(rule) = Rule::from_code(get_redirect_target(code).unwrap_or(code))
                        {
                            Some(rule.noqa_code())
                        } else {
                            warn!("Invalid code provided to `# ruff: noqa`: {}", code);
                            None
                        }
                    }));
                }
                ParsedFileExemption::None => {}
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
    /// No file-level exemption was found.
    None,
    /// The file-level exemption ignores all rules (e.g., `# ruff: noqa`).
    All,
    /// The file-level exemption ignores specific rules (e.g., `# ruff: noqa: F401, F841`).
    Codes(Vec<&'a str>),
}

impl<'a> ParsedFileExemption<'a> {
    /// Return a [`ParsedFileExemption`] for a given comment line.
    fn extract(line: &'a str) -> Self {
        let line = line.trim_whitespace_start();

        if line.starts_with("# flake8: noqa")
            || line.starts_with("# flake8: NOQA")
            || line.starts_with("# flake8: NoQA")
        {
            return Self::All;
        }

        if let Some(remainder) = line
            .strip_prefix("# ruff: noqa")
            .or_else(|| line.strip_prefix("# ruff: NOQA"))
            .or_else(|| line.strip_prefix("# ruff: NoQA"))
        {
            if remainder.is_empty() {
                return Self::All;
            } else if let Some(codes) = remainder.strip_prefix(':') {
                let codes = codes
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .map(str::trim)
                    .filter(|code| !code.is_empty())
                    .collect_vec();
                if codes.is_empty() {
                    warn!("Expected rule codes on `noqa` directive: \"{line}\"");
                }
                return Self::Codes(codes);
            }
            warn!("Unexpected suffix on `noqa` directive: \"{line}\"");
        }

        Self::None
    }
}

/// Adds noqa comments to suppress all diagnostics of a file.
pub(crate) fn add_noqa(
    path: &Path,
    diagnostics: &[Diagnostic],
    locator: &Locator,
    commented_lines: &[TextRange],
    noqa_line_for: &NoqaMapping,
    line_ending: LineEnding,
) -> Result<usize> {
    let (count, output) = add_noqa_inner(
        diagnostics,
        locator,
        commented_lines,
        noqa_line_for,
        line_ending,
    );
    fs::write(path, output)?;
    Ok(count)
}

fn add_noqa_inner(
    diagnostics: &[Diagnostic],
    locator: &Locator,
    commented_ranges: &[TextRange],
    noqa_line_for: &NoqaMapping,
    line_ending: LineEnding,
) -> (usize, String) {
    // Map of line start offset to set of (non-ignored) diagnostic codes that are triggered on that line.
    let mut matches_by_line: BTreeMap<TextSize, (RuleSet, Option<&Directive>)> =
        BTreeMap::default();

    // Whether the file is exempted from all checks.
    // Codes that are globally exempted (within the current file).
    let exemption = FileExemption::try_extract(locator.contents(), commented_ranges);
    let directives = NoqaDirectives::from_commented_ranges(commented_ranges, locator);

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
                            .entry(directive_line.range.start())
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
        output.push_str(&locator.contents()[TextRange::new(prev_end, offset)]);

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

    output.push_str(&locator.contents()[usize::from(prev_end)..]);

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

#[derive(Debug, Default)]
pub(crate) struct NoqaDirectives<'a> {
    inner: Vec<NoqaDirectiveLine<'a>>,
}

impl<'a> NoqaDirectives<'a> {
    pub(crate) fn from_commented_ranges(
        comment_ranges: &[TextRange],
        locator: &'a Locator<'a>,
    ) -> Self {
        let mut directives = Vec::new();

        for range in comment_ranges {
            if let Some(directive) = Directive::try_extract(locator.slice(*range), range.start()) {
                // noqa comments are guaranteed to be single line.
                directives.push(NoqaDirectiveLine {
                    range: locator.line_range(range.start()),
                    directive,
                    matches: Vec::new(),
                });
            };
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
    pub fn resolve(&self, offset: TextSize) -> TextSize {
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

    pub fn push_mapping(&mut self, range: TextRange) {
        if let Some(last_range) = self.ranges.last_mut() {
            // Strictly sorted insertion
            if last_range.end() <= range.start() {
                // OK
            }
            // Try merging with the last inserted range
            else if let Some(intersected) = last_range.intersect(range) {
                *last_range = intersected;
                return;
            } else {
                panic!("Ranges must be inserted in sorted order")
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
    use insta::assert_debug_snapshot;
    use ruff_text_size::{TextRange, TextSize};

    use ruff_diagnostics::Diagnostic;
    use ruff_python_ast::source_code::Locator;
    use ruff_python_whitespace::LineEnding;

    use crate::noqa::{add_noqa_inner, Directive, NoqaMapping};
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
    fn modification() {
        let contents = "x = 1";
        let noqa_line_for = NoqaMapping::default();
        let (count, output) = add_noqa_inner(
            &[],
            &Locator::new(contents),
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
            &diagnostics,
            &Locator::new(contents),
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
        let (count, output) = add_noqa_inner(
            &diagnostics,
            &Locator::new(contents),
            &[TextRange::new(TextSize::from(7), TextSize::from(19))],
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
        let (count, output) = add_noqa_inner(
            &diagnostics,
            &Locator::new(contents),
            &[TextRange::new(TextSize::from(7), TextSize::from(13))],
            &noqa_line_for,
            LineEnding::Lf,
        );
        assert_eq!(count, 0);
        assert_eq!(output, "x = 1  # noqa");
    }
}
