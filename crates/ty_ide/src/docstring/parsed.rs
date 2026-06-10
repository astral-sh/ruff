use std::borrow::Cow;
use std::ops::Range;

use ruff_text_size::TextSize;

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
        let blocks = parse_rest_blocks(raw, rest.field_lists());

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
        self.blocks
            .iter()
            .filter_map(Block::as_section)
            .flat_map(SectionBlock::parameter_documentation)
            .collect()
    }
}

fn parse_rest_blocks<'a>(raw: &'a str, field_lists: &[rest::FieldList]) -> Vec<Block<'a>> {
    let mut blocks = Vec::new();
    let mut rendered_through = 0;

    for field_list in field_lists {
        if field_list.indent() != TextSize::default() {
            continue;
        }

        let range = field_list.range();
        let start = range.start().to_usize();
        let end = range.end().to_usize();
        if start < rendered_through {
            continue;
        }

        let Some(section) = basic_rest_section_block(field_list) else {
            continue;
        };

        if !push_raw_block(&mut blocks, raw, rendered_through..start) {
            return Vec::new();
        }
        blocks.push(Block::Section(section));
        rendered_through = end;
    }

    if !blocks.is_empty() && !push_raw_block(&mut blocks, raw, rendered_through..raw.len()) {
        return Vec::new();
    }

    blocks
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

fn basic_rest_section_block(field_list: &rest::FieldList) -> Option<SectionBlock> {
    let plan = BasicRestFieldRenderPlan::from_fields(field_list.fields())?;
    let items = plan.items(field_list.fields());
    items
        .iter()
        .all(|item| !item.is_empty())
        .then(|| SectionBlock::new(items))
}

/// Validates a basic reST field list and stores cross-field metadata needed while rendering.
struct BasicRestFieldRenderPlan<'a> {
    return_type: Option<&'a str>,
    has_returns: bool,
}

impl<'a> BasicRestFieldRenderPlan<'a> {
    fn from_fields(fields: &'a [rest::Field]) -> Option<Self> {
        let mut has_rendered_field = false;
        let mut has_returns = false;
        let mut has_return_type = false;
        let mut return_type = None;

        for field in fields {
            match field {
                rest::Field::Parameter { .. } => {
                    has_rendered_field = true;
                }
                rest::Field::Returns { .. } => {
                    has_rendered_field = true;
                    has_returns = true;
                }
                rest::Field::ReturnType { ty } => {
                    if has_return_type {
                        return None;
                    }
                    has_return_type = true;
                    return_type = (!ty.is_empty()).then_some(ty.as_str());
                    has_rendered_field |= return_type.is_some();
                }
                rest::Field::ParameterType { .. }
                | rest::Field::Attribute { .. }
                | rest::Field::AttributeType { .. }
                | rest::Field::Raises { .. }
                | rest::Field::Metadata
                | rest::Field::Unknown { .. } => return None,
            }
        }

        has_rendered_field.then_some(Self {
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
                    ty.as_deref().map(str::to_string),
                    description.clone(),
                )),
                rest::Field::Returns { name, description } => items.push(SectionItem::new(
                    DocstringSectionKind::Returns,
                    name.as_ref().map(ToString::to_string),
                    None,
                    self.return_type.map(str::to_string),
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
                | rest::Field::Attribute { .. }
                | rest::Field::AttributeType { .. }
                | rest::Field::Raises { .. }
                | rest::Field::ReturnType { .. }
                | rest::Field::Metadata
                | rest::Field::Unknown { .. } => {}
            }
        }

        items
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
    fn basic_rest_field_lists_render_markdown_sections() {
        let docstring = "\
Summary.

:param str value: The value.
:param other: Another value.
:returns: Whether validation passed.
:rtype: bool
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_snapshot!(parsed.render_markdown_source(), @"
        Summary.

        ## Parameters
        `value` (`str`): The value.
        `other`: Another value.

        ## Returns
        `bool`: Whether validation passed.
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
    fn unsupported_rest_field_lists_stay_raw() {
        let docstring = "\
Summary.

:param value: The value.
:type value: int
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let docstring = "\
Summary.

:param value: The value.
:raises ValueError: Invalid value.
";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let docstring = "\
Summary.

:param value: The value.
:unknown field: Preserve this field list.
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
    }
}
