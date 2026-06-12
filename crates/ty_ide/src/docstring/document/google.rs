use indexmap::IndexMap;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::{TextRange, TextSize};

use super::SectionKind;
use super::preformatted::PreformattedBlockScanner;
use super::syntax::{
    ParsedLine, indented_container_end, parse_parenthesized_type, parsed_lines,
    split_once_unbracketed_colon,
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
    mut visit: impl FnMut(SectionKind, TextRange, TextSize, &[ParsedLine<'a>]),
) {
    let lines = parsed_lines(raw);
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut index = 0;

    while index < lines.len() {
        // Content in another block can resemble a top-level Google section.
        if preformatted_blocks.consume_preformatted_line(lines[index].text) {
            index += 1;
            continue;
        }
        if let Some(end) = indented_container_end(&lines, index) {
            index = end;
            continue;
        }

        let Some(header) = parse_section_header(&lines, index) else {
            preformatted_blocks.observe_line_outside_preformatted_block(lines[index].text);
            index += 1;
            continue;
        };
        let (body_end, range) = section_body_end(&lines, header);
        if let HeaderKind::Structured(kind) = header.kind {
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

/// Returns the index of the first line outside `header`'s body.
fn section_body_end(lines: &[ParsedLine<'_>], header: SectionHeader) -> (usize, TextRange) {
    let mut body_end = header.body_start;
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut item_indent = None;

    while let Some(line) = lines.get(body_end) {
        // Once a preformatted block begins, its contents cannot end the section.
        if preformatted_blocks.is_active()
            && preformatted_blocks.consume_preformatted_line(line.text)
        {
            body_end += 1;
            continue;
        }

        if line.text.trim().is_empty() {
            if !blank_line_continues_section(&lines[body_end..], header, item_indent) {
                break;
            }
            while let Some(line) = lines.get(body_end)
                && line.text.trim().is_empty()
            {
                body_end += 1;
            }
            continue;
        }

        if section_header_ends_body(lines, body_end, header, item_indent)
            || !line_belongs_to_body(header, *line, item_indent)
        {
            break;
        }

        item_indent = item_indent.or_else(|| section_item_indent(header, *line));

        if !preformatted_blocks.consume_preformatted_line(line.text) {
            preformatted_blocks.observe_line_outside_preformatted_block(line.text);
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

/// Returns whether content after leading blank lines still belongs to `header`.
fn blank_line_continues_section(
    lines: &[ParsedLine<'_>],
    header: SectionHeader,
    item_indent: Option<TextSize>,
) -> bool {
    let Some((offset, next)) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| !line.text.trim().is_empty())
    else {
        return false;
    };

    if next.structural_indent <= header.structural_indent {
        if parse_section_header(lines, offset).is_some() || is_inline_section_header(next.text) {
            return false;
        }
        // Returns and yields have no item syntax that distinguishes an aligned body from prose
        // following an empty section.
        if next.raw_indent <= header.indent
            && item_indent.is_none()
            && matches!(
                header.kind,
                HeaderKind::Structured(SectionKind::Returns | SectionKind::Yields)
            )
        {
            return false;
        }
    }

    // A blank line separates prose aligned with parameter items from the section body.
    if item_indent == Some(next.raw_indent)
        && matches!(
            header.kind,
            HeaderKind::Structured(
                SectionKind::Parameters
                    | SectionKind::KeywordArguments
                    | SectionKind::OtherParameters
            )
        )
        && section_item_indent(header, *next).is_none()
    {
        return false;
    }

    line_belongs_to_body(header, *next, item_indent)
}

/// Returns whether a recognized header at `index` ends the current section body.
fn section_header_ends_body(
    lines: &[ParsedLine<'_>],
    index: usize,
    header: SectionHeader,
    item_indent: Option<TextSize>,
) -> bool {
    let Some(line) = lines.get(index) else {
        return false;
    };
    let prefer_item = prefer_item_over_section_header(header, *line, item_indent);
    if line.structural_indent <= header.structural_indent && is_inline_section_header(line.text) {
        return !prefer_item;
    }

    parse_section_header(lines, index)
        .is_some_and(|next| next.structural_indent <= header.structural_indent && !prefer_item)
}

/// Returns whether `line` belongs to `header` under Google-style indentation rules.
fn line_belongs_to_body(
    header: SectionHeader,
    line: ParsedLine<'_>,
    item_indent: Option<TextSize>,
) -> bool {
    let item_indent_matches_line = item_indent.is_none_or(|indent| indent == line.raw_indent);
    let is_same_indent_parameter_continuation = item_indent == Some(line.raw_indent)
        && matches!(
            header.kind,
            HeaderKind::Structured(
                SectionKind::Parameters
                    | SectionKind::KeywordArguments
                    | SectionKind::OtherParameters
            )
        );

    line.raw_indent > header.indent
        || (line.raw_indent == header.indent
            && (is_same_indent_parameter_continuation
                || (item_indent_matches_line && section_item_indent(header, line).is_some())))
}

/// Returns whether an ambiguous section header is an item in the current section.
fn prefer_item_over_section_header(
    header: SectionHeader,
    line: ParsedLine<'_>,
    item_indent: Option<TextSize>,
) -> bool {
    let has_named_items = matches!(
        header.kind,
        HeaderKind::Structured(
            SectionKind::Parameters
                | SectionKind::KeywordArguments
                | SectionKind::OtherParameters
                | SectionKind::Attributes
                | SectionKind::Raises
        )
    );
    item_indent.is_none_or(|indent| indent == line.raw_indent)
        && section_item_indent(header, line).is_some()
        && ((has_named_items && line.raw_indent > header.indent)
            || line
                .text
                .trim()
                .chars()
                .next()
                .is_some_and(char::is_lowercase)
            || (matches!(header.kind, HeaderKind::Structured(SectionKind::Raises))
                && split_once_unbracketed_colon(line.text.trim())
                    .is_some_and(|(name, _)| has_exception_name_suffix(name.trim()))))
}

/// Returns the indentation of an item recognized in the current section.
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
        HeaderKind::Container => false,
    };
    is_item.then_some(line.raw_indent)
}

/// Parses a recognized Google-style section header at `index`.
fn parse_section_header(lines: &[ParsedLine<'_>], index: usize) -> Option<SectionHeader> {
    let line = lines.get(index)?;
    let kind = section_kind(line.text)?;

    Some(SectionHeader {
        kind,
        indent: line.raw_indent,
        structural_indent: line.structural_indent,
        body_start: index + 1,
        range: line.range,
    })
}

fn section_kind(line: &str) -> Option<HeaderKind> {
    let name = line.trim().strip_suffix(':')?.trim();
    section_kind_from_name(name)
}

fn section_kind_from_name(name: &str) -> Option<HeaderKind> {
    let normalized = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    Some(match normalized.as_str() {
        "args" | "arguments" | "parameters" => HeaderKind::Structured(SectionKind::Parameters),
        "keyword args" | "keyword arguments" => {
            HeaderKind::Structured(SectionKind::KeywordArguments)
        }
        "other args" | "other arguments" | "other parameters" => {
            HeaderKind::Structured(SectionKind::OtherParameters)
        }
        "attributes" => HeaderKind::Structured(SectionKind::Attributes),
        "return" | "returns" => HeaderKind::Structured(SectionKind::Returns),
        "yield" | "yields" => HeaderKind::Structured(SectionKind::Yields),
        "raise" | "raises" => HeaderKind::Structured(SectionKind::Raises),
        "attention" | "caution" | "danger" | "error" | "example" | "examples" | "hint"
        | "important" | "methods" | "note" | "notes" | "references" | "see also" | "tip"
        | "todo" | "todos" | "warning" | "warnings" | "warns" => HeaderKind::Container,
        _ => return None,
    })
}

/// Returns whether `line` is a recognized section header followed by inline content.
fn is_inline_section_header(line: &str) -> bool {
    let Some((name, description)) = split_once_unbracketed_colon(line.trim()) else {
        return false;
    };
    let name = name.trim();
    let description = description.trim();
    !description.is_empty()
        && !description.starts_with(':')
        && name.chars().next().is_some_and(char::is_uppercase)
        && section_kind_from_name(name).is_some()
}

/// Returns whether `line` is a recognized Google-style section header.
pub(in crate::docstring) fn is_section_like_header(line: &str) -> bool {
    section_kind(line).is_some() || is_inline_section_header(line)
}

/// Returns whether `name` ends with a conventional exception-class suffix.
pub(in crate::docstring) fn has_exception_name_suffix(name: &str) -> bool {
    ["Error", "Exception", "Warning"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

/// Parses a parameter item into its display name and description.
fn parse_parameter(line: &str) -> Option<(&str, &str)> {
    let (name, description) = split_once_unbracketed_colon(line)?;
    let (display_name, _) = parse_parenthesized_type(name.trim());
    google_parameter_names(display_name)
        .all(is_parameter_name)
        .then_some((display_name, description.trim()))
}

/// Extends `parameters` with the documented items in one parameter section body.
fn extend_parameter_documentation(
    parameters: &mut IndexMap<String, String>,
    lines: &[ParsedLine<'_>],
) {
    let mut current: Option<(String, String)> = None;
    let mut item_indent = None;

    // The first item fixes the indentation. Colons at other levels remain continuation prose.
    for line in lines {
        let trimmed = line.text.trim();
        if trimmed.is_empty() {
            if let Some((_, description)) = &mut current {
                if !description.is_empty() && !description.ends_with('\n') {
                    description.push('\n');
                }
                description.push('\n');
            }
        } else if item_indent.is_none_or(|indent| line.raw_indent == indent)
            && let Some((names, description)) = parse_parameter(trimmed)
        {
            insert_parameter_documentation(
                parameters,
                current.replace((names.to_string(), description.to_string())),
            );
            item_indent.get_or_insert(line.raw_indent);
        } else if let Some((_, description)) = &mut current {
            if !description.is_empty() && !description.ends_with('\n') {
                description.push('\n');
            }
            description.push_str(trimmed);
        }
    }

    insert_parameter_documentation(parameters, current);
}

/// Inserts a completed parameter item under each of its comma-separated names.
fn insert_parameter_documentation(
    parameters: &mut IndexMap<String, String>,
    parameter: Option<(String, String)>,
) {
    let Some((names, description)) = parameter else {
        return;
    };
    let description = description.trim();
    if !description.is_empty() {
        for name in google_parameter_names(&names) {
            parameters.insert(name.to_string(), description.to_string());
        }
    }
}

fn google_parameter_names(display_name: &str) -> impl Iterator<Item = &str> {
    display_name.split(',').map(str::trim)
}

/// Returns whether every component of `name` is a Python identifier.
pub(in crate::docstring) fn is_dotted_identifier(name: &str) -> bool {
    !name.is_empty() && name.split('.').all(is_identifier)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SectionHeader {
    kind: HeaderKind,
    indent: TextSize,
    structural_indent: TextSize,
    body_start: usize,
    range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderKind {
    Structured(SectionKind),
    Container,
}

#[cfg(test)]
mod tests {
    use ruff_text_size::TextSize;

    use super::{SectionKind, parameter_documentation, visit_sections};

    #[test]
    fn extracts_parameter_items() {
        for (raw, expected) in [
            (
                "Arguments:\nfirst: First parameter.\nAligned continuation.\nsecond: Second parameter.\nReturns:\nbool: Result.",
                &[
                    ("first", "First parameter.\nAligned continuation."),
                    ("second", "Second parameter."),
                ][..],
            ),
            (
                "Args:\n  \tfirst: First parameter.\n        second: Second parameter.",
                &[
                    ("first", "First parameter."),
                    ("second", "Second parameter."),
                ],
            ),
            (
                "Args:\n    x, y: Coordinates.",
                &[("x", "Coordinates."), ("y", "Coordinates.")],
            ),
            (
                "Args:\n    value: Initial documentation.\n    value, for example: can be omitted.",
                &[(
                    "value",
                    "Initial documentation.\nvalue, for example: can be omitted.",
                )],
            ),
            (
                "Args:\n    value: First documentation.\n    value: Replacement documentation.",
                &[("value", "Replacement documentation.")],
            ),
            (
                "Args:\n    value (Literal[\"(\"]): Quoted parenthesis.",
                &[("value", "Quoted parenthesis.")],
            ),
            (
                "Args:\n    callback() (Callable): Not a parameter.\n    value: Documentation.",
                &[("value", "Documentation.")],
            ),
            (
                "Args:\n    value: First paragraph.\n\n\n        Second paragraph.",
                &[("value", "First paragraph.\n\n\nSecond paragraph.")],
            ),
        ] {
            assert_parameter_documentation(raw, expected);
        }
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
            let raw = format!("{heading}:\n    value: Parameter documentation.");
            assert_parameter_documentation(&raw, &[("value", "Parameter documentation.")]);
        }
    }

    #[test]
    fn respects_section_boundaries() {
        for (raw, expected) in [
            (
                "Args:\n    value: Parameter documentation.\nMethods:\n    helper: Method documentation.",
                &[("value", "Parameter documentation.")][..],
            ),
            (
                "Example:\n    Args:\n        nested: Not parameter documentation.\nArgs:\n    value: Parameter documentation.",
                &[("value", "Parameter documentation.")],
            ),
            (
                "Args:\n    first: First parameter.\n    last: Last parameter.\n\nReturns: Result.",
                &[("first", "First parameter."), ("last", "Last parameter.")],
            ),
            (
                "Args:\nerror:\n    Error documentation.\nargs: Args documentation.\nreturns: Return documentation.\nReturns:\nbool: Result.",
                &[
                    ("error", "Error documentation."),
                    ("args", "Args documentation."),
                    ("returns", "Return documentation."),
                ],
            ),
            (
                "Args:\nvalue: Parameter documentation.\n\nAdditional details.",
                &[("value", "Parameter documentation.")],
            ),
            (
                "Args:\n    Warning: Capitalized parameter.\n    following: Following parameter.",
                &[
                    ("Warning", "Capitalized parameter."),
                    ("following", "Following parameter."),
                ],
            ),
            (
                "Args:\nfirst: First parameter.\nlast: Last parameter.\nReturns: Result.",
                &[("first", "First parameter."), ("last", "Last parameter.")],
            ),
            (
                "Args:\n    value: Parameter documentation.\n\n    Additional details.",
                &[("value", "Parameter documentation.")],
            ),
        ] {
            assert_parameter_documentation(raw, expected);
        }
    }

    #[test]
    fn uses_pep257_indentation_for_section_hierarchy() {
        assert_parameter_documentation(
            "Note:\n        context\n\n    Args:\n        value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
        assert_parameter_documentation(
            "\n    Note:\n        context\n\n        Args:\n            nested: Not parameter documentation.",
            &[],
        );
        assert_parameter_documentation(
            "Example:\nArgs:\n    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
        assert_parameter_documentation(
            "Args:\n        value: Parameter documentation.\n    Returns:\n        bool: Result.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn finds_shifted_top_level_section() {
        assert_parameter_documentation(
            "A decoded newline follows:\nThis line starts at column zero.\n\n    Keyword Args:\n        shifted: Documentation in a shifted section.",
            &[("shifted", "Documentation in a shifted section.")],
        );
    }

    #[test]
    fn keeps_colon_prose_with_variadic_parameters() {
        assert_parameter_documentation(
            "Args:\n    param1 (str): The first parameter description.\n    For example: pass an absolute path.\n    *args: Extra positional arguments.\n    **kwargs: Extra keyword arguments.",
            &[
                (
                    "param1",
                    "The first parameter description.\nFor example: pass an absolute path.",
                ),
                ("*args", "Extra positional arguments."),
                ("**kwargs", "Extra keyword arguments."),
            ],
        );
    }

    #[test]
    fn ignores_sections_in_other_containers() {
        for raw in [
            ".. note::\n    Args:\n        nested: Not parameter documentation.",
            ".. note::\n\n        Keyword Args:\n            nested: Not parameter documentation.",
            "- Example:\n    Args:\n        nested: Not parameter documentation.",
            "- Example:\n\n        Args:\n            nested: Not parameter documentation.",
            "1. Example:\n    Args:\n        nested: Not parameter documentation.",
            ":param value: Example input.\n    Args:\n        nested: Not parameter documentation.",
            "Example::\n\n        Args:\n            nested: Not parameter documentation.",
        ] {
            assert_parameter_documentation(raw, &[]);
        }

        for raw in [
            "Summary.\n\n    ```text\n    Args:\n        nested: Not parameter documentation.\n    ```\n\n    Args:\n        value: Parameter documentation.",
            "Summary.\n\n    Example::\n\n        Args:\n            nested: Not parameter documentation.\n\n    Args:\n        value: Parameter documentation.",
        ] {
            assert_parameter_documentation(raw, &[("value", "Parameter documentation.")]);
        }

        assert_parameter_documentation(
            ".. note::\n    Args:\n        nested: Not parameter documentation.\nArgs:\n    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ignores_doctest_content_and_resumes_after_it() {
        for raw in [
            "        >>> example()\n\tArgs:\n\t    nested: Not parameter documentation.",
            "        >>> example()\n        \u{a0}\n        Args:\n            nested: Not parameter documentation.",
        ] {
            assert_parameter_documentation(raw, &[]);
        }
        assert_parameter_documentation(
            "        >>> example()\n        result\n\t\n        Args:\n            value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn double_colon_is_not_an_inline_section_header() {
        assert_parameter_documentation(
            "Args:\nvalue: Parameter documentation.\nReturns:: reST literal marker.",
            &[
                ("value", "Parameter documentation."),
                ("Returns", ": reST literal marker."),
            ],
        );
    }

    #[test]
    fn visits_structured_sections_with_final_metadata() {
        let raw = "    Args:\r\n        value: Documentation.\r\nKeyword Args:\r\n    option: Optional.\r\nOther Parameters:\r\n    other: Other.\r\nReturns:\r\n    bool: Result.";
        let mut sections = Vec::new();
        visit_sections(raw, |kind, range, header_indent, body| {
            sections.push((
                kind,
                &raw[range],
                header_indent,
                body.iter().map(|line| line.text).collect::<Vec<_>>(),
            ));
        });

        assert_eq!(
            sections,
            vec![
                (
                    SectionKind::Parameters,
                    "    Args:\r\n        value: Documentation.",
                    TextSize::new(4),
                    vec!["        value: Documentation."],
                ),
                (
                    SectionKind::KeywordArguments,
                    "Keyword Args:\r\n    option: Optional.",
                    TextSize::new(0),
                    vec!["    option: Optional."],
                ),
                (
                    SectionKind::OtherParameters,
                    "Other Parameters:\r\n    other: Other.",
                    TextSize::new(0),
                    vec!["    other: Other."],
                ),
                (
                    SectionKind::Returns,
                    "Returns:\r\n    bool: Result.",
                    TextSize::new(0),
                    vec!["    bool: Result."],
                ),
            ]
        );
    }

    fn assert_parameter_documentation(raw: &str, expected: &[(&str, &str)]) {
        let parameters = parameter_documentation(raw);
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
