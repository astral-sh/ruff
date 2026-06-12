use indexmap::IndexMap;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_source_file::UniversalNewlines;
use ruff_text_size::TextSize;

use super::indentation;
use super::preformatted::PreformattedBlockScanner;
use super::rst::is_field_list_marker;

/// Returns parameter documentation from recognized Google-style parameter sections.
pub(super) fn parameter_documentation(raw: &str) -> IndexMap<String, String> {
    let mut parameters = IndexMap::new();
    visit_parameter_sections(raw, |body| {
        extend_parameter_documentation(&mut parameters, body);
    });
    parameters
}

/// Visits Google-style parameter sections in source order.
fn visit_parameter_sections<'a>(raw: &'a str, mut visit: impl FnMut(&[ParsedLine<'a>])) {
    let lines = parsed_lines(raw);
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut index = 0;

    while index < lines.len() {
        // Content in another block can resemble a top-level Google section.
        if preformatted_blocks.consume_preformatted_line(lines[index].text) {
            index += 1;
            continue;
        }
        if let Some(end) = indented_non_google_block_end(&lines, index) {
            index = end;
            continue;
        }

        let Some(header) = parse_section_header(&lines, index) else {
            preformatted_blocks.observe_line_outside_preformatted_block(lines[index].text);
            index += 1;
            continue;
        };
        let body_end = section_body_end(&lines, header);
        if header.kind == HeaderKind::Parameters {
            visit(&lines[header.body_start..body_end]);
        }
        index = body_end;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedLine<'a> {
    text: &'a str,
    raw_indent: TextSize,
    structural_indent: TextSize,
}

fn parsed_lines(raw: &str) -> Vec<ParsedLine<'_>> {
    let mut lines = raw
        .universal_newlines()
        .map(|line| ParsedLine {
            text: line.as_str(),
            raw_indent: indentation(line.as_str()),
            structural_indent: TextSize::new(0),
        })
        .collect::<Vec<_>>();

    // PEP 257 ignores indentation on the first physical line and removes the common margin from
    // all later lines. Keep the raw indentation as well because item and block indentation can
    // still disambiguate content within a section.
    let continuation_margin = lines
        .iter()
        .skip(1)
        .filter(|line| !line.text.trim().is_empty())
        .map(|line| line.raw_indent)
        .min()
        .unwrap_or(TextSize::new(0));
    for line in lines.iter_mut().skip(1) {
        line.structural_indent = line.raw_indent.saturating_sub(continuation_margin);
    }

    lines
}

/// Returns the index after an indented reST or Markdown container at `index`.
fn indented_non_google_block_end(lines: &[ParsedLine<'_>], index: usize) -> Option<usize> {
    let marker = lines.get(index)?;
    if !is_rest_directive_marker(marker.text)
        && !is_field_list_marker(marker.text)
        && !starts_with_markdown_list_item(marker.text.trim_start())
    {
        return None;
    }

    // Raw indentation identifies nested content. A blank-separated sibling can still carry the
    // source margin when the marker is on the docstring's first line, so compare recognized
    // sections using their PEP 257 indentation as well.
    Some(
        (index + 1..lines.len())
            .find(|&end| {
                let line = lines[end];
                !line.text.trim().is_empty()
                    && (line.raw_indent <= marker.raw_indent
                        || (lines[end - 1].text.trim().is_empty()
                            && line.structural_indent <= marker.structural_indent
                            && (parse_section_header(lines, end).is_some()
                                || is_inline_section_header(line.text))))
            })
            .unwrap_or(lines.len()),
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

fn starts_with_markdown_list_item(line: &str) -> bool {
    let bytes = line.as_bytes();
    if matches!(bytes, [b'-' | b'+' | b'*', b' ' | b'\t', ..]) {
        return true;
    }

    let digits = bytes
        .iter()
        .take(9)
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    digits > 0
        && matches!(bytes.get(digits), Some(b'.' | b')'))
        && matches!(bytes.get(digits + 1), Some(b' ' | b'\t'))
}

/// Returns the index of the first line outside `header`'s body.
fn section_body_end(lines: &[ParsedLine<'_>], header: SectionHeader) -> usize {
    let mut body_end = header.body_start;
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut parameter_item_indent = None;

    while let Some(line) = lines.get(body_end) {
        // Once a preformatted block begins, its contents cannot end the section.
        if preformatted_blocks.is_active()
            && preformatted_blocks.consume_preformatted_line(line.text)
        {
            body_end += 1;
            continue;
        }

        if line.text.trim().is_empty() {
            if !blank_line_continues_section(&lines[body_end..], header, parameter_item_indent) {
                break;
            }
            while let Some(line) = lines.get(body_end)
                && line.text.trim().is_empty()
            {
                body_end += 1;
            }
            continue;
        }

        if section_header_ends_body(lines, body_end, header, parameter_item_indent)
            || !line_belongs_to_body(header, *line, parameter_item_indent)
        {
            break;
        }

        parameter_item_indent =
            parameter_item_indent.or_else(|| parameter_item_indent_for_line(header, *line));

        if !preformatted_blocks.consume_preformatted_line(line.text) {
            preformatted_blocks.observe_line_outside_preformatted_block(line.text);
        }
        body_end += 1;
    }

    body_end
}

/// Returns whether content after leading blank lines still belongs to `header`.
fn blank_line_continues_section(
    lines: &[ParsedLine<'_>],
    header: SectionHeader,
    parameter_item_indent: Option<TextSize>,
) -> bool {
    let Some((offset, next)) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| !line.text.trim().is_empty())
    else {
        return false;
    };

    if next.structural_indent <= header.structural_indent
        && (parse_section_header(lines, offset).is_some() || is_inline_section_header(next.text))
    {
        return false;
    }
    // A blank line separates prose aligned with the parameter items from the section body.
    if parameter_item_indent == Some(next.raw_indent)
        && parameter_item_indent_for_line(header, *next).is_none()
    {
        return false;
    }

    line_belongs_to_body(header, *next, parameter_item_indent)
}

/// Returns whether a recognized header at `index` ends the current section body.
fn section_header_ends_body(
    lines: &[ParsedLine<'_>],
    index: usize,
    header: SectionHeader,
    parameter_item_indent: Option<TextSize>,
) -> bool {
    let Some(line) = lines.get(index) else {
        return false;
    };
    if line.structural_indent <= header.structural_indent
        && is_inline_section_header(line.text)
        && !parameter_item_takes_precedence(header, *line, parameter_item_indent)
    {
        return true;
    }

    parse_section_header(lines, index).is_some_and(|next| {
        next.structural_indent <= header.structural_indent
            && !parameter_item_takes_precedence(header, *line, parameter_item_indent)
    })
}

/// Returns whether `line` belongs to `header` under Google-style indentation rules.
fn line_belongs_to_body(
    header: SectionHeader,
    line: ParsedLine<'_>,
    parameter_item_indent: Option<TextSize>,
) -> bool {
    line.raw_indent > header.raw_indent
        || (line.raw_indent == header.raw_indent
            && parameter_item_indent.is_none_or(|indent| indent == line.raw_indent)
            && (parameter_item_indent.is_some()
                || parameter_item_indent_for_line(header, line).is_some()))
}

/// Returns whether a parameter item is more specific than a possible section header.
fn parameter_item_takes_precedence(
    header: SectionHeader,
    line: ParsedLine<'_>,
    parameter_item_indent: Option<TextSize>,
) -> bool {
    parameter_item_indent.is_none_or(|indent| indent == line.raw_indent)
        && parameter_item_indent_for_line(header, line).is_some()
        && (line.raw_indent > header.raw_indent
            || line
                .text
                .trim()
                .chars()
                .next()
                .is_some_and(char::is_lowercase))
}

fn parameter_item_indent_for_line(header: SectionHeader, line: ParsedLine<'_>) -> Option<TextSize> {
    (header.kind == HeaderKind::Parameters && parse_parameter(line.text.trim()).is_some())
        .then_some(line.raw_indent)
}

/// Parses a recognized Google-style section header at `index`.
fn parse_section_header(lines: &[ParsedLine<'_>], index: usize) -> Option<SectionHeader> {
    let line = lines.get(index)?;
    let kind = section_kind(line.text)?;

    Some(SectionHeader {
        kind,
        raw_indent: line.raw_indent,
        structural_indent: line.structural_indent,
        body_start: index + 1,
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
        "args" | "arguments" | "parameters" | "keyword args" | "keyword arguments"
        | "other args" | "other arguments" | "other parameters" => HeaderKind::Parameters,
        "attributes" | "return" | "returns" | "yield" | "yields" | "raise" | "raises"
        | "attention" | "caution" | "danger" | "error" | "example" | "examples" | "hint"
        | "important" | "methods" | "note" | "notes" | "references" | "see also" | "tip"
        | "todo" | "todos" | "warning" | "warnings" | "warns" => HeaderKind::Other,
        _ => return None,
    })
}

/// Returns whether `line` is a recognized section header followed by inline content.
fn is_inline_section_header(line: &str) -> bool {
    let Some((name, description)) = split_once_unbracketed_colon(line.trim()) else {
        return false;
    };
    let name = name.trim();
    !description.trim().is_empty()
        && name.chars().next().is_some_and(char::is_uppercase)
        && section_kind_from_name(name).is_some()
}

/// Parses a parameter item into its display name and description.
fn parse_parameter(line: &str) -> Option<(&str, &str)> {
    let (name, description) = split_once_unbracketed_colon(line)?;
    let (display_name, _) = parse_parenthesized_type(name.trim());
    google_parameter_names(display_name)
        .all(is_parameter_name)
        .then_some((display_name, description.trim()))
}

fn is_parameter_name(name: &str) -> bool {
    let identifier = name
        .strip_prefix("**")
        .or_else(|| name.strip_prefix('*'))
        .unwrap_or(name);
    is_identifier(identifier)
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

/// Splits at the first colon outside bracket pairs and quoted strings.
fn split_once_unbracketed_colon(line: &str) -> Option<(&str, &str)> {
    let mut depths = [0usize; 3];
    let mut quote = None;
    let mut escaped = false;

    for (index, character) in line.char_indices() {
        if let Some(quote_character) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote_character {
                quote = None;
            }
            continue;
        }

        match character {
            '\'' | '"' => quote = Some(character),
            '(' => depths[0] += 1,
            ')' => depths[0] = depths[0].saturating_sub(1),
            '[' => depths[1] += 1,
            ']' => depths[1] = depths[1].saturating_sub(1),
            '{' => depths[2] += 1,
            '}' => depths[2] = depths[2].saturating_sub(1),
            ':' if depths == [0; 3] => {
                return Some((&line[..index], &line[index + character.len_utf8()..]));
            }
            _ => {}
        }
    }
    None
}

/// Splits a trailing parenthesized type from a parameter display name.
fn parse_parenthesized_type(name: &str) -> (&str, Option<&str>) {
    if !name.ends_with(')') {
        return (name, None);
    }

    let mut depth = 0usize;
    let mut opening = None;
    let mut quote = None;
    let mut escaped = false;

    // Only a balanced group that closes at the end can be a type suffix.
    for (index, character) in name.char_indices() {
        if let Some(quote_character) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote_character {
                quote = None;
            }
            continue;
        }

        match character {
            '\'' | '"' => quote = Some(character),
            '(' => {
                if depth == 0 {
                    opening = Some(index);
                }
                depth += 1;
            }
            ')' => {
                depth = match depth.checked_sub(1) {
                    Some(depth) => depth,
                    None => return (name, None),
                };
                if depth == 0 && index + character.len_utf8() == name.len() {
                    let Some(opening) = opening else {
                        return (name, None);
                    };
                    let display_name = name[..opening].trim();
                    let ty = name[opening + '('.len_utf8()..index].trim();
                    return if display_name.is_empty() || ty.is_empty() {
                        (name, None)
                    } else {
                        (display_name, Some(ty))
                    };
                }
            }
            _ => {}
        }
    }
    (name, None)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SectionHeader {
    kind: HeaderKind,
    raw_indent: TextSize,
    structural_indent: TextSize,
    body_start: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderKind {
    Parameters,
    Other,
}

#[cfg(test)]
mod tests {
    use super::parameter_documentation;

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
        assert_parameter_documentation(
            "Examples\n    --------\n        context\n\n    Args:\n        value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
        assert_parameter_documentation(
            ".. note::\n        context\n\n    Args:\n        value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ignores_sections_in_other_containers() {
        for raw in [
            ".. note::\n    Args:\n        nested: Not parameter documentation.",
            "- Example:\n    Args:\n        nested: Not parameter documentation.",
            "1. Example:\n    Args:\n        nested: Not parameter documentation.",
            ":param value: Example input.\n    Args:\n        nested: Not parameter documentation.",
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
