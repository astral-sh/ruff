use std::fmt::{Display, Write};
use std::fs;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
use log::warn;
use nohash_hasher::IntMap;
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::codes::NoqaCode;
use crate::registry::{Diagnostic, Rule};
use crate::rule_redirects::get_redirect_target;
use crate::source_code::{LineEnding, Locator};

static NOQA_LINE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?P<spaces>\s*)(?P<noqa>(?i:# noqa)(?::\s?(?P<codes>([A-Z]+[0-9]+(?:[,\s]+)?)+))?)",
    )
    .unwrap()
});
static SPLIT_COMMA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,\s]").unwrap());

#[derive(Debug)]
pub enum Exemption<'a> {
    None,
    All,
    Codes(Vec<&'a str>),
}

/// Return `true` if a file is exempt from checking based on the contents of the
/// given line.
pub fn extract_file_exemption(line: &str) -> Exemption {
    let line = line.trim_start();

    if line.starts_with("# flake8: noqa")
        || line.starts_with("# flake8: NOQA")
        || line.starts_with("# flake8: NoQA")
    {
        return Exemption::All;
    }

    if let Some(remainder) = line
        .strip_prefix("# ruff: noqa")
        .or_else(|| line.strip_prefix("# ruff: NOQA"))
        .or_else(|| line.strip_prefix("# ruff: NoQA"))
    {
        if remainder.is_empty() {
            return Exemption::All;
        } else if let Some(codes) = remainder.strip_prefix(':') {
            let codes: Vec<&str> = SPLIT_COMMA_REGEX
                .split(codes.trim())
                .map(str::trim)
                .filter(|code| !code.is_empty())
                .collect();
            if codes.is_empty() {
                warn!("Expected rule codes on `noqa` directive: \"{line}\"");
            }
            return Exemption::Codes(codes);
        }
        warn!("Unexpected suffix on `noqa` directive: \"{line}\"");
    }

    Exemption::None
}

#[derive(Debug)]
pub enum Directive<'a> {
    None,
    All(usize, usize, usize),
    Codes(usize, usize, usize, Vec<&'a str>),
}

/// Extract the noqa `Directive` from a line of Python source code.
pub fn extract_noqa_directive(line: &str) -> Directive {
    match NOQA_LINE_REGEX.captures(line) {
        Some(caps) => match caps.name("spaces") {
            Some(spaces) => match caps.name("noqa") {
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
                            spaces.as_str().chars().count(),
                            noqa.start(),
                            noqa.end(),
                            codes,
                        )
                    }
                    None => {
                        Directive::All(spaces.as_str().chars().count(), noqa.start(), noqa.end())
                    }
                },
                None => Directive::None,
            },
            None => Directive::None,
        },
        None => Directive::None,
    }
}

/// Returns `true` if the string list of `codes` includes `code` (or an alias
/// thereof).
pub fn includes(needle: &Rule, haystack: &[&str]) -> bool {
    let needle = needle.noqa_code();
    haystack
        .iter()
        .any(|candidate| needle == get_redirect_target(candidate).unwrap_or(candidate))
}

/// Returns `true` if the given [`Rule`] is ignored at the specified `lineno`.
pub fn rule_is_ignored(
    code: &Rule,
    lineno: usize,
    noqa_line_for: &IntMap<usize, usize>,
    locator: &Locator,
) -> bool {
    let noqa_lineno = noqa_line_for.get(&lineno).unwrap_or(&lineno);
    let line = locator.slice(&Range::new(
        Location::new(*noqa_lineno, 0),
        Location::new(noqa_lineno + 1, 0),
    ));
    match extract_noqa_directive(line) {
        Directive::None => false,
        Directive::All(..) => true,
        Directive::Codes(.., codes) => includes(code, &codes),
    }
}

pub fn add_noqa(
    path: &Path,
    diagnostics: &[Diagnostic],
    contents: &str,
    commented_lines: &[usize],
    noqa_line_for: &IntMap<usize, usize>,
    line_ending: &LineEnding,
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
    line_ending: &LineEnding,
) -> (usize, String) {
    // Map of line number to set of (non-ignored) diagnostic codes that are triggered on that line.
    let mut matches_by_line: FxHashMap<usize, FxHashSet<&Rule>> = FxHashMap::default();

    // Whether the file is exempted from all checks.
    let mut file_exempted = false;

    // Codes that are globally exempted (within the current file).
    let mut file_exemptions: Vec<NoqaCode> = vec![];

    let lines: Vec<&str> = contents.lines().collect();
    for lineno in commented_lines {
        match extract_file_exemption(lines[lineno - 1]) {
            Exemption::All => {
                file_exempted = true;
            }
            Exemption::Codes(codes) => {
                file_exemptions.extend(codes.into_iter().filter_map(|code| {
                    if let Ok(rule) = Rule::from_code(get_redirect_target(code).unwrap_or(code)) {
                        Some(rule.noqa_code())
                    } else {
                        warn!("Invalid code provided to `# ruff: noqa`: {}", code);
                        None
                    }
                }));
            }
            Exemption::None => {}
        }
    }

    // Mark any non-ignored diagnostics.
    for diagnostic in diagnostics {
        // If the file is exempted, don't add any noqa directives.
        if file_exempted {
            continue;
        }

        // If the diagnostic is ignored by a global exemption, don't add a noqa directive.
        if !file_exemptions.is_empty() {
            if file_exemptions.contains(&diagnostic.kind.rule().noqa_code()) {
                continue;
            }
        }

        // Is the violation ignored by a `noqa` directive on the parent line?
        if let Some(parent_lineno) = diagnostic.parent.map(|location| location.row()) {
            let noqa_lineno = noqa_line_for.get(&parent_lineno).unwrap_or(&parent_lineno);
            if commented_lines.contains(noqa_lineno) {
                match extract_noqa_directive(lines[noqa_lineno - 1]) {
                    Directive::All(..) => {
                        continue;
                    }
                    Directive::Codes(.., codes) => {
                        if includes(diagnostic.kind.rule(), &codes) {
                            continue;
                        }
                    }
                    Directive::None => {}
                }
            }
        }

        // Is the diagnostic ignored by a `noqa` directive on the same line?
        let diagnostic_lineno = diagnostic.location.row();
        let noqa_lineno = noqa_line_for
            .get(&diagnostic_lineno)
            .unwrap_or(&diagnostic_lineno);
        if commented_lines.contains(noqa_lineno) {
            match extract_noqa_directive(lines[noqa_lineno - 1]) {
                Directive::All(..) => {
                    continue;
                }
                Directive::Codes(.., codes) => {
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
    for (lineno, line) in contents.lines().enumerate() {
        match matches_by_line.get(&lineno) {
            None => {
                output.push_str(line);
                output.push_str(line_ending);
            }
            Some(rules) => {
                match extract_noqa_directive(line) {
                    Directive::None => {
                        // Add existing content.
                        output.push_str(line.trim_end());

                        // Add `noqa` directive.
                        output.push_str("  # noqa: ");

                        // Add codes.
                        push_codes(&mut output, rules.iter().map(|r| r.noqa_code()));
                        output.push_str(line_ending);
                        count += 1;
                    }
                    Directive::All(..) => {
                        // Leave the line as-is.
                        output.push_str(line);
                        output.push_str(line_ending);
                    }
                    Directive::Codes(_, start_byte, _, existing) => {
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
                        output.push_str(line_ending);

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
        let _ = write!(str, "{}", code);
        first = false;
    }
}

#[cfg(test)]
mod tests {
    use nohash_hasher::IntMap;
    use rustpython_parser::ast::Location;

    use crate::ast::types::Range;
    use crate::noqa::{add_noqa_inner, NOQA_LINE_REGEX};
    use crate::registry::Diagnostic;
    use crate::rules::pycodestyle::rules::AmbiguousVariableName;
    use crate::rules::pyflakes;
    use crate::source_code::LineEnding;

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
            &LineEnding::Lf,
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
            &LineEnding::Lf,
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
            &LineEnding::Lf,
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
            &LineEnding::Lf,
        );
        assert_eq!(count, 0);
        assert_eq!(output, "x = 1  # noqa\n");
    }
}
