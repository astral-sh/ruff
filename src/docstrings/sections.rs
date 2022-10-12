use std::collections::BTreeSet;

use once_cell::sync::Lazy;
use titlecase::titlecase;

use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::docstrings::docstring_checks::range_for;
use crate::docstrings::types::Definition;

static NUMPY_SECTION_NAMES: Lazy<BTreeSet<&'static str>> = Lazy::new(|| {
    BTreeSet::from([
        "Short Summary",
        "Extended Summary",
        "Parameters",
        "Returns",
        "Yields",
        "Other Parameters",
        "Raises",
        "See Also",
        "Notes",
        "References",
        "Examples",
        "Attributes",
        "Methods",
    ])
});

static NUMPY_SECTION_NAMES_LOWERCASE: Lazy<BTreeSet<&'static str>> = Lazy::new(|| {
    BTreeSet::from([
        "short summary",
        "extended summary",
        "parameters",
        "returns",
        "yields",
        "other parameters",
        "raises",
        "see also",
        "notes",
        "references",
        "examples",
        "attributes",
        "methods",
    ])
});

// TODO(charlie): Include Google section names.
// static GOOGLE_SECTION_NAMES: Lazy<BTreeSet<&'static str>> = Lazy::new(|| {
//     BTreeSet::from([
//         "Args",
//         "Arguments",
//         "Attention",
//         "Attributes",
//         "Caution",
//         "Danger",
//         "Error",
//         "Example",
//         "Examples",
//         "Hint",
//         "Important",
//         "Keyword Args",
//         "Keyword Arguments",
//         "Methods",
//         "Note",
//         "Notes",
//         "Return",
//         "Returns",
//         "Raises",
//         "References",
//         "See Also",
//         "Tip",
//         "Todo",
//         "Warning",
//         "Warnings",
//         "Warns",
//         "Yield",
//         "Yields",
//     ])
// });

fn get_leading_words(line: &str) -> String {
    line.trim()
        .chars()
        .take_while(|char| char.is_alphanumeric() || char.is_whitespace())
        .collect()
}

fn suspected_as_section(line: &str) -> bool {
    NUMPY_SECTION_NAMES_LOWERCASE.contains(&get_leading_words(line).to_lowercase().as_str())
}

#[derive(Debug)]
pub struct SectionContext<'a> {
    section_name: String,
    previous_line: &'a str,
    line: &'a str,
    following_lines: &'a [&'a str],
    original_index: usize,
    is_last_section: bool,
}

/// Check if the suspected context is really a section header.
fn is_docstring_section(context: &SectionContext) -> bool {
    let section_name_suffix = context
        .line
        .trim()
        .strip_prefix(&context.section_name)
        .unwrap()
        .trim();
    let this_looks_like_a_section_name =
        section_name_suffix == ":" || section_name_suffix.is_empty();
    if !this_looks_like_a_section_name {
        return false;
    }

    let prev_line = context.previous_line.trim();
    let prev_line_ends_with_punctuation = [',', ';', '.', '-', '\\', '/', ']', '}', ')']
        .into_iter()
        .any(|char| prev_line.ends_with(char));
    let prev_line_looks_like_end_of_paragraph =
        prev_line_ends_with_punctuation || prev_line.is_empty();
    if !prev_line_looks_like_end_of_paragraph {
        return false;
    }

    true
}

/// Extract all `SectionContext` values from a docstring.
pub fn section_contexts<'a>(lines: &'a [&'a str]) -> Vec<SectionContext<'a>> {
    let suspected_section_indices: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(lineno, line)| {
            if lineno > 0 && suspected_as_section(line) {
                Some(lineno)
            } else {
                None
            }
        })
        .collect();

    let mut contexts = vec![];
    for lineno in suspected_section_indices {
        let context = SectionContext {
            section_name: get_leading_words(lines[lineno]),
            previous_line: lines[lineno - 1],
            line: lines[lineno],
            following_lines: &lines[lineno..],
            original_index: lineno,
            is_last_section: false,
        };
        if is_docstring_section(&context) {
            contexts.push(context);
        }
    }

    let mut truncated_contexts = vec![];
    let mut end: Option<usize> = None;
    for context in contexts.into_iter().rev() {
        let next_end = context.original_index;
        truncated_contexts.push(SectionContext {
            section_name: context.section_name,
            previous_line: context.previous_line,
            line: context.line,
            following_lines: if let Some(end) = end {
                &lines[context.original_index..end]
            } else {
                context.following_lines
            },
            original_index: context.original_index,
            is_last_section: end.is_none(),
        });
        end = Some(next_end);
    }
    truncated_contexts.reverse();
    truncated_contexts
}

fn check_common_section(checker: &mut Checker, definition: &Definition, context: &SectionContext) {
    // TODO(charlie): Implement D214.
    // TODO(charlie): Implement `_check_blanks_and_section_underline`.
    let docstring = definition
        .docstring
        .expect("Sections are only available for docstrings.");

    if checker.settings.enabled.contains(&CheckCode::D405) {
        if !NUMPY_SECTION_NAMES.contains(&context.section_name.as_str())
            && NUMPY_SECTION_NAMES.contains(titlecase(&context.section_name).as_str())
        {
            checker.add_check(Check::new(
                CheckKind::CapitalizeSectionName(context.section_name.to_string()),
                range_for(docstring),
            ))
        }
    }

    if context
        .following_lines
        .last()
        .map(|line| !line.trim().is_empty())
        .unwrap_or(true)
    {
        if context.is_last_section {
            if checker.settings.enabled.contains(&CheckCode::D413) {
                checker.add_check(Check::new(
                    CheckKind::BlankLineAfterLastSection(context.section_name.to_string()),
                    range_for(docstring),
                ))
            }
        } else {
            if checker.settings.enabled.contains(&CheckCode::D410) {
                checker.add_check(Check::new(
                    CheckKind::BlankLineAfterSection(context.section_name.to_string()),
                    range_for(docstring),
                ))
            }
        }
    }

    if checker.settings.enabled.contains(&CheckCode::D411) {
        if !context.previous_line.is_empty() {
            checker.add_check(Check::new(
                CheckKind::BlankLineBeforeSection(context.section_name.to_string()),
                range_for(docstring),
            ))
        }
    }
}

pub fn check_numpy_section(
    checker: &mut Checker,
    definition: &Definition,
    context: &SectionContext,
) {
    // TODO(charlie): Implement `_check_parameters_section`.
    check_common_section(checker, definition, context);

    if checker.settings.enabled.contains(&CheckCode::D406) {
        let suffix = context
            .line
            .trim()
            .strip_prefix(&context.section_name)
            .unwrap();
        if !suffix.is_empty() {
            let docstring = definition
                .docstring
                .expect("Sections are only available for docstrings.");
            checker.add_check(Check::new(
                CheckKind::NewLineAfterSectionName(context.section_name.to_string()),
                range_for(docstring),
            ))
        }
    }
}
