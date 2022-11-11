//! Extract `# noqa` and `# isort: skip` directives from tokenized source.

use nohash_hasher::IntSet;
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::Location;
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::types::Range;
use crate::SourceCodeLocator;

static ISORT_SKIP_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"isort:\s?skip").expect("Invalid regex"));
static ISORT_OFF_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^# isort:\s?off$").expect("Invalid regex"));
static ISORT_ON_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^# isort:\s?on$").expect("Invalid regex"));

pub struct Directives {
    // TODO(charlie): Benchmark use of IntMap.
    pub noqa_line_for: Vec<usize>,
    pub isort_exclusions: IntSet<usize>,
}

pub fn extract_directives(lxr: &[LexResult], locator: &SourceCodeLocator) -> Directives {
    Directives {
        // TODO(charlie): Compute these in one pass.
        noqa_line_for: extract_noqa_line_for(lxr),
        // TODO(charlie): Skip if `isort` is disabled.
        isort_exclusions: extract_isort_exclusions(lxr, locator),
    }
}

/// Extract a mapping from logical line to noqa line.
pub fn extract_noqa_line_for(lxr: &[LexResult]) -> Vec<usize> {
    let mut noqa_line_for: Vec<usize> = vec![];
    for (start, tok, end) in lxr.iter().flatten() {
        if matches!(tok, Tok::EndOfFile) {
            break;
        }
        // For multi-line strings, we expect `noqa` directives on the last line of the
        // string. By definition, we can't have multiple multi-line strings on
        // the same line, so we don't need to verify that we haven't already
        // traversed past the current line.
        if matches!(tok, Tok::String { .. }) && end.row() > start.row() {
            for i in (noqa_line_for.len())..(start.row() - 1) {
                noqa_line_for.push(i + 1);
            }
            noqa_line_for.extend(vec![end.row(); (end.row() + 1) - start.row()]);
        }
    }
    noqa_line_for
}

/// Extract a set of lines over which to disable isort.
pub fn extract_isort_exclusions(lxr: &[LexResult], locator: &SourceCodeLocator) -> IntSet<usize> {
    let mut exclusions: IntSet<usize> = IntSet::default();
    let mut off: Option<&Location> = None;
    for (start, tok, end) in lxr.iter().flatten() {
        if matches!(tok, Tok::Comment) {
            let comment_text = locator.slice_source_code_range(&Range {
                location: *start,
                end_location: *end,
            });
            if off.is_some() {
                if ISORT_ON_REGEX.is_match(&comment_text) {
                    if let Some(start) = off {
                        for row in start.row() + 1..=end.row() {
                            exclusions.insert(row);
                        }
                    }
                    off = None;
                }
            } else {
                if ISORT_SKIP_REGEX.is_match(&comment_text) {
                    exclusions.insert(start.row());
                } else if ISORT_OFF_REGEX.is_match(&comment_text) {
                    off = Some(start);
                }
            }
        } else if matches!(tok, Tok::EndOfFile) {
            if let Some(start) = off {
                for row in start.row() + 1..=end.row() {
                    exclusions.insert(row);
                }
            }
            break;
        }
    }
    exclusions
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser::lexer;
    use rustpython_parser::lexer::LexResult;

    use crate::directives::extract_noqa_line_for;

    #[test]
    fn extraction() -> Result<()> {
        let empty: Vec<usize> = Default::default();

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
y = 2
z = x + 1",
        )
        .collect();
        assert_eq!(extract_noqa_line_for(&lxr), empty);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "
x = 1
y = 2
z = x + 1",
        )
        .collect();
        assert_eq!(extract_noqa_line_for(&lxr), empty);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
y = 2
z = x + 1
        ",
        )
        .collect();
        assert_eq!(extract_noqa_line_for(&lxr), empty);

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1

y = 2
z = x + 1
        ",
        )
        .collect();
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
}
