//! Shared suppression validation and ownership of unused-directive candidates.

use std::convert::Infallible;

use ruff_db::PythonFile;
use ruff_db::diagnostic::Diagnostic;
use ruff_db::source::{SourceText, source_text};
use ty_mapping_probe_macros::shared_semantic_family;

use super::{
    BLANKET_IGNORE_COMMENT, CheckSuppressionsContext, FileSuppressionId,
    IGNORE_COMMENT_UNKNOWN_RULE, INVALID_IGNORE_COMMENT, Suppression, SuppressionKind,
    SuppressionTarget, Suppressions, UNUSED_IGNORE_COMMENT, UNUSED_TYPE_IGNORE_COMMENT,
    selection, suppressions, unused,
};
use crate::Db;
use crate::lint::{LintId, LintMetadata, RuleSelection};
use crate::types::TypeCheckDiagnostics;

#[derive(Clone, Copy)]
pub(crate) enum SuppressionLint {
    UnknownRule,
    Invalid,
    Blanket,
    Unused,
    UnusedType,
}

impl SuppressionLint {
    pub(crate) fn metadata(self) -> &'static LintMetadata {
        match self {
            Self::UnknownRule => &IGNORE_COMMENT_UNKNOWN_RULE,
            Self::Invalid => &INVALID_IGNORE_COMMENT,
            Self::Blanket => &BLANKET_IGNORE_COMMENT,
            Self::Unused => &UNUSED_IGNORE_COMMENT,
            Self::UnusedType => &UNUSED_TYPE_IGNORE_COMMENT,
        }
    }
}

/// The file owner retains these allocations while an effect can refuse admission.
#[derive(Default)]
pub(crate) struct SuppressionCheckState<'db> {
    unused: Vec<&'db Suppression>,
    rendered: usize,
}

impl<'db> SuppressionCheckState<'db> {
    #[cfg(feature = "experimental-analysis")]
    pub(crate) fn storage(&self) -> (usize, usize) {
        (self.unused.len(), self.unused.capacity())
    }

    pub(crate) fn reserve(&mut self, capacity: usize) {
        self.unused.reserve_exact(capacity);
    }

    pub(crate) fn push(&mut self, suppression: &'db Suppression) {
        self.unused.push(suppression);
    }

    pub(crate) fn next(&mut self) -> Option<&'db Suppression> {
        let suppression = self.unused.get(self.rendered).copied();
        self.rendered += usize::from(suppression.is_some());
        suppression
    }
}

pub(crate) struct SuppressionFacts;

shared_semantic_family! {
    #[synchronous(SynchronousSuppressionEffects)]
    pub(crate) trait SuppressionEffects<'db> {
        type Error;
        #[operation(local)]
        async fn is_lint_disabled(&self, lint: SuppressionLint) -> Result<bool, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_unknown(&self, cursor: &mut usize) -> Result<Option<usize>, Self::Error>;
        #[operation(source)]
        async fn report_unknown(&self, diagnostics: &mut TypeCheckDiagnostics, index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_invalid(&self, cursor: &mut usize) -> Result<Option<usize>, Self::Error>;
        #[operation(source)]
        async fn report_invalid(&self, diagnostics: &mut TypeCheckDiagnostics, index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_suppression(&self, cursor: &mut usize) -> Result<Option<&'db Suppression>, Self::Error>;
        #[operation(source)]
        async fn preferred_suppression(&self, suppression: &'db Suppression, unused: bool) -> Result<Option<FileSuppressionId>, Self::Error>;
        #[operation(local)]
        async fn mark_used(&self, diagnostics: &mut TypeCheckDiagnostics, id: FileSuppressionId) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn report_blanket(&self, diagnostics: &mut TypeCheckDiagnostics, suppression: &'db Suppression) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn prepare_unused(&self, diagnostics: &TypeCheckDiagnostics, state: &mut SuppressionCheckState<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn is_used(&self, diagnostics: &TypeCheckDiagnostics, suppression: &'db Suppression) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn push_unused(&self, state: &mut SuppressionCheckState<'db>, suppression: &'db Suppression) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn source(&self) -> Result<SourceText, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_unused(&self, state: &mut SuppressionCheckState<'db>) -> Result<Option<&'db Suppression>, Self::Error>;
        #[operation(source)]
        async fn report_unused(&self, diagnostics: &mut TypeCheckDiagnostics, state: &mut SuppressionCheckState<'db>, suppression: &'db Suppression, source: &SourceText) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl SuppressionFacts {
        fn is_blanket(&self, suppression: &Suppression) -> bool {
            suppression.kind == SuppressionKind::Ty && suppression.target == SuppressionTarget::All
        }
    }

    #[synchronous(check_suppressions_sync)]
    #[capabilities(effects = SuppressionEffects, facts = SuppressionFacts)]
    #[passive_values(SuppressionLint::UnknownRule, SuppressionLint::Invalid, SuppressionLint::Blanket, SuppressionLint::Unused, SuppressionLint::UnusedType)]
    pub(crate) async fn check_suppressions_with<'db, E: SuppressionEffects<'db>>(
        diagnostics: &mut TypeCheckDiagnostics,
        state: &mut SuppressionCheckState<'db>,
        facts: SuppressionFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        if !effects.is_lint_disabled(SuppressionLint::UnknownRule).await? {
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(index) = effects.next_unknown(&mut cursor).await? {
                effects.report_unknown(diagnostics, index).await?;
            }
        }
        if !effects.is_lint_disabled(SuppressionLint::Invalid).await? {
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(index) = effects.next_invalid(&mut cursor).await? {
                effects.report_invalid(diagnostics, index).await?;
            }
        }
        if !effects.is_lint_disabled(SuppressionLint::Blanket).await? {
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(suppression) = effects.next_suppression(&mut cursor).await? {
                if !facts.is_blanket(suppression) {
                    continue;
                }
                // A blanket directive cannot suppress itself; a lint-specific directive can.
                if let Some(id) = effects.preferred_suppression(suppression, false).await? {
                    effects.mark_used(diagnostics, id).await?;
                } else {
                    effects.report_blanket(diagnostics, suppression).await?;
                }
            }
        }
        if effects.is_lint_disabled(SuppressionLint::Unused).await?
            && effects.is_lint_disabled(SuppressionLint::UnusedType).await?
        {
            return Ok(());
        }

        effects.prepare_unused(diagnostics, state).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(suppression) = effects.next_suppression(&mut cursor).await? {
            if effects.is_used(diagnostics, suppression).await? {
                continue;
            }
            // Only a distinct, lint-specific directive can suppress an unused-directive
            // diagnostic. A blanket directive would otherwise suppress its own diagnostic.
            if let Some(id) = effects.preferred_suppression(suppression, true).await? {
                effects.mark_used(diagnostics, id).await?;
                continue;
            }
            effects.push_unused(state, suppression).await?;
        }

        let source = effects.source().await?;
        #[cursor_loop]
        while let Some(suppression) = effects.next_unused(state).await? {
            // A later code can mark an earlier candidate as used during collection, as in
            // `a = 10 / 2  # ty: ignore[unused-ignore-comment, division-by-zero]`.
            if effects.is_used(diagnostics, suppression).await? {
                continue;
            }
            effects.report_unused(diagnostics, state, suppression, &source).await?;
        }
        Ok(())
    }
}

pub(super) fn check_suppressions(
    db: &dyn Db,
    file: PythonFile<'_>,
    mut diagnostics: TypeCheckDiagnostics,
) -> Vec<Diagnostic> {
    let effects = OrdinarySuppressionEffects {
        db,
        file,
        suppressions: suppressions(db, file),
    };
    let mut state = SuppressionCheckState::default();
    match check_suppressions_sync(&mut diagnostics, &mut state, SuppressionFacts, &effects) {
        Ok(()) => diagnostics.into_diagnostics(),
        Err(error) => match error {},
    }
}

struct OrdinarySuppressionEffects<'db> {
    db: &'db dyn Db,
    file: PythonFile<'db>,
    suppressions: &'db Suppressions,
}

impl OrdinarySuppressionEffects<'_> {
    fn report(
        &self,
        diagnostics: &mut TypeCheckDiagnostics,
        report: impl FnOnce(&CheckSuppressionsContext),
    ) {
        let context =
            CheckSuppressionsContext::new(self.db, self.file, std::mem::take(diagnostics));
        report(&context);
        *diagnostics = context.diagnostics.into_inner();
    }
}

impl<'db> SynchronousSuppressionEffects<'db> for OrdinarySuppressionEffects<'db> {
    type Error = Infallible;

    fn is_lint_disabled(&self, lint: SuppressionLint) -> Result<bool, Self::Error> {
        Ok(is_lint_disabled(
            self.db.rule_selection(self.file.file(self.db)),
            lint,
        ))
    }

    fn next_unknown(&self, cursor: &mut usize) -> Result<Option<usize>, Self::Error> {
        Ok(next_unknown(self.suppressions, cursor))
    }

    fn report_unknown(
        &self,
        diagnostics: &mut TypeCheckDiagnostics,
        index: usize,
    ) -> Result<(), Self::Error> {
        self.report(diagnostics, |context| {
            let unknown = &self.suppressions.unknown[index];
            if let Some(diag) = context.report_lint(&IGNORE_COMMENT_UNKNOWN_RULE, unknown.range) {
                diag.into_diagnostic(&unknown.reason);
            }
        });
        Ok(())
    }

    fn next_invalid(&self, cursor: &mut usize) -> Result<Option<usize>, Self::Error> {
        Ok(next_invalid(self.suppressions, cursor))
    }

    fn report_invalid(
        &self,
        diagnostics: &mut TypeCheckDiagnostics,
        index: usize,
    ) -> Result<(), Self::Error> {
        self.report(diagnostics, |context| {
            let invalid = &self.suppressions.invalid[index];
            if let Some(diag) = context.report_lint(&INVALID_IGNORE_COMMENT, invalid.error.range) {
                diag.into_diagnostic(format_args!(
                    "Invalid `{kind}` comment: {reason}",
                    kind = invalid.kind,
                    reason = invalid.error
                ));
            }
        });
        Ok(())
    }

    fn next_suppression(
        &self,
        cursor: &mut usize,
    ) -> Result<Option<&'db Suppression>, Self::Error> {
        Ok(next_suppression(self.suppressions, cursor))
    }

    fn preferred_suppression(
        &self,
        suppression: &'db Suppression,
        unused: bool,
    ) -> Result<Option<FileSuppressionId>, Self::Error> {
        let lint = if unused {
            &UNUSED_IGNORE_COMMENT
        } else {
            &BLANKET_IGNORE_COMMENT
        };
        let mode = if unused {
            selection::SelectionMode::SpecificExcept(suppression.id())
        } else {
            selection::SelectionMode::Specific
        };
        Ok(selection::select(
            self.suppressions,
            suppression.range,
            LintId::of(lint),
            mode,
        )
        .map(Suppression::id))
    }

    fn mark_used(
        &self,
        diagnostics: &mut TypeCheckDiagnostics,
        id: FileSuppressionId,
    ) -> Result<(), Self::Error> {
        diagnostics.mark_used(id);
        Ok(())
    }

    fn report_blanket(
        &self,
        diagnostics: &mut TypeCheckDiagnostics,
        suppression: &'db Suppression,
    ) -> Result<(), Self::Error> {
        self.report(diagnostics, |context| {
            if let Some(diag) = context.report_unchecked(&BLANKET_IGNORE_COMMENT, suppression.range)
            {
                diag.into_diagnostic("Use specific rule codes in `ty: ignore`");
            }
        });
        Ok(())
    }

    fn prepare_unused(
        &self,
        diagnostics: &TypeCheckDiagnostics,
        state: &mut SuppressionCheckState<'db>,
    ) -> Result<(), Self::Error> {
        state.reserve(unused_capacity(self.suppressions, diagnostics));
        Ok(())
    }

    fn is_used(
        &self,
        diagnostics: &TypeCheckDiagnostics,
        suppression: &'db Suppression,
    ) -> Result<bool, Self::Error> {
        Ok(diagnostics.is_used(suppression.id()))
    }

    fn push_unused(
        &self,
        state: &mut SuppressionCheckState<'db>,
        suppression: &'db Suppression,
    ) -> Result<(), Self::Error> {
        state.push(suppression);
        Ok(())
    }

    fn source(&self) -> Result<SourceText, Self::Error> {
        Ok(source_text(self.db, self.file.file(self.db)))
    }

    fn next_unused(
        &self,
        state: &mut SuppressionCheckState<'db>,
    ) -> Result<Option<&'db Suppression>, Self::Error> {
        Ok(state.next())
    }

    fn report_unused(
        &self,
        diagnostics: &mut TypeCheckDiagnostics,
        state: &mut SuppressionCheckState<'db>,
        suppression: &'db Suppression,
        source: &SourceText,
    ) -> Result<(), Self::Error> {
        let mut consumed = 0;
        self.report(diagnostics, |context| {
            unused::report_unused_suppression(
                context,
                suppression,
                &state.unused[state.rendered..],
                &mut consumed,
                source,
            );
        });
        state.rendered += consumed;
        Ok(())
    }
}

pub(crate) fn is_lint_disabled(rules: &RuleSelection, lint: SuppressionLint) -> bool {
    !rules.is_enabled(LintId::of(lint.metadata()))
}

pub(crate) fn next_unknown(suppressions: &Suppressions, cursor: &mut usize) -> Option<usize> {
    let index = suppressions.unknown.get(*cursor).map(|_| *cursor);
    *cursor += usize::from(index.is_some());
    index
}

pub(crate) fn next_invalid(suppressions: &Suppressions, cursor: &mut usize) -> Option<usize> {
    let index = suppressions.invalid.get(*cursor).map(|_| *cursor);
    *cursor += usize::from(index.is_some());
    index
}

pub(crate) fn next_suppression<'db>(
    suppressions: &'db Suppressions,
    cursor: &mut usize,
) -> Option<&'db Suppression> {
    let suppression = if *cursor < suppressions.file.len() {
        suppressions.file.get(*cursor)
    } else {
        suppressions
            .inline
            .entries
            .get(*cursor - suppressions.file.len())
            .map(|entry| &entry.suppression)
    };
    *cursor += usize::from(suppression.is_some());
    suppression
}

pub(crate) fn unused_capacity(
    suppressions: &Suppressions,
    diagnostics: &TypeCheckDiagnostics,
) -> usize {
    suppressions
        .file
        .len()
        .saturating_add(suppressions.inline.len())
        .saturating_sub(diagnostics.used_len())
}
