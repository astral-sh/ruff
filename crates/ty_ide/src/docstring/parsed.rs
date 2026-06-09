use std::borrow::Cow;
use std::ops::Range;

use ruff_python_stdlib::identifiers::is_identifier;
use ruff_python_trivia::leading_indentation;
use ruff_source_file::UniversalNewlines;
use ruff_text_size::TextSize;
use rustc_hash::{FxHashMap, FxHashSet};

use super::preformatted::PreformattedBlockScanner;
use super::rest;
use super::sections::{DocstringItem, DocstringSectionKind, DocstringSections};

/// A tolerant, display-oriented parse of a normalized docstring.
pub(super) struct ParsedDocstring<'a> {
    raw: &'a str,
    blocks: Vec<Block<'a>>,
}

impl<'a> ParsedDocstring<'a> {
    pub(super) fn parse(raw: &'a str) -> Self {
        let rest = rest::Docstring::parse(raw);
        let blocks = parse_blocks(raw, rest.field_lists());

        Self { raw, blocks }
    }

    pub(super) fn render_markdown_source(&self) -> Cow<'a, str> {
        if self.blocks.is_empty()
            || matches!(
                self.blocks.as_slice(),
                [Block::Raw(raw)] if *raw == self.raw
            )
        {
            return Cow::Borrowed(self.raw);
        }

        let mut output = String::new();
        for (index, block) in self.blocks.iter().enumerate() {
            match block {
                Block::Raw(raw) => output.push_str(raw),
                Block::Section(section) => {
                    output.push_str(&section.render_markdown());
                    if let Some(next) = self.blocks.get(index + 1) {
                        section.render_boundary_before_following_block(&mut output, next.as_raw());
                    }
                }
            }
        }

        Cow::Owned(output)
    }

    pub(super) fn parameter_documentation(&self) -> Vec<ParameterDocumentation> {
        let mut parameters: Vec<ParameterDocumentation> = Vec::new();
        let mut names = FxHashSet::default();
        let mut rest_parameter_indices: FxHashMap<String, usize> = FxHashMap::default();

        for field_list in rest::Docstring::parse(self.raw).field_lists() {
            for field in field_list.fields() {
                let rest::Field::Parameter {
                    lookup_name,
                    description,
                    ..
                } = field
                else {
                    continue;
                };

                if description.is_empty() {
                    continue;
                }

                let name = lookup_name.to_string();
                if let Some(index) = rest_parameter_indices.get(&name).copied() {
                    parameters[index].description.clone_from(description);
                } else {
                    rest_parameter_indices.insert(name.clone(), parameters.len());
                    names.insert(name.clone());
                    parameters.push(ParameterDocumentation {
                        name,
                        description: description.clone(),
                    });
                }
            }
        }

        for parameter in self
            .blocks
            .iter()
            .filter_map(Block::as_section)
            .flat_map(SectionBlock::parameter_documentation)
        {
            if names.insert(parameter.name.clone()) {
                parameters.push(parameter);
            }
        }

        parameters
    }
}

fn parse_blocks<'a>(raw: &'a str, rest_field_lists: &[rest::FieldList]) -> Vec<Block<'a>> {
    let mut sections = Vec::new();
    sections.extend(rest_section_candidates(rest_field_lists));
    sections.extend(google_section_candidates(raw));
    sections.extend(numpy_section_candidates(raw));
    sections.sort_by_key(|section| section.range.start);
    let mut blocks = Vec::new();
    let mut rendered_through = 0;

    for section in sections {
        if section.range.start < rendered_through {
            continue;
        }

        if !push_raw_block(&mut blocks, raw, rendered_through..section.range.start) {
            return Vec::new();
        }
        rendered_through = section.range.end;
        blocks.push(Block::Section(section.block));
    }

    if !blocks.is_empty() && !push_raw_block(&mut blocks, raw, rendered_through..raw.len()) {
        return Vec::new();
    }

    blocks
}

fn rest_section_candidates(field_lists: &[rest::FieldList]) -> Vec<SectionCandidate> {
    let mut sections = Vec::new();

    for field_list in field_lists {
        if field_list.indent() != TextSize::default() {
            continue;
        }

        let range = field_list.range();

        let Some(section) = rest_section_block(field_list) else {
            continue;
        };

        sections.push(SectionCandidate {
            range: range.start().to_usize()..range.end().to_usize(),
            block: section,
        });
    }

    sections
}

fn push_raw_block<'a>(blocks: &mut Vec<Block<'a>>, raw: &'a str, range: Range<usize>) -> bool {
    if range.is_empty() {
        return true;
    }

    let Some(raw) = raw.get(range) else {
        return false;
    };
    blocks.push(Block::Raw(raw));
    true
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SectionCandidate {
    range: Range<usize>,
    block: SectionBlock,
}

fn google_section_candidates(raw: &str) -> Vec<SectionCandidate> {
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
        if let Some(kind) = header.kind
            && let Some(section) = google_section_block(kind, &lines[header.body_start..body_end])
        {
            sections.push(SectionCandidate {
                range: header.range_start..range_end,
                block: section,
            });
        }
        index = body_end;
    }

    sections
}

fn parsed_lines(raw: &str) -> Vec<ParsedLine<'_>> {
    raw.universal_newlines()
        .map(|line| ParsedLine {
            text: line.as_str(),
            start: line.start().to_usize(),
            end: line.end().to_usize(),
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct ParsedLine<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn google_section_body_end(
    lines: &[ParsedLine<'_>],
    header: GoogleSectionHeader,
) -> (usize, usize) {
    let mut body_end = header.body_start;
    let mut range_end = header.range_end;
    let mut body_preformatted_blocks = PreformattedBlockScanner::default();

    while let Some(line) = lines.get(body_end) {
        let previous_body = &lines[header.body_start..body_end];

        if body_preformatted_blocks.is_active()
            && body_preformatted_blocks.consume_preformatted_line(line.text)
        {
            range_end = line.end;
            body_end += 1;
            continue;
        }

        if line.text.trim().is_empty()
            && !google_blank_line_continues_section(previous_body, &lines[body_end..], header)
        {
            break;
        }

        if parse_google_section_like_header(lines, body_end)
            .is_some_and(|next| next.indent <= header.indent)
        {
            break;
        }

        if google_top_level_preformatted_block_follows_body(header, line, previous_body) {
            break;
        }

        if !line.text.trim().is_empty()
            && !google_line_belongs_to_body(header, line.text, previous_body)
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

fn google_blank_line_continues_section(
    previous_lines: &[ParsedLine<'_>],
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

    if parse_google_section_like_header(lines, offset)
        .is_some_and(|next| next.indent <= header.indent)
    {
        return false;
    }

    google_line_belongs_to_body_after_blank(header, non_blank_line.text, previous_lines)
}

fn google_top_level_preformatted_block_follows_body(
    header: GoogleSectionHeader,
    line: &ParsedLine<'_>,
    previous_lines: &[ParsedLine<'_>],
) -> bool {
    header.range_start == 0
        && indentation(line.text) == header.indent
        && previous_lines
            .iter()
            .any(|line| !line.text.trim().is_empty())
        && PreformattedBlockScanner::line_starts_preformatted_block(line.text)
}

fn google_line_belongs_to_body(
    header: GoogleSectionHeader,
    line: &str,
    previous_lines: &[ParsedLine<'_>],
) -> bool {
    let line_indent = indentation(line);
    if line_indent > header.indent {
        return true;
    }

    // `documentation_trim` ignores the first docstring line when computing common
    // indentation, so the body of a first-line section can be dedented to the
    // header's column before structured parsing sees it.
    if header.range_start != 0
        || line_indent != header.indent
        || is_google_section_like_header(line.trim())
    {
        return false;
    }

    if previous_lines
        .iter()
        .any(|line| !line.text.trim().is_empty() && indentation(line.text) > header.indent)
    {
        return matches!(
            header.kind,
            Some(
                kind @ (DocstringSectionKind::Parameters
                | DocstringSectionKind::Attributes
                | DocstringSectionKind::Raises)
            ) if parse_google_named_item(kind, line.trim()).is_some()
                && google_named_item_indent(kind, previous_lines) == Some(line_indent)
        );
    }

    true
}

fn google_named_item_indent(kind: DocstringSectionKind, lines: &[ParsedLine<'_>]) -> Option<usize> {
    lines.iter().find_map(|line| {
        parse_google_named_item(kind, line.text.trim()).map(|_| indentation(line.text))
    })
}

fn google_line_belongs_to_body_after_blank(
    header: GoogleSectionHeader,
    line: &str,
    previous_lines: &[ParsedLine<'_>],
) -> bool {
    if !google_line_belongs_to_body(header, line, previous_lines) {
        return false;
    }

    let line_indent = indentation(line);
    if header.range_start != 0 || line_indent > header.indent {
        return true;
    }

    match header.kind {
        Some(
            kind @ (DocstringSectionKind::Parameters
            | DocstringSectionKind::Attributes
            | DocstringSectionKind::Raises),
        ) => parse_google_named_item(kind, line.trim()).is_some(),
        Some(DocstringSectionKind::Returns | DocstringSectionKind::Yields) | None => true,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GoogleSectionHeader {
    kind: Option<DocstringSectionKind>,
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
    let (kind, indent) = if let Some((kind, indent)) = parse_google_header(line.text) {
        (Some(kind), indent)
    } else if is_google_section_like_header(line.text.trim()) {
        (None, indentation(line.text))
    } else {
        return None;
    };

    Some(GoogleSectionHeader {
        kind,
        indent,
        body_start: index + 1,
        range_start: line.start,
        range_end: line.end,
    })
}

fn parse_google_header(line: &str) -> Option<(DocstringSectionKind, usize)> {
    let name = line.trim().strip_suffix(':')?.trim();
    let kind = match normalized_google_section_name(name).as_str() {
        "args" | "arguments" | "parameters" | "keyword args" | "keyword arguments" => {
            DocstringSectionKind::Parameters
        }
        "attributes" => DocstringSectionKind::Attributes,
        "returns" | "return" => DocstringSectionKind::Returns,
        "yields" | "yield" => DocstringSectionKind::Yields,
        "raises" | "raise" => DocstringSectionKind::Raises,
        _ => return None,
    };
    Some((kind, indentation(line)))
}

fn is_google_section_like_header(line: &str) -> bool {
    if parse_google_header(line).is_some() {
        return true;
    }

    let Some(name) = line.strip_suffix(':') else {
        return false;
    };

    matches!(
        normalized_google_section_name(name).as_str(),
        "example"
            | "examples"
            | "note"
            | "notes"
            | "other parameters"
            | "references"
            | "see also"
            | "todo"
            | "todos"
            | "warning"
            | "warnings"
    )
}

fn normalized_google_section_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn google_section_block(
    kind: DocstringSectionKind,
    body: &[ParsedLine<'_>],
) -> Option<SectionBlock> {
    let items = if matches!(
        kind,
        DocstringSectionKind::Returns | DocstringSectionKind::Yields
    ) {
        parse_google_return_item(kind, body)?
    } else {
        parse_google_named_items(kind, body)?
    };

    Some(SectionBlock::new(items))
}

fn numpy_section_candidates(raw: &str) -> Vec<SectionCandidate> {
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
        if let Some(section) = numpy_section_block(
            header.kind,
            header.indent,
            &lines[header.body_start..body_end],
        ) {
            sections.push(SectionCandidate {
                range: header.range_start..range_end,
                block: section,
            });
            index = body_end;
        } else {
            index += 1;
        }
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
        DocstringSectionKind::Parameters | DocstringSectionKind::Attributes => {
            numpy_named_item_starts(line, following_lines)
        }
        DocstringSectionKind::Returns | DocstringSectionKind::Yields => {
            numpy_return_item_starts(line, previous_lines, following_lines)
        }
        DocstringSectionKind::Raises => numpy_raise_item_starts(line, following_lines),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NumpySectionHeader {
    kind: DocstringSectionKind,
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

fn parse_numpy_header(line: &str) -> Option<DocstringSectionKind> {
    match line.trim().to_ascii_lowercase().as_str() {
        "parameters" => Some(DocstringSectionKind::Parameters),
        "attributes" => Some(DocstringSectionKind::Attributes),
        "returns" | "return" => Some(DocstringSectionKind::Returns),
        "yields" | "yield" => Some(DocstringSectionKind::Yields),
        "raises" | "raise" => Some(DocstringSectionKind::Raises),
        _ => None,
    }
}

fn is_numpy_underline(line: &str) -> bool {
    let line = line.trim();
    line.len() >= 3 && line.chars().all(|char| char == '-')
}

fn numpy_section_block(
    kind: DocstringSectionKind,
    indent: usize,
    body: &[ParsedLine<'_>],
) -> Option<SectionBlock> {
    let items = match kind {
        DocstringSectionKind::Parameters | DocstringSectionKind::Attributes => {
            parse_named_items(kind, body, |line| parse_numpy_named_item(kind, line))?
        }
        DocstringSectionKind::Returns | DocstringSectionKind::Yields => {
            parse_numpy_return_items(kind, body, indent)?
        }
        DocstringSectionKind::Raises => parse_named_items(kind, body, parse_numpy_raise_item)?,
    };

    Some(SectionBlock::new(items))
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

fn parse_numpy_named_item(kind: DocstringSectionKind, line: &str) -> Option<SectionItemBuilder> {
    let (name, ty) = split_numpy_type_separator(line).map_or_else(
        || is_numpy_item_name(line.trim()).then_some((line.trim(), None)),
        |(name, ty)| Some((name, Some(ty))),
    )?;

    Some(SectionItemBuilder {
        display_name: Some(name.to_string()),
        lookup_name: (kind == DocstringSectionKind::Parameters)
            .then(|| numpy_parameter_lookup_name(name))
            .flatten(),
        ty: ty.map(str::to_string),
        description_lines: Vec::new(),
    })
}

fn numpy_parameter_lookup_name(display_name: &str) -> Option<String> {
    comma_separated_lookup_names(display_name)
        .into_iter()
        .next()
}

fn parse_numpy_return_items(
    kind: DocstringSectionKind,
    body: &[ParsedLine<'_>],
    section_indent: usize,
) -> Option<Vec<SectionItem>> {
    let first = body.iter().find(|line| !line.text.trim().is_empty())?;
    if indentation(first.text) != section_indent {
        return None;
    }

    parse_named_items(kind, body, parse_numpy_return_item)
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

fn parse_numpy_return_item(line: &str) -> Option<SectionItemBuilder> {
    parse_numpy_named_return_item(line).or_else(|| {
        is_numpy_anonymous_return_type(line).then(|| SectionItemBuilder {
            display_name: None,
            lookup_name: None,
            ty: Some(line.to_string()),
            description_lines: Vec::new(),
        })
    })
}

fn parse_numpy_named_return_item(line: &str) -> Option<SectionItemBuilder> {
    let (name, ty) = split_numpy_type_separator(line)?;

    Some(SectionItemBuilder {
        display_name: Some(name.to_string()),
        lookup_name: None,
        ty: Some(ty.to_string()),
        description_lines: Vec::new(),
    })
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
    parse_numpy_raise_item(trimmed).is_some_and(|item| !item.description_lines.is_empty())
        || numpy_untyped_item_starts(trimmed, line, following_lines)
}

fn parse_numpy_raise_item(line: &str) -> Option<SectionItemBuilder> {
    let (name, description) = line
        .split_once(':')
        .map_or((line.trim(), None), |(name, description)| {
            (name.trim(), Some(description.trim()))
        });
    if !is_numpy_item_name(name) {
        return None;
    }

    Some(SectionItemBuilder {
        display_name: Some(name.to_string()),
        lookup_name: None,
        ty: None,
        description_lines: description
            .filter(|description| !description.is_empty())
            .map(DescriptionLine::normalized)
            .into_iter()
            .collect(),
    })
}

fn split_numpy_type_separator(line: &str) -> Option<(&str, &str)> {
    let (name, ty) = split_once_unbracketed_colon(line)?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct SectionItemBuilder {
    display_name: Option<String>,
    lookup_name: Option<String>,
    ty: Option<String>,
    description_lines: Vec<DescriptionLine>,
}

impl SectionItemBuilder {
    fn finish(self, kind: DocstringSectionKind) -> SectionItem {
        SectionItem::new(
            kind,
            self.display_name,
            self.lookup_name,
            self.ty,
            normalize_description(self.description_lines),
        )
    }

    fn push_description(&mut self, line: &str) {
        self.description_lines
            .push(DescriptionLine::Source(line.to_string()));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DescriptionLine {
    Normalized(String),
    Source(String),
}

impl DescriptionLine {
    fn normalized(line: &str) -> Self {
        Self::Normalized(line.trim().to_string())
    }
}

fn parse_google_named_items(
    kind: DocstringSectionKind,
    body: &[ParsedLine<'_>],
) -> Option<Vec<SectionItem>> {
    parse_named_items(kind, body, |line| parse_google_named_item(kind, line))
}

fn parse_named_items(
    kind: DocstringSectionKind,
    body: &[ParsedLine<'_>],
    mut parse_item: impl FnMut(&str) -> Option<SectionItemBuilder>,
) -> Option<Vec<SectionItem>> {
    let mut items = Vec::new();
    let mut current: Option<SectionItemBuilder> = None;
    let mut item_indent = None;

    for line in body {
        let trimmed = line.text.trim();
        if trimmed.is_empty() {
            if let Some(current) = &mut current {
                current.push_description("");
            }
            continue;
        }

        let line_indent = indentation(line.text);
        if item_indent.is_none_or(|indent| line_indent == indent) {
            if let Some(item) = parse_item(trimmed) {
                if let Some(current) = current.replace(item) {
                    items.push(current.finish(kind));
                }
                item_indent.get_or_insert(line_indent);
                continue;
            }
            if item_indent.is_some() {
                return None;
            }
        }
        if item_indent.is_some_and(|indent| line_indent < indent) {
            return None;
        }

        let current = current.as_mut()?;
        current.push_description(line.text);
    }

    if let Some(current) = current {
        items.push(current.finish(kind));
    }
    (!items.is_empty()).then_some(items)
}

fn parse_google_named_item(kind: DocstringSectionKind, line: &str) -> Option<SectionItemBuilder> {
    if is_google_section_like_header(line) {
        return None;
    }

    let (name, description) = split_once_unbracketed_colon(line)?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let (display_name, lookup_name, ty) = match kind {
        DocstringSectionKind::Parameters => {
            let (display_name, ty) = parse_parenthesized_type(name);
            let lookup_name = google_parameter_lookup_name(display_name)?;
            (
                display_name.to_string(),
                Some(lookup_name),
                ty.map(str::to_string),
            )
        }
        DocstringSectionKind::Attributes => {
            let (display_name, ty) = parse_parenthesized_type(name);
            (display_name.to_string(), None, ty.map(str::to_string))
        }
        DocstringSectionKind::Raises => (name.to_string(), None, None),
        DocstringSectionKind::Returns | DocstringSectionKind::Yields => return None,
    };

    Some(SectionItemBuilder {
        display_name: Some(display_name),
        lookup_name,
        ty,
        description_lines: vec![DescriptionLine::normalized(description)],
    })
}

fn parse_google_return_item(
    kind: DocstringSectionKind,
    body: &[ParsedLine<'_>],
) -> Option<Vec<SectionItem>> {
    let mut lines = body.iter().skip_while(|line| line.text.trim().is_empty());
    let first_line = lines.next()?;
    let first = first_line.text.trim();
    if is_google_section_like_header(first) {
        return None;
    }

    let (ty, first_description) = split_google_return_type(first)
        .map_or((None, first), |(ty, description)| (Some(ty), description));

    let mut description_lines = vec![DescriptionLine::normalized(first_description)];
    let mut preformatted_blocks = PreformattedBlockScanner::default();
    if !preformatted_blocks.consume_preformatted_line(first_line.text) {
        preformatted_blocks.observe_non_preformatted_line(first_line.text);
    }

    for line in lines {
        if preformatted_blocks.consume_preformatted_line(line.text) {
            description_lines.push(DescriptionLine::Source(line.text.to_string()));
            continue;
        }
        if is_google_section_like_header(line.text.trim()) {
            return None;
        }
        preformatted_blocks.observe_non_preformatted_line(line.text);
        description_lines.push(DescriptionLine::Source(line.text.to_string()));
    }

    Some(vec![SectionItem::new(
        kind,
        None,
        None,
        ty.map(str::to_string),
        normalize_description(description_lines),
    )])
}

fn google_parameter_lookup_name(display_name: &str) -> Option<String> {
    comma_separated_lookup_names(display_name)
        .into_iter()
        .next()
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

fn split_google_return_type(line: &str) -> Option<(&str, &str)> {
    let (ty, description) = split_once_unbracketed_colon(line)?;
    let ty = ty.trim();
    if is_uri_scheme_prefix(ty, description) {
        return None;
    }

    let description = description.trim();

    if is_structured_return_type(ty) {
        Some((ty, description))
    } else {
        None
    }
}

fn is_uri_scheme_prefix(ty: &str, description: &str) -> bool {
    if !is_uri_scheme(ty) {
        return false;
    }

    if description.starts_with("//") {
        return true;
    }

    let Some(first) = description.chars().next() else {
        return false;
    };
    if first.is_whitespace() {
        return false;
    }

    matches!(first, '/' | '?' | '#' | '@' | ':')
        || description
            .chars()
            .skip(1)
            .any(|char| matches!(char, '/' | '?' | '#' | '@' | ':'))
}

fn is_uri_scheme(scheme: &str) -> bool {
    let mut chars = scheme.chars();
    chars.next().is_some_and(|char| char.is_ascii_alphabetic())
        && chars.all(|char| char.is_ascii_alphanumeric() || matches!(char, '+' | '-' | '.'))
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

fn normalize_description(lines: Vec<DescriptionLine>) -> String {
    let dedent = lines
        .iter()
        .filter_map(|line| match line {
            DescriptionLine::Source(line) if !line.trim().is_empty() => Some(indentation(line)),
            DescriptionLine::Normalized(_) | DescriptionLine::Source(_) => None,
        })
        .min()
        .unwrap_or(0);

    let mut description = lines
        .into_iter()
        .map(|line| match line {
            DescriptionLine::Normalized(line) => line,
            DescriptionLine::Source(line) => {
                strip_indentation(&line, dedent).trim_end().to_string()
            }
        })
        .skip_while(String::is_empty)
        .collect::<Vec<_>>();
    while description.last().is_some_and(String::is_empty) {
        description.pop();
    }
    description.join("\n")
}

fn strip_indentation(line: &str, width: usize) -> &str {
    let mut indentation_width = 0;
    for (index, char) in line.char_indices() {
        let char_width = match char {
            ' ' => 1,
            '\t' => 8,
            _ => return &line[index..],
        };

        if indentation_width + char_width > width {
            return &line[index..];
        }

        indentation_width += char_width;
        if indentation_width == width {
            return &line[index + char.len_utf8()..];
        }
    }

    ""
}

fn indentation(line: &str) -> usize {
    leading_indentation(line)
        .chars()
        .map(|char| if char == '\t' { 8 } else { 1 })
        .sum()
}

fn rest_section_block(field_list: &rest::FieldList) -> Option<SectionBlock> {
    let plan = RestFieldRenderPlan::from_fields(field_list.fields())?;
    let items = plan.items(field_list.fields());
    items
        .iter()
        .all(|item| !item.is_empty())
        .then(|| SectionBlock::new(items))
}

/// Validates a reST field list and stores cross-field metadata needed while rendering.
struct RestFieldRenderPlan<'a> {
    parameter_types: FxHashMap<&'a str, &'a str>,
    attribute_types: FxHashMap<&'a str, &'a str>,
    return_type: Option<&'a str>,
    has_returns: bool,
}

impl<'a> RestFieldRenderPlan<'a> {
    fn from_fields(fields: &'a [rest::Field]) -> Option<Self> {
        let mut has_rendered_field = false;
        let mut has_returns = false;
        let mut has_return_type = false;
        let mut parameters: FxHashMap<&'a str, TypedFieldRenderState> = FxHashMap::default();
        let mut attributes: FxHashMap<&'a str, TypedFieldRenderState> = FxHashMap::default();
        let mut parameter_types: FxHashMap<&'a str, &'a str> = FxHashMap::default();
        let mut attribute_types: FxHashMap<&'a str, &'a str> = FxHashMap::default();
        let mut return_type = None;

        for field in fields {
            match field {
                rest::Field::Parameter {
                    lookup_name, ty, ..
                } => {
                    has_rendered_field = true;
                    parameters
                        .entry(lookup_name.as_str())
                        .or_default()
                        .record_field(ty.is_some());
                }
                rest::Field::Attribute { name, ty, .. } => {
                    has_rendered_field = true;
                    attributes
                        .entry(name.as_str())
                        .or_default()
                        .record_field(ty.is_some());
                }
                rest::Field::Returns { .. } => {
                    has_rendered_field = true;
                    has_returns = true;
                }
                rest::Field::Raises { .. } => {
                    has_rendered_field = true;
                }
                rest::Field::ParameterType { lookup_name, ty } => {
                    if parameter_types
                        .insert(lookup_name.as_str(), ty.as_str())
                        .is_some()
                    {
                        return None;
                    }
                }
                rest::Field::AttributeType { name, ty } => {
                    if attribute_types.insert(name.as_str(), ty.as_str()).is_some() {
                        return None;
                    }
                }
                rest::Field::ReturnType { ty } => {
                    if has_return_type {
                        return None;
                    }
                    has_return_type = true;
                    return_type = (!ty.is_empty()).then_some(ty.as_str());
                    has_rendered_field |= return_type.is_some();
                }
                rest::Field::Metadata => {}
                rest::Field::Unknown { .. } => return None,
            }
        }

        for lookup_name in parameter_types.keys() {
            if !parameters
                .get(*lookup_name)
                .is_some_and(TypedFieldRenderState::accepts_separate_type)
            {
                return None;
            }
        }

        for name in attribute_types.keys() {
            if !attributes
                .get(*name)
                .is_some_and(TypedFieldRenderState::accepts_separate_type)
            {
                return None;
            }
        }

        if !has_rendered_field {
            return None;
        }

        Some(Self {
            parameter_types,
            attribute_types,
            return_type,
            has_returns,
        })
    }

    fn items(&self, fields: &'a [rest::Field]) -> Vec<SectionItem> {
        let mut items = Vec::new();

        for field in fields {
            match field {
                rest::Field::Parameter {
                    display_name,
                    lookup_name,
                    ty,
                    description,
                } => items.push(SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some(display_name.to_string()),
                    Some(lookup_name.to_string()),
                    ty.as_deref()
                        .or_else(|| {
                            self.parameter_types
                                .get(lookup_name.as_str())
                                .copied()
                                .filter(|ty| !ty.is_empty())
                        })
                        .map(str::to_string),
                    description.clone(),
                )),
                rest::Field::Attribute {
                    name,
                    ty,
                    description,
                } => items.push(SectionItem::new(
                    DocstringSectionKind::Attributes,
                    Some(name.to_string()),
                    None,
                    ty.as_deref()
                        .or_else(|| {
                            self.attribute_types
                                .get(name.as_str())
                                .copied()
                                .filter(|ty| !ty.is_empty())
                        })
                        .map(str::to_string),
                    description.clone(),
                )),
                rest::Field::Returns { name, description } => items.push(SectionItem::new(
                    DocstringSectionKind::Returns,
                    name.as_ref().map(ToString::to_string),
                    None,
                    self.return_type.map(str::to_string),
                    description.clone(),
                )),
                rest::Field::Raises {
                    exception,
                    description,
                } => items.push(SectionItem::new(
                    DocstringSectionKind::Raises,
                    exception.as_ref().map(ToString::to_string),
                    None,
                    None,
                    description.clone(),
                )),
                rest::Field::ReturnType { .. } if !self.has_returns => {
                    if let Some(return_type) = self.return_type {
                        items.push(SectionItem::new(
                            DocstringSectionKind::Returns,
                            None,
                            None,
                            Some(return_type.to_string()),
                            String::new(),
                        ));
                    }
                }
                rest::Field::ParameterType { .. }
                | rest::Field::AttributeType { .. }
                | rest::Field::ReturnType { .. }
                | rest::Field::Metadata
                | rest::Field::Unknown { .. } => {}
            }
        }

        items
    }
}

#[derive(Default)]
struct TypedFieldRenderState {
    has_untyped_field: bool,
    has_inline_typed_field: bool,
}

impl TypedFieldRenderState {
    fn record_field(&mut self, has_inline_type: bool) {
        if has_inline_type {
            self.has_inline_typed_field = true;
        } else {
            self.has_untyped_field = true;
        }
    }

    fn accepts_separate_type(&self) -> bool {
        self.has_untyped_field && !self.has_inline_typed_field
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Block<'a> {
    Raw(&'a str),
    Section(SectionBlock),
}

impl Block<'_> {
    fn as_raw(&self) -> Option<&str> {
        match self {
            Self::Raw(raw) => Some(raw),
            Self::Section(_) => None,
        }
    }

    fn as_section(&self) -> Option<&SectionBlock> {
        match self {
            Self::Section(section) => Some(section),
            Self::Raw(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SectionBlock {
    items: Vec<SectionItem>,
}

impl SectionBlock {
    pub(super) fn new(items: Vec<SectionItem>) -> Self {
        Self { items }
    }

    fn render_markdown(&self) -> String {
        self.sections().render_markdown()
    }

    fn render_boundary_before_following_block(
        &self,
        output: &mut String,
        following_raw: Option<&str>,
    ) {
        self.sections()
            .render_boundary_before_following_block(output, following_raw);
    }

    fn sections(&self) -> DocstringSections<'_> {
        let mut sections = DocstringSections::default();
        for item in &self.items {
            sections.push(
                item.kind,
                DocstringItem::new(
                    item.display_name.as_deref(),
                    item.ty.as_deref(),
                    item.description.as_str(),
                ),
            );
        }

        sections
    }

    fn parameter_documentation(&self) -> Vec<ParameterDocumentation> {
        let mut parameters = Vec::new();

        for item in &self.items {
            if item.kind != DocstringSectionKind::Parameters || item.description.is_empty() {
                continue;
            }

            for name in item.parameter_lookup_names() {
                parameters.push(ParameterDocumentation {
                    name,
                    description: item.description.clone(),
                });
            }
        }

        parameters
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SectionItem {
    kind: DocstringSectionKind,
    display_name: Option<String>,
    lookup_name: Option<String>,
    ty: Option<String>,
    description: String,
}

impl SectionItem {
    pub(super) fn new(
        kind: DocstringSectionKind,
        display_name: Option<String>,
        lookup_name: Option<String>,
        ty: Option<String>,
        description: String,
    ) -> Self {
        Self {
            kind,
            display_name,
            lookup_name,
            ty,
            description,
        }
    }

    fn parameter_lookup_names(&self) -> Vec<String> {
        let Some(lookup_name) = &self.lookup_name else {
            return Vec::new();
        };

        let mut names = self
            .display_name
            .as_deref()
            .map(comma_separated_lookup_names)
            .unwrap_or_default();
        if !names.iter().any(|name| name == lookup_name) {
            names.clear();
            names.push(lookup_name.clone());
        }
        names
    }

    fn is_empty(&self) -> bool {
        self.display_name.is_none()
            && self.ty.as_deref().is_none_or(str::is_empty)
            && self.description.is_empty()
    }
}

fn comma_separated_lookup_names(display_name: &str) -> Vec<String> {
    display_name
        .split(',')
        .filter_map(|name| {
            let lookup_name = name.trim().trim_start_matches('*');
            (!lookup_name.is_empty()).then(|| lookup_name.to_string())
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParameterDocumentation {
    pub(super) name: String,
    pub(super) description: String,
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::{Block, ParsedDocstring, SectionBlock, SectionItem};
    use crate::docstring::sections::DocstringSectionKind;

    #[test]
    fn raw_docstring_renders_borrowed() {
        let docstring = "Summary.\n\nDetails.";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);
        assert!(parsed.parameter_documentation().is_empty());

        let parsed = ParsedDocstring {
            raw: docstring,
            blocks: vec![Block::Raw(&docstring[.."Summary.".len()])],
        };

        assert_eq!(parsed.render_markdown_source(), "Summary.");
    }

    #[test]
    fn rest_field_lists_render_markdown_sections() {
        let docstring = "\
Summary.

:param str value: The value.
:param other: Another value.
:type other: int
:returns: Whether validation passed.
:rtype: bool
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        Summary.

        ## Parameters
        `value` (`str`): The value.
        `other` (`int`): Another value.

        ## Returns
        `bool`: Whether validation passed.
        ");

        let parameters = parsed.parameter_documentation();
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters[0].name, "value");
        assert_eq!(parameters[0].description, "The value.");
        assert_eq!(parameters[1].name, "other");
        assert_eq!(parameters[1].description, "Another value.");

        let docstring = "\
:param value: Stale description.
:param value: Corrected description.
";
        let parsed = ParsedDocstring::parse(docstring);
        let parameters = parsed.parameter_documentation();

        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, "value");
        assert_eq!(parameters[0].description, "Corrected description.");

        let docstring = "\
Summary.

:param value: The value.
:rtype: str
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        Summary.

        ## Parameters
        `value`: The value.

        ## Returns
        `str`
        ");

        let docstring = "\
Summary.

:rtype: str
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        Summary.

        ## Returns
        `str`
        ");
    }

    #[test]
    fn rest_field_lists_render_edge_cases() {
        let docstring = "\
This is a function description.
:class:`Foo` instances can be passed here.

:param str param1: The first parameter description
:meta private:
:param param2: The second parameter description
:type param2: int
:kwparam retries: Retry attempts.
:paramtype retries: int
:param *args: Extra positional arguments.
:type args: tuple[str, ...]
:param **kwargs: Extra keyword arguments.
:type **kwargs: dict[str, object]
:var cache: Cached data.
:vartype cache: dict[str,
    object]
:ivar state: Instance state.
:var str title: Display title.
:cvar VERSION: Package version.
:vartype VERSION: str
:returns baz: The return value description
:rtype: dict[str,
    int]
:raises ValueError: If the value is invalid.
:meta hide-value:
:exception RuntimeError: If the system is unavailable.";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        This is a function description.
        :class:`Foo` instances can be passed here.

        ## Parameters
        `param1` (`str`): The first parameter description
        `param2` (`int`): The second parameter description
        `retries` (`int`): Retry attempts.
        `*args` (`tuple[str, ...]`): Extra positional arguments.
        `**kwargs` (`dict[str, object]`): Extra keyword arguments.

        ## Attributes
        `cache` (`dict[str, object]`): Cached data.
        `state`: Instance state.
        `title` (`str`): Display title.
        `VERSION` (`str`): Package version.

        ## Returns
        `baz` (`dict[str, int]`): The return value description

        ## Raises
        `ValueError`: If the value is invalid.
        `RuntimeError`: If the system is unavailable.
        ");
    }

    #[test]
    fn rest_field_lists_preserve_unrenderable_and_preformatted_lists() {
        let docstring = "\
:param first: First parameter.
:type orphan: str

Some prose between field lists.

:meta private:

Markdown input:

```text
:param sample: This is sample input
```

Doctest output:

>>> print(\"field list\")
:param sample: This is sample output

Literal block::

    :param sample: This is sample input

:param second:
    - First option.
    - Second option.
:param third:
    1. Validate the input.
    2. Return the result.
:param done: Whether work is done.";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        :param first: First parameter.
        :type orphan: str

        Some prose between field lists.

        :meta private:

        Markdown input:

        ```text
        :param sample: This is sample input
        ```

        Doctest output:

        >>> print(\"field list\")
        :param sample: This is sample output

        Literal block::

            :param sample: This is sample input

        ## Parameters
        `second`:
        - First option.
        - Second option.

        `third`:
        1. Validate the input.
        2. Return the result.

        `done`: Whether work is done.
        ");

        let parameters = parsed.parameter_documentation();
        assert_eq!(parameters.len(), 4);
        assert_eq!(parameters[0].name, "first");
        assert_eq!(parameters[1].name, "second");
        assert_eq!(parameters[2].name, "third");
        assert_eq!(parameters[3].name, "done");
    }

    #[test]
    fn google_sections_render_markdown_sections() {
        let docstring = "\
Summary.

Args:
    value (str): The value.
        More detail.
    *items: Extra items.

Returns:
    bool: Whether validation passed.

Yields:
    int: Next value.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        Summary.

        ## Parameters
        `value` (`str`): The value.
            More detail.
        `*items`: Extra items.

        ## Returns
        `bool`: Whether validation passed.

        ## Yields
        `int`: Next value.
        ");

        let parameters = parsed.parameter_documentation();
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters[0].name, "value");
        assert_eq!(parameters[0].description, "The value.\nMore detail.");
        assert_eq!(parameters[1].name, "items");
        assert_eq!(parameters[1].description, "Extra items.");

        let docstring = "\
Args:
    x, y: Coordinates.
";
        let parsed = ParsedDocstring::parse(docstring);
        let parameters = parsed.parameter_documentation();

        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters[0].name, "x");
        assert_eq!(parameters[0].description, "Coordinates.");
        assert_eq!(parameters[1].name, "y");
        assert_eq!(parameters[1].description, "Coordinates.");

        let docstring = "\
Args:
    value: The value.
Additional details.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        ## Parameters
        `value`: The value.
        Additional details.
        ");

        let docstring = "\
Args:
    value: The value.
Methods:
    work: Does work.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        ## Parameters
        `value`: The value.
        Methods:
            work: Does work.
        ");

        let docstring = "\
Returns:
    bool: Whether validation passed.
Additional details.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        ## Returns
        `bool`: Whether validation passed.
        Additional details.
        ");

        let docstring = "\
Returns:
    str: Example output.
        ```python
        Args:
            value: still code.
        Returns:
            still code.
        ```
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        ## Returns
        `str`: Example output.

        ```python
        Args:
            value: still code.
        Returns:
            still code.
        ```
        ");

        let docstring = "\
Yields:
    int: Example output.
        Example::
            Args:
                still code.
            Yields:
                still code.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        ## Yields
        `int`: Example output.
            Example::
                Args:
                    still code.
                Yields:
                    still code.
        ");
    }

    #[test]
    fn unsupported_google_sections_stay_raw() {
        let docstring = "\
Summary.

Args:
    Inputs are normalized first.
    value: The value.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let docstring = "\
Summary.

Examples:
    Args:
        value: demo input.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let docstring = "\
Summary.

Returns:
    bool: Whether validation passed.

    Examples:
        Use it.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let docstring = "\
Summary.

Yields:
    int: Next value.

    Examples:
        Use it.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let docstring = "\
Summary.

Args:
    Inputs are normalized first.
    Args:
        value: demo input.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let docstring = "\
Summary.

Args:
    value: The value.

    Examples:
        Use it.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let docstring = "\
Summary.

Returns:
    Examples:
        Use it.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let docstring = "\
Summary.

Args:
    value: Example.
        ```python

Args:
    nested = 1
        ```
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);
    }

    #[test]
    fn numpy_sections_render_markdown_sections() {
        let docstring = "\
Summary.

Parameters
----------
value, alias : str
    The value.
other
    Another value.

Returns
-------
result : bool
    Whether validation passed.

Yields
------
int
    Next value.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        Summary.

        ## Parameters
        `value, alias` (`str`): The value.
        `other`: Another value.

        ## Returns
        `result` (`bool`): Whether validation passed.

        ## Yields
        `int`: Next value.
        ");

        let parameters = parsed.parameter_documentation();
        assert_eq!(parameters.len(), 3);
        assert_eq!(parameters[0].name, "value");
        assert_eq!(parameters[0].description, "The value.");
        assert_eq!(parameters[1].name, "alias");
        assert_eq!(parameters[1].description, "The value.");
        assert_eq!(parameters[2].name, "other");
        assert_eq!(parameters[2].description, "Another value.");

        let docstring = "\
Summary.

Parameters
----------
value: str
    The value.

Returns
-------
result: bool
    Whether validation passed.

Yields
------
item: int
    Next value.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        Summary.

        ## Parameters
        `value` (`str`): The value.

        ## Returns
        `result` (`bool`): Whether validation passed.

        ## Yields
        `item` (`int`): Next value.
        ");

        let parameters = parsed.parameter_documentation();
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, "value");
        assert_eq!(parameters[0].description, "The value.");

        let docstring = "\
Summary.

Returns
-------
list of int
    Primary values.
list of node-like
    Related nodes.

Yields
------
str
    Next label.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        Summary.

        ## Returns
        `list of int`: Primary values.
        `list of node-like`: Related nodes.

        ## Yields
        `str`: Next label.
        ");
    }

    #[test]
    fn unsupported_numpy_sections_stay_raw() {
        let docstring = "\
Summary.

Returns
-------
    The created object.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);
    }

    #[test]
    fn indented_sections_stay_raw() {
        let docstring = "\
Summary.

    Args:
        value: The value.

    Parameters
    ----------
    other : str
        Another value.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);
    }

    #[test]
    fn unsupported_rest_field_lists_stay_raw() {
        let docstring = "\
Summary.

:param value: The value.
:unknown field: Preserve this field list.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let docstring = "\
Summary.

:returns:
:raises:
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let docstring = "\
Summary.

:param value: The value.
:returns:
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);
    }

    #[test]
    fn section_blocks_render_markdown_source_and_parameter_docs() {
        let parsed = ParsedDocstring {
            raw: "Summary.\n\nArgs:\n    value: The value.",
            blocks: vec![
                Block::Raw("Summary.\n\n"),
                Block::Section(SectionBlock::new(vec![
                    SectionItem::new(
                        DocstringSectionKind::Parameters,
                        Some("value".to_string()),
                        Some("value".to_string()),
                        Some("str".to_string()),
                        "The value.".to_string(),
                    ),
                    SectionItem::new(
                        DocstringSectionKind::Returns,
                        None,
                        None,
                        Some("bool".to_string()),
                        "Whether validation passed.".to_string(),
                    ),
                ])),
            ],
        };

        assert_snapshot!(parsed.render_markdown_source(), @"
        Summary.

        ## Parameters
        `value` (`str`): The value.

        ## Returns
        `bool`: Whether validation passed.
        ");

        let parameters = parsed.parameter_documentation();
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, "value");
        assert_eq!(parameters[0].description, "The value.");
    }

    #[test]
    fn section_blocks_separate_following_raw_blocks() {
        let parsed = ParsedDocstring {
            raw: "Args:\n    value: The value.\nAfter.",
            blocks: vec![
                Block::Section(SectionBlock::new(vec![SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some("value".to_string()),
                    Some("value".to_string()),
                    None,
                    "The value.".to_string(),
                )])),
                Block::Raw("After."),
            ],
        };

        assert_snapshot!(parsed.render_markdown_source(), @"
        ## Parameters
        `value`: The value.
        After.
        ");

        let parsed = ParsedDocstring {
            raw: "Args:\n    value: The value.\n\nAfter.",
            blocks: vec![
                Block::Section(SectionBlock::new(vec![SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some("value".to_string()),
                    Some("value".to_string()),
                    None,
                    "The value.".to_string(),
                )])),
                Block::Raw("\n\nAfter."),
            ],
        };

        assert_snapshot!(parsed.render_markdown_source(), @"
        ## Parameters
        `value`: The value.

        After.
        ");

        let parsed = ParsedDocstring {
            raw: "Args:\n    value:\n        - First option.\nAfter.",
            blocks: vec![
                Block::Section(SectionBlock::new(vec![SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some("value".to_string()),
                    Some("value".to_string()),
                    None,
                    "- First option.".to_string(),
                )])),
                Block::Raw("After."),
            ],
        };

        assert_snapshot!(parsed.render_markdown_source(), @"
        ## Parameters
        `value`:
        - First option.

        After.
        ");

        let parsed = ParsedDocstring {
            raw: "Args:\n    value:\n        - First option.\nAfter.",
            blocks: vec![
                Block::Section(SectionBlock::new(vec![SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some("value".to_string()),
                    Some("value".to_string()),
                    None,
                    "- First option.".to_string(),
                )])),
                Block::Raw("\nAfter."),
            ],
        };

        assert_snapshot!(parsed.render_markdown_source(), @"
        ## Parameters
        `value`:
        - First option.

        After.
        ");

        let parsed = ParsedDocstring {
            raw: "Args:\n    value:\n        ```python\n        value = 1\nAfter.",
            blocks: vec![
                Block::Section(SectionBlock::new(vec![SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some("value".to_string()),
                    Some("value".to_string()),
                    None,
                    "```python\nvalue = 1".to_string(),
                )])),
                Block::Raw("After."),
            ],
        };

        assert_snapshot!(parsed.render_markdown_source(), @"
        ## Parameters
        `value`:
        ```python
        value = 1
        ```

        After.
        ");

        let parsed = ParsedDocstring {
            raw: "Yields:\n    int:\n        - Next value.\nAfter.",
            blocks: vec![
                Block::Section(SectionBlock::new(vec![SectionItem::new(
                    DocstringSectionKind::Yields,
                    None,
                    None,
                    Some("int".to_string()),
                    "- Next value.".to_string(),
                )])),
                Block::Raw("After."),
            ],
        };

        assert_snapshot!(parsed.render_markdown_source(), @"
        ## Yields
        `int`:
        - Next value.

        After.
        ");
    }
}
