use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::{Definition, SemanticModel};
use ruff_source_file::UniversalNewlines;

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
            return SectionContexts::from_docstring(docstring, SectionStyle::Google);
        }
        Some(Convention::Numpy) => {
            return SectionContexts::from_docstring(docstring, SectionStyle::Numpy);
        }
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

            // Otherwise, use whichever convention matched more sections.
            if google_sections.len() > numpy_sections.len() {
                google_sections
            } else {
                numpy_sections
            }
        }
    }
}
