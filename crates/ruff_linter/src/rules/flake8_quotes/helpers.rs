use ruff_python_ast::{AnyStringFlags, StringFlags};
use ruff_text_size::TextLen;

/// Returns the raw contents of the string given the string's contents and flags.
/// This is a string without the prefix and quotes.
pub(super) fn raw_contents(contents: &str, flags: AnyStringFlags) -> &str {
    &contents[flags.opener_len().to_usize()..(contents.text_len() - flags.closer_len()).to_usize()]
}

/// Return `true` if the haystack contains an escaped quote.
pub(super) fn contains_escaped_quote(haystack: &str, quote: char) -> bool {
    for index in memchr::memchr_iter(quote as u8, haystack.as_bytes()) {
        // If the quote is preceded by an even number of backslashes, it's not escaped.
        if haystack.as_bytes()[..index]
            .iter()
            .rev()
            .take_while(|&&c| c == b'\\')
            .count()
            % 2
            != 0
        {
            return true;
        }
    }
    false
}

/// Return a modified version of the string with all quote escapes removed.
pub(super) fn unescape_string(haystack: &str, quote: char) -> String {
    let mut fixed_contents = String::with_capacity(haystack.len());

    let mut chars = haystack.chars().peekable();
    let mut backslashes = 0;
    while let Some(char) = chars.next() {
        if char != '\\' {
            fixed_contents.push(char);
            backslashes = 0;
            continue;
        }
        // If we're at the end of the line
        let Some(next_char) = chars.peek() else {
            fixed_contents.push(char);
            continue;
        };
        // Remove quote escape
        if *next_char == quote && backslashes % 2 == 0 {
            backslashes = 0;
            continue;
        }
        backslashes += 1;
        fixed_contents.push(char);
    }

    fixed_contents
}
