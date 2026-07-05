use ruff_python_trivia::leading_indentation;
use ruff_source_file::UniversalNewlines;
use ruff_text_size::{TextRange, TextSize};

/// Collects docstring lines without their universal-newline terminators while preserving their
/// source ranges.
///
/// For example, `first\r\nsecond` yields `first` at offset 0 and `second` at offset 7.
pub(in crate::docstring) fn parsed_lines(raw: &str) -> Vec<ParsedLine<'_>> {
    let mut lines = raw
        .universal_newlines()
        .map(|line| ParsedLine {
            text: line.as_str(),
            range: line.range(),
            raw_indent: indentation(line.as_str()),
            structural_indent: TextSize::new(0),
        })
        .collect::<Vec<_>>();

    // PEP 257 ignores indentation on the first physical line and removes the common margin from
    // all later lines. Keep the raw indentation as well because item and block indentation can
    // still disambiguate content within a section.
    let continuation_margin = lines
        .iter()
        .skip(1)
        .filter(|line| !line.text.trim().is_empty())
        .map(|line| line.raw_indent)
        .min()
        .unwrap_or(TextSize::new(0));
    for line in lines.iter_mut().skip(1) {
        line.structural_indent = line.raw_indent.saturating_sub(continuation_margin);
    }

    lines
}

/// A docstring line and its source range, excluding the newline terminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::docstring) struct ParsedLine<'a> {
    /// The line text, excluding its newline terminator.
    pub(in crate::docstring) text: &'a str,
    /// The byte range of `text` within the source document.
    pub(in crate::docstring) range: TextRange,
    /// The indentation in the decoded docstring text.
    pub(in crate::docstring) raw_indent: TextSize,
    /// The indentation after removing the PEP 257 continuation margin.
    pub(in crate::docstring) structural_indent: TextSize,
}

/// Returns whether `line` starts with a `CommonMark` list-item marker.
///
/// Ordered markers are limited to nine digits, as required by `CommonMark`.
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

/// Splits the input once at the first colon outside bracket pairs and quoted strings.
pub(in crate::docstring) fn split_once_unbracketed_colon(line: &str) -> Option<(&str, &str)> {
    let mut depths = [0usize; 3];
    let mut quote = None;
    let mut escaped = false;

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
pub(in crate::docstring) fn indentation(line: &str) -> TextSize {
    TextSize::new(
        leading_indentation(line)
            .bytes()
            .fold(0u32, |column, byte| match byte {
                b'\t' => (column / 8 + 1) * 8,
                _ => column + 1,
            }),
    )
}
