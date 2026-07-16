//! Parsing for Google-style docstring sections.
//!
//! The [Google Python Style Guide](https://google.github.io/styleguide/pyguide.html#s3.8.3-functions-and-methods)
//! describes the canonical conventions but does not define a formal grammar. This parser recognizes
//! these parameter section headings:
//!
//! - `Args`, `Arguments`, and `Parameters`
//! - `Keyword Args` and `Keyword Arguments`
//! - `Other Args`, `Other Arguments`, and `Other Parameters`
//!
//! It accepts comma-separated Python names with optional parenthesized types, recovers common
//! conjunction-separated names, preserves continuation text, and skips section-like text inside
//! preformatted or container blocks.
//! Supported section bodies are parsed into prose and named-item fragments; other known headings
//! only delimit sections.
//!
//! Example:
//!
//! ```text
//! Copy a file with retry controls.
//!
//! Args:
//!     source (str): Path to copy.
//!     destination: Destination path.
//!
//! Keyword Args:
//!     timeout, deadline (float): Time limits in seconds.
//!
//! Other Parameters:
//!     retries: Number of retries.
//! ```

use indexmap::IndexMap;
use ruff_python_stdlib::identifiers::is_identifier;
use ruff_python_trivia::Cursor;
use ruff_text_size::{TextRange, TextSize};

use super::SectionKind;
use super::preformatted::PreformattedBlockScanner;
use super::syntax::{
    ParsedLine, consume_quoted_string, container_block_end, indentation, parsed_lines,
    split_once_at_top_level_colon, split_trailing_parenthetical, starts_with_markdown_list_item,
};

/// Returns parameter documentation from recognized Google-style parameter sections.
///
/// `normalized_source` must have already undergone PEP-257 trimming and universal newline
/// normalization.
pub(super) fn parameter_documentation(normalized_source: &str) -> IndexMap<String, String> {
    let mut parameters = Parameters::default();
    for section in sections(normalized_source) {
        let Section {
            kind,
            body,
            range: _,
        } = section;
        if matches!(
            kind,
            SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters
        ) {
            parameters.extend_fragments(body.into_fragments());
        }
    }
    parameters.into_inner()
}

/// Returns recognized Google-style sections in source order.
///
/// `source` must have already undergone PEP-257 trimming and universal
/// newline normalization (typically via `docstring::documentation_trim`).
pub(in crate::docstring) fn sections(source: &str) -> impl Iterator<Item = Section> {
    Parser::new(parsed_lines(source)).parse().into_iter()
}

/// A recognized Google-style docstring section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::docstring) struct Section {
    kind: SectionKind,
    range: TextRange,
    body: SectionBody,
}

impl Section {
    /// Returns the section kind.
    pub(in crate::docstring) const fn kind(&self) -> SectionKind {
        self.kind
    }

    /// Returns the section's source range.
    pub(in crate::docstring) const fn range(&self) -> TextRange {
        self.range
    }

    /// Consumes this section and returns its fragments when it can be rendered structurally.
    pub(in crate::docstring) fn into_renderable_fragments(self) -> Option<Vec<BodyFragment>> {
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
enum SectionBody {
    /// A body parsed into semantic fragments.
    Parsed {
        fragments: Vec<BodyFragment>,
        /// Whether the body's structure is ambiguous.
        has_structural_ambiguity: bool,
    },
    /// A body whose contents were not parsed.
    Opaque,
}

impl SectionBody {
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

    fn into_fragments(self) -> Vec<BodyFragment> {
        match self {
            Self::Parsed { fragments, .. } => fragments,
            Self::Opaque => Vec::new(),
        }
    }
}

/// One parsed fragment in a Google section body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::docstring) enum BodyFragment {
    /// Section-level prose that is not attached to a named item.
    Prose(String),
    /// A named item and its description.
    Item(Item),
}

/// A named item in a Google section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::docstring) struct Item {
    display_name: String,
    ty: Option<String>,
    description: String,
}

impl Item {
    /// Consumes this item and returns its display parts.
    pub(in crate::docstring) fn into_display_name_type_and_description(
        self,
    ) -> (String, Option<String>, String) {
        (self.display_name, self.ty, self.description)
    }
}

/// Splits a display name from a trailing parenthesized type.
///
/// For example, `"value (str)"` yields `("value", Some("str"))`.
fn split_name_and_type(value: &str) -> (&str, Option<&str>) {
    let Some((name, ty)) = split_trailing_parenthetical(value) else {
        return (value, None);
    };

    if name.is_empty() || ty.is_empty() {
        (value, None)
    } else {
        (name, Some(ty))
    }
}

/// Implements a simple heuristic to recover a parameter name and description
/// from a line with a malformed type.
///
/// This assumes the first `" ("` begins the type and the first unquoted `")"` followed by
/// optional whitespace and `":"` ends it.
///
/// For example, `"value (list[str) : description"` yields the name `"value"` and description
/// `" description"`.
fn recover_parameter_without_type(line: &str) -> Option<(&str, &str)> {
    let (display_name, remainder) = line.split_once(" (")?;
    let mut cursor = Cursor::new(remainder);

    while let Some(character) = cursor.bump() {
        match character {
            '\'' | '"' => consume_quoted_string(&mut cursor, character),
            ')' => {
                let mut delimiter = cursor.clone();
                delimiter.eat_while(char::is_whitespace);
                if delimiter.eat_char(':') {
                    return Some((display_name.trim(), delimiter.as_str()));
                }
            }
            _ => {}
        }
    }

    None
}

/// Returns whether `name` is a valid Python parameter name, including variadic prefixes.
fn is_parameter_name(name: &str) -> bool {
    let identifier = name.strip_prefix('*').unwrap_or(name);
    let identifier = identifier.strip_prefix('*').unwrap_or(identifier);
    is_identifier(identifier)
}

/// A parameter name that has been parsed into comma- and conjunction- separated parts.
#[derive(Clone, Copy)]
struct ParameterDisplayName<'a> {
    comma_separated_names: &'a str,
    final_name: Option<&'a str>,
}

impl<'a> ParameterDisplayName<'a> {
    /// Parses comma-separated parameter names, optionally joined by a final conjunction.
    ///
    /// For example, `"stdin, stdout and stderr"` yields the three individual names.
    fn parse(display_name: &'a str) -> Option<Self> {
        let comma_separated = Self {
            comma_separated_names: display_name,
            final_name: None,
        };
        if comma_separated.names().all(is_parameter_name) {
            return Some(comma_separated);
        }

        for conjunction in [" and ", " or "] {
            let Some((comma_separated_names, final_name)) = display_name.rsplit_once(conjunction)
            else {
                continue;
            };
            let conjunction_separated = Self {
                comma_separated_names,
                final_name: Some(final_name),
            };
            if conjunction_separated.names().all(is_parameter_name) {
                return Some(conjunction_separated);
            }
        }

        None
    }

    fn names(self) -> impl Iterator<Item = &'a str> {
        self.comma_separated_names
            .split(',')
            .chain(self.final_name)
            .map(str::trim)
    }

    const fn is_conjunction_separated(self) -> bool {
        self.final_name.is_some()
    }
}

#[derive(Default)]
struct Parameters(IndexMap<String, String>);

impl Parameters {
    fn extend_fragments(&mut self, fragments: Vec<BodyFragment>) {
        for fragment in fragments {
            let BodyFragment::Item(item) = fragment else {
                continue;
            };
            let Item {
                display_name,
                description,
                ty: _,
            } = item;
            if description.is_empty() {
                continue;
            }
            let Some(display_name) = ParameterDisplayName::parse(&display_name) else {
                continue;
            };
            for name in display_name.names() {
                self.0.insert(name.to_string(), description.clone());
            }
        }
    }

    fn into_inner(self) -> IndexMap<String, String> {
        self.0
    }
}

struct Parser<'a> {
    lines: Vec<ParsedLine<'a>>,
    current_line: usize,
    sections: Vec<Section>,
    current: Option<SectionBuilder<'a>>,
    scanner: PreformattedBlockScanner<'a>,
}

impl<'a> Parser<'a> {
    fn new(lines: Vec<ParsedLine<'a>>) -> Self {
        Self {
            lines,
            current_line: 0,
            sections: Vec::new(),
            current: None,
            scanner: PreformattedBlockScanner::default(),
        }
    }

    fn parse(mut self) -> Vec<Section> {
        while self.current_line < self.lines.len() {
            self.push_line();
        }

        if let Some(section) = self.current.take() {
            self.finish_section(section);
        }

        self.sections
    }

    fn push_line(&mut self) {
        let index = self.current_line;
        let line = self.lines[index];
        self.current_line += 1;
        let line_header = (!line.text.trim().is_empty())
            .then(|| Self::parse_header(line))
            .flatten();

        // First, attempt to add the current line to the current section.
        if let Some(mut section) = self.current.take() {
            // If the line is accepted by the current section, then continue parsing that section.
            if section.push_line(line, line_header) {
                self.current = Some(section);
                return;
            }

            // If the line is rejected by the current section, then finalize it.
            self.finish_section(section);
        }

        // Second, skip content owned by a preformatted or container block, where nested headers
        // are inert.
        if self.scanner.consume_preformatted_line(line.text) {
            return;
        }
        if let Some(end) = container_block_end(&self.lines, index) {
            self.current_line = end;
            return;
        }

        // Finally, start a new section from a standalone header, or observe syntax that may
        // introduce a preformatted block.
        if let Some(header) = line_header
            && header.form == HeaderForm::Section
        {
            self.current = Some(SectionBuilder::new(header));
        } else {
            self.scanner
                .observe_line_outside_preformatted_block(line.text);
        }
    }

    fn parse_header(line: ParsedLine<'_>) -> Option<Header> {
        let trimmed = line.text.trim();
        if trimmed.ends_with("::") {
            // A trailing double colon introduces a reST literal block rather than a section.
            return None;
        }

        let (name, description) = split_once_at_top_level_colon(trimmed)?;
        let form = if description.is_empty() {
            HeaderForm::Section
        } else if name.chars().next().is_some_and(char::is_uppercase) {
            HeaderForm::Inline
        } else {
            // Lowercase inline labels are more likely field-like content than section headers.
            return None;
        };
        let kind = HeaderKind::from_name(name)?;

        Some(Header {
            kind,
            form,
            indent: line.indent,
            range: line.range,
        })
    }

    fn finish_section(&mut self, section: SectionBuilder<'a>) {
        if let Some(section) = section.finish() {
            self.sections.push(section);
        }
    }
}

struct SectionBuilder<'a> {
    section_header: Header,
    range: TextRange,
    /// Blank lines whose ownership depends on the next nonblank line.
    pending_blank_lines: Vec<ParsedLine<'a>>,
    /// Prevents code examples from participating in section-boundary detection.
    preformatted: PreformattedBlockScanner<'a>,
    /// Indentation established by the first item-like line.
    ///
    /// This controls section boundaries and may come from a line that cannot be represented as a
    /// structured item.
    boundary_item_indent: Option<TextSize>,
    body: BodyBuilder<'a>,
}

impl<'a> SectionBuilder<'a> {
    fn new(section_header: Header) -> Self {
        Self {
            range: section_header.range,
            pending_blank_lines: Vec::new(),
            preformatted: PreformattedBlockScanner::default(),
            boundary_item_indent: None,
            body: BodyBuilder::new(section_header.kind),
            section_header,
        }
    }

    /// Returns `false` when `line` belongs outside this section.
    fn push_line(&mut self, line: ParsedLine<'a>, line_header: Option<Header>) -> bool {
        // First, let an active preformatted block consume the line before interpreting it.
        let preformatted_block_is_active = self.preformatted.is_active();
        let line_is_preformatted = self.preformatted.consume_preformatted_line(line.text);
        if preformatted_block_is_active && line_is_preformatted {
            self.commit_pending_blank_lines();
            self.push_content_line(line, ItemLine::default());
            return true;
        }

        // Second, defer blank lines until the next content line determines their ownership.
        if line.text.trim().is_empty() {
            self.pending_blank_lines.push(line);
            return true;
        }

        // Third, classify a nonblank line and stop if it begins content outside
        // this section.
        let also_parses_as_section_header =
            line_header.is_some() && line.text.trim().starts_with(char::is_uppercase);
        let item_line = ItemLine::classify(
            self.section_header.kind,
            line,
            also_parses_as_section_header,
        );
        let has_leading_blank_lines = !self.pending_blank_lines.is_empty();
        if self.should_end_before(
            line,
            line_header,
            item_line.boundary_item,
            has_leading_blank_lines,
        ) {
            return false;
        }

        // Finally, commit the accepted line and update the state used to
        // classify later lines.
        let boundary_item = item_line.boundary_item;
        self.commit_pending_blank_lines();
        self.push_content_line(line, item_line);
        if boundary_item {
            self.boundary_item_indent.get_or_insert(line.indent);
        }
        if !line_is_preformatted {
            self.preformatted
                .observe_line_outside_preformatted_block(line.text);
        }

        true
    }

    fn should_end_before(
        &self,
        line: ParsedLine<'_>,
        line_header: Option<Header>,
        boundary_item: bool,
        has_leading_blank_lines: bool,
    ) -> bool {
        // A sibling-level recognized header starts a new section.
        if line_header.is_some_and(|header| header.indent <= self.section_header.indent) {
            return true;
        }

        // Returns and yields sections end before blank-separated sibling-level or dedented prose
        // when no item indentation has been established.
        if matches!(
            self.section_header.kind,
            HeaderKind::Structured(SectionKind::Returns | SectionKind::Yields)
        ) && has_leading_blank_lines
            && line.indent <= self.section_header.indent
            && self.boundary_item_indent.is_none()
        {
            return true;
        }

        // Blank-separated aligned prose ends a parameter section unless it starts another item.
        if has_leading_blank_lines
            && self.section_header.kind.is_parameter_section()
            && self.boundary_item_indent == Some(line.indent)
            && !boundary_item
        {
            return true;
        }

        match line.indent.cmp(&self.section_header.indent) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => {
                // Parameter sections can start with aligned prose before an item establishes the
                // sibling indentation. Once established, aligned lines must match that indentation.
                let item_indent_matches = self
                    .boundary_item_indent
                    .is_none_or(|indent| indent == line.indent);
                !item_indent_matches
                    || (!self.section_header.kind.is_parameter_section() && !boundary_item)
            }
        }
    }

    fn commit_pending_blank_lines(&mut self) {
        for line in self.pending_blank_lines.drain(..) {
            self.range = self.range.cover(line.range);
            self.body.push_blank_line();
        }
    }

    fn push_content_line(&mut self, line: ParsedLine<'a>, item_line: ItemLine<'a>) {
        self.range = self.range.cover(line.range);
        self.body.push_line(self.section_header, line, item_line);
    }

    fn finish(self) -> Option<Section> {
        let HeaderKind::Structured(kind) = self.section_header.kind else {
            return None;
        };

        Some(Section {
            kind,
            range: self.range,
            body: self.body.finish(),
        })
    }
}

/// Returns whether every component of `name` is a Python identifier.
fn is_dotted_identifier(name: &str) -> bool {
    !name.is_empty() && name.split('.').all(is_identifier)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Header {
    kind: HeaderKind,
    form: HeaderForm,
    indent: TextSize,
    range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderForm {
    Section,
    Inline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderKind {
    Structured(SectionKind),
    Opaque,
}

impl HeaderKind {
    fn from_name(name: &str) -> Option<Self> {
        let normalized = name
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_ascii_lowercase();

        // Google specifies the core sections for functions and classes:
        // https://google.github.io/styleguide/pyguide.html#383-functions-and-methods
        // https://google.github.io/styleguide/pyguide.html#384-classes
        //
        // Recognize additional headings and aliases for compatibility with Sphinx Napoleon and
        // existing docstrings:
        // https://www.sphinx-doc.org/en/master/usage/extensions/napoleon.html#docstring-sections
        Some(match normalized.as_str() {
            "args" | "arguments" | "parameters" => Self::Structured(SectionKind::Parameters),
            "keyword args" | "keyword arguments" => Self::Structured(SectionKind::KeywordArguments),
            "other args" | "other arguments" | "other parameters" => {
                Self::Structured(SectionKind::OtherParameters)
            }
            "attributes" => Self::Structured(SectionKind::Attributes),
            "return" | "returns" => Self::Structured(SectionKind::Returns),
            "yield" | "yields" => Self::Structured(SectionKind::Yields),
            "raise" | "raises" => Self::Structured(SectionKind::Raises),

            // Recognized headings without a structured representation still delimit sections.
            "attention" | "caution" | "danger" | "error" | "example" | "examples" | "hint"
            | "important" | "methods" | "note" | "notes" | "references" | "see also" | "tip"
            | "todo" | "todos" | "warning" | "warnings" | "warns" => Self::Opaque,

            // Unrecognized headings remain ordinary content rather than creating a new section.
            _ => return None,
        })
    }
}

enum BodyBuilder<'a> {
    /// A section whose body consists of named items and their descriptions.
    ItemList(ItemListBuilder<'a>),
    /// A section whose entire body is semantic prose (e.g., returns or yields).
    Prose(DescriptionBuilder<'a>),
    /// A recognized section that participates in boundary detection but is not rendered.
    Opaque,
}

impl<'a> BodyBuilder<'a> {
    fn new(kind: HeaderKind) -> Self {
        match kind {
            HeaderKind::Structured(SectionKind::Returns | SectionKind::Yields) => {
                Self::Prose(DescriptionBuilder::default())
            }
            HeaderKind::Structured(_) => Self::ItemList(ItemListBuilder::default()),
            HeaderKind::Opaque => Self::Opaque,
        }
    }

    fn push_blank_line(&mut self) {
        match self {
            Self::ItemList(builder) => builder.push_blank_line(),
            Self::Prose(builder) => builder.push_continuation(""),
            Self::Opaque => {}
        }
    }

    fn push_line(&mut self, section_header: Header, line: ParsedLine<'a>, item_line: ItemLine<'a>) {
        match self {
            Self::ItemList(builder) => builder.push_line(section_header, line, item_line),
            Self::Prose(builder) => builder.push_line(line.text),
            Self::Opaque => {}
        }
    }

    fn finish(self) -> SectionBody {
        match self {
            Self::ItemList(builder) => builder.finish(),
            Self::Prose(builder) => SectionBody::from_prose(builder.finish()),
            Self::Opaque => SectionBody::Opaque,
        }
    }
}

#[derive(Default)]
struct ItemListBuilder<'a> {
    fragments: Vec<BodyFragment>,
    current_item: Option<ItemBuilder<'a>>,
    /// Content encountered before the first recognized item.
    leading_prose: DescriptionBuilder<'a>,
    /// Indentation established by the first renderable item.
    item_indent: Option<TextSize>,
    /// Whether the body's structure is ambiguous.
    has_structural_ambiguity: bool,
}

impl<'a> ItemListBuilder<'a> {
    fn push_blank_line(&mut self) {
        if let Some(item) = &mut self.current_item {
            item.description.push_continuation("");
        } else {
            self.leading_prose.push_continuation("");
        }
    }

    fn push_line(&mut self, section_header: Header, line: ParsedLine<'a>, item_line: ItemLine<'a>) {
        let line_indent = indentation(line.text);
        if self
            .item_indent
            .is_none_or(|item_indent| line_indent == item_indent)
            && let Some(item_header) = item_line.item_header
        {
            self.finish_leading_prose();
            self.finish_current_item();
            self.current_item = Some(ItemBuilder::new(&item_header));
            self.item_indent.get_or_insert(line_indent);
            self.has_structural_ambiguity |=
                item_line.also_parses_as_section_header || item_header.has_structural_ambiguity;
            return;
        }

        if let Some(item_indent) = self.item_indent
            && !item_line.can_render_as_continuation(section_header.kind, line_indent, item_indent)
        {
            self.has_structural_ambiguity = true;
        }

        if let Some(item) = &mut self.current_item {
            item.description.push_continuation(line.text);
        } else {
            self.has_structural_ambiguity = true;
            self.leading_prose.push_line(line.text);
        }
    }

    fn finish_leading_prose(&mut self) {
        let prose = std::mem::take(&mut self.leading_prose).finish();
        if !prose.is_empty() {
            self.fragments.push(BodyFragment::Prose(prose));
        }
    }

    fn finish_current_item(&mut self) {
        if let Some(item) = self.current_item.take() {
            self.fragments.push(BodyFragment::Item(item.finish()));
        }
    }

    fn finish(mut self) -> SectionBody {
        self.finish_leading_prose();
        self.finish_current_item();
        SectionBody::Parsed {
            fragments: self.fragments,
            has_structural_ambiguity: self.has_structural_ambiguity,
        }
    }
}

struct ItemBuilder<'a> {
    display_name: &'a str,
    ty: Option<&'a str>,
    description: DescriptionBuilder<'a>,
}

impl<'a> ItemBuilder<'a> {
    fn new(item_header: &ItemHeader<'a>) -> Self {
        Self {
            display_name: item_header.display_name,
            ty: item_header.ty,
            description: DescriptionBuilder::with_inline(item_header.inline_description),
        }
    }

    fn finish(self) -> Item {
        Item {
            display_name: self.display_name.to_string(),
            ty: self.ty.map(str::to_string),
            description: self.description.finish(),
        }
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

#[derive(Default)]
struct ItemLine<'a> {
    /// Whether this line establishes item indentation for section-boundary detection.
    boundary_item: bool,
    item_header: Option<ItemHeader<'a>>,
    /// Whether this line resembles an item but is actually a URL or path continuation.
    is_item_like_continuation: bool,
    /// Whether this item could instead introduce a new section.
    also_parses_as_section_header: bool,
}

impl<'a> ItemLine<'a> {
    fn can_render_as_continuation(
        &self,
        section_kind: HeaderKind,
        line_indent: TextSize,
        item_indent: TextSize,
    ) -> bool {
        // More deeply indented lines are unambiguously part of the current item.
        line_indent > item_indent
            // Although the style guide suggests indenting continuation lines,
            // aligned parameter prose is common in practice.
            || (line_indent == item_indent && section_kind.is_parameter_section())
            // Aligned URLs and paths are continuations despite resembling item headers.
            || (line_indent == item_indent && self.is_item_like_continuation)
    }

    fn classify(
        section_kind: HeaderKind,
        line: ParsedLine<'a>,
        also_parses_as_section_header: bool,
    ) -> Self {
        let HeaderKind::Structured(kind) = section_kind else {
            return Self::default();
        };
        if matches!(kind, SectionKind::Returns | SectionKind::Yields) {
            return Self {
                boundary_item: true,
                ..Self::default()
            };
        }

        let line_text = line.text.trim();
        let (split, type_was_discarded) = if let Some(split) =
            split_once_at_field_delimiter(line_text)
        {
            (split, false)
        } else if matches!(
            kind,
            SectionKind::Parameters | SectionKind::KeywordArguments | SectionKind::OtherParameters
        ) {
            // If malformed type syntax hides the delimiter, recover the conventional field shape
            // and discard the type. The section remains opaque to the structured renderer.
            let Some(split) = recover_parameter_without_type(line_text) else {
                return Self::default();
            };
            (split, true)
        } else {
            return Self::default();
        };

        Self::from_split(
            kind,
            split,
            also_parses_as_section_header,
            type_was_discarded,
        )
    }

    fn from_split(
        kind: SectionKind,
        (raw_name, inline_description): (&'a str, &'a str),
        also_parses_as_section_header: bool,
        type_was_discarded: bool,
    ) -> Self {
        let name = raw_name.trim();
        if name.is_empty() {
            return Self::default();
        }

        let (display_name, ty, display_name_has_structural_ambiguity) = match kind {
            SectionKind::Parameters
            | SectionKind::KeywordArguments
            | SectionKind::OtherParameters => {
                let (display_name, ty) = split_name_and_type(name);
                let Some(parsed_display_name) = ParameterDisplayName::parse(display_name) else {
                    return Self::default();
                };
                (
                    display_name,
                    ty,
                    parsed_display_name.is_conjunction_separated(),
                )
            }
            SectionKind::Attributes => {
                let (display_name, ty) = split_name_and_type(name);
                if !is_attribute_display_name(display_name) {
                    return Self {
                        boundary_item: true,
                        ..Self::default()
                    };
                }
                (display_name, ty, false)
            }
            SectionKind::Raises => {
                if !is_dotted_identifier(name) {
                    return Self {
                        boundary_item: true,
                        ..Self::default()
                    };
                }
                (name, None, false)
            }
            SectionKind::Returns | SectionKind::Yields => return Self::default(),
        };

        // URLs (`https://...`), Windows paths (`C:\\...`), and reST literal-block introductions
        // (`Example::`) are description continuations rather than item headers.
        //
        // This was configured from a survey of such continuations in popular public projects that
        // use Google-style docstrings; it may need to be reconfigured in the future.
        if matches!(
            inline_description.as_bytes().first(),
            Some(b'/' | b'\\' | b':')
        ) {
            return Self {
                is_item_like_continuation: true,
                ..Self::default()
            };
        }

        Self {
            boundary_item: true,
            item_header: Some(ItemHeader {
                display_name,
                ty,
                inline_description,
                has_structural_ambiguity: type_was_discarded
                    || display_name_has_structural_ambiguity,
            }),
            is_item_like_continuation: false,
            also_parses_as_section_header,
        }
    }
}

struct ItemHeader<'a> {
    display_name: &'a str,
    ty: Option<&'a str>,
    inline_description: &'a str,
    /// Whether this header's noncanonical syntax makes its structure ambiguous.
    has_structural_ambiguity: bool,
}

/// Splits at the field delimiter, skipping top-level colons in reST roles.
///
/// For example, ``:exc:`ValueError`: description`` returns
/// ``(":exc:`ValueError`", " description")``.
fn split_once_at_field_delimiter(line: &str) -> Option<(&str, &str)> {
    let mut cursor = Cursor::new(line);
    loop {
        let (before_colon, after_colon) = split_once_at_top_level_colon(cursor.as_str())?;
        cursor.skip_bytes(before_colon.len());

        if consume_rest_prefix_role(&mut cursor) {
            continue;
        }

        return Some((&line[..cursor.offset().to_usize()], after_colon));
    }
}

/// Consumes the prefix-role pattern recognized by the field parser, leaving the cursor unchanged
/// otherwise.
///
/// For example, this consumes the entire input:
///
/// ```text
/// :exc:`ValueError`
/// ```
fn consume_rest_prefix_role(cursor: &mut Cursor<'_>) -> bool {
    let mut role = cursor.clone();

    // First, require the candidate delimiter to be the opening colon of a role.
    if !role.eat_char(':') {
        return false;
    }

    // Role names start with a Unicode alphanumeric run. Rejecting punctuation here preserves the
    // first colon in `value::class:` as the field delimiter.
    if !role.eat_if(char::is_alphanumeric) {
        return false;
    }

    // Next, scan the rest of the role name until its closing colon and the opening content
    // backtick.
    loop {
        role.eat_while(char::is_alphanumeric);
        if role.eat_char2(':', '`') {
            break;
        }

        // `-._+:` separators are allowed, but only internally to alphanumeric characters.
        if !role.eat_if(|character| matches!(character, '-' | '.' | '_' | '+' | ':'))
            || !role.eat_if(char::is_alphanumeric)
        {
            return false;
        }
    }

    // Finally, skip the role content so delimiter scanning resumes after its closing backtick.
    role.eat_while(|character| character != '`');
    if !role.eat_char('`') {
        return false;
    }

    *cursor = role;
    true
}

fn is_attribute_display_name(display_name: &str) -> bool {
    display_name
        .split(',')
        .all(|name| is_dotted_identifier(name.trim()))
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

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use itertools::Itertools;

    use super::{
        BodyFragment, Item, SectionKind, parameter_documentation, sections,
        split_once_at_field_delimiter,
    };

    #[test]
    fn extracts_aligned_parameter_items() {
        let raw = "\
Arguments:
first: First parameter.
Aligned continuation.
second: Second parameter.
Returns:
bool: Result.";

        assert_snapshot!(display_parameters(raw), @"
        first:
          │ First parameter.
          │ Aligned continuation.
        second:
          │ Second parameter.
        ");
    }

    #[test]
    fn uses_visual_indentation_for_parameter_items() {
        let raw = "\
Args:
  \tfirst: First parameter.
        second: Second parameter.";

        assert_snapshot!(display_parameters(raw), @"
        first:
          │ First parameter.
        second:
          │ Second parameter.
        ");
    }

    #[test]
    fn extracts_comma_separated_parameter_names() {
        let raw = "\
Args:
    x, y: Coordinates.";

        assert_snapshot!(display_parameters(raw), @"
        x:
          │ Coordinates.
        y:
          │ Coordinates.
        ");
    }

    #[test]
    fn extracts_conjunction_separated_parameter_names() {
        let raw = "\
Args:
    stdin, stdout and stderr: Standard streams.
    encoding or errors: Text settings.";

        assert_snapshot!(display_parameters(raw), @"
        stdin:
          │ Standard streams.
        stdout:
          │ Standard streams.
        stderr:
          │ Standard streams.
        encoding:
          │ Text settings.
        errors:
          │ Text settings.
        ");
    }

    #[test]
    fn ignores_prose_before_first_parameter() {
        let raw = "\
Args:
Partition into non-overlapping windows with padding if needed.
    hidden_states (tensor): Input tokens.";

        assert_snapshot!(display_parameters(raw), @"
        hidden_states:
          │ Input tokens.
        ");
    }

    #[test]
    fn extracts_parameter_despite_unbalanced_type_brackets() {
        let raw = "\
Args:
    query_embeddings (`Union[torch.Tensor, list[torch.Tensor]`): Query embeddings.";

        assert_snapshot!(display_parameters(raw), @"
        query_embeddings:
          │ Query embeddings.
        ");
    }

    #[test]
    fn recovers_parameter_after_quoted_delimiter_in_malformed_type() {
        let raw = "\
Args:
    value (Literal['):'], list[str): Actual description.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Actual description.
        ");
    }

    #[test]
    fn recovers_parameter_after_spaced_delimiter_in_malformed_type() {
        let raw = "\
Args:
    value (list[str) : Description.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Description.
        ");
    }

    #[test]
    fn accepts_dashed_parameter_section_underline() {
        let raw = "\
Args:
----
    value: Parameter documentation.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Parameter documentation.
        ");
    }

    #[test]
    fn treats_invalid_parameter_name_as_continuation() {
        let raw = "\
Args:
    value: Initial documentation.
    value, for example: can be omitted.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Initial documentation.
          │ value, for example: can be omitted.
        ");
    }

    #[test]
    fn uses_last_documentation_for_duplicate_parameter() {
        let raw = "\
Args:
    value: First documentation.
    value: Replacement documentation.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Replacement documentation.
        ");
    }

    #[test]
    fn parses_parenthesized_type_with_quoted_parenthesis() {
        let raw = "\
Args:
    value (Literal[\"(\"]): Quoted parenthesis.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Quoted parenthesis.
        ");
    }

    #[test]
    fn ignores_callable_like_parameter_names() {
        let raw = "\
Args:
    callback() (Callable): Not a parameter.
    value: Documentation.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Documentation.
        ");
    }

    #[test]
    fn preserves_parameter_paragraph_breaks() {
        let raw = "\
Args:
    value: First paragraph.


        Second paragraph.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ First paragraph.
          │
          │
          │ Second paragraph.
        ");
    }

    #[test]
    fn recognizes_parameter_section_headings() {
        for heading in [
            "Args",
            "Arguments",
            "Parameters",
            "Keyword Args",
            "Keyword Arguments",
            "Other Args",
            "Other Arguments",
            "Other Parameters",
        ] {
            let raw = format!(
                "\
{heading}:
    value: Parameter documentation."
            );
            assert_parameter_documentation(&raw, &[("value", "Parameter documentation.")]);
        }
    }

    #[test]
    fn ends_parameter_section_at_structured_section_header() {
        assert_parameter_documentation(
            "\
Args:
    value: Parameter documentation.
Methods:
    helper: Method documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ends_unindented_parameter_section_at_aligned_prose() {
        assert_parameter_documentation(
            "\
Args:
value: Parameter documentation.

Additional details.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ends_indented_parameter_section_at_aligned_prose() {
        assert_parameter_documentation(
            "\
Args:
    value: Parameter documentation.

    Additional details.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ends_indented_parameter_section_at_inline_section_header() {
        assert_parameter_documentation(
            "\
Args:
    first: First parameter.
    last: Last parameter.

Returns: Result.",
            &[("first", "First parameter."), ("last", "Last parameter.")],
        );
    }

    #[test]
    fn ends_unindented_parameter_section_at_inline_section_header() {
        assert_parameter_documentation(
            "\
Args:
first: First parameter.
last: Last parameter.
Returns: Result.",
            &[("first", "First parameter."), ("last", "Last parameter.")],
        );
    }

    #[test]
    fn pep257_normalization_recognizes_shifted_sibling_section() {
        assert_parameter_documentation(
            "\
Note:
        context

    Args:
        value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn pep257_normalization_preserves_nested_section() {
        assert_parameter_documentation(
            "
    Note:
        context

        Args:
            nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn pep257_normalization_ends_section_at_dedented_header() {
        assert_parameter_documentation(
            "\
Args:
        value: Parameter documentation.
    Returns:
        bool: Result.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn adjacent_aligned_section_headers_are_siblings() {
        assert_parameter_documentation(
            "\
Example:
Args:
    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn finds_shifted_top_level_section() {
        assert_parameter_documentation(
            "\
A decoded newline follows:
This line starts at column zero.

    Keyword Args:
        shifted: Documentation in a shifted section.",
            &[("shifted", "Documentation in a shifted section.")],
        );
    }

    #[test]
    fn finds_parameter_section_after_first_line_literal_block() {
        // Regression for https://github.com/pytorch/pytorch/blob/e3f5bf0b18585511e6cd7d7a574ebf82f465e5ae/torch/_native/instrumentation.py#L365-L383
        assert_parameter_documentation(
            "\
Instrument a single ``@triton.jit`` kernel, stacked above the jit::

        @instrument_triton_kernel(\"aten::bmm\")
        @triton.jit
        def _bmm_kernel(...): ...

    A Triton kernel compiles lazily and caches variants on the kernel object.

    Args:
        op: Operator symbol being compiled for, e.g. ``\"aten::bmm\"``.",
            &[(
                "op",
                "Operator symbol being compiled for, e.g. ``\"aten::bmm\"``.",
            )],
        );
    }

    #[test]
    fn keeps_colon_prose_in_parameter_documentation() {
        assert_parameter_documentation(
            "\
Args:
    param1 (str): The first parameter description.
    For example: pass an absolute path.
    param2: The second parameter description.",
            &[
                (
                    "param1",
                    "The first parameter description.\nFor example: pass an absolute path.",
                ),
                ("param2", "The second parameter description."),
            ],
        );
    }

    #[test]
    fn preserves_rest_literal_block_indentation_in_parameter_documentation() {
        assert_parameter_documentation(
            "\
Args:
    value: Documentation.
        Example::
            Args:
                nested: Not parameter documentation.
    other: Other documentation.",
            &[
                (
                    "value",
                    "Documentation.\nExample::\n    Args:\n        nested: Not parameter documentation.",
                ),
                ("other", "Other documentation."),
            ],
        );
    }

    #[test]
    fn extracts_variadic_parameters() {
        assert_parameter_documentation(
            "\
Args:
    *args: Extra positional arguments.
    **kwargs: Extra keyword arguments.",
            &[
                ("*args", "Extra positional arguments."),
                ("**kwargs", "Extra keyword arguments."),
            ],
        );
    }

    #[test]
    fn ignores_parameter_section_nested_in_container_section() {
        assert_parameter_documentation(
            "\
Example:
    Args:
        nested: Not parameter documentation.
Args:
    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ignores_parameter_section_in_rest_directive() {
        assert_parameter_documentation(
            "\
Summary.

.. note::
    Args:
        nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_after_blank_line_in_rest_directive() {
        assert_parameter_documentation(
            "\
Summary.

.. note::

        Keyword Args:
            nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_in_unordered_markdown_list_item() {
        assert_parameter_documentation(
            "\
Summary.

- Example:
    Args:
        nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_after_blank_line_in_markdown_list_item() {
        assert_parameter_documentation(
            "\
Summary.

- Example:

        Args:
            nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_in_ordered_markdown_list_item() {
        assert_parameter_documentation(
            "\
Summary.

1. Example:
    Args:
        nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_in_rest_field_list() {
        assert_parameter_documentation(
            "\
Summary.

:param value: Example input.
    Args:
        nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn ignores_parameter_section_in_rest_literal_block() {
        assert_parameter_documentation(
            "\
Summary.

Example::

        Args:
            nested: Not parameter documentation.",
            &[],
        );
    }

    #[test]
    fn resumes_after_markdown_fence() {
        assert_parameter_documentation(
            "\
Summary.

    ```text
    Args:
        nested: Not parameter documentation.
    ```

    Args:
        value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn resumes_after_rest_literal_block() {
        assert_parameter_documentation(
            "\
Summary.

    Example::

        Args:
            nested: Not parameter documentation.

    Args:
        value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn resumes_after_rest_directive() {
        assert_parameter_documentation(
            "\
.. note::
    Args:
        nested: Not parameter documentation.
Args:
    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn resumes_after_unindented_markdown_fence_following_rest_literal_marker() {
        assert_parameter_documentation(
            "\
Example::

```
sample
```

Args:
    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn backticks_in_fence_info_do_not_hide_parameter_sections() {
        assert_parameter_documentation(
            "\
```PRNGKey`` is accepted.

Args:
    value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn ignores_doctest_content_and_resumes_after_it() {
        assert_parameter_documentation(
            "        >>> example()
        Args:
            nested: Not parameter documentation.

        Args:
            value: Parameter documentation.",
            &[("value", "Parameter documentation.")],
        );
    }

    #[test]
    fn returns_structured_section_kinds_in_source_order() {
        let raw = "\
Args:
    value: Documentation.
Keyword Args:
    option: Optional.
Other Parameters:
    other: Other.
Returns:
    bool: Result.";
        let kinds = sections(raw)
            .map(|section| section.kind)
            .collect::<Vec<_>>();

        assert_eq!(
            kinds,
            [
                SectionKind::Parameters,
                SectionKind::KeywordArguments,
                SectionKind::OtherParameters,
                SectionKind::Returns,
            ]
        );
    }

    #[test]
    fn returns_section_fragments_and_range() {
        let raw = "    Args:
        value: Documentation.
Methods:
    helper: Method documentation.";
        let sections = sections(raw)
            .map(|section| {
                (
                    section.kind,
                    section.body.into_fragments(),
                    &raw[section.range],
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            sections,
            vec![(
                SectionKind::Parameters,
                vec![BodyFragment::Item(Item {
                    display_name: "value".to_string(),
                    ty: None,
                    description: "Documentation.".to_string(),
                })],
                "    Args:\n        value: Documentation.",
            )]
        );
    }

    #[test]
    fn ends_populated_return_section_at_aligned_prose() {
        let raw = "\
Returns:
    bool: Result.
Additional details.";
        let sections = sections(raw)
            .map(|section| (section.kind, &raw[section.range]))
            .collect::<Vec<_>>();

        assert_eq!(
            sections,
            vec![(
                SectionKind::Returns,
                "\
Returns:
    bool: Result.",
            )]
        );
    }

    #[test]
    fn skips_rest_roles_before_field_delimiter() {
        assert_eq!(
            split_once_at_field_delimiter(":py:class:`ValueError`: Invalid value."),
            Some((":py:class:`ValueError`", " Invalid value."))
        );
        assert_eq!(
            split_once_at_field_delimiter(":external+python:py:class:`ValueError`: Invalid value."),
            Some((":external+python:py:class:`ValueError`", " Invalid value."))
        );
        assert_eq!(
            split_once_at_field_delimiter(":étiquette:`valeur`: Description."),
            Some((":étiquette:`valeur`", " Description."))
        );
    }

    #[test]
    fn does_not_skip_invalid_rest_roles() {
        for (line, description) in [
            ("value:foo..bar:`X`", "foo..bar:`X`"),
            ("value:foo-:`X`", "foo-:`X`"),
        ] {
            assert_eq!(
                split_once_at_field_delimiter(line),
                Some(("value", description)),
                "{line:?}"
            );
        }
    }

    #[test]
    fn splits_before_rest_role_adjacent_to_field_delimiter() {
        assert_eq!(
            split_once_at_field_delimiter("value::class:`Widget` description."),
            Some(("value", ":class:`Widget` description."))
        );
    }

    #[test]
    fn does_not_split_at_rest_roles_in_prose() {
        assert_eq!(
            split_once_at_field_delimiter("Typically :class:`Intermediate` or a subclass is used."),
            None
        );
    }

    fn display_parameters(raw: &str) -> String {
        let normalized_source = crate::docstring::documentation_trim(raw);
        parameter_documentation(&normalized_source)
            .into_iter()
            .map(|(name, documentation)| {
                let documentation = documentation
                    .lines()
                    .map(|line| match line {
                        "" => "  │".to_string(),
                        _ => format!("  │ {line}"),
                    })
                    .join("\n");
                format!("{name}:\n{documentation}")
            })
            .join("\n")
    }

    #[track_caller]
    fn assert_parameter_documentation(raw: &str, expected: &[(&str, &str)]) {
        let normalized_source = crate::docstring::documentation_trim(raw);
        let parameters = parameter_documentation(&normalized_source);
        assert_eq!(parameters.len(), expected.len(), "{raw}");

        for &(name, documentation) in expected {
            assert_eq!(
                parameters.get(name).map(String::as_str),
                Some(documentation),
                "{raw}"
            );
        }
    }
}
