use indexmap::IndexMap;
use ruff_text_size::{TextRange, TextSize};
use strum_macros::EnumIter;

use self::syntax::{indentation, starts_with_markdown_list_item};

pub(super) mod google;
pub(super) mod preformatted;
pub(super) mod rst;
pub(in crate::docstring) mod syntax;

/// Returns docs for all parameters recognized in the given docstring.
///
/// `normalized_source` must have already undergone PEP-257 trimming and universal newline
/// normalization.
pub(super) fn parameter_documentation(
    normalized_source: &str,
    numpy_parameters: IndexMap<String, String>,
) -> IndexMap<String, String> {
    let mut parameters = google::parameter_documentation(normalized_source);
    parameters.extend(numpy_parameters);
    parameters.extend(rst::parameter_documentation(normalized_source));
    parameters
}

/// Canonical docstring sections shared by supported formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter)]
pub(in crate::docstring) enum SectionKind {
    /// Function or method parameters.
    Parameters,
    /// Keyword arguments documented separately from the main parameter section.
    KeywordArguments,
    /// Less commonly used parameters listed separately from the main parameter section.
    OtherParameters,
    /// Class or module attributes.
    Attributes,
    /// A returned value.
    Returns,
    /// A yielded value.
    Yields,
    /// Exceptions raised by a callable.
    Raises,
}

impl SectionKind {
    /// Returns the canonical display heading for this section.
    pub(super) const fn heading(self) -> &'static str {
        match self {
            SectionKind::Parameters => "Parameters",
            SectionKind::KeywordArguments => "Keyword Arguments",
            SectionKind::OtherParameters => "Other Parameters",
            SectionKind::Attributes => "Attributes",
            SectionKind::Returns => "Returns",
            SectionKind::Yields => "Yields",
            SectionKind::Raises => "Raises",
        }
    }
}

/// A recognized structured docstring section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::docstring) struct Section<Name> {
    kind: SectionKind,
    range: TextRange,
    body: SectionBody<Name>,
}

impl<Name> Section<Name> {
    /// Returns the section kind.
    pub(in crate::docstring) const fn kind(&self) -> SectionKind {
        self.kind
    }

    /// Returns the section's source range.
    pub(in crate::docstring) const fn range(&self) -> TextRange {
        self.range
    }

    /// Consumes this section and returns its fragments when it can be rendered structurally.
    pub(in crate::docstring) fn into_renderable_fragments(self) -> Option<Vec<BodyFragment<Name>>> {
        let SectionBody::Parsed {
            fragments,
            has_structural_ambiguity,
        } = self.body
        else {
            return None;
        };
        (!has_structural_ambiguity).then_some(fragments)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SectionBody<Name> {
    /// A body parsed into semantic fragments.
    Parsed {
        fragments: Vec<BodyFragment<Name>>,
        /// Whether the body's structure is ambiguous.
        has_structural_ambiguity: bool,
    },
    /// A body whose contents were not parsed.
    Opaque,
}

impl<Name> SectionBody<Name> {
    /// Creates an unambiguous body containing the description as a single prose fragment.
    fn from_prose(description: String) -> Self {
        let fragments = (!description.is_empty())
            .then_some(BodyFragment::Prose(description))
            .into_iter()
            .collect();
        Self::Parsed {
            fragments,
            has_structural_ambiguity: false,
        }
    }

    fn into_fragments(self) -> Vec<BodyFragment<Name>> {
        match self {
            Self::Parsed { fragments, .. } => fragments,
            Self::Opaque => Vec::new(),
        }
    }
}

/// One parsed fragment in a structured section body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::docstring) enum BodyFragment<Name> {
    /// Section-level prose that is not attached to an item.
    Prose(String),
    /// A named or anonymous section item.
    Item(Item<Name>),
}

/// An item in a structured docstring section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::docstring) struct Item<Name> {
    display_name: Name,
    ty: Option<String>,
    description: String,
}

impl<Name> Item<Name> {
    /// Consumes this item and returns its display parts.
    pub(in crate::docstring) fn into_display_name_type_and_description(
        self,
    ) -> (Name, Option<String>, String) {
        (self.display_name, self.ty, self.description)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderKind {
    Structured(SectionKind),
    Opaque,
}

impl HeaderKind {
    fn is_parameter_section(self) -> bool {
        matches!(
            self,
            Self::Structured(
                SectionKind::Parameters
                    | SectionKind::KeywordArguments
                    | SectionKind::OtherParameters
            )
        )
    }
}

#[derive(Default)]
struct DescriptionBuilder<'a> {
    inline: Option<&'a str>,
    continuation_lines: Vec<&'a str>,
}

impl<'a> DescriptionBuilder<'a> {
    fn with_inline(inline: &'a str) -> Self {
        let inline = inline.trim();
        Self {
            inline: (!inline.is_empty()).then_some(inline),
            continuation_lines: Vec::new(),
        }
    }

    fn push_line(&mut self, line: &'a str) {
        // Keep a leading list item with the block so that its indentation establishes the baseline
        // for nested items. Ordinary first lines use the allocation-free inline representation.
        if self.inline.is_none()
            && self.continuation_lines.is_empty()
            && !starts_with_markdown_list_item(line.trim_start())
        {
            self.inline = Some(line.trim());
        } else {
            self.push_continuation(line);
        }
    }

    fn push_continuation(&mut self, line: &'a str) {
        self.continuation_lines.push(line);
    }

    fn finish(mut self) -> String {
        if self.continuation_lines.is_empty() {
            return self.inline.map_or_else(String::new, str::to_string);
        }

        let continuation_indent = self
            .continuation_lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| indentation(line))
            .min()
            .unwrap_or_default();
        for line in &mut self.continuation_lines {
            *line = if line.trim().is_empty() {
                ""
            } else {
                strip_indentation(line, continuation_indent).trim_end()
            };
        }

        if let Some(inline) = self.inline {
            self.continuation_lines.insert(0, inline);
        }
        let lines = self.continuation_lines;

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

fn strip_indentation(line: &str, width: TextSize) -> &str {
    let mut indentation_width = TextSize::default();
    for (index, char) in line.char_indices() {
        let next_indentation_width = match char {
            ' ' => indentation_width + TextSize::new(1),
            '\t' => TextSize::new((indentation_width.to_u32() / 8 + 1) * 8),
            _ => return &line[index..],
        };

        if next_indentation_width > width {
            return &line[index..];
        }

        indentation_width = next_indentation_width;
        if indentation_width == width {
            return &line[index + char.len_utf8()..];
        }
    }

    ""
}
