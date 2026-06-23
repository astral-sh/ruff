use std::borrow::Cow;

use ruff_text_size::{Ranged, TextRange, TextSize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use super::general;
use crate::docstring::document::preformatted::MarkdownFence;

mod rst;

/// Renders a docstring as Markdown.
///
/// `source` must have already undergone PEP-257 trimming and universal newline
/// normalization (typically via `docstring::documentation_trim`).
pub(super) fn render_into(output: &mut String, source: &str) {
    render_sections_into(output, source, rst::structured_sections(source));
}

/// Renders a docstring from non-overlapping structured sections and general source fragments.
fn render_sections_into(output: &mut String, source: &str, sections: Vec<Section>) {
    if sections.is_empty() {
        general::render_into(output, source);
        return;
    }

    let Some(segments) = segments(source, sections) else {
        general::render_into(output, source);
        return;
    };

    for (index, segment) in segments.iter().enumerate() {
        match segment {
            Segment::Raw(raw) => {
                let follows_section = index > 0;
                let precedes_section = index + 1 < segments.len();
                let raw = if follows_section {
                    raw.trim_start_matches('\n')
                } else {
                    raw
                };
                let raw = if precedes_section {
                    raw.trim_end_matches('\n')
                } else {
                    raw
                };

                if follows_section && !raw.is_empty() && !output.is_empty() {
                    ensure_blank_line(output);
                }
                general::render_into(output, raw);
            }
            Segment::Structured(section) => {
                if section.is_empty() {
                    continue;
                }
                if !output.is_empty() {
                    ensure_blank_line(output);
                }
                section.render_markdown(output);
            }
        }
    }
}

/// A contiguous segment of a docstring with one Markdown-rendering owner.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment<'a> {
    Raw(&'a str),
    Structured(Section),
}

/// Constructs a list of segments that represent the entire source docstring
/// by interleaving raw segments with structured ones (the latter of which are
/// constructed by wrapping the given parsed sections).
fn segments(source: &str, mut sections: Vec<Section>) -> Option<Vec<Segment<'_>>> {
    sections.sort_unstable_by_key(Section::start);
    let source_len = TextSize::of(source);
    let mut segments = Vec::new();
    let mut cursor = TextSize::default();

    for section in sections {
        let start = section.start();
        let end = section.end();

        if start < cursor {
            return None;
        }

        push_raw_segment(&mut segments, source, TextRange::new(cursor, start));
        cursor = end;
        segments.push(Segment::Structured(section));
    }

    push_raw_segment(&mut segments, source, TextRange::new(cursor, source_len));
    Some(segments)
}

fn push_raw_segment<'a>(segments: &mut Vec<Segment<'a>>, source: &'a str, range: TextRange) {
    if range.is_empty() {
        return;
    }

    let raw = &source[range];
    segments.push(Segment::Raw(raw));
}

fn ensure_blank_line(output: &mut String) {
    if output.ends_with("\n\n") {
        return;
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output.push('\n');
}

/// A parsed section ready for Markdown rendering.
///
/// Parser modules create one of these for each supported source section or
/// field list, then this structured renderer places it back into the
/// surrounding source text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Section {
    /// Byte range occupied by this section in the normalized docstring source.
    range: TextRange,
    /// The list of semantically-meaningful items to render to Markdown.
    items: Vec<SectionItem>,
}

impl Section {
    /// Creates a structured replacement from the items parsed out of one source section.
    fn new(range: TextRange, items: Vec<SectionItem>) -> Option<Self> {
        if range.is_empty() || items.iter().any(SectionItem::is_empty) {
            return None;
        }

        Some(Self { range, items })
    }

    fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Renders the section as Markdown into the given buffer.
    fn render_markdown(&self, output: &mut String) {
        let mut rendered_section = false;
        for kind in SectionKind::iter() {
            if render_markdown_section(
                output,
                kind.heading(),
                self.items.iter().filter(move |item| item.kind == kind),
                rendered_section,
            ) {
                rendered_section = true;
            }
        }
    }
}

impl Ranged for Section {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// One display item within a structured docstring section.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SectionItem {
    kind: SectionKind,
    display_name: Option<String>,
    ty: Option<String>,
    description_source: String,
}

impl SectionItem {
    /// Creates a section item from parser-prepared name, type, and description parts.
    fn new(
        kind: SectionKind,
        display_name: Option<&str>,
        ty: Option<&str>,
        description_source: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            display_name: display_name.map(str::to_string),
            ty: ty.filter(|ty| !ty.is_empty()).map(str::to_string),
            description_source: description_source.into(),
        }
    }

    /// Returns whether the item would render no user-visible Markdown.
    fn is_empty(&self) -> bool {
        self.display_name.is_none() && self.ty.is_none() && self.description_source.is_empty()
    }

    fn render(&self, output: &mut String) {
        let mut has_label = false;

        if let Some(name) = self.display_name.as_deref() {
            if matches!(self.kind, SectionKind::Raises) {
                render_code_span_into(output, name);
            } else {
                render_bold_text_into(output, name);
            }
            has_label = true;
        }

        if let Some(ty) = self.ty.as_deref() {
            if has_label {
                output.push_str(": ");
                render_type_code_span_into(output, ty);
            } else {
                render_type_code_span_into(output, ty);
                has_label = true;
            }
        }

        if !self.description_source.is_empty() {
            let mut description = String::new();
            general::render_fragment_into(&mut description, &self.description_source);
            let block_start = description_block_start(&description);

            match block_start {
                Some(0) => {
                    if has_label {
                        output.push_str("\n\n");
                    }
                    output.push_str(&description);
                }
                Some(block_start) => {
                    let before_block = description[..block_start].trim_end();
                    if has_label {
                        output.push_str("  \n");
                        output.push_str(before_block);
                    } else {
                        output.push_str(before_block);
                    }
                    output.push_str("\n\n");
                    output.push_str(&description[block_start..]);
                }
                None => {
                    if has_label {
                        output.push_str("  \n");
                        output.push_str(&description);
                    } else {
                        output.push_str(&description);
                    }
                }
            }
        }
    }
}

/// Canonical docstring sections shared by supported formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter)]
enum SectionKind {
    Parameters,
    KeywordArguments,
    OtherParameters,
    Attributes,
    Returns,
    Yields,
    Raises,
}

impl SectionKind {
    const fn heading(self) -> &'static str {
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

fn render_markdown_section<'a>(
    output: &mut String,
    heading: &str,
    fields: impl Iterator<Item = &'a SectionItem>,
    rendered_previous_section: bool,
) -> bool {
    let mut rendered_field = false;

    for field in fields {
        if !rendered_field {
            if rendered_previous_section {
                output.push_str("\n\n");
            }

            output.push_str("## ");
            output.push_str(heading);
            output.push('\n');
        } else {
            output.push_str("\n\n");
        }

        field.render(output);
        rendered_field = true;
    }

    rendered_field
}

fn starts_with_markdown_list_item(line: &str) -> bool {
    starts_with_unordered_markdown_list_item(line) || starts_with_ordered_markdown_list_item(line)
}

fn line_starts_markdown_block_content(line: &str, at_description_start: bool) -> bool {
    MarkdownFence::find(line).is_some()
        || (at_description_start && starts_with_markdown_list_item(line))
}

/// Returns whether `line` begins with `-`, `+`, or `*` followed by whitespace.
fn starts_with_unordered_markdown_list_item(line: &str) -> bool {
    matches!(line.as_bytes(), [b'-' | b'+' | b'*', b' ' | b'\t', ..])
}

/// Returns whether `line` begins with one to nine ASCII digits followed by
/// `.` or `)`, then whitespace.
///
/// `CommonMark` limits ordered-list markers to nine digits to avoid integer
/// overflow in browsers: <https://spec.commonmark.org/0.31.2/#list-items>.
fn starts_with_ordered_markdown_list_item(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut digit_count = 0;

    for byte in bytes {
        if digit_count < 9 && byte.is_ascii_digit() {
            digit_count += 1;
            continue;
        }

        if digit_count > 0 && matches!(*byte, b'.' | b')') {
            return bytes
                .get(digit_count + 1)
                .is_some_and(|byte| matches!(*byte, b' ' | b'\t'));
        }

        return false;
    }

    false
}

/// Returns the byte offset where `description` first needs block-style rendering.
fn description_block_start(description: &str) -> Option<usize> {
    let mut offset = 0;
    let mut saw_blank_line = false;

    // By this point, line endings in `description` have already been normalized
    // so it is safe to split only on "\n".
    for line in description.split_inclusive('\n') {
        let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = line_without_newline.trim_start_matches(' ');
        let leading_spaces = line_without_newline.len() - trimmed.len();
        if saw_blank_line {
            if !trimmed.is_empty() {
                return Some(offset);
            }
        } else if trimmed.is_empty() {
            saw_blank_line = true;
        }

        // Later list markers keep their original paragraph-interruption semantics.
        if (offset == 0 && leading_spaces >= 4)
            || (leading_spaces <= 3 && line_starts_markdown_block_content(trimmed, offset == 0))
        {
            return Some(offset);
        }

        offset += line.len();
    }

    None
}

fn render_type_code_span_into(output: &mut String, ty: &str) {
    let normalized = normalize_type_for_code_span(ty);
    render_code_span_into(output, normalized.as_ref());
}

/// Normalizes type text so it fits in a single Markdown code span.
///
/// One-line types are returned unchanged. Multi-line types are trimmed line by
/// line, with empty lines discarded and remaining lines joined by a single
/// space.
///
/// For example:
///
/// ```python
/// dict[str,
///     object]
/// ```
///
/// becomes `dict[str, object]`.
fn normalize_type_for_code_span(ty: &str) -> Cow<'_, str> {
    if !ty.contains('\n') {
        return Cow::Borrowed(ty);
    }

    let mut normalized = String::new();
    for line in ty.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if !normalized.is_empty() {
            normalized.push(' ');
        }
        normalized.push_str(line);
    }

    Cow::Owned(normalized)
}

/// Wraps `text` in Markdown strong emphasis and appends it to output.
fn render_bold_text_into(output: &mut String, text: &str) {
    output.push_str("**");
    for char in text.chars() {
        match char {
            '\\' | '`' | '*' | '_' | '[' | ']' | '|' | '~' => {
                output.push('\\');
                output.push(char);
            }
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(char),
        }
    }
    output.push_str("**");
}

/// Wraps `text` in a Markdown code span and appends it to output.
fn render_code_span_into(output: &mut String, text: &str) {
    // This chooses the number of backticks that we use to delimit the start and
    // end of the inline Markdown code span.
    //
    // The number we pick is one greater than the longest run of consecutive
    // backticks in `text`, which guarantees that we can wrap `text` unambiguously.
    let delimiter_len = text
        .split(|char| char != '`')
        .map(str::len)
        .max()
        .unwrap_or(0)
        + 1;

    output.extend(std::iter::repeat_n('`', delimiter_len));
    if text.starts_with('`') || text.ends_with('`') {
        // Per the CommonMark spec, wrap the contents of the code span in
        // whitespace if those contents start or end with backticks.
        //
        // <https://spec.commonmark.org/0.31.2/#code-spans>
        output.push(' ');
        output.push_str(text);
        output.push(' ');
    } else {
        output.push_str(text);
    }
    output.extend(std::iter::repeat_n('`', delimiter_len));
}

#[cfg(test)]
mod tests {
    use insta::{Settings, assert_snapshot};
    use ruff_text_size::{TextRange, TextSize};

    use super::{Section, SectionItem, SectionKind, render_sections_into};

    #[test]
    fn sections_render_in_canonical_order() {
        let section = section_block(vec![
            SectionItem::new(
                SectionKind::Raises,
                Some("ValueError"),
                None,
                "Invalid value.",
            ),
            SectionItem::new(
                SectionKind::Parameters,
                Some("value"),
                Some("str"),
                "The value.",
            ),
            SectionItem::new(
                SectionKind::OtherParameters,
                Some("kw_only"),
                Some("str"),
                "Less common option.",
            ),
            SectionItem::new(
                SectionKind::KeywordArguments,
                Some("limit"),
                Some("int"),
                "Maximum result count.",
            ),
            SectionItem::new(
                SectionKind::Returns,
                None,
                Some("bool"),
                "Whether validation passed.",
            ),
            SectionItem::new(
                SectionKind::Yields,
                Some("item"),
                Some("Iterator[int]"),
                "Generated values.",
            ),
            SectionItem::new(
                SectionKind::Attributes,
                Some("cache"),
                Some("dict[str,\n object]"),
                "Cached data.",
            ),
        ]);

        assert_snapshot!(render_markdown(&section), @r"
        ## Parameters
        **value**: `str`  
        The value.

        ## Keyword Arguments
        **limit**: `int`  
        Maximum result count.

        ## Other Parameters
        **kw\_only**: `str`  
        Less common option.

        ## Attributes
        **cache**: `dict[str, object]`  
        Cached data.

        ## Returns
        `bool`  
        Whether validation passed.

        ## Yields
        **item**: `Iterator[int]`  
        Generated values.

        ## Raises
        `ValueError`  
        Invalid value.
        ");
    }

    #[test]
    fn empty_types_are_normalized_to_absent() {
        let item = SectionItem::new(SectionKind::Returns, None, Some(""), "");

        assert!(item.ty.is_none());
        assert!(item.is_empty());
    }

    #[test]
    fn empty_section_ranges_are_rejected() {
        let section = Section::new(
            TextRange::default(),
            vec![SectionItem::new(
                SectionKind::Parameters,
                Some("value"),
                None,
                "The value.",
            )],
        );

        assert!(section.is_none());
    }

    #[test]
    fn section_items_escape_bold_names() {
        let section = section_block(vec![
            SectionItem::new(
                SectionKind::Parameters,
                Some("*args"),
                None,
                "Escaped name.",
            ),
            SectionItem::new(
                SectionKind::Parameters,
                Some("__value__"),
                None,
                "Escaped name.",
            ),
            SectionItem::new(
                SectionKind::Parameters,
                Some("<value> & [docs](target) | ~deleted~"),
                None,
                "Escaped name.",
            ),
        ]);

        assert_snapshot!(render_markdown(&section), @r"
        ## Parameters
        **\*args**  
        Escaped name.

        **\_\_value\_\_**  
        Escaped name.

        **&lt;value&gt; &amp; \[docs\](target) \| \~deleted\~**  
        Escaped name.
        ");
    }

    #[test]
    fn section_items_keep_block_descriptions_in_block_context() {
        let _snap = bind_markdown_snapshot_filters();
        let section = section_block(vec![
            SectionItem::new(
                SectionKind::Parameters,
                Some("paragraphs"),
                None,
                "First paragraph.\n\nSecond paragraph.",
            ),
            SectionItem::new(
                SectionKind::Parameters,
                Some("indented"),
                None,
                "    code\n\ntrailing",
            ),
            SectionItem::new(
                SectionKind::Parameters,
                Some("nested"),
                None,
                "- parent\n  - child",
            ),
        ]);

        assert_snapshot!(render_markdown(&section), @"
        ## Parameters
        **paragraphs**<HB>
        First paragraph.

        Second paragraph.

        **indented**

            code<HB>
        <HB>
        trailing

        **nested**

        - parent<HB>
          - child
        ");
    }

    #[test]
    fn section_items_preserve_non_interrupting_ordered_list_markers() {
        let _snap = bind_markdown_snapshot_filters();
        let section = section_block(vec![SectionItem::new(
            SectionKind::Parameters,
            Some("ordered"),
            None,
            "Introduction.\n2. Continuation.",
        )]);

        assert_snapshot!(render_markdown(&section), @"
        ## Parameters
        **ordered**<HB>
        Introduction.<HB>
        2. Continuation.
        ");
    }

    #[test]
    fn docstrings_without_structured_sections_are_rendered_as_general_content() {
        let docstring = "Summary.\n\nDetails.";

        assert_eq!(
            render_sections(docstring, Vec::new()),
            "Summary.  \n  \nDetails."
        );
    }

    #[test]
    fn following_prose_does_not_continue_a_rendered_parameter_list() {
        let rendered = render_parameter_docstring("1. First option.", "After.");

        assert_snapshot!(rendered, @"
        ## Parameters
        **value**

        1. First option.

        After.
        ");
    }

    #[test]
    fn following_prose_does_not_continue_a_rendered_parameter_paragraph() {
        let rendered = render_parameter_docstring("Value.", "After.");

        assert_snapshot!(rendered, @"
        ## Parameters
        **value**  
        Value.

        After.
        ");
    }

    #[test]
    fn following_prose_is_rendered_outside_a_parameter_doctest() {
        let rendered = render_parameter_docstring(">>> value\n1", "After.");

        assert_snapshot!(rendered, @"
        ## Parameters
        **value**

        ```````````python
        >>> value
        1
        ```````````

        After.
        ");
    }

    #[test]
    fn following_prose_is_rendered_outside_an_unclosed_parameter_code_fence() {
        let rendered = render_parameter_docstring("```python\nvalue = 1", "After.");

        assert_snapshot!(rendered, @"
        ## Parameters
        **value**

        ```python
        value = 1
        ```

        After.
        ");
    }

    #[test]
    fn invalid_section_ranges_panic() {
        let raw = "Summary.\n\n:param value:\n    Value.";
        let cases = [
            (
                raw,
                TextRange::new(TextSize::from(10), TextSize::of(raw) + TextSize::from(1)),
            ),
            ("é", TextRange::new(TextSize::default(), TextSize::from(1))),
        ];

        for (raw, range) in cases {
            let result = std::panic::catch_unwind(|| {
                render_sections(
                    raw,
                    vec![section_block_at(
                        range,
                        vec![SectionItem::new(
                            SectionKind::Parameters,
                            Some("value"),
                            None,
                            "Value.",
                        )],
                    )],
                );
            });

            assert!(result.is_err(), "invalid range {range:?} did not panic");
        }
    }

    #[test]
    fn adjacent_sections_are_separated() {
        let raw = "ab";
        let rendered = render_sections(
            raw,
            vec![
                section_block_at(
                    TextRange::new(TextSize::default(), TextSize::from(1)),
                    vec![SectionItem::new(
                        SectionKind::Parameters,
                        Some("value"),
                        None,
                        "The value.",
                    )],
                ),
                section_block_at(
                    TextRange::new(TextSize::from(1), TextSize::of(raw)),
                    vec![SectionItem::new(
                        SectionKind::Returns,
                        None,
                        Some("bool"),
                        "The result.",
                    )],
                ),
            ],
        );

        assert_snapshot!(rendered, @"
        ## Parameters
        **value**  
        The value.

        ## Returns
        `bool`  
        The result.
        ");
    }

    #[test]
    fn partially_overlapping_sections_fall_back_to_general_rendering() {
        let first_source = ":param first: First parameter.";
        let overlap_start = TextSize::of(":param first: ");
        let raw = "\
:param first: First parameter.
:param second: Second parameter.";
        let rendered = render_sections(
            raw,
            vec![
                section_block_at(
                    TextRange::up_to(TextSize::of(first_source)),
                    vec![SectionItem::new(
                        SectionKind::Parameters,
                        Some("first"),
                        None,
                        "First parameter.",
                    )],
                ),
                section_block_at(
                    TextRange::new(overlap_start, TextSize::of(raw)),
                    vec![SectionItem::new(
                        SectionKind::Parameters,
                        Some("second"),
                        None,
                        "Second parameter.",
                    )],
                ),
            ],
        );

        assert_eq!(
            rendered,
            ":param first: First parameter.  \n:param second: Second parameter."
        );
    }

    fn render_markdown(section: &Section) -> String {
        let mut output = String::new();
        section.render_markdown(&mut output);
        output
    }

    fn section_block(items: Vec<SectionItem>) -> Section {
        section_block_at(TextRange::up_to(TextSize::from(1)), items)
    }

    fn section_block_at(range: TextRange, items: Vec<SectionItem>) -> Section {
        let Some(section) = Section::new(range, items) else {
            panic!("test section items should form a section block");
        };
        section
    }

    fn render_parameter_docstring(description: &str, following_prose: &str) -> String {
        let section_source = format!(":param value:\n{}", indent_description(description));
        let raw = format!("{section_source}\n{following_prose}");

        render_sections(
            &raw,
            vec![section_block_at(
                TextRange::up_to(TextSize::of(section_source.as_str())),
                vec![SectionItem::new(
                    SectionKind::Parameters,
                    Some("value"),
                    None,
                    description,
                )],
            )],
        )
    }

    fn indent_description(description: &str) -> String {
        description
            .lines()
            .map(|line| format!("    {line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_sections(raw: &str, sections: Vec<Section>) -> String {
        let mut output = String::new();
        render_sections_into(&mut output, raw, sections);
        output
    }

    fn bind_markdown_snapshot_filters() -> impl Drop {
        let mut settings = Settings::clone_current();
        settings.add_filter("  \n", "<HB>\n");
        settings.bind_to_scope()
    }
}
