use std::borrow::Cow;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DocstringSectionKind {
    Parameters,
    Attributes,
    Returns,
    Raises,
}

fn render_markdown_section<'a>(
    output: &mut String,
    heading: &str,
    fields: impl Iterator<Item = &'a SectionItem>,
) {
    let mut previous_description = None;

    // Render each field into the output with the appropriate spacing between fields.
    for field in fields.filter(|field| !field.is_empty()) {
        if previous_description.is_none() {
            if !output.is_empty() {
                output.push_str("\n\n");
            }

            output.push_str("## ");
            output.push_str(heading);
            output.push('\n');
        }

        if let Some(description) = previous_description {
            render_separator_after_description(output, description);
        }

        field.render_into(output);
        previous_description = Some(field.description.as_str());
    }

    if let Some(description) = previous_description {
        render_section_end_after_description(output, description);
    }
}

fn render_inline_description(output: &mut String, description: &str) {
    if description.contains('\n') {
        output.push_str(&description.replace('\n', "\n    "));
    } else {
        output.push_str(description);
    }
}

fn render_separator_after_description(output: &mut String, description: &str) {
    let state = DescriptionState::scan(description);
    if let Some(fence) = state.open_markdown_fence() {
        output.push('\n');
        output.push_str(fence.marker());
        output.push_str("\n\n");
    } else if state.needs_blank_before_next_field() {
        // Add an extra newline to keep the next field out of an open block.
        output.push_str("\n\n");
    } else {
        output.push('\n');
    }
}

fn render_boundary_after_description(
    output: &mut String,
    description: &str,
    following_raw: Option<&str>,
) {
    let state = DescriptionState::scan(description);
    if state.open_markdown_fence().is_some() || state.needs_blank_before_next_field() {
        push_missing_blank_boundary(output, following_raw);
    } else if !following_raw.is_some_and(|raw| raw.starts_with('\n')) {
        output.push('\n');
    }
}

fn push_missing_blank_boundary(output: &mut String, following_raw: Option<&str>) {
    if following_raw.is_some_and(|raw| raw.starts_with("\n\n")) {
        return;
    }

    if following_raw.is_some_and(|raw| raw.starts_with('\n')) {
        output.push('\n');
    } else {
        output.push_str("\n\n");
    }
}

fn render_section_end_after_description(output: &mut String, description: &str) {
    let state = DescriptionState::scan(description);
    if let Some(fence) = state.open_markdown_fence() {
        output.push('\n');
        output.push_str(fence.marker());
    }
}

#[derive(Debug, Default)]
struct DescriptionState<'a> {
    markdown_fence: Option<super::MarkdownFence<'a>>,
    in_doctest: bool,
    // Markdown allows later paragraph lines to lazily continue a list item, so
    // any list item in the trailing block keeps the next field at risk.
    trailing_block_has_markdown_list: bool,
}

impl<'a> DescriptionState<'a> {
    fn scan(description: &'a str) -> Self {
        let mut state = Self::default();

        for line in description.lines().map(|line| line.trim_start_matches(' ')) {
            state.consume_line(line);
        }

        state
    }

    fn needs_blank_before_next_field(&self) -> bool {
        self.in_doctest || self.trailing_block_has_markdown_list
    }

    fn open_markdown_fence(&self) -> Option<super::MarkdownFence<'a>> {
        self.markdown_fence
    }

    fn consume_line(&mut self, line: &'a str) {
        if let Some(fence) = self.markdown_fence {
            if fence.is_closed_by(line) {
                self.markdown_fence = None;
            }
            return;
        }

        if self.in_doctest {
            if line.is_empty() {
                self.in_doctest = false;
                self.trailing_block_has_markdown_list = false;
            }
            return;
        }

        if line.is_empty() {
            self.trailing_block_has_markdown_list = false;
        } else if line.starts_with(">>>") {
            self.in_doctest = true;
            self.trailing_block_has_markdown_list = false;
        } else if let Some(fence) = super::MarkdownFence::find(line) {
            self.markdown_fence = Some(fence);
            self.trailing_block_has_markdown_list = false;
        } else if starts_with_markdown_list_item(line) {
            self.trailing_block_has_markdown_list = true;
        }
    }
}

fn description_block_start(description: &str) -> Option<usize> {
    let mut offset = 0;

    for line in description.split_inclusive('\n') {
        let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
        if line_starts_block_content(line_without_newline) {
            return Some(offset);
        }

        offset += line.len();
    }

    None
}

fn line_starts_block_content(line: &str) -> bool {
    let line = line.trim_start_matches(' ');
    super::MarkdownFence::find(line).is_some()
        || line.starts_with(">>>")
        || starts_with_markdown_list_item(line)
}

fn starts_with_markdown_list_item(line: &str) -> bool {
    starts_with_unordered_markdown_list_item(line) || starts_with_ordered_markdown_list_item(line)
}

fn starts_with_unordered_markdown_list_item(line: &str) -> bool {
    matches!(
        line.as_bytes(),
        [b'-' | b'+' | b'*'] | [b'-' | b'+' | b'*', b' ' | b'\t', ..]
    )
}

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
                .is_none_or(|byte| matches!(*byte, b' ' | b'\t'));
        }

        return false;
    }

    false
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
mod section_tests {
    use insta::{Settings, assert_snapshot};

    use super::{DocstringSectionKind, SectionBlock, SectionItem};

    #[test]
    fn sections_render_in_canonical_order() {
        let section = SectionBlock::new(vec![
            SectionItem::new(
                DocstringSectionKind::Raises,
                Some("ValueError"),
                None,
                "Invalid value.",
            ),
            SectionItem::new(
                DocstringSectionKind::Parameters,
                Some("value"),
                Some("str"),
                "The value.",
            ),
            SectionItem::new(
                DocstringSectionKind::Returns,
                None,
                Some("bool"),
                "Whether validation passed.",
            ),
            SectionItem::new(
                DocstringSectionKind::Attributes,
                Some("cache"),
                Some("dict[str,\n object]"),
                "Cached data.",
            ),
        ]);

        assert_snapshot!(section.render_markdown(), @"
        ## Parameters
        `value` (`str`): The value.

        ## Attributes
        `cache` (`dict[str, object]`): Cached data.

        ## Returns
        `bool`: Whether validation passed.

        ## Raises
        `ValueError`: Invalid value.
        ");
    }

    #[test]
    fn sections_skip_empty_items() {
        let section = SectionBlock::new(vec![
            SectionItem::new(DocstringSectionKind::Parameters, None, None, ""),
            SectionItem::new(DocstringSectionKind::Returns, None, Some(""), ""),
        ]);

        assert_eq!(section.render_markdown(), "");
    }

    #[test]
    fn sections_render_multiline_and_block_descriptions() {
        let mut settings = Settings::clone_current();
        settings.add_filter("\n    \n", "\n<INDENTED-BLANK>\n");
        let _snap = settings.bind_to_scope();

        let section = SectionBlock::new(vec![
            SectionItem::new(
                DocstringSectionKind::Parameters,
                Some("`value`"),
                None,
                "First sentence.\nContinued sentence.\n\nSecond paragraph.",
            ),
            SectionItem::new(
                DocstringSectionKind::Parameters,
                Some("mode"),
                None,
                "Allowed values:\n- fast\n- slow",
            ),
            SectionItem::new(
                DocstringSectionKind::Parameters,
                Some("example"),
                None,
                "Example:\n```python\nif ok:\n    do_work()\n```",
            ),
            SectionItem::new(
                DocstringSectionKind::Parameters,
                Some("prompt"),
                None,
                "Example:\n>>> print('prompt')",
            ),
            SectionItem::new(
                DocstringSectionKind::Parameters,
                Some("choices"),
                None,
                "- first\n- second",
            ),
            SectionItem::new(
                DocstringSectionKind::Parameters,
                Some("steps"),
                None,
                "1. first\n2. second",
            ),
            SectionItem::new(
                DocstringSectionKind::Parameters,
                Some("unterminated"),
                None,
                "```python\nprint('open')",
            ),
            SectionItem::new(
                DocstringSectionKind::Parameters,
                Some("other"),
                None,
                "Another parameter.",
            ),
            SectionItem::new(
                DocstringSectionKind::Returns,
                None,
                Some("str"),
                "```python\nprint('result')",
            ),
            SectionItem::new(
                DocstringSectionKind::Raises,
                Some("ValueError"),
                None,
                "Invalid value.",
            ),
        ]);

        assert_snapshot!(section.render_markdown(), @r#"
        ## Parameters
        `` `value` ``: First sentence.
            Continued sentence.
        <INDENTED-BLANK>
            Second paragraph.
        `mode`: Allowed values:

        - fast
        - slow

        `example`: Example:

        ```python
        if ok:
            do_work()
        ```
        `prompt`: Example:

        >>> print('prompt')

        `choices`:
        - first
        - second

        `steps`:
        1. first
        2. second

        `unterminated`:
        ```python
        print('open')
        ```

        `other`: Another parameter.

        ## Returns
        `str`:
        ```python
        print('result')
        ```

        ## Raises
        `ValueError`: Invalid value.
        "#);
    }
}

pub(super) fn render(raw: &str) -> Cow<'_, str> {
    let blocks = parse_blocks(raw, Vec::new());
    ParsedDocstring { raw, blocks }.render_markdown_source()
}

/// A tolerant, display-oriented parse of a normalized docstring.
pub(super) struct ParsedDocstring<'a> {
    raw: &'a str,
    blocks: Vec<Block<'a>>,
}

impl<'a> ParsedDocstring<'a> {
    #[cfg(test)]
    pub(super) fn parse(raw: &'a str) -> Self {
        let blocks = parse_blocks(raw, Vec::new());
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SectionBlock {
    items: Vec<SectionItem>,
}

impl SectionBlock {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by follow-up structured docstring parsers")
    )]
    pub(super) fn new(items: Vec<SectionItem>) -> Self {
        Self { items }
    }

    fn render_markdown(&self) -> String {
        let mut output = String::new();
        self.render_section(&mut output, "Parameters", DocstringSectionKind::Parameters);
        self.render_section(&mut output, "Attributes", DocstringSectionKind::Attributes);
        self.render_section(&mut output, "Returns", DocstringSectionKind::Returns);
        self.render_section(&mut output, "Raises", DocstringSectionKind::Raises);
        output
    }

    fn render_boundary_before_following_block(
        &self,
        output: &mut String,
        following_raw: Option<&str>,
    ) {
        if let Some(description) = self.last_rendered_description() {
            render_boundary_after_description(output, description, following_raw);
        }
    }

    fn render_section(&self, output: &mut String, heading: &str, kind: DocstringSectionKind) {
        render_markdown_section(
            output,
            heading,
            self.items.iter().filter(move |item| item.kind == kind),
        );
    }

    fn last_rendered_description(&self) -> Option<&str> {
        [
            DocstringSectionKind::Raises,
            DocstringSectionKind::Returns,
            DocstringSectionKind::Attributes,
            DocstringSectionKind::Parameters,
        ]
        .into_iter()
        .find_map(|kind| {
            self.items
                .iter()
                .rev()
                .find(|item| item.kind == kind && !item.is_empty())
                .map(|item| item.description.as_str())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SectionItem {
    kind: DocstringSectionKind,
    display_name: Option<String>,
    ty: Option<String>,
    description: String,
}

impl SectionItem {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by follow-up structured docstring parsers")
    )]
    pub(super) fn new(
        kind: DocstringSectionKind,
        display_name: Option<&str>,
        ty: Option<&str>,
        description: &str,
    ) -> Self {
        Self {
            kind,
            display_name: display_name.map(str::to_string),
            ty: ty.map(str::to_string),
            description: description.to_string(),
        }
    }

    fn is_empty(&self) -> bool {
        self.display_name.is_none()
            && self.ty.as_deref().is_none_or(str::is_empty)
            && self.description.is_empty()
    }

    fn render_into(&self, output: &mut String) {
        let mut has_label = false;

        if let Some(name) = self.display_name.as_deref() {
            render_code_span_into(output, name);
            has_label = true;
        }

        if let Some(ty) = self.ty.as_deref()
            && !ty.is_empty()
        {
            if has_label {
                output.push_str(" (");
                render_type_code_span_into(output, ty);
                output.push(')');
            } else {
                render_type_code_span_into(output, ty);
                has_label = true;
            }
        }

        if !self.description.is_empty() {
            let description = self.description.as_str();
            let block_start = description_block_start(description);

            if has_label {
                output.push_str(if block_start == Some(0) { ":\n" } else { ": " });
            }

            if block_start == Some(0) {
                output.push_str(description);
            } else if let Some(block_start) = block_start {
                let before_block = &description[..block_start];
                render_inline_description(output, before_block.trim_end_matches('\n'));
                output.push_str("\n\n");
                output.push_str(&description[block_start..]);
            } else if description.contains('\n') {
                render_inline_description(output, description);
            } else {
                output.push_str(description);
            }
        }
    }
}

fn parse_blocks(raw: &str, mut sections: Vec<SectionCandidate>) -> Vec<Block<'_>> {
    sections.sort_by_key(|section| section.range.start);
    let mut blocks = Vec::new();
    let mut rendered_through = 0;

    for section in sections {
        if section.range.start < rendered_through {
            continue;
        }

        if !push_raw_block(&mut blocks, raw, rendered_through..section.range.start) {
            return Vec::new();
        }
        rendered_through = section.range.end;
        blocks.push(Block::Section(section.block));
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SectionCandidate {
    range: Range<usize>,
    block: SectionBlock,
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use super::{Block, DocstringSectionKind, ParsedDocstring, SectionBlock, SectionItem};

    #[test]
    fn raw_docstring_renders_borrowed() {
        let docstring = "Summary.\n\nDetails.";
        let parsed = ParsedDocstring::parse(docstring);

        assert_eq!(parsed.render_markdown_source(), docstring);

        let parsed = ParsedDocstring {
            raw: docstring,
            blocks: vec![Block::Raw(&docstring[.."Summary.".len()])],
        };

        assert_eq!(parsed.render_markdown_source(), "Summary.");
    }

    #[test]
    fn section_blocks_render_markdown_source() {
        let parsed = ParsedDocstring {
            raw: "Summary.\n\n:param str value: The value.",
            blocks: vec![
                Block::Raw("Summary.\n\n"),
                Block::Section(SectionBlock::new(vec![
                    SectionItem::new(
                        DocstringSectionKind::Parameters,
                        Some("value"),
                        Some("str"),
                        "The value.",
                    ),
                    SectionItem::new(
                        DocstringSectionKind::Returns,
                        None,
                        Some("bool"),
                        "Whether validation passed.",
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
    }

    #[test]
    fn section_blocks_separate_following_raw_blocks() {
        let parsed = ParsedDocstring {
            raw: ":param value: The value.\nAfter.",
            blocks: vec![
                Block::Section(SectionBlock::new(vec![SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some("value"),
                    None,
                    "The value.",
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
            raw: ":param value: The value.\n\nAfter.",
            blocks: vec![
                Block::Section(SectionBlock::new(vec![SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some("value"),
                    None,
                    "The value.",
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
            raw: ":param value:\n    - First option.\nAfter.",
            blocks: vec![
                Block::Section(SectionBlock::new(vec![SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some("value"),
                    None,
                    "- First option.",
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
            raw: ":param value:\n    - First option.\nAfter.",
            blocks: vec![
                Block::Section(SectionBlock::new(vec![SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some("value"),
                    None,
                    "- First option.",
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
            raw: ":param value:\n    ```python\n    value = 1\nAfter.",
            blocks: vec![
                Block::Section(SectionBlock::new(vec![SectionItem::new(
                    DocstringSectionKind::Parameters,
                    Some("value"),
                    None,
                    "```python\nvalue = 1",
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
