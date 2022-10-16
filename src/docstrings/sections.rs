use std::collections::BTreeSet;

use itertools::Itertools;
use rustpython_ast::{Arg, StmtKind};
use titlecase::titlecase;

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::docstrings::helpers;
use crate::docstrings::styles::SectionStyle;
use crate::docstrings::types::{Definition, DefinitionKind};
use crate::visibility::is_static;

#[derive(Debug)]
pub(crate) struct SectionContext<'a> {
    pub(crate) section_name: String,
    pub(crate) previous_line: &'a str,
    pub(crate) line: &'a str,
    pub(crate) following_lines: &'a [&'a str],
    pub(crate) is_last_section: bool,
    original_index: usize,
}

fn suspected_as_section(line: &str, style: &SectionStyle) -> bool {
    style
        .lowercase_section_names()
        .contains(&helpers::leading_words(line).to_lowercase().as_str())
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
pub(crate) fn section_contexts<'a>(
    lines: &'a [&'a str],
    style: &SectionStyle,
) -> Vec<SectionContext<'a>> {
    let suspected_section_indices: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(lineno, line)| {
            if lineno > 0 && suspected_as_section(line, style) {
                Some(lineno)
            } else {
                None
            }
        })
        .collect();

    let mut contexts = vec![];
    for lineno in suspected_section_indices {
        let context = SectionContext {
            section_name: helpers::leading_words(lines[lineno]),
            previous_line: lines[lineno - 1],
            line: lines[lineno],
            following_lines: &lines[lineno + 1..],
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
                &lines[context.original_index + 1..end]
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

fn check_blanks_and_section_underline(
    checker: &mut Checker,
    definition: &Definition,
    context: &SectionContext,
) {
    let docstring = definition
        .docstring
        .expect("Sections are only available for docstrings.");

    let mut blank_lines_after_header = 0;
    for line in context.following_lines {
        if !line.trim().is_empty() {
            break;
        }
        blank_lines_after_header += 1;
    }

    // Nothing but blank lines after the section header.
    if blank_lines_after_header == context.following_lines.len() {
        if checker.settings.enabled.contains(&CheckCode::D407) {
            checker.add_check(Check::new(
                CheckKind::DashedUnderlineAfterSection(context.section_name.to_string()),
                Range::from_located(docstring),
            ));
        }
        if checker.settings.enabled.contains(&CheckCode::D414) {
            checker.add_check(Check::new(
                CheckKind::NonEmptySection(context.section_name.to_string()),
                Range::from_located(docstring),
            ));
        }
        return;
    }

    let non_empty_line = context.following_lines[blank_lines_after_header];
    let dash_line_found = non_empty_line
        .chars()
        .all(|char| char.is_whitespace() || char == '-');

    if !dash_line_found {
        if checker.settings.enabled.contains(&CheckCode::D407) {
            checker.add_check(Check::new(
                CheckKind::DashedUnderlineAfterSection(context.section_name.to_string()),
                Range::from_located(docstring),
            ));
        }
        if blank_lines_after_header > 0 {
            if checker.settings.enabled.contains(&CheckCode::D212) {
                checker.add_check(Check::new(
                    CheckKind::NoBlankLinesBetweenHeaderAndContent(
                        context.section_name.to_string(),
                    ),
                    Range::from_located(docstring),
                ));
            }
        }
    } else {
        if blank_lines_after_header > 0 {
            if checker.settings.enabled.contains(&CheckCode::D408) {
                checker.add_check(Check::new(
                    CheckKind::SectionUnderlineAfterName(context.section_name.to_string()),
                    Range::from_located(docstring),
                ));
            }
        }

        if non_empty_line
            .trim()
            .chars()
            .filter(|char| *char == '-')
            .count()
            != context.section_name.len()
        {
            if checker.settings.enabled.contains(&CheckCode::D409) {
                checker.add_check(Check::new(
                    CheckKind::SectionUnderlineMatchesSectionLength(
                        context.section_name.to_string(),
                    ),
                    Range::from_located(docstring),
                ));
            }
        }

        if checker.settings.enabled.contains(&CheckCode::D215) {
            if helpers::leading_space(non_empty_line).len()
                > helpers::indentation(checker, docstring).len()
            {
                checker.add_check(Check::new(
                    CheckKind::SectionUnderlineNotOverIndented(context.section_name.to_string()),
                    Range::from_located(docstring),
                ));
            }
        }

        let line_after_dashes_index = blank_lines_after_header + 1;

        if line_after_dashes_index < context.following_lines.len() {
            let line_after_dashes = context.following_lines[line_after_dashes_index];
            if line_after_dashes.trim().is_empty() {
                let rest_of_lines = &context.following_lines[line_after_dashes_index..];
                if rest_of_lines.iter().all(|line| line.trim().is_empty()) {
                    if checker.settings.enabled.contains(&CheckCode::D414) {
                        checker.add_check(Check::new(
                            CheckKind::NonEmptySection(context.section_name.to_string()),
                            Range::from_located(docstring),
                        ));
                    }
                } else {
                    if checker.settings.enabled.contains(&CheckCode::D412) {
                        checker.add_check(Check::new(
                            CheckKind::NoBlankLinesBetweenHeaderAndContent(
                                context.section_name.to_string(),
                            ),
                            Range::from_located(docstring),
                        ));
                    }
                }
            }
        } else {
            if checker.settings.enabled.contains(&CheckCode::D414) {
                checker.add_check(Check::new(
                    CheckKind::NonEmptySection(context.section_name.to_string()),
                    Range::from_located(docstring),
                ));
            }
        }
    }
}

pub(crate) fn check_common_section(
    checker: &mut Checker,
    definition: &Definition,
    context: &SectionContext,
    style: &SectionStyle,
) {
    let docstring = definition
        .docstring
        .expect("Sections are only available for docstrings.");

    if checker.settings.enabled.contains(&CheckCode::D405) {
        if !style
            .section_names()
            .contains(&context.section_name.as_str())
            && style
                .section_names()
                .contains(titlecase(&context.section_name).as_str())
        {
            checker.add_check(Check::new(
                CheckKind::CapitalizeSectionName(context.section_name.to_string()),
                Range::from_located(docstring),
            ))
        }
    }

    if checker.settings.enabled.contains(&CheckCode::D214) {
        if helpers::leading_space(context.line).len()
            > helpers::indentation(checker, docstring).len()
        {
            checker.add_check(Check::new(
                CheckKind::SectionNotOverIndented(context.section_name.to_string()),
                Range::from_located(docstring),
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
                    Range::from_located(docstring),
                ))
            }
        } else {
            if checker.settings.enabled.contains(&CheckCode::D410) {
                checker.add_check(Check::new(
                    CheckKind::BlankLineAfterSection(context.section_name.to_string()),
                    Range::from_located(docstring),
                ))
            }
        }
    }

    if checker.settings.enabled.contains(&CheckCode::D411) {
        if !context.previous_line.is_empty() {
            checker.add_check(Check::new(
                CheckKind::BlankLineBeforeSection(context.section_name.to_string()),
                Range::from_located(docstring),
            ))
        }
    }

    check_blanks_and_section_underline(checker, definition, context);
}

pub(crate) fn check_missing_args(
    checker: &mut Checker,
    definition: &Definition,
    docstrings_args: &BTreeSet<&str>,
) {
    if let DefinitionKind::Function(parent)
    | DefinitionKind::NestedFunction(parent)
    | DefinitionKind::Method(parent) = definition.kind
    {
        if let StmtKind::FunctionDef {
            args: arguments, ..
        }
        | StmtKind::AsyncFunctionDef {
            args: arguments, ..
        } = &parent.node
        {
            // Collect all the arguments into a single vector.
            let mut all_arguments: Vec<&Arg> = arguments
                .args
                .iter()
                .chain(arguments.posonlyargs.iter())
                .chain(arguments.kwonlyargs.iter())
                .skip(
                    // If this is a non-static method, skip `cls` or `self`.
                    if matches!(definition.kind, DefinitionKind::Method(_)) && !is_static(parent) {
                        1
                    } else {
                        0
                    },
                )
                .collect();
            if let Some(arg) = &arguments.vararg {
                all_arguments.push(arg);
            }
            if let Some(arg) = &arguments.kwarg {
                all_arguments.push(arg);
            }

            // Look for arguments that weren't included in the docstring.
            let mut missing_args: BTreeSet<&str> = Default::default();
            for arg in all_arguments {
                let arg_name = arg.node.arg.as_str();
                if arg_name.starts_with('_') {
                    continue;
                }
                if docstrings_args.contains(&arg_name) {
                    continue;
                }
                missing_args.insert(arg_name);
            }

            if !missing_args.is_empty() {
                let names = missing_args
                    .into_iter()
                    .map(String::from)
                    .sorted()
                    .collect();
                checker.add_check(Check::new(
                    CheckKind::DocumentAllArguments(names),
                    Range::from_located(parent),
                ));
            }
        }
    }
}
