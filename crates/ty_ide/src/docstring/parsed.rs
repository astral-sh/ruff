use std::borrow::Cow;

use super::sections::{DocstringItem, DocstringSectionKind, DocstringSections};

/// A tolerant, display-oriented parse of a normalized docstring.
pub(super) struct ParsedDocstring<'a> {
    raw: &'a str,
    blocks: Vec<Block<'a>>,
}

impl<'a> ParsedDocstring<'a> {
    pub(super) fn parse(raw: &'a str) -> Self {
        Self {
            raw,
            blocks: vec![],
        }
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
