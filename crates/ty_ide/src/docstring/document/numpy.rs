//! Parsing for NumPy-style docstring sections.
//!
//! The [numpydoc style guide](https://numpydoc.readthedocs.io/en/latest/format.html)
//! organizes documentation into sections whose headings are underlined with hyphens. Item-oriented
//! sections conventionally use a `name : type` line followed by an indented description. This
//! parser recognizes `Parameters`, `Other Parameters`, `Attributes`, `Returns`, `Yields`, and
//! `Raises`.
//!
//! Example:
//!
//! ```text
//! Compute the mean of a sequence.
//!
//! Parameters
//! ----------
//! values : sequence of float
//!     Values to average.
//! axis : int, optional
//!     Axis along which to compute the mean.
//!
//! Returns
//! -------
//! float
//!     The arithmetic mean.
//! ```

use indexmap::IndexMap;
use ruff_text_size::{TextRange, TextSize};

use super::preformatted::{PreformattedBlockScanner, starts_preformatted_block};
use super::syntax::{
    ParsedLine, container_block_end, is_dotted_identifier, is_markdown_code_span, parsed_lines,
    split_once_at_top_level_colon, starts_container_block,
};
use super::{DescriptionBuilder, HeaderKind, SectionKind};

/// Returns parameter documentation from recognized NumPy-style parameter sections.
///
/// `normalized_source` must have already undergone PEP-257 trimming and universal newline
/// normalization.
pub(super) fn parameter_documentation(normalized_source: &str) -> IndexMap<String, String> {
    let mut parameters = Parameters::default();

    for section in sections(normalized_source) {
        let Section {
            kind,
            range: _,
            body,
        } = section;
        if matches!(kind, SectionKind::Parameters | SectionKind::OtherParameters) {
            parameters.extend_fragments(body.into_fragments());
        }
    }

    parameters.into_inner()
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
                ty: _,
                description,
            } = item;
            let Some(display_name) = display_name else {
                continue;
            };
            let description = description.trim();
            if description.is_empty() {
                continue;
            }
            let Some(names) = parameter_lookup_names(&display_name) else {
                continue;
            };
            for name in names {
                self.0.insert(name, description.to_string());
            }
        }
    }

    fn into_inner(self) -> IndexMap<String, String> {
        self.0
    }
}

fn parameter_lookup_names(display_name: &str) -> Option<Vec<String>> {
    let mut lookup_names = Vec::new();
    for name in display_name.split(',').map(str::trim) {
        if name == "..." {
            continue;
        }

        if !is_item_name_part(name) {
            return None;
        }
        lookup_names.push(name.to_string());
    }

    (!lookup_names.is_empty()).then_some(lookup_names)
}

/// Returns recognized NumPy-style sections in source order.
///
/// `source` must have already undergone PEP-257 trimming and universal newline normalization
/// (typically via `docstring::documentation_trim`).
pub(in crate::docstring) fn sections(source: &str) -> Vec<Section> {
    Parser::new(parsed_lines(source)).parse()
}

/// A recognized NumPy-style docstring section.
pub(in crate::docstring) type Section = super::Section<Option<String>>;

type SectionBody = super::SectionBody<Option<String>>;

/// One parsed fragment in a NumPy section body.
pub(in crate::docstring) type BodyFragment = super::BodyFragment<Option<String>>;

/// A named or anonymous item in a NumPy section.
type Item = super::Item<Option<String>>;

struct Parser<'a> {
    lines: Vec<ParsedLine<'a>>,
    current_line: usize,
    sections: Vec<Section>,
    current_section: Option<SectionBuilder<'a>>,
    scanner: PreformattedBlockScanner<'a>,
}

impl<'a> Parser<'a> {
    fn new(lines: Vec<ParsedLine<'a>>) -> Self {
        Self {
            lines,
            current_line: 0,
            sections: Vec::new(),
            current_section: None,
            scanner: PreformattedBlockScanner::default(),
        }
    }

    fn parse(mut self) -> Vec<Section> {
        while self.current_line < self.lines.len() {
            self.parse_line();
        }

        if let Some(section) = self.current_section.take() {
            self.finish_section(section);
        }

        self.sections
    }

    fn parse_line(&mut self) {
        let line = self.lines[self.current_line];
        let line_header = self.parse_header(self.current_line);
        let index = self.current_line;
        self.current_line += 1;

        // First, attempt to add the current line to the current section.
        if let Some(mut section) = self.current_section.take() {
            if section.push_line(line, line_header, &self.lines[self.current_line..]) {
                self.current_section = Some(section);
                return;
            }

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
        if let Some(header) = line_header {
            self.current_section = Some(SectionBuilder::new(header));
            self.current_line += 1;
        } else {
            self.scanner
                .observe_line_outside_preformatted_block(line.text);
        }
    }

    fn parse_header(&self, index: usize) -> Option<Header> {
        let line = self.lines[index];
        let underline = self.lines.get(index + 1)?;

        if line.text.trim().is_empty() || !is_underline(underline.text) {
            return None;
        }

        let indent = if index == 0 {
            // PEP 257 trimming strips the indentation from the first line,
            // so instead use the underline to determine this section's indentation.
            underline.indent
        } else if underline.indent == line.indent {
            line.indent
        } else {
            // After the first line, each underline must align with its section title.
            return None;
        };

        Some(Header {
            kind: section_kind(line.text)
                .map(HeaderKind::Structured)
                .unwrap_or(HeaderKind::Opaque),
            indent,
            range: TextRange::new(line.range.start(), underline.range.end()),
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
    pending_blank_lines: Vec<ParsedLine<'a>>,
    preformatted: PreformattedBlockScanner<'a>,
    has_seen_item_block: bool,
    body: BodyBuilder<'a>,
}

impl<'a> SectionBuilder<'a> {
    fn new(section_header: Header) -> Self {
        Self {
            range: section_header.range,
            pending_blank_lines: Vec::new(),
            preformatted: PreformattedBlockScanner::default(),
            has_seen_item_block: false,
            body: BodyBuilder::new(section_header.kind, section_header.indent),
            section_header,
        }
    }

    /// Returns `false` when `line` belongs outside this section.
    fn push_line(
        &mut self,
        line: ParsedLine<'a>,
        line_header: Option<Header>,
        following_lines: &[ParsedLine<'_>],
    ) -> bool {
        // Let an active preformatted block consume the line before interpreting it.
        let preformatted_block_is_active = self.preformatted.is_active();
        let line_is_preformatted = self.preformatted.consume_preformatted_line(line.text);
        if preformatted_block_is_active && line_is_preformatted {
            self.push_body_line(line, None);
            return true;
        }

        // Defer blank lines until the next content line determines their ownership.
        if line.text.trim().is_empty() {
            self.pending_blank_lines.push(line);
            return true;
        }

        // Omit a marker for a static substitution from extracted parameter
        // documentation, but keep scanning explicit parameters and leave the
        // section raw when rendering.
        //
        // This is an edge case, but static substitutions commonly appear in
        // some popular libraries (e.g., SciPy and Matplotlib).
        if self.section_header.kind.is_parameter_section()
            && line_header.is_none()
            && !line_is_preformatted
            && is_static_substitution(line, self.section_header.indent)
        {
            self.push_static_substitution(line);
            return true;
        }

        // Parse the line as an item and determine whether it belongs to this section.
        let item_line = ItemLine::parse(self.section_header, line, following_lines);
        let starts_item_block = item_line.is_some();
        let has_leading_blank_lines = !self.pending_blank_lines.is_empty();
        if !self.line_belongs_to_section(
            line,
            line_header,
            starts_item_block,
            has_leading_blank_lines,
        ) {
            return false;
        }

        // Finally, commit the accepted line and update the state used to classify later lines.
        self.push_body_line(line, item_line);
        self.has_seen_item_block |= starts_item_block;
        if !line_is_preformatted {
            self.preformatted
                .observe_line_outside_preformatted_block(line.text);
        }

        true
    }

    fn line_belongs_to_section(
        &self,
        line: ParsedLine<'_>,
        line_header: Option<Header>,
        starts_item_block: bool,
        has_leading_blank_lines: bool,
    ) -> bool {
        // A sibling-level underlined header starts a new section.
        // Every section, including an opaque one, ends at a sibling or shallower header.
        if line_header.is_some_and(|header| header.indent <= self.section_header.indent) {
            return false;
        }

        // Items are not parsed in opaque sections so only the above header can end them.
        if self.section_header.kind == HeaderKind::Opaque {
            return true;
        }

        match line.indent.cmp(&self.section_header.indent) {
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Equal => {
                if self.section_header.kind.is_parameter_section() {
                    // Parameter sections may contain leading prose and aligned continuations.
                    // After an item establishes the list, a blank line followed by an aligned
                    // non-item ends the section.
                    !self.has_seen_item_block || !has_leading_blank_lines || starts_item_block
                } else {
                    starts_item_block
                }
            }
        }
    }

    fn push_static_substitution(&mut self, line: ParsedLine<'a>) {
        self.commit_pending_blank_lines();
        self.range = self.range.cover(line.range);

        if let BodyBuilder::ItemList(builder) = &mut self.body {
            // At item indentation, the substitution may expand into more items, so end the
            // preceding item. An indented substitution remains within the current description.
            if line.indent == self.section_header.indent {
                builder.finish_current_item();
                self.has_seen_item_block = true;
            }

            // The unknown expansion cannot be reproduced by structured rendering.
            builder.has_structural_ambiguity = true;
        }
    }

    fn commit_pending_blank_lines(&mut self) {
        for line in self.pending_blank_lines.drain(..) {
            self.range = self.range.cover(line.range);
            self.body.push_blank_line();
        }
    }

    fn push_body_line(&mut self, line: ParsedLine<'a>, item_line: Option<ItemLine<'a>>) {
        self.commit_pending_blank_lines();
        self.range = self.range.cover(line.range);
        self.body.push_line(line, item_line);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Header {
    kind: HeaderKind,
    indent: TextSize,
    range: TextRange,
}

fn section_kind(line: &str) -> Option<SectionKind> {
    match line.trim().to_ascii_lowercase().as_str() {
        "parameters" => Some(SectionKind::Parameters),
        "other parameters" => Some(SectionKind::OtherParameters),
        "attributes" => Some(SectionKind::Attributes),
        "returns" => Some(SectionKind::Returns),
        "yields" => Some(SectionKind::Yields),
        "raises" => Some(SectionKind::Raises),
        _ => None,
    }
}

fn is_underline(line: &str) -> bool {
    let line = line.trim();
    line.len() >= 3 && line.chars().all(|char| char == '-')
}

/// Recognizes standalone percent- and dollar-style substitutions.
///
/// Percent substitutions may use any nonempty name without parentheses:
///
/// ```python
/// "%(name)s"
/// "%(Class:kwdoc)s"
/// ```
///
/// Dollar substitutions require a dotted Python identifier:
///
/// ```python
/// "$name"
/// "${package.name}"
/// ```
fn is_static_substitution(line: ParsedLine<'_>, section_indent: TextSize) -> bool {
    let text = line.text.trim();
    let is_percent_marker = text
        .strip_prefix("%(")
        .and_then(|line| line.strip_suffix(")s"))
        .is_some_and(|name| !name.is_empty() && !name.contains('(') && !name.contains(')'));
    let is_dollar_marker = text
        .strip_prefix("${")
        .and_then(|line| line.strip_suffix('}'))
        .or_else(|| text.strip_prefix('$'))
        .is_some_and(is_dotted_identifier);

    line.indent >= section_indent && (is_percent_marker || is_dollar_marker)
}

/// Accepts description-backed items, plus single-token types without one.
fn is_anonymous_return_item(line: &str, has_description: bool) -> bool {
    !line.is_empty()
        && !line.ends_with(['.', ':'])
        && (has_description || !line.chars().any(char::is_whitespace))
}

enum BodyBuilder<'a> {
    /// A recognized section whose body consists of named items and their descriptions.
    ItemList(ItemListBuilder<'a>),
    /// An underlined section that participates in boundary detection but is not parsed.
    Opaque,
}

impl<'a> BodyBuilder<'a> {
    fn new(kind: HeaderKind, required_item_indent: TextSize) -> Self {
        match kind {
            HeaderKind::Structured(_) => Self::ItemList(ItemListBuilder::new(
                kind.is_parameter_section(),
                required_item_indent,
            )),
            HeaderKind::Opaque => Self::Opaque,
        }
    }

    fn push_blank_line(&mut self) {
        if let Self::ItemList(builder) = self {
            builder.push_blank_line();
        }
    }

    fn push_line(&mut self, line: ParsedLine<'a>, item_line: Option<ItemLine<'a>>) {
        if let Self::ItemList(builder) = self {
            builder.push_line(line, item_line);
        }
    }

    fn finish(self) -> SectionBody {
        match self {
            Self::ItemList(builder) => builder.finish(),
            Self::Opaque => SectionBody::Opaque,
        }
    }
}

struct ItemListBuilder<'a> {
    fragments: Vec<BodyFragment>,
    current_item: Option<ItemBuilder<'a>>,
    leading_prose: DescriptionBuilder<'a>,
    required_item_indent: TextSize,
    preserve_leading_prose: bool,
    has_structural_ambiguity: bool,
}

impl<'a> ItemListBuilder<'a> {
    fn new(preserve_leading_prose: bool, required_item_indent: TextSize) -> Self {
        Self {
            fragments: Vec::new(),
            current_item: None,
            leading_prose: DescriptionBuilder::default(),
            required_item_indent,
            preserve_leading_prose,
            has_structural_ambiguity: false,
        }
    }

    fn push_blank_line(&mut self) {
        if let Some(item) = &mut self.current_item {
            item.description.push_continuation("");
        } else if self.preserve_leading_prose {
            self.leading_prose.push_continuation("");
        }
    }

    fn push_line(&mut self, line: ParsedLine<'a>, item_line: Option<ItemLine<'a>>) {
        let is_at_item_indent = line.indent == self.required_item_indent;

        // An item starts a new fragment; its description is collected from later lines.
        if let Some(ItemLine {
            item,
            has_structural_ambiguity,
        }) = item_line
        {
            self.finish_pending_fragments();
            self.current_item = Some(item);
            self.has_structural_ambiguity |= has_structural_ambiguity;
            return;
        }

        // Record when preserving the remaining line as prose or a continuation loses structure.
        if is_at_item_indent {
            // Aligned prose after an item may instead be another, malformed item.
            if self.current_item.is_some() {
                self.has_structural_ambiguity = true;
            }
        } else if self.current_item.is_none() && self.preserve_leading_prose {
            // Indented content before the first item may be nested content or code rather than
            // section-level prose, so interpreting it as prose could discard meaningful structure.
            self.has_structural_ambiguity = true;
        }

        // Preserve the line as an item continuation or leading prose when supported.
        if let Some(item) = &mut self.current_item {
            item.description.push_continuation(line.text);
        } else if self.preserve_leading_prose {
            self.leading_prose.push_line(line.text);
        } else {
            self.has_structural_ambiguity = true;
        }
    }

    fn finish_pending_fragments(&mut self) {
        self.finish_leading_prose();
        self.finish_current_item();
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
        if self.preserve_leading_prose && self.current_item.is_none() && self.fragments.is_empty() {
            return SectionBody::Opaque;
        }

        self.finish_pending_fragments();
        SectionBody::Parsed {
            fragments: self.fragments,
            has_structural_ambiguity: self.has_structural_ambiguity,
        }
    }
}

struct ItemLine<'a> {
    item: ItemBuilder<'a>,
    has_structural_ambiguity: bool,
}

impl<'a> ItemLine<'a> {
    fn parse(
        section_header: Header,
        line: ParsedLine<'a>,
        following_lines: &[ParsedLine<'_>],
    ) -> Option<Self> {
        // Only aligned lines can start items. Other lines are prose or item continuations.
        if line.indent != section_header.indent {
            return None;
        }

        // Each structured section has its own item grammar. Opaque sections only delimit content.
        let HeaderKind::Structured(kind) = section_header.kind else {
            return None;
        };

        match kind {
            SectionKind::Parameters
            | SectionKind::KeywordArguments
            | SectionKind::OtherParameters
            | SectionKind::Attributes => Self::parse_named_item(line, following_lines),
            SectionKind::Returns | SectionKind::Yields => {
                Self::parse_return_item(line, following_lines)
            }
            SectionKind::Raises => Self::parse_raise_item(line),
        }
    }

    fn parse_named_item(line: ParsedLine<'a>, following_lines: &[ParsedLine<'_>]) -> Option<Self> {
        let text = line.text.trim();

        let Some(separator) = parse_type_separator(text) else {
            // Named items may omit their type.
            return is_item_name(text)
                .then(|| Self::new(ItemBuilder::new(Some(text), None, ""), false));
        };

        let name_is_valid = is_item_name(separator.name);
        let item = ItemBuilder::new(Some(separator.name), Some(separator.ty), "");

        // Conventional `name : type` syntax establishes an item boundary even when the name is
        // invalid, preventing it from absorbing adjacent items.
        if separator.has_whitespace_before_colon {
            return Some(Self::new(item, !name_is_valid));
        }

        // Compact syntax requires a valid name and either a type or description.
        if name_is_valid
            && (!separator.ty.is_empty() || has_indented_description(&line, following_lines))
        {
            return Some(Self::new(item, separator.has_structural_ambiguity));
        }

        None
    }

    fn parse_return_item(line: ParsedLine<'a>, following_lines: &[ParsedLine<'_>]) -> Option<Self> {
        let text = line.text.trim();

        // Block openers at item indentation belong outside the section, not to a return item.
        if starts_preformatted_block(text) || starts_container_block(text) {
            return None;
        }

        // A complete code span is an anonymous type even when its contents contain a colon.
        if is_markdown_code_span(text) {
            return Some(Self::new(ItemBuilder::new(None, Some(text), ""), false));
        }

        // Next, prefer the named `name : type` form. A colon adjacent to the name needs a
        // description block to distinguish it from prose.
        let Some(separator) =
            parse_type_separator(text).filter(|separator| !separator.name.is_empty())
        else {
            // Otherwise, accept an anonymous type only when its shape or description
            // distinguishes it from prose.
            let has_description = has_indented_description(&line, following_lines);
            return is_anonymous_return_item(text, has_description)
                .then(|| Self::new(ItemBuilder::new(None, Some(text), ""), false));
        };

        let item = ItemBuilder::new(Some(separator.name), Some(separator.ty), "");

        // Conventional `name : type` syntax is sufficient on its own.
        if separator.has_whitespace_before_colon {
            return Some(Self::new(item, separator.has_structural_ambiguity));
        }

        // A compact separator needs a description to distinguish it from prose.
        has_indented_description(&line, following_lines)
            .then(|| Self::new(item, separator.has_structural_ambiguity))
    }

    fn parse_raise_item(line: ParsedLine<'a>) -> Option<Self> {
        let text = line.text.trim();

        // Raises use a named item, with an optional inline description after the first colon.
        let (name, description) = text
            .split_once(':')
            .map_or((text, ""), |(name, description)| {
                (name.trim(), description.trim())
            });
        if !is_item_name(name) && !is_markdown_code_span(name) {
            return None;
        }

        Some(Self::new(
            ItemBuilder::new(Some(name), None, description),
            false,
        ))
    }

    fn new(item: ItemBuilder<'a>, has_structural_ambiguity: bool) -> Self {
        Self {
            item,
            has_structural_ambiguity,
        }
    }
}

struct ItemBuilder<'a> {
    display_name: Option<&'a str>,
    ty: Option<&'a str>,
    description: DescriptionBuilder<'a>,
}

impl<'a> ItemBuilder<'a> {
    fn new(
        display_name: Option<&'a str>,
        ty: Option<&'a str>,
        inline_description: &'a str,
    ) -> Self {
        Self {
            display_name,
            ty,
            description: DescriptionBuilder::with_inline(inline_description),
        }
    }

    fn finish(self) -> Item {
        Item {
            display_name: self.display_name.map(str::to_string),
            ty: self.ty.map(str::to_string),
            description: self.description.finish(),
        }
    }
}

/// A parsed NumPy-style `name : type` separator.
struct TypeSeparator<'a> {
    /// The documented item name.
    name: &'a str,
    /// The documented item type.
    ty: &'a str,
    /// Whether whitespace before the colon identifies conventional NumPy item syntax.
    has_whitespace_before_colon: bool,
    /// Whether the separator omits whitespace on both sides.
    has_structural_ambiguity: bool,
}

/// Parses a NumPy-style `name : type` separator.
fn parse_type_separator(line: &str) -> Option<TypeSeparator<'_>> {
    let (name, ty) = split_once_at_top_level_colon(line)?;
    let has_whitespace_before_colon = name.ends_with(char::is_whitespace);
    let has_whitespace_after_colon = ty.starts_with(char::is_whitespace);
    let has_structural_ambiguity =
        !has_whitespace_before_colon && !has_whitespace_after_colon && !ty.is_empty();

    Some(TypeSeparator {
        name: name.trim(),
        ty: ty.trim(),
        has_whitespace_before_colon,
        has_structural_ambiguity,
    })
}

fn has_indented_description(line: &ParsedLine<'_>, following_lines: &[ParsedLine<'_>]) -> bool {
    following_lines
        .iter()
        .find(|line| !line.text.trim().is_empty())
        .is_some_and(|next| next.indent > line.indent)
}

/// Returns whether `name` is a valid NumPy-style item name or comma-separated name list.
fn is_item_name(name: &str) -> bool {
    let mut has_lookup_name = false;

    for part in name.split(',') {
        let part = part.trim();
        if part == "..." {
            continue;
        }

        if !is_item_name_part(part) {
            return false;
        }

        has_lookup_name = true;
    }

    has_lookup_name
}

fn is_item_name_part(name: &str) -> bool {
    let name = name
        .strip_prefix("**")
        .or_else(|| name.strip_prefix('*'))
        .unwrap_or(name);

    is_dotted_identifier(name)
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use itertools::Itertools;

    use super::{BodyFragment, Item, SectionBody, parameter_documentation, sections};

    #[test]
    fn extracts_supported_numpy_parameter_items() {
        let raw = r#"
        This is a function description.

        Parameters
        ----------
        param1 : str
            The first parameter description

            This is a second paragraph.
            This is a continuation of the first parameter description.
        param2, param4, ... : int
            The shared parameter description
        param3
            A parameter without type annotation
        *args : object
            Extra positional arguments
        **kwargs : object
            Extra keyword arguments
        options.mode : str
            Nested field documentation
        π : int
            A Unicode parameter
        override_repr: callable, optional
            Replacement representation function
        formats, names :
        undocumented
        copy : bool
            Whether to copy the input

        Other Parameters
        ----------------
        kw_only : str, optional
            A less commonly used keyword-only parameter
        "#;

        assert_snapshot!(display_parameters(raw), @"
        param1:
          │ The first parameter description
          │
          │ This is a second paragraph.
          │ This is a continuation of the first parameter description.
        param2:
          │ The shared parameter description
        param4:
          │ The shared parameter description
        param3:
          │ A parameter without type annotation
        *args:
          │ Extra positional arguments
        **kwargs:
          │ Extra keyword arguments
        options.mode:
          │ Nested field documentation
        π:
          │ A Unicode parameter
        override_repr:
          │ Replacement representation function
        copy:
          │ Whether to copy the input
        kw_only:
          │ A less commonly used keyword-only parameter
        ");
    }

    #[test]
    fn uses_last_documentation_for_duplicate_parameter() {
        let source = normalized(
            r#"
        Parameters
        ----------
        value : str
            First documentation.
        value : str
            Replacement documentation.
        "#,
        );

        assert_eq!(
            parameter_documentation(&source)["value"],
            "Replacement documentation."
        );
    }

    #[test]
    fn extracts_shifted_top_level_numpy_sections() {
        let raw = "\
A decoded newline follows:
This line starts at column zero.

    Parameters
    ----------
    shifted : int
        Documentation in a shifted section.

    Returns
    -------
    bool
        Result.";

        assert_snapshot!(display_parameters(raw), @"
        shifted:
          │ Documentation in a shifted section.
        ");
    }

    #[test]
    fn ignores_numpy_items_nested_in_section_preambles() {
        let raw = "\
Parameters
----------
Choose one of the following.
    nested : int
        Example-only text.
beta : float
    Useful documentation.";

        assert_snapshot!(display_parameters(raw), @"
        beta:
          │ Useful documentation.
        ");
    }

    #[test]
    fn ignores_numpy_sections_in_containers() {
        let raw = "\
Summary.

- Example data:
    Parameters
    ----------
    nested : int
        Not parameter documentation.";

        assert_snapshot!(display_parameters(raw), @"");
    }

    #[test]
    fn ignores_numpy_sections_in_rest_literal_blocks() {
        let raw = "\
Summary.

Example::

    Other Parameters
    ----------------
    nested : int
        Literal content.";

        assert_snapshot!(display_parameters(raw), @"");
    }

    #[test]
    fn finds_numpy_section_after_first_line_rest_literal_block() {
        let raw = "\
Example::

      sample output

    Parameters
    ----------
    value : int
        Parameter documentation.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Parameter documentation.
        ");
    }

    #[test]
    fn ignores_numpy_sections_nested_in_other_sections() {
        let raw = "\
Examples
--------
    Parameters
    ----------
    nested : int
        Not parameter documentation.

Notes
-----
More details.

Parameters
----------
value : int
    Parameter documentation.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Parameter documentation.
        ");
    }

    #[test]
    fn extracts_parameters_from_a_first_line_section() {
        let raw = "\
Parameters
    ----------
    value : int
        Description.

Examples:
    Example prose.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Description.
        ");

        let source = normalized(raw);
        assert!(
            sections(&source)
                .first()
                .is_some_and(|section| &source[section.range]
                    == "\
Parameters
    ----------
    value : int
        Description.")
        );
    }

    #[test]
    fn preserves_blank_lines_in_preformatted_parameter_descriptions() {
        let raw = "\
Parameters
----------
value : str
    ```text
    first

    second
    ```
other : int
    Another value.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ ```text
          │ first
          │
          │ second
          │ ```
        other:
          │ Another value.
        ");
    }

    #[test]
    fn leaves_misaligned_parameter_section_opaque() {
        let raw = "\
Parameters
----------
    value : int
        Description.
    other : str
        Other.";

        let source = normalized(raw);
        assert!(
            sections(&source)
                .first()
                .is_some_and(|section| matches!(section.body, SectionBody::Opaque))
        );
    }

    #[test]
    fn extracts_compact_parameters_without_rendering_them_structurally() {
        let raw = "\
Parameters
----------
d:int
    Parameter d.";

        assert_snapshot!(display_parameters(raw), @"
        d:
          │ Parameter d.
        ");

        assert_section_is_structurally_ambiguous(raw);
    }

    #[test]
    fn leaves_indented_parameter_preambles_raw() {
        let raw = "\
Parameters
----------
Choose one form.
    foo()
beta : int
    Useful documentation.";

        assert_snapshot!(display_parameters(raw), @"
        beta:
          │ Useful documentation.
        ");

        assert_section_is_structurally_ambiguous(raw);
    }

    #[test]
    fn leaves_unconfirmed_parameter_item_opaque() {
        let source = normalized(
            "\
Summary.

Parameters
----------
Note:",
        );
        assert!(
            sections(&source)
                .first()
                .is_some_and(|section| matches!(section.body, SectionBody::Opaque))
        );
    }

    #[test]
    fn extracts_later_parameters_from_an_ambiguous_section() {
        let raw = "\
Parameters
----------
value : int
    Description.
Ambiguous prose.
other : str
    Other.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Description.
          │ Ambiguous prose.
        other:
          │ Other.
        ");

        assert_section_is_structurally_ambiguous(raw);
    }

    #[test]
    fn skips_an_invalid_item_without_ending_the_parameter_list() {
        let raw = "\
Parameters
----------
value : int
    Description.

malformed name : str
    Not value documentation.

other : str
    Other.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Description.
        other:
          │ Other.
        ");

        assert_section_is_structurally_ambiguous(raw);
    }

    #[test]
    fn skips_static_substitutions() {
        let raw = "\
Summary.
    Parameters
    ----------
    first : int
        Before.
        %(description)s
          $DESCRIPTION
        After.

    $ITEM
        Expansion content.
    ${OTHER}
    second : int
        Description.
%(OUTSIDE)s
    outside : int
        Not parameter documentation.

    Parameters
    ----------
    %(boundary)s

    %(left)s or %(right)s
    hidden : int
        Also not parameter documentation.";

        assert_snapshot!(display_parameters(raw), @"
        first:
          │ Before.
          │ After.
        second:
          │ Description.
        ");

        let source = normalized(raw);
        assert!(
            sections(&source)
                .into_iter()
                .all(|section| section.into_renderable_fragments().is_none())
        );
    }

    #[test]
    fn extracts_later_parameters_after_an_unconfirmed_item() {
        let raw = "\
Parameters
----------
value : int
    Description.
Note:
other : str
    Other.";

        assert_snapshot!(display_parameters(raw), @"
        value:
          │ Description.
          │ Note:
        other:
          │ Other.
        ");

        assert_section_is_structurally_ambiguous(raw);
    }

    #[test]
    fn treats_indented_return_items_as_structurally_ambiguous() {
        let raw = "\
Returns
-------
    foo()
    result";

        assert_section_is_structurally_ambiguous(raw);
    }

    #[test]
    fn ends_parameters_before_preformatted_block() {
        let source = normalized(
            "\
Parameters
----------
value : int
    Description.

```text
other : str
```",
        );

        assert!(sections(&source).first().is_some_and(|section| {
            matches!(
                section.body,
                SectionBody::Parsed {
                    has_structural_ambiguity: false,
                    ..
                }
            ) && &source[section.range]
                == "\
Parameters
----------
value : int
    Description."
        }));
    }

    #[test]
    fn parses_attributes_without_descriptions() {
        let source = normalized(
            "\
Attributes
----------
dtype : np.dtype
index",
        );

        assert_eq!(
            sections(&source).first().map(|section| &section.body),
            Some(&SectionBody::Parsed {
                fragments: vec![
                    BodyFragment::Item(Item {
                        display_name: Some("dtype".to_string()),
                        ty: Some("np.dtype".to_string()),
                        description: String::new(),
                    }),
                    BodyFragment::Item(Item {
                        display_name: Some("index".to_string()),
                        ty: None,
                        description: String::new(),
                    }),
                ],
                has_structural_ambiguity: false,
            })
        );
    }

    #[test]
    fn parses_named_and_anonymous_return_items() {
        let source = normalized(
            "\
Returns
-------
np.ndarray, bool
    The values and a flag.
angular separation : Quantity
    The angle between two points.
`module:Type`",
        );

        assert_eq!(
            sections(&source).first().map(|section| &section.body),
            Some(&SectionBody::Parsed {
                fragments: vec![
                    BodyFragment::Item(Item {
                        display_name: None,
                        ty: Some("np.ndarray, bool".to_string()),
                        description: "The values and a flag.".to_string(),
                    }),
                    BodyFragment::Item(Item {
                        display_name: Some("angular separation".to_string()),
                        ty: Some("Quantity".to_string()),
                        description: "The angle between two points.".to_string(),
                    }),
                    BodyFragment::Item(Item {
                        display_name: None,
                        ty: Some("`module:Type`".to_string()),
                        description: String::new(),
                    }),
                ],
                has_structural_ambiguity: false,
            })
        );
    }

    fn assert_section_is_structurally_ambiguous(raw: &str) {
        let source = normalized(raw);
        assert!(sections(&source).first().is_some_and(|section| matches!(
            section.body,
            SectionBody::Parsed {
                has_structural_ambiguity: true,
                ..
            }
        )));
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

    fn normalized(raw: &str) -> String {
        crate::docstring::documentation_trim(raw)
    }
}
