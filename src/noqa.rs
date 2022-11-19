use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::Result;
use nohash_hasher::IntMap;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::checks::{Check, CheckCode};

static NO_QA_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?P<spaces>\s*)(?P<noqa>(?i:# noqa)(?::\s?(?P<codes>([A-Z]+[0-9]+(?:[,\s]+)?)+))?)",
    )
    .expect("Invalid regex")
});
static SPLIT_COMMA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,\s]").expect("Invalid regex"));

#[derive(Debug)]
pub enum Directive<'a> {
    None,
    All(usize, usize, usize),
    Codes(usize, usize, usize, Vec<&'a str>),
}

pub fn extract_noqa_directive(line: &str) -> Directive {
    match NO_QA_REGEX.captures(line) {
        Some(caps) => match caps.name("spaces") {
            Some(spaces) => match caps.name("noqa") {
                Some(noqa) => match caps.name("codes") {
                    Some(codes) => Directive::Codes(
                        spaces.as_str().chars().count(),
                        noqa.start(),
                        noqa.end(),
                        SPLIT_COMMA_REGEX
                            .split(codes.as_str())
                            .map(|code| code.trim())
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

pub fn add_noqa(
    checks: &[Check],
    contents: &str,
    noqa_line_for: &IntMap<usize, usize>,
    path: &Path,
) -> Result<usize> {
    let (count, output) = add_noqa_inner(checks, contents, noqa_line_for)?;
    fs::write(path, output)?;
    Ok(count)
}

fn add_noqa_inner(
    checks: &[Check],
    contents: &str,
    noqa_line_for: &IntMap<usize, usize>,
) -> Result<(usize, String)> {
    let lines: Vec<&str> = contents.lines().collect();
    let mut matches_by_line: BTreeMap<usize, BTreeSet<&CheckCode>> = BTreeMap::new();
    for lineno in 0..lines.len() {
        let mut codes: BTreeSet<&CheckCode> = BTreeSet::new();
        for check in checks {
            if check.location.row() == lineno + 1 {
                codes.insert(check.kind.code());
            }
        }

        // Grab the noqa (logical) line number for the current (physical) line.
        // If there are newlines at the end of the file, they won't be represented in
        // `noqa_line_for`, so fallback to the current line.
        let noqa_lineno = noqa_line_for.get(&(lineno + 1)).unwrap_or(&(lineno + 1)) - 1;

        if !codes.is_empty() {
            let matches = matches_by_line.entry(noqa_lineno).or_default();
            matches.append(&mut codes);
        }
    }

    let mut count: usize = 0;
    let mut output = "".to_string();
    for (lineno, line) in lines.iter().enumerate() {
        match matches_by_line.get(&lineno) {
            None => {
                output.push_str(line);
                output.push('\n');
            }
            Some(codes) => {
                match extract_noqa_directive(line) {
                    Directive::None => {
                        output.push_str(line);
                        output.push_str("  # noqa: ");
                    }
                    Directive::All(_, start, _) | Directive::Codes(_, start, ..) => {
                        output.push_str(&line[..start]);
                        output.push_str("# noqa: ");
                    }
                };
                let codes: Vec<&str> = codes.iter().map(|code| code.as_ref()).collect();
                output.push_str(&codes.join(", "));
                output.push('\n');
                count += 1;
            }
        }
    }

    Ok((count, output))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser::ast::Location;

    use crate::ast::types::Range;
    use crate::checks::{Check, CheckKind};
    use crate::noqa::{add_noqa_inner, NO_QA_REGEX};

    #[test]
    fn regex() {
        assert!(NO_QA_REGEX.is_match("# noqa"));
        assert!(NO_QA_REGEX.is_match("# NoQA"));

        assert!(NO_QA_REGEX.is_match("# noqa: F401"));
        assert!(NO_QA_REGEX.is_match("# NoQA: F401"));
        assert!(NO_QA_REGEX.is_match("# noqa: F401, E501"));

        assert!(NO_QA_REGEX.is_match("# noqa:F401"));
        assert!(NO_QA_REGEX.is_match("# NoQA:F401"));
        assert!(NO_QA_REGEX.is_match("# noqa:F401, E501"));
    }

    #[test]
    fn modification() -> Result<()> {
        let checks = vec![];
        let contents = "x = 1";
        let noqa_line_for = Default::default();
        let (count, output) = add_noqa_inner(&checks, contents, &noqa_line_for)?;
        assert_eq!(count, 0);
        assert_eq!(output.trim(), contents.trim());

        let checks = vec![Check::new(
            CheckKind::UnusedVariable("x".to_string()),
            Range {
                location: Location::new(1, 0),
                end_location: Location::new(1, 0),
            },
        )];
        let contents = "x = 1";
        let noqa_line_for = Default::default();
        let (count, output) = add_noqa_inner(&checks, contents, &noqa_line_for)?;
        assert_eq!(count, 1);
        assert_eq!(output.trim(), "x = 1  # noqa: F841".trim());

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
        let contents = "x = 1  # noqa: E741";
        let noqa_line_for = Default::default();
        let (count, output) = add_noqa_inner(&checks, contents, &noqa_line_for)?;
        assert_eq!(count, 1);
        assert_eq!(output.trim(), "x = 1  # noqa: E741, F841".trim());

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
        let noqa_line_for = Default::default();
        let (count, output) = add_noqa_inner(&checks, contents, &noqa_line_for)?;
        assert_eq!(count, 1);
        assert_eq!(output.trim(), "x = 1  # noqa: E741, F841".trim());

        Ok(())
    }
}
