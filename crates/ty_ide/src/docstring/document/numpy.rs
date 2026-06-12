use std::borrow::Cow;

use indexmap::IndexMap;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_text_size::TextRange;

use super::SectionKind;
use super::google;
use super::preformatted::PreformattedBlockScanner;
use super::syntax::{
    ParsedLine, indentation, indented_container_end, is_docstring_type_expression, parsed_lines,
    split_once_unbracketed_colon,
};

pub(super) fn parameter_documentation(raw: &str) -> IndexMap<String, String> {
    let mut parameters = IndexMap::new();

    visit_sections(raw, |kind, _, body| {
        if matches!(kind, SectionKind::Parameters | SectionKind::OtherParameters) {
            extend_parameter_documentation(&mut parameters, body);
        }
    });

    parameters
}

/// Visits recognized top-level NumPy-style sections in source order.
pub(in crate::docstring) fn visit_sections<'a>(
    raw: &'a str,
    mut visit: impl FnMut(SectionKind, TextRange, &[ParsedLine<'a>]),
) {
    let lines = parsed_lines(raw);
    let top_level_indent = effective_top_level_indent(&lines);
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut index = 0;

    while index < lines.len() {
        if preformatted_blocks.consume_preformatted_line(lines[index].text) {
            index += 1;
            continue;
        }
        if let Some(container_end) = nested_container_end(&lines, index) {
            index = container_end;
            continue;
        }

        let Some(header) = parse_section_header(&lines, index) else {
            if let Some(section_end) = google::non_google_underlined_section_end(&lines, index) {
                index = section_end;
                continue;
            }
            preformatted_blocks.observe_line_outside_preformatted_block(lines[index].text);
            index += 1;
            continue;
        };
        if header.indent != top_level_indent {
            index += 1;
            continue;
        }

        let (body_end, range) = section_body_end(&lines, header);
        visit(header.kind, range, &lines[header.body_start..body_end]);
        index = body_end;
    }
}

fn effective_top_level_indent(lines: &[ParsedLine<'_>]) -> usize {
    // PEP 257 ignores the first line's indentation, so a lone column-zero first line cannot
    // distinguish a nested block from a shifted top-level section. A later column-zero logical
    // line can prevent physically top-level lines after an escaped newline from being dedented.
    if !lines
        .iter()
        .skip(1)
        .any(|line| !line.text.trim().is_empty() && indentation(line.text) == 0)
    {
        return 0;
    }

    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut top_level_indent = None;
    let mut index = 0;

    while index < lines.len() {
        if preformatted_blocks.consume_preformatted_line(lines[index].text) {
            index += 1;
            continue;
        }
        if let Some(container_end) = nested_container_end(lines, index) {
            index = container_end;
            continue;
        }

        if let Some(header) = parse_section_header(lines, index) {
            top_level_indent = Some(
                top_level_indent.map_or(header.indent, |indent: usize| indent.min(header.indent)),
            );
            index += 2;
            continue;
        }
        if let Some(section_end) = google::non_google_underlined_section_end(lines, index) {
            index = section_end;
            continue;
        }

        preformatted_blocks.observe_line_outside_preformatted_block(lines[index].text);
        index += 1;
    }

    top_level_indent.unwrap_or_default()
}

fn nested_container_end(lines: &[ParsedLine<'_>], index: usize) -> Option<usize> {
    indented_container_end(lines, index).or_else(|| google::section_like_end(lines, index))
}

fn section_body_end(lines: &[ParsedLine<'_>], header: NumpySectionHeader) -> (usize, TextRange) {
    let mut body_end = header.body_start;
    let mut range = header.range;
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let first_item = first_body_item_index(lines, header);

    while let Some(line) = lines.get(body_end) {
        if preformatted_blocks.is_active()
            && preformatted_blocks.consume_preformatted_line(line.text)
        {
            range = TextRange::new(range.start(), line.range.end());
            body_end += 1;
            continue;
        }

        if first_item.is_some_and(|first_item| body_end < first_item) {
            if !preformatted_blocks.consume_preformatted_line(line.text) {
                preformatted_blocks.observe_line_outside_preformatted_block(line.text);
            }
            range = TextRange::new(range.start(), line.range.end());
            body_end += 1;
            continue;
        }

        if line.text.trim().is_empty() {
            if !blank_line_continues_section(&lines[body_end..], header) {
                break;
            }

            while let Some(blank_line) = lines.get(body_end)
                && blank_line.text.trim().is_empty()
            {
                range = TextRange::new(range.start(), blank_line.range.end());
                body_end += 1;
            }
            continue;
        }

        if underlined_section_indent(lines, body_end).is_some_and(|indent| indent <= header.indent)
        {
            break;
        }

        if !line.text.trim().is_empty()
            && !line_belongs_to_body(header, line, &lines[body_end + 1..])
        {
            break;
        }

        if !preformatted_blocks.consume_preformatted_line(line.text) {
            preformatted_blocks.observe_line_outside_preformatted_block(line.text);
        }
        range = TextRange::new(range.start(), line.range.end());
        body_end += 1;
    }

    (body_end, range)
}

fn first_body_item_index(lines: &[ParsedLine<'_>], header: NumpySectionHeader) -> Option<usize> {
    if !matches!(
        header.kind,
        SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters
    ) {
        return Some(header.body_start);
    }

    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut index = header.body_start;
    while let Some(line) = lines.get(index) {
        if preformatted_blocks.consume_preformatted_line(line.text) {
            index += 1;
            continue;
        }

        if underlined_section_indent(lines, index).is_some_and(|indent| indent <= header.indent) {
            return None;
        }

        if !line.text.trim().is_empty() {
            let line_indent = indentation(line.text);
            if line_indent < header.indent {
                return None;
            }
            if line_indent == header.indent {
                if parameter_item_starts(line, &lines[index + 1..]) {
                    return Some(index);
                }
                if google::is_section_like_header(line.text) {
                    return None;
                }
            }
        }

        preformatted_blocks.observe_line_outside_preformatted_block(line.text);
        index += 1;
    }

    None
}

fn blank_line_continues_section(lines: &[ParsedLine<'_>], header: NumpySectionHeader) -> bool {
    let Some((offset, non_blank_line)) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| !line.text.trim().is_empty())
    else {
        return false;
    };

    if underlined_section_indent(lines, offset).is_some_and(|indent| indent <= header.indent) {
        return false;
    }

    line_belongs_to_body(header, non_blank_line, &lines[offset + 1..])
}

fn line_belongs_to_body(
    header: NumpySectionHeader,
    line: &ParsedLine<'_>,
    following_lines: &[ParsedLine<'_>],
) -> bool {
    let line_indent = indentation(line.text);
    if line_indent > header.indent {
        return true;
    }
    if line_indent != header.indent {
        return false;
    }
    if !matches!(
        header.kind,
        SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters
    ) && google::is_section_like_header(line.text)
    {
        return false;
    }

    match header.kind {
        SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters => {
            parameter_item_starts(line, following_lines)
        }
        SectionKind::Attributes => named_item_starts(line, following_lines),
        SectionKind::Returns | SectionKind::Yields => return_item_starts(line, following_lines),
        SectionKind::Raises => raise_item_starts(line),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NumpySectionHeader {
    kind: SectionKind,
    indent: usize,
    body_start: usize,
    range: TextRange,
}

fn parse_section_header(lines: &[ParsedLine<'_>], index: usize) -> Option<NumpySectionHeader> {
    let line = lines.get(index)?;
    let underline = lines.get(index + 1)?;
    let indent = underlined_section_indent(lines, index)?;

    Some(NumpySectionHeader {
        kind: section_kind(line.text)?,
        indent,
        body_start: index + 2,
        range: TextRange::new(line.range.start(), underline.range.end()),
    })
}

fn underlined_section_indent(lines: &[ParsedLine<'_>], index: usize) -> Option<usize> {
    let line = lines.get(index)?;
    let underline = lines.get(index + 1)?;
    let indent = indentation(line.text);

    (!line.text.trim().is_empty()
        && indentation(underline.text) == indent
        && is_underline(underline.text))
    .then_some(indent)
}

fn section_kind(line: &str) -> Option<SectionKind> {
    match line.trim().to_ascii_lowercase().as_str() {
        "parameters" => Some(SectionKind::Parameters),
        "other param" | "other params" | "other parameter" | "other parameters" => {
            Some(SectionKind::OtherParameters)
        }
        "attributes" => Some(SectionKind::Attributes),
        "returns" | "return" => Some(SectionKind::Returns),
        "yields" | "yield" => Some(SectionKind::Yields),
        "raises" | "raise" => Some(SectionKind::Raises),
        _ => None,
    }
}

fn is_underline(line: &str) -> bool {
    let line = line.trim();
    line.len() >= 3 && line.chars().all(|char| char == '-')
}

fn parameter_item_starts(line: &ParsedLine<'_>, following_lines: &[ParsedLine<'_>]) -> bool {
    let trimmed = line.text.trim();
    if let Some(separator) = parse_type_separator(trimmed) {
        if !separator.requires_description_block {
            return true;
        }

        if !separator.ty.is_empty() {
            return !google::is_section_like_header(trimmed);
        }

        let description_line = following_lines
            .iter()
            .find(|line| !line.text.trim().is_empty())
            .filter(|next| indentation(next.text) > indentation(line.text));
        return description_line.is_some_and(|description_line| {
            !separator.ty.is_empty()
                || !google::section_body_starts_with_item(separator.name, description_line.text)
        });
    }

    is_item_name(trimmed)
}

fn named_item_starts(line: &ParsedLine<'_>, following_lines: &[ParsedLine<'_>]) -> bool {
    let trimmed = line.text.trim();
    if let Some(separator) = parse_type_separator(trimmed) {
        return !separator.requires_description_block
            || has_indented_description(line, following_lines);
    }

    untyped_item_starts(trimmed, line, following_lines)
}

fn untyped_item_starts(
    trimmed: &str,
    line: &ParsedLine<'_>,
    following_lines: &[ParsedLine<'_>],
) -> bool {
    is_item_name(trimmed) && has_indented_description(line, following_lines)
}

fn return_item_starts(line: &ParsedLine<'_>, following_lines: &[ParsedLine<'_>]) -> bool {
    let trimmed = line.text.trim();
    if let Some(separator) = parse_type_separator(trimmed) {
        return !separator.requires_description_block
            || has_indented_description(line, following_lines);
    }

    is_anonymous_return_type(trimmed)
}

/// Returns whether `line` is a valid anonymous NumPy-style return type.
pub(in crate::docstring) fn is_anonymous_return_type(line: &str) -> bool {
    !line.is_empty()
        && !line.ends_with('.')
        && !line.ends_with(':')
        && (is_docstring_type_expression(line) || is_prose_return_type(line))
}

fn is_prose_return_type(line: &str) -> bool {
    line.chars()
        .next()
        .is_some_and(|char| char.is_ascii_lowercase())
        && line
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, '_' | '-' | '.' | ' '))
}

fn raise_item_starts(line: &ParsedLine<'_>) -> bool {
    parse_raise_item(line.text.trim()).is_some()
}

fn parse_raise_item(line: &str) -> Option<&str> {
    let (name, description) = line
        .split_once(':')
        .map_or((line.trim(), None), |(name, description)| {
            (name.trim(), Some(description.trim()))
        });
    if !is_item_name(name) {
        return None;
    }

    Some(description.unwrap_or_default())
}

fn extend_parameter_documentation(
    parameters: &mut IndexMap<String, String>,
    lines: &[ParsedLine<'_>],
) {
    let Some(item_indent) = parameter_item_indent(lines) else {
        return;
    };
    let mut current: Option<Vec<(String, String)>> = None;

    for line in lines {
        let trimmed = line.text.trim();
        let line_indent = indentation(line.text);

        if trimmed.is_empty() {
            if let Some(current) = &mut current {
                for (_, description) in current {
                    description.push('\n');
                }
            }
            continue;
        }

        if line_indent == item_indent
            && let Some(parameter_group) = parse_parameter_line(trimmed)
        {
            insert_parameter_group(parameters, current.replace(parameter_group));
            continue;
        }

        if let Some(current) = &mut current {
            if line_indent <= item_indent {
                break;
            }
            for (_, description) in current {
                if !description.is_empty() {
                    description.push('\n');
                }
                description.push_str(trimmed);
            }
        }
    }

    insert_parameter_group(parameters, current);
}

fn parameter_item_indent(lines: &[ParsedLine<'_>]) -> Option<usize> {
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut item_indent = None;

    for (index, line) in lines.iter().enumerate() {
        if preformatted_blocks.consume_preformatted_line(line.text) {
            continue;
        }

        if parameter_item_starts(line, &lines[index + 1..]) {
            let line_indent = indentation(line.text);
            item_indent =
                Some(item_indent.map_or(line_indent, |indent: usize| indent.min(line_indent)));
        }
        preformatted_blocks.observe_line_outside_preformatted_block(line.text);
    }

    item_indent
}

fn parse_parameter_line(line: &str) -> Option<Vec<(String, String)>> {
    let name = line.split_once(':').map_or(line, |(name, _)| name).trim();
    Some(
        parameter_lookup_names(name)?
            .into_iter()
            .map(|name| (name, String::new()))
            .collect(),
    )
}

fn parameter_lookup_names(display_name: &str) -> Option<Vec<String>> {
    let mut lookup_names = Vec::new();
    for name in display_name.split(',').map(str::trim) {
        if name == "..." {
            continue;
        }

        let name = normalize_item_name(name);
        if !is_item_name_part(&name) {
            return None;
        }
        lookup_names.push(name.into_owned());
    }

    (!lookup_names.is_empty()).then_some(lookup_names)
}

fn insert_parameter_group(
    parameters: &mut IndexMap<String, String>,
    parameter_group: Option<Vec<(String, String)>>,
) {
    let Some(parameter_group) = parameter_group else {
        return;
    };

    for (name, description) in parameter_group {
        let description = description.trim().to_string();
        if !description.is_empty() {
            parameters.insert(name, description);
        }
    }
}

/// A parsed NumPy-style `name : type` separator.
pub(in crate::docstring) struct TypeSeparator<'a> {
    /// The documented item name.
    pub(in crate::docstring) name: &'a str,
    /// The documented item type.
    pub(in crate::docstring) ty: &'a str,
    /// Whether the separator requires an indented description to disambiguate it from prose.
    pub(in crate::docstring) requires_description_block: bool,
}

/// Parses a NumPy-style `name : type` separator.
pub(in crate::docstring) fn parse_type_separator(line: &str) -> Option<TypeSeparator<'_>> {
    let (name, ty) = split_once_unbracketed_colon(line)?;
    let has_whitespace_before_colon = name.chars().last().is_some_and(char::is_whitespace);
    let has_whitespace_after_colon = ty.chars().next().is_some_and(char::is_whitespace);
    if !has_whitespace_before_colon && !has_whitespace_after_colon && !ty.is_empty() {
        return None;
    }

    let name = name.trim();
    let ty = ty.trim();
    if !is_item_name(name) {
        return None;
    }
    Some(TypeSeparator {
        name,
        ty,
        requires_description_block: !has_whitespace_before_colon,
    })
}

fn has_indented_description(line: &ParsedLine<'_>, following_lines: &[ParsedLine<'_>]) -> bool {
    following_lines
        .iter()
        .find(|line| !line.text.trim().is_empty())
        .is_some_and(|next| indentation(next.text) > indentation(line.text))
}

/// Returns whether `name` is a valid NumPy-style item name or comma-separated name list.
pub(in crate::docstring) fn is_item_name(name: &str) -> bool {
    let mut has_lookup_name = false;
    let valid = name.split(',').all(|part| {
        let part = part.trim();
        if part == "..." {
            return true;
        }

        let part = normalize_item_name(part);
        if is_item_name_part(&part) {
            has_lookup_name = true;
            true
        } else {
            false
        }
    });

    valid && has_lookup_name
}

/// Removes reStructuredText escapes from NumPy variadic parameter names.
pub(in crate::docstring) fn normalize_item_name(name: &str) -> Cow<'_, str> {
    if name.contains(r"\*") {
        Cow::Owned(name.replace(r"\*", "*"))
    } else {
        Cow::Borrowed(name)
    }
}

fn is_item_name_part(name: &str) -> bool {
    let name = name
        .strip_prefix("**")
        .or_else(|| name.strip_prefix('*'))
        .unwrap_or(name);

    !name.is_empty() && name.split('.').all(is_identifier)
}
