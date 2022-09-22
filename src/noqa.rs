use std::cmp::{max, min};

use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::lexer::{LexResult, Tok};

static NO_QA_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?P<noqa># noqa(?::\s?(?P<codes>([A-Z]+[0-9]+(?:[,\s]+)?)+))?)")
        .expect("Invalid regex")
});
static SPLIT_COMMA_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,\s]").expect("Invalid regex"));

#[derive(Debug)]
pub enum Directive<'a> {
    None,
    All(usize),
    Codes(usize, Vec<&'a str>),
}

pub fn extract_noqa_directive(line: &str) -> Directive {
    match NO_QA_REGEX.captures(line) {
        Some(caps) => match caps.name("noqa") {
            Some(noqa) => match caps.name("codes") {
                Some(codes) => Directive::Codes(
                    noqa.start(),
                    SPLIT_COMMA_REGEX
                        .split(codes.as_str())
                        .map(|code| code.trim())
                        .filter(|code| !code.is_empty())
                        .collect(),
                ),
                None => Directive::All(noqa.start()),
            },
            None => Directive::None,
        },
        None => Directive::None,
    }
}

pub fn extract_noqa_line_for(lxr: &[LexResult]) -> Vec<usize> {
    let mut line_map: Vec<usize> = vec![];

    let mut last_is_string = false;
    let mut last_seen = usize::MIN;
    let mut min_line = usize::MAX;
    let mut max_line = usize::MIN;

    for (start, tok, end) in lxr.iter().flatten() {
        if matches!(tok, Tok::EndOfFile) {
            break;
        }

        if matches!(tok, Tok::Newline) {
            min_line = min(min_line, start.row());
            max_line = max(max_line, start.row());

            // For now, we only care about preserving noqa directives across multi-line strings.
            if last_is_string {
                line_map.extend(vec![max_line; (max_line + 1) - min_line]);
            } else {
                for i in (min_line - 1)..(max_line) {
                    line_map.push(i + 1);
                }
            }

            min_line = usize::MAX;
            max_line = usize::MIN;
        } else {
            // Handle empty lines.
            if start.row() > last_seen {
                for i in last_seen..(start.row() - 1) {
                    line_map.push(i + 1);
                }
            }

            min_line = min(min_line, start.row());
            max_line = max(max_line, end.row());
        }
        last_seen = start.row();
        last_is_string = matches!(tok, Tok::String { .. });
    }

    line_map
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser::lexer;
    use rustpython_parser::lexer::LexResult;

    use crate::noqa::extract_noqa_line_for;

    #[test]
    fn line_map() -> Result<()> {
        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
y = 2
z = x + 1",
        )
        .collect();
        println!("{:?}", extract_noqa_line_for(&lxr));
        assert_eq!(extract_noqa_line_for(&lxr), vec![1, 2, 3]);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "
x = 1
y = 2
z = x + 1",
        )
        .collect();
        println!("{:?}", extract_noqa_line_for(&lxr));
        assert_eq!(extract_noqa_line_for(&lxr), vec![1, 2, 3, 4]);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
y = 2
z = x + 1
        ",
        )
        .collect();
        println!("{:?}", extract_noqa_line_for(&lxr));
        assert_eq!(extract_noqa_line_for(&lxr), vec![1, 2, 3]);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1

y = 2
z = x + 1
        ",
        )
        .collect();
        println!("{:?}", extract_noqa_line_for(&lxr));
        assert_eq!(extract_noqa_line_for(&lxr), vec![1, 2, 3, 4]);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = '''abc
def
ghi
'''
y = 2
z = x + 1",
        )
        .collect();
        assert_eq!(extract_noqa_line_for(&lxr), vec![4, 4, 4, 4, 5, 6]);

        Ok(())
    }
}
