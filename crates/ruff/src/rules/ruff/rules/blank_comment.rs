use crate::registry::Rule;
use crate::settings::{flags, Settings};
use once_cell::sync::Lazy;
use regex::Regex;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::Line;
use ruff_text_size::{TextLen, TextRange, TextSize};

/// ## What it does
/// Check for blank comments.
///
/// ## Why is this bad?
/// Blank comments are useless and should be removed.
///
/// ## Example
/// ```python
/// print("Hello, World!")  #
/// ```
///
/// Use instead:
/// ```python
/// print("Hello, World!")
/// ```
///
/// ## References
/// - [Ruff documentation](https://beta.ruff.rs/docs/configuration/#error-suppression)
#[violation]
pub struct BlankComment;

impl AlwaysAutofixableViolation for BlankComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Blank comments are useless and should be removed")
    }

    fn autofix_title(&self) -> String {
        "Remove blank comment".to_string()
    }
}

static BLACK_COMMENT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\S(\s*#\s*)$").unwrap());

/// RUF010
pub fn blank_comment(
    diagnostics: &mut Vec<Diagnostic>,
    line: &Line,
    settings: &Settings,
    autofix: flags::Autofix,
) {
    if let Some(captures) = BLACK_COMMENT_REGEX.captures(line.as_str()) {
        let match_ = captures.get(1).unwrap();
        let range = TextRange::at(
            line.start() + TextSize::try_from(match_.start()).unwrap(),
            match_.as_str().text_len(),
        );
        let mut diagnostic = Diagnostic::new(BlankComment, range);
        if autofix.into() && settings.rules.should_fix(Rule::BlankComment) {
            diagnostic.set_fix(Edit::deletion(range.start(), range.end()));
        }
        diagnostics.push(diagnostic);
    }
}
