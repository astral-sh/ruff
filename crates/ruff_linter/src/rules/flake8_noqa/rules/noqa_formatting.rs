use std::collections::HashSet;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::noqa::Directive;

#[violation]
pub struct NOQAMissingColon;

impl AlwaysFixableViolation for NOQAMissingColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`noqa` must have a colon")
    }

    fn fix_title(&self) -> String {
        "Add missing colon".to_string()
    }
}

#[violation]
pub struct NOQASpaceBeforeColon;

impl AlwaysFixableViolation for NOQASpaceBeforeColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`noqa` must not have a space before the colon")
    }

    fn fix_title(&self) -> String {
        "Remove space(s) before colon".to_string()
    }
}

#[violation]
pub struct NOQAMultipleSpacesBeforeCode;

impl AlwaysFixableViolation for NOQAMultipleSpacesBeforeCode {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`noqa` must only have one space before the code(s)")
    }

    fn fix_title(&self) -> String {
        "Remove extra space(s) before code(s)".to_string()
    }
}

#[violation]
pub struct NOQADuplicateCodes;

impl AlwaysFixableViolation for NOQADuplicateCodes {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`noqa` has duplicate codes")
    }

    fn fix_title(&self) -> String {
        "Remove duplicate code".to_string()
    }
}

/// NQA002, NQA003, NQA004, NQA005
pub(crate) fn noqa_formatting(
    diagnostics: &mut Vec<Diagnostic>,
    indexer: &Indexer,
    locator: &Locator,
) {
    for range in indexer.comment_ranges() {
        let line = locator.slice(*range);
        let offset = range.start();
        if let Ok(Some(directive)) = Directive::try_extract(line, TextSize::new(0)) {
            match directive {
                Directive::All(all) => {
                    let post_noqa_start = all.range().end().to_usize();
                    let mut cursor = post_noqa_start;
                    cursor += leading_whitespace_len(&line[cursor..]);

                    // NQA003
                    if matches!(line[cursor..].chars().next(), Some(':')) {
                        let start = offset + all.range().end();
                        let end = offset + TextSize::new(u32::try_from(cursor).unwrap());
                        let mut diagnostic =
                            Diagnostic::new(NOQASpaceBeforeColon, TextRange::new(start, end));
                        diagnostic.set_fix(Fix::safe_edit(Edit::deletion(start, end)));
                        diagnostics.push(diagnostic);
                    }
                    // NQA002
                    else if let Some(_) = Directive::lex_code(&line[cursor..]) {
                        let start = offset + all.range().end();
                        let end = start + TextSize::new(1);
                        let mut diagnostic =
                            Diagnostic::new(NOQAMissingColon, TextRange::new(start, end));
                        diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(':'.to_string(), start)));
                        diagnostics.push(diagnostic);
                    }
                }
                Directive::Codes(codes) => {
                    let mut cursor = codes.range().start().to_usize();
                    cursor += find_noqa_end(line).unwrap();
                    let num_spaces = leading_whitespace_len(&line[cursor..]);

                    // NQA004
                    if num_spaces > 1 {
                        let start = offset + TextSize::new(u32::try_from(cursor + 1).unwrap());
                        let end = start + TextSize::new(u32::try_from(num_spaces - 1).unwrap());
                        let mut diagnostic =
                            Diagnostic::new(NOQAMultipleSpacesBeforeCode, TextRange::new(start, end));
                        diagnostic.set_fix(Fix::safe_edit(Edit::deletion(start, end)));
                        diagnostics.push(diagnostic);
                    }

                    let mut seen = HashSet::new();
                    for (code, code_range) in codes.codes().iter().zip(codes.code_ranges_full().iter()) {
                        if !seen.insert(code) {
                            let start = offset + code_range.start();
                            let end = offset + code_range.end();
                            let mut diagnostic = Diagnostic::new(NOQADuplicateCodes, TextRange::new(start, end));
                            diagnostic.set_fix(Fix::safe_edit(Edit::deletion(start, end)));
                            diagnostics.push(diagnostic);
                        }
                    }
                }
            }
        }
    }
}

fn leading_whitespace_len(text: &str) -> usize {
    return text.find(|c: char| !c.is_whitespace()).unwrap_or(0);
}

fn find_noqa_end(text: &str) -> Option<usize> {
    for (char_index, char) in text.char_indices() {
        // Only bother checking for the `noqa` literal if the character is `n` or `N`.
        if !matches!(char, 'n' | 'N') {
            continue;
        }

        // Determine the start of the `noqa` literal.
        if !matches!(
            text[char_index..].as_bytes(),
            [b'n' | b'N', b'o' | b'O', b'q' | b'Q', b'a' | b'A', b':', ..]
        ) {
            continue;
        }

        return Some(char_index + "noqa:".len());
    }
    None
}
