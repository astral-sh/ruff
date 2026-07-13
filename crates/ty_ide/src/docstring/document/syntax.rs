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

/// Parses a reStructuredText directive marker and its complete argument.
pub(in crate::docstring) fn parse_rest_directive(line: &str) -> Option<RestDirective<'_>> {
    let directive = line.trim_start().strip_prefix(".. ")?;
    let (name, argument) = directive.split_once("::")?;
    if name.is_empty() || name.chars().any(char::is_whitespace) {
        return None;
    }

    Some(RestDirective {
        name,
        argument: argument.trim(),
    })
}

/// A parsed reStructuredText directive marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::docstring) struct RestDirective<'a> {
    name: &'a str,
    argument: &'a str,
}

impl<'a> RestDirective<'a> {
    /// Returns the directive name without the leading `..` or trailing `::`.
    pub(in crate::docstring) const fn name(self) -> &'a str {
        self.name
    }

    /// Returns all text following the directive marker on the same line.
    pub(in crate::docstring) const fn argument(self) -> &'a str {
        self.argument
    }

    /// Returns whether this directive has the given case-insensitive name.
    pub(in crate::docstring) fn is_named(self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }

    /// Returns how the directive's content should be rendered.
    pub(in crate::docstring) fn kind(self) -> RestDirectiveKind {
        if self.has_name_in(&[
            "code",
            "code-block",
            "sourcecode",
            "doctest",
            "testcode",
            "testsetup",
            "testcleanup",
            "testoutput",
        ]) {
            RestDirectiveKind::Code
        } else if self.has_name_in(&[
            "math",
            "parsed-literal",
            "raw",
            "csv-table",
            "productionlist",
            "graphviz",
        ]) {
            RestDirectiveKind::Preformatted
        } else if self.has_name_in(&[
            "image",
            "contents",
            "sectnum",
            "section-numbering",
            "target-notes",
            "include",
            "unicode",
            "default-role",
            "title",
            "highlight",
            "literalinclude",
            "sectionauthor",
            "moduleauthor",
            "codeauthor",
            "tabularcolumns",
            "default-domain",
            "currentmodule",
            "program",
            "index",
        ]) {
            RestDirectiveKind::Control
        } else {
            RestDirectiveKind::Prose
        }
    }

    fn has_name_in(self, names: &[&str]) -> bool {
        names.iter().any(|name| self.is_named(name))
    }
}

/// Describes how a reStructuredText directive's content should be rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::docstring) enum RestDirectiveKind {
    /// Source code or interactive examples.
    Code,
    /// Whitespace-sensitive content that is not necessarily source code.
    Preformatted,
    /// Content parsed as ordinary reStructuredText body elements.
    Prose,
    /// A directive that configures the document without introducing a content block.
    Control,
}

impl RestDirectiveKind {
    /// Returns whether the directive introduces content whose whitespace must be preserved.
    pub(in crate::docstring) const fn is_preformatted(self) -> bool {
        matches!(self, Self::Code | Self::Preformatted)
    }
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

/// Returns the end of an indented Markdown or reStructuredText container block.
pub(super) fn container_block_end(lines: &[ParsedLine<'_>], index: usize) -> Option<usize> {
    let marker = lines.get(index)?;
    if parse_rest_directive(marker.text).is_none()
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
    use super::{RestDirectiveKind, parse_rest_directive};

    #[test]
    fn parses_complete_rest_directive_arguments() {
        let directive = parse_rest_directive("  .. math:: x^2 + y^2 = z^2").unwrap();

        assert_eq!(directive.name(), "math");
        assert_eq!(directive.argument(), "x^2 + y^2 = z^2");
        assert_eq!(directive.kind(), RestDirectiveKind::Preformatted);
    }

    #[test]
    fn classifies_rest_directives() {
        for (name, expected) in [
            ("code-block", RestDirectiveKind::Code),
            ("doctest", RestDirectiveKind::Code),
            ("parsed-literal", RestDirectiveKind::Preformatted),
            ("MATH", RestDirectiveKind::Preformatted),
            ("productionlist", RestDirectiveKind::Preformatted),
            ("warning", RestDirectiveKind::Prose),
            ("seealso", RestDirectiveKind::Prose),
            ("custom-directive", RestDirectiveKind::Prose),
            ("highlight", RestDirectiveKind::Control),
            ("literalinclude", RestDirectiveKind::Control),
        ] {
            let marker = format!(".. {name}::");
            assert_eq!(parse_rest_directive(&marker).unwrap().kind(), expected);
        }
    }

    #[test]
    fn rejects_invalid_rest_directive_markers() {
        assert_eq!(parse_rest_directive(".. missing-colons"), None);
        assert_eq!(parse_rest_directive(".. two words::"), None);
        assert_eq!(parse_rest_directive("prose .. warning::"), None);
    }
}
