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
    code: &str,
    range: TextRange,
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

/// Returns the number of newlines between `offset` and the first non whitespace character in the source code.
#[allow(unused)] // TODO(micha) Remove after using for statements.
pub(crate) fn lines_before(code: &str, offset: TextSize) -> u32 {
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

#[cfg(test)]
mod tests {
    use crate::trivia::lines_before;
    use ruff_text_size::TextSize;

    #[test]
    fn lines_before_empty_string() {
        assert_eq!(lines_before("", TextSize::new(0)), 0);
    }

    #[test]
    fn lines_before_in_the_middle_of_a_line() {
        assert_eq!(lines_before("a = 20", TextSize::new(4)), 0);
    }

    #[test]
    fn lines_before_on_a_new_line() {
        assert_eq!(lines_before("a = 20\nb = 10", TextSize::new(7)), 1);
    }

    #[test]
    fn lines_before_multiple_leading_newlines() {
        assert_eq!(lines_before("a = 20\n\r\nb = 10", TextSize::new(9)), 2);
    }

    #[test]
    fn lines_before_with_comment_offset() {
        assert_eq!(lines_before("a = 20\n# a comment", TextSize::new(8)), 0);
    }

    #[test]
    fn lines_before_with_trailing_comment() {
        assert_eq!(
            lines_before("a = 20 # some comment\nb = 10", TextSize::new(22)),
            1
        );
    }

    #[test]
    fn lines_before_with_comment_only_line() {
        assert_eq!(
            lines_before("a = 20\n# some comment\nb = 10", TextSize::new(22)),
            1
        );
    }
}
