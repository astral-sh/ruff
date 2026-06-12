use std::ops::Range;

use indexmap::IndexMap;
use ruff_python_stdlib::identifiers::is_identifier;

use super::SectionKind;
use crate::docstring::parsing::{ParsedLine, indentation, parsed_lines};
use crate::docstring::preformatted::PreformattedBlockScanner;

pub(in crate::docstring) struct Docstring<'a> {
    sections: Vec<Section<'a>>,
    parameters: IndexMap<String, String>,
}

impl<'a> Docstring<'a> {
    pub(in crate::docstring) fn parse(raw: &'a str) -> Self {
        let sections = parse_sections(raw);
        let parameters = parameter_documentation(&sections);
        Self {
            sections,
            parameters,
        }
    }

    pub(in crate::docstring) fn sections(&self) -> &[Section<'a>] {
        &self.sections
    }

    pub(in crate::docstring) fn parameter_documentation(&self) -> IndexMap<String, String> {
        self.parameters.clone()
    }
}

pub(in crate::docstring) struct Section<'a> {
    kind: SectionKind,
    indent: usize,
    range: Range<usize>,
    body: Vec<ParsedLine<'a>>,
}

impl<'a> Section<'a> {
    pub(in crate::docstring) fn kind(&self) -> SectionKind {
        self.kind
    }

    pub(in crate::docstring) fn indent(&self) -> usize {
        self.indent
    }

    pub(in crate::docstring) fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    pub(in crate::docstring) fn body(&self) -> &[ParsedLine<'a>] {
        &self.body
    }
}

fn parse_sections(raw: &str) -> Vec<Section<'_>> {
    let lines = parsed_lines(raw);
    let mut sections = Vec::new();
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut index = 0;

    while index < lines.len() {
        if preformatted_blocks.consume_preformatted_line(lines[index].text) {
            index += 1;
            continue;
        }

        let Some(header) = parse_numpy_section_header(&lines, index) else {
            preformatted_blocks.observe_non_preformatted_line(lines[index].text);
            index += 1;
            continue;
        };
        if header.indent != 0 {
            index += 1;
            continue;
        }

        let (body_end, range_end) = numpy_section_body_end(&lines, header);
        sections.push(Section {
            kind: header.kind,
            indent: header.indent,
            range: header.range_start..range_end,
            body: lines[header.body_start..body_end].to_vec(),
        });
        index = body_end;
    }

    sections
}

fn numpy_section_body_end(lines: &[ParsedLine<'_>], header: NumpySectionHeader) -> (usize, usize) {
    let mut body_end = header.body_start;
    let mut range_end = header.range_end;
    let mut body_preformatted_blocks = PreformattedBlockScanner::default();

    while let Some(line) = lines.get(body_end) {
        let previous_body = &lines[header.body_start..body_end];

        if line.text.trim().is_empty()
            && !numpy_blank_line_continues_section(previous_body, &lines[body_end..], header)
        {
            break;
        }

        if body_preformatted_blocks.is_active()
            && body_preformatted_blocks.consume_preformatted_line(line.text)
        {
            range_end = line.end;
            body_end += 1;
            continue;
        }

        if parse_numpy_section_header(lines, body_end)
            .is_some_and(|next| next.indent <= header.indent)
        {
            break;
        }

        if !line.text.trim().is_empty()
            && !numpy_line_belongs_to_body(header, line, previous_body, &lines[body_end + 1..])
        {
            break;
        }

        if !body_preformatted_blocks.consume_preformatted_line(line.text) {
            body_preformatted_blocks.observe_non_preformatted_line(line.text);
        }
        range_end = line.end;
        body_end += 1;
    }

    (body_end, range_end)
}

fn numpy_blank_line_continues_section(
    previous_lines: &[ParsedLine<'_>],
    lines: &[ParsedLine<'_>],
    header: NumpySectionHeader,
) -> bool {
    let Some((offset, non_blank_line)) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| !line.text.trim().is_empty())
    else {
        return false;
    };

    if parse_numpy_section_header(lines, offset).is_some_and(|next| next.indent <= header.indent) {
        return false;
    }

    numpy_line_belongs_to_body(header, non_blank_line, previous_lines, &lines[offset + 1..])
}

fn numpy_line_belongs_to_body(
    header: NumpySectionHeader,
    line: &ParsedLine<'_>,
    previous_lines: &[ParsedLine<'_>],
    following_lines: &[ParsedLine<'_>],
) -> bool {
    let line_indent = indentation(line.text);
    if line_indent > header.indent {
        return true;
    }

    if line_indent != header.indent {
        return false;
    }

    match header.kind {
        SectionKind::Parameters | SectionKind::Attributes => {
            numpy_named_item_starts(line, following_lines)
        }
        SectionKind::Returns | SectionKind::Yields => {
            numpy_return_item_starts(line, previous_lines, following_lines)
        }
        SectionKind::Raises => numpy_raise_item_starts(line, following_lines),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NumpySectionHeader {
    kind: SectionKind,
    indent: usize,
    body_start: usize,
    range_start: usize,
    range_end: usize,
}

fn parse_numpy_section_header(
    lines: &[ParsedLine<'_>],
    index: usize,
) -> Option<NumpySectionHeader> {
    let line = lines.get(index)?;
    let underline = lines.get(index + 1)?;
    let indent = indentation(line.text);
    if indentation(underline.text) != indent || !is_numpy_underline(underline.text) {
        return None;
    }

    let kind = parse_numpy_header(line.text)?;
    Some(NumpySectionHeader {
        kind,
        indent,
        body_start: index + 2,
        range_start: line.start,
        range_end: underline.end,
    })
}

fn parse_numpy_header(line: &str) -> Option<SectionKind> {
    match line.trim().to_ascii_lowercase().as_str() {
        "parameters" | "other parameter" | "other parameters" => Some(SectionKind::Parameters),
        "attributes" => Some(SectionKind::Attributes),
        "returns" | "return" => Some(SectionKind::Returns),
        "yields" | "yield" => Some(SectionKind::Yields),
        "raises" | "raise" => Some(SectionKind::Raises),
        _ => None,
    }
}

fn is_numpy_underline(line: &str) -> bool {
    let line = line.trim();
    line.len() >= 3 && line.chars().all(|char| char == '-')
}

fn numpy_named_item_starts(line: &ParsedLine<'_>, following_lines: &[ParsedLine<'_>]) -> bool {
    let trimmed = line.text.trim();
    split_numpy_type_separator(trimmed).is_some()
        || numpy_untyped_item_starts(trimmed, line, following_lines)
}

fn numpy_untyped_item_starts(
    trimmed: &str,
    line: &ParsedLine<'_>,
    following_lines: &[ParsedLine<'_>],
) -> bool {
    is_numpy_item_name(trimmed)
        && following_lines
            .iter()
            .find(|line| !line.text.trim().is_empty())
            .is_some_and(|next| indentation(next.text) > indentation(line.text))
}

fn numpy_return_item_starts(
    line: &ParsedLine<'_>,
    previous_lines: &[ParsedLine<'_>],
    following_lines: &[ParsedLine<'_>],
) -> bool {
    let trimmed = line.text.trim();
    parse_numpy_named_return_item(trimmed).is_some()
        || (!previous_lines
            .iter()
            .any(|line| !line.text.trim().is_empty())
            && is_numpy_anonymous_return_type(trimmed))
        || (is_numpy_anonymous_return_type(trimmed)
            && following_lines
                .iter()
                .find(|line| !line.text.trim().is_empty())
                .is_some_and(|next| indentation(next.text) > indentation(line.text)))
}

fn parse_numpy_named_return_item(line: &str) -> Option<(&str, &str)> {
    split_numpy_type_separator(line)
}

fn is_numpy_anonymous_return_type(line: &str) -> bool {
    !line.is_empty()
        && !line.ends_with('.')
        && !line.ends_with(':')
        && (is_structured_return_type(line) || is_numpy_prose_return_type(line))
}

fn is_numpy_prose_return_type(line: &str) -> bool {
    line.chars()
        .next()
        .is_some_and(|char| char.is_ascii_lowercase())
        && line
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, '_' | '-' | '.' | ' '))
}

fn numpy_raise_item_starts(line: &ParsedLine<'_>, following_lines: &[ParsedLine<'_>]) -> bool {
    let trimmed = line.text.trim();
    parse_numpy_raise_item(trimmed).is_some_and(|description| !description.is_empty())
        || numpy_untyped_item_starts(trimmed, line, following_lines)
}

fn parse_numpy_raise_item(line: &str) -> Option<&str> {
    let (name, description) = line
        .split_once(':')
        .map_or((line.trim(), None), |(name, description)| {
            (name.trim(), Some(description.trim()))
        });
    if !is_numpy_item_name(name) {
        return None;
    }

    Some(description.unwrap_or_default())
}

fn parameter_documentation(sections: &[Section<'_>]) -> IndexMap<String, String> {
    let mut parameters = IndexMap::new();

    for section in sections
        .iter()
        .filter(|section| section.kind == SectionKind::Parameters)
    {
        let mut current: Option<Vec<(String, String)>> = None;
        let mut item_indent = None;

        for line in &section.body {
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

            if item_indent.is_none_or(|indent| line_indent == indent) {
                if let Some(parameter_group) = parse_parameter_line(trimmed) {
                    insert_parameter_group(&mut parameters, current.replace(parameter_group));
                    item_indent.get_or_insert(line_indent);
                    continue;
                }
            }

            if let Some(current) = &mut current {
                if line_indent <= item_indent.unwrap_or(section.indent) {
                    break;
                }
                for (_, description) in current {
                    if !description.is_empty() {
                        description.push('\n');
                    }
                    description.push_str(trimmed);
                }
            } else {
                break;
            }
        }

        insert_parameter_group(&mut parameters, current);
    }

    parameters
}

fn parse_parameter_line(line: &str) -> Option<Vec<(String, String)>> {
    let name = line.split_once(':').map_or(line, |(name, _)| name).trim();
    let lookup_names = name
        .split(',')
        .map(parameter_lookup_name)
        .collect::<Option<Vec<_>>>()?;

    (!lookup_names.is_empty()).then(|| {
        lookup_names
            .into_iter()
            .map(|name| (name, String::new()))
            .collect()
    })
}

fn parameter_lookup_name(name: &str) -> Option<String> {
    let name = name.trim();
    is_numpy_item_name(name).then(|| name.to_string())
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
            parameters.entry(name).or_insert(description);
        }
    }
}

fn split_numpy_type_separator(line: &str) -> Option<(&str, &str)> {
    let (name, ty) = line.split_once(':')?;
    if !name.chars().last().is_some_and(char::is_whitespace)
        && !ty.chars().next().is_some_and(char::is_whitespace)
    {
        return None;
    }

    let name = name.trim();
    let ty = ty.trim();
    if !is_numpy_item_name(name) || ty.is_empty() {
        return None;
    }

    Some((name, ty))
}

fn is_numpy_item_name(name: &str) -> bool {
    name.split(',').all(|part| {
        let part = part.trim();
        let part = part
            .strip_prefix("**")
            .or_else(|| part.strip_prefix('*'))
            .unwrap_or(part);

        !part.is_empty() && part.split('.').all(is_numpy_name_part)
    })
}

fn is_numpy_name_part(part: &str) -> bool {
    is_identifier(part)
}

fn is_structured_return_type(ty: &str) -> bool {
    if ty.is_empty() || !ty.chars().all(is_structured_return_type_char) {
        return false;
    }

    !ty.chars().any(char::is_whitespace) || ty.contains('[') || ty.contains(',') || ty.contains('|')
}

fn is_structured_return_type_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || "_.[](){},|\"':/ ".contains(ch)
}
