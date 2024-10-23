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
/// # Panics
/// If the first line is indented by less than the provided indent.
pub fn dedent_to(text: &str, indent: &str) -> Option<String> {
    // Look at the indentation of the first non-empty line, to determine the "baseline" indentation.
    let mut first_comment = None;
    let existing_indent_len = text
        .universal_newlines()
        .find_map(|line| {
            let trimmed = line.trim_whitespace_start();
            if trimmed.is_empty() {
                None
            } else if trimmed.starts_with('#') && first_comment.is_none() {
                first_comment = Some(line.len() - trimmed.len());
                None
            } else {
                Some(line.len() - trimmed.len())
            }
        })
        .unwrap_or(first_comment.unwrap_or_default());

    if existing_indent_len < indent.len() {
        return None;
    }

    // Determine the amount of indentation to remove.
    let dedent_len = existing_indent_len - indent.len();

    let mut result = String::with_capacity(text.len() + indent.len());
    for line in text.universal_newlines() {
        let trimmed = line.trim_whitespace_start();
        if trimmed.is_empty() {
            if let Some(line_ending) = line.line_ending() {
                result.push_str(&line_ending);
            }
        } else {
            // Determine the current indentation level.
            let current_indent_len = line.len() - trimmed.len();
            if current_indent_len < existing_indent_len {
                // If the current indentation level is less than the baseline, keep it as is.
                result.push_str(line.as_full_str());
            } else {
                // Otherwise, reduce the indentation level.
                result.push_str(&line.as_full_str()[dedent_len..]);
            }
        }
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
    }
}
