//! Abstractions for Google-style docstrings.

use std::collections::BTreeSet;

use crate::ast::types::Range;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::docstrings::sections;
use crate::docstrings::sections::SectionContext;
use crate::docstrings::styles::SectionStyle;
use crate::docstrings::types::Definition;

pub(crate) static GOOGLE_SECTION_NAMES: Lazy<BTreeSet<&'static str>> = Lazy::new(|| {
    BTreeSet::from([
        "Args",
        "Arguments",
        "Attention",
        "Attributes",
        "Caution",
        "Danger",
        "Error",
        "Example",
        "Examples",
        "Hint",
        "Important",
        "Keyword Args",
        "Keyword Arguments",
        "Methods",
        "Note",
        "Notes",
        "Return",
        "Returns",
        "Raises",
        "References",
        "See Also",
        "Tip",
        "Todo",
        "Warning",
        "Warnings",
        "Warns",
        "Yield",
        "Yields",
    ])
});

pub(crate) static LOWERCASE_GOOGLE_SECTION_NAMES: Lazy<BTreeSet<&'static str>> = Lazy::new(|| {
    BTreeSet::from([
        "args",
        "arguments",
        "attention",
        "attributes",
        "caution",
        "danger",
        "error",
        "example",
        "examples",
        "hint",
        "important",
        "keyword args",
        "keyword arguments",
        "methods",
        "note",
        "notes",
        "return",
        "returns",
        "raises",
        "references",
        "see also",
        "tip",
        "todo",
        "warning",
        "warnings",
        "warns",
        "yield",
        "yields",
    ])
});

// See: `GOOGLE_ARGS_REGEX` in `pydocstyle/checker.py`.
static GOOGLE_ARGS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*(\w+)\s*(\(.*?\))?\s*:\n?\s*.+").expect("Invalid regex"));

fn check_args_section(checker: &mut Checker, definition: &Definition, context: &SectionContext) {
    let mut args_sections: Vec<String> = vec![];
    for line in textwrap::dedent(&context.following_lines.join("\n")).lines() {
        if line
            .chars()
            .next()
            .map(|char| char.is_whitespace())
            .unwrap_or(true)
        {
            // This is a continuation of documentation for the last
            // parameter because it does start with whitespace.
            if let Some(current) = args_sections.last_mut() {
                current.push_str(line);
            }
        } else {
            // This line is the start of documentation for the next
            // parameter because it doesn't start with any whitespace.
            args_sections.push(line.to_string());
        }
    }

    sections::check_missing_args(
        checker,
        definition,
        // Collect the list of arguments documented in the docstring.
        &BTreeSet::from_iter(args_sections.iter().filter_map(|section| {
            match GOOGLE_ARGS_REGEX.captures(section.as_str()) {
                Some(caps) => caps.get(1).map(|arg_name| arg_name.as_str()),
                None => None,
            }
        })),
    )
}

pub(crate) fn check_google_section(
    checker: &mut Checker,
    definition: &Definition,
    context: &SectionContext,
) {
    sections::check_common_section(checker, definition, context, &SectionStyle::Google);

    if checker.settings.enabled.contains(&CheckCode::D416) {
        let suffix = context
            .line
            .trim()
            .strip_prefix(&context.section_name)
            .unwrap();
        if suffix != ":" {
            let docstring = definition
                .docstring
                .expect("Sections are only available for docstrings.");
            checker.add_check(Check::new(
                CheckKind::SectionNameEndsInColon(context.section_name.to_string()),
                Range::from_located(docstring),
            ))
        }
    }

    if checker.settings.enabled.contains(&CheckCode::D417) {
        let capitalized_section_name = titlecase::titlecase(&context.section_name);
        if capitalized_section_name == "Args" || capitalized_section_name == "Arguments" {
            check_args_section(checker, definition, context);
        }
    }
}
