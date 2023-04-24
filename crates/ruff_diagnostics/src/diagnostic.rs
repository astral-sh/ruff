use anyhow::Result;
use log::error;
use rustpython_parser::ast::Location;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use ruff_python_ast::types::Range;

use crate::Fix;

#[derive(Debug, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub location: Location,
    pub end_location: Location,
    pub fix: Fix,
    pub parent: Option<Location>,
}

impl Diagnostic {
    pub fn new<T: Into<DiagnosticKind>>(kind: T, range: Range) -> Self {
        Self {
            kind: kind.into(),
            location: range.location,
            end_location: range.end_location,
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

    /// Set the location of the diagnostic's parent node.
    #[inline]
    pub fn set_parent(&mut self, parent: Location) {
        self.parent = Some(parent);
    }
}
