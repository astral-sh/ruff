use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::TextRange;

use crate::Locator;
use crate::Violation;
use crate::checkers::ast::LintContext;
use crate::settings::LinterSettings;

/// ## What it does
/// Checks for modules with too many lines.
///
/// By default, this rule allows up to 1000 lines, as configured by the
/// [`lint.pylint.max-module-lines`] option.
///
/// ## Why is this bad?
/// Modules with many lines are generally harder to read and understand.
/// Extracting functionality into separate modules can improve code organization
/// and maintainability.
///
/// ## Example
/// A module with 1500 lines when `max-module-lines` is set to 1000 will trigger
/// this rule.
///
/// ## Options
/// - `lint.pylint.max-module-lines`
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.14.9")]
pub(crate) struct TooManyLines {
    actual_lines: usize,
    max_lines: usize,
}

impl Violation for TooManyLines {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyLines {
            actual_lines,
            max_lines,
        } = self;
        format!("Too many lines in module ({actual_lines}/{max_lines})")
    }
}

/// C0302
pub(crate) fn too_many_lines(locator: &Locator, settings: &LinterSettings, context: &LintContext) {
    let actual_lines = locator.contents().lines().count();
    let max_lines = settings.pylint.max_module_lines;

    if actual_lines > max_lines {
        context.report_diagnostic(
            TooManyLines {
                actual_lines,
                max_lines,
            },
            TextRange::default(),
        );
    }
}
