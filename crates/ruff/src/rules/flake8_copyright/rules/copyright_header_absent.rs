use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::Line;

use crate::settings::Settings;

use lazy_regex::Regex;
#[violation]
pub struct HeaderLacksCopyright;

impl Violation for HeaderLacksCopyright {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Copyright notice not present")
    }
}
/// ## What it does
/// Checks for Copyright Header to exist within at the top of a file within `copyright_min_file_size chars`
/// format Copyright (C) <year> <author>
///
/// Error code C801
pub(crate) fn copyright_header_absent(
    line: &Line,
    settings: &Settings,
    current_char_index: i64,
) -> Option<bool> {
    let copyright_regexp = format!(
        "{} {}",
        settings.flake8_copyright.copyright_regexp, settings.flake8_copyright.copyright_author
    );
    let regex = Regex::new(copyright_regexp.trim()).unwrap();

    let out_of_range =
        current_char_index > (settings.flake8_copyright.copyright_min_file_size as i64);
    let copyright_missing = regex.find(line.as_str()).is_none();

    if copyright_missing && out_of_range {
        // Missing copyright header
        return Some(true);
    }
    if !copyright_missing {
        // Found copyright header, should stop checking
        return Some(false);
    }
    // Missing copyright header, but need to keep checking
    None
}
