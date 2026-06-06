use std::borrow::Cow;
use std::iter::{Enumerate, Peekable};

use compact_str::{CompactString, ToCompactString};
use ruff_python_trivia::leading_indentation;
use ruff_source_file::{Line as SourceLine, UniversalNewlineIterator, UniversalNewlines};
use ruff_text_size::{TextRange, TextSize};
use rustc_hash::FxHashMap;

use super::markdown;
use super::sections::{DocstringItem, DocstringSectionKind, DocstringSections};

/// Returns whether a normalized docstring may contain a top-level field list
/// that markdown rendering would rewrite.
pub(super) fn may_contain_top_level_field_list(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    memchr::memchr_iter(b':', bytes).any(|colon| {
        (colon == 0 || bytes[colon - 1] == b'\n')
            && starts_renderable_field_header(&raw[colon + 1..])
    })
}

fn starts_renderable_field_header(after_opening_colon: &str) -> bool {
    let line = after_opening_colon
        .split_once('\n')
        .map_or(after_opening_colon, |(line, _)| line);

    let Some(name_end) = line.find(|char: char| char == ':' || char.is_whitespace()) else {
        return false;
    };

    let name = &line[..name_end];
    matches!(
        name,
        "param"
            | "parameter"
            | "arg"
            | "argument"
            | "key"
            | "keyword"
            | "kwarg"
            | "kwparam"
            | "var"
            | "ivar"
            | "cvar"
            | "return"
            | "returns"
            | "raises"
            | "raise"
            | "except"
            | "exception"
    ) && line[name_end..].contains(':')
}

/// Represents a parsed restructured text (reST) docstring.
pub(super) struct Docstring<'a> {
    raw: &'a str,
    field_lists: Vec<FieldList>,
}

impl<'a> Docstring<'a> {
    /// Constructs a parsed representation from a raw docstring.
    pub(super) fn parse(raw: &'a str) -> Self {
        let field_lists = FieldList::parse_all(raw);
        Self { raw, field_lists }
    }

    /// Returns the parameter documentation that we were able to recognize in a docstring.
    pub(super) fn parameter_documentation(&self) -> Vec<ParameterDocumentation> {
        let mut parameters = Vec::new();

        for field_list in &self.field_lists {
            for field in &field_list.fields {
                let Field::Parameter {
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

                parameters.push(ParameterDocumentation {
                    name: lookup_name.clone(),
                    description: description.clone(),
                });
            }
        }

        parameters
    }

    /// Returns the original docstring when no supported field list is rendered.
    pub(super) fn render_markdown(&self) -> Cow<'a, str> {
        let mut output: Option<String> = None;
        let mut rendered_through = TextSize::default();

        for field_list in &self.field_lists {
            if field_list.indent != TextSize::default()
                || field_list.range.start() < rendered_through
            {
                continue;
            }

            let Some(markdown) = field_list.render_markdown() else {
                continue;
            };

            let output = output.get_or_insert_with(String::new);
            output.push_str(
                &self.raw[rendered_through.to_usize()..field_list.range.start().to_usize()],
            );
            output.push_str(&markdown);
            rendered_through = field_list.range.end();
        }

        let Some(mut output) = output else {
            return Cow::Borrowed(self.raw);
        };
        output.push_str(&self.raw[rendered_through.to_usize()..]);
        Cow::Owned(output)
    }
}

/// Cursor over docstring lines and their line numbers.
#[derive(Clone)]
struct Lines<'a> {
    inner: Peekable<Enumerate<UniversalNewlineIterator<'a>>>,
}

impl<'a> Lines<'a> {
    /// Constructs a line cursor from raw docstring text.
    fn new(raw: &'a str) -> Self {
        Self {
            inner: raw.universal_newlines().enumerate().peekable(),
        }
    }

    /// Returns the next line without advancing the cursor.
    fn peek(&mut self) -> Option<DocstringLine<'a>> {
        let (index, line) = self.inner.peek()?;
        Some(DocstringLine::new(*index, line))
    }

    /// Advances the cursor and returns the next line.
    fn next(&mut self) -> Option<DocstringLine<'a>> {
        let (index, line) = self.inner.next()?;
        Some(DocstringLine::new(index, &line))
    }
}

/// A docstring line with its source position.
#[derive(Debug, Clone, Copy)]
struct DocstringLine<'a> {
    index: usize,
    text: &'a str,
    start: TextSize,
    end: TextSize,
}

impl<'a> DocstringLine<'a> {
    fn new(index: usize, line: &SourceLine<'a>) -> Self {
        Self {
            index,
            text: line.as_str(),
            start: line.start(),
            end: line.end(),
        }
    }
}

/// Represents a contiguous list of reST fields.
///
/// <https://www.sphinx-doc.org/en/master/usage/restructuredtext/basics.html#field-lists>
#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldList {
    start_line: usize,
    end_line: usize,
    range: TextRange,
    indent: TextSize,
    fields: Vec<Field>,
}

impl FieldList {
    /// Parse all the field lists in the given lines of a docstring.
    fn parse_all(raw: &str) -> Vec<Self> {
        let mut field_lists = Vec::new();
        let mut preformatted_blocks = PreformattedBlockScanner::default();
        let mut lines = Lines::new(raw);

        while let Some(line) = lines.peek() {
            if preformatted_blocks.consume_preformatted_line(line.text) {
                lines.next();
                continue;
            }

            let Some(field_list) = Self::parse(&mut lines) else {
                preformatted_blocks.observe_non_field_line(line.text);
                lines.next();
                continue;
            };

            field_lists.push(field_list);
        }

        field_lists
    }

    /// Attempt to parse a single field list from the given lines of a docstring.
    fn parse(lines: &mut Lines<'_>) -> Option<Self> {
        let line = lines.peek()?;
        let start_line = line.index;
        let range_start = line.start;
        let header = FieldHeader::parse(line.text)?;
        lines.next();

        let field_list_indent = header.indent;
        let mut fields = Vec::new();
        let mut current = FieldBuilder::new(header);
        let mut end_line = start_line + 1;
        let mut range_end = line.end;

        while let Some(line) = lines.peek() {
            if line.text.trim().is_empty() {
                // Blank lines continue the field list only before another field or a continuation.

                if !Self::blank_line_continues_field_list(lines, field_list_indent) {
                    break;
                }

                current.lines.push(line.text);
                lines.next();
                end_line = line.index + 1;
                range_end = line.end;
                continue;
            }

            if let Some(header) = FieldHeader::at_indent(line.text, field_list_indent) {
                // Same-indent field header starts the next field in this list.

                let previous = std::mem::replace(&mut current, FieldBuilder::new(header));
                fields.push(previous.finish());
                lines.next();
                end_line = line.index + 1;
                range_end = line.end;
                continue;
            }

            if FieldHeader::indentation(line.text) <= field_list_indent {
                // Same- or less-indented content ends this field list.
                break;
            }

            // More-indented non-blank lines continue the current field body
            // (and hence also the current field list).
            current.lines.push(line.text);
            lines.next();
            end_line = line.index + 1;
            range_end = line.end;
        }

        // Finalize the last field.
        fields.push(current.finish());

        Some(Self {
            start_line,
            end_line,
            range: TextRange::new(range_start, range_end),
            indent: field_list_indent,
            fields,
        })
    }

    /// Returns whether a blank line keeps the current field list open.
    ///
    /// A blank line before an indented continuation stays in the current field list:
    ///
    /// ```rst
    /// :param x: First paragraph.
    ///
    ///     Second paragraph.
    /// :param y: Next parameter.
    /// ```
    ///
    /// A blank line before another same-indent field also stays in the current field list:
    ///
    /// ```rst
    /// :param x: First parameter.
    ///
    /// :param y: Second parameter.
    /// ```
    ///
    /// A blank line before same-indent prose ends the field list:
    ///
    /// ```rst
    /// :param x: First parameter.
    ///
    /// This is normal prose.
    /// ```
    fn blank_line_continues_field_list(lines: &Lines<'_>, indent: TextSize) -> bool {
        let mut next = lines.clone();
        while let Some(line) = next.peek()
            && line.text.trim().is_empty()
        {
            next.next();
        }

        let Some(non_blank_line) = next.peek() else {
            return false;
        };

        FieldHeader::indentation(non_blank_line.text) > indent
            || FieldHeader::at_indent(non_blank_line.text, indent).is_some()
    }

    fn render_markdown(&self) -> Option<String> {
        let plan = FieldListRenderPlan::from_fields(&self.fields)?;
        let markdown = plan.render(&self.fields);
        (!markdown.is_empty()).then_some(markdown)
    }
}

/// Validates a field list and stores cross-field metadata needed while rendering.
struct FieldListRenderPlan<'a> {
    parameter_types: FxHashMap<&'a str, &'a str>,
    attribute_types: FxHashMap<&'a str, &'a str>,
    return_type: Option<&'a str>,
}

impl<'a> FieldListRenderPlan<'a> {
    fn from_fields(fields: &'a [Field]) -> Option<Self> {
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
                Field::Parameter {
                    lookup_name, ty, ..
                } => {
                    has_rendered_field = true;
                    parameters
                        .entry(lookup_name.as_str())
                        .or_default()
                        .record_field(ty.is_some());
                }
                Field::Attribute { name, ty, .. } => {
                    has_rendered_field = true;
                    attributes
                        .entry(name.as_str())
                        .or_default()
                        .record_field(ty.is_some());
                }
                Field::Returns { .. } => {
                    has_rendered_field = true;
                    has_returns = true;
                }
                Field::Raises { .. } => {
                    has_rendered_field = true;
                }
                Field::ParameterType { lookup_name, ty } => {
                    if parameter_types
                        .insert(lookup_name.as_str(), ty.as_str())
                        .is_some()
                    {
                        return None;
                    }
                }
                Field::AttributeType { name, ty } => {
                    if attribute_types.insert(name.as_str(), ty.as_str()).is_some() {
                        return None;
                    }
                }
                Field::ReturnType { ty } => {
                    if has_return_type {
                        return None;
                    }
                    has_return_type = true;
                    return_type = (!ty.is_empty()).then_some(ty.as_str());
                }
                Field::Metadata => {}
                Field::Unknown { .. } => return None,
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

        if !has_rendered_field || (has_return_type && !has_returns) {
            return None;
        }

        Some(Self {
            parameter_types,
            attribute_types,
            return_type,
        })
    }

    fn render(&self, fields: &'a [Field]) -> String {
        let mut sections = DocstringSections::default();
        for field in fields {
            match field {
                Field::Parameter {
                    display_name,
                    lookup_name,
                    ty,
                    description,
                } => sections.push(
                    DocstringSectionKind::Parameters,
                    DocstringItem::new(
                        Some(display_name.as_str()),
                        ty.as_deref().or_else(|| {
                            self.parameter_types
                                .get(lookup_name.as_str())
                                .copied()
                                .filter(|ty| !ty.is_empty())
                        }),
                        description.as_str(),
                    ),
                ),
                Field::Attribute {
                    name,
                    ty,
                    description,
                } => sections.push(
                    DocstringSectionKind::Attributes,
                    DocstringItem::new(
                        Some(name.as_str()),
                        ty.as_deref().or_else(|| {
                            self.attribute_types
                                .get(name.as_str())
                                .copied()
                                .filter(|ty| !ty.is_empty())
                        }),
                        description.as_str(),
                    ),
                ),
                Field::Returns { name, description } => sections.push(
                    DocstringSectionKind::Returns,
                    DocstringItem::new(name.as_deref(), self.return_type, description.as_str()),
                ),
                Field::Raises {
                    exception,
                    description,
                } => sections.push(
                    DocstringSectionKind::Raises,
                    DocstringItem::new(exception.as_deref(), None, description.as_str()),
                ),
                Field::ParameterType { .. }
                | Field::AttributeType { .. }
                | Field::ReturnType { .. }
                | Field::Metadata
                | Field::Unknown { .. } => {}
            }
        }

        sections.render_markdown()
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

/// Recognizes preformatted blocks that may occur within a docstring (e.g. a markdown fence).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct PreformattedBlockScanner<'a> {
    active_markdown_fence: Option<markdown::MarkdownFence<'a>>,
    active_doctest: bool,
    preformatted_block_state: PreformattedBlockState,
}

/// The set of characters that can each be used to denote a block quote.
///
/// <https://docutils.sourceforge.io/docs/ref/rst/restructuredtext.html#quoted-literal-blocks>
const QUOTED_LITERAL_BLOCK_QUOTE_CHARACTERS: &str = r##"!"#$%&'()*+,-./:;<=>?@[\]^_`{|}~"##;

impl<'a> PreformattedBlockScanner<'a> {
    /// Updates internal state to reflect the given line and returns whether or
    /// not the given line is contained within a preformatted block.
    fn consume_preformatted_line(&mut self, line: &'a str) -> bool {
        if let Some(fence) = self.active_markdown_fence {
            if fence.is_closed_by(line) {
                self.active_markdown_fence = None;
            }
            return true;
        }

        if self.is_within_preformatted_block(line) {
            return true;
        }

        let trimmed = line.trim_start_matches(' ');
        if self.active_doctest {
            if trimmed.is_empty() {
                self.active_doctest = false;
            }
            return true;
        }

        if trimmed.starts_with(">>>") {
            self.active_doctest = true;
            return true;
        }

        if let Some(fence) = markdown::MarkdownFence::find(line) {
            self.active_markdown_fence = Some(fence);
            return true;
        }

        false
    }

    /// Whether or not the given line is specifically within a preformatted block
    /// introduced by reST syntax.
    fn is_within_preformatted_block(&mut self, line: &str) -> bool {
        let current_indent = FieldHeader::indentation(line);
        let line_is_empty = line.trim_start().is_empty();

        match self.preformatted_block_state {
            PreformattedBlockState::Active(PreformattedBlockKind::Indented { marker_indent }) => {
                if !line_is_empty && current_indent <= marker_indent {
                    // We've reached the de-dent that marks the end of the preformatted block.
                    self.preformatted_block_state = PreformattedBlockState::Inactive;
                    false
                } else {
                    true
                }
            }
            PreformattedBlockState::Active(PreformattedBlockKind::QuotedLiteral {
                indent,
                quote,
            }) => {
                if line_is_empty {
                    self.preformatted_block_state = PreformattedBlockState::Inactive;
                    false
                } else if Self::quote_character(line, indent) == Some(quote) {
                    true
                } else {
                    self.preformatted_block_state = PreformattedBlockState::Inactive;
                    false
                }
            }
            PreformattedBlockState::Pending {
                marker_indent,
                allows_quoted_literal_block,
            } if !line_is_empty => {
                if current_indent > marker_indent {
                    // We just entered a new preformatted block.
                    self.preformatted_block_state =
                        PreformattedBlockState::Active(PreformattedBlockKind::Indented {
                            marker_indent,
                        });
                    true
                } else if allows_quoted_literal_block
                    && let Some(quote) = Self::quote_character(line, marker_indent)
                {
                    self.preformatted_block_state =
                        PreformattedBlockState::Active(PreformattedBlockKind::QuotedLiteral {
                            indent: marker_indent,
                            quote,
                        });
                    true
                } else {
                    self.preformatted_block_state = PreformattedBlockState::Inactive;
                    false
                }
            }
            PreformattedBlockState::Pending { .. } | PreformattedBlockState::Inactive => false,
        }
    }

    /// Updates internal state that allows us to detect preformatted blocks introduced by reST
    /// syntax.
    fn observe_non_field_line(&mut self, line: &str) {
        if matches!(
            self.preformatted_block_state,
            PreformattedBlockState::Inactive
        ) && Self::starts_preformatted_block(line.trim_start())
        {
            self.preformatted_block_state = PreformattedBlockState::Pending {
                marker_indent: FieldHeader::indentation(line),
                allows_quoted_literal_block: Self::allows_quoted_literal_block(line.trim_start()),
            };
        }
    }

    /// Whether or not the given line marks the start of a preformatted block.
    fn starts_preformatted_block(line: &str) -> bool {
        let Some(marker) = Self::preformatted_block_marker(line) else {
            return false;
        };

        !matches!(
            marker,
            PreformattedBlockMarker::Directive(
                "attention"
                    | "caution"
                    | "danger"
                    | "error"
                    | "hint"
                    | "important"
                    | "note"
                    | "tip"
                    | "warning"
                    | "admonition"
                    | "seealso"
                    | "versionadded"
                    | "version-added"
                    | "versionchanged"
                    | "version-changed"
                    | "version-deprecated"
                    | "deprecated"
                    | "version-removed"
                    | "versionremoved"
            )
        )
    }

    /// Tries to identify a marker that introduces a preformatted block.
    fn preformatted_block_marker(line: &str) -> Option<PreformattedBlockMarker<'_>> {
        let marker = if let Some(marker) = line.strip_suffix("::") {
            marker
        } else {
            let (before_language, _language) = line.rsplit_once(' ')?;
            before_language.trim_end().strip_suffix("::")?
        };

        if let Some(directive) = marker.strip_prefix(".. ") {
            Some(PreformattedBlockMarker::Directive(directive))
        } else {
            Some(PreformattedBlockMarker::Paragraph)
        }
    }

    /// Whether or not a particular preformatted block can contain an unindented quoted literal block.
    fn allows_quoted_literal_block(line: &str) -> bool {
        line.ends_with("::")
            && matches!(
                Self::preformatted_block_marker(line),
                Some(PreformattedBlockMarker::Paragraph)
            )
    }

    /// Returns the quote character for a quoted literal block line.
    fn quote_character(line: &str, indent: TextSize) -> Option<char> {
        if FieldHeader::indentation(line) != indent {
            return None;
        }

        let quote = line.get(indent.to_usize()..)?.chars().next()?;
        QUOTED_LITERAL_BLOCK_QUOTE_CHARACTERS
            .contains(quote)
            .then_some(quote)
    }
}

/// Identifies the syntax that introduced a potential preformatted block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreformattedBlockMarker<'a> {
    Paragraph,
    Directive(&'a str),
}

/// Tracks the state of a preformatted block introduced by reST syntax.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum PreformattedBlockState {
    #[default]
    Inactive,
    Pending {
        marker_indent: TextSize,
        allows_quoted_literal_block: bool,
    },
    Active(PreformattedBlockKind),
}

/// Tracks the type of an active preformatted block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreformattedBlockKind {
    Indented { marker_indent: TextSize },
    QuotedLiteral { indent: TextSize, quote: char },
}

/// Constructs new instances of the model for a reST field.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldBuilder<'a> {
    indent: TextSize,
    kind: FieldKind<'a>,
    body: &'a str,
    lines: Vec<&'a str>,
}

impl<'a> FieldBuilder<'a> {
    /// Initializes a builder object for a new field instance.
    fn new(header: FieldHeader<'a>) -> Self {
        Self {
            indent: header.indent,
            kind: header.kind,
            body: header.body,
            lines: vec![header.raw],
        }
    }

    /// Emits the field that was constructed with this builder.
    fn finish(self) -> Field {
        let body = self.normalized_body();

        match self.kind {
            FieldKind::Parameter {
                display_name,
                lookup_name,
                ty,
            } => Field::Parameter {
                display_name: display_name.to_compact_string(),
                lookup_name: lookup_name.to_compact_string(),
                ty: ty.map(|ty| ty.to_compact_string()),
                description: body,
            },
            FieldKind::ParameterType { lookup_name } => Field::ParameterType {
                lookup_name: lookup_name.to_compact_string(),
                ty: body,
            },
            FieldKind::Attribute { name, ty } => Field::Attribute {
                name: name.to_compact_string(),
                ty: ty.map(|ty| ty.to_compact_string()),
                description: body,
            },
            FieldKind::AttributeType { name } => Field::AttributeType {
                name: name.to_compact_string(),
                ty: body,
            },
            FieldKind::Returns { name } => Field::Returns {
                name: name.map(|name| name.to_compact_string()),
                description: body,
            },
            FieldKind::ReturnType => Field::ReturnType { ty: body },
            FieldKind::Raises { exception } => Field::Raises {
                exception: exception.map(|exception| exception.to_compact_string()),
                description: body,
            },
            FieldKind::Metadata => Field::Metadata,
            FieldKind::Unknown { name, argument } => Field::Unknown {
                name: name.to_compact_string(),
                argument: argument.to_compact_string(),
                body,
            },
        }
    }

    /// Normalizes the text of the body of a field (e.g., the documentation for a parameter).
    fn normalized_body(&self) -> String {
        // Skip the field header line.
        let continuation_lines = self.lines.iter().skip(1);

        // Use the smallest indentation from all non-blank continuation lines as the normalized
        // continuation indent.
        let continuation_indent = continuation_lines
            .clone()
            .filter(|line| !line.trim().is_empty())
            .map(|line| FieldHeader::indentation(line))
            .min()
            .unwrap_or_default();

        let mut lines = Vec::with_capacity(self.lines.len());

        // Begin with the inline body text parsed from the field header line.
        lines.push(self.body.trim_end().to_string());

        // Then normalize and add all continuation lines.
        lines.extend(continuation_lines.map(|line| {
            if line.trim().is_empty() {
                // Any pure whitespace line becomes an empty line.
                String::new()
            } else {
                // For any other line we strip the shared continuation indent and trailing whitespace.
                line.get(continuation_indent.to_usize()..)
                    .unwrap_or_default()
                    .trim_end()
                    .to_string()
            }
        }));

        // Find non-empty start and end lines.
        let Some(start) = lines.iter().position(|line| !line.is_empty()) else {
            return String::new();
        };
        let end = lines
            .iter()
            .rposition(|line| !line.is_empty())
            .map_or(start, |index| index + 1);

        // Trim empty lines from either end of the result.
        lines[start..end].join("\n")
    }
}

/// Represents a parsed reST field header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FieldHeader<'a> {
    indent: TextSize,
    kind: FieldKind<'a>,
    body: &'a str,
    raw: &'a str,
}

impl<'a> FieldHeader<'a> {
    /// Finds the start of a reST field (if any) on the given line and at the
    /// given indentation level.
    fn at_indent(line: &'a str, indent: TextSize) -> Option<Self> {
        (Self::indentation(line) == indent)
            .then(|| Self::parse(line))
            .flatten()
    }

    /// Parses a reST field header of the form `:name [argument]: [body]`.
    ///
    /// The argument may consist of multiple, whitespace-delimited tokens, and both the argument
    /// and the body are optional, so all of the following are accepted:
    ///
    /// ```rst
    /// :meta:
    /// :param count:
    /// :param int count:
    /// :param int count: Number of items.
    /// ```
    ///
    /// Leading indentation is allowed and recorded, so this is also accepted:
    ///
    /// ```rst
    ///     :param int count: Number of items.
    /// ```
    ///
    /// Lines without a field name or without whitespace before a non-empty body are rejected:
    ///
    /// ```rst
    /// ::
    /// :param name:Description.
    /// ```
    fn parse(line: &'a str) -> Option<Self> {
        let trimmed = line.trim_start();
        let after_opening_colon = trimmed.strip_prefix(':')?;
        let (name_and_argument, body) = after_opening_colon.split_once(':')?;
        if body
            .chars()
            .next()
            .is_some_and(|char| !char.is_whitespace())
        {
            return None;
        }

        let name_and_argument = name_and_argument.trim();
        if name_and_argument.is_empty() {
            return None;
        }

        let name_end = name_and_argument
            .find(char::is_whitespace)
            .unwrap_or(name_and_argument.len());
        let name = &name_and_argument[..name_end];
        let argument = name_and_argument[name_end..].trim();

        Some(Self {
            indent: Self::indentation(line),
            kind: FieldKind::parse(name, argument),
            body: body.trim_start(),
            raw: line,
        })
    }

    /// Returns the leading indentation of the given source line.
    fn indentation(line: &str) -> TextSize {
        TextSize::of(leading_indentation(line))
    }
}

/// Categorizes the type of a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind<'a> {
    Parameter {
        display_name: &'a str,
        lookup_name: &'a str,
        ty: Option<&'a str>,
    },
    ParameterType {
        lookup_name: &'a str,
    },
    Attribute {
        name: &'a str,
        ty: Option<&'a str>,
    },
    AttributeType {
        name: &'a str,
    },
    Returns {
        name: Option<&'a str>,
    },
    ReturnType,
    Raises {
        exception: Option<&'a str>,
    },
    Metadata,
    Unknown {
        name: &'a str,
        argument: &'a str,
    },
}

impl<'a> FieldKind<'a> {
    /// Categorizes a parsed field as a supported field or an unknown field.
    fn parse(name: &'a str, argument: &'a str) -> Self {
        match name {
            "param" | "parameter" | "arg" | "argument" | "key" | "keyword" | "kwarg"
            | "kwparam" => Self::parse_parameter_argument(argument)
                .map(|(ty, name)| Self::Parameter {
                    display_name: name.display,
                    lookup_name: name.lookup,
                    ty,
                })
                .unwrap_or(Self::Unknown { name, argument }),
            "type" | "paramtype" => Self::parse_parameter_name(argument)
                .map(|name| Self::ParameterType {
                    lookup_name: name.lookup,
                })
                .unwrap_or(Self::Unknown { name, argument }),
            "var" | "ivar" | "cvar" => Self::parse_attribute_argument(argument)
                .map(|(ty, attribute_name)| Self::Attribute {
                    name: attribute_name,
                    ty,
                })
                .unwrap_or(Self::Unknown { name, argument }),
            "vartype" => Self::parse_attribute_name(argument)
                .map(|attribute_name| Self::AttributeType {
                    name: attribute_name,
                })
                .unwrap_or(Self::Unknown { name, argument }),
            "return" | "returns" => Self::Returns {
                name: Self::parse_parameter_name(argument).map(|name| name.lookup),
            },
            "rtype" => Self::ReturnType,
            "raises" | "raise" | "except" | "exception" => {
                let exception = argument.trim();
                Self::Raises {
                    exception: (!exception.is_empty()).then_some(exception),
                }
            }
            "meta" => Self::Metadata,
            _ => Self::Unknown { name, argument },
        }
    }

    /// Parses a parameter name and an optional parameter type from a raw field argument.
    /// Returns None if we fail to parse the argument.
    fn parse_parameter_argument(argument: &'a str) -> Option<(Option<&'a str>, ParameterName<'a>)> {
        let argument = argument.trim();
        if argument.is_empty() {
            return None;
        }

        let (ty, name) = Self::split_type_and_name(argument);
        Some((ty, Self::parse_parameter_name(name)?))
    }

    /// Splits up a field argument into an optional type and name.
    fn split_type_and_name(argument: &'a str) -> (Option<&'a str>, &'a str) {
        for (index, char) in argument.char_indices().rev() {
            if char.is_whitespace() {
                let ty = argument[..index].trim();
                let name = &argument[index + char.len_utf8()..];
                return ((!ty.is_empty()).then_some(ty), name);
            }
        }

        (None, argument)
    }

    fn parse_attribute_argument(argument: &'a str) -> Option<(Option<&'a str>, &'a str)> {
        let argument = argument.trim();
        if argument.is_empty() {
            return None;
        }

        let (ty, name) = Self::split_type_and_name(argument);
        Some((ty, Self::parse_attribute_name(name)?))
    }

    fn parse_attribute_name(name: &'a str) -> Option<&'a str> {
        let name = name.trim();
        (!name.is_empty()).then_some(name)
    }

    /// Normalizes a parameter name into display and lookup identifiers.
    fn parse_parameter_name(name: &'a str) -> Option<ParameterName<'a>> {
        let display = name.trim();
        let lookup = display.trim_start_matches('*');
        (!lookup.is_empty()).then_some(ParameterName { display, lookup })
    }
}

/// Represents the reST fields captured by the parser.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Field {
    Parameter {
        display_name: CompactString,
        lookup_name: CompactString,
        ty: Option<CompactString>,
        description: String,
    },
    ParameterType {
        lookup_name: CompactString,
        ty: String,
    },
    Attribute {
        name: CompactString,
        ty: Option<CompactString>,
        description: String,
    },
    AttributeType {
        name: CompactString,
        ty: String,
    },
    Returns {
        name: Option<CompactString>,
        description: String,
    },
    ReturnType {
        ty: String,
    },
    Raises {
        exception: Option<CompactString>,
        description: String,
    },
    Metadata,
    Unknown {
        name: CompactString,
        argument: CompactString,
        body: String,
    },
}

/// Parameter documentation extracted from a reST field list.
pub(super) struct ParameterDocumentation {
    pub(super) name: CompactString,
    pub(super) description: String,
}

/// Container for the display name (shown to the user) and the lookup name
/// (used to look up semantic information) for a particular parameter.
///
/// For instance, typical variadic positional parameters will have a `display`
/// of "*args" and `lookup` of "args".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParameterName<'a> {
    display: &'a str,
    lookup: &'a str,
}

#[cfg(test)]
mod tests {
    use insta::{assert_debug_snapshot, assert_snapshot};

    use super::{Docstring, may_contain_top_level_field_list};

    #[test]
    fn parameter_documentation_extracts_rest_parameters() {
        let docstring = r#"
        This is a function description.

        :param str param1: The first parameter description
        :param int param2: The second parameter description
            This is a continuation of param2 description.
        :param **kwargs: Extra keyword arguments.
        :returns: The return value description
        "#;
        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @r"
        param1: The first parameter description
        param2: The second parameter description
          This is a continuation of param2 description.
        kwargs: Extra keyword arguments.
        ");
    }

    #[test]
    fn field_list_precheck_detects_renderable_top_level_fields() {
        assert!(may_contain_top_level_field_list(
            "Summary.\n:param value: The value."
        ));
        assert!(may_contain_top_level_field_list(
            ":type value: int\n:param value: The value."
        ));
        assert!(!may_contain_top_level_field_list(
            "Summary: no field list here."
        ));
        assert!(!may_contain_top_level_field_list(
            ":class:`Foo` instances are accepted.\n:meta private:"
        ));
        assert!(!may_contain_top_level_field_list(
            "    :param value: Nested field lists are preserved."
        ));
        assert!(!may_contain_top_level_field_list(":rtype: str"));
    }

    #[test]
    fn parameter_documentation_supports_parameter_aliases() {
        let docstring = r#"
        :parameter first: The first parameter.
        :arg second: The second parameter.
        :argument third: The third parameter.
        :key fourth: The fourth parameter.
        :keyword fifth: The fifth parameter.
        :kwarg sixth: The sixth parameter.
        :kwparam seventh: The seventh parameter.
        "#;
        let param_docs = parameter_documentation(docstring);
        assert_snapshot!(param_docs, @r"
        first: The first parameter.
        second: The second parameter.
        third: The third parameter.
        fourth: The fourth parameter.
        fifth: The fifth parameter.
        sixth: The sixth parameter.
        seventh: The seventh parameter.
        ");
    }

    #[test]
    fn parser_supports_complex_inline_parameter_types() {
        let parsed = Docstring::parse(
            "\
:param list[str] items: Item descriptions.
:param dict[str, list[int | None]] mapping: Mapping description.
:param Callable[[int, str], bool] callback: Callback description.",
        );

        assert_debug_snapshot!(&parsed.field_lists[0].fields, @r#"
        [
            Parameter {
                display_name: "items",
                lookup_name: "items",
                ty: Some(
                    "list[str]",
                ),
                description: "Item descriptions.",
            },
            Parameter {
                display_name: "mapping",
                lookup_name: "mapping",
                ty: Some(
                    "dict[str, list[int | None]]",
                ),
                description: "Mapping description.",
            },
            Parameter {
                display_name: "callback",
                lookup_name: "callback",
                ty: Some(
                    "Callable[[int, str], bool]",
                ),
                description: "Callback description.",
            },
        ]
        "#);
    }

    #[test]
    fn parameter_documentation_stops_at_field_boundaries() {
        let docstring = r#"
        :param param: The parameter description
        :type param: bool
        :returns value: The return value description
        :rtype: str
        "#;
        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @"param: The parameter description");
    }

    #[test]
    fn parameter_documentation_ignores_parameters_without_names_after_normalization() {
        assert_snapshot!(
            parameter_documentation(":param **: Missing a parameter name."),
            @""
        );
    }

    #[test]
    fn parser_preserves_supported_and_unknown_fields() {
        let docstring = "\
:param tuple[str, ...] *args: Extra positional arguments.
:type args: tuple[str, ...]
:var dict[str, int] cache: Cached values.
:vartype cache: dict[str, int]
:returns result: Return description.
:rtype: str
:raises ValueError: Error description.
:meta private:
:unknown with argument: Unknown description.";
        let parsed = Docstring::parse(docstring);

        assert_eq!(parsed.field_lists[0].start_line, 0);
        assert_eq!(parsed.field_lists[0].end_line, 9);
        assert_eq!(
            &docstring[parsed.field_lists[0].range.start().to_usize()
                ..parsed.field_lists[0].range.end().to_usize()],
            docstring
        );
        assert_debug_snapshot!(&parsed.field_lists[0].fields, @r#"
        [
            Parameter {
                display_name: "*args",
                lookup_name: "args",
                ty: Some(
                    "tuple[str, ...]",
                ),
                description: "Extra positional arguments.",
            },
            ParameterType {
                lookup_name: "args",
                ty: "tuple[str, ...]",
            },
            Attribute {
                name: "cache",
                ty: Some(
                    "dict[str, int]",
                ),
                description: "Cached values.",
            },
            AttributeType {
                name: "cache",
                ty: "dict[str, int]",
            },
            Returns {
                name: Some(
                    "result",
                ),
                description: "Return description.",
            },
            ReturnType {
                ty: "str",
            },
            Raises {
                exception: Some(
                    "ValueError",
                ),
                description: "Error description.",
            },
            Metadata,
            Unknown {
                name: "unknown",
                argument: "with argument",
                body: "Unknown description.",
            },
        ]
        "#);
    }

    #[test]
    fn parser_records_field_list_ranges() {
        let docstring = "\
Intro paragraph.

:param first: First parameter.

Intervening prose.

:param second: Second parameter.
    Continued.
";
        let parsed = Docstring::parse(docstring);

        assert_eq!(parsed.field_lists.len(), 2);

        let first = &parsed.field_lists[0];
        assert_eq!(first.start_line, 2);
        assert_eq!(first.end_line, 3);
        assert_eq!(
            docstring[first.range.start().to_usize()..first.range.end().to_usize()]
                .trim_end_matches('\n'),
            ":param first: First parameter."
        );

        let second = &parsed.field_lists[1];
        assert_eq!(second.start_line, 6);
        assert_eq!(second.end_line, 8);
        assert_eq!(
            docstring[second.range.start().to_usize()..second.range.end().to_usize()]
                .trim_end_matches('\n'),
            ":param second: Second parameter.\n    Continued."
        );
    }

    #[test]
    fn parser_recovers_from_partial_and_malformed_fields() {
        let param_docs = parameter_documentation(
            "\
:param first: Parsed before malformed input.
:param missing-space:This is malformed because body text must be separated by whitespace.
:param:
:param **: Invalid after parameter-name normalization.
:param empty:
:param list[str] second: Parsed after malformed and partial fields.
:param
:param third: Parsed after an incomplete field marker.",
        );

        assert_snapshot!(param_docs, @r"
        first: Parsed before malformed input.
        second: Parsed after malformed and partial fields.
        third: Parsed after an incomplete field marker.
        ");
    }

    #[test]
    fn parameter_documentation_supports_continuation_only_descriptions() {
        let param_docs = parameter_documentation(
            "\
:param value:
  First paragraph.

  Second paragraph.
:param other: Other parameter.",
        );

        assert_snapshot!(param_docs, @r"
        value: First paragraph.

          Second paragraph.
        other: Other parameter.
        ");
    }

    #[test]
    fn parser_treats_indented_field_like_text_as_continuation() {
        let param_docs = parameter_documentation(
            "\
:param first: First line.
    :param fake: This is continuation text, not a new field.
:param second: Real second parameter.",
        );

        assert_snapshot!(param_docs, @r"
        first: First line.
          :param fake: This is continuation text, not a new field.
        second: Real second parameter.
        ");
    }

    #[test]
    fn literal_blocks_take_precedence_over_markdown_fences_in_preformatted_blocks() {
        let docstring = "\
Literal block::

    ```python
    :param fake: This is sample input.
    ```

:param real: Real parameter.";

        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @"real: Real parameter.");
    }

    #[test]
    fn literal_blocks_use_marker_indentation_as_exit_threshold() {
        let docstring = "\
Literal block::

        sample
    :param fake: This is sample input.

:param real: Real parameter.";

        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @"real: Real parameter.");
    }

    #[test]
    fn quoted_literal_blocks_are_preformatted_blocks() {
        let docstring = "\
Literal block::

:param fake: This is sample input.
:param also_fake: This is more sample input.

:param real: Real parameter.";

        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @"real: Real parameter.");
    }

    #[test]
    fn parameter_documentation_recovers_after_same_indent_one_line_directive() {
        let docstring = "\
.. seealso:: other
:param value: Value parameter.

Section::

    :param fake: This is sample input.

:param next: Next parameter.";

        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @r"
        value: Value parameter.
        next: Next parameter.
        ");
    }

    #[test]
    fn doctests_take_precedence_over_markdown_fences_in_preformatted_blocks() {
        let docstring = "\
>>> print(\"field list\")
```
:param fake: This is doctest output.

:param real: Real parameter.";

        let param_docs = parameter_documentation(docstring);

        assert_snapshot!(param_docs, @"real: Real parameter.");
    }

    #[test]
    fn field_lists_render_supported_sections() {
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

        assert_snapshot!(Docstring::parse(docstring).render_markdown(), @"
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
    fn field_lists_ignore_metadata_fields_when_rendering() {
        let docstring = "\
:meta private:
:param value: The value to validate.
:meta public:";

        assert_snapshot!(Docstring::parse(docstring).render_markdown(), @"
        ## Parameters
        `value`: The value to validate.
        ");

        let metadata_only = "\
:meta private:
:meta public:";
        assert_eq!(
            Docstring::parse(metadata_only).render_markdown(),
            metadata_only
        );
    }

    #[test]
    fn field_lists_preserve_unrenderable_lists() {
        let docstring = "\
:param first: First parameter
:meta private:
:param second: Second parameter
:type orphan: str
:param **: Missing a parameter name.";

        assert_eq!(Docstring::parse(docstring).render_markdown(), docstring);

        assert_snapshot!(parameter_documentation(docstring), @"
        first: First parameter
        second: Second parameter
        ");

        for docstring in [
            "\
:param value: The value to validate.
:rtype: str",
            "\
:param value: The value to validate.
:type value: str
:type value: int",
            "\
:param str value: The value to validate.
:type value: int",
            "\
:var value: The value.
:vartype orphan: str",
            "\
:var value: The value.
:vartype value: str
:vartype value: int",
            "\
:var str value: The value.
:vartype value: int",
            "\
:returns:",
            "\
:raises:",
            "\
:meta private:
:returns:
:raises:",
        ] {
            assert_eq!(Docstring::parse(docstring).render_markdown(), docstring);
        }
    }

    #[test]
    fn field_lists_skip_empty_rendered_sections() {
        let docstring = "\
:param value: The value to validate.
:returns:
:raises:";

        assert_snapshot!(Docstring::parse(docstring).render_markdown(), @"
        ## Parameters
        `value`: The value to validate.
        ");
    }

    #[test]
    fn field_lists_render_return_type_without_name() {
        let docstring = "\
:returns: The return value description.
:rtype: dict[str,
    int]";

        assert_snapshot!(Docstring::parse(docstring).render_markdown(), @"
        ## Returns
        `dict[str, int]`: The return value description.
        ");
    }

    #[test]
    fn field_lists_render_sections_in_canonical_order() {
        let docstring = "\
:raises ValueError: If validation fails.
:param value: The value to validate.
:returns: The normalized value.
:raises TypeError: If validation has the wrong type.";

        assert_snapshot!(Docstring::parse(docstring).render_markdown(), @"
        ## Parameters
        `value`: The value to validate.

        ## Returns
        The normalized value.

        ## Raises
        `ValueError`: If validation fails.
        `TypeError`: If validation has the wrong type.
        ");
    }

    #[test]
    fn field_lists_render_list_descriptions_as_blocks() {
        let docstring = "\
:param value:
    - First option.
    - Second option.
:param steps:
    1. Validate the input.
    2. Return the result.
:param done: Whether work is done.";

        assert_snapshot!(Docstring::parse(docstring).render_markdown(), @"
        ## Parameters
        `value`:
        - First option.
        - Second option.

        `steps`:
        1. Validate the input.
        2. Return the result.

        `done`: Whether work is done.
        ");
    }

    #[test]
    fn field_lists_render_well_formed_lists_after_unrenderable_lists() {
        let docstring = "\
:param first: First parameter.

Some prose between field lists.

:meta private:

More prose between field lists.

:param second: Second parameter.";

        assert_snapshot!(Docstring::parse(docstring).render_markdown(), @"
        ## Parameters
        `first`: First parameter.

        Some prose between field lists.

        :meta private:

        More prose between field lists.

        ## Parameters
        `second`: Second parameter.
        ");
    }

    #[test]
    fn field_lists_inside_code_examples_are_preserved() {
        let docstring = "\
Markdown input:

```text
:param sample: This is sample input
```

Doctest output:

>>> print(\"field list\")
:param sample: This is sample output

Literal block::

    :param sample: This is sample input

:param real: Real parameter";

        assert_snapshot!(Docstring::parse(docstring).render_markdown(), @"
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
        `real`: Real parameter
        ");

        assert_snapshot!(parameter_documentation(docstring), @"real: Real parameter");
    }

    fn parameter_documentation(docstring: &str) -> String {
        let parameters = Docstring::parse(docstring).parameter_documentation();
        let mut rendered = String::new();

        for parameter in parameters {
            if !rendered.is_empty() {
                rendered.push('\n');
            }

            rendered.push_str(parameter.name.as_str());
            rendered.push_str(": ");

            let mut lines = parameter.description.lines();
            let Some(first_line) = lines.next() else {
                continue;
            };
            rendered.push_str(first_line);

            for line in lines {
                rendered.push('\n');
                if !line.is_empty() {
                    rendered.push_str("  ");
                    rendered.push_str(line);
                }
            }
        }

        rendered
    }
}
