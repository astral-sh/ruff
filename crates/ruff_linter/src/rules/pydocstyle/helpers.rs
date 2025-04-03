use std::cmp::Ordering;

use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::{Definition, SemanticModel};
use ruff_python_trivia::Cursor;
use ruff_source_file::{Line, UniversalNewlines};
use ruff_text_size::{TextRange, TextSize};

use crate::docstrings::sections::{SectionContexts, SectionKind};
use crate::docstrings::styles::SectionStyle;
use crate::docstrings::Docstring;
use crate::rules::pydocstyle::settings::{Convention, Settings};

/// Return the index of the first logical line in a string.
pub(super) fn logical_line(content: &str) -> Option<usize> {
    // Find the first logical line.
    let mut logical_line = None;
    for (i, line) in content.universal_newlines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.chars().all(|c| matches!(c, '-' | '~' | '=' | '#')) {
            // Empty line, or underline. If this is the line _after_ the first logical line, stop.
            if logical_line.is_some() {
                break;
            }
        } else {
            // Non-empty line. Store the index.
            logical_line = Some(i);
        }
    }
    logical_line
}

/// Normalize a word by removing all non-alphanumeric characters
/// and converting it to lowercase.
pub(super) fn normalize_word(first_word: &str) -> String {
    first_word
        .replace(|c: char| !c.is_alphanumeric(), "")
        .to_lowercase()
}

/// Return true if a line ends with an odd number of backslashes (i.e., ends with an escape).
pub(super) fn ends_with_backslash(line: &str) -> bool {
    line.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1
}

/// Check decorator list to see if function should be ignored.
pub(crate) fn should_ignore_definition(
    definition: &Definition,
    settings: &Settings,
    semantic: &SemanticModel,
) -> bool {
    let ignore_decorators = settings.ignore_decorators();

    if ignore_decorators.len() == 0 {
        return false;
    }

    let Some(function) = definition.as_function_def() else {
        return false;
    };

    function.decorator_list.iter().any(|decorator| {
        semantic
            .resolve_qualified_name(map_callable(&decorator.expression))
            .is_some_and(|qualified_name| {
                ignore_decorators
                    .clone()
                    .any(|decorator| decorator == qualified_name)
            })
    })
}

pub(crate) fn get_section_contexts<'a>(
    docstring: &'a Docstring<'a>,
    convention: Option<Convention>,
) -> SectionContexts<'a> {
    match convention {
        Some(Convention::Google) => {
            SectionContexts::from_docstring(docstring, SectionStyle::Google)
        }
        Some(Convention::Numpy) => SectionContexts::from_docstring(docstring, SectionStyle::Numpy),
        Some(Convention::Pep257) | None => {
            // There are some overlapping section names, between the Google and NumPy conventions
            // (e.g., "Returns", "Raises"). Break ties by checking for the presence of some of the
            // section names that are unique to each convention.

            // If the docstring contains `Parameters:` or `Other Parameters:`, use the NumPy
            // convention.
            let numpy_sections = SectionContexts::from_docstring(docstring, SectionStyle::Numpy);
            if numpy_sections.iter().any(|context| {
                matches!(
                    context.kind(),
                    SectionKind::Parameters
                        | SectionKind::OtherParams
                        | SectionKind::OtherParameters
                )
            }) {
                return numpy_sections;
            }

            // If the docstring contains any argument specifier, use the Google convention.
            let google_sections = SectionContexts::from_docstring(docstring, SectionStyle::Google);
            if google_sections.iter().any(|context| {
                matches!(
                    context.kind(),
                    SectionKind::Args
                        | SectionKind::Arguments
                        | SectionKind::KeywordArgs
                        | SectionKind::KeywordArguments
                        | SectionKind::OtherArgs
                        | SectionKind::OtherArguments
                )
            }) {
                return google_sections;
            }

            // Otherwise, If one convention matched more sections, return that...
            match google_sections.len().cmp(&numpy_sections.len()) {
                Ordering::Greater => return google_sections,
                Ordering::Less => return numpy_sections,
                Ordering::Equal => {}
            }

            // 0 sections of either convention? Default to numpy
            if google_sections.len() == 0 {
                return numpy_sections;
            }

            for section in &google_sections {
                // If any section has something that could be an underline
                // on the following line, assume Numpy.
                // If it *doesn't* have an underline and it *does* have a colon
                // at the end of a section name, assume Google.
                if let Some(following_line) = section.following_lines().next() {
                    if find_underline(&following_line, '-').is_some() {
                        return numpy_sections;
                    }
                }
                if section.summary_after_section_name().starts_with(':') {
                    return google_sections;
                }
            }

            // If all else fails, default to numpy
            numpy_sections
        }
    }
}

/// Returns the [`TextRange`] of the underline, if a line consists of only dashes.
pub(super) fn find_underline(line: &Line, dash: char) -> Option<TextRange> {
    let mut cursor = Cursor::new(line.as_str());

    // Eat leading whitespace.
    cursor.eat_while(char::is_whitespace);

    // Determine the start of the dashes.
    let offset = cursor.token_len();

    // Consume the dashes.
    cursor.start_token();
    cursor.eat_while(|c| c == dash);

    // Determine the end of the dashes.
    let len = cursor.token_len();

    // If there are no dashes, return None.
    if len == TextSize::new(0) {
        return None;
    }

    // Eat trailing whitespace.
    cursor.eat_while(char::is_whitespace);

    // If there are any characters after the dashes, return None.
    if !cursor.is_eof() {
        return None;
    }

    Some(TextRange::at(offset, len) + line.start())
}
