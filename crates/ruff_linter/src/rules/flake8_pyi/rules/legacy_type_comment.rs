use std::sync::LazyLock;

use regex::Regex;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::PySourceType;
use ruff_python_trivia::CommentRanges;

use crate::Locator;
use crate::Violation;
use crate::checkers::ast::LintContext;
use crate::preview::is_legacy_type_comment_in_non_stub_enabled;

/// ## What it does
/// Checks for the use of type comments (e.g., `x = 1  # type: int`).
///
/// By default, this check only runs on stub files. If [`preview`] mode is enabled,
/// the check also runs on `.py` files.
///
/// ## Why is this bad?
/// Type comments are a soft-deprecated form of type annotation. They are unsupported
/// by some modern type checkers, including ty and Pyrefly, and are only necessary when
/// supporting end-of-life Python versions such as 3.5 or older. They are never necessary
/// in stub files, which are not executed at runtime.
///
/// This rule does not apply to `# type: ignore` suppression comments.
///
/// ## Example
/// ```pyi
/// x = 1  # type: int
/// ```
///
/// Use instead:
/// ```pyi
/// x: int = 1
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.254")]
pub(crate) struct LegacyTypeComment;

impl Violation for LegacyTypeComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Don't use type comments".to_string()
    }
}

/// PYI033
pub(crate) fn legacy_type_comment(
    context: &LintContext,
    locator: &Locator,
    comment_ranges: &CommentRanges,
    source_type: PySourceType,
) {
    if !source_type.is_stub() && !is_legacy_type_comment_in_non_stub_enabled(context.settings()) {
        return;
    }

    for range in comment_ranges {
        let comment = locator.slice(range);

        if TYPE_COMMENT_REGEX.is_match(comment) && !TYPE_IGNORE_REGEX.is_match(comment) {
            context.report_diagnostic(LegacyTypeComment, range);
        }
    }
}

static TYPE_COMMENT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#\s*type:\s*([^#]+)(\s*#.*?)?$").unwrap());

static TYPE_IGNORE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#\s*type:\s*ignore([^#]+)?(\s*#.*?)?$").unwrap());
