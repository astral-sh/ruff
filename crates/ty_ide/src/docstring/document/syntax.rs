use ruff_python_stdlib::identifiers::is_identifier;
use ruff_python_trivia::{Cursor, leading_indentation, tab_offset_u32};
use ruff_source_file::UniversalNewlines;
use ruff_text_size::{TextRange, TextSize};

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

/// Returns whether `text` consists of a complete Markdown code span.
///
/// For example, this returns `true` for ``"`value`"`` and `false` for
/// ``"`value` trailing"``.
pub(in crate::docstring) fn is_markdown_code_span(text: &str) -> bool {
    find_backtick_run(text, TextSize::ZERO).and_then(|opening| markdown_code_span(text, opening))
        == Some(TextRange::up_to(TextSize::of(text)))
}

/// Returns the byte range of the first consecutive backtick run at or after `from`.
///
/// For example, searching ``"value `code`"`` from the start returns the range covering the
/// opening ``"`"``.
pub(in crate::docstring) fn find_backtick_run(text: &str, from: TextSize) -> Option<TextRange> {
    let from = from.to_usize();
    let start = from + text.get(from..)?.find('`')?;
    let len = text[start..]
        .bytes()
        .take_while(|byte| *byte == b'`')
        .count();
    Some(TextRange::new(
        TextSize::of(&text[..start]),
        TextSize::of(&text[..start + len]),
    ))
}

/// Returns the Markdown code span delimited by `opening`, if it has a matching closing run.
///
/// For example, the opening run in "``value`with:ticks`` trailing" produces the range covering
/// "``value`with:ticks``".
pub(in crate::docstring) fn markdown_code_span(
    text: &str,
    opening: TextRange,
) -> Option<TextRange> {
    let mut search_from = opening.end();
    loop {
        let closing = find_backtick_run(text, search_from)?;
        if closing.len() == opening.len() {
            return Some(opening.cover(closing));
        }
        search_from = closing.end();
    }
}

/// Returns whether the backtick run at `index` is escaped by a preceding backslash.
///
/// For example, the backtick in ``"\`"`` is escaped, while the backtick in ``"\\`"`` is not.
pub(in crate::docstring) fn is_backtick_run_escaped(text: &str, index: usize) -> bool {
    !text[..index]
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\\')
        .count()
        .is_multiple_of(2)
}

/// Returns the end of an indented Markdown or reStructuredText container block.
///
/// For example, in `["- item", "  first", "  second", "next"]`, the block at index 0 ends
/// at index 3.
pub(super) fn container_block_end(lines: &[ParsedLine<'_>], index: usize) -> Option<usize> {
    let marker = lines.get(index)?;
    if !is_rest_directive_marker(marker.text)
        && !is_field_list_marker(marker.text)
        && !starts_with_markdown_list_item(marker.text.trim_start())
    {
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

    while let Some(character) = cursor.bump() {
        let index = cursor.offset().to_usize() - character.len_utf8();
        match character {
            '\'' | '"' => consume_quoted_string(&mut cursor, character),
            '`' if !is_backtick_run_escaped(value, index) => {
                let opening = find_backtick_run(value, TextSize::of(&value[..index]))?;
                let span = markdown_code_span(value, opening).unwrap_or(opening);
                cursor.skip_bytes((span.end() - cursor.offset()).to_usize());
            }
            '(' => {
                if depth == 0 {
                    outermost_opening = Some(index);
                }
                depth += 1;
            }
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 && cursor.is_eof() {
                    let opening = outermost_opening?;
                    let prefix = value[..opening].trim();
                    let contents = value[opening + '('.len_utf8()..index].trim();
                    return Some((prefix, contents));
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
        is_markdown_code_span, split_once_at_top_level_colon, split_trailing_parenthetical,
    };

    #[test]
    fn recognizes_complete_markdown_code_spans() {
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
            assert_eq!(is_markdown_code_span(text), expected, "{text:?}");
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
}
