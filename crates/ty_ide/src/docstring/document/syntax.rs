use ruff_python_trivia::leading_indentation;
use ruff_source_file::UniversalNewlines;
use ruff_text_size::{TextRange, TextSize};

use super::rst::is_field_list_marker;

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

    let digit_count = bytes
        .iter()
        .take(9)
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    digit_count > 0
        && matches!(bytes.get(digit_count), Some(b'.' | b')'))
        && matches!(bytes.get(digit_count + 1), Some(b' ' | b'\t'))
}

/// Returns the end of an indented Markdown or reStructuredText container block.
pub(in crate::docstring) fn indented_container_end(
    lines: &[ParsedLine<'_>],
    index: usize,
) -> Option<usize> {
    let marker = lines.get(index)?;
    if !is_rest_directive_marker(marker.text)
        && !is_field_list_marker(marker.text)
        && !starts_with_markdown_list_item(marker.text.trim_start())
    {
        return None;
    }

    // Container membership is determined before PEP 257 normalization. This keeps raw-indented
    // section-like text inside lists, directives, and field-list entries.
    Some(
        (index + 1..lines.len())
            .find(|&end| {
                let line = lines[end];
                !line.text.trim().is_empty() && line.raw_indent <= marker.raw_indent
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
pub(in crate::docstring) fn split_once_unbracketed_colon(line: &str) -> Option<(&str, &str)> {
    // Track each bracket kind independently because type expressions can nest them in any order.
    let mut parentheses = 0usize;
    let mut brackets = 0usize;
    let mut braces = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut code_span_delimiter_len = None;
    let mut index = 0;

    while index < line.len() {
        let rest = &line[index..];
        let char = rest.chars().next()?;
        if let Some(opening_len) = code_span_delimiter_len {
            if char == '`' {
                let delimiter_len = rest.bytes().take_while(|byte| *byte == b'`').count();
                if opening_len == delimiter_len {
                    code_span_delimiter_len = None;
                }
                index += delimiter_len;
            } else {
                index += char.len_utf8();
            }
            continue;
        }

        if let Some(quote_char) = quote {
            // Brackets and colons inside a quoted literal are not structural.
            if escaped {
                escaped = false;
            } else if char == '\\' {
                escaped = true;
            } else if char == quote_char {
                quote = None;
            }
            index += char.len_utf8();
            continue;
        }

        if char == '`' {
            let delimiter_len = rest.bytes().take_while(|byte| *byte == b'`').count();
            code_span_delimiter_len = Some(delimiter_len);
            index += delimiter_len;
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
        index += char.len_utf8();
    }

    None
}

/// Splits a trailing parenthesized type from a parameter display name.
pub(in crate::docstring) fn parse_parenthesized_type(name: &str) -> (&str, Option<&str>) {
    let Some((display_name, ty)) = split_parenthesized_suffix(name) else {
        return (name, None);
    };
    let display_name = display_name.trim();
    let ty = ty.trim();
    if display_name.is_empty() || ty.is_empty() {
        (name, None)
    } else {
        (display_name, Some(ty))
    }
}

fn split_parenthesized_suffix(value: &str) -> Option<(&str, &str)> {
    if !value.ends_with(')') {
        return None;
    }

    let mut depth = 0usize;
    let mut opening = None;
    let mut quote = None;
    let mut escaped = false;

    // Only a balanced group that closes at the end can be a type suffix.
    for (index, char) in value.char_indices() {
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
                let new_depth = depth.checked_sub(1)?;
                depth = new_depth;
                if depth == 0 && index + char.len_utf8() == value.len() {
                    let opening = opening?;
                    return Some((&value[..opening], &value[opening + '('.len_utf8()..index]));
                }
            }
            _ => {}
        }
    }

    None
}

/// Removes a balanced Markdown code-span wrapper from a documented type.
pub(in crate::docstring) fn strip_code_span_wrapper(ty: &str) -> &str {
    let delimiter_len = ty.bytes().take_while(|byte| *byte == b'`').count();
    let closing_delimiter_len = ty.bytes().rev().take_while(|byte| *byte == b'`').count();
    if delimiter_len == 0 || delimiter_len != closing_delimiter_len || delimiter_len > ty.len() / 2
    {
        return ty;
    }

    let inner = &ty[delimiter_len..ty.len() - delimiter_len];
    if inner
        .split(|character| character != '`')
        .any(|run| run.len() == delimiter_len)
    {
        return ty;
    }

    let inner = inner.trim();
    if inner.is_empty() { ty } else { inner }
}

/// Returns whether `ty` resembles a type expression used in docstrings.
pub(in crate::docstring) fn is_docstring_type_expression(ty: &str) -> bool {
    let ty = strip_code_span_wrapper(ty);
    if !has_docstring_type_expression_characters(ty) {
        return false;
    }

    if !ty.chars().any(char::is_whitespace) {
        return true;
    }

    // Whitespace makes prose ambiguous, so require syntax that strongly indicates a type.
    is_subscript_style_docstring_type_expression(ty)
        || ty.contains('|')
        || is_call_style_docstring_type_expression(ty)
        || is_conventional_spaced_docstring_type_expression(ty)
}

fn is_subscript_style_docstring_type_expression(ty: &str) -> bool {
    ty.split_once('[')
        .is_some_and(|(name, _)| is_docstring_type_expression_atom(name) && ty.ends_with(']'))
}

fn is_call_style_docstring_type_expression(ty: &str) -> bool {
    split_parenthesized_suffix(ty).is_some_and(|(name, arguments)| {
        !name.contains(['(', ')'])
            && is_docstring_type_expression_atom(name)
            && arguments
                .split(',')
                .all(|argument| is_docstring_type_expression_atom(argument.trim()))
    })
}

fn is_conventional_spaced_docstring_type_expression(ty: &str) -> bool {
    let mut tokens = ty.split_whitespace();
    let Some(first) = tokens.next() else {
        return false;
    };
    if !is_docstring_type_expression_atom(first) {
        return false;
    }

    let mut found_connector = false;
    while let Some(connector) = tokens.next() {
        if !matches!(connector, "of" | "or") {
            return false;
        }
        let Some(atom) = tokens.next() else {
            return false;
        };
        if !is_docstring_type_expression_atom(atom) {
            return false;
        }
        found_connector = true;
    }

    found_connector
}

fn is_docstring_type_expression_atom(atom: &str) -> bool {
    !atom.chars().any(char::is_whitespace) && has_docstring_type_expression_characters(atom)
}

fn has_docstring_type_expression_characters(expression: &str) -> bool {
    expression
        .chars()
        .next()
        .is_some_and(is_docstring_type_expression_start)
        && expression.chars().all(is_docstring_type_expression_char)
}

fn is_docstring_type_expression_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || matches!(ch, '_' | '~' | ':' | '`' | '(')
}

fn is_docstring_type_expression_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || "_.[](){},|\"':/ `~-".contains(ch)
}

/// Calculates indentation width, treating tabs like Python does.
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
