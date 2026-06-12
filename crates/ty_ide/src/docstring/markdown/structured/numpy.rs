use std::borrow::Cow;

use ruff_text_size::TextRange;

use crate::docstring::document::numpy::{
    self, is_anonymous_return_type, is_item_name, normalize_item_name, parse_type_separator,
};
use crate::docstring::document::syntax::{ParsedLine, split_once_unbracketed_colon};

use super::body::{SectionItemBuilder, parse_named_items, parse_named_items_with_leading_prose};
use super::{Section, SectionKind};

pub(super) fn structured_sections(source: &str) -> Vec<Section> {
    let mut sections = Vec::new();

    numpy::visit_sections(source, |kind, range, body| {
        if let Some(section) = structured_section(kind, range, body) {
            sections.push(section);
        }
    });

    sections
}

fn structured_section(
    kind: SectionKind,
    range: TextRange,
    body: &[ParsedLine<'_>],
) -> Option<Section> {
    let items = match kind {
        SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters => {
            parse_named_items_with_leading_prose(kind, body, parse_named_item)
        }
        SectionKind::Attributes => parse_named_items(kind, body, parse_named_item),
        SectionKind::Returns | SectionKind::Yields => {
            parse_named_items(kind, body, parse_return_item)
        }
        SectionKind::Raises => parse_named_items(kind, body, parse_raise_item_builder),
    }?;

    Section::new(range, items)
}

fn parse_named_item<'a>(line: &ParsedLine<'a>) -> Option<SectionItemBuilder<'a>> {
    let trimmed = line.text.trim();
    let (name, ty) = if let Some(separator) = parse_type_separator(trimmed) {
        (separator.name, Some(separator.ty))
    } else {
        is_item_name(trimmed).then_some((trimmed, None))?
    };

    let mut item = SectionItemBuilder::new(Some(name), ty, "");
    if let Cow::Owned(name) = normalize_item_name(name) {
        item.set_display_name(name);
    }
    Some(item)
}

fn parse_return_item<'a>(line: &ParsedLine<'a>) -> Option<SectionItemBuilder<'a>> {
    let trimmed = line.text.trim();
    if let Some(separator) = parse_type_separator(trimmed) {
        return Some(SectionItemBuilder::new(
            Some(separator.name),
            Some(separator.ty),
            "",
        ));
    }
    if has_named_return_separator(trimmed) {
        return None;
    }

    is_anonymous_return_type(trimmed).then(|| SectionItemBuilder::new(None, Some(trimmed), ""))
}

fn has_named_return_separator(line: &str) -> bool {
    split_once_unbracketed_colon(line)
        .is_some_and(|(name, _)| name.chars().last().is_some_and(char::is_whitespace))
}

fn parse_raise_item_builder<'a>(line: &ParsedLine<'a>) -> Option<SectionItemBuilder<'a>> {
    let (name, description) = parse_raise_item(line.text.trim())?;
    Some(SectionItemBuilder::new(Some(name), None, description))
}

fn parse_raise_item(line: &str) -> Option<(&str, &str)> {
    let (name, description) = line
        .split_once(':')
        .map_or((line.trim(), ""), |(name, description)| {
            (name.trim(), description.trim())
        });
    is_item_name(name).then_some((name, description))
}
