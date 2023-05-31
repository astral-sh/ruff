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
