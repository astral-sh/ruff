use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::noqa::Directive;

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

/// PGH004
pub(crate) fn blanket_noqa(
    diagnostics: &mut Vec<Diagnostic>,
    indexer: &Indexer,
    locator: &Locator,
) {
    for range in indexer.comment_ranges() {
        let line = locator.slice(*range);
        if let Ok(Some(Directive::All(all))) = Directive::try_extract(line, range.start()) {
            diagnostics.push(Diagnostic::new(BlanketNOQA, all.range()));
        }
    }
}
