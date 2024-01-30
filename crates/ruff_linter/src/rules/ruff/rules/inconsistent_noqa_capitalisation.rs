use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic};
use ruff_macros::violation;
use ruff_python_index::Indexer;
use ruff_source_file::{Line, Locator};

use crate::settings::LinterSettings;

#[violation]
pub struct InconsistentNoqaCapitalisation;

impl AlwaysFixableViolation for InconsistentNoqaCapitalisation {
    fn message(&self) -> String {
        todo!()
    }

    fn fix_title(&self) -> String {
        todo!()
    }

    fn message_formats() -> &'static [&'static str] {
        todo!()
    }
}

pub fn consistent_noqa_capitalisation(
    line: &Line,
    locator: &Locator,
    indexer: &Indexer,
    settings: &LinterSettings,
) -> Option<Diagnostic> {
    todo!()
}
