use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::ast::Location;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

#[violation]
pub struct BlanketTypeIgnore;

impl Violation for BlanketTypeIgnore {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use specific rule codes when ignoring type issues")
    }
}

/// PGH003
pub fn blanket_type_ignore(diagnostics: &mut Vec<Diagnostic>, lineno: usize, line: &str) {
    for match_ in TYPE_IGNORE_PATTERN.find_iter(line) {
        if let Ok(codes) = parse_type_ignore_tag(line[match_.end()..].trim()) {
            if codes.is_empty() {
                let start = line[..match_.start()].chars().count();
                let end = start + line[match_.start()..match_.end()].chars().count();
                diagnostics.push(Diagnostic::new(
                    BlanketTypeIgnore,
                    Range::new(
                        Location::new(lineno + 1, start),
                        Location::new(lineno + 1, end),
                    ),
                ));
            }
        }
    }
}

// Match, e.g., `# type: ignore` or `# type: ignore[attr-defined]`.
// See: https://github.com/python/mypy/blob/b43e0d34247a6d1b3b9d9094d184bbfcb9808bb9/mypy/fastparse.py#L248
static TYPE_IGNORE_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"#\s*(type|pyright):\s*ignore\s*").unwrap());

// Match, e.g., `[attr-defined]` or `[attr-defined, misc]`.
// See: https://github.com/python/mypy/blob/b43e0d34247a6d1b3b9d9094d184bbfcb9808bb9/mypy/fastparse.py#L327
static TYPE_IGNORE_TAG_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*\[(?P<codes>[^]#]*)]\s*(#.*)?$").unwrap());

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
