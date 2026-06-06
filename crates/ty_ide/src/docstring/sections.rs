use std::borrow::Cow;

use super::markdown;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct DocstringSections<'a> {
    parameters: Vec<DocstringItem<'a>>,
    attributes: Vec<DocstringItem<'a>>,
    returns: Vec<DocstringItem<'a>>,
    raises: Vec<DocstringItem<'a>>,
}

impl<'a> DocstringSections<'a> {
    pub(super) fn push(&mut self, kind: DocstringSectionKind, item: DocstringItem<'a>) {
        if item.is_empty() {
            return;
        }

        match kind {
            DocstringSectionKind::Parameters => self.parameters.push(item),
            DocstringSectionKind::Attributes => self.attributes.push(item),
            DocstringSectionKind::Returns => self.returns.push(item),
            DocstringSectionKind::Raises => self.raises.push(item),
        }
    }

    pub(super) fn render_markdown(&self) -> String {
        let mut output = String::new();
        render_markdown_section(&mut output, "Parameters", &self.parameters);
        render_markdown_section(&mut output, "Attributes", &self.attributes);
        render_markdown_section(&mut output, "Returns", &self.returns);
        render_markdown_section(&mut output, "Raises", &self.raises);
        output
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DocstringSectionKind {
    Parameters,
    Attributes,
    Returns,
    Raises,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DocstringItem<'a> {
    name: Option<&'a str>,
    ty: Option<&'a str>,
    description: &'a str,
}

impl<'a> DocstringItem<'a> {
    pub(super) fn new(name: Option<&'a str>, ty: Option<&'a str>, description: &'a str) -> Self {
        Self {
            name,
            ty,
            description,
        }
    }

    fn is_empty(&self) -> bool {
        self.name.is_none() && self.ty.is_none_or(str::is_empty) && self.description.is_empty()
    }

    fn render_into(&self, output: &mut String) {
        let mut has_label = false;

        if let Some(name) = self.name {
            render_code_span_into(output, name);
            has_label = true;
        }

        if let Some(ty) = self.ty
            && !ty.is_empty()
        {
            if has_label {
                output.push_str(" (");
                render_type_code_span_into(output, ty);
                output.push(')');
            } else {
                render_type_code_span_into(output, ty);
                has_label = true;
            }
        }

        if !self.description.is_empty() {
            let starts_with_block = description_starts_with_block_content(self.description);

            // Choose the separator between the label and the description
            if has_label {
                output.push_str(if starts_with_block { ":\n" } else { ": " });
            }

            if starts_with_block {
                // Render block descriptions verbatim
                output.push_str(self.description);
            } else if self.description.contains('\n') {
                // Indent continuation lines in non-block descriptions
                output.push_str(&self.description.replace('\n', "\n    "));
            } else {
                output.push_str(self.description);
            }
        }
    }
}

fn render_markdown_section(output: &mut String, heading: &str, fields: &[DocstringItem<'_>]) {
    if fields.is_empty() {
        return;
    }

    if !output.is_empty() {
        output.push_str("\n\n");
    }

    output.push_str("## ");
    output.push_str(heading);
    output.push('\n');

    let mut previous_description = None;

    // Render each field into the output with the appropriate spacing between fields.
    for field in fields {
        if let Some(description) = previous_description {
            output.push_str(if description_needs_blank_before_next_field(description) {
                // Add an extra newline to keep the next field out of an open block.
                "\n\n"
            } else {
                "\n"
            });
        }

        field.render_into(output);
        previous_description = Some(field.description);
    }
}

fn description_needs_blank_before_next_field(description: &str) -> bool {
    description_leaves_doctest_open(description)
        || description_starts_with_markdown_list(description)
}

/// Detects whether or not the given text leaves a doctest block open.
///
/// Doctest markers that appear within a Markdown fence are ignored.
fn description_leaves_doctest_open(description: &str) -> bool {
    let mut markdown_fence: Option<markdown::MarkdownFence<'_>> = None;
    let mut in_doctest = false;

    for line in description.lines().map(|line| line.trim_start_matches(' ')) {
        if let Some(fence) = markdown_fence {
            if fence.is_closed_by(line) {
                markdown_fence = None;
            }
        } else if in_doctest {
            if line.is_empty() {
                in_doctest = false;
            }
        } else if line.starts_with(">>>") {
            in_doctest = true;
        } else if let Some(fence) = markdown::MarkdownFence::find(line) {
            markdown_fence = Some(fence);
        }
    }

    in_doctest
}

fn description_starts_with_block_content(description: &str) -> bool {
    description.lines().next().is_some_and(|first_line| {
        let first_line = first_line.trim_start_matches(' ');
        markdown::MarkdownFence::find(first_line).is_some()
            || first_line.starts_with(">>>")
            || starts_with_markdown_list_item(first_line)
    })
}

fn description_starts_with_markdown_list(description: &str) -> bool {
    description.lines().next().is_some_and(|first_line| {
        starts_with_markdown_list_item(first_line.trim_start_matches(' '))
    })
}

fn starts_with_markdown_list_item(line: &str) -> bool {
    starts_with_unordered_markdown_list_item(line) || starts_with_ordered_markdown_list_item(line)
}

fn starts_with_unordered_markdown_list_item(line: &str) -> bool {
    matches!(
        line.as_bytes(),
        [b'-' | b'+' | b'*'] | [b'-' | b'+' | b'*', b' ' | b'\t', ..]
    )
}

fn starts_with_ordered_markdown_list_item(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut digit_count = 0;

    for byte in bytes {
        if digit_count < 9 && byte.is_ascii_digit() {
            digit_count += 1;
            continue;
        }

        if digit_count > 0 && matches!(*byte, b'.' | b')') {
            return bytes
                .get(digit_count + 1)
                .is_none_or(|byte| matches!(*byte, b' ' | b'\t'));
        }

        return false;
    }

    false
}

fn render_type_code_span_into(output: &mut String, ty: &str) {
    let normalized = normalize_type_for_code_span(ty);
    render_code_span_into(output, normalized.as_ref());
}

/// Normalizes type text so it fits in a single Markdown code span.
///
/// One-line types are returned unchanged. Multi-line types are trimmed line by
/// line, with empty lines discarded and remaining lines joined by a single
/// space.
///
/// For example:
///
/// ```python
/// dict[str,
///     object]
/// ```
///
/// becomes `dict[str, object]`.
fn normalize_type_for_code_span(ty: &str) -> Cow<'_, str> {
    if !ty.contains('\n') {
        return Cow::Borrowed(ty);
    }

    let mut normalized = String::new();
    for line in ty.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !normalized.is_empty() {
            normalized.push(' ');
        }
        normalized.push_str(line);
    }

    Cow::Owned(normalized)
}

/// Wraps `text` in a Markdown code span and appends it to output.
fn render_code_span_into(output: &mut String, text: &str) {
    // This chooses the number of backticks that we use to delimit the start and
    // end of the inline Markdown code span.
    //
    // The number we pick is one greater than the longest run of consecutive
    // backticks in `text`, which guarantees that we can wrap `text` unambiguously.
    let delimiter_len = text
        .split(|char| char != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
        + 1;

    output.extend(std::iter::repeat_n('`', delimiter_len));
    if text.starts_with('`') || text.ends_with('`') {
        // Per the CommonMark spec, wrap the contents of the code span in
        // whitespace if those contents start or end with backticks.
        //
        // <https://spec.commonmark.org/0.31.2/#code-spans>
        output.push(' ');
        output.push_str(text);
        output.push(' ');
    } else {
        output.push_str(text);
    }
    output.extend(std::iter::repeat_n('`', delimiter_len));
}

#[cfg(test)]
mod tests {
    use insta::{Settings, assert_snapshot};

    use super::{DocstringItem, DocstringSectionKind, DocstringSections};

    #[test]
    fn sections_render_in_canonical_order() {
        let mut sections = DocstringSections::default();
        sections.push(
            DocstringSectionKind::Raises,
            DocstringItem::new(Some("ValueError"), None, "Invalid value."),
        );
        sections.push(
            DocstringSectionKind::Parameters,
            DocstringItem::new(Some("value"), Some("str"), "The value."),
        );
        sections.push(
            DocstringSectionKind::Returns,
            DocstringItem::new(None, Some("bool"), "Whether validation passed."),
        );
        sections.push(
            DocstringSectionKind::Attributes,
            DocstringItem::new(Some("cache"), Some("dict[str,\n object]"), "Cached data."),
        );

        assert_snapshot!(sections.render_markdown(), @"
        ## Parameters
        `value` (`str`): The value.

        ## Attributes
        `cache` (`dict[str, object]`): Cached data.

        ## Returns
        `bool`: Whether validation passed.

        ## Raises
        `ValueError`: Invalid value.
        ");
    }

    #[test]
    fn sections_skip_empty_items() {
        let mut sections = DocstringSections::default();
        sections.push(
            DocstringSectionKind::Parameters,
            DocstringItem::new(None, None, ""),
        );
        sections.push(
            DocstringSectionKind::Returns,
            DocstringItem::new(None, Some(""), ""),
        );

        assert_eq!(sections.render_markdown(), "");
    }

    #[test]
    fn sections_render_multiline_and_block_descriptions() {
        let mut settings = Settings::clone_current();
        settings.add_filter("\n    \n", "\n<INDENTED-BLANK>\n");
        let _snap = settings.bind_to_scope();

        let mut sections = DocstringSections::default();
        sections.push(
            DocstringSectionKind::Parameters,
            DocstringItem::new(
                Some("`value`"),
                None,
                "First sentence.\nContinued sentence.\n\nSecond paragraph.",
            ),
        );
        sections.push(
            DocstringSectionKind::Parameters,
            DocstringItem::new(
                Some("example"),
                None,
                "```python\nif ok:\n    do_work()\n```",
            ),
        );
        sections.push(
            DocstringSectionKind::Parameters,
            DocstringItem::new(Some("prompt"), None, ">>> print('prompt')"),
        );
        sections.push(
            DocstringSectionKind::Parameters,
            DocstringItem::new(Some("choices"), None, "- first\n- second"),
        );
        sections.push(
            DocstringSectionKind::Parameters,
            DocstringItem::new(Some("steps"), None, "1. first\n2. second"),
        );
        sections.push(
            DocstringSectionKind::Parameters,
            DocstringItem::new(Some("other"), None, "Another parameter."),
        );

        assert_snapshot!(sections.render_markdown(), @r#"
        ## Parameters
        `` `value` ``: First sentence.
            Continued sentence.
        <INDENTED-BLANK>
            Second paragraph.
        `example`:
        ```python
        if ok:
            do_work()
        ```
        `prompt`:
        >>> print('prompt')

        `choices`:
        - first
        - second

        `steps`:
        1. first
        2. second

        `other`: Another parameter.
        "#);
    }
}
