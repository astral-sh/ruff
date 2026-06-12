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
    kind: Option<SectionKind>,
    indent: usize,
    range: Range<usize>,
    body: Vec<ParsedLine<'a>>,
}

impl<'a> Section<'a> {
    pub(in crate::docstring) fn kind(&self) -> Option<SectionKind> {
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

        let Some(header) = parse_google_section_like_header(&lines, index) else {
            preformatted_blocks.observe_non_preformatted_line(lines[index].text);
            index += 1;
            continue;
        };
        if header.indent != 0 {
            index += 1;
            continue;
        }

        let (body_end, range_end) = google_section_body_end(&lines, header);
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

fn google_section_body_end(
    lines: &[ParsedLine<'_>],
    header: GoogleSectionHeader,
) -> (usize, usize) {
    let mut body_end = header.body_start;
    let mut range_end = header.range_end;
    let mut body_preformatted_blocks = PreformattedBlockScanner::default();

    while let Some(line) = lines.get(body_end) {
        if body_preformatted_blocks.is_active()
            && body_preformatted_blocks.consume_preformatted_line(line.text)
        {
            range_end = line.end;
            body_end += 1;
            continue;
        }

        if line.text.trim().is_empty()
            && !google_blank_line_continues_section(&lines[body_end..], header)
        {
            break;
        }

        if google_section_header_ends_body(lines, body_end, header) {
            break;
        }

        if !line.text.trim().is_empty() && !google_line_belongs_to_body(header, line.text) {
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

fn google_blank_line_continues_section(
    lines: &[ParsedLine<'_>],
    header: GoogleSectionHeader,
) -> bool {
    let Some((offset, non_blank_line)) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| !line.text.trim().is_empty())
    else {
        return false;
    };

    if google_section_header_ends_body(lines, offset, header) {
        return false;
    }

    google_line_belongs_to_body(header, non_blank_line.text)
}

fn google_section_header_ends_body(
    lines: &[ParsedLine<'_>],
    index: usize,
    header: GoogleSectionHeader,
) -> bool {
    let Some(next) = parse_google_section_like_header(lines, index) else {
        return false;
    };

    next.indent <= header.indent
}

fn google_line_belongs_to_body(header: GoogleSectionHeader, line: &str) -> bool {
    indentation(line) > header.indent
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GoogleSectionHeader {
    kind: Option<SectionKind>,
    indent: usize,
    body_start: usize,
    range_start: usize,
    range_end: usize,
}

fn parse_google_section_like_header(
    lines: &[ParsedLine<'_>],
    index: usize,
) -> Option<GoogleSectionHeader> {
    let line = lines.get(index)?;
    let kind = match google_section_kind(line.text)? {
        GoogleSectionKind::Supported(kind) => Some(kind),
        GoogleSectionKind::Unsupported => None,
    };

    Some(GoogleSectionHeader {
        kind,
        indent: indentation(line.text),
        body_start: index + 1,
        range_start: line.start,
        range_end: line.end,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GoogleSectionKind {
    Supported(SectionKind),
    Unsupported,
}

fn google_section_kind(line: &str) -> Option<GoogleSectionKind> {
    let name = normalized_google_section_name(line)?;
    let kind = match name.as_str() {
        "args" | "arguments" | "parameters" | "keyword args" | "keyword arguments" => {
            GoogleSectionKind::Supported(SectionKind::Parameters)
        }
        "attributes" => GoogleSectionKind::Supported(SectionKind::Attributes),
        "return" | "returns" => GoogleSectionKind::Supported(SectionKind::Returns),
        "yield" | "yields" => GoogleSectionKind::Supported(SectionKind::Yields),
        "raise" | "raises" => GoogleSectionKind::Supported(SectionKind::Raises),
        "example" | "examples" | "note" | "notes" | "other parameters" | "references"
        | "see also" | "todo" | "todos" | "warning" | "warnings" => GoogleSectionKind::Unsupported,
        _ => return None,
    };
    Some(kind)
}

fn normalized_google_section_name(line: &str) -> Option<String> {
    let name = line.trim().strip_suffix(':')?.trim();
    Some(
        name.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase(),
    )
}

fn parameter_documentation(sections: &[Section<'_>]) -> IndexMap<String, String> {
    let mut parameters = IndexMap::new();

    for section in sections
        .iter()
        .filter(|section| section.kind == Some(SectionKind::Parameters))
    {
        let mut current: Option<(String, String)> = None;
        let mut item_indent = None;

        for line in &section.body {
            let trimmed = line.text.trim();
            let line_indent = indentation(line.text);

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
                && let Some(parameter) = parse_parameter_line(trimmed)
            {
                insert_parameter_documentation(&mut parameters, current.replace(parameter));
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

        insert_parameter_documentation(&mut parameters, current);
    }

    parameters
}

fn parse_parameter_line(line: &str) -> Option<(String, String)> {
    let (name, description) = split_once_unbracketed_colon(line)?;
    let name = name.trim();
    let (display_name, _) = parse_parenthesized_type(name);
    let lookup_name = google_parameter_lookup_name(display_name)?;

    Some((lookup_name, description.trim().to_string()))
}
fn insert_parameter_documentation(
    parameters: &mut IndexMap<String, String>,
    parameter: Option<(String, String)>,
) {
    let Some((name, description)) = parameter else {
        return;
    };
    let description = description.trim().to_string();
    if !description.is_empty() {
        parameters.entry(name).or_insert(description);
    }
}

fn google_parameter_lookup_name(display_name: &str) -> Option<String> {
    let name = display_name.split(',').next()?.trim();
    let identifier = name
        .strip_prefix("**")
        .or_else(|| name.strip_prefix('*'))
        .unwrap_or(name);

    is_identifier(identifier).then(|| name.to_string())
}

fn split_once_unbracketed_colon(line: &str) -> Option<(&str, &str)> {
    let mut parentheses = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut quote = None;
    let mut escaped = false;

    for (index, char) in line.char_indices() {
        if let Some(quote_char) = quote {
            if escaped {
                escaped = false;
            } else if char == '\\' {
                escaped = true;
            } else if char == quote_char {
                quote = None;
            }
            continue;
        }

        match char {
            '\'' | '"' => quote = Some(char),
            '(' => parentheses += 1,
            ')' => parentheses = parentheses.saturating_sub(1),
            '[' => brackets += 1,
            ']' => brackets = brackets.saturating_sub(1),
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            ':' if parentheses == 0 && brackets == 0 && braces == 0 => {
                return Some((&line[..index], &line[index + ':'.len_utf8()..]));
            }
            _ => {}
        }
    }

    None
}

fn parse_parenthesized_type(name: &str) -> (&str, Option<&str>) {
    if !name.ends_with(')') {
        return (name, None);
    }

    let mut depth = 0usize;
    for (index, char) in name.char_indices().rev() {
        match char {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    return (name, None);
                }
                depth -= 1;
                if depth == 0 {
                    let display_name = name[..index].trim();
                    let ty = name[index + '('.len_utf8()..name.len() - ')'.len_utf8()].trim();
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
