use once_cell::sync::Lazy;
use regex::Regex;
use ruff_python_trivia::CommentRanges;
use ruff_source_file::Locator;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

/// ## What it does
/// Checks for the use of type comments (e.g., `x = 1  # type: int`) in stub
/// files.
///
/// ## Why is this bad?
/// Stub (`.pyi`) files should use type annotations directly, rather
/// than type comments, even if they're intended to support Python 2, since
/// stub files are not executed at runtime. The one exception is `# type: ignore`.
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
#[violation]
pub struct TypeCommentInStub;

impl Violation for TypeCommentInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Don't use type comments in stub file")
    }
}

/// PYI033
pub(crate) fn type_comment_in_stub(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    comment_ranges: &CommentRanges,
) {
    for range in comment_ranges {
        let comment = locator.slice(range);

        if TYPE_COMMENT_REGEX.is_match(comment) && !TYPE_IGNORE_REGEX.is_match(comment) {
            diagnostics.push(Diagnostic::new(TypeCommentInStub, range));
        }
    }
}

static TYPE_COMMENT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^#\s*type:\s*([^#]+)(\s*#.*?)?$").unwrap());

static TYPE_IGNORE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^#\s*type:\s*ignore([^#]+)?(\s*#.*?)?$").unwrap());
