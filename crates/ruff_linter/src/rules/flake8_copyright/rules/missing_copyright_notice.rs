use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::{TextRange, TextSize};

use crate::Locator;
use crate::Violation;
use crate::checkers::ast::LintContext;
use crate::settings::LinterSettings;

/// ## What it does
/// Checks for the absence of copyright notices within Python files.
///
/// Note that this check only searches within the first 4096 bytes of the file.
///
/// ## Why is this bad?
/// In some codebases, it's common to have a license header at the top of every
/// file. This rule ensures that the license header is present.
///
/// ## Options
/// - `lint.flake8-copyright.author`
/// - `lint.flake8-copyright.min-file-size`
/// - `lint.flake8-copyright.notice-rgx`
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.0.273")]
pub(crate) struct MissingCopyrightNotice;

impl Violation for MissingCopyrightNotice {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Missing copyright notice at top of file".to_string()
    }
}

/// CPY001
pub(crate) fn missing_copyright_notice(
    locator: &Locator,
    settings: &LinterSettings,
    context: &LintContext,
) {
    // Ignore files that are too small to contain a copyright notice.
    if locator.len() < settings.flake8_copyright.min_file_size {
        return;
    }

    // Only search the first 4096 bytes in the file.
    let contents = locator.up_to(locator.floor_char_boundary(TextSize::new(4096)));

    // Locate the copyright notice.
    if let Some(match_) = settings.flake8_copyright.notice_rgx.find(contents) {
        match settings.flake8_copyright.author {
            Some(ref author) => {
                // Ensure that it's immediately followed by the author.
                if contents[match_.end()..].trim_start().starts_with(author) {
                    return;
                }
            }
            None => return,
        }
    }

    context.report_diagnostic(MissingCopyrightNotice, TextRange::default());
}
