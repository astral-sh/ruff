use anyhow::Result;
use log::error;
use ruff_text_size::{TextRange, TextSize};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::Fix;

#[derive(Debug, PartialEq, Eq, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DiagnosticKind {
    /// The identifier of the diagnostic, used to align the diagnostic with a rule.
    pub name: String,
    /// The message body to display to the user, to explain the diagnostic.
    pub body: String,
    /// The message to display to the user, to explain the suggested fix.
    pub suggestion: Option<String>,
    /// Whether the diagnostic is automatically fixable.
    pub fixable: bool,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub range: TextRange,
    pub fix: Fix,
    pub parent: Option<TextSize>,
}

impl Diagnostic {
    pub fn new<T: Into<DiagnosticKind>>(kind: T, range: TextRange) -> Self {
        Self {
            kind: kind.into(),
            range,
            fix: Fix::empty(),
            parent: None,
        }
    }

    /// Set the [`Fix`] used to fix the diagnostic.
    #[inline]
    pub fn set_fix<T: Into<Fix>>(&mut self, fix: T) {
        self.fix = fix.into();
    }

    /// Consumes `self` and returns a new `Diagnostic` with the given `fix`.
    #[inline]
    #[must_use]
    pub fn with_fix<T: Into<Fix>>(mut self, fix: T) -> Self {
        self.set_fix(fix);
        self
    }

    /// Set the [`Fix`] used to fix the diagnostic, if the provided function returns `Ok`.
    /// Otherwise, log the error.
    #[inline]
    pub fn try_set_fix<T: Into<Fix>>(&mut self, func: impl FnOnce() -> Result<T>) {
        match func() {
            Ok(fix) => self.fix = fix.into(),
            Err(err) => error!("Failed to create fix: {}", err),
        }
    }

    pub const fn range(&self) -> TextRange {
        self.range
    }

    pub const fn start(&self) -> TextSize {
        self.range.start()
    }

    pub const fn end(&self) -> TextSize {
        self.range.end()
    }

    /// Set the location of the diagnostic's parent node.
    #[inline]
    pub fn set_parent(&mut self, parent: TextSize) {
        self.parent = Some(parent);
    }
}
