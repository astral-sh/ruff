//! Extract `# noqa`, `# isort: skip`, and `# TODO` directives from tokenized source.

use std::str::FromStr;

use bitflags::bitflags;
use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use ruff_python_index::Indexer;
use ruff_source_file::Locator;

use crate::noqa::NoqaMapping;
use crate::settings::Settings;

bitflags! {
    #[derive(Debug, Copy, Clone)]
    pub struct Flags: u8 {
        const NOQA  = 0b0000_0001;
        const ISORT = 0b0000_0010;
    }
}

impl Flags {
    pub fn from_settings(settings: &Settings) -> Self {
        if settings
            .rules
            .iter_enabled()
            .any(|rule_code| rule_code.lint_source().is_imports())
        {
            Self::NOQA | Self::ISORT
        } else {
            Self::NOQA
        }
    }
}

#[derive(Default, Debug)]
pub struct IsortDirectives {
    /// Ranges for which sorting is disabled
    pub exclusions: Vec<TextRange>,
    /// Text positions at which splits should be inserted
    pub splits: Vec<TextSize>,
    pub skip_file: bool,
}

impl IsortDirectives {
    pub fn is_excluded(&self, offset: TextSize) -> bool {
        for range in &self.exclusions {
            if range.contains(offset) {
                return true;
            }

            if range.start() > offset {
                break;
            }
        }

        false
    }
}

pub struct Directives {
    pub noqa_line_for: NoqaMapping,
    pub isort: IsortDirectives,
}

pub fn extract_directives(
    lxr: &[LexResult],
    flags: Flags,
    locator: &Locator,
    indexer: &Indexer,
) -> Directives {
    Directives {
        noqa_line_for: if flags.intersects(Flags::NOQA) {
            extract_noqa_line_for(lxr, locator, indexer)
        } else {
            NoqaMapping::default()
        },
        isort: if flags.intersects(Flags::ISORT) {
            extract_isort_directives(lxr, locator)
        } else {
            IsortDirectives::default()
        },
    }
}

/// Extract a mapping from logical line to noqa line.
fn extract_noqa_line_for(lxr: &[LexResult], locator: &Locator, indexer: &Indexer) -> NoqaMapping {
    let mut string_mappings = Vec::new();

    for (tok, range) in lxr.iter().flatten() {
        match tok {
            Tok::EndOfFile => {
                break;
            }

            // For multi-line strings, we expect `noqa` directives on the last line of the
            // string.
            Tok::String {
                triple_quoted: true,
                ..
            } => {
                if locator.contains_line_break(*range) {
                    string_mappings.push(TextRange::new(
                        locator.line_start(range.start()),
                        range.end(),
                    ));
                }
            }

            _ => {}
        }
    }

    let mut continuation_mappings = Vec::new();

    // For continuations, we expect `noqa` directives on the last line of the
    // continuation.
    let mut last: Option<TextRange> = None;
    for continuation_line in indexer.continuation_line_starts() {
        let line_end = locator.full_line_end(*continuation_line);
        if let Some(last_range) = last.take() {
            if last_range.end() == *continuation_line {
                last = Some(TextRange::new(last_range.start(), line_end));
                continue;
            }
            // new continuation
            continuation_mappings.push(last_range);
        }

        last = Some(TextRange::new(*continuation_line, line_end));
    }

    if let Some(last_range) = last.take() {
        continuation_mappings.push(last_range);
    }

    // Merge the mappings in sorted order
    let mut mappings =
        NoqaMapping::with_capacity(continuation_mappings.len() + string_mappings.len());

    let mut continuation_mappings = continuation_mappings.into_iter().peekable();
    let mut string_mappings = string_mappings.into_iter().peekable();

    while let (Some(continuation), Some(string)) =
        (continuation_mappings.peek(), string_mappings.peek())
    {
        if continuation.start() <= string.start() {
            mappings.push_mapping(continuation_mappings.next().unwrap());
        } else {
            mappings.push_mapping(string_mappings.next().unwrap());
        }
    }

    for mapping in continuation_mappings {
        mappings.push_mapping(mapping);
    }

    for mapping in string_mappings {
        mappings.push_mapping(mapping);
    }

    mappings
}

/// Extract a set of ranges over which to disable isort.
fn extract_isort_directives(lxr: &[LexResult], locator: &Locator) -> IsortDirectives {
    let mut exclusions: Vec<TextRange> = Vec::default();
    let mut splits: Vec<TextSize> = Vec::default();
    let mut off: Option<TextSize> = None;

    for &(ref tok, range) in lxr.iter().flatten() {
        let Tok::Comment(comment_text) = tok else {
            continue;
        };

        // `isort` allows for `# isort: skip` and `# isort: skip_file` to include or
        // omit a space after the colon. The remaining action comments are
        // required to include the space, and must appear on their own lines.
        let comment_text = comment_text.trim_end();
        if matches!(comment_text, "# isort: split" | "# ruff: isort: split") {
            splits.push(range.start());
        } else if matches!(
            comment_text,
            "# isort: skip_file"
                | "# isort:skip_file"
                | "# ruff: isort: skip_file"
                | "# ruff: isort:skip_file"
        ) {
            return IsortDirectives {
                skip_file: true,
                ..IsortDirectives::default()
            };
        } else if off.is_some() {
            if comment_text == "# isort: on" || comment_text == "# ruff: isort: on" {
                if let Some(exclusion_start) = off {
                    exclusions.push(TextRange::new(exclusion_start, range.start()));
                }
                off = None;
            }
        } else {
            if comment_text.contains("isort: skip") || comment_text.contains("isort:skip") {
                exclusions.push(locator.line_range(range.start()));
            } else if comment_text == "# isort: off" || comment_text == "# ruff: isort: off" {
                off = Some(range.start());
            }
        }
    }

    if let Some(start) = off {
        // Enforce unterminated `isort: off`.
        exclusions.push(TextRange::new(start, locator.contents().text_len()));
    }

    IsortDirectives {
        exclusions,
        splits,
        ..IsortDirectives::default()
    }
}

/// A comment that contains a [`TodoDirective`]
pub(crate) struct TodoComment<'a> {
    /// The comment's text
    pub(crate) content: &'a str,
    /// The directive found within the comment.
    pub(crate) directive: TodoDirective<'a>,
    /// The comment's actual [`TextRange`].
    pub(crate) range: TextRange,
    /// The comment range's position in [`Indexer`].comment_ranges()
    pub(crate) range_index: usize,
}

impl<'a> TodoComment<'a> {
    /// Attempt to transform a normal comment into a [`TodoComment`].
    pub(crate) fn from_comment(
        content: &'a str,
        range: TextRange,
        range_index: usize,
    ) -> Option<Self> {
        TodoDirective::from_comment(content, range).map(|directive| Self {
            content,
            directive,
            range,
            range_index,
        })
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct TodoDirective<'a> {
    /// The actual directive
    pub(crate) content: &'a str,
    /// The directive's [`TextRange`] in the file.
    pub(crate) range: TextRange,
    /// The directive's kind: HACK, XXX, FIXME, or TODO.
    pub(crate) kind: TodoDirectiveKind,
}

impl<'a> TodoDirective<'a> {
    /// Extract a [`TodoDirective`] from a comment.
    pub(crate) fn from_comment(comment: &'a str, comment_range: TextRange) -> Option<Self> {
        // The directive's offset from the start of the comment.
        let mut relative_offset = TextSize::new(0);
        let mut subset_opt = Some(comment);

        // Loop over `#`-delimited sections of the comment to check for directives. This will
        // correctly handle cases like `# foo # TODO`.
        while let Some(subset) = subset_opt {
            let trimmed = subset.trim_start_matches('#').trim_start();

            let offset = subset.text_len() - trimmed.text_len();
            relative_offset += offset;

            // If we detect a TodoDirectiveKind variant substring in the comment, construct and
            // return the appropriate TodoDirective
            if let Ok(directive_kind) = trimmed.parse::<TodoDirectiveKind>() {
                let len = directive_kind.len();

                return Some(Self {
                    content: &comment[TextRange::at(relative_offset, len)],
                    range: TextRange::at(comment_range.start() + relative_offset, len),
                    kind: directive_kind,
                });
            }

            // Shrink the subset to check for the next phrase starting with "#".
            subset_opt = if let Some(new_offset) = trimmed.find('#') {
                relative_offset += TextSize::try_from(new_offset).unwrap();
                subset.get(relative_offset.to_usize()..)
            } else {
                None
            };
        }

        None
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum TodoDirectiveKind {
    Todo,
    Fixme,
    Xxx,
    Hack,
}

impl FromStr for TodoDirectiveKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // The lengths of the respective variant strings: TODO, FIXME, HACK, XXX
        for length in [3, 4, 5] {
            let Some(substr) = s.get(..length) else {
                break;
            };

            match substr.to_lowercase().as_str() {
                "fixme" => {
                    return Ok(TodoDirectiveKind::Fixme);
                }
                "hack" => {
                    return Ok(TodoDirectiveKind::Hack);
                }
                "todo" => {
                    return Ok(TodoDirectiveKind::Todo);
                }
                "xxx" => {
                    return Ok(TodoDirectiveKind::Xxx);
                }
                _ => continue,
            }
        }

        Err(())
    }
}

impl TodoDirectiveKind {
    fn len(&self) -> TextSize {
        match self {
            TodoDirectiveKind::Xxx => TextSize::new(3),
            TodoDirectiveKind::Hack | TodoDirectiveKind::Todo => TextSize::new(4),
            TodoDirectiveKind::Fixme => TextSize::new(5),
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_parser::lexer::LexResult;
    use ruff_python_parser::{lexer, Mode};
    use ruff_text_size::{TextLen, TextRange, TextSize};

    use ruff_python_index::Indexer;
    use ruff_source_file::Locator;

    use crate::directives::{
        extract_isort_directives, extract_noqa_line_for, TodoDirective, TodoDirectiveKind,
    };
    use crate::noqa::NoqaMapping;

    fn noqa_mappings(contents: &str) -> NoqaMapping {
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let locator = Locator::new(contents);
        let indexer = Indexer::from_tokens(&lxr, &locator);

        extract_noqa_line_for(&lxr, &locator, &indexer)
    }

    #[test]
    fn noqa_extraction() {
        let contents = "x = 1
y = 2 \
    + 1
z = x + 1";

        assert_eq!(noqa_mappings(contents), NoqaMapping::default());

        let contents = "
x = 1
y = 2
z = x + 1";
        assert_eq!(noqa_mappings(contents), NoqaMapping::default());

        let contents = "x = 1
y = 2
z = x + 1
        ";
        assert_eq!(noqa_mappings(contents), NoqaMapping::default());

        let contents = "x = 1

y = 2
z = x + 1
        ";
        assert_eq!(noqa_mappings(contents), NoqaMapping::default());

        let contents = "x = '''abc
def
ghi
'''
y = 2
z = x + 1";
        assert_eq!(
            noqa_mappings(contents),
            NoqaMapping::from_iter([TextRange::new(TextSize::from(0), TextSize::from(22))])
        );

        let contents = "x = 1
y = '''abc
def
ghi
'''
z = 2";
        assert_eq!(
            noqa_mappings(contents),
            NoqaMapping::from_iter([TextRange::new(TextSize::from(6), TextSize::from(28))])
        );

        let contents = "x = 1
y = '''abc
def
ghi
'''";
        assert_eq!(
            noqa_mappings(contents),
            NoqaMapping::from_iter([TextRange::new(TextSize::from(6), TextSize::from(28))])
        );

        let contents = r"x = \
    1";
        assert_eq!(
            noqa_mappings(contents),
            NoqaMapping::from_iter([TextRange::new(TextSize::from(0), TextSize::from(6))])
        );

        let contents = r"from foo import \
    bar as baz, \
    qux as quux";
        assert_eq!(
            noqa_mappings(contents),
            NoqaMapping::from_iter([TextRange::new(TextSize::from(0), TextSize::from(36))])
        );

        let contents = r"
# Foo
from foo import \
    bar as baz, \
    qux as quux # Baz
x = \
    1
y = \
    2";
        assert_eq!(
            noqa_mappings(contents),
            NoqaMapping::from_iter([
                TextRange::new(TextSize::from(7), TextSize::from(43)),
                TextRange::new(TextSize::from(65), TextSize::from(71)),
                TextRange::new(TextSize::from(77), TextSize::from(83)),
            ])
        );
    }

    #[test]
    fn isort_exclusions() {
        let contents = "x = 1
y = 2
z = x + 1";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        assert_eq!(
            extract_isort_directives(&lxr, &Locator::new(contents)).exclusions,
            Vec::default()
        );

        let contents = "# isort: off
x = 1
y = 2
# isort: on
z = x + 1";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        assert_eq!(
            extract_isort_directives(&lxr, &Locator::new(contents)).exclusions,
            Vec::from_iter([TextRange::new(TextSize::from(0), TextSize::from(25))])
        );

        let contents = "# isort: off
x = 1
# isort: off
y = 2
# isort: on
z = x + 1
# isort: on";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        assert_eq!(
            extract_isort_directives(&lxr, &Locator::new(contents)).exclusions,
            Vec::from_iter([TextRange::new(TextSize::from(0), TextSize::from(38))])
        );

        let contents = "# isort: off
x = 1
y = 2
z = x + 1";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        assert_eq!(
            extract_isort_directives(&lxr, &Locator::new(contents)).exclusions,
            Vec::from_iter([TextRange::at(TextSize::from(0), contents.text_len())])
        );

        let contents = "# isort: skip_file
x = 1
y = 2
z = x + 1";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        assert_eq!(
            extract_isort_directives(&lxr, &Locator::new(contents)).exclusions,
            Vec::default()
        );

        let contents = "# isort: off
x = 1
# isort: on
y = 2
# isort: skip_file
z = x + 1";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        assert_eq!(
            extract_isort_directives(&lxr, &Locator::new(contents)).exclusions,
            Vec::default()
        );
    }

    #[test]
    fn isort_splits() {
        let contents = "x = 1
y = 2
z = x + 1";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        assert_eq!(
            extract_isort_directives(&lxr, &Locator::new(contents)).splits,
            Vec::new()
        );

        let contents = "x = 1
y = 2
# isort: split
z = x + 1";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        assert_eq!(
            extract_isort_directives(&lxr, &Locator::new(contents)).splits,
            vec![TextSize::from(12)]
        );

        let contents = "x = 1
y = 2  # isort: split
z = x + 1";
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        assert_eq!(
            extract_isort_directives(&lxr, &Locator::new(contents)).splits,
            vec![TextSize::from(13)]
        );
    }

    #[test]
    fn todo_directives() {
        let test_comment = "# TODO: todo tag";
        let test_comment_range = TextRange::at(TextSize::new(0), test_comment.text_len());
        let expected = TodoDirective {
            content: "TODO",
            range: TextRange::new(TextSize::new(2), TextSize::new(6)),
            kind: TodoDirectiveKind::Todo,
        };
        assert_eq!(
            expected,
            TodoDirective::from_comment(test_comment, test_comment_range).unwrap()
        );

        let test_comment = "#TODO: todo tag";
        let test_comment_range = TextRange::at(TextSize::new(0), test_comment.text_len());
        let expected = TodoDirective {
            content: "TODO",
            range: TextRange::new(TextSize::new(1), TextSize::new(5)),
            kind: TodoDirectiveKind::Todo,
        };
        assert_eq!(
            expected,
            TodoDirective::from_comment(test_comment, test_comment_range).unwrap()
        );

        let test_comment = "# fixme: fixme tag";
        let test_comment_range = TextRange::at(TextSize::new(0), test_comment.text_len());
        let expected = TodoDirective {
            content: "fixme",
            range: TextRange::new(TextSize::new(2), TextSize::new(7)),
            kind: TodoDirectiveKind::Fixme,
        };
        assert_eq!(
            expected,
            TodoDirective::from_comment(test_comment, test_comment_range).unwrap()
        );

        let test_comment = "# noqa # TODO: todo";
        let test_comment_range = TextRange::at(TextSize::new(0), test_comment.text_len());
        let expected = TodoDirective {
            content: "TODO",
            range: TextRange::new(TextSize::new(9), TextSize::new(13)),
            kind: TodoDirectiveKind::Todo,
        };
        assert_eq!(
            expected,
            TodoDirective::from_comment(test_comment, test_comment_range).unwrap()
        );

        let test_comment = "# no directive";
        let test_comment_range = TextRange::at(TextSize::new(0), test_comment.text_len());
        assert_eq!(
            None,
            TodoDirective::from_comment(test_comment, test_comment_range)
        );
    }
}
