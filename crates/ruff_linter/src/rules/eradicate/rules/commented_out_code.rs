use crate::settings::LinterSettings;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_trivia::CommentRanges;
use ruff_source_file::{Locator, UniversalNewlineIterator};
use ruff_text_size::TextRange;

use super::super::detection::comment_contains_code;

/// ## What it does
/// Checks for commented-out Python code.
///
/// ## Why is this bad?
/// Commented-out code is dead code, and is often included inadvertently.
/// It should be removed.
///
/// ## Known problems
/// Prone to false positives when checking comments that resemble Python code,
/// but are not actually Python code ([#4845]).
///
/// ## Example
/// ```python
/// # print("Hello, world!")
/// ```
///
/// ## Options
/// - `lint.task-tags`
///
/// [#4845]: https://github.com/astral-sh/ruff/issues/4845
#[violation]
pub struct CommentedOutCode;

impl Violation for CommentedOutCode {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found commented-out code")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Remove commented-out code"))
    }
}

/// ERA001
pub(crate) fn commented_out_code(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    settings: &LinterSettings,
) {
    let mut comments = comment_ranges.into_iter().peekable();
    // Iterate over all comments in the document.
    while let Some(range) = comments.next() {
        let line = locator.line(range.start());

        if is_script_tag_start(line) {
            if skip_script_comments(range, &mut comments, locator) {
                continue;
            }
        }

        // Verify that the comment is on its own line, and that it contains code.
        if is_own_line_comment(line) && comment_contains_code(line, &settings.task_tags[..]) {
            let mut diagnostic = Diagnostic::new(CommentedOutCode, range);
            diagnostic.set_fix(Fix::display_only_edit(Edit::range_deletion(
                locator.full_lines_range(range),
            )));
            diagnostics.push(diagnostic);
        }
    }
}

/// Parses the rest of a [PEP 723](https://peps.python.org/pep-0723/)
/// script comment and moves `comments` past the script comment's end unless
/// the script comment is invalid.
///
/// Returns `true` if it is a valid script comment.
fn skip_script_comments<I>(
    script_start: TextRange,
    comments: &mut std::iter::Peekable<I>,
    locator: &Locator,
) -> bool
where
    I: Iterator<Item = TextRange>,
{
    let line_end = locator.full_line_end(script_start.end());
    let rest = locator.after(line_end);
    let mut end_offset = None;
    let mut lines = UniversalNewlineIterator::with_offset(rest, line_end).peekable();

    while let Some(line) = lines.next() {
        let Some(content) = script_line_content(&line) else {
            break;
        };

        if content == "///" {
            // > Precedence for an ending line # /// is given when the next line is not a valid
            // > embedded content line as described above.
            // > For example, the following is a single fully valid block:
            // > ```python
            // > # /// some-toml
            // > # embedded-csharp = """
            // > # /// <summary>
            // > # /// text
            // > # ///
            // > # /// </summary>
            // > # public class MyClass { }
            // > # """
            // > # ///
            // ````
            if lines.next().is_some_and(|line| is_valid_script_line(&line)) {
                continue;
            }
            end_offset = Some(line.full_end());
            break;
        }
    }

    // > Unclosed blocks MUST be ignored.
    let Some(end_offset) = end_offset else {
        return false;
    };

    // Skip over all script-comments.
    while let Some(comment) = comments.peek() {
        if comment.start() >= end_offset {
            break;
        }

        comments.next();
    }

    true
}

fn script_line_content(line: &str) -> Option<&str> {
    let Some(rest) = line.strip_prefix('#') else {
        // Not a comment
        return None;
    };

    // An empty line
    if rest.is_empty() {
        return Some("");
    }

    // > If there are characters after the # then the first character MUST be a space.
    rest.strip_prefix(' ')
}

fn is_valid_script_line(line: &str) -> bool {
    script_line_content(line).is_some()
}

/// Returns `true` if line contains an own-line comment.
fn is_own_line_comment(line: &str) -> bool {
    for char in line.chars() {
        if char == '#' {
            return true;
        }
        if !char.is_whitespace() {
            return false;
        }
    }
    unreachable!("Comment should contain '#' character")
}

/// Returns `true` if the line appears to start a script tag.
///
/// See: <https://peps.python.org/pep-0723/>
fn is_script_tag_start(line: &str) -> bool {
    line == "# /// script"
}

#[cfg(test)]
mod tests {
    use crate::rules::eradicate::rules::commented_out_code::skip_script_comments;
    use ruff_python_parser::parse_module;
    use ruff_python_trivia::CommentRanges;
    use ruff_source_file::Locator;
    use ruff_text_size::TextSize;
    #[test]
    fn script_comment() {
        let code = r#"
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "requests<3",
#   "rich",
# ]
# ///

a = 10 # abc
        "#;

        let parsed = parse_module(code).unwrap();
        let locator = Locator::new(code);

        let comments = CommentRanges::from(parsed.tokens());
        let mut comments = comments.into_iter().peekable();

        let script_start = code.find("# /// script").unwrap();
        let script_start_range = locator.full_line_range(TextSize::try_from(script_start).unwrap());

        let valid = skip_script_comments(script_start_range, &mut comments, &Locator::new(code));

        assert!(valid);

        let next_comment = comments.next();

        assert!(next_comment.is_some());
        assert_eq!(&code[next_comment.unwrap()], "# abc");
    }

    #[test]
    fn script_comment_end_precedence() {
        let code = r#"
# /// script
# [tool.uv]
# extra-index-url = ["https://pypi.org/simple", """\
# https://example.com/
# ///
# """
# ]
# ///

a = 10 # abc
        "#;

        let parsed = parse_module(code).unwrap();
        let locator = Locator::new(code);

        let comments = CommentRanges::from(parsed.tokens());
        let mut comments = comments.into_iter().peekable();

        let script_start = code.find("# /// script").unwrap();
        let script_start_range = locator.full_line_range(TextSize::try_from(script_start).unwrap());

        let valid = skip_script_comments(script_start_range, &mut comments, &Locator::new(code));

        assert!(valid);

        let next_comment = comments.next();

        assert!(next_comment.is_some());
        assert_eq!(&code[next_comment.unwrap()], "# abc");
    }
}
