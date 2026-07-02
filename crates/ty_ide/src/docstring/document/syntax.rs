use ruff_python_trivia::leading_indentation;
use ruff_source_file::UniversalNewlines;
use ruff_text_size::TextRange;

/// Collects docstring lines without their universal-newline terminators while preserving their
/// source ranges.
///
/// For example, `first\r\nsecond` yields `first` at offset 0 and `second` at offset 7.
pub(in crate::docstring) fn parsed_lines(raw: &str) -> Vec<ParsedLine<'_>> {
    raw.universal_newlines()
        .map(|line| ParsedLine {
            text: line.as_str(),
            range: line.range(),
        })
        .collect()
}

/// A docstring line and its source range, excluding the newline terminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::docstring) struct ParsedLine<'a> {
    /// The line text, excluding its newline terminator.
    pub(in crate::docstring) text: &'a str,
    /// The byte range of `text` within the source document.
    pub(in crate::docstring) range: TextRange,
}

/// Returns whether `line` starts with a `CommonMark` list-item marker.
///
/// Ordered markers are limited to nine digits, as required by `CommonMark`.
pub(in crate::docstring) fn starts_with_markdown_list_item(line: &str) -> bool {
    let bytes = line.as_bytes();
    if matches!(bytes, [b'-' | b'+' | b'*', b' ' | b'\t', ..]) {
        return true;
    }

    let digit_count = bytes
        .iter()
        .take(9)
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    digit_count > 0
        && matches!(bytes.get(digit_count), Some(b'.' | b')'))
        && matches!(bytes.get(digit_count + 1), Some(b' ' | b'\t'))
}

/// Splits the input once at the first colon outside bracket pairs and quoted strings.
pub(in crate::docstring) fn split_once_unbracketed_colon(line: &str) -> Option<(&str, &str)> {
    // Track each bracket kind independently because type expressions can nest them in any order.
    let mut parentheses = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut quote = None;
    let mut escaped = false;

    for (index, char) in line.char_indices() {
        if let Some(quote_char) = quote {
            // Brackets and colons inside a quoted literal are not structural.
            if escaped {
                escaped = false;
            } else if char == '\\' {
                escaped = true;
            } else if char == quote_char {
                quote = None;
            }
            continue;
        }

        // Saturating depth updates tolerate unmatched closing brackets in malformed input.
        match char {
            '\'' | '"' => quote = Some(char),
            '(' => parentheses += 1,
            ')' => parentheses = parentheses.saturating_sub(1),
            '[' => brackets += 1,
            ']' => brackets = brackets.saturating_sub(1),
            '{' => braces += 1,
            '}' => braces = braces.saturating_sub(1),
            ':' if parentheses == 0 && brackets == 0 && braces == 0 => {
                return Some((&line[..index], &line[index + ':'.len_utf8()..]));
            }
            _ => {}
        }
    }

    None
}

/// Splits a trailing parenthesized type from a parameter display name.
pub(in crate::docstring) fn parse_parenthesized_type(name: &str) -> (&str, Option<&str>) {
    if !name.ends_with(')') {
        return (name, None);
    }

    let mut depth = 0usize;
    let mut opening = None;
    let mut quote = None;
    let mut escaped = false;

    // Only a balanced group that closes at the end can be a type suffix.
    for (index, char) in name.char_indices() {
        if let Some(quote_char) = quote {
            if escaped {
                escaped = false;
            } else if char == '\\' {
                escaped = true;
            } else if char == quote_char {
                quote = None;
            }
            continue;
        }

        match char {
            '\'' | '"' => quote = Some(char),
            '(' => {
                if depth == 0 {
                    opening = Some(index);
                }
                depth += 1;
            }
            ')' => {
                let Some(new_depth) = depth.checked_sub(1) else {
                    return (name, None);
                };
                depth = new_depth;
                if depth == 0 && index + char.len_utf8() == name.len() {
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

/// Calculates indentation width, treating tabs like Python does.
pub(super) fn indentation(line: &str) -> usize {
    leading_indentation(line)
        .bytes()
        .fold(0, |column, byte| match byte {
            b'\t' => (column / 8 + 1) * 8,
            _ => column + 1,
        })
}
