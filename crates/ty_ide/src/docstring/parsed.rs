use std::borrow::Cow;
use std::ops::Range;

use ruff_text_size::TextSize;
use rustc_hash::{FxHashMap, FxHashSet};

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

        let Some(section) = rest_section_block(field_list) else {
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
    }
}
