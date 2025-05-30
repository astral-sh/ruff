use anyhow::Result;
use log::debug;

use ruff_db::diagnostic::{self as db, Annotation, DiagnosticId, LintName, Severity, Span};
use ruff_source_file::SourceFile;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::codes::NoqaCode;
use crate::registry::AsRule;
use crate::violation::Violation;
use crate::{Fix, codes::Rule};

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct OldDiagnostic {
    pub diagnostic: db::Diagnostic,
    pub fix: Option<Fix>,
    pub parent: Option<TextSize>,
    pub(crate) noqa_offset: Option<TextSize>,
    pub(crate) noqa_code: Option<NoqaCode>,
}

impl OldDiagnostic {
    // TODO(brent) We temporarily allow this to avoid updating all of the call sites to add
    // references. I expect this method to go away or change significantly with the rest of the
    // diagnostic refactor, but if it still exists in this form at the end of the refactor, we
    // should just update the call sites.
    #[expect(clippy::needless_pass_by_value)]
    pub fn new<T: Violation>(kind: T, range: TextRange, file: &SourceFile) -> Self {
        let rule = T::rule();

        let mut diagnostic = db::Diagnostic::new(
            DiagnosticId::Lint(LintName::of(rule.into())),
            Severity::Error,
            Violation::message(&kind),
        );
        let span = Span::from(file.clone()).with_range(range);
        let mut annotation = Annotation::primary(span);
        if let Some(suggestion) = Violation::fix_title(&kind) {
            annotation = annotation.message(suggestion);
        }
        diagnostic.annotate(annotation);

        Self {
            diagnostic,
            fix: None,
            parent: None,
            noqa_offset: None,
            noqa_code: Some(rule.noqa_code()),
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
            Err(err) => debug!("Failed to create fix for {}: {}", self.rule(), err),
        }
    }

    /// Set the [`Fix`] used to fix the diagnostic, if the provided function returns `Ok`.
    /// Otherwise, log the error.
    #[inline]
    pub fn try_set_optional_fix(&mut self, func: impl FnOnce() -> Result<Option<Fix>>) {
        match func() {
            Ok(None) => {}
            Ok(Some(fix)) => self.fix = Some(fix),
            Err(err) => debug!("Failed to create fix for {}: {}", self.rule(), err),
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

    /// Consumes `self` and returns a new `Diagnostic` with the given noqa offset.
    #[inline]
    #[must_use]
    pub fn with_noqa_offset(mut self, noqa_offset: TextSize) -> Self {
        self.noqa_offset = Some(noqa_offset);
        self
    }
}

impl AsRule for OldDiagnostic {
    fn rule(&self) -> Rule {
        self.diagnostic
            .id()
            .as_str()
            .parse()
            .expect("Expected valid rule name for ruff diagnostic")
    }
}

impl Ranged for OldDiagnostic {
    fn range(&self) -> TextRange {
        self.diagnostic
            .expect_primary_span()
            .range()
            .expect("Expected range for ruff span")
    }
}
