use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::Fix;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Diagnostic {
    /// The identifier of the diagnostic, used to align the diagnostic with a rule.
    pub name: &'static str,
    /// The message body to display to the user, to explain the diagnostic.
    pub body: String,
    /// The message to display to the user, to explain the suggested fix.
    pub suggestion: Option<String>,
    pub range: TextRange,
    pub fix: Option<Fix>,
    pub parent: Option<TextSize>,
}

impl Diagnostic {}

impl Ranged for Diagnostic {
    fn range(&self) -> TextRange {
        self.range
    }
}
