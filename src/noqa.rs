use std::cmp::{max, min};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::lexer::{LexResult, Tok};

use crate::checks::{Check, CheckCode};

static NO_QA_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?P<noqa>\s*# noqa(?::\s?(?P<codes>([A-Z]+[0-9]+(?:[,\s]+)?)+))?)")
        .expect("Invalid regex")
});
static SPLIT_COMMA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,\s]").expect("Invalid regex"));

#[derive(Debug)]
pub enum Directive<'a> {
    None,
    All(usize, usize),
    Codes(usize, usize, Vec<&'a str>),
}

pub fn extract_noqa_directive(line: &str) -> Directive {
    match NO_QA_REGEX.captures(line) {
        Some(caps) => match caps.name("noqa") {
            Some(noqa) => match caps.name("codes") {
                Some(codes) => Directive::Codes(
                    noqa.start(),
                    noqa.end(),
                    SPLIT_COMMA_REGEX
                        .split(codes.as_str())
                        .map(|code| code.trim())
                        .filter(|code| !code.is_empty())
                        .collect(),
                ),
                None => Directive::All(noqa.start(), noqa.end()),
            },
            None => Directive::None,
        },
        None => Directive::None,
    }
}

pub fn extract_noqa_line_for(lxr: &[LexResult]) -> Vec<usize> {
    let mut noqa_line_for: Vec<usize> = vec![];

    let mut min_line = usize::MAX;
    let mut max_line = usize::MIN;
    let mut in_string = false;

    for (start, tok, end) in lxr.iter().flatten() {
        if matches!(tok, Tok::EndOfFile) {
            break;
        }

        if matches!(tok, Tok::Newline) {
            min_line = min(min_line, start.row());
            max_line = max(max_line, start.row());

            // For now, we only care about preserving noqa directives across multi-line strings.
            if in_string {
                for i in (noqa_line_for.len())..(min_line - 1) {
                    noqa_line_for.push(i + 1);
                }
                noqa_line_for.extend(vec![max_line; (max_line + 1) - min_line]);
            }

            min_line = usize::MAX;
            max_line = usize::MIN;
        } else {
            min_line = min(min_line, start.row());
            max_line = max(max_line, end.row());
        }
        in_string = matches!(tok, Tok::String { .. });
    }

    noqa_line_for
}

fn add_noqa_inner(
    checks: &Vec<Check>,
    contents: &str,
    noqa_line_for: &[usize],
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
        let noqa_lineno = noqa_line_for
            .get(lineno)
            .map(|lineno| lineno - 1)
            .unwrap_or(lineno);

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
                    }
                    Directive::All(start, _) => output.push_str(&line[..start]),
                    Directive::Codes(start, _, _) => output.push_str(&line[..start]),
                };
                let codes: Vec<&str> = codes.iter().map(|code| code.as_str()).collect();
                output.push_str("  # noqa: ");
                output.push_str(&codes.join(", "));
                output.push('\n');
                count += 1;
            }
        }
    }

    Ok((count, output))
}

pub fn add_noqa(
    checks: &Vec<Check>,
    contents: &str,
    noqa_line_for: &[usize],
    path: &Path,
) -> Result<usize> {
    let (count, output) = add_noqa_inner(checks, contents, noqa_line_for)?;
    fs::write(path, output)?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use crate::ast::types::Range;
    use anyhow::Result;
    use rustpython_parser::ast::Location;
    use rustpython_parser::lexer;
    use rustpython_parser::lexer::LexResult;

    use crate::checks::{Check, CheckKind};
    use crate::noqa::{add_noqa_inner, extract_noqa_line_for};

    #[test]
    fn extraction() -> Result<()> {
        let empty: Vec<usize> = Default::default();

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
y = 2
z = x + 1",
        )
        .collect();
        println!("{:?}", extract_noqa_line_for(&lxr));
        assert_eq!(extract_noqa_line_for(&lxr), empty);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "
x = 1
y = 2
z = x + 1",
        )
        .collect();
        println!("{:?}", extract_noqa_line_for(&lxr));
        assert_eq!(extract_noqa_line_for(&lxr), empty);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
y = 2
z = x + 1
        ",
        )
        .collect();
        println!("{:?}", extract_noqa_line_for(&lxr));
        assert_eq!(extract_noqa_line_for(&lxr), empty);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1

y = 2
z = x + 1
        ",
        )
        .collect();
        println!("{:?}", extract_noqa_line_for(&lxr));
        assert_eq!(extract_noqa_line_for(&lxr), empty);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = '''abc
def
ghi
'''
y = 2
z = x + 1",
        )
        .collect();
        assert_eq!(extract_noqa_line_for(&lxr), vec![4, 4, 4, 4]);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
y = '''abc
def
ghi
'''
z = 2",
        )
        .collect();
        assert_eq!(extract_noqa_line_for(&lxr), vec![1, 5, 5, 5, 5]);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
y = '''abc
def
ghi
'''",
        )
        .collect();
        assert_eq!(extract_noqa_line_for(&lxr), vec![1, 5, 5, 5, 5]);

        Ok(())
    }

    #[test]
    fn modification() -> Result<()> {
        let checks = vec![];
        let contents = "x = 1";
        let noqa_line_for = vec![1];
        let (count, output) = add_noqa_inner(&checks, contents, &noqa_line_for)?;
        assert_eq!(count, 0);
        assert_eq!(output.trim(), contents.trim());

        let checks = vec![Check::new(
            CheckKind::UnusedVariable("x".to_string()),
            Range {
                location: Location::new(1, 1),
                end_location: Location::new(1, 1),
            },
        )];
        let contents = "x = 1";
        let noqa_line_for = vec![1];
        let (count, output) = add_noqa_inner(&checks, contents, &noqa_line_for)?;
        assert_eq!(count, 1);
        assert_eq!(output.trim(), "x = 1  # noqa: F841".trim());

        let checks = vec![
            Check::new(
                CheckKind::AmbiguousVariableName("x".to_string()),
                Range {
                    location: Location::new(1, 1),
                    end_location: Location::new(1, 1),
                },
            ),
            Check::new(
                CheckKind::UnusedVariable("x".to_string()),
                Range {
                    location: Location::new(1, 1),
                    end_location: Location::new(1, 1),
                },
            ),
        ];
        let contents = "x = 1  # noqa: E741";
        let noqa_line_for = vec![1];
        let (count, output) = add_noqa_inner(&checks, contents, &noqa_line_for)?;
        assert_eq!(count, 1);
        assert_eq!(output.trim(), "x = 1  # noqa: E741, F841".trim());

        let checks = vec![
            Check::new(
                CheckKind::AmbiguousVariableName("x".to_string()),
                Range {
                    location: Location::new(1, 1),
                    end_location: Location::new(1, 1),
                },
            ),
            Check::new(
                CheckKind::UnusedVariable("x".to_string()),
                Range {
                    location: Location::new(1, 1),
                    end_location: Location::new(1, 1),
                },
            ),
        ];
        let contents = "x = 1  # noqa";
        let noqa_line_for = vec![1];
        let (count, output) = add_noqa_inner(&checks, contents, &noqa_line_for)?;
        assert_eq!(count, 1);
        assert_eq!(output.trim(), "x = 1  # noqa: E741, F841".trim());

        Ok(())
    }
}
