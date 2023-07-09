use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;

use crate::settings::Settings;

/// ## What it does
/// Checks for the absence of copyright notices within Python files.
///
/// ## Why is this bad?
/// In some codebases, it's common to have a license header at the top of every
/// file. This rule ensures that the license header is present.
#[violation]
pub struct MissingCopyrightNotice;

impl Violation for MissingCopyrightNotice {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing copyright notice at top of file")
    }
}

/// CPY001
pub(crate) fn missing_copyright_notice(
    locator: &Locator,
    settings: &Settings,
) -> Option<Diagnostic> {
    // Ignore files that are too small to contain a copyright notice.
    if locator.len() < settings.flake8_copyright.min_file_size {
        return None;
    }

    // Only search the first 1024 bytes in the file.
    let contents = if locator.len() < 1024 {
        locator.contents()
    } else {
        locator.up_to(TextSize::from(1024))
    };

    // Locate the copyright notice.
    if let Some(match_) = settings.flake8_copyright.notice_rgx.find(contents) {
        match settings.flake8_copyright.author {
            Some(ref author) => {
                // Ensure that it's immediately followed by the author.
                if contents[match_.end()..].trim_start().starts_with(author) {
                    return None;
                }
            }
            None => return None,
        }
    }

    Some(Diagnostic::new(
        MissingCopyrightNotice,
        TextRange::default(),
    ))
}
