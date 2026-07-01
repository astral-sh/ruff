use indexmap::IndexMap;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::TextRange;

use super::SectionKind;
use super::preformatted::PreformattedBlockScanner;
use super::rst::is_field_list_marker;
use super::syntax::{
    ParsedLine, indentation, parse_parenthesized_type, parsed_lines, split_once_unbracketed_colon,
    starts_with_markdown_list_item,
};

/// Returns parameter documentation from recognized Google-style parameter sections.
pub(super) fn parameter_documentation(raw: &str) -> IndexMap<String, String> {
    let mut parameters = IndexMap::new();

    visit_sections(raw, |kind, _, _, body| {
        if matches!(
            kind,
            SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters
        ) {
            extend_parameter_documentation(&mut parameters, body);
        }
    });

    parameters
}

/// Visits recognized Google-style sections in source order.
pub(in crate::docstring) fn visit_sections<'a>(
    raw: &'a str,
    mut visit: impl FnMut(SectionKind, TextRange, usize, &[ParsedLine<'a>]),
) {
    let lines = parsed_lines(raw);
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut index = 0;

    while index < lines.len() {
        // Skip containers whose contents may look like Google sections but are not top-level
        // siblings.
        if preformatted_blocks.consume_preformatted_line(lines[index].text) {
            index += 1;
            continue;
        }

        if let Some(section_end) = non_google_underlined_section_end(&lines, index) {
            index = section_end;
            continue;
        }
        if let Some(block_end) = indented_non_google_block_end(&lines, index) {
            index = block_end;
            continue;
        }

        let Some(header) = parse_google_section_like_header(&lines, index) else {
            preformatted_blocks.observe_line_outside_preformatted_block(lines[index].text);
            index += 1;
            continue;
        };
        let (body_end, range) = google_section_body_end(&lines, header);
        if let GoogleSectionHeaderKind::Supported(kind) = header.kind {
            visit(
                kind,
                range,
                header.indent,
                &lines[header.body_start..body_end],
            );
        }
        index = body_end;
    }
}

/// Returns whether `name` is a valid Python parameter name, including variadic prefixes.
pub(in crate::docstring) fn is_parameter_name(name: &str) -> bool {
    let identifier = name
        .strip_prefix("**")
        .or_else(|| name.strip_prefix('*'))
        .unwrap_or(name);

    is_identifier(identifier)
}

fn non_google_underlined_section_end(lines: &[ParsedLine<'_>], index: usize) -> Option<usize> {
    let header_indent = non_google_underlined_section_indent(lines, index)?;

    let mut section_end = index + 2;
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    while section_end < lines.len() {
        if preformatted_blocks.consume_preformatted_line(lines[section_end].text) {
            section_end += 1;
            continue;
        }

        if section_end > index + 2 {
            if non_google_underlined_section_indent(lines, section_end)
                .is_some_and(|indent| indent <= header_indent)
            {
                break;
            }

            // A blank-separated Google header at the same or lower indentation is a sibling.
            if lines[section_end - 1].text.trim().is_empty()
                && parse_google_section_like_header(lines, section_end)
                    .is_some_and(|header| header.indent <= header_indent)
            {
                break;
            }
        }
        preformatted_blocks.observe_line_outside_preformatted_block(lines[section_end].text);
        section_end += 1;
    }
    Some(section_end)
}

fn non_google_underlined_section_indent(lines: &[ParsedLine<'_>], index: usize) -> Option<usize> {
    let header = lines.get(index)?;
    let underline = lines.get(index + 1)?;
    let header_indent = indentation(header.text);
    let underline_text = underline.text.trim();

    (!header.text.trim().is_empty()
        && !header.text.trim_end().ends_with(':')
        && indentation(underline.text) == header_indent
        && underline_text.len() >= 3
        && underline_text
            .chars()
            .all(|character| matches!(character, '-' | '=')))
    .then_some(header_indent)
}

fn indented_non_google_block_end(lines: &[ParsedLine<'_>], index: usize) -> Option<usize> {
    let marker = lines.get(index)?;
    if !is_rest_directive_marker(marker.text)
        && !is_field_list_marker(marker.text)
        && !starts_with_markdown_list_item(marker.text.trim_start())
    {
        return None;
    }

    let marker_indent = indentation(marker.text);
    Some(
        lines[index + 1..]
            .iter()
            .position(|line| {
                !line.text.trim().is_empty() && indentation(line.text) <= marker_indent
            })
            .map_or(lines.len(), |offset| index + 1 + offset),
    )
}

fn is_rest_directive_marker(line: &str) -> bool {
    let Some(directive) = line.trim_start().strip_prefix(".. ") else {
        return false;
    };
    let Some((name, _)) = directive.split_once("::") else {
        return false;
    };

    !name.is_empty() && !name.chars().any(char::is_whitespace)
}

fn google_section_body_end(
    lines: &[ParsedLine<'_>],
    header: GoogleSectionHeader,
) -> (usize, TextRange) {
    let mut body_end = header.body_start;
    let mut body_preformatted_blocks = PreformattedBlockScanner::default();
    let mut item_indent = None;
    let mut aligned_unsupported_body = false;

    while let Some(line) = lines.get(body_end) {
        // Once a preformatted block begins, its contents do not participate in indentation or
        // sibling-header checks.
        if body_preformatted_blocks.is_active()
            && body_preformatted_blocks.consume_preformatted_line(line.text)
        {
            body_end += 1;
            continue;
        }

        if line.text.trim().is_empty() {
            // A blank line belongs to the section only when the next non-blank line does.
            if !google_blank_line_continues_section(
                &lines[body_end..],
                header,
                item_indent,
                aligned_unsupported_body,
            ) {
                break;
            }

            while let Some(blank_line) = lines.get(body_end)
                && blank_line.text.trim().is_empty()
            {
                body_end += 1;
            }
            continue;
        }

        // PEP 257 can align an unsupported section's first body line with its header. Once found,
        // keep accepting that aligned body shape.
        let can_start_aligned_unsupported_body = body_end == header.body_start;
        if google_section_header_ends_body(
            lines,
            body_end,
            header,
            item_indent,
            aligned_unsupported_body || can_start_aligned_unsupported_body,
        ) {
            break;
        }

        if !google_line_belongs_to_body(
            header,
            line.text,
            item_indent,
            aligned_unsupported_body || can_start_aligned_unsupported_body,
        ) {
            break;
        }

        aligned_unsupported_body |= is_aligned_unsupported_section_body(header, line.text);

        // The first item fixes the indentation used to distinguish aligned items from sibling
        // section headers.
        item_indent = item_indent.or_else(|| google_section_item_indent(header, line.text));

        if !body_preformatted_blocks.consume_preformatted_line(line.text) {
            body_preformatted_blocks.observe_line_outside_preformatted_block(line.text);
        }
        body_end += 1;
    }

    let range = lines[header.body_start..body_end]
        .last()
        .map_or(header.range, |line| {
            TextRange::new(header.range.start(), line.range.end())
        });
    (body_end, range)
}

fn google_blank_line_continues_section(
    lines: &[ParsedLine<'_>],
    header: GoogleSectionHeader,
    item_indent: Option<usize>,
    aligned_unsupported_body: bool,
) -> bool {
    let Some((offset, non_blank_line)) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| !line.text.trim().is_empty())
    else {
        return false;
    };

    let next_indent = indentation(non_blank_line.text);
    if next_indent <= header.indent {
        // A blank line disambiguates lowercase section names from same-indent parameters.
        if parse_google_section_like_header(lines, offset).is_some()
            || is_inline_google_section_header(non_blank_line.text)
        {
            return false;
        }

        // Unlike named sections, returns and yields have no item syntax that can distinguish a
        // same-indent body from prose following an empty section.
        if item_indent.is_none()
            && matches!(
                header.kind,
                GoogleSectionHeaderKind::Supported(SectionKind::Returns | SectionKind::Yields)
            )
        {
            return false;
        }
    }

    // A blank line separates prose aligned with parameter items from the section body.
    if item_indent == Some(next_indent)
        && matches!(
            header.kind,
            GoogleSectionHeaderKind::Supported(
                SectionKind::Parameters
                    | SectionKind::KeywordArguments
                    | SectionKind::OtherParameters
            )
        )
        && google_section_item_indent(header, non_blank_line.text).is_none()
    {
        return false;
    }

    google_line_belongs_to_body(
        header,
        non_blank_line.text,
        item_indent,
        aligned_unsupported_body,
    )
}

fn google_section_header_ends_body(
    lines: &[ParsedLine<'_>],
    index: usize,
    header: GoogleSectionHeader,
    item_indent: Option<usize>,
    aligned_unsupported_body: bool,
) -> bool {
    let Some(line) = lines.get(index) else {
        return false;
    };
    if aligned_unsupported_body && is_aligned_unsupported_section_body(header, line.text) {
        return false;
    }
    let prefer_item = prefer_section_item_over_section_header(header, line.text, item_indent);
    if indentation(line.text) <= header.indent && is_inline_google_section_header(line.text) {
        return !prefer_item;
    }

    let Some(next) = parse_google_section_like_header(lines, index) else {
        return false;
    };

    next.indent <= header.indent && (next.underlined || !prefer_item)
}

fn google_line_belongs_to_body(
    header: GoogleSectionHeader,
    line: &str,
    item_indent: Option<usize>,
    aligned_unsupported_body: bool,
) -> bool {
    let line_indent = indentation(line);
    // PEP 257 can align a first-line parameter section with its body. Once an item establishes
    // that layout, same-indent continuation lines remain part of the section.
    let item_indent_matches_line = item_indent.is_none_or(|indent| indent == line_indent);
    let is_same_indent_parameter_continuation = item_indent == Some(line_indent)
        && matches!(
            header.kind,
            GoogleSectionHeaderKind::Supported(
                SectionKind::Parameters
                    | SectionKind::KeywordArguments
                    | SectionKind::OtherParameters
            )
        );
    (aligned_unsupported_body && is_aligned_unsupported_section_body(header, line))
        || line_indent > header.indent
        || (line_indent == header.indent
            && (is_same_indent_parameter_continuation
                || (item_indent_matches_line
                    && google_section_item_indent(header, line).is_some())))
}

fn is_aligned_unsupported_section_body(header: GoogleSectionHeader, line: &str) -> bool {
    header.kind == GoogleSectionHeaderKind::Unsupported && indentation(line) == header.indent
}

/// Returns whether an ambiguous section header should be treated as an item in the current section.
fn prefer_section_item_over_section_header(
    header: GoogleSectionHeader,
    line: &str,
    item_indent: Option<usize>,
) -> bool {
    let line_indent = indentation(line);
    matches!(
        header.kind,
        GoogleSectionHeaderKind::Supported(
            SectionKind::Parameters
                | SectionKind::KeywordArguments
                | SectionKind::OtherParameters
                | SectionKind::Attributes
                | SectionKind::Raises
        )
    ) && line_indent == header.indent
        && item_indent.is_none_or(|indent| indent == line_indent)
        && google_section_item_indent(header, line).is_some()
        && (line.trim().chars().next().is_some_and(char::is_lowercase)
            || (matches!(
                header.kind,
                GoogleSectionHeaderKind::Supported(SectionKind::Raises)
            ) && split_once_unbracketed_colon(line.trim())
                .is_some_and(|(name, _)| has_exception_name_suffix(name.trim()))))
}

/// Returns the indentation of an item recognized in the current Google-style section.
fn google_section_item_indent(header: GoogleSectionHeader, line: &str) -> Option<usize> {
    let trimmed = line.trim();
    let is_item = match header.kind {
        GoogleSectionHeaderKind::Supported(
            SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters,
        ) => parse_google_parameter(trimmed).is_some(),
        GoogleSectionHeaderKind::Supported(SectionKind::Attributes | SectionKind::Raises) => {
            split_once_unbracketed_colon(trimmed).is_some_and(|(name, _)| !name.trim().is_empty())
        }
        GoogleSectionHeaderKind::Supported(SectionKind::Returns | SectionKind::Yields) => {
            !trimmed.is_empty()
        }
        GoogleSectionHeaderKind::Unsupported => false,
    };
    is_item.then(|| indentation(line))
}

fn is_google_section_underline(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty() && line.chars().all(|character| matches!(character, '-' | '='))
}

fn parse_google_section_like_header(
    lines: &[ParsedLine<'_>],
    index: usize,
) -> Option<GoogleSectionHeader> {
    let line = lines.get(index)?;
    let kind = google_section_kind(line.text)?;
    let underline = lines
        .get(index + 1)
        .filter(|line| is_google_section_underline(line.text));

    Some(GoogleSectionHeader {
        kind,
        indent: indentation(line.text),
        body_start: index + 1 + usize::from(underline.is_some()),
        range: underline.map_or(line.range, |underline| {
            TextRange::new(line.range.start(), underline.range.end())
        }),
        underlined: underline.is_some(),
    })
}

fn google_section_kind(line: &str) -> Option<GoogleSectionHeaderKind> {
    let name = line.trim().strip_suffix(':')?.trim();
    google_section_kind_from_name(name)
}

fn google_section_kind_from_name(name: &str) -> Option<GoogleSectionHeaderKind> {
    let normalized = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let kind = match normalized.as_str() {
        "args" | "arguments" | "parameters" => {
            GoogleSectionHeaderKind::Supported(SectionKind::Parameters)
        }
        "keyword args" | "keyword arguments" => {
            GoogleSectionHeaderKind::Supported(SectionKind::KeywordArguments)
        }
        "other args" | "other arguments" | "other parameters" => {
            GoogleSectionHeaderKind::Supported(SectionKind::OtherParameters)
        }
        "attributes" => GoogleSectionHeaderKind::Supported(SectionKind::Attributes),
        "return" | "returns" => GoogleSectionHeaderKind::Supported(SectionKind::Returns),
        "yield" | "yields" => GoogleSectionHeaderKind::Supported(SectionKind::Yields),
        "raise" | "raises" => GoogleSectionHeaderKind::Supported(SectionKind::Raises),
        "attention" | "caution" | "danger" | "error" | "example" | "examples" | "hint"
        | "important" | "methods" | "note" | "notes" | "references" | "see also" | "tip"
        | "todo" | "todos" | "warning" | "warnings" | "warns" => {
            GoogleSectionHeaderKind::Unsupported
        }
        _ => return None,
    };
    Some(kind)
}

fn is_inline_google_section_header(line: &str) -> bool {
    let Some((name, description)) = split_once_unbracketed_colon(line.trim()) else {
        return false;
    };
    let name = name.trim();
    let description = description.trim();

    !description.is_empty()
        && !description.starts_with(':')
        && name.chars().next().is_some_and(char::is_uppercase)
        && google_section_kind_from_name(name).is_some()
}

/// Returns whether `line` is a recognized Google-style section header.
pub(in crate::docstring) fn is_section_like_header(line: &str) -> bool {
    google_section_kind(line).is_some() || is_inline_google_section_header(line)
}

/// Returns whether `name` ends with a conventional exception-class suffix.
pub(in crate::docstring) fn has_exception_name_suffix(name: &str) -> bool {
    ["Error", "Exception", "Warning"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

/// Parses a Google-style parameter item into its display name and inline description.
fn parse_google_parameter(line: &str) -> Option<(&str, &str)> {
    let (name, description) = split_once_unbracketed_colon(line)?;
    let name = name.trim();
    let (display_name, _) = parse_parenthesized_type(name);
    if !google_parameter_names(display_name).all(is_parameter_name) {
        return None;
    }

    Some((display_name, description.trim()))
}

fn extend_parameter_documentation(parameters: &mut IndexMap<String, String>, lines: &[ParsedLine]) {
    let mut current: Option<(String, String)> = None;
    let mut item_indent = None;

    // The first parameter fixes the item indentation. A colon on a differently indented line is
    // continuation prose rather than the start of a new item.
    for line in lines {
        let line = line.text;
        let trimmed = line.trim();
        let line_indent = indentation(line);

        if trimmed.is_empty() {
            if let Some(current) = &mut current {
                if !current.1.is_empty() && !current.1.ends_with('\n') {
                    current.1.push('\n');
                }
                current.1.push('\n');
            }
            continue;
        }

        if item_indent.is_none_or(|indent| line_indent == indent)
            && let Some((names, description)) = parse_google_parameter(trimmed)
        {
            insert_parameter_documentation(
                parameters,
                current.replace((names.to_string(), description.to_string())),
            );
            item_indent.get_or_insert(line_indent);
            continue;
        }

        if let Some(current) = &mut current {
            if !current.1.is_empty() && !current.1.ends_with('\n') {
                current.1.push('\n');
            }
            current.1.push_str(trimmed);
        }
    }

    insert_parameter_documentation(parameters, current);
}

fn insert_parameter_documentation(
    parameters: &mut IndexMap<String, String>,
    parameter: Option<(String, String)>,
) {
    let Some((names, description)) = parameter else {
        return;
    };
    let description = description.trim();
    if description.is_empty() {
        return;
    }
    for name in google_parameter_names(&names) {
        parameters.insert(name.to_string(), description.to_string());
    }
}

fn google_parameter_names(display_name: &str) -> impl Iterator<Item = &str> {
    display_name.split(',').map(str::trim)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GoogleSectionHeader {
    kind: GoogleSectionHeaderKind,
    indent: usize,
    body_start: usize,
    range: TextRange,
    underlined: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GoogleSectionHeaderKind {
    Supported(SectionKind),
    Unsupported,
}

pub(in crate::docstring) fn is_dotted_identifier(name: &str) -> bool {
    !name.is_empty() && name.split('.').all(is_identifier)
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::{SectionKind, parameter_documentation, visit_sections};

    #[test]
    fn parameter_documentation_preserves_consecutive_blank_lines() {
        assert_parameter_documentation(
            "\
Args:
    value: First paragraph.


        Second paragraph.",
            &[("value", "First paragraph.\n\n\nSecond paragraph.")],
        );
    }

    #[test]
    fn parameter_documentation_accepts_same_indent_items() {
        assert_parameter_documentation(
            "\
Arguments:
first: First parameter.
Aligned continuation.
second: Second parameter.
Returns:
bool: Result.",
            &[
                ("first", "First parameter.\nAligned continuation."),
                ("second", "Second parameter."),
            ],
        );
    }

    #[test]
    fn parameter_documentation_accepts_visually_aligned_mixed_indentation() {
        assert_parameter_documentation(
            "Args:\n  \tfirst: First parameter.\n        second: Second parameter.",
            &[
                ("first", "First parameter."),
                ("second", "Second parameter."),
            ],
        );
    }

    #[test]
    fn parameter_documentation_accepts_grouped_items() {
        assert_parameter_documentation(
            "\
Args:
    x, y: Coordinates.",
            &[("x", "Coordinates."), ("y", "Coordinates.")],
        );
    }

    #[test]
    fn parameter_documentation_rejects_partially_invalid_grouped_items() {
        assert_parameter_documentation(
            "\
Args:
    value: Initial documentation.
    value, for example: can be omitted.",
            &[(
                "value",
                "Initial documentation.\nvalue, for example: can be omitted.",
            )],
        );
    }

    #[test]
    fn parameter_documentation_prefers_last_duplicate() {
        assert_parameter_documentation(
            "\
Args:
    value: First documentation.
    value: Replacement documentation.",
            &[("value", "Replacement documentation.")],
        );
    }

    #[test]
    fn parameter_documentation_accepts_parentheses_in_quoted_types() {
        assert_parameter_documentation(
            r#"Args:
    value (Literal["("]): Parameter with a quoted parenthesis."#,
            &[("value", "Parameter with a quoted parenthesis.")],
        );
    }

    #[test]
    fn parameter_documentation_preserves_indentation_after_first_line_header() {
        for name in ["Warning", "Returns"] {
            let raw = format!(
                "Args:\n    {name}: Capitalized parameter.\n    following: Following parameter."
            );
            let parameters = super::super::parameter_documentation(&raw, IndexMap::default());

            assert_eq!(parameters.len(), 2, "{raw}");
            assert_eq!(parameters[name], "Capitalized parameter.", "{raw}");
            assert_eq!(parameters["following"], "Following parameter.", "{raw}");
        }
    }

    #[test]
    fn parameter_documentation_accepts_other_parameter_sections() {
        for heading in ["Other Args", "Other Arguments", "Other Parameters"] {
            let raw = format!("{heading}:\n    value: Parameter documentation.");
            assert_parameter_documentation(&raw, &[("value", "Parameter documentation.")]);
        }
    }

    #[test]
    fn parameter_documentation_consumes_recognized_section_bodies() {
        assert_parameter_documentation(
            "\
Args:
    value: Parameter documentation.
Methods:
    helper: Method documentation.",
            &[("value", "Parameter documentation.")],
        );

        assert_parameter_documentation(
            "\
Example:
    Args:
        nested: Not parameter documentation.
    Returns:
        str: Example result.
Args:
    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );

        assert_parameter_documentation(
            "\
Example:
Args:
    nested: Not parameter documentation.
Returns:
    str: Example result.",
            &[],
        );
    }

    #[test]
    fn parameter_documentation_ignores_sections_in_non_google_containers() {
        for raw in [
            "\
Examples
--------
Args:
    nested: Not parameter documentation.",
            "\
.. note::
    Args:
        nested: Not parameter documentation.",
            "\
- Example:
    Args:
        nested: Not parameter documentation.",
            "\
1. Example:
    Args:
        nested: Not parameter documentation.",
            "\
:param value: Example input.
    Args:
        nested: Not parameter documentation.",
        ] {
            assert_parameter_documentation(raw, &[]);
        }

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
    fn parameter_documentation_resumes_after_underlined_section() {
        assert_parameter_documentation(
            "\
Parameters
----------
numpy_value : int
    NumPy parameter documentation.

Args:
    google_value: Google parameter documentation.",
            &[("google_value", "Google parameter documentation.")],
        );

        assert_parameter_documentation(
            "\
Examples
--------
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
    fn parameter_documentation_stops_at_inline_section_summary() {
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
    fn parameter_documentation_ignores_sections_in_preformatted_blocks() {
        for raw in [
            "\
Summary.

    ```text
    Args:
        nested: Not parameter documentation.
    ```

    Args:
        value: Parameter documentation.",
            "\
Summary.

    >>> example()
    Args:
        nested: Not parameter documentation.

    Args:
        value: Parameter documentation.",
            "\
Summary.

    Example::

        Args:
            nested: Not parameter documentation.

    Args:
        value: Parameter documentation.",
        ] {
            let parameters = super::super::parameter_documentation(raw, IndexMap::default());
            assert_parameters(raw, &parameters, &[("value", "Parameter documentation.")]);
        }
    }

    #[test]
    fn parameter_documentation_ignores_visually_aligned_doctest_output() {
        assert_parameter_documentation(
            "        >>> example()\n\tArgs:\n\t    nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn parameter_documentation_recovers_after_tabbed_doctest_separator() {
        assert_parameter_documentation(
            "        >>> example()\n        result\n\t\n        Args:\n            value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn parameter_documentation_preserves_unicode_whitespace_doctest_output() {
        assert_parameter_documentation(
            "        >>> example()\n        \u{a0}\n        Args:\n            nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn parameter_documentation_stops_at_same_indent_inline_section_summary() {
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
    fn parameter_documentation_accepts_underlined_section() {
        assert_parameter_documentation(
            "\
Summary.

Args:
----
    value: Parameter documentation.

Returns:
    bool: Result.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn parameter_documentation_prefers_lowercase_same_indent_parameter() {
        assert_parameter_documentation(
            "\
Args:
error:
    Error documentation.
args: Args parameter documentation.
code: Code documentation.
returns: Return parameter documentation.
Returns:
bool: Result.",
            &[
                ("error", "Error documentation."),
                ("args", "Args parameter documentation."),
                ("code", "Code documentation."),
                ("returns", "Return parameter documentation."),
            ],
        );
    }

    #[test]
    fn section_visiting_preserves_underlined_lowercase_header() {
        let mut returns_body = None;
        visit_sections(
            "\
Args:
value: Parameter documentation.
returns:
--------
    bool: Result.",
            |kind, _, _, body| {
                if kind == SectionKind::Returns {
                    returns_body = Some(
                        body.iter()
                            .map(|line| line.text.to_string())
                            .collect::<Vec<_>>(),
                    );
                }
            },
        );

        assert_eq!(returns_body, Some(vec!["    bool: Result.".to_string()]));
    }

    #[test]
    fn parameter_documentation_stops_at_blank_separated_content() {
        assert_parameter_documentation(
            "\
Args:
value: Parameter documentation.

returns:
    bool: Result.",
            &[("value", "Parameter documentation.")],
        );

        assert_parameter_documentation(
            "\
Args:
value: Parameter documentation.

Additional details about the function.",
            &[("value", "Parameter documentation.")],
        );

        assert_parameter_documentation(
            "\
Args:
    value: Parameter documentation.

    Additional details about the function.",
            &[("value", "Parameter documentation.")],
        );
    }

    fn assert_parameter_documentation(raw: &str, expected: &[(&str, &str)]) {
        let parameters = parameter_documentation(raw);
        assert_parameters(raw, &parameters, expected);
    }

    fn assert_parameters(
        raw: &str,
        parameters: &IndexMap<String, String>,
        expected: &[(&str, &str)],
    ) {
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
