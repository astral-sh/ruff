//! Extract `# noqa` and `# isort: skip` directives from tokenized source.

use bitflags::bitflags;
use nohash_hasher::{IntMap, IntSet};
use rustpython_parser::ast::Location;
use rustpython_parser::lexer::{LexResult, Tok};

use crate::registry::LintSource;
use crate::settings::Settings;

bitflags! {
    pub struct Flags: u32 {
        const NOQA = 0b0000_0001;
        const ISORT = 0b0000_0010;
    }
}

impl Flags {
    pub fn from_settings(settings: &Settings) -> Self {
        if settings
            .rules
            .iter_enabled()
            .any(|rule_code| matches!(rule_code.lint_source(), LintSource::Imports))
        {
            Self::NOQA | Self::ISORT
        } else {
            Self::NOQA
        }
    }
}

#[derive(Default)]
pub struct IsortDirectives {
    pub exclusions: IntSet<usize>,
    pub splits: Vec<usize>,
    pub skip_file: bool,
}

pub struct Directives {
    pub noqa_line_for: IntMap<usize, usize>,
    pub isort: IsortDirectives,
}

pub fn extract_directives(lxr: &[LexResult], flags: Flags) -> Directives {
    Directives {
        noqa_line_for: if flags.contains(Flags::NOQA) {
            extract_noqa_line_for(lxr)
        } else {
            IntMap::default()
        },
        isort: if flags.contains(Flags::ISORT) {
            extract_isort_directives(lxr)
        } else {
            IsortDirectives::default()
        },
    }
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
pub fn extract_isort_directives(lxr: &[LexResult]) -> IsortDirectives {
    let mut exclusions: IntSet<usize> = IntSet::default();
    let mut splits: Vec<usize> = Vec::default();
    let mut off: Option<Location> = None;
    let mut last: Option<Location> = None;
    for &(start, ref tok, end) in lxr.iter().flatten() {
        last = Some(end);

        let Tok::Comment(comment_text) = tok else {
            continue;
        };

        // `isort` allows for `# isort: skip` and `# isort: skip_file` to include or
        // omit a space after the colon. The remaining action comments are
        // required to include the space, and must appear on their own lines.
        let comment_text = comment_text.trim_end();
        if comment_text == "# isort: split" {
            splits.push(start.row());
        } else if comment_text == "# isort: skip_file" || comment_text == "# isort:skip_file" {
            return IsortDirectives {
                skip_file: true,
                ..IsortDirectives::default()
            };
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
            if comment_text.contains("isort: skip") || comment_text.contains("isort:skip") {
                exclusions.insert(start.row());
            } else if comment_text == "# isort: off" {
                off = Some(start);
            }
        }
    }

    if let Some(start) = off {
        // Enforce unterminated `isort: off`.
        if let Some(end) = last {
            for row in start.row() + 1..=end.row() {
                exclusions.insert(row);
            }
        }
    }
    IsortDirectives {
        exclusions,
        splits,
        ..IsortDirectives::default()
    }
}

#[cfg(test)]
mod tests {
    use nohash_hasher::{IntMap, IntSet};
    use rustpython_parser::lexer;
    use rustpython_parser::lexer::LexResult;

    use crate::directives::{extract_isort_directives, extract_noqa_line_for};

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
        assert_eq!(extract_isort_directives(&lxr).exclusions, IntSet::default());

        let contents = "# isort: off
x = 1
y = 2
# isort: on
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        assert_eq!(
            extract_isort_directives(&lxr).exclusions,
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
        assert_eq!(
            extract_isort_directives(&lxr).exclusions,
            IntSet::from_iter([2, 3, 4, 5])
        );

        let contents = "# isort: off
x = 1
y = 2
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        assert_eq!(
            extract_isort_directives(&lxr).exclusions,
            IntSet::from_iter([2, 3, 4])
        );

        let contents = "# isort: skip_file
x = 1
y = 2
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        assert_eq!(extract_isort_directives(&lxr).exclusions, IntSet::default());

        let contents = "# isort: off
x = 1
# isort: on
y = 2
# isort: skip_file
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        assert_eq!(extract_isort_directives(&lxr).exclusions, IntSet::default());
    }

    #[test]
    fn isort_splits() {
        let contents = "x = 1
y = 2
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        assert_eq!(extract_isort_directives(&lxr).splits, Vec::<usize>::new());

        let contents = "x = 1
y = 2
# isort: split
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        assert_eq!(extract_isort_directives(&lxr).splits, vec![3]);

        let contents = "x = 1
y = 2  # isort: split
z = x + 1";
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        assert_eq!(extract_isort_directives(&lxr).splits, vec![2]);
    }
}
