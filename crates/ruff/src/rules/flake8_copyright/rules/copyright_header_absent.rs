use once_cell::unsync::Lazy;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_newlines::StrExt;
use ruff_python_ast::source_code::Locator;

use crate::settings::Settings;

use regex::Regex;

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
/// Error code CPY801
pub(crate) fn copyright_header_absent(
    locator: &Locator,
    settings: &Settings,
) -> Option<Diagnostic> {
    let copyright_regexp = format!(
        "{} {}",
        settings.flake8_copyright.copyright_regexp, settings.flake8_copyright.copyright_author
    );

    // use default string if we panic
    let regex = Lazy::new(|| {
        Regex::new(copyright_regexp.trim())
            .unwrap_or_else(|_| Regex::new("(?i)Copyright \\(C\\) \\d{4}").unwrap())
    });

    // flake8 copyright uses maximum allowed chars to be 1024 before copyright
    let copyright_file_size: u32 = match settings.flake8_copyright.copyright_min_file_size {
        x if x <= 1024 => settings.flake8_copyright.copyright_min_file_size,
        _ => 1024, // max is 1024 in flake8 rule
    };
    let mut current_char_index: u32 = 0;

    for (_, line) in locator.contents().universal_newlines().enumerate() {
        let out_of_range = current_char_index > copyright_file_size;
        let copyright_missing = regex.find(line.as_str()).is_none();

        if !copyright_missing {
            // copyright found
            return None;
        }
        if out_of_range {
            // Missing copyright header
            return Some(Diagnostic::new(HeaderLacksCopyright, line.range()));
        }
        current_char_index += u32::try_from(line.chars().count()).unwrap_or(1024);
    }
    None
}
