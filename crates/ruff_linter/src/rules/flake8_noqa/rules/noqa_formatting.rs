use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Violation, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::noqa::Directive;

#[violation]
pub struct NOQAMissingColon;

impl Violation for NOQAMissingColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`noqa` must have a colon")
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
        "Remove space before colon".to_string()
    }
}

#[violation]
pub struct NOQAMultipleSpacesBeforeCode;

impl Violation for NOQAMultipleSpacesBeforeCode {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`noqa` must only have one space before the codes")
    }
}

#[violation]
pub struct NOQADuplicateCodes;

impl Violation for NOQADuplicateCodes {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`noqa` has duplicate codes")
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

                    if matches!(line[cursor..].chars().next(), Some(':')) {
                        let start = offset + TextSize::new(u32::try_from(post_noqa_start).unwrap());
                        let end = offset + TextSize::new(u32::try_from(cursor).unwrap());
                        println!("{} {}", start.to_usize(), end.to_usize());
                        let mut diagnostic =
                            Diagnostic::new(NOQASpaceBeforeColon, TextRange::new(start, end));
                        diagnostic.set_fix(Fix::safe_edit(Edit::deletion(start, end)));
                        diagnostics.push(diagnostic);
                    } else if  {

                    }
                    println!("{}", &line[cursor..]);
                }
                _ => continue,
            }
        }
    }
}

fn leading_whitespace_len(text: &str) -> usize {
    return text.find(|c: char| !c.is_whitespace()).unwrap_or(0);
}
