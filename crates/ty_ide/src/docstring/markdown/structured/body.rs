use crate::docstring::document::preformatted::PreformattedBlockScanner;
use crate::docstring::document::syntax::{ParsedLine, indentation, strip_code_span_wrapper};

use super::{SectionItem, SectionKind};

pub(super) struct SectionItemBuilder<'a> {
    display_name: Option<&'a str>,
    ty: Option<&'a str>,
    inline_description: Option<&'a str>,
    continuation_lines: Vec<&'a str>,
}

impl<'a> SectionItemBuilder<'a> {
    pub(super) fn new(
        display_name: Option<&'a str>,
        ty: Option<&'a str>,
        inline_description: &'a str,
    ) -> Self {
        let inline_description = inline_description.trim();
        Self {
            display_name,
            ty: ty.map(strip_code_span_wrapper),
            inline_description: (!inline_description.is_empty()).then_some(inline_description),
            continuation_lines: Vec::new(),
        }
    }

    pub(super) fn finish(self, kind: SectionKind) -> SectionItem {
        let description = self.description_source();
        SectionItem::new(kind, self.display_name, self.ty, description)
    }

    pub(super) fn push_description(&mut self, line: &'a str) {
        self.continuation_lines.push(line);
    }

    fn description_source(&self) -> String {
        let continuation_indent = self
            .continuation_lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| indentation(line))
            .min()
            .unwrap_or_default();

        let mut lines = Vec::with_capacity(
            self.continuation_lines.len() + usize::from(self.inline_description.is_some()),
        );
        if let Some(inline_description) = &self.inline_description {
            lines.push((*inline_description).to_string());
        }
        lines.extend(self.continuation_lines.iter().map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                strip_indentation(line, continuation_indent)
                    .trim_end()
                    .to_string()
            }
        }));

        let Some(start) = lines.iter().position(|line| !line.is_empty()) else {
            return String::new();
        };
        let end = lines
            .iter()
            .rposition(|line| !line.is_empty())
            .map_or(start, |index| index + 1);
        lines[start..end].join("\n")
    }
}

pub(super) fn parse_named_items<'a>(
    kind: SectionKind,
    body: &[ParsedLine<'a>],
    parse_item: impl FnMut(&ParsedLine<'a>) -> Option<SectionItemBuilder<'a>>,
) -> Option<Vec<SectionItem>> {
    parse_named_items_impl(kind, None, body, parse_item)
}

/// Parses named items while accepting continuations aligned with a first-line section heading.
pub(super) fn parse_named_items_with_aligned_continuations<'a>(
    kind: SectionKind,
    header_indent: usize,
    body: &[ParsedLine<'a>],
    parse_item: impl FnMut(&ParsedLine<'a>) -> Option<SectionItemBuilder<'a>>,
) -> Option<Vec<SectionItem>> {
    parse_named_items_impl(kind, Some(header_indent), body, parse_item)
}

fn parse_named_items_impl<'a>(
    kind: SectionKind,
    aligned_continuation_indent: Option<usize>,
    body: &[ParsedLine<'a>],
    mut parse_item: impl FnMut(&ParsedLine<'a>) -> Option<SectionItemBuilder<'a>>,
) -> Option<Vec<SectionItem>> {
    let mut items = Vec::new();
    let mut current: Option<SectionItemBuilder<'a>> = None;
    let mut item_indent = None;
    let mut preformatted = PreformattedBlockScanner::default();

    for line in body {
        if let Some(current) = &mut current
            && preformatted.consume_preformatted_line(line.text)
        {
            if !line.text.trim().is_empty()
                && item_indent.is_some_and(|indent| indentation(line.text) < indent)
            {
                return None;
            }
            current.push_description(line.text);
            continue;
        }

        let trimmed = line.text.trim();
        if trimmed.is_empty() {
            if let Some(current) = &mut current {
                current.push_description("");
            }
            continue;
        }

        let line_indent = indentation(line.text);
        if item_indent.is_none_or(|indent| line_indent == indent) {
            if let Some(item) = parse_item(line) {
                if let Some(current) = current.replace(item) {
                    items.push(current.finish(kind));
                }
                item_indent.get_or_insert(line_indent);
                preformatted = PreformattedBlockScanner::default();
                preformatted.observe_line_outside_preformatted_block(line.text);
                continue;
            }
            // PEP 257 can align a first-line parameter section with its items. Only in that
            // layout is a same-indent non-item unambiguously a continuation.
            if item_indent.is_some()
                && !(item_indent == aligned_continuation_indent
                    && matches!(
                        kind,
                        SectionKind::Parameters
                            | SectionKind::KeywordArguments
                            | SectionKind::OtherParameters
                    ))
            {
                return None;
            }
        }
        if item_indent.is_some_and(|indent| line_indent < indent) {
            return None;
        }

        let current = current.as_mut()?;
        current.push_description(line.text);
        preformatted.observe_line_outside_preformatted_block(line.text);
    }

    if let Some(current) = current {
        items.push(current.finish(kind));
    }
    (!items.is_empty()).then_some(items)
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
