//! Parsing for Google-style docstring sections.
//!
//! The [Google Python Style Guide](https://google.github.io/styleguide/pyguide.html#s3.8.3-functions-and-methods)
//! describes the canonical conventions but does not define a formal grammar. This parser recognizes
//! these parameter section headings:
//!
//! - `Args`, `Arguments`, and `Parameters`
//! - `Keyword Args` and `Keyword Arguments`
//! - `Other Args`, `Other Arguments`, and `Other Parameters`
//!
//! It accepts comma-separated Python names with optional parenthesized types, preserves
//! continuation text, and skips section-like text inside preformatted or container blocks. Other
//! known headings only delimit parameter sections; their contents are not parsed here.
//!
//! Example:
//!
//! ```text
//! Copy a file with retry controls.
//!
//! Args:
//!     source (str): Path to copy.
//!     destination: Destination path.
//!
//! Keyword Args:
//!     timeout, deadline (float): Time limits in seconds.
//!
//! Other Parameters:
//!     retries: Number of retries.
//! ```

use std::cmp::Ordering;

use indexmap::IndexMap;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::{TextRange, TextSize};

use super::SectionKind;
use super::preformatted::PreformattedBlockScanner;
use super::syntax::{
    ParsedLine, container_block_end, parse_parenthesized_type, parsed_lines,
    split_once_unbracketed_colon,
};

/// Returns parameter documentation from recognized Google-style parameter sections.
///
/// `normalized_source` must have already undergone PEP-257 trimming and universal newline
/// normalization.
pub(super) fn parameter_documentation(normalized_source: &str) -> IndexMap<String, String> {
    let lines = parsed_lines(normalized_source);
    let mut parameters = Parameters::default();
    for section in sections(&lines) {
        if matches!(
            section.kind,
            SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters
        ) {
            extend_parameter_documentation(&mut parameters, section.body);
        }
    }
    parameters.into_inner()
}

/// Returns recognized Google-style sections in source order.
///
/// `lines` must come from source that has already undergone PEP-257 trimming and universal
/// newline normalization (typically via `docstring::documentation_trim`).
pub(in crate::docstring) fn sections<'a>(
    lines: &'a [ParsedLine<'a>],
) -> impl Iterator<Item = Section<'a>> + 'a {
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut index = 0;

    std::iter::from_fn(move || {
        while index < lines.len() {
            // Skip blocks that "own" all internal content (in which we should not
            // recognize content that might otherwise look like a Google section header)
            if preformatted_blocks.consume_preformatted_line(lines[index].text) {
                index += 1;
                continue;
            }
            if let Some(end) = container_block_end(lines, index) {
                index = end;
                continue;
            }

            let Some(header) = parse_section_header(lines, index) else {
                preformatted_blocks.observe_line_outside_preformatted_block(lines[index].text);
                index += 1;
                continue;
            };

            let (range, body_end_line_index) = section_body_end(lines, header);
            index = body_end_line_index;
            if let HeaderKind::Structured(kind) = header.kind {
                return Some(Section {
                    kind,
                    body: &lines[header.body_start_line_index..body_end_line_index],
                    range,
                    header_indent: header.indent,
                });
            }
        }

        None
    })
}

/// A recognized Google-style docstring section.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::docstring) struct Section<'a> {
    /// The recognized section kind.
    pub(in crate::docstring) kind: SectionKind,
    /// The lines in the section body.
    pub(in crate::docstring) body: &'a [ParsedLine<'a>],
    /// The section's source range, including its header.
    pub(in crate::docstring) range: TextRange,
    /// The indentation of the section header.
    pub(in crate::docstring) header_indent: TextSize,
}

/// Extends `parameters` with the documented items in one parameter section body.
fn extend_parameter_documentation(parameters: &mut Parameters, lines: &[ParsedLine<'_>]) {
    let mut current: Option<(String, String)> = None;
    let mut item_indent = None;

    for line in lines {
        let trimmed = line.text.trim();

        // The first recognized item establishes the sibling indentation.
        // Each item at that indentation starts a new sibling and completes its predecessor.
        if item_indent.is_none_or(|indent| line.indent == indent)
            && let Some((names, description)) = parse_parameter(trimmed)
        {
            parameters.insert_documentation(
                current.replace((names.to_string(), description.to_string())),
            );
            item_indent = Some(line.indent);
            continue;
        }

        // Ignore prose until the first item has started.
        let Some((_, description)) = &mut current else {
            continue;
        };

        // Lines that are not sibling items extend the current description.
        // Empty lines preserve paragraph breaks.
        if !description.is_empty() && !description.ends_with('\n') {
            description.push('\n');
        }
        description.push_str(if trimmed.is_empty() { "\n" } else { trimmed });
    }

    // A following item completes its predecessor in the loop, so complete the final item here.
    parameters.insert_documentation(current);
}

/// Parses a parameter item into its display name and description.
fn parse_parameter(line: &str) -> Option<(&str, &str)> {
    let (name, description) = split_once_unbracketed_colon(line)?;
    let (display_name, _) = parse_parenthesized_type(name.trim());

    google_parameter_names(display_name)
        .is_some()
        .then_some((display_name, description.trim()))
}

/// Returns whether `name` is a valid Python parameter name, including variadic prefixes.
fn is_parameter_name(name: &str) -> bool {
    let identifier = name.strip_prefix('*').unwrap_or(name);
    let identifier = identifier.strip_prefix('*').unwrap_or(identifier);
    is_identifier(identifier)
}

#[derive(Default)]
struct Parameters(IndexMap<String, String>);

impl Parameters {
    /// Inserts a completed parameter item under each of its comma-separated names.
    fn insert_documentation(&mut self, parameter: Option<(String, String)>) {
        let Some((names, description)) = parameter else {
            return;
        };
        let description = description.trim();
        if !description.is_empty()
            && let Some(names) = google_parameter_names(&names)
        {
            for name in names {
                self.0.insert(name.to_string(), description.to_string());
            }
        }
    }

    fn into_inner(self) -> IndexMap<String, String> {
        self.0
    }
}

fn google_parameter_names(display_name: &str) -> Option<impl Iterator<Item = &str>> {
    let names = display_name.split(',').map(str::trim);
    names.clone().all(is_parameter_name).then_some(names)
}

/// Parses a recognized Google-style section header at `index`.
fn parse_section_header(lines: &[ParsedLine<'_>], index: usize) -> Option<SectionHeader> {
    let line = lines[index];
    let kind = section_kind(line.text)?;

    Some(SectionHeader {
        kind,
        indent: line.indent,
        body_start_line_index: index + 1,
        range: line.range,
    })
}

fn section_kind(line: &str) -> Option<HeaderKind> {
    let name = line.trim().strip_suffix(':')?.trim();
    HeaderKind::from_name(name)
}

/// Returns the section's source range and the index of the first line outside its body.
fn section_body_end(lines: &[ParsedLine<'_>], header: SectionHeader) -> (TextRange, usize) {
    let mut body_end_index = header.body_start_line_index;
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut item_indent = None;

    while let Some(line) = lines.get(body_end_index) {
        // Once a preformatted block begins, its contents cannot end the section.
        if preformatted_blocks.is_active()
            && preformatted_blocks.consume_preformatted_line(line.text)
        {
            body_end_index += 1;
            continue;
        }

        let Some((leading_blank_lines, line)) =
            section_body_continuation(&lines[body_end_index..], header, item_indent)
        else {
            break;
        };
        body_end_index += leading_blank_lines;

        item_indent = item_indent.or_else(|| section_item_indent(header, line));

        if !preformatted_blocks.consume_preformatted_line(line.text) {
            preformatted_blocks.observe_line_outside_preformatted_block(line.text);
        }
        body_end_index += 1;
    }

    let body = &lines[header.body_start_line_index..body_end_index];
    let range = match body.last() {
        Some(last) => header.range.cover(last.range),
        None => header.range,
    };
    (range, body_end_index)
}

/// Returns the number of leading blank lines and first nonblank line that continue
/// `header`'s body.
fn section_body_continuation<'a>(
    lines: &[ParsedLine<'a>],
    header: SectionHeader,
    item_indent: Option<TextSize>,
) -> Option<(usize, ParsedLine<'a>)> {
    let (leading_blank_lines, next_line) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| !line.text.trim().is_empty())?;

    if leading_blank_lines == 0 && section_header_ends_body(lines, 0, header) {
        return None;
    }

    if leading_blank_lines > 0
        && next_line.indent <= header.indent
        && (parse_section_header(lines, leading_blank_lines).is_some()
            || is_inline_section_header(next_line.text))
    {
        return None;
    }

    // Returns and yields have no item syntax that distinguishes an aligned body from prose
    // following an empty section.
    if leading_blank_lines > 0
        && next_line.indent <= header.indent
        && item_indent.is_none()
        && matches!(
            header.kind,
            HeaderKind::Structured(SectionKind::Returns | SectionKind::Yields)
        )
    {
        return None;
    }

    // A blank line ends a parameter section when the following aligned text is
    // not another parameter item.
    if leading_blank_lines > 0
        && matches!(
            header.kind,
            HeaderKind::Structured(
                SectionKind::Parameters
                    | SectionKind::KeywordArguments
                    | SectionKind::OtherParameters
            )
        )
        && item_indent == Some(next_line.indent)
        && section_item_indent(header, *next_line).is_none()
    {
        return None;
    }

    line_belongs_to_body(header, *next_line, item_indent)
        .then_some((leading_blank_lines, *next_line))
}

/// Returns whether a recognized header at `index` ends the current section body.
fn section_header_ends_body(lines: &[ParsedLine<'_>], index: usize, header: SectionHeader) -> bool {
    let Some(line) = lines.get(index) else {
        return false;
    };
    if line.indent <= header.indent && is_inline_section_header(line.text) {
        return true;
    }

    parse_section_header(lines, index).is_some_and(|next| next.indent <= header.indent)
}

/// Returns whether `line` belongs to `header` under Google-style indentation rules.
fn line_belongs_to_body(
    header: SectionHeader,
    line: ParsedLine<'_>,
    item_indent: Option<TextSize>,
) -> bool {
    match line.indent.cmp(&header.indent) {
        Ordering::Less => false,
        Ordering::Greater => true,
        Ordering::Equal => {
            let item_indent_matches_line = item_indent.is_none_or(|indent| indent == line.indent);
            let is_parameter_section = matches!(
                header.kind,
                HeaderKind::Structured(
                    SectionKind::Parameters
                        | SectionKind::KeywordArguments
                        | SectionKind::OtherParameters
                )
            );

            // Parameter sections can start with aligned prose before an item establishes the
            // sibling indentation. Once established, aligned lines must match that indentation.
            item_indent_matches_line
                && (is_parameter_section || section_item_indent(header, line).is_some())
        }
    }
}

/// Returns the indentation of an item recognized in the current section.
///
/// The first recognized item establishes the indentation for sibling items.
/// Item-like lines at a different indentation within the section are treated as
/// continuation text.
fn section_item_indent(header: SectionHeader, line: ParsedLine<'_>) -> Option<TextSize> {
    let trimmed = line.text.trim();
    let is_item = match header.kind {
        HeaderKind::Structured(
            SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters,
        ) => parse_parameter(trimmed).is_some(),
        HeaderKind::Structured(SectionKind::Attributes | SectionKind::Raises) => {
            split_once_unbracketed_colon(trimmed).is_some_and(|(name, _)| !name.trim().is_empty())
        }
        HeaderKind::Structured(SectionKind::Returns | SectionKind::Yields) => !trimmed.is_empty(),
        HeaderKind::Opaque => false,
    };
    is_item.then_some(line.indent)
}

/// Returns whether `line` is a recognized section header followed by inline content.
fn is_inline_section_header(line: &str) -> bool {
    let line = line.trim();
    // A trailing double colon introduces a reST literal block, not an inline section.
    if line.ends_with("::") {
        return false;
    }

    let Some((name, description)) = split_once_unbracketed_colon(line) else {
        return false;
    };

    let name = name.trim();
    let description = description.trim();
    !description.is_empty()
        && name.chars().next().is_some_and(char::is_uppercase)
        && HeaderKind::from_name(name).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SectionHeader {
    kind: HeaderKind,
    indent: TextSize,
    body_start_line_index: usize,
    range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderKind {
    Structured(SectionKind),
    Opaque,
}

impl HeaderKind {
    fn from_name(name: &str) -> Option<Self> {
        let normalized = name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();
        Some(match normalized.as_str() {
            "args" | "arguments" | "parameters" => Self::Structured(SectionKind::Parameters),
            "keyword args" | "keyword arguments" => Self::Structured(SectionKind::KeywordArguments),
            "other args" | "other arguments" | "other parameters" => {
                Self::Structured(SectionKind::OtherParameters)
            }
            "attributes" => Self::Structured(SectionKind::Attributes),
            "return" | "returns" => Self::Structured(SectionKind::Returns),
            "yield" | "yields" => Self::Structured(SectionKind::Yields),
            "raise" | "raises" => Self::Structured(SectionKind::Raises),
            "attention" | "caution" | "danger" | "error" | "example" | "examples" | "hint"
            | "important" | "methods" | "note" | "notes" | "references" | "see also" | "tip"
            | "todo" | "todos" | "warning" | "warnings" | "warns" => Self::Opaque,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use itertools::Itertools;
    use ruff_text_size::TextSize;

    use super::{SectionKind, parameter_documentation, parsed_lines, sections};

    #[test]
    fn extracts_aligned_parameter_items() {
        let raw = "\
Arguments:
first: First parameter.
Aligned continuation.
second: Second parameter.
Returns:
bool: Result.";

        assert_snapshot!(display_parameters(raw), @"
        first:
          │ First parameter.
          │ Aligned continuation.
        second:
          │ Second parameter.
        ");
    }

    #[test]
    fn uses_visual_indentation_for_parameter_items() {
        let raw = "\
Args:
  \tfirst: First parameter.
        second: Second parameter.";

        assert_snapshot!(display_parameters(raw), @"
        first:
          │ First parameter.
        second:
          │ Second parameter.
        ");
    }

    #[test]
    fn extracts_comma_separated_parameter_names() {
        let raw = "\
Args:
    x, y: Coordinates.";

        assert_snapshot!(display_parameters(raw), @"
        x:
          │ Coordinates.
        y:
          │ Coordinates.
        ");
    }

    #[test]
    fn ignores_prose_before_first_parameter() {
        let raw = "\
Args:
Partition into non-overlapping windows with padding if needed.
    hidden_states (tensor): Input tokens.";

        assert_snapshot!(display_parameters(raw), @"
        hidden_states:
          │ Input tokens.
        ");
    }

    #[test]
    fn extracts_parameter_with_unbalanced_type_brackets() {
        let raw = "\
Args:
    query_embeddings (`Union[torch.Tensor, list[torch.Tensor]`): Query embeddings.";

        assert_snapshot!(display_parameters(raw), @"
        query_embeddings:
          │ Query embeddings.
        ");
    }

    #[test]
    fn accepts_dashed_parameter_section_underline() {
        let raw = "\
Args:
----
    value: Parameter documentation.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Parameter documentation.
        ");
    }

    #[test]
    fn treats_invalid_parameter_name_as_continuation() {
        let raw = "\
Args:
    value: Initial documentation.
    value, for example: can be omitted.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Initial documentation.
          │ value, for example: can be omitted.
        ");
    }

    #[test]
    fn uses_last_documentation_for_duplicate_parameter() {
        let raw = "\
Args:
    value: First documentation.
    value: Replacement documentation.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Replacement documentation.
        ");
    }

    #[test]
    fn parses_parenthesized_type_with_quoted_parenthesis() {
        let raw = "\
Args:
    value (Literal[\"(\"]): Quoted parenthesis.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Quoted parenthesis.
        ");
    }

    #[test]
    fn ignores_callable_like_parameter_names() {
        let raw = "\
Args:
    callback() (Callable): Not a parameter.
    value: Documentation.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Documentation.
        ");
    }

    #[test]
    fn preserves_parameter_paragraph_breaks() {
        let raw = "\
Args:
    value: First paragraph.


        Second paragraph.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ First paragraph.
          │
          │
          │ Second paragraph.
        ");
    }

    #[test]
    fn recognizes_parameter_section_headings() {
        for heading in [
            "Args",
            "Arguments",
            "Parameters",
            "Keyword Args",
            "Keyword Arguments",
            "Other Args",
            "Other Arguments",
            "Other Parameters",
        ] {
            let raw = format!(
                "\
{heading}:
    value: Parameter documentation."
            );
            assert_parameter_documentation(&raw, &[("value", "Parameter documentation.")]);
        }
    }

    #[test]
    fn ends_parameter_section_at_structured_section_header() {
        assert_parameter_documentation(
            "\
Args:
    value: Parameter documentation.
Methods:
    helper: Method documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ends_unindented_parameter_section_at_aligned_prose() {
        assert_parameter_documentation(
            "\
Args:
value: Parameter documentation.

Additional details.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ends_indented_parameter_section_at_aligned_prose() {
        assert_parameter_documentation(
            "\
Args:
    value: Parameter documentation.

    Additional details.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ends_indented_parameter_section_at_inline_section_header() {
        assert_parameter_documentation(
            "\
Args:
    first: First parameter.
    last: Last parameter.

Returns: Result.",
            &[("first", "First parameter."), ("last", "Last parameter.")],
        );
    }

    #[test]
    fn ends_unindented_parameter_section_at_inline_section_header() {
        assert_parameter_documentation(
            "\
Args:
first: First parameter.
last: Last parameter.
Returns: Result.",
            &[("first", "First parameter."), ("last", "Last parameter.")],
        );
    }

    #[test]
    fn pep257_normalization_recognizes_shifted_sibling_section() {
        assert_parameter_documentation(
            "\
Note:
        context

    Args:
        value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn pep257_normalization_preserves_nested_section() {
        assert_parameter_documentation(
            "
    Note:
        context

        Args:
            nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn pep257_normalization_ends_section_at_dedented_header() {
        assert_parameter_documentation(
            "\
Args:
        value: Parameter documentation.
    Returns:
        bool: Result.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn adjacent_aligned_section_headers_are_siblings() {
        assert_parameter_documentation(
            "\
Example:
Args:
    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn finds_shifted_top_level_section() {
        assert_parameter_documentation(
            "\
A decoded newline follows:
This line starts at column zero.

    Keyword Args:
        shifted: Documentation in a shifted section.",
            &[("shifted", "Documentation in a shifted section.")],
        );
    }

    #[test]
    fn finds_parameter_section_after_first_line_literal_block() {
        // Regression for https://github.com/pytorch/pytorch/blob/e3f5bf0b18585511e6cd7d7a574ebf82f465e5ae/torch/_native/instrumentation.py#L365-L383
        assert_parameter_documentation(
            "\
Instrument a single ``@triton.jit`` kernel, stacked above the jit::

        @instrument_triton_kernel(\"aten::bmm\")
        @triton.jit
        def _bmm_kernel(...): ...

    A Triton kernel compiles lazily and caches variants on the kernel object.

    Args:
        op: Operator symbol being compiled for, e.g. ``\"aten::bmm\"``.",
            &[(
                "op",
                "Operator symbol being compiled for, e.g. ``\"aten::bmm\"``.",
            )],
        );
    }

    #[test]
    fn keeps_colon_prose_in_parameter_documentation() {
        assert_parameter_documentation(
            "\
Args:
    param1 (str): The first parameter description.
    For example: pass an absolute path.
    param2: The second parameter description.",
            &[
                (
                    "param1",
                    "The first parameter description.\nFor example: pass an absolute path.",
                ),
                ("param2", "The second parameter description."),
            ],
        );
    }

    #[test]
    fn keeps_rest_literal_blocks_in_parameter_documentation() {
        assert_parameter_documentation(
            "\
Args:
    value: Documentation.
        Example::
            Args:
                nested: Not parameter documentation.
    other: Other documentation.",
            &[
                (
                    "value",
                    "Documentation.\nExample::\nArgs:\nnested: Not parameter documentation.",
                ),
                ("other", "Other documentation."),
            ],
        );
    }

    #[test]
    fn extracts_variadic_parameters() {
        assert_parameter_documentation(
            "\
Args:
    *args: Extra positional arguments.
    **kwargs: Extra keyword arguments.",
            &[
                ("*args", "Extra positional arguments."),
                ("**kwargs", "Extra keyword arguments."),
            ],
        );
    }

    #[test]
    fn ignores_parameter_section_nested_in_container_section() {
        assert_parameter_documentation(
            "\
Example:
    Args:
        nested: Not parameter documentation.
Args:
    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ignores_parameter_section_in_rest_directive() {
        assert_parameter_documentation(
            "\
Summary.

.. note::
    Args:
        nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_after_blank_line_in_rest_directive() {
        assert_parameter_documentation(
            "\
Summary.

.. note::

        Keyword Args:
            nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_in_unordered_markdown_list_item() {
        assert_parameter_documentation(
            "\
Summary.

- Example:
    Args:
        nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_after_blank_line_in_markdown_list_item() {
        assert_parameter_documentation(
            "\
Summary.

- Example:

        Args:
            nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_in_ordered_markdown_list_item() {
        assert_parameter_documentation(
            "\
Summary.

1. Example:
    Args:
        nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_in_rest_field_list() {
        assert_parameter_documentation(
            "\
Summary.

:param value: Example input.
    Args:
        nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_in_rest_literal_block() {
        assert_parameter_documentation(
            "\
Summary.

Example::

        Args:
            nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn resumes_after_markdown_fence() {
        assert_parameter_documentation(
            "\
Summary.

    ```text
    Args:
        nested: Not parameter documentation.
    ```

    Args:
        value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn resumes_after_rest_literal_block() {
        assert_parameter_documentation(
            "\
Summary.

    Example::

        Args:
            nested: Not parameter documentation.

    Args:
        value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn resumes_after_rest_directive() {
        assert_parameter_documentation(
            "\
.. note::
    Args:
        nested: Not parameter documentation.
Args:
    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn resumes_after_unindented_markdown_fence_following_rest_literal_marker() {
        assert_parameter_documentation(
            "\
Example::

```
sample
```

Args:
    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn backticks_in_fence_info_do_not_hide_parameter_sections() {
        assert_parameter_documentation(
            "\
```PRNGKey`` is accepted.

Args:
    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ignores_doctest_content_and_resumes_after_it() {
        assert_parameter_documentation(
            "        >>> example()
        Args:
            nested: Not parameter documentation.

        Args:
            value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn returns_structured_section_kinds_in_source_order() {
        let raw = "\
Args:
    value: Documentation.
Keyword Args:
    option: Optional.
Other Parameters:
    other: Other.
Returns:
    bool: Result.";
        let lines = parsed_lines(raw);
        let kinds = sections(&lines)
            .map(|section| section.kind)
            .collect::<Vec<_>>();

        assert_eq!(
            kinds,
            [
                SectionKind::Parameters,
                SectionKind::KeywordArguments,
                SectionKind::OtherParameters,
                SectionKind::Returns,
            ]
        );
    }

    #[test]
    fn returns_section_body_range_and_header_indent() {
        let raw = "    Args:
        value: Documentation.
Methods:
    helper: Method documentation.";
        let lines = parsed_lines(raw);
        let sections = sections(&lines)
            .map(|section| {
                (
                    section.kind,
                    section
                        .body
                        .iter()
                        .map(|line| line.text)
                        .collect::<Vec<_>>(),
                    &raw[section.range],
                    section.header_indent,
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            sections,
            vec![(
                SectionKind::Parameters,
                vec!["        value: Documentation."],
                "    Args:\n        value: Documentation.",
                TextSize::new(4),
            )]
        );
    }

    #[test]
    fn ends_populated_return_section_at_aligned_prose() {
        let raw = "\
Returns:
    bool: Result.
Additional details.";
        let lines = parsed_lines(raw);
        let sections = sections(&lines)
            .map(|section| (section.kind, &raw[section.range]))
            .collect::<Vec<_>>();

        assert_eq!(
            sections,
            vec![(
                SectionKind::Returns,
                "\
Returns:
    bool: Result.",
            )]
        );
    }

    fn display_parameters(raw: &str) -> String {
        let normalized_source = crate::docstring::documentation_trim(raw);
        parameter_documentation(&normalized_source)
            .into_iter()
            .map(|(name, documentation)| {
                let documentation = documentation
                    .lines()
                    .map(|line| match line {
                        "" => "  │".to_string(),
                        _ => format!("  │ {line}"),
                    })
                    .join("\n");
                format!("{name}:\n{documentation}")
            })
            .join("\n")
    }

    #[track_caller]
    fn assert_parameter_documentation(raw: &str, expected: &[(&str, &str)]) {
        let normalized_source = crate::docstring::documentation_trim(raw);
        let parameters = parameter_documentation(&normalized_source);
        assert_eq!(parameters.len(), expected.len(), "{raw}");

        for &(name, documentation) in expected {
            assert_eq!(
                parameters.get(name).map(String::as_str),
                Some(documentation),
                "{raw}"
            );
        }
    }
}
