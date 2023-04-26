//! Extract `# noqa` and `# isort: skip` directives from tokenized source.

use crate::noqa::NoqaMapping;
use bitflags::bitflags;
use ruff_python_ast::source_code::{Indexer, Locator};
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

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
        noqa_line_for: if flags.contains(Flags::NOQA) {
            extract_noqa_line_for(lxr, locator, indexer)
        } else {
            NoqaMapping::default()
        },
        isort: if flags.contains(Flags::ISORT) {
            extract_isort_directives(lxr, locator)
        } else {
            IsortDirectives::default()
        },
    }
}

/// Extract a mapping from logical line to noqa line.
pub fn extract_noqa_line_for(
    lxr: &[LexResult],
    locator: &Locator,
    indexer: &Indexer,
) -> NoqaMapping {
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
                    string_mappings.push(*range);
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
pub fn extract_isort_directives(lxr: &[LexResult], locator: &Locator) -> IsortDirectives {
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

#[cfg(test)]
mod tests {
    use ruff_python_ast::source_code::{Indexer, Locator};
    use ruff_text_size::{TextLen, TextRange, TextSize};
    use rustpython_parser::lexer::LexResult;
    use rustpython_parser::{lexer, Mode};

    use crate::directives::{extract_isort_directives, extract_noqa_line_for};
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
            NoqaMapping::from_iter([TextRange::new(TextSize::from(4), TextSize::from(22)),])
        );

        let contents = "x = 1
y = '''abc
def
ghi
'''
z = 2";
        assert_eq!(
            noqa_mappings(contents),
            NoqaMapping::from_iter([TextRange::new(TextSize::from(10), TextSize::from(28))])
        );

        let contents = "x = 1
y = '''abc
def
ghi
'''";
        assert_eq!(
            noqa_mappings(contents),
            NoqaMapping::from_iter([TextRange::new(TextSize::from(10), TextSize::from(28))])
        );

        let contents = r#"x = \
    1"#;
        assert_eq!(
            noqa_mappings(contents),
            NoqaMapping::from_iter([TextRange::new(TextSize::from(0), TextSize::from(6))])
        );

        let contents = r#"from foo import \
    bar as baz, \
    qux as quux"#;
        assert_eq!(
            noqa_mappings(contents),
            NoqaMapping::from_iter([TextRange::new(TextSize::from(0), TextSize::from(36))])
        );

        let contents = r#"
# Foo
from foo import \
    bar as baz, \
    qux as quux # Baz
x = \
    1
y = \
    2"#;
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
}
