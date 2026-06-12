use crate::docstring::formats::{self, SectionKind};
use crate::docstring::parsing::ParsedLine;
use crate::docstring::preformatted::PreformattedBlockScanner;

use super::{
    DescriptionLine, DocstringSectionKind, SectionBlock, SectionCandidate, SectionItem,
    SectionItemBuilder, is_structured_return_type, is_uri_scheme_prefix, normalize_description,
    parse_named_items, parse_parenthesized_type, split_once_unbracketed_colon,
};

pub(super) fn section_candidates(
    docstring: &formats::google::Docstring<'_>,
) -> Vec<SectionCandidate> {
    docstring
        .sections()
        .iter()
        .filter_map(|section| {
            if section.indent() != 0 {
                return None;
            }
            let kind = docstring_section_kind(section.kind()?);
            let block = google_section_block(kind, section.body())?;
            Some(SectionCandidate {
                range: section.range(),
                block,
            })
        })
        .collect()
}

fn docstring_section_kind(kind: SectionKind) -> DocstringSectionKind {
    match kind {
        SectionKind::Parameters => DocstringSectionKind::Parameters,
        SectionKind::Attributes => DocstringSectionKind::Attributes,
        SectionKind::Returns => DocstringSectionKind::Returns,
        SectionKind::Yields => DocstringSectionKind::Yields,
        SectionKind::Raises => DocstringSectionKind::Raises,
    }
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

fn parse_google_named_items(
    kind: DocstringSectionKind,
    body: &[ParsedLine<'_>],
) -> Option<Vec<SectionItem>> {
    parse_named_items(kind, body, |line| parse_google_named_item(kind, line))
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

    let (display_name, ty) = match kind {
        DocstringSectionKind::Parameters => {
            let (display_name, ty) = parse_parenthesized_type(name);
            (display_name.to_string(), ty.map(str::to_string))
        }
        DocstringSectionKind::Attributes => {
            let (display_name, ty) = parse_parenthesized_type(name);
            (display_name.to_string(), ty.map(str::to_string))
        }
        DocstringSectionKind::Raises => (name.to_string(), None),
        DocstringSectionKind::Returns | DocstringSectionKind::Yields => return None,
    };

    Some(SectionItemBuilder {
        display_name: Some(display_name),
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

    let description = normalize_description(description_lines);
    Some(vec![SectionItem::new(kind, None, ty, &description)])
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

fn parse_google_header(line: &str) -> Option<DocstringSectionKind> {
    let name = line.trim().strip_suffix(':')?.trim();
    match normalized_google_section_name(name).as_str() {
        "args" | "arguments" | "parameters" | "keyword args" | "keyword arguments" => {
            Some(DocstringSectionKind::Parameters)
        }
        "attributes" => Some(DocstringSectionKind::Attributes),
        "returns" | "return" => Some(DocstringSectionKind::Returns),
        "yields" | "yield" => Some(DocstringSectionKind::Yields),
        "raises" | "raise" => Some(DocstringSectionKind::Raises),
        _ => None,
    }
}

fn normalized_google_section_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}
