use crate::docstrings::helpers;
use crate::docstrings::styles::SectionStyle;

#[derive(Debug)]
pub(crate) struct SectionContext<'a> {
    pub(crate) section_name: String,
    pub(crate) previous_line: &'a str,
    pub(crate) line: &'a str,
    pub(crate) following_lines: &'a [&'a str],
    pub(crate) is_last_section: bool,
    pub(crate) original_index: usize,
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
