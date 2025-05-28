use anyhow::Result;
use log::debug;

use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::registry::AsRule;
use crate::violation::Violation;
use crate::{Fix, codes::Rule};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Diagnostic {
    /// The message body to display to the user, to explain the diagnostic.
    pub body: String,
    /// The message to display to the user, to explain the suggested fix.
    pub suggestion: Option<String>,
    pub range: TextRange,
    pub fix: Option<Fix>,
    pub parent: Option<TextSize>,

    pub(crate) rule: Rule,
}

impl Diagnostic {
    // TODO(brent) We temporarily allow this to avoid updating all of the call sites to add
    // references. I expect this method to go away or change significantly with the rest of the
    // diagnostic refactor, but if it still exists in this form at the end of the refactor, we
    // should just update the call sites.
    #[expect(clippy::needless_pass_by_value)]
    pub fn new<T: Violation>(kind: T, range: TextRange) -> Self {
        Self {
            body: Violation::message(&kind),
            suggestion: Violation::fix_title(&kind),
            range,
            fix: None,
            parent: None,
            rule: T::rule(),
        }
    }

    /// Consumes `self` and returns a new `Diagnostic` with the given `fix`.
    #[inline]
    #[must_use]
    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.set_fix(fix);
        self
    }

    /// Set the [`Fix`] used to fix the diagnostic.
    #[inline]
    pub fn set_fix(&mut self, fix: Fix) {
        self.fix = Some(fix);
    }

    /// Set the [`Fix`] used to fix the diagnostic, if the provided function returns `Ok`.
    /// Otherwise, log the error.
    #[inline]
    pub fn try_set_fix(&mut self, func: impl FnOnce() -> Result<Fix>) {
        match func() {
            Ok(fix) => self.fix = Some(fix),
            Err(err) => debug!("Failed to create fix for {}: {}", self.rule, err),
        }
    }

    /// Set the [`Fix`] used to fix the diagnostic, if the provided function returns `Ok`.
    /// Otherwise, log the error.
    #[inline]
    pub fn try_set_optional_fix(&mut self, func: impl FnOnce() -> Result<Option<Fix>>) {
        match func() {
            Ok(None) => {}
            Ok(Some(fix)) => self.fix = Some(fix),
            Err(err) => debug!("Failed to create fix for {}: {}", self.rule, err),
        }
    }

    /// Consumes `self` and returns a new `Diagnostic` with the given parent node.
    #[inline]
    #[must_use]
    pub fn with_parent(mut self, parent: TextSize) -> Self {
        self.set_parent(parent);
        self
    }

    /// Set the location of the diagnostic's parent node.
    #[inline]
    pub fn set_parent(&mut self, parent: TextSize) {
        self.parent = Some(parent);
    }
}

impl AsRule for Diagnostic {
    fn rule(&self) -> Rule {
        self.rule
    }
}

impl Ranged for Diagnostic {
    fn range(&self) -> TextRange {
        self.range
    }
}
