//! Functions related to adding and removing indentation from lines of
//! text.

use std::borrow::Cow;
use std::cmp;

use crate::PythonWhitespace;
use ruff_source_file::newlines::UniversalNewlines;

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
/// maximum amount of whitespace that can be removed from all lines:
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
        let text = r#"        C = int(f.rea1,0],[-1,0,1]],
              [[-1,-1,1],[1,1,-1],[0,-1,0]],
              [[-1,-1,-1],[1,1,0],[1,0,1]]
             ]"#;
        assert_eq!(dedent(text), text);
    }
}
