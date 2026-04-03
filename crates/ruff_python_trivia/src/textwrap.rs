//! Functions related to adding and removing indentation from lines of
//! text.

use std::borrow::Cow;
use std::cmp;

use ruff_source_file::UniversalNewlines;

use crate::PythonWhitespace;

/// Indent each line by the given prefix.
///
/// # Examples
///
/// ```
/// # use ruff_python_trivia::textwrap::indent;
///
/// assert_eq!(indent("First line.\nSecond line.\n", "  "),
///            "  First line.\n  Second line.\n");
/// ```
///
/// When indenting, trailing whitespace is stripped from the prefix.
/// This means that empty lines remain empty afterwards:
///
/// ```
/// # use ruff_python_trivia::textwrap::indent;
///
/// assert_eq!(indent("First line.\n\n\nSecond line.\n", "  "),
///            "  First line.\n\n\n  Second line.\n");
/// ```
///
/// Notice how `"\n\n\n"` remained as `"\n\n\n"`.
///
/// This feature is useful when you want to indent text and have a
/// space between your prefix and the text. In this case, you _don't_
/// want a trailing space on empty lines:
///
/// ```
/// # use ruff_python_trivia::textwrap::indent;
///
/// assert_eq!(indent("foo = 123\n\nprint(foo)\n", "# "),
///            "# foo = 123\n#\n# print(foo)\n");
/// ```
///
/// Notice how `"\n\n"` became `"\n#\n"` instead of `"\n# \n"` which
/// would have trailing whitespace.
///
/// Leading and trailing whitespace coming from the text itself is
/// kept unchanged:
///
/// ```
/// # use ruff_python_trivia::textwrap::indent;
///
/// assert_eq!(indent(" \t  Foo   ", "->"), "-> \t  Foo   ");
/// ```
pub fn indent<'a>(text: &'a str, prefix: &str) -> Cow<'a, str> {
    if prefix.is_empty() {
        return Cow::Borrowed(text);
    }

    let mut result = String::with_capacity(text.len() + prefix.len());
    let trimmed_prefix = prefix.trim_whitespace_end();
    for line in text.universal_newlines() {
        if line.trim_whitespace().is_empty() {
            result.push_str(trimmed_prefix);
        } else {
            result.push_str(prefix);
        }
        result.push_str(line.as_full_str());
    }
    Cow::Owned(result)
}

/// Indent only the first line by the given prefix.
///
/// This function is useful when you want to indent the first line of a multi-line
/// expression while preserving the relative indentation of subsequent lines.
///
/// # Examples
///
/// ```
/// # use ruff_python_trivia::textwrap::indent_first_line;
///
/// assert_eq!(indent_first_line("First line.\nSecond line.\n", "  "),
///            "  First line.\nSecond line.\n");
/// ```
///
/// When indenting, trailing whitespace is stripped from the prefix.
/// This means that empty lines remain empty afterwards:
///
/// ```
/// # use ruff_python_trivia::textwrap::indent_first_line;
///
/// assert_eq!(indent_first_line("\n\n\nSecond line.\n", "  "),
///            "\n\n\nSecond line.\n");
/// ```
///
/// Leading and trailing whitespace coming from the text itself is
/// kept unchanged:
///
/// ```
/// # use ruff_python_trivia::textwrap::indent_first_line;
///
/// assert_eq!(indent_first_line(" \t  Foo   ", "->"), "-> \t  Foo   ");
/// ```
pub fn indent_first_line<'a>(text: &'a str, prefix: &str) -> Cow<'a, str> {
    if prefix.is_empty() {
        return Cow::Borrowed(text);
    }

    let mut lines = text.universal_newlines();
    let Some(first_line) = lines.next() else {
        return Cow::Borrowed(text);
    };

    let mut result = String::with_capacity(text.len() + prefix.len());

    // Indent only the first line
    if first_line.trim_whitespace().is_empty() {
        result.push_str(prefix.trim_whitespace_end());
    } else {
        result.push_str(prefix);
    }
    result.push_str(first_line.as_full_str());

    // Add remaining lines without indentation
    for line in lines {
        result.push_str(line.as_full_str());
    }

    Cow::Owned(result)
}

/// Removes common leading whitespace from each line.
///
/// This function will look at each non-empty line and determine the
/// maximum amount of whitespace that can be removed from all lines.
///
/// Lines that consist solely of whitespace are trimmed to a blank line.
///
/// ```
/// # use ruff_python_trivia::textwrap::dedent;
///
/// assert_eq!(dedent("
///     1st line
///       2nd line
///     3rd line
/// "), "
/// 1st line
///   2nd line
/// 3rd line
/// ");
/// ```
pub fn dedent(text: &str) -> Cow<'_, str> {
    // Find the minimum amount of leading whitespace on each line.
    let prefix_len = text
        .universal_newlines()
        .fold(usize::MAX, |prefix_len, line| {
            let leading_whitespace_len = line.len() - line.trim_whitespace_start().len();
            if leading_whitespace_len == line.len() {
                // Skip empty lines.
                prefix_len
            } else {
                cmp::min(prefix_len, leading_whitespace_len)
            }
        });

    // If there is no common prefix, no need to dedent.
    if prefix_len == usize::MAX {
        return Cow::Borrowed(text);
    }

    // Remove the common prefix from each line.
    let mut result = String::with_capacity(text.len());
    for line in text.universal_newlines() {
        if line.trim_whitespace().is_empty() {
            if let Some(line_ending) = line.line_ending() {
                result.push_str(&line_ending);
            }
        } else {
            result.push_str(&line.as_full_str()[prefix_len..]);
        }
    }
    Cow::Owned(result)
}

/// Reduce a block's indentation to match the provided indentation.
///
/// This function looks at the first line in the block to determine the
/// current indentation, then removes whitespace from each line to
/// match the provided indentation.
///
/// Leading comments are ignored unless the block is only composed of comments.
///
/// Lines that are indented by _less_ than the indent of the first line
/// are left unchanged.
///
/// Lines that consist solely of whitespace are trimmed to a blank line.
///
/// Lines that start with formfeeds have the indentation after the formfeeds
/// removed and the formfeeds reinstated
///
/// # Panics
/// If the first line is indented by less than the provided indent.
pub fn dedent_to(text: &str, indent: &str) -> Option<String> {
    // The caller may provide an `indent` from source code by taking
    // a range of text beginning with the start of a line. In Python,
    // while a line may begin with form feeds, these do not contribute
    // to the indentation. So we strip those here.
    let indent = indent.trim_start_matches('\x0C');
    // Look at the indentation of the first non-empty line, to determine the "baseline" indentation.
    let mut first_comment_indent = None;
    let existing_indent_len = text
        .universal_newlines()
        .find_map(|line| {
            // Following Python's lexer, treat form feed character's at the start of a line
            // the same as a line break (reset the indentation)
            let trimmed_start_of_line_formfeed = line.trim_start_matches('\x0C');
            let trimmed = trimmed_start_of_line_formfeed.trim_whitespace_start();

            // A whitespace only line
            if trimmed.is_empty() {
                return None;
            }

            let indent_len = trimmed_start_of_line_formfeed.len() - trimmed.len();

            if trimmed.starts_with('#') && first_comment_indent.is_none() {
                first_comment_indent = Some(indent_len);
                None
            } else {
                Some(indent_len)
            }
        })
        .unwrap_or(first_comment_indent.unwrap_or_default());

    if existing_indent_len < indent.len() {
        return None;
    }

    // Determine the amount of indentation to remove.
    let dedent_len = existing_indent_len - indent.len();

    let mut result = String::with_capacity(text.len() + indent.len());

    for line in text.universal_newlines() {
        let line_content = line.trim_start_matches('\x0C');
        let formfeed_count = line.len() - line_content.len();

        let line_ending = if let Some(line_ending) = line.line_ending() {
            line_ending.as_str()
        } else {
            ""
        };

        let line_without_indent = line.trim_whitespace_start();

        if line_without_indent.is_empty() {
            result.push_str(line_ending);
            continue;
        }

        // Determine the current indentation level.
        let current_indent_len = line_content.len() - line_without_indent.len();

        if current_indent_len < existing_indent_len {
            // If the current indentation level is less than the baseline, keep it as is.
            result.push_str(line.as_full_str());
            continue;
        }
        let dedented_content = &line_content[dedent_len..];

        let formfeeds = &line[..formfeed_count];
        result.push_str(formfeeds);
        result.push_str(dedented_content);
        result.push_str(line_ending);
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indent_empty() {
        assert_eq!(indent("\n", "  "), "\n");
    }

    #[test]
    #[rustfmt::skip]
    fn indent_nonempty() {
        let text = [
            "  foo\n",
            "bar\n",
            "  baz\n",
        ].join("");
        let expected = [
            "//   foo\n",
            "// bar\n",
            "//   baz\n",
        ].join("");
        assert_eq!(indent(&text, "// "), expected);
    }

    #[test]
    #[rustfmt::skip]
    fn indent_empty_line() {
        let text = [
            "  foo",
            "bar",
            "",
            "  baz",
        ].join("\n");
        let expected = [
            "//   foo",
            "// bar",
            "//",
            "//   baz",
        ].join("\n");
        assert_eq!(indent(&text, "// "), expected);
    }

    #[test]
    #[rustfmt::skip]
    fn indent_mixed_newlines() {
        let text = [
            "  foo\r\n",
            "bar\n",
            "  baz\r",
        ].join("");
        let expected = [
            "//   foo\r\n",
            "// bar\n",
            "//   baz\r",
        ].join("");
        assert_eq!(indent(&text, "// "), expected);
    }

    #[test]
    fn dedent_empty() {
        assert_eq!(dedent(""), "");
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_multi_line() {
        let x = [
            "    foo",
            "  bar",
            "    baz",
        ].join("\n");
        let y = [
            "  foo",
            "bar",
            "  baz"
        ].join("\n");
        assert_eq!(dedent(&x), y);
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_empty_line() {
        let x = [
            "    foo",
            "  bar",
            "   ",
            "    baz"
        ].join("\n");
        let y = [
            "  foo",
            "bar",
            "",
            "  baz"
        ].join("\n");
        assert_eq!(dedent(&x), y);
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_blank_line() {
        let x = [
            "      foo",
            "",
            "        bar",
            "          foo",
            "          bar",
            "          baz",
        ].join("\n");
        let y = [
            "foo",
            "",
            "  bar",
            "    foo",
            "    bar",
            "    baz",
        ].join("\n");
        assert_eq!(dedent(&x), y);
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_whitespace_line() {
        let x = [
            "      foo",
            " ",
            "        bar",
            "          foo",
            "          bar",
            "          baz",
        ].join("\n");
        let y = [
            "foo",
            "",
            "  bar",
            "    foo",
            "    bar",
            "    baz",
        ].join("\n");
        assert_eq!(dedent(&x), y);
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_mixed_whitespace() {
        let x = [
            "\tfoo",
            "  bar",
        ].join("\n");
        let y = [
            "foo",
            " bar",
        ].join("\n");
        assert_eq!(dedent(&x), y);
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_tabbed_whitespace() {
        let x = [
            "\t\tfoo",
            "\t\t\tbar",
        ].join("\n");
        let y = [
            "foo",
            "\tbar",
        ].join("\n");
        assert_eq!(dedent(&x), y);
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_mixed_tabbed_whitespace() {
        let x = [
            "\t  \tfoo",
            "\t  \t\tbar",
        ].join("\n");
        let y = [
            "foo",
            "\tbar",
        ].join("\n");
        assert_eq!(dedent(&x), y);
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_preserve_no_terminating_newline() {
        let x = [
            "  foo",
            "    bar",
        ].join("\n");
        let y = [
            "foo",
            "  bar",
        ].join("\n");
        assert_eq!(dedent(&x), y);
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_mixed_newlines() {
        let x = [
            "    foo\r\n",
            "  bar\n",
            "    baz\r",
        ].join("");
        let y = [
            "  foo\r\n",
            "bar\n",
            "  baz\r"
        ].join("");
        assert_eq!(dedent(&x), y);
    }

    #[test]
    fn dedent_non_python_whitespace() {
        let text = r"        C = int(f.rea1,0],[-1,0,1]],
              [[-1,-1,1],[1,1,-1],[0,-1,0]],
              [[-1,-1,-1],[1,1,0],[1,0,1]]
             ]";
        assert_eq!(dedent(text), text);
    }

    #[test]
    fn indent_first_line_empty() {
        assert_eq!(indent_first_line("\n", "  "), "\n");
    }

    #[test]
    #[rustfmt::skip]
    fn indent_first_line_nonempty() {
        let text = [
            "  foo\n",
            "bar\n",
            "  baz\n",
        ].join("");
        let expected = [
            "//   foo\n",
            "bar\n",
            "  baz\n",
        ].join("");
        assert_eq!(indent_first_line(&text, "// "), expected);
    }

    #[test]
    #[rustfmt::skip]
    fn indent_first_line_empty_line() {
        let text = [
            "  foo",
            "bar",
            "",
            "  baz",
        ].join("\n");
        let expected = [
            "//   foo",
            "bar",
            "",
            "  baz",
        ].join("\n");
        assert_eq!(indent_first_line(&text, "// "), expected);
    }

    #[test]
    #[rustfmt::skip]
    fn indent_first_line_mixed_newlines() {
        let text = [
            "  foo\r\n",
            "bar\n",
            "  baz\r",
        ].join("");
        let expected = [
            "//   foo\r\n",
            "bar\n",
            "  baz\r",
        ].join("");
        assert_eq!(indent_first_line(&text, "// "), expected);
    }

    #[test]
    #[rustfmt::skip]
    fn adjust_indent() {
        let x = [
            "    foo",
            "  bar",
            "   ",
            "    baz"
        ].join("\n");
        let y = [
            "  foo",
            "  bar",
            "",
            "  baz"
        ].join("\n");
        assert_eq!(dedent_to(&x, "  "), Some(y));

        let x = [
            "    foo",
            "        bar",
            "    baz",
        ].join("\n");
        let y = [
            "foo",
            "    bar",
            "baz"
        ].join("\n");
        assert_eq!(dedent_to(&x, ""), Some(y));

        let x = [
            "  # foo",
            "    # bar",
            "# baz"
        ].join("\n");
        let y = [
            "  # foo",
            "  # bar",
            "# baz"
        ].join("\n");
        assert_eq!(dedent_to(&x, "  "), Some(y));

        let x = [
            "  # foo",
            "    bar",
            "      baz"
        ].join("\n");
        let y = [
            "  # foo",
            "  bar",
            "    baz"
        ].join("\n");
        assert_eq!(dedent_to(&x, "  "), Some(y));

        let x = [
            "\x0C    1",
            "    2"
        ].join("\n");
        let y = [
            "\x0C1",
            "2"
        ].join("\n");
        assert_eq!(dedent_to(&x, ""), Some(y));
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_to_returns_none_if_indent_too_large() {
        let x = [
            "    foo",
            "    bar"
        ].join("\n");
        assert_eq!(dedent_to(&x, "      "), None);
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_to_only_whitespace_lines() {
        let x = [
            "   ",
            "\t",
            "  "
        ].join("\n");
        let y = "\n\n".to_string();
        assert_eq!(dedent_to(&x, ""), Some(y));
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_to_preserves_crlf_for_lines_starting_with_form_feed() {
        let x = [
            "\x0C    1\r\n",
            "    2\r\n",
        ].join("");
        let y = [
            "\x0C1\r\n",
            "2\r\n",
        ].join("");
        assert_eq!(dedent_to(&x, ""), Some(y));
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_to_preserves_multiple_leading_form_feeds_on_first_line() {
        let x = [
            "\x0C\x0C    1",
            "    2",
        ].join("\n");
        let y = [
            "\x0C\x0C1",
            "2",
        ].join("\n");
        assert_eq!(dedent_to(&x, ""), Some(y));
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_to_preserves_multiple_leading_form_feeds_on_second_line() {
        let x = [
            "    1",
            "\x0C\x0C    2",
        ].join("\n");
        let y = [
            "1",
            "\x0C\x0C2",
        ].join("\n");
        assert_eq!(dedent_to(&x, ""), Some(y));
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_to_handles_when_multiple_leading_form_feeds_greater_than_dedent_len() {
        let x = [
            "\x0C\x0C\x0C\x0C  1",
            "  2",
        ].join("\n");
        let y = [
            "\x0C\x0C\x0C\x0C1",
            "2",
        ].join("\n");
        assert_eq!(dedent_to(&x, ""), Some(y));
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_to_ignores_leading_form_feeds_when_checking_indentation() {
        let x = [
            "    1",
            "\x0C\x0C  2",
        ].join("\n");
        let y = [
            "1",
            "\x0C\x0C  2",
        ].join("\n");
        assert_eq!(dedent_to(&x, ""), Some(y));
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_to_is_idempotent() {
        let x = [
            "    foo",
            "  bar",
            "   ",
            "    baz"
        ].join("\n");
        let y = [
            "  foo",
            "  bar",
            "",
            "  baz"
        ].join("\n");
        let first_result = dedent_to(&x, "  ").unwrap();
        assert_eq!(dedent_to(&first_result, "  "), Some(y));
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_to_preserves_less_indented_later_line() {
        let x = [
            "   foo\n",
            "  bar\n",
        ].join("");
        let y = [
            "foo\n",
            "  bar\n",
        ].join("");
        assert_eq!(dedent_to(&x, ""), Some(y));
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_to_preserves_less_indented_later_line_with_crlf() {
        let x = [
            "   foo\r\n",
            "  bar\r\n",
        ].join("");
        let y = [
            "foo\r\n",
            "  bar\r\n",
        ].join("");
        assert_eq!(dedent_to(&x, ""), Some(y));
    }

    #[test]
    #[rustfmt::skip]
    fn dedent_to_ignores_leading_form_feeds_in_provided_indentation() {
        let x = [
            "  1",
            "  2",
        ].join("\n");
        let y = [
            "1",
            "2",
        ].join("\n");
        assert_eq!(dedent_to(&x, "\x0C\x0C"), Some(y));
    }
}
