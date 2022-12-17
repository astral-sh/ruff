//! Extract `# noqa` and `# isort: skip` directives from tokenized source.

use bitflags::bitflags;
use nohash_hasher::{IntMap, IntSet};
use rustpython_ast::Location;
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::types::Range;
use crate::checks::LintSource;
use crate::{Settings, SourceCodeLocator};

bitflags! {
    pub struct Flags: u32 {
        const NOQA = 0b0000_0001;
        const ISORT = 0b0000_0010;
    }
}

impl Flags {
    pub fn from_settings(settings: &Settings) -> Self {
        if settings
            .enabled
            .iter()
            .any(|check_code| matches!(check_code.lint_source(), LintSource::Imports))
        {
            Flags::NOQA | Flags::ISORT
        } else {
            Flags::NOQA
        }
    }
}

#[derive(Default)]
pub struct IsortDirectives {
    pub exclusions: IntSet<usize>,
    pub splits: Vec<usize>,
}

pub struct Directives {
    pub commented_lines: Vec<usize>,
    pub noqa_line_for: IntMap<usize, usize>,
    pub isort: IsortDirectives,
}

pub fn extract_directives(
    lxr: &[LexResult],
    locator: &SourceCodeLocator,
    flags: Flags,
) -> Directives {
    Directives {
        commented_lines: extract_commented_lines(lxr),
        noqa_line_for: if flags.contains(Flags::NOQA) {
            extract_noqa_line_for(lxr)
        } else {
            IntMap::default()
        },
        isort: if flags.contains(Flags::ISORT) {
            extract_isort_directives(lxr, locator)
        } else {
            IsortDirectives::default()
        },
    }
}

pub fn extract_commented_lines(lxr: &[LexResult]) -> Vec<usize> {
    let mut commented_lines = Vec::new();
    for (start, tok, ..) in lxr.iter().flatten() {
        if matches!(tok, Tok::Comment) {
            commented_lines.push(start.row());
        }
    }
    commented_lines
}

/// Extract a mapping from logical line to noqa line.
pub fn extract_noqa_line_for(lxr: &[LexResult]) -> IntMap<usize, usize> {
    let mut noqa_line_for: IntMap<usize, usize> = IntMap::default();
    for (start, tok, end) in lxr.iter().flatten() {
        if matches!(tok, Tok::EndOfFile) {
            break;
        }
        // For multi-line strings, we expect `noqa` directives on the last line of the
        // string.
        if matches!(tok, Tok::String { .. }) && end.row() > start.row() {
            for i in start.row()..end.row() {
                noqa_line_for.insert(i, end.row());
            }
        }
    }
    noqa_line_for
}

/// Extract a set of lines over which to disable isort.
pub fn extract_isort_directives(lxr: &[LexResult], locator: &SourceCodeLocator) -> IsortDirectives {
    let mut exclusions: IntSet<usize> = IntSet::default();
    let mut splits: Vec<usize> = Vec::default();
    let mut skip_file: bool = false;
    let mut off: Option<Location> = None;
    let mut last: Option<Location> = None;
    for &(start, ref tok, end) in lxr.iter().flatten() {
        last = Some(end);

        // No need to keep processing, but we do need to determine the last token.
        if skip_file {
            continue;
        }

        if !matches!(tok, Tok::Comment) {
            continue;
        }

        // TODO(charlie): Modify RustPython to include the comment text in the token.
        let comment_text = locator.slice_source_code_range(&Range {
            location: start,
            end_location: end,
        });

        if comment_text == "# isort: split" {
            splits.push(start.row());
        } else if comment_text == "# isort: skip_file" {
            skip_file = true;
        } else if off.is_some() {
            if comment_text == "# isort: on" {
                if let Some(start) = off {
                    for row in start.row() + 1..=end.row() {
                        exclusions.insert(row);
                    }
                }
                off = None;
            }
        } else {
            if comment_text.contains("isort: skip") {
                exclusions.insert(start.row());
            } else if comment_text == "# isort: off" {
                off = Some(start);
            }
        }
    }
    if skip_file {
        // Enforce `isort: skip_file`.
        if let Some(end) = last {
            for row in 1..=end.row() {
                exclusions.insert(row);
            }
        }
    } else if let Some(start) = off {
        // Enforce unterminated `isort: off`.
        if let Some(end) = last {
            for row in start.row() + 1..=end.row() {
                exclusions.insert(row);
            }
        }
    }
    IsortDirectives { exclusions, splits }
}

#[cfg(test)]
mod tests {
    use nohash_hasher::{IntMap, IntSet};
    use rustpython_parser::lexer;
    use rustpython_parser::lexer::LexResult;

    use crate::directives::{extract_isort_directives, extract_noqa_line_for};
    use crate::SourceCodeLocator;

    #[test]
    fn noqa_extraction() {
        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
y = 2
z = x + 1",
        )
        .collect();
        assert_eq!(extract_noqa_line_for(&lxr), IntMap::default());

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "
x = 1
y = 2
z = x + 1",
        )
        .collect();
        assert_eq!(extract_noqa_line_for(&lxr), IntMap::default());

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
y = 2
z = x + 1
        ",
        )
        .collect();
        assert_eq!(extract_noqa_line_for(&lxr), IntMap::default());

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1

y = 2
z = x + 1
        ",
        )
        .collect();
        assert_eq!(extract_noqa_line_for(&lxr), IntMap::default());

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = '''abc
def
ghi
'''
y = 2
z = x + 1",
        )
        .collect();
        assert_eq!(
            extract_noqa_line_for(&lxr),
            IntMap::from_iter([(1, 4), (2, 4), (3, 4)])
        );

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
        y = '''abc
        def
        ghi
        '''
        z = 2",
        )
        .collect();
        assert_eq!(
            extract_noqa_line_for(&lxr),
            IntMap::from_iter([(2, 5), (3, 5), (4, 5)])
        );

        let lxr: Vec<LexResult> = lexer::make_tokenizer(
            "x = 1
        y = '''abc
        def
        ghi
        '''",
        )
        .collect();
        assert_eq!(
            extract_noqa_line_for(&lxr),
            IntMap::from_iter([(2, 5), (3, 5), (4, 5)])
        );
    }

    #[test]
    fn isort_exclusions() {
        let contents = "x = 1
y = 2
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            extract_isort_directives(&lxr, &locator).exclusions,
            IntSet::default()
        );

        let contents = "# isort: off
x = 1
y = 2
# isort: on
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            extract_isort_directives(&lxr, &locator).exclusions,
            IntSet::from_iter([2, 3, 4])
        );

        let contents = "# isort: off
x = 1
# isort: off
y = 2
# isort: on
z = x + 1
# isort: on";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            extract_isort_directives(&lxr, &locator).exclusions,
            IntSet::from_iter([2, 3, 4, 5])
        );

        let contents = "# isort: off
x = 1
y = 2
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            extract_isort_directives(&lxr, &locator).exclusions,
            IntSet::from_iter([2, 3, 4])
        );

        let contents = "# isort: skip_file
x = 1
y = 2
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            extract_isort_directives(&lxr, &locator).exclusions,
            IntSet::from_iter([1, 2, 3, 4])
        );

        let contents = "# isort: off
x = 1
# isort: on
y = 2
# isort: skip_file
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            extract_isort_directives(&lxr, &locator).exclusions,
            IntSet::from_iter([1, 2, 3, 4, 5, 6])
        );
    }

    #[test]
    fn isort_splits() {
        let contents = "x = 1
y = 2
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(
            extract_isort_directives(&lxr, &locator).splits,
            Vec::<usize>::new()
        );

        let contents = "x = 1
y = 2
# isort: split
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(extract_isort_directives(&lxr, &locator).splits, vec![3]);

        let contents = "x = 1
y = 2  # isort: split
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let locator = SourceCodeLocator::new(contents);
        assert_eq!(extract_isort_directives(&lxr, &locator).splits, vec![2]);
    }
}
