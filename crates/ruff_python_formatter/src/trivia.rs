use ruff_python_ast::whitespace::is_python_whitespace;
use ruff_text_size::{TextLen, TextRange, TextSize};

/// Searches for the first non-trivia character in `range`.
///
/// The search skips over any whitespace and comments.
///
/// Returns `Some` if the range contains any non-trivia character. The first item is the absolute offset
/// of the character, the second item the non-trivia character.
///
/// Returns `None` if the range is empty or only contains trivia (whitespace or comments).
pub(crate) fn find_first_non_trivia_character_in_range(
    range: TextRange,
    code: &str,
) -> Option<(TextSize, char)> {
    let rest = &code[range];
    let mut char_iter = rest.chars();

    while let Some(c) = char_iter.next() {
        match c {
            '#' => {
                // We're now inside of a comment. Skip all content until the end of the line
                for c in char_iter.by_ref() {
                    if matches!(c, '\n' | '\r') {
                        break;
                    }
                }
            }
            c => {
                if !is_python_whitespace(c) {
                    let index = range.start() + rest.text_len()
                        - char_iter.as_str().text_len()
                        - c.text_len();
                    return Some((index, c));
                }
            }
        }
    }

    None
}

pub(crate) fn find_first_non_trivia_character_after(
    offset: TextSize,
    code: &str,
) -> Option<(TextSize, char)> {
    find_first_non_trivia_character_in_range(TextRange::new(offset, code.text_len()), code)
}

pub(crate) fn find_first_non_trivia_character_before(
    offset: TextSize,
    code: &str,
) -> Option<(TextSize, char)> {
    let head = &code[TextRange::up_to(offset)];
    let mut char_iter = head.chars();

    while let Some(c) = char_iter.next_back() {
        match c {
            c if is_python_whitespace(c) => {
                continue;
            }

            // Empty comment
            '#' => continue,

            non_trivia_character => {
                // Non trivia character but we don't know if it is a comment or not. Consume all characters
                // until the start of the line and track if the last non-whitespace character was a `#`.
                let mut is_comment = false;

                let first_non_trivia_offset = char_iter.as_str().text_len();

                while let Some(c) = char_iter.next_back() {
                    match c {
                        '#' => {
                            is_comment = true;
                        }
                        '\n' | '\r' => {
                            if !is_comment {
                                return Some((first_non_trivia_offset, non_trivia_character));
                            }
                        }

                        c => {
                            if !is_python_whitespace(c) {
                                is_comment = false;
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Returns the number of newlines between `offset` and the first non whitespace character in the source code.
pub(crate) fn lines_before(offset: TextSize, code: &str) -> u32 {
    let head = &code[TextRange::up_to(offset)];
    let mut newlines = 0u32;

    for (index, c) in head.char_indices().rev() {
        match c {
            '\n' => {
                if head.as_bytes()[index.saturating_sub(1)] == b'\r' {
                    continue;
                }
                newlines += 1;
            }

            '\r' => {
                newlines += 1;
            }

            c if is_python_whitespace(c) => continue,

            _ => break,
        }
    }

    newlines
}

/// Counts the empty lines between `offset` and the first non-whitespace character.
pub(crate) fn lines_after(offset: TextSize, code: &str) -> u32 {
    let rest = &code[usize::from(offset)..];
    let mut newlines = 0;

    for (index, c) in rest.char_indices() {
        match c {
            '\n' => {
                newlines += 1;
            }
            '\r' if rest.as_bytes().get(index + 1).copied() == Some(b'\n') => {
                continue;
            }
            '\r' => {
                newlines += 1;
            }
            c if is_python_whitespace(c) => continue,
            _ => break,
        }
    }

    newlines
}

#[cfg(test)]
mod tests {
    use crate::trivia::{lines_after, lines_before};
    use ruff_text_size::TextSize;

    #[test]
    fn lines_before_empty_string() {
        assert_eq!(lines_before(TextSize::new(0), ""), 0);
    }

    #[test]
    fn lines_before_in_the_middle_of_a_line() {
        assert_eq!(lines_before(TextSize::new(4), "a = 20"), 0);
    }

    #[test]
    fn lines_before_on_a_new_line() {
        assert_eq!(lines_before(TextSize::new(7), "a = 20\nb = 10"), 1);
    }

    #[test]
    fn lines_before_multiple_leading_newlines() {
        assert_eq!(lines_before(TextSize::new(9), "a = 20\n\r\nb = 10"), 2);
    }

    #[test]
    fn lines_before_with_comment_offset() {
        assert_eq!(lines_before(TextSize::new(8), "a = 20\n# a comment"), 0);
    }

    #[test]
    fn lines_before_with_trailing_comment() {
        assert_eq!(
            lines_before(TextSize::new(22), "a = 20 # some comment\nb = 10"),
            1
        );
    }

    #[test]
    fn lines_before_with_comment_only_line() {
        assert_eq!(
            lines_before(TextSize::new(22), "a = 20\n# some comment\nb = 10"),
            1
        );
    }

    #[test]
    fn lines_after_empty_string() {
        assert_eq!(lines_after(TextSize::new(0), ""), 0);
    }

    #[test]
    fn lines_after_in_the_middle_of_a_line() {
        assert_eq!(lines_after(TextSize::new(4), "a = 20"), 0);
    }

    #[test]
    fn lines_after_before_a_new_line() {
        assert_eq!(lines_after(TextSize::new(6), "a = 20\nb = 10"), 1);
    }

    #[test]
    fn lines_after_multiple_newlines() {
        assert_eq!(lines_after(TextSize::new(6), "a = 20\n\r\nb = 10"), 2);
    }

    #[test]
    fn lines_after_before_comment_offset() {
        assert_eq!(lines_after(TextSize::new(7), "a = 20 # a comment\n"), 0);
    }

    #[test]
    fn lines_after_with_comment_only_line() {
        assert_eq!(
            lines_after(TextSize::new(6), "a = 20\n# some comment\nb = 10"),
            1
        );
    }
}
