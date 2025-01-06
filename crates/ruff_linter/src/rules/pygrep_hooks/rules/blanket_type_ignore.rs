use std::sync::LazyLock;

use anyhow::{anyhow, Result};
use memchr::memchr_iter;
use regex::Regex;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_trivia::CommentRanges;
use ruff_text_size::TextSize;

use crate::Locator;

/// ## What it does
/// Check for `type: ignore` annotations that suppress all type warnings, as
/// opposed to targeting specific type warnings.
///
/// ## Why is this bad?
/// Suppressing all warnings can hide issues in the code.
///
/// Blanket `type: ignore` annotations are also more difficult to interpret and
/// maintain, as the annotation does not clarify which warnings are intended
/// to be suppressed.
///
/// ## Example
/// ```python
/// from foo import secrets  # type: ignore
/// ```
///
/// Use instead:
/// ```python
/// from foo import secrets  # type: ignore[attr-defined]
/// ```
///
/// ## References
/// Mypy supports a [built-in setting](https://mypy.readthedocs.io/en/stable/error_code_list2.html#check-that-type-ignore-include-an-error-code-ignore-without-code)
/// to enforce that all `type: ignore` annotations include an error code, akin
/// to enabling this rule:
/// ```toml
/// [tool.mypy]
/// enable_error_code = ["ignore-without-code"]
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct BlanketTypeIgnore;

impl Violation for BlanketTypeIgnore {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use specific rule codes when ignoring type issues".to_string()
    }
}

/// PGH003
pub(crate) fn blanket_type_ignore(
    diagnostics: &mut Vec<Diagnostic>,
    comment_ranges: &CommentRanges,
    locator: &Locator,
) {
    for range in comment_ranges {
        let line = locator.slice(range);

        // Match, e.g., `# type: ignore` or `# type: ignore[attr-defined]`.
        // See: https://github.com/python/mypy/blob/b43e0d34247a6d1b3b9d9094d184bbfcb9808bb9/mypy/fastparse.py#L248
        for start in memchr_iter(b'#', line.as_bytes()) {
            // Strip the `#` and any trailing whitespace.
            let comment = &line[start + 1..].trim_start();

            // Match the `type` or `pyright` prefixes (in, e.g., `# type: ignore`).
            let Some(comment) = comment
                .strip_prefix("type")
                .or_else(|| comment.strip_prefix("pyright"))
            else {
                continue;
            };

            // Next character must be a colon.
            if !comment.starts_with(':') {
                continue;
            }

            // Strip the colon and any trailing whitespace.
            let comment = &comment[1..].trim_start();

            // Match the `ignore`.
            let Some(comment) = comment.strip_prefix("ignore") else {
                continue;
            };

            // Strip any trailing whitespace.
            let comment = comment.trim_start();

            // Match the optional `[...]` tag.
            if let Ok(codes) = parse_type_ignore_tag(comment) {
                if codes.is_empty() {
                    diagnostics.push(Diagnostic::new(
                        BlanketTypeIgnore,
                        range.add_start(TextSize::try_from(start).unwrap()),
                    ));
                }
            }
        }
    }
}

// Match, e.g., `[attr-defined]` or `[attr-defined, misc]`.
// See: https://github.com/python/mypy/blob/b43e0d34247a6d1b3b9d9094d184bbfcb9808bb9/mypy/fastparse.py#L327
static TYPE_IGNORE_TAG_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\[(?P<codes>[^]#]*)]\s*(#.*)?$").unwrap());

/// Parse the optional `[...]` tag in a `# type: ignore[...]` comment.
///
/// Returns a list of error codes to ignore, or an empty list if the tag is
/// a blanket ignore.
fn parse_type_ignore_tag(tag: &str) -> Result<Vec<&str>> {
    // See: https://github.com/python/mypy/blob/b43e0d34247a6d1b3b9d9094d184bbfcb9808bb9/mypy/fastparse.py#L316
    // No tag -- ignore all errors.
    let trimmed = tag.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(vec![]);
    }

    // Parse comma-separated list of error codes.
    TYPE_IGNORE_TAG_PATTERN
        .captures(tag)
        .map(|captures| {
            captures
                .name("codes")
                .unwrap()
                .as_str()
                .split(',')
                .map(str::trim)
                .collect()
        })
        .ok_or_else(|| anyhow!("Invalid type ignore tag: {tag}"))
}

#[cfg(test)]
mod tests {

    #[test]
    fn type_ignore_tag() {
        let tag = "";
        let result = super::parse_type_ignore_tag(tag);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Vec::<&str>::new());

        let tag = "[attr-defined]";
        let result = super::parse_type_ignore_tag(tag);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec!["attr-defined"]);

        let tag = "   [attr-defined]";
        let result = super::parse_type_ignore_tag(tag);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec!["attr-defined"]);

        let tag = "[attr-defined, misc]";
        let result = super::parse_type_ignore_tag(tag);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec!["attr-defined", "misc"]);
    }
}
