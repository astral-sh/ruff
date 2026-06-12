use std::borrow::Cow;

use ruff_text_size::{Ranged, TextRange, TextSize};
use strum::IntoEnumIterator;

use super::general;
use crate::docstring::document::SectionKind;
use crate::docstring::document::preformatted::MarkdownFence;
use crate::docstring::document::syntax::starts_with_markdown_list_item;

mod body;
mod google;
mod numpy;
mod rst;

/// Renders a docstring as Markdown.
///
/// `source` must have already undergone PEP-257 trimming and universal newline
/// normalization (typically via `docstring::documentation_trim`).
pub(super) fn render_into(output: &mut String, source: &str) {
    let mut sections = rst::structured_sections(source);
    sections.extend(google::structured_sections(source));
    sections.extend(numpy::structured_sections(source));
    render_sections_into(output, source, sections);
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

fn line_starts_markdown_block_content(line: &str, at_description_start: bool) -> bool {
    MarkdownFence::find(line).is_some()
        || (at_description_start && starts_with_markdown_list_item(line))
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

    use super::{Section, SectionItem, SectionKind, render_into, render_sections_into};

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
    fn google_sections_render_markdown_sections() {
        let docstring = "\
Summary.

Args:
    value (str): The value.
        More detail.
    *items: Extra items.

Keyword Args:
    optional (int): Optional value.

Returns:
    bool: Whether validation passed.

Yields:
    int: Next value.
";
        let rendered = render_docstring(docstring);

        assert_snapshot!(rendered, @r"
        Summary.

        ## Parameters
        **value**: `str`  
        The value.  
        More detail.

        **\*items**  
        Extra items.

        ## Keyword Arguments
        **optional**: `int`  
        Optional value.

        ## Returns
        `bool`  
        Whether validation passed.

        ## Yields
        `int`  
        Next value.
        ");

        let docstring = "\
Args:
    x, y: Coordinates.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Parameters
        **x, y**  
        Coordinates.
        ");

        let docstring = "\
Keyword Arguments:
    retries: Retry count.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Keyword Arguments
        **retries**  
        Retry count.
        ");

        let docstring = "\
Args:
    value: The value.
Additional details.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Parameters
        **value**  
        The value.

        Additional details.
        ");

        let docstring = "\
Args:
    value: The value.
Methods:
    work: Does work.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Parameters
        **value**  
        The value.

        Methods:  
        &nbsp;&nbsp;&nbsp;&nbsp;work: Does work.
        ");

        let docstring = "\
Returns:
    bool: Whether validation passed.
Additional details.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Returns
        `bool`  
        Whether validation passed.

        Additional details.
        ");

        let docstring = "\
Returns:
    str | None: Optional value.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Returns
        `str | None`  
        Optional value.
        ");

        let docstring = "\
Returns:
    One of the known values: foo or bar.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Returns
        One of the known values: foo or bar.
        ");

        let docstring = "\
Returns:
    True if it succeeded.
    False otherwise.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Returns
        True if it succeeded.  
        False otherwise.
        ");

        let docstring = "\
Yields:
    The next item.
    Nothing when exhausted.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Yields
        The next item.  
        Nothing when exhausted.
        ");

        let docstring = "\
Returns:
    str:Path/to/file.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Returns
        `str`  
        Path/to/file.
        ");

        let docstring = "\
Returns:
    Path:foo@bar.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Returns
        `Path`  
        foo@bar.
        ");

        let docstring = "\
Returns:
    https://example.com/path
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Returns
        https://example.com/path
        ");

        let docstring = "\
Yields:
    :obj:`list` of :obj:`str`: Result chunks.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Yields
        `` :obj:`list` of :obj:`str` ``  
        Result chunks.
        ");

        let docstring = "\
Returns:
    str: Example output.
        ```python
        Args:
            value: still code.
        Returns:
            still code.
        ```
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Returns
        `str`  
        Example output.

        ```python
        Args:
            value: still code.
        Returns:
            still code.
        ```
        ");

        let docstring = "\
Yields:
    int: Example output.
        Example::
            Args:
                still code.
            Yields:
                still code.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Yields
        `int`  
        Example output.  
        Example:

        ```````````python
            Args:
                still code.
            Yields:
                still code.
        ```````````
        ");
    }

    #[test]
    fn other_parameter_sections_render_markdown_sections() {
        let docstring = "\
Other Parameters:
    timeout (float): Maximum wait in seconds.
";

        assert_eq!(
            render_docstring(docstring),
            "## Other Parameters\n**timeout**: `float`  \nMaximum wait in seconds."
        );
    }

    #[test]
    fn lowercase_parameter_names_that_resemble_sections_render() {
        let docstring = "\
Args:
    error:
        Error callback.
    returns:
        Whether to return the result.
";

        assert_eq!(
            render_docstring(docstring),
            "## Parameters\n**error**  \nError callback.\n\n\
             **returns**  \nWhether to return the result."
        );
    }

    #[test]
    fn google_item_continuation_paragraphs_use_common_indent() {
        let docstring = "\
Args:
    value: First paragraph.

        Second paragraph.
";

        assert_eq!(
            render_docstring(docstring),
            "## Parameters\n**value**  \nFirst paragraph.\n\nSecond paragraph."
        );
    }

    #[test]
    fn google_parameter_uri_and_path_continuations_stay_raw() {
        for continuation in ["https://example.com/api", "C:\\temp\\result.txt"] {
            let docstring = format!(
                "Args:\n    endpoint: Service endpoint, for example:\n    {continuation}\n"
            );

            assert_eq!(render_docstring(&docstring), render_general(&docstring));
        }
    }

    #[test]
    fn google_existing_code_spans_in_types_are_not_nested() {
        for (docstring, expected) in [
            (
                "Args:\n    signature (``bytes``): Signature bytes.\n",
                "## Parameters\n**signature**: `bytes`  \nSignature bytes.",
            ),
            (
                "Returns:\n    `str`: The result.\n",
                "## Returns\n`str`  \nThe result.",
            ),
        ] {
            assert_eq!(render_docstring(docstring), expected);
        }
    }

    #[test]
    fn google_return_colons_inside_code_spans_are_not_fields() {
        for (docstring, expected) in [
            ("Returns:\n    `key: value`\n", "## Returns\n`key: value`"),
            ("Yields:\n    ``key: value``\n", "## Yields\n``key: value``"),
            (
                "Returns:\n    `dict[str, int]`: Mapping.\n",
                "## Returns\n`dict[str, int]`  \nMapping.",
            ),
        ] {
            assert_eq!(render_docstring(docstring), expected);
        }
    }

    #[test]
    fn google_multiple_return_entries_stay_raw() {
        for heading in ["Returns", "Yields"] {
            let docstring = format!("{heading}:\n    int: Count.\n    str: Name.\n");

            assert_eq!(render_docstring(&docstring), render_general(&docstring));
        }
    }

    #[test]
    fn google_empty_return_sections_do_not_claim_following_prose() {
        for heading in ["Returns", "Yields"] {
            let docstring = format!("Summary.\n\n{heading}:\n\nAfter.\n");

            assert_eq!(render_docstring(&docstring), render_general(&docstring));
        }
    }

    #[test]
    fn google_conventional_spaced_return_types_render() {
        for (docstring, expected) in [
            (
                "Returns:\n    list of int: A new list.\n",
                "## Returns\n`list of int`  \nA new list.",
            ),
            (
                "Yields:\n    int or float: The next number.\n",
                "## Yields\n`int or float`  \nThe next number.",
            ),
            (
                "Returns:\n    tuple(int, int): A pair.\n",
                "## Returns\n`tuple(int, int)`  \nA pair.",
            ),
        ] {
            assert_eq!(render_docstring(docstring), expected);
        }
    }

    #[test]
    fn google_return_prose_with_commas_is_not_a_type() {
        let docstring = "\
Returns:
    (count, results) tuples where:
    count is the number of matches.
";

        assert_eq!(
            render_docstring(docstring),
            "## Returns\n(count, results) tuples where:  \ncount is the number of matches."
        );

        let docstring = "Returns:\n    mapping of user IDs: Preserved as prose.\n";

        assert_eq!(
            render_docstring(docstring),
            "## Returns\nmapping of user IDs: Preserved as prose."
        );
    }

    #[test]
    fn google_return_prose_with_inline_markdown_is_not_a_type() {
        for description in [
            "A `Result` object: When successful.",
            "A [Result] object: When successful.",
        ] {
            let docstring = format!("Returns:\n    {description}\n");

            assert_eq!(
                render_docstring(&docstring),
                format!("## Returns\n{description}")
            );
        }
    }

    #[test]
    fn google_return_prose_with_parenthetical_is_not_a_type() {
        let docstring = "Returns:\n    The result (if any): Additional context.\n";

        assert_eq!(
            render_docstring(docstring),
            "## Returns\nThe result (if any): Additional context."
        );
    }

    #[test]
    fn google_return_lowercase_labels_remain_descriptions() {
        let docstring = "\
Returns:
    A mapping. For example:
    example:
        {\"key\": \"value\"}
";

        assert_eq!(
            render_docstring(docstring),
            "## Returns\nA mapping. For example:  \nexample:  \n    {\"key\": \"value\"}"
        );
    }

    #[test]
    fn google_return_bare_literals_are_not_types() {
        for description in ["42: int.", "200: Success"] {
            let docstring = format!("Returns:\n    {description}\n");

            assert_eq!(
                render_docstring(&docstring),
                format!("## Returns\n{description}")
            );
        }
    }

    #[test]
    fn google_return_windows_paths_remain_intact() {
        let docstring = "Returns:\n    C:\\temp\\result.txt\n";

        assert_eq!(
            render_docstring(docstring),
            "## Returns\nC:\\temp\\result.txt"
        );
    }

    #[test]
    fn google_return_opaque_uris_remain_intact() {
        for uri in ["mailto:user@example.com", "urn:isbn:9780141036144"] {
            let docstring = format!("Returns:\n    {uri}\n");

            assert_eq!(render_docstring(&docstring), format!("## Returns\n{uri}"));
        }
    }

    #[test]
    fn google_return_preformatted_first_lines_render() {
        let docstring = "\
Returns:
    ```text
    Args:
    ```
";

        assert_eq!(
            render_docstring(docstring),
            "## Returns\n```text\nArgs:\n```"
        );

        let docstring = "\
Returns:
    ```text:example
    value
    ```
";

        assert_eq!(
            render_docstring(docstring),
            "## Returns\n```text:example\nvalue\n```"
        );

        let docstring = "\
Yields:
    ~~~text:example
    value
    ~~~
";

        assert_eq!(
            render_docstring(docstring),
            "## Yields\n~~~text:example\nvalue\n~~~"
        );

        let docstring = "\
Yields:
    >>> print(\"Returns:\")
    Returns:
";

        assert_eq!(
            render_docstring(docstring),
            "## Yields\n```````````python\n>>> print(\"Returns:\")\nReturns:\n```````````"
        );
    }

    #[test]
    fn google_raises_section_like_exception_names_render() {
        let docstring = "\
Raises:
    Warning:
        Emitted for legacy input.
    Error:
        Raised for a generic failure.
";

        assert_eq!(
            render_docstring(docstring),
            "## Raises\n`Warning`  \nEmitted for legacy input.\n\n\
             `Error`  \nRaised for a generic failure."
        );
    }

    #[test]
    fn google_headers_nested_in_non_google_containers_stay_raw() {
        for docstring in [
            "\
Examples
--------
Args:
    nested: Example output.
",
            "\
.. note::
    Args:
        nested: Note content.
",
            "\
- Example:
    Args:
        nested: List content.
",
        ] {
            assert_eq!(render_docstring(docstring), render_general(docstring));
        }

        let rst_field = "\
:param value: Example input.
    Args:
        nested: Field content.
:param other: Other input.
";
        assert_eq!(
            render_docstring(rst_field),
            "## Parameters\n**value**  \nExample input.  \nArgs:  \n    nested: Field content.\n\n\
             **other**  \nOther input."
        );
    }

    #[test]
    fn numpy_section_followed_by_google_section_renders_both() {
        let docstring = "\
Parameters
----------
numpy_value : int
    NumPy parameter documentation.

Args:
    google_value: Google parameter documentation.
";

        assert_eq!(
            render_docstring(docstring),
            "## Parameters\n**numpy\\_value**: `int`  \nNumPy parameter documentation.\n\n\
             ## Parameters\n**google\\_value**  \nGoogle parameter documentation."
        );
    }

    #[test]
    fn non_parameter_numpy_sections_stop_at_google_siblings() {
        let docstring = "\
Returns
-------
bool
    Result.

Args:
    value: Parameter documentation.
";
        let rendered = render_docstring(docstring);
        assert!(rendered.contains("## Returns"));
        assert!(rendered.contains("## Parameters"));
        assert!(rendered.contains("**value**"));

        let docstring = "\
Returns
-------
bool
    Result.

Note:
    Caveat.
";
        let rendered = render_docstring(docstring);
        assert!(rendered.contains("## Returns"));
        assert!(rendered.contains("Note:"));
        assert!(!rendered.contains("**Note**"));
    }

    #[test]
    fn unsupported_google_sections_stay_raw() {
        for docstring in [
            "\
Summary.

Args:
    Inputs are normalized first.
    value: The value.
",
            "\
Summary.

Examples:
    Args:
        value: demo input.
",
            "\
Examples:
Args:
    value: demo input.
Returns:
    str: demo output.
",
            "\
Summary.

Returns:
    bool: Whether validation passed.

    Examples:
        Use it.
",
            "\
Summary.

Yields:
    int: Next value.

    Examples:
        Use it.
",
            "\
Summary.

Args:
    Inputs are normalized first.
    Args:
        value: demo input.
",
            "\
Summary.

Args:
    value: The value.

    Examples:
        Use it.
",
            "\
Summary.

Args:
    value: The value.
    Examples: Try it.
",
            "\
Summary.

Returns:
    Examples:
        Use it.
",
            "\
Summary.

Raises:
    ValueError: If invalid.

    Examples:
        Use it.
",
            "\
Summary.

Args:
    value: Example.
        ```python

Args:
    nested = 1
        ```
",
        ] {
            assert_eq!(render_docstring(docstring), render_general(docstring));
        }
    }

    #[test]
    fn google_return_literal_block_first_lines_preserve_literal_rendering() {
        for heading in ["Returns", "Yields"] {
            for marker in ["Result::", "list[str]::"] {
                let docstring = format!("{heading}:\n    {marker}\n\n        value = 1\n");

                assert_eq!(
                    render_docstring(&docstring),
                    format!(
                        "## {heading}\n{}:\n\n```````````python\nvalue = 1\n```````````",
                        marker.trim_end_matches(':')
                    )
                );
            }
        }
    }

    #[test]
    fn numpy_sections_render_markdown_sections() {
        let docstring = "\
Summary.

Parameters
----------
value, alias : str
    The value.
other
    Another value.

Other Parameters
----------------
kw_only : str, optional
    Less common option.

Returns
-------
    result : bool
        Whether validation passed.

Yields
------
    int
        Next value.
";
        assert_snapshot!(render_docstring(docstring), @r"
        Summary.

        ## Parameters
        **value, alias**: `str`  
        The value.

        **other**  
        Another value.

        ## Other Parameters
        **kw\_only**: `str, optional`  
        Less common option.

        ## Returns
        **result**: `bool`  
        Whether validation passed.

        ## Yields
        `int`  
        Next value.
        ");

        let docstring = "\
Summary.

Parameters
----------
value: str
    The value.

Returns
-------
result: bool
    Whether validation passed.

Yields
------
item: int
    Next value.
";
        assert_snapshot!(render_docstring(docstring), @"
        Summary.

        ## Parameters
        **value**: `str`  
        The value.

        ## Returns
        **result**: `bool`  
        Whether validation passed.

        ## Yields
        **item**: `int`  
        Next value.
        ");

        let docstring = "\
Summary.

Returns
-------
    :obj:`list` of :obj:`str`
        Primary values.
    list of node-like
        Related nodes.

Yields
------
    :class:`Iterator` of :obj:`str`
        Next labels.
";
        assert_snapshot!(render_docstring(docstring), @"
        Summary.

        ## Returns
        `` :obj:`list` of :obj:`str` ``  
        Primary values.

        `list of node-like`  
        Related nodes.

        ## Yields
        `` :class:`Iterator` of :obj:`str` ``  
        Next labels.
        ");

        let docstring = "\
Parameters
----------
value : str
    Example::
        ```
other : int
    Another value.
";
        assert_snapshot!(render_docstring(docstring), @"
        ## Parameters
        **value**: `str`  
        Example:

        ```````````python
            ```
        ```````````

        **other**: `int`  
        Another value.
        ");
    }

    #[test]
    fn compact_numpy_parameters_render_as_structured_markdown() {
        let docstring = "\
Parameters
----------
matrix: scipy.sparse array
    Sparse adjacency matrix.
args:
    Additional arguments.
*values:
    Additional values.
Note: deprecated
";

        assert_eq!(
            render_docstring(docstring),
            "## Parameters\n**matrix**: `scipy.sparse array`  \nSparse adjacency matrix.\n\n\
             **args**  \nAdditional arguments.\n\n**\\*values**  \nAdditional values.\n\n\
             Note: deprecated"
        );
    }

    #[test]
    fn undocumented_compact_numpy_parameters_render_as_structured_markdown() {
        let docstring = "\
Parameters
----------
G: Graph
beta : float
    Useful documentation.
";

        assert_eq!(
            render_docstring(docstring),
            "## Parameters\n**G**: `Graph`\n\n**beta**: `float`  \nUseful documentation."
        );
    }

    #[test]
    fn numpy_parameter_section_preambles_render_as_structured_markdown() {
        let docstring = "\
Parameters
----------
Either x or y must be provided.

beta : float
    Useful documentation.
";

        assert_eq!(
            render_docstring(docstring),
            "## Parameters\nEither x or y must be provided.\n\n**beta**: `float`  \n\
             Useful documentation."
        );
    }

    #[test]
    fn numpy_parameter_section_preambles_keep_nested_items_unstructured() {
        let docstring = "\
Parameters
----------
Choose one of the following.
    nested : int
        Example-only text.
beta : float
    Useful documentation.
";

        let rendered = render_docstring(docstring);
        assert!(rendered.contains("**beta**: `float`"));
        assert!(!rendered.contains("**nested**"));
    }

    #[test]
    fn numpy_headers_nested_in_containers_stay_raw() {
        for docstring in [
            "\
Summary.

- Example data:
    Parameters
    ----------
    nested : int
        Not parameter documentation.
",
            "\
Summary.

Examples:
    Parameters
    ----------
    nested : int
        Not parameter documentation.
",
            "\
Examples
--------
    Parameters
    ----------
    nested : int
        Not parameter documentation.

Notes
-----
More details.
",
        ] {
            assert!(!render_docstring(docstring).contains("## Parameters"));
        }

        let docstring = "\
:param value: Example input.

    Parameters
    ----------
    nested : int
        Not parameter documentation.
:param other: Other input.
";
        assert!(!render_docstring(docstring).contains("**nested**"));
    }

    #[test]
    fn shifted_top_level_numpy_sections_render_as_structured_markdown() {
        let docstring = "\
A decoded newline follows:
This line starts at column zero.

    Parameters
    ----------
    shifted : int
        Documentation in a shifted section.

    Returns
    -------
    bool
        Result.
";

        assert_eq!(
            render_docstring(docstring),
            "A decoded newline follows:  \nThis line starts at column zero.\n\n## Parameters\n\
             **shifted**: `int`  \nDocumentation in a shifted section.\n\n## Returns\n\
             `bool`  \nResult."
        );
    }

    #[test]
    fn numpy_undocumented_return_and_raise_items_render() {
        let docstring = "\
Returns
-------
int
str

Raises
------
ValueError
    Invalid value.
TypeError
";

        assert_eq!(
            render_docstring(docstring),
            "## Returns\n`int`\n\n`str`\n\n## Raises\n`ValueError`  \nInvalid value.\n\n`TypeError`"
        );
    }

    #[test]
    fn unsupported_numpy_sections_stay_raw() {
        for docstring in [
            "\
Summary.

Returns
-------
    The created object.
",
            "\
Summary.

Returns
-------

Notes
-----
Not a return value.
",
            "\
Summary.

Parameters
----------
value : str
    Example:
    ```python
other : str
    ```
other : int
    Real parameter.
",
        ] {
            assert_eq!(render_docstring(docstring), render_general(docstring));
        }
    }

    #[test]
    fn indented_numpy_sections_stay_raw() {
        let docstring = "\
Summary.

    Parameters
    ----------
    other : str
        Another value.
";

        assert_eq!(render_docstring(docstring), render_general(docstring));
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

    fn render_docstring(raw: &str) -> String {
        let mut output = String::new();
        render_into(&mut output, raw);
        output
    }

    fn render_general(raw: &str) -> String {
        let mut output = String::new();
        crate::docstring::markdown::general::render_into(&mut output, raw);
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
