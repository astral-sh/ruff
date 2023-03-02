use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use ruff_macros::{define_violation, derive_message_formats};

use crate::registry::Diagnostic;
use crate::violation::Violation;
use crate::Range;

define_violation!(
    /// ## What it does
    /// Do not use type comments (e.g. `x = 1 # type: int`) in stubs, even if
    /// the stub supports Python 2. Always use annotations instead (e.g.
    /// `x: int = 1`).
    ///
    /// ## Why is this bad?
    /// You should use type annotation directly in pyi files.
    ///
    /// ## Example
    /// ```python
    /// x = 1 # type: int
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// x: int = 1
    /// ```
    pub struct TypeCommentInStub;
);
impl Violation for TypeCommentInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Don't use type comments in stub file")
    }
}

static TYPE_COMMENT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^#\s*type:\s*([^#]+)(\s*#.*?)?$").unwrap());
static TYPE_IGNORE_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^#\s*type:\s*ignore([^#]+)?(\s*#.*?)?$").unwrap());

/// PYI033
pub fn type_comment_in_stub(tokens: &[LexResult]) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    for token in tokens.iter().flatten() {
        if let (location, Tok::Comment(comment), end_location) = token {
            if TYPE_COMMENT_REGEX.is_match(comment) && !TYPE_IGNORE_REGEX.is_match(comment) {
                diagnostics.push(Diagnostic::new(
                    TypeCommentInStub,
                    Range {
                        location: *location,
                        end_location: *end_location,
                    },
                ));
            }
        }
    }

    diagnostics
}
