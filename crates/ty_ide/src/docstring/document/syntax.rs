use ruff_python_stdlib::identifiers::is_identifier;
use ruff_python_trivia::{Cursor, leading_indentation, tab_offset_u32};
use ruff_source_file::UniversalNewlines;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use super::rst::is_field_list_marker;

/// Collects docstring lines without their universal-newline terminators while preserving their
/// source ranges.
///
/// For example, `first\r\nsecond` yields `first` at offset 0 and `second` at offset 7.
pub(super) fn parsed_lines(source: &str) -> Vec<ParsedLine<'_>> {
    source
        .universal_newlines()
        .map(|line| ParsedLine {
            text: line.as_str(),
            range: line.range(),
            indent: indentation(line.as_str()),
        })
        .collect()
}

/// A docstring line and its source range, excluding the newline terminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ParsedLine<'a> {
    /// The line text, excluding its newline terminator.
    pub(super) text: &'a str,
    /// The byte range of `text` within the source document.
    pub(super) range: TextRange,
    /// The indentation in the source document.
    pub(super) indent: TextSize,
}

/// Returns whether every component of `name` is a Python identifier.
///
/// For example, this returns `true` for `"package.Type"` and `false` for `"package.1"`.
pub(super) fn is_dotted_identifier(name: &str) -> bool {
    !name.is_empty() && name.split('.').all(is_identifier)
}

/// Returns whether `name` is a valid reStructuredText role name.
///
/// For example, this accepts `py:class` and `external+python:py:class`, but rejects `foo..bar`
/// and `foo-`.
pub(in crate::docstring) fn is_rest_role_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .split(['-', '.', '_', '+', ':'])
            .all(|part| !part.is_empty() && part.chars().all(char::is_alphanumeric))
}

/// Returns whether `line` starts with a `CommonMark` list-item marker.
///
/// `CommonMark` limits ordered-list markers to nine digits to avoid integer
/// overflow in browsers: <https://spec.commonmark.org/0.31.2/#list-items>.
pub(in crate::docstring) fn starts_with_markdown_list_item(line: &str) -> bool {
    let bytes = line.as_bytes();
    if matches!(bytes, [b'-' | b'+' | b'*', b' ' | b'\t', ..]) {
        return true;
    }

    let digits = bytes
        .iter()
        .take(9)
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    digits > 0
        && matches!(bytes.get(digits), Some(b'.' | b')'))
        && matches!(bytes.get(digits + 1), Some(b' ' | b'\t'))
}

/// Returns whether `text` is wrapped in a Markdown code span.
///
/// For example, this returns `true` for ``"`value`"`` and `false` for
/// ``"`value` trailing"``.
pub(crate) fn is_wrapped_in_markdown_code_span(text: &str) -> bool {
    let mut tokens = InlineMarkupScanner::new(text);
    let Some(InlineMarkupToken::Code(_)) = tokens.next() else {
        return false;
    };

    tokens.next().is_none()
}

/// Emits non-overlapping tokens that completely span the source text.
///
/// Currently supports `Code` for complete, unescaped backtick-delimited segments and `Text`
/// for everything else.
///
/// For example:
///
/// ```text
/// InlineMarkupScanner::new("before `code` after")
///     => Text("before "), Code("code"), Text(" after")
/// ```
pub(crate) struct InlineMarkupScanner<'a> {
    /// The scanner used to find complete code spans.
    scanner: BacktickScanner<'a>,
    /// The end of the last token returned to the caller.
    last_token_end: TextSize,
    /// A span saved while its preceding text is returned first.
    pending_span: Option<BacktickSpan<'a>>,
}

impl<'a> InlineMarkupScanner<'a> {
    /// Creates a lossless iterator over plain text and complete backtick-delimited code spans.
    ///
    /// Escaped or unmatched backticks remain part of an [`InlineMarkupToken::Text`] token.
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            scanner: BacktickScanner::new(source),
            last_token_end: TextSize::ZERO,
            pending_span: None,
        }
    }

    fn take_remaining_text(&mut self) -> Option<InlineMarkupToken<'a>> {
        let source_end = self.scanner.source.text_len();
        let remaining = TextRange::new(self.last_token_end, source_end);
        self.last_token_end = source_end;
        (!remaining.is_empty()).then(|| InlineMarkupToken::Text(&self.scanner.source[remaining]))
    }
}

impl<'a> Iterator for InlineMarkupScanner<'a> {
    type Item = InlineMarkupToken<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let span = if let Some(span) = self.pending_span.take() {
            // Emit the span saved while returning its preceding text on the previous call.
            span
        } else {
            loop {
                // Without another backtick run, the remaining source is all plain text.
                let Some(opening) = self.scanner.next() else {
                    return self.take_remaining_text();
                };

                // Escaped runs are literal source text, so continue looking for the next possible
                // opening without emitting a token boundary.
                if opening.is_escaped() {
                    continue;
                }

                // Without a closing delimiter, callers cannot treat the opening or any later runs as
                // structured markup. Emit the remainder as one text token.
                let Some(span) = self.scanner.eat_span(opening) else {
                    return self.take_remaining_text();
                };
                break span;
            }
        };

        if self.last_token_end < span.start() {
            let preceding_text = TextRange::new(self.last_token_end, span.start());
            self.last_token_end = span.start();
            self.pending_span = Some(span);
            return Some(InlineMarkupToken::Text(
                &self.scanner.source[preceding_text],
            ));
        }

        debug_assert_eq!(self.last_token_end, span.start());
        self.last_token_end = span.end();
        Some(InlineMarkupToken::Code(span))
    }
}

/// One lossless token produced by [`InlineMarkupScanner`].
///
/// For example:
///
/// ```text
/// source      "before `code` after"
/// tokens      Text("before "), Code("code"), Text(" after")
/// ```
///
/// Escaped and unmatched backticks remain text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InlineMarkupToken<'a> {
    /// Source text outside a complete, unescaped backtick span.
    Text(&'a str),
    /// A complete code span whose backtick delimiters have equal lengths.
    Code(BacktickSpan<'a>),
}

/// Source text delimited by ordered backtick runs of equal length.
///
/// For example:
///
/// ```text
/// source          "before ``code`` after"
/// range()         7..15
/// is_single()     false
/// content()       "code"
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BacktickSpan<'a> {
    /// The source between the opening and closing delimiters.
    content: &'a str,
    /// The byte range including both delimiters.
    range: TextRange,
    /// The byte length of either delimiter.
    delimiter_len: TextSize,
}

impl<'a> BacktickSpan<'a> {
    /// Returns whether both delimiters consist of one backtick.
    pub(crate) fn is_single(self) -> bool {
        self.delimiter_len == TextSize::new(1)
    }

    /// Returns the source between the opening and closing runs.
    pub(crate) fn content(self) -> &'a str {
        self.content
    }
}

impl Ranged for BacktickSpan<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Scans consecutive backtick runs in source order.
///
/// The scanner can consume a complete span after returning its opening run. For example:
///
/// ```text
/// source                  "prefix ``code`` suffix"
/// opening = next()        Some(BacktickRun("``"))
/// as_str()                "code`` suffix"
/// eat_span(opening)       Some(BacktickSpan("``code``"))
/// as_str()                " suffix"
/// ```
#[derive(Clone)]
pub(crate) struct BacktickScanner<'a> {
    /// The complete source whose runs are returned.
    source: &'a str,
    /// The current scan position within `source`.
    cursor: Cursor<'a>,
}

impl<'a> BacktickScanner<'a> {
    /// Creates a scanner positioned at the start of `source`.
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            cursor: Cursor::new(source),
        }
    }

    /// Creates a scanner positioned at `offset` within `source`.
    fn starts_at(offset: TextSize, source: &'a str) -> Self {
        let mut scanner = Self::new(source);
        scanner.cursor.skip_bytes(offset.to_usize());
        scanner
    }

    /// Returns the remaining source.
    pub(crate) fn as_str(&self) -> &'a str {
        self.cursor.as_str()
    }

    /// Consumes the closing run that matches the most recently returned `opening`.
    ///
    /// Returns `None` without advancing when no matching run exists.
    pub(crate) fn eat_span(&mut self, opening: BacktickRun) -> Option<BacktickSpan<'a>> {
        debug_assert_eq!(opening.end(), self.cursor.offset());

        let mut lookahead = self.clone();
        while let Some(closing) = lookahead.next() {
            if let Some(span) = self.span(opening, closing) {
                *self = lookahead;
                return Some(span);
            }
        }
        None
    }

    /// Creates a span from two ordered runs of equal length.
    ///
    /// Both runs must use ranges in this scanner's source.
    pub(crate) fn span(
        &self,
        opening: BacktickRun,
        closing: BacktickRun,
    ) -> Option<BacktickSpan<'a>> {
        debug_assert!(opening.end() <= closing.start());

        if opening.range.len() != closing.range.len() {
            return None;
        }

        Some(BacktickSpan {
            content: &self.source[TextRange::new(opening.end(), closing.start())],
            range: opening.range.cover(closing.range),
            delimiter_len: opening.range.len(),
        })
    }
}

impl Iterator for BacktickScanner<'_> {
    type Item = BacktickRun;

    fn next(&mut self) -> Option<Self::Item> {
        self.cursor.eat_while(|character| character != '`');
        if self.cursor.is_eof() {
            return None;
        }

        let start = self.cursor.offset();
        self.cursor.eat_while(|character| character == '`');
        let range = TextRange::new(start, self.cursor.offset());

        let preceding_backslashes = self.source[..start.to_usize()]
            .bytes()
            .rev()
            .take_while(|byte| *byte == b'\\')
            .count();
        let escaped = !preceding_backslashes.is_multiple_of(2);

        Some(BacktickRun { range, escaped })
    }
}

/// One consecutive run of backticks found by [`BacktickScanner`].
///
/// For example:
///
/// ```text
/// source          "before \\`` after"
/// range()         8..10
/// is_single()     false
/// is_escaped()    true
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BacktickRun {
    /// The byte range of the consecutive backticks.
    range: TextRange,
    /// Whether an odd-length backslash run escapes the first backtick.
    escaped: bool,
}

impl BacktickRun {
    /// Returns whether this run consists of one backtick.
    pub(crate) fn is_single(self) -> bool {
        self.range.len() == TextSize::new(1)
    }

    /// Returns whether a preceding odd-length backslash run escapes this run.
    pub(crate) fn is_escaped(self) -> bool {
        self.escaped
    }
}

impl Ranged for BacktickRun {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// Returns the end of an indented Markdown or reStructuredText container block.
///
/// For example, in `["- item", "  first", "  second", "next"]`, the block at index 0 ends
/// at index 3.
pub(super) fn container_block_end(lines: &[ParsedLine<'_>], index: usize) -> Option<usize> {
    let marker = lines.get(index)?;
    if !starts_container_block(marker.text) {
        return None;
    }

    Some(
        (index + 1..lines.len())
            .find(|&end| {
                let line = lines[end];
                !line.text.trim().is_empty() && line.indent <= marker.indent
            })
            .unwrap_or(lines.len()),
    )
}

/// Returns whether `line` starts a block that owns its indented contents.
pub(super) fn starts_container_block(line: &str) -> bool {
    is_rest_directive_marker(line)
        || is_field_list_marker(line)
        || starts_with_markdown_list_item(line.trim_start())
}

fn is_rest_directive_marker(line: &str) -> bool {
    let Some(directive) = line.trim_start().strip_prefix(".. ") else {
        return false;
    };
    let Some((name, _)) = directive.split_once("::") else {
        return false;
    };

    !name.is_empty() && !name.chars().any(char::is_whitespace)
}

/// Splits at the first top-level colon, ignoring colons inside brackets and quoted strings.
///
/// Bracket kinds are tracked independently, so mismatched nesting such as `([a)]` is treated as
/// balanced. This is sufficient because callers only need to find a delimiter after all bracket
/// groups close; validating the surrounding syntax is outside this helper's scope.
///
/// For example, `"value (Literal['a:b']): description"` splits before `"description"`, not at
/// the colon inside the quoted string.
/// Returns `None` if every colon is inside a bracket group, including an unclosed one.
pub(super) fn split_once_at_top_level_colon(line: &str) -> Option<(&str, &str)> {
    let mut nesting = BracketNesting::default();
    let mut cursor = Cursor::new(line);

    while let Some(character) = cursor.bump() {
        match character {
            '\'' | '"' => consume_quoted_string(&mut cursor, character),
            ':' if nesting.is_top_level() => {
                let index = cursor.offset().to_usize() - character.len_utf8();
                return Some((&line[..index], cursor.as_str()));
            }
            _ => nesting.update(character),
        }
    }

    None
}

#[derive(Default)]
struct BracketNesting {
    parentheses: usize,
    square: usize,
    curly: usize,
}

impl BracketNesting {
    fn is_top_level(&self) -> bool {
        self.parentheses == 0 && self.square == 0 && self.curly == 0
    }

    /// Updates the nesting depth while tolerating unmatched closing brackets.
    ///
    /// For example, `'('` increments the parenthesis depth and a later `')'` decrements it.
    fn update(&mut self, character: char) {
        match character {
            '(' => self.parentheses += 1,
            ')' => self.parentheses = self.parentheses.saturating_sub(1),
            '[' => self.square += 1,
            ']' => self.square = self.square.saturating_sub(1),
            '{' => self.curly += 1,
            '}' => self.curly = self.curly.saturating_sub(1),
            _ => {}
        }
    }
}

/// Advances past a quoted string after its opening quote has been consumed.
///
/// For example, after the opening quote in `"value" trailing` has been consumed, a cursor over
/// `value" trailing` with `quote` set to `'"'` advances to ` trailing`.
pub(super) fn consume_quoted_string(cursor: &mut Cursor<'_>, quote: char) {
    while let Some(character) = cursor.bump() {
        if character == '\\' {
            cursor.bump();
        } else if character == quote {
            break;
        }
    }
}

/// Splits and trims the prefix and contents of a trailing parenthetical expression.
///
/// Parentheses inside quoted strings and Markdown code spans do not affect nesting.
///
/// For example, `"value (Callable[[int], str])"` splits into `"value"` and
/// `"Callable[[int], str]"`.
pub(super) fn split_trailing_parenthetical(value: &str) -> Option<(&str, &str)> {
    if !value.ends_with(')') {
        return None;
    }

    let mut depth = 0usize;
    let mut outermost_opening = None;
    let mut cursor = Cursor::new(value);

    loop {
        let start = cursor.offset();
        let Some(character) = cursor.bump() else {
            break;
        };

        match character {
            quote @ ('\'' | '"') => consume_quoted_string(&mut cursor, quote),
            '`' => {
                let mut scanner = BacktickScanner::starts_at(start, value);
                let opening = scanner.next()?;
                if opening.is_escaped() {
                    // The loop has consumed only the first, escaped backtick. Leave the rest of
                    // the run for the next iteration, where it may open a shorter span.
                    continue;
                }

                let end = scanner
                    .eat_span(opening)
                    .map_or_else(|| opening.end(), |span| span.end());
                cursor.skip_bytes((end - cursor.offset()).to_usize());
            }
            '(' => {
                if depth == 0 {
                    outermost_opening = Some(start);
                }
                depth += 1;
            }
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 && cursor.is_eof() {
                    let opening = outermost_opening?;
                    let (prefix, parenthetical) = value.split_at(opening.to_usize());
                    let contents = parenthetical.strip_prefix('(')?.strip_suffix(')')?;
                    return Some((prefix.trim(), contents.trim()));
                }
            }
            _ => {}
        }
    }

    None
}

/// Calculates indentation width, advancing tabs to the next multiple of eight columns.
pub(super) fn indentation(line: &str) -> TextSize {
    TextSize::new(
        leading_indentation(line)
            .bytes()
            .fold(0u32, |column, byte| match byte {
                b'\t' => column + tab_offset_u32(column, 8),
                _ => column + 1,
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        BacktickScanner, InlineMarkupScanner, InlineMarkupToken, TextSize, is_rest_role_name,
        is_wrapped_in_markdown_code_span, split_once_at_top_level_colon,
        split_trailing_parenthetical,
    };

    #[test]
    fn scans_backtick_runs_and_spans() {
        let mut scanner = BacktickScanner::starts_at(TextSize::new(7), "prefix ``code`` suffix");
        let opening = scanner.next().expect("an opening backtick run");

        assert!(!opening.is_single());
        assert!(!opening.is_escaped());
        assert_eq!(scanner.as_str(), "code`` suffix");

        let span = scanner.eat_span(opening).expect("a matching backtick run");
        assert!(!span.is_single());
        assert_eq!(span.content(), "code");
        assert_eq!(scanner.as_str(), " suffix");
    }

    #[test]
    fn scans_text_and_code_tokens() {
        let source = "é :class:`~pkg.Widget` or ``literal`tick`` β";

        assert_eq!(
            token_contents(source),
            vec![
                ("text", "é :class:"),
                ("code", "~pkg.Widget"),
                ("text", " or "),
                ("code", "literal`tick"),
                ("text", " β"),
            ]
        );
    }

    #[test]
    fn scans_code_at_source_boundaries() {
        assert_eq!(
            token_contents("`first` and `last`"),
            vec![("code", "first"), ("text", " and "), ("code", "last")]
        );
    }

    #[test]
    fn preserves_escaped_and_unmatched_backticks_as_text() {
        let source = r"\`literal\` and `unfinished";

        assert_eq!(token_contents(source), vec![("text", source)]);
    }

    #[test]
    fn recognizes_wrapped_markdown_code_spans() {
        for (text, expected) in [
            ("`value`", true),
            ("``value`with:ticks``", true),
            ("`value` trailing", false),
            ("before `value`", false),
            ("`first` second`", false),
            ("``value```", false),
            ("``", false),
            ("value", false),
        ] {
            assert_eq!(is_wrapped_in_markdown_code_span(text), expected, "{text:?}");
        }
    }

    #[test]
    fn recognizes_rest_role_names() {
        for (name, expected) in [
            ("class", true),
            ("py:class", true),
            ("external+python:py:class", true),
            ("étiquette", true),
            ("foo..bar", false),
        ] {
            assert_eq!(is_rest_role_name(name), expected, "{name:?}");
        }
    }

    #[test]
    fn splits_after_nested_brackets() {
        assert_eq!(
            split_once_at_top_level_colon("value (dict[str, list[{key: value}]]): Description"),
            Some(("value (dict[str, list[{key: value}]])", " Description"))
        );
    }

    #[test]
    fn ignores_colons_inside_quoted_strings() {
        assert_eq!(
            split_once_at_top_level_colon(r"value (Literal['a\'b:c']): Description"),
            Some((r"value (Literal['a\'b:c'])", " Description"))
        );
    }

    #[test]
    fn treats_backticks_as_plain_text() {
        assert_eq!(
            split_once_at_top_level_colon("value (`str): Description such as `.py`."),
            Some(("value (`str)", " Description such as `.py`."))
        );
    }

    #[test]
    fn ignores_colons_inside_balanced_brackets() {
        for line in ["value [a:b]", "value {a:b}"] {
            assert_eq!(split_once_at_top_level_colon(line), None, "{line:?}");
        }
    }

    #[test]
    fn does_not_split_inside_unclosed_brackets() {
        for line in [
            "value (str: Description",
            "value [str: Description",
            "value {str: Description",
        ] {
            assert_eq!(split_once_at_top_level_colon(line), None, "{line:?}");
        }
    }

    #[test]
    fn splits_trailing_parenthesized_group() {
        assert_eq!(
            split_trailing_parenthetical(" value  (  str  )"),
            Some(("value", "str"))
        );
    }

    #[test]
    fn splits_nested_parenthesized_group() {
        assert_eq!(
            split_trailing_parenthetical("value (Callable[(int), tuple[str]])"),
            Some(("value", "Callable[(int), tuple[str]]"))
        );
    }

    #[test]
    fn ignores_parentheses_inside_quoted_strings() {
        assert_eq!(
            split_trailing_parenthetical("value (Literal[')'])"),
            Some(("value", "Literal[')']"))
        );
    }

    #[test]
    fn ignores_parentheses_inside_code_spans() {
        assert_eq!(
            split_trailing_parenthetical("value (`(`)"),
            Some(("value", "`(`"))
        );
    }

    #[test]
    fn ignores_parentheses_inside_code_spans_after_escaped_backtick() {
        assert_eq!(
            split_trailing_parenthetical(r"value (\``)`)"),
            Some(("value", r"\``)`"))
        );
    }

    #[test]
    fn treats_unmatched_backticks_as_plain_parenthetical_text() {
        assert_eq!(
            split_trailing_parenthetical("value (`unfinished)"),
            Some(("value", "`unfinished"))
        );
    }

    #[test]
    fn ignores_parentheses_after_escaped_quotes() {
        assert_eq!(
            split_trailing_parenthetical(r#"value (Literal["a\"b)c"])"#),
            Some(("value", r#"Literal["a\"b)c"]"#))
        );
    }

    #[test]
    fn rejects_unclosed_parenthesized_group() {
        assert_eq!(split_trailing_parenthetical("value (str"), None);
    }

    #[test]
    fn rejects_parenthesized_group_before_trailing_text() {
        assert_eq!(split_trailing_parenthetical("value (str) or None"), None);
    }

    fn token_contents(source: &str) -> Vec<(&'static str, &str)> {
        InlineMarkupScanner::new(source)
            .map(|token| match token {
                InlineMarkupToken::Text(text) => ("text", text),
                InlineMarkupToken::Code(code) => ("code", code.content()),
            })
            .collect()
    }
}
