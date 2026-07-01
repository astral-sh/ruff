use ruff_text_size::TextRange;

use crate::docstring::document::google;
use crate::docstring::document::preformatted::PreformattedBlockScanner;
use crate::docstring::document::syntax::{
    ParsedLine, indentation, is_docstring_type_expression, parse_parenthesized_type,
    split_once_unbracketed_colon,
};

use super::body::{
    SectionItemBuilder, parse_named_items, parse_named_items_with_aligned_continuations,
};
use super::{Section, SectionItem, SectionKind};

pub(super) fn structured_sections(source: &str) -> Vec<Section> {
    let mut sections = Vec::new();

    google::visit_sections(source, |kind, range, header_indent, body| {
        if let Some(section) = google_section(kind, range, header_indent, body) {
            sections.push(section);
        }
    });

    sections
}

fn google_section(
    kind: SectionKind,
    range: TextRange,
    header_indent: usize,
    body: &[ParsedLine<'_>],
) -> Option<Section> {
    let items = match kind {
        SectionKind::Returns | SectionKind::Yields => parse_return_item(kind, body),
        SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters => {
            parse_named_items_with_aligned_continuations(kind, header_indent, body, |line| {
                parse_named_item(kind, line)
            })
        }
        SectionKind::Attributes | SectionKind::Raises => {
            parse_named_items(kind, body, |line| parse_named_item(kind, line))
        }
    }?;
    Section::new(range, items)
}

fn parse_named_item<'a>(
    kind: SectionKind,
    line: &ParsedLine<'a>,
) -> Option<SectionItemBuilder<'a>> {
    let line_text = line.text.trim();
    let section_like_header = is_uppercase_section_like_header(line_text);

    let (name, description) = split_field_colon(line_text)?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    if is_uri_scheme_prefix(name, description) {
        return None;
    }
    if section_like_header
        && !(kind == SectionKind::Raises && google::has_exception_name_suffix(name))
    {
        return None;
    }

    let (display_name, ty) = match kind {
        SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters => {
            let (display_name, ty) = parse_parenthesized_type(name);
            if !is_parameter_display_name(display_name) {
                return None;
            }
            (display_name, ty)
        }
        SectionKind::Attributes => {
            let (display_name, ty) = parse_parenthesized_type(name);
            if !is_attribute_display_name(display_name) {
                return None;
            }
            (display_name, ty)
        }
        SectionKind::Raises => {
            if !google::is_dotted_identifier(name) {
                return None;
            }
            (name, None)
        }
        SectionKind::Returns | SectionKind::Yields => return None,
    };

    Some(SectionItemBuilder::new(Some(display_name), ty, description))
}

fn parse_return_item(kind: SectionKind, body: &[ParsedLine<'_>]) -> Option<Vec<SectionItem>> {
    let mut lines = body.iter().skip_while(|line| line.text.trim().is_empty());
    let first_line = lines.next()?;
    let first = first_line.text.trim();
    if is_uppercase_section_like_header(first) {
        return None;
    }

    let mut preformatted = PreformattedBlockScanner::default();
    let first_is_preformatted = preformatted.consume_preformatted_line(first_line.text);
    let (ty, first_description) = if first_is_preformatted {
        (None, first)
    } else {
        split_return_type(first).map_or((None, first), |(ty, description)| (Some(ty), description))
    };
    let first_indent = indentation(first_line.text);
    let has_type = ty.is_some();
    let mut item = SectionItemBuilder::new(None, ty, first_description);
    if !first_is_preformatted {
        preformatted.observe_line_outside_preformatted_block(first_line.text);
    }

    for line in lines {
        if preformatted.consume_preformatted_line(line.text) {
            item.push_description(line.text);
            continue;
        }
        if is_uppercase_section_like_header(line.text.trim()) {
            return None;
        }
        if has_type
            && indentation(line.text) == first_indent
            && split_return_type(line.text.trim()).is_some()
        {
            return None;
        }
        preformatted.observe_line_outside_preformatted_block(line.text);
        item.push_description(line.text);
    }

    Some(vec![item.finish(kind)])
}

fn split_return_type(line: &str) -> Option<(&str, &str)> {
    if PreformattedBlockScanner::line_starts_rest_literal_block(line) {
        return None;
    }

    let (ty, description) = split_field_colon(line)?;
    let ty = ty.trim();
    if is_uri_scheme_prefix(ty, description) {
        return None;
    }

    is_docstring_type_expression(ty).then_some((ty, description.trim()))
}

fn split_field_colon(line: &str) -> Option<(&str, &str)> {
    let mut start = 0;
    while start < line.len() {
        let (before_colon, after_colon) = split_once_unbracketed_colon(line.get(start..)?)?;
        let colon = start + before_colon.len();
        if let Some(role_end) = rst_role_markup_end(line, colon) {
            start = role_end;
            continue;
        }
        return Some((&line[..colon], after_colon));
    }
    None
}

fn rst_role_markup_end(line: &str, start: usize) -> Option<usize> {
    let rest = line.get(start..)?;
    let after_initial_colon = rest.strip_prefix(':')?;
    let role_end = after_initial_colon.find(":`")?;
    let role = &after_initial_colon[..role_end];
    if role.is_empty()
        || !role
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, ':' | '_' | '-' | '.'))
    {
        return None;
    }

    let content_start = start + ':'.len_utf8() + role_end + ":`".len();
    let content = line.get(content_start..)?;
    let closing_backtick = content.find('`')?;
    Some(content_start + closing_backtick + '`'.len_utf8())
}

fn is_uri_scheme_prefix(prefix: &str, description: &str) -> bool {
    let mut chars = prefix.chars();
    let is_scheme = chars.next().is_some_and(|char| char.is_ascii_alphabetic())
        && chars.all(|char| char.is_ascii_alphanumeric() || matches!(char, '+' | '-' | '.'));
    is_scheme
        && description.chars().next().is_some_and(|first| {
            !first.is_whitespace()
                && (matches!(first, '/' | '\\' | '?' | '#' | '@' | ':')
                    || matches_opaque_uri_scheme(prefix))
        })
}

fn matches_opaque_uri_scheme(scheme: &str) -> bool {
    // Keep this list narrow so type-and-description forms like `Path:foo` remain types.
    ["data", "geo", "mailto", "news", "sms", "tel", "urn"]
        .iter()
        .any(|known| scheme.eq_ignore_ascii_case(known))
}

fn is_uppercase_section_like_header(line: &str) -> bool {
    line.chars().next().is_some_and(char::is_uppercase) && google::is_section_like_header(line)
}

fn is_parameter_display_name(display_name: &str) -> bool {
    display_name
        .split(',')
        .all(|name| google::is_parameter_name(name.trim()))
}

fn is_attribute_display_name(display_name: &str) -> bool {
    display_name
        .split(',')
        .all(|name| google::is_dotted_identifier(name.trim()))
}
