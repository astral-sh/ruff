use indexmap::IndexMap;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::docstring::parsing::{indentation, parsed_lines};
use crate::docstring::preformatted::PreformattedBlockScanner;

pub(in crate::docstring) struct Docstring {
    parameters: IndexMap<String, String>,
}

impl Docstring {
    pub(in crate::docstring) fn parse(raw: &str) -> Self {
        let parameters = parse_parameter_documentation(raw);
        Self { parameters }
    }

    pub(in crate::docstring) fn parameter_documentation(&self) -> IndexMap<String, String> {
        self.parameters.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SectionKind {
    Parameters,
    Other,
}

fn parse_parameter_documentation(raw: &str) -> IndexMap<String, String> {
    let lines = parsed_lines(raw);
    let mut parameters = IndexMap::new();
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    let mut index = 0;

    while index < lines.len() {
        if preformatted_blocks.consume_preformatted_line(lines[index]) {
            index += 1;
            continue;
        }

        let Some(header) = parse_google_section_like_header(&lines, index) else {
            preformatted_blocks.observe_non_preformatted_line(lines[index]);
            index += 1;
            continue;
        };
        if header.indent != 0 {
            index += 1;
            continue;
        }

        let body_end = google_section_body_end(&lines, header);
        if header.kind == SectionKind::Parameters {
            extend_parameter_documentation(&mut parameters, &lines[header.body_start..body_end]);
        }
        index = body_end;
    }

    parameters
}

fn google_section_body_end(lines: &[&str], header: GoogleSectionHeader) -> usize {
    let mut body_end = header.body_start;
    let mut body_preformatted_blocks = PreformattedBlockScanner::default();

    while let Some(&line) = lines.get(body_end) {
        if body_preformatted_blocks.is_active()
            && body_preformatted_blocks.consume_preformatted_line(line)
        {
            body_end += 1;
            continue;
        }

        if line.trim().is_empty()
            && !google_blank_line_continues_section(&lines[body_end..], header)
        {
            break;
        }

        if google_section_header_ends_body(lines, body_end, header) {
            break;
        }

        if !line.trim().is_empty() && !google_line_belongs_to_body(header, line) {
            break;
        }

        if !body_preformatted_blocks.consume_preformatted_line(line) {
            body_preformatted_blocks.observe_non_preformatted_line(line);
        }
        body_end += 1;
    }

    body_end
}

fn google_blank_line_continues_section(lines: &[&str], header: GoogleSectionHeader) -> bool {
    let Some((offset, non_blank_line)) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| !line.trim().is_empty())
    else {
        return false;
    };

    if google_section_header_ends_body(lines, offset, header) {
        return false;
    }

    google_line_belongs_to_body(header, non_blank_line)
}

fn google_section_header_ends_body(
    lines: &[&str],
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
    kind: SectionKind,
    indent: usize,
    body_start: usize,
}

fn parse_google_section_like_header(lines: &[&str], index: usize) -> Option<GoogleSectionHeader> {
    let line = lines.get(index)?;
    let kind = google_section_kind(line)?;

    Some(GoogleSectionHeader {
        kind,
        indent: indentation(line),
        body_start: index + 1,
    })
}

fn google_section_kind(line: &str) -> Option<SectionKind> {
    let name = normalized_google_section_name(line)?;
    let kind = match name.as_str() {
        "args" | "arguments" | "parameters" | "keyword args" | "keyword arguments" => {
            SectionKind::Parameters
        }
        "attributes" | "example" | "examples" | "note" | "notes" | "other parameters"
        | "references" | "return" | "returns" | "raise" | "raises" | "see also" | "todo"
        | "todos" | "warning" | "warnings" | "yield" | "yields" => SectionKind::Other,
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

fn parse_google_parameter(line: &str) -> Option<(String, String)> {
    let (name, description) = split_once_unbracketed_colon(line)?;
    let name = name.trim();
    let (display_name, _) = parse_parenthesized_type(name);
    let lookup_name = google_parameter_lookup_name(display_name)?;

    Some((lookup_name, description.trim().to_string()))
}

fn extend_parameter_documentation(parameters: &mut IndexMap<String, String>, lines: &[&str]) {
    let mut current: Option<(String, String)> = None;
    let mut item_indent = None;

    for line in lines {
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
            && let Some(parameter) = parse_google_parameter(trimmed)
        {
            insert_parameter_documentation(parameters, current.replace(parameter));
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
