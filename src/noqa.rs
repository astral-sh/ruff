use std::fs;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
use nohash_hasher::IntMap;
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::checks::{Check, CheckCode, CODE_REDIRECTS};
use crate::source_code_style::LineEnding;

static NOQA_LINE_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?P<spaces>\s*)(?P<noqa>(?i:# noqa)(?::\s?(?P<codes>([A-Z]+[0-9]+(?:[,\s]+)?)+))?)",
    )
    .unwrap()
});
static SPLIT_COMMA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,\s]").unwrap());

/// Return `true` if a file is exempt from checking based on the contents of the
/// given line.
pub fn is_file_exempt(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("# flake8: noqa")
        || line.starts_with("# flake8: NOQA")
        || line.starts_with("# flake8: NoQA")
        || line.starts_with("# ruff: noqa")
        || line.starts_with("# ruff: NOQA")
        || line.starts_with("# ruff: NoQA")
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
                    Some(codes) => Directive::Codes(
                        spaces.as_str().chars().count(),
                        noqa.start(),
                        noqa.end(),
                        SPLIT_COMMA_REGEX
                            .split(codes.as_str())
                            .map(str::trim)
                            .filter(|code| !code.is_empty())
                            .collect(),
                    ),
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
pub fn includes(needle: &CheckCode, haystack: &[&str]) -> bool {
    let needle: &str = needle.as_ref();
    haystack.iter().any(|candidate| {
        if let Some(candidate) = CODE_REDIRECTS.get(candidate) {
            needle == candidate.as_ref()
        } else {
            &needle == candidate
        }
    })
}

pub fn add_noqa(
    path: &Path,
    checks: &[Check],
    contents: &str,
    noqa_line_for: &IntMap<usize, usize>,
    external: &FxHashSet<String>,
    line_ending: &LineEnding,
) -> Result<usize> {
    let (count, output) = add_noqa_inner(checks, contents, noqa_line_for, external, line_ending);
    fs::write(path, output)?;
    Ok(count)
}

fn add_noqa_inner(
    checks: &[Check],
    contents: &str,
    noqa_line_for: &IntMap<usize, usize>,
    external: &FxHashSet<String>,
    line_ending: &LineEnding,
) -> (usize, String) {
    let mut matches_by_line: FxHashMap<usize, FxHashSet<&CheckCode>> = FxHashMap::default();
    for (lineno, line) in contents.lines().enumerate() {
        // If we hit an exemption for the entire file, bail.
        if is_file_exempt(line) {
            return (0, contents.to_string());
        }

        let mut codes: FxHashSet<&CheckCode> = FxHashSet::default();
        for check in checks {
            // TODO(charlie): Consider respecting parent `noqa` directives. For now, we'll
            // add a `noqa` for every check, on its own line. This could lead to
            // duplication, whereby some parent `noqa` directives become
            // redundant.
            if check.location.row() == lineno + 1 {
                codes.insert(check.kind.code());
            }
        }

        // Grab the noqa (logical) line number for the current (physical) line.
        let noqa_lineno = noqa_line_for.get(&(lineno + 1)).unwrap_or(&(lineno + 1)) - 1;

        if !codes.is_empty() {
            matches_by_line
                .entry(noqa_lineno)
                .or_default()
                .extend(codes);
        }
    }

    let mut count: usize = 0;
    let mut output = String::new();
    for (lineno, line) in contents.lines().enumerate() {
        match matches_by_line.get(&lineno) {
            None => {
                output.push_str(line);
                output.push_str(line_ending);
            }
            Some(codes) => {
                match extract_noqa_directive(line) {
                    Directive::None => {
                        // Add existing content.
                        output.push_str(line.trim_end());

                        // Add `noqa` directive.
                        output.push_str("  # noqa: ");

                        // Add codes.
                        let codes: Vec<&str> = codes.iter().map(AsRef::as_ref).collect();
                        let suffix = codes.join(", ");
                        output.push_str(&suffix);
                        output.push_str(line_ending);
                        count += 1;
                    }
                    Directive::All(_, start, _) => {
                        // Add existing content.
                        output.push_str(line[..start].trim_end());

                        // Add `noqa` directive.
                        output.push_str("  # noqa: ");

                        // Add codes.
                        let codes: Vec<&str> =
                            codes.iter().map(AsRef::as_ref).sorted_unstable().collect();
                        let suffix = codes.join(", ");
                        output.push_str(&suffix);
                        output.push_str(line_ending);
                        count += 1;
                    }
                    Directive::Codes(_, start, _, existing) => {
                        // Reconstruct the line based on the preserved check codes.
                        // This enables us to tally the number of edits.
                        let mut formatted = String::new();

                        // Add existing content.
                        formatted.push_str(line[..start].trim_end());

                        // Add `noqa` directive.
                        formatted.push_str("  # noqa: ");

                        // Add codes.
                        let codes: Vec<&str> = codes
                            .iter()
                            .map(AsRef::as_ref)
                            .chain(existing.into_iter().filter(|code| external.contains(*code)))
                            .sorted_unstable()
                            .collect();
                        let suffix = codes.join(", ");
                        formatted.push_str(&suffix);

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

#[cfg(test)]
mod tests {

    use nohash_hasher::IntMap;
    use rustc_hash::FxHashSet;
    use rustpython_parser::ast::Location;

    use crate::ast::types::Range;
    use crate::checks::{Check, CheckKind};
    use crate::noqa::{add_noqa_inner, NOQA_LINE_REGEX};
    use crate::source_code_style::LineEnding;

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
        let checks = vec![];
        let contents = "x = 1";
        let noqa_line_for = IntMap::default();
        let external = FxHashSet::default();
        let (count, output) = add_noqa_inner(
            &checks,
            contents,
            &noqa_line_for,
            &external,
            &LineEnding::Lf,
        );
        assert_eq!(count, 0);
        assert_eq!(output, format!("{contents}\n"));

        let checks = vec![Check::new(
            CheckKind::UnusedVariable("x".to_string()),
            Range {
                location: Location::new(1, 0),
                end_location: Location::new(1, 0),
            },
        )];
        let contents = "x = 1";
        let noqa_line_for = IntMap::default();
        let external = FxHashSet::default();
        let (count, output) = add_noqa_inner(
            &checks,
            contents,
            &noqa_line_for,
            &external,
            &LineEnding::Lf,
        );
        assert_eq!(count, 1);
        assert_eq!(output, "x = 1  # noqa: F841\n");

        let checks = vec![
            Check::new(
                CheckKind::AmbiguousVariableName("x".to_string()),
                Range {
                    location: Location::new(1, 0),
                    end_location: Location::new(1, 0),
                },
            ),
            Check::new(
                CheckKind::UnusedVariable("x".to_string()),
                Range {
                    location: Location::new(1, 0),
                    end_location: Location::new(1, 0),
                },
            ),
        ];
        let contents = "x = 1  # noqa: E741\n";
        let noqa_line_for = IntMap::default();
        let external = FxHashSet::default();
        let (count, output) = add_noqa_inner(
            &checks,
            contents,
            &noqa_line_for,
            &external,
            &LineEnding::Lf,
        );
        assert_eq!(count, 1);
        assert_eq!(output, "x = 1  # noqa: E741, F841\n");

        let checks = vec![
            Check::new(
                CheckKind::AmbiguousVariableName("x".to_string()),
                Range {
                    location: Location::new(1, 0),
                    end_location: Location::new(1, 0),
                },
            ),
            Check::new(
                CheckKind::UnusedVariable("x".to_string()),
                Range {
                    location: Location::new(1, 0),
                    end_location: Location::new(1, 0),
                },
            ),
        ];
        let contents = "x = 1  # noqa";
        let noqa_line_for = IntMap::default();
        let external = FxHashSet::default();
        let (count, output) = add_noqa_inner(
            &checks,
            contents,
            &noqa_line_for,
            &external,
            &LineEnding::Lf,
        );
        assert_eq!(count, 1);
        assert_eq!(output, "x = 1  # noqa: E741, F841\n");
    }
}
