//! Abstractions for NumPy-style docstrings.

use std::collections::BTreeSet;

use crate::ast::types::Range;
use once_cell::sync::Lazy;

use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::docstrings::definition::Definition;
use crate::docstrings::sections::SectionContext;
use crate::docstrings::styles::SectionStyle;
use crate::docstrings::{helpers, sections};

pub(crate) static LOWERCASE_NUMPY_SECTION_NAMES: Lazy<BTreeSet<&'static str>> = Lazy::new(|| {
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

pub(crate) static NUMPY_SECTION_NAMES: Lazy<BTreeSet<&'static str>> = Lazy::new(|| {
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

fn check_parameters_section(
    checker: &mut Checker,
    definition: &Definition,
    context: &SectionContext,
) {
    // Collect the list of arguments documented in the docstring.
    let mut docstring_args: BTreeSet<&str> = Default::default();
    let section_level_indent = helpers::leading_space(context.line);
    for i in 1..context.following_lines.len() {
        let current_line = context.following_lines[i - 1];
        let current_leading_space = helpers::leading_space(current_line);
        let next_line = context.following_lines[i];
        if current_leading_space == section_level_indent
            && (helpers::leading_space(next_line).len() > current_leading_space.len())
            && !next_line.trim().is_empty()
        {
            let parameters = if let Some(semi_index) = current_line.find(':') {
                // If the parameter has a type annotation, exclude it.
                &current_line[..semi_index]
            } else {
                // Otherwise, it's just a list of parameters on the current line.
                current_line.trim()
            };
            // Notably, NumPy lets you put multiple parameters of the same type on the same line.
            for parameter in parameters.split(',') {
                docstring_args.insert(parameter.trim());
            }
        }
    }
    // Validate that all arguments were documented.
    sections::check_missing_args(checker, definition, &docstring_args);
}

pub(crate) fn check_numpy_section(
    checker: &mut Checker,
    definition: &Definition,
    context: &SectionContext,
) {
    sections::check_common_section(checker, definition, context, &SectionStyle::NumPy);

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
                Range::from_located(docstring),
            ))
        }
    }

    if checker.settings.enabled.contains(&CheckCode::D417) {
        let capitalized_section_name = titlecase::titlecase(&context.section_name);
        if capitalized_section_name == "Parameters" {
            check_parameters_section(checker, definition, context);
        }
    }
}
