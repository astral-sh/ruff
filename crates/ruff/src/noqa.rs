use std::fmt::{Display, Write};
use std::fs;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
use log::warn;
use nohash_hasher::IntMap;
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::FxHashMap;
use rustpython_parser::ast::Location;

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::source_code::{LineEnding, Locator};
use ruff_python_ast::types::Range;

use crate::codes::NoqaCode;
use crate::registry::{AsRule, Rule, RuleSet};
use crate::rule_redirects::get_redirect_target;

static NOQA_LINE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?P<leading_spaces>\s*)(?P<noqa>(?i:# noqa)(?::\s?(?P<codes>(?:[A-Z]+[0-9]+)(?:[,\s]+[A-Z]+[0-9]+)*))?)(?P<trailing_spaces>\s*)",
    )
    .unwrap()
});
static SPLIT_COMMA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,\s]").unwrap());

#[derive(Debug)]
pub enum Directive<'a> {
    None,
    All(usize, usize, usize, usize),
    Codes(usize, usize, usize, Vec<&'a str>, usize),
}

/// Extract the noqa `Directive` from a line of Python source code.
pub fn extract_noqa_directive(line: &str) -> Directive {
    match NOQA_LINE_REGEX.captures(line) {
        Some(caps) => match caps.name("leading_spaces") {
            Some(leading_spaces) => match caps.name("trailing_spaces") {
                Some(trailing_spaces) => match caps.name("noqa") {
                    Some(noqa) => match caps.name("codes") {
                        Some(codes) => {
                            let codes: Vec<&str> = SPLIT_COMMA_REGEX
                                .split(codes.as_str().trim())
                                .map(str::trim)
                                .filter(|code| !code.is_empty())
                                .collect();
                            if codes.is_empty() {
                                warn!("Expected rule codes on `noqa` directive: \"{line}\"");
                            }
                            Directive::Codes(
                                leading_spaces.as_str().chars().count(),
                                noqa.start(),
                                noqa.end(),
                                codes,
                                trailing_spaces.as_str().chars().count(),
                            )
                        }
                        None => Directive::All(
                            leading_spaces.as_str().chars().count(),
                            noqa.start(),
                            noqa.end(),
                            trailing_spaces.as_str().chars().count(),
                        ),
                    },
                    None => Directive::None,
                },
                None => Directive::None,
            },
            None => Directive::None,
        },
        None => Directive::None,
    }
}

enum ParsedExemption<'a> {
    None,
    All,
    Codes(Vec<&'a str>),
}

/// Return a [`ParsedExemption`] for a given comment line.
fn parse_file_exemption(line: &str) -> ParsedExemption {
    let line = line.trim_start();

    if line.starts_with("# flake8: noqa")
        || line.starts_with("# flake8: NOQA")
        || line.starts_with("# flake8: NoQA")
    {
        return ParsedExemption::All;
    }

    if let Some(remainder) = line
        .strip_prefix("# ruff: noqa")
        .or_else(|| line.strip_prefix("# ruff: NOQA"))
        .or_else(|| line.strip_prefix("# ruff: NoQA"))
    {
        if remainder.is_empty() {
            return ParsedExemption::All;
        } else if let Some(codes) = remainder.strip_prefix(':') {
            let codes: Vec<&str> = SPLIT_COMMA_REGEX
                .split(codes.trim())
                .map(str::trim)
                .filter(|code| !code.is_empty())
                .collect();
            if codes.is_empty() {
                warn!("Expected rule codes on `noqa` directive: \"{line}\"");
            }
            return ParsedExemption::Codes(codes);
        }
        warn!("Unexpected suffix on `noqa` directive: \"{line}\"");
    }

    ParsedExemption::None
}

/// Returns `true` if the string list of `codes` includes `code` (or an alias
/// thereof).
pub fn includes(needle: Rule, haystack: &[&str]) -> bool {
    let needle = needle.noqa_code();
    haystack
        .iter()
        .any(|candidate| needle == get_redirect_target(candidate).unwrap_or(candidate))
}

/// Returns `true` if the given [`Rule`] is ignored at the specified `lineno`.
pub fn rule_is_ignored(
    code: Rule,
    lineno: usize,
    noqa_line_for: &IntMap<usize, usize>,
    locator: &Locator,
) -> bool {
    let noqa_lineno = noqa_line_for.get(&lineno).unwrap_or(&lineno);
    let line = locator.slice(Range::new(
        Location::new(*noqa_lineno, 0),
        Location::new(noqa_lineno + 1, 0),
    ));
    match extract_noqa_directive(line) {
        Directive::None => false,
        Directive::All(..) => true,
        Directive::Codes(.., codes, _) => includes(code, &codes),
    }
}

pub enum FileExemption {
    None,
    All,
    Codes(Vec<NoqaCode>),
}

/// Extract the [`FileExemption`] for a given Python source file, enumerating any rules that are
/// globally ignored within the file.
pub fn file_exemption(lines: &[&str], commented_lines: &[usize]) -> FileExemption {
    let mut exempt_codes: Vec<NoqaCode> = vec![];

    for lineno in commented_lines {
        match parse_file_exemption(lines[lineno - 1]) {
            ParsedExemption::All => {
                return FileExemption::All;
            }
            ParsedExemption::Codes(codes) => {
                exempt_codes.extend(codes.into_iter().filter_map(|code| {
                    if let Ok(rule) = Rule::from_code(get_redirect_target(code).unwrap_or(code)) {
                        Some(rule.noqa_code())
                    } else {
                        warn!("Invalid code provided to `# ruff: noqa`: {}", code);
                        None
                    }
                }));
            }
            ParsedExemption::None => {}
        }
    }

    if exempt_codes.is_empty() {
        FileExemption::None
    } else {
        FileExemption::Codes(exempt_codes)
    }
}

pub fn add_noqa(
    path: &Path,
    diagnostics: &[Diagnostic],
    contents: &str,
    commented_lines: &[usize],
    noqa_line_for: &IntMap<usize, usize>,
    line_ending: LineEnding,
) -> Result<usize> {
    let (count, output) = add_noqa_inner(
        diagnostics,
        contents,
        commented_lines,
        noqa_line_for,
        line_ending,
    );
    fs::write(path, output)?;
    Ok(count)
}

fn add_noqa_inner(
    diagnostics: &[Diagnostic],
    contents: &str,
    commented_lines: &[usize],
    noqa_line_for: &IntMap<usize, usize>,
    line_ending: LineEnding,
) -> (usize, String) {
    // Map of line number to set of (non-ignored) diagnostic codes that are triggered on that line.
    let mut matches_by_line: FxHashMap<usize, RuleSet> = FxHashMap::default();

    let lines: Vec<&str> = contents.universal_newlines().collect();

    // Whether the file is exempted from all checks.
    // Codes that are globally exempted (within the current file).
    let exemption = file_exemption(&lines, commented_lines);

    // Mark any non-ignored diagnostics.
    for diagnostic in diagnostics {
        match &exemption {
            FileExemption::All => {
                // If the file is exempted, don't add any noqa directives.
                continue;
            }
            FileExemption::Codes(codes) => {
                // If the diagnostic is ignored by a global exemption, don't add a noqa directive.
                if codes.contains(&diagnostic.kind.rule().noqa_code()) {
                    continue;
                }
            }
            FileExemption::None => {}
        }

        let diagnostic_lineno = diagnostic.location.row();

        // Is the violation ignored by a `noqa` directive on the parent line?
        if let Some(parent_lineno) = diagnostic.parent.map(|location| location.row()) {
            if parent_lineno != diagnostic_lineno {
                let noqa_lineno = noqa_line_for.get(&parent_lineno).unwrap_or(&parent_lineno);
                if commented_lines.contains(noqa_lineno) {
                    match extract_noqa_directive(lines[noqa_lineno - 1]) {
                        Directive::All(..) => {
                            continue;
                        }
                        Directive::Codes(.., codes, _) => {
                            if includes(diagnostic.kind.rule(), &codes) {
                                continue;
                            }
                        }
                        Directive::None => {}
                    }
                }
            }
        }

        // Is the diagnostic ignored by a `noqa` directive on the same line?
        let noqa_lineno = noqa_line_for
            .get(&diagnostic_lineno)
            .unwrap_or(&diagnostic_lineno);
        if commented_lines.contains(noqa_lineno) {
            match extract_noqa_directive(lines[noqa_lineno - 1]) {
                Directive::All(..) => {
                    continue;
                }
                Directive::Codes(.., codes, _) => {
                    if includes(diagnostic.kind.rule(), &codes) {
                        continue;
                    }
                }
                Directive::None => {}
            }
        }

        // The diagnostic is not ignored by any `noqa` directive; add it to the list.
        let lineno = diagnostic.location.row() - 1;
        let noqa_lineno = noqa_line_for.get(&(lineno + 1)).unwrap_or(&(lineno + 1)) - 1;
        matches_by_line
            .entry(noqa_lineno)
            .or_default()
            .insert(diagnostic.kind.rule());
    }

    let mut count: usize = 0;
    let mut output = String::new();
    for (lineno, line) in lines.into_iter().enumerate() {
        match matches_by_line.get(&lineno) {
            None => {
                output.push_str(line);
                output.push_str(&line_ending);
            }
            Some(rules) => {
                match extract_noqa_directive(line) {
                    Directive::None => {
                        // Add existing content.
                        output.push_str(line.trim_end());

                        // Add `noqa` directive.
                        output.push_str("  # noqa: ");

                        // Add codes.
                        push_codes(&mut output, rules.iter().map(|rule| rule.noqa_code()));
                        output.push_str(&line_ending);
                        count += 1;
                    }
                    Directive::All(..) => {
                        // Leave the line as-is.
                        output.push_str(line);
                        output.push_str(&line_ending);
                    }
                    Directive::Codes(_, start_byte, _, existing, _) => {
                        // Reconstruct the line based on the preserved rule codes.
                        // This enables us to tally the number of edits.
                        let mut formatted = String::with_capacity(line.len());

                        // Add existing content.
                        formatted.push_str(line[..start_byte].trim_end());

                        // Add `noqa` directive.
                        formatted.push_str("  # noqa: ");

                        // Add codes.
                        push_codes(
                            &mut formatted,
                            rules
                                .iter()
                                .map(|r| r.noqa_code().to_string())
                                .chain(existing.into_iter().map(ToString::to_string))
                                .sorted_unstable(),
                        );

                        output.push_str(&formatted);
                        output.push_str(&line_ending);

                        // Only count if the new line is an actual edit.
                        if formatted != line {
                            count += 1;
                        }
                    }
                };
            }
        }
    }

    (count, output)
}

fn push_codes<I: Display>(str: &mut String, codes: impl Iterator<Item = I>) {
    let mut first = true;
    for code in codes {
        if !first {
            str.push_str(", ");
        }
        let _ = write!(str, "{code}");
        first = false;
    }
}

#[cfg(test)]
mod tests {
    use nohash_hasher::IntMap;
    use rustpython_parser::ast::Location;

    use ruff_diagnostics::Diagnostic;
    use ruff_python_ast::source_code::LineEnding;
    use ruff_python_ast::types::Range;

    use crate::noqa::{add_noqa_inner, NOQA_LINE_REGEX};
    use crate::rules::pycodestyle::rules::AmbiguousVariableName;
    use crate::rules::pyflakes;

    #[test]
    fn regex() {
        assert!(NOQA_LINE_REGEX.is_match("# noqa"));
        assert!(NOQA_LINE_REGEX.is_match("# NoQA"));

        assert!(NOQA_LINE_REGEX.is_match("# noqa: F401"));
        assert!(NOQA_LINE_REGEX.is_match("# NoQA: F401"));
        assert!(NOQA_LINE_REGEX.is_match("# noqa: F401, E501"));

        assert!(NOQA_LINE_REGEX.is_match("# noqa:F401"));
        assert!(NOQA_LINE_REGEX.is_match("# NoQA:F401"));
        assert!(NOQA_LINE_REGEX.is_match("# noqa:F401, E501"));
    }

    #[test]
    fn modification() {
        let diagnostics = vec![];
        let contents = "x = 1";
        let commented_lines = vec![];
        let noqa_line_for = IntMap::default();
        let (count, output) = add_noqa_inner(
            &diagnostics,
            contents,
            &commented_lines,
            &noqa_line_for,
            LineEnding::Lf,
        );
        assert_eq!(count, 0);
        assert_eq!(output, format!("{contents}\n"));

        let diagnostics = vec![Diagnostic::new(
            pyflakes::rules::UnusedVariable {
                name: "x".to_string(),
            },
            Range::new(Location::new(1, 0), Location::new(1, 0)),
        )];
        let contents = "x = 1";
        let commented_lines = vec![1];
        let noqa_line_for = IntMap::default();
        let (count, output) = add_noqa_inner(
            &diagnostics,
            contents,
            &commented_lines,
            &noqa_line_for,
            LineEnding::Lf,
        );
        assert_eq!(count, 1);
        assert_eq!(output, "x = 1  # noqa: F841\n");

        let diagnostics = vec![
            Diagnostic::new(
                AmbiguousVariableName("x".to_string()),
                Range::new(Location::new(1, 0), Location::new(1, 0)),
            ),
            Diagnostic::new(
                pyflakes::rules::UnusedVariable {
                    name: "x".to_string(),
                },
                Range::new(Location::new(1, 0), Location::new(1, 0)),
            ),
        ];
        let contents = "x = 1  # noqa: E741\n";
        let commented_lines = vec![1];
        let noqa_line_for = IntMap::default();
        let (count, output) = add_noqa_inner(
            &diagnostics,
            contents,
            &commented_lines,
            &noqa_line_for,
            LineEnding::Lf,
        );
        assert_eq!(count, 1);
        assert_eq!(output, "x = 1  # noqa: E741, F841\n");

        let diagnostics = vec![
            Diagnostic::new(
                AmbiguousVariableName("x".to_string()),
                Range::new(Location::new(1, 0), Location::new(1, 0)),
            ),
            Diagnostic::new(
                pyflakes::rules::UnusedVariable {
                    name: "x".to_string(),
                },
                Range::new(Location::new(1, 0), Location::new(1, 0)),
            ),
        ];
        let contents = "x = 1  # noqa";
        let commented_lines = vec![1];
        let noqa_line_for = IntMap::default();
        let (count, output) = add_noqa_inner(
            &diagnostics,
            contents,
            &commented_lines,
            &noqa_line_for,
            LineEnding::Lf,
        );
        assert_eq!(count, 0);
        assert_eq!(output, "x = 1  # noqa\n");
    }
}
