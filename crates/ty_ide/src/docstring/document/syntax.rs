use ruff_python_trivia::leading_indentation;
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

/// Splits the input once at the first colon outside bracket pairs and quoted strings.
pub(super) fn split_once_unbracketed_colon(line: &str) -> Option<(&str, &str)> {
    let mut depths = [0usize; 3];
    let mut quote = None;
    let mut escaped = false;
    let mut fallback_colon = None;

    for (index, character) in line.char_indices() {
        if let Some(quote_character) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote_character {
                quote = None;
            }
            continue;
        }

        match character {
            '\'' | '"' => quote = Some(character),
            '(' => depths[0] += 1,
            ')' => depths[0] = depths[0].saturating_sub(1),
            '[' => depths[1] += 1,
            ']' => depths[1] = depths[1].saturating_sub(1),
            '{' => depths[2] += 1,
            '}' => depths[2] = depths[2].saturating_sub(1),
            ':' if depths == [0; 3] => {
                return Some((&line[..index], &line[index + character.len_utf8()..]));
            }
            // Retain a colon outside parentheses as a fallback. This recovers an item delimiter
            // after malformed square or curly brackets while preferring a fully balanced split.
            ':' if depths[0] == 0 && fallback_colon.is_none() => fallback_colon = Some(index),
            _ => {}
        }
    }

    fallback_colon.map(|index| (&line[..index], &line[index + ':'.len_utf8()..]))
}

/// Splits a trailing parenthesized type from a parameter display name.
pub(super) fn parse_parenthesized_type(name: &str) -> (&str, Option<&str>) {
    if !name.ends_with(')') {
        return (name, None);
    }

    let mut depth = 0usize;
    let mut opening = None;
    let mut quote = None;
    let mut escaped = false;

    for (index, character) in name.char_indices() {
        if let Some(quote_character) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote_character {
                quote = None;
            }
            continue;
        }

        match character {
            '\'' | '"' => quote = Some(character),
            '(' => {
                if depth == 0 {
                    opening = Some(index);
                }
                depth += 1;
            }
            ')' => {
                depth = match depth.checked_sub(1) {
                    Some(depth) => depth,
                    None => return (name, None),
                };
                if depth == 0 && index + character.len_utf8() == name.len() {
                    let Some(opening) = opening else {
                        return (name, None);
                    };
                    let display_name = name[..opening].trim();
                    let ty = name[opening + '('.len_utf8()..index].trim();

                    return if display_name.is_empty() || ty.is_empty() {
                        (name, None)
                    } else {
                        (display_name, Some(ty))
                    };
                }
            }
            _ => {}
        }
    }
    (name, None)
}

/// Calculates indentation width, advancing tabs to the next multiple of eight columns.
pub(super) fn indentation(line: &str) -> TextSize {
    TextSize::new(
        leading_indentation(line)
            .bytes()
            .fold(0u32, |column, byte| match byte {
                b'\t' => (column / 8 + 1) * 8,
                _ => column + 1,
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::is_markdown_code_span;

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
}
