use once_cell::sync::Lazy;
use regex::Regex;
use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_whitespace::Line;

/// ## What it does
/// Check for `noqa` annotations that suppress all diagnostics, as opposed to
/// targeting specific diagnostics.
///
/// ## Why is this bad?
/// Suppressing all diagnostics can hide issues in the code.
///
/// Blanket `noqa` annotations are also more difficult to interpret and
/// maintain, as the annotation does not clarify which diagnostics are intended
/// to be suppressed.
///
/// ## Example
/// ```python
/// from .base import *  # noqa
/// ```
///
/// Use instead:
/// ```python
/// from .base import *  # noqa: F403
/// ```
///
/// ## References
/// - [Ruff documentation](https://beta.ruff.rs/docs/configuration/#error-suppression)
#[violation]
pub struct BlanketNOQA;

impl Violation for BlanketNOQA {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use specific rule codes when using `noqa`")
    }
}

static BLANKET_NOQA_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)# noqa($|\s|:[^ ])").unwrap());

/// PGH004
pub(crate) fn blanket_noqa(diagnostics: &mut Vec<Diagnostic>, line: &Line) {
    if let Some(match_) = BLANKET_NOQA_REGEX.find(line.as_str()) {
        diagnostics.push(Diagnostic::new(
            BlanketNOQA,
            TextRange::at(
                line.start() + TextSize::try_from(match_.start()).unwrap(),
                match_.as_str().text_len(),
            ),
        ));
    }
}
