use once_cell::unsync::Lazy;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::Line;

use crate::settings::Settings;

use regex::Regex;

// Three states are possible:
// 1. Found copyright header
// 2. Missing copyright header
// 3. file length < chars_before_copyright_header
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum CopyrightHeaderKind {
    Missing,
    Present,
    NotFoundInRange,
}

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
    line: &Line,
    settings: &Settings,
    current_char_index: u32,
) -> CopyrightHeaderKind {
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

    let out_of_range = current_char_index > copyright_file_size;
    let copyright_missing = regex.find(line.as_str()).is_none();

    if copyright_missing && out_of_range {
        // Missing copyright header
        return CopyrightHeaderKind::Missing;
    }
    if !copyright_missing {
        // Found copyright header, should stop checking
        return CopyrightHeaderKind::Present;
    }
    // Missing copyright header, but need to keep checking
    CopyrightHeaderKind::NotFoundInRange
}
