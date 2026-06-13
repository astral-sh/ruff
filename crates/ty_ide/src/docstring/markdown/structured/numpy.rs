use ruff_python_stdlib::identifiers::is_identifier;

use crate::docstring::formats::{self, SectionKind};
use crate::docstring::parsing::{ParsedLine, indentation};

use super::{
    DescriptionLine, DocstringSectionKind, SectionBlock, SectionCandidate, SectionItem,
    SectionItemBuilder, is_structured_return_type, parse_named_items, split_once_unbracketed_colon,
};

pub(super) fn section_candidates(
    docstring: &formats::numpy::Docstring<'_>,
) -> Vec<SectionCandidate> {
    docstring
        .sections()
        .iter()
        .filter_map(|section| {
            if section.indent() != 0 {
                return None;
            }
            let kind = docstring_section_kind(section.kind());
            let block = numpy_section_block(kind, section.indent(), section.body())?;
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

fn parse_numpy_named_item(_kind: DocstringSectionKind, line: &str) -> Option<SectionItemBuilder> {
    let (name, ty) = split_numpy_type_separator(line).map_or_else(
        || is_numpy_item_name(line.trim()).then_some((line.trim(), None)),
        |(name, ty)| Some((name, Some(ty))),
    )?;

    Some(SectionItemBuilder {
        display_name: Some(name.to_string()),
        ty: ty.map(str::to_string),
        description_lines: Vec::new(),
    })
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

fn parse_numpy_return_item(line: &str) -> Option<SectionItemBuilder> {
    parse_numpy_named_return_item(line).or_else(|| {
        is_numpy_anonymous_return_type(line).then(|| SectionItemBuilder {
            display_name: None,
            ty: Some(line.to_string()),
            description_lines: Vec::new(),
        })
    })
}

fn parse_numpy_named_return_item(line: &str) -> Option<SectionItemBuilder> {
    let (name, ty) = split_numpy_type_separator(line)?;

    Some(SectionItemBuilder {
        display_name: Some(name.to_string()),
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
