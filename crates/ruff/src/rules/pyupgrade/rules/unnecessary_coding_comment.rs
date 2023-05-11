use once_cell::sync::Lazy;
use regex::Regex;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::Line;

// TODO: document referencing [PEP 3120]: https://peps.python.org/pep-3120/
#[violation]
pub struct UTF8EncodingDeclaration;

impl AlwaysAutofixableViolation for UTF8EncodingDeclaration {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("UTF-8 encoding declaration is unnecessary")
    }

    fn autofix_title(&self) -> String {
        "Remove unnecessary coding comment".to_string()
    }
}

// Regex from PEP263.
static CODING_COMMENT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[ \t\f]*#.*?coding[:=][ \t]*utf-?8").unwrap());

/// UP009
pub(crate) fn unnecessary_coding_comment(line: &Line, autofix: bool) -> Option<Diagnostic> {
    // PEP3120 makes utf-8 the default encoding.
    if CODING_COMMENT_REGEX.is_match(line.as_str()) {
        let mut diagnostic = Diagnostic::new(UTF8EncodingDeclaration, line.full_range());
        if autofix {
            #[allow(deprecated)]
            diagnostic.set_fix(Fix::unspecified(Edit::deletion(
                line.start(),
                line.full_end(),
            )));
        }
        Some(diagnostic)
    } else {
        None
    }
}
