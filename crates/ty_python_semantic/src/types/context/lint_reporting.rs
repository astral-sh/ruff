//! Shared lint eligibility and metadata for ordinary and controlled diagnostic construction.

use std::convert::Infallible;

use ruff_db::diagnostic::{Diagnostic, Severity, Span};
use ruff_text_size::TextRange;

use super::{InferContext, OrdinaryLintEligibilityEffects, lint_severity_sync};
use crate::lint::{LintId, LintMetadata, LintSource};
use crate::suppression::{FileSuppressionId, suppressions};

/// The selected lint and range after policy, suppression and reachability checks.
#[derive(Debug, Clone, Copy)]
pub(in crate::types) struct EligibleLint {
    pub(crate) id: LintId,
    pub(crate) severity: Severity,
    pub(crate) source: LintSource,
    pub(crate) range: TextRange,
}

/// Metadata used to construct one owned diagnostic without publishing it on drop.
#[derive(Debug)]
pub(in crate::types) struct LintReportMetadata {
    pub(crate) id: LintId,
    pub(crate) severity: Severity,
    pub(crate) source: LintSource,
    pub(crate) primary_span: Span,
    pub(crate) verbose: bool,
}

/// Selects a lint report without constructing or publishing its diagnostic.
pub(in crate::types) fn begin_lint_report(
    context: &InferContext<'_, '_>,
    lint: &'static LintMetadata,
    range: TextRange,
) -> Option<LintReportMetadata> {
    let eligible = match lint_report_eligibility_sync(
        context,
        LintId::of(lint),
        range,
        &OrdinaryLintReportEligibilityEffects,
    ) {
        Ok(eligible) => eligible?,
        Err(never) => match never {},
    };
    Some(LintReportMetadata {
        id: eligible.id,
        severity: eligible.severity,
        source: eligible.source,
        primary_span: Span::from(context.file()).with_range(eligible.range),
        verbose: context.db().verbose(),
    })
}

/// Adds common lint metadata and moves the finished diagnostic into its inference context.
pub(in crate::types) fn finish_lint_report(
    context: &InferContext<'_, '_>,
    metadata: LintReportMetadata,
    diagnostic: Diagnostic,
) {
    match finalize_lint_report_sync(context, metadata, diagnostic, &OrdinaryLintFinalizationEffects) {
        Ok(()) => {}
        Err(never) => match never {},
    }
}

/// The fixed text after a lint name in verbose rule-selection information.
pub(in crate::types) const fn verbose_lint_suffix(source: LintSource) -> &'static str {
    match source {
        LintSource::Default => "` is enabled by default",
        LintSource::Cli => "` was selected on the command line",
        LintSource::File => "` was selected in the configuration file",
        LintSource::ScriptMetadata => "` was selected in script metadata",
        LintSource::Editor => "` was selected in the editor settings",
        LintSource::UvMetadata => "` was selected by uv metadata",
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousLintReportEligibilityEffects)]
    pub(in crate::types) trait LintReportEligibilityEffects<'db, 'ast> {
        type Error;

        #[operation(source)]
        async fn policy(&self, context: &InferContext<'db, 'ast>, lint: LintId) -> Result<Option<(Severity, LintSource)>, Self::Error>;
        #[operation(source)]
        async fn suppression(&self, context: &InferContext<'db, 'ast>, lint: LintId, range: TextRange) -> Result<Option<FileSuppressionId>, Self::Error>;
        #[operation(local)]
        async fn mark_used(&self, context: &InferContext<'db, 'ast>, id: FileSuppressionId) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn reachable(&self, context: &InferContext<'db, 'ast>, range: TextRange) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn eligible(&self, id: LintId, severity: Severity, source: LintSource, range: TextRange) -> Result<EligibleLint, Self::Error>;
    }

    #[synchronous(lint_report_eligibility_sync)]
    #[capabilities(effects = LintReportEligibilityEffects)]
    #[passive_values()]
    pub(in crate::types) async fn lint_report_eligibility_with<'db, 'ast, E: LintReportEligibilityEffects<'db, 'ast>>(
        context: &InferContext<'db, 'ast>,
        lint: LintId,
        range: TextRange,
        effects: &E,
    ) -> Result<Option<EligibleLint>, E::Error> {
        let Some((severity, source)) = effects.policy(context, lint).await? else {
            return Ok(None);
        };
        if let Some(suppression) = effects.suppression(context, lint, range).await? {
            effects.mark_used(context, suppression).await?;
            return Ok(None);
        }
        // Suppress diagnostics in unreachable code. This checks both whether
        // the scope itself is unreachable and whether the specific statement or
        // expression containing this diagnostic is unreachable.
        if !effects.reachable(context, range).await? {
            return Ok(None);
        }
        Ok(Some(effects.eligible(lint, severity, source, range).await?))
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousLintFinalizationEffects)]
    pub(in crate::types) trait LintFinalizationEffects<'db, 'ast> {
        type Error;

        #[operation(source)]
        async fn documentation(&self, diagnostic: &mut Diagnostic, metadata: &LintReportMetadata) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn verbose(&self, metadata: &LintReportMetadata) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn information(&self, diagnostic: &mut Diagnostic, metadata: &LintReportMetadata) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn publish(&self, context: &InferContext<'db, 'ast>, diagnostic: Diagnostic) -> Result<(), Self::Error>;
    }

    #[synchronous(finalize_lint_report_sync)]
    #[capabilities(effects = LintFinalizationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn finalize_lint_report_with<'db, 'ast, E: LintFinalizationEffects<'db, 'ast>>(
        context: &InferContext<'db, 'ast>,
        metadata: LintReportMetadata,
        mut diagnostic: Diagnostic,
        effects: &E,
    ) -> Result<(), E::Error> {
        effects.documentation(&mut diagnostic, &metadata).await?;
        if effects.verbose(&metadata).await? {
            effects.information(&mut diagnostic, &metadata).await?;
        }
        effects.publish(context, diagnostic).await
    }
}

#[derive(Debug)]
struct OrdinaryLintFinalizationEffects;

impl<'db, 'ast> SynchronousLintFinalizationEffects<'db, 'ast>
    for OrdinaryLintFinalizationEffects
{
    type Error = Infallible;

    fn documentation(
        &self,
        diagnostic: &mut Diagnostic,
        metadata: &LintReportMetadata,
    ) -> Result<(), Self::Error> {
        diagnostic.set_documentation_url(Some(metadata.id.documentation_url()));
        Ok(())
    }

    fn verbose(&self, metadata: &LintReportMetadata) -> Result<bool, Self::Error> {
        Ok(metadata.verbose)
    }

    fn information(
        &self,
        diagnostic: &mut Diagnostic,
        metadata: &LintReportMetadata,
    ) -> Result<(), Self::Error> {
        let name = metadata.id.name();
        let suffix = verbose_lint_suffix(metadata.source);
        diagnostic.info(format!("rule `{name}{suffix}"));
        Ok(())
    }

    fn publish(
        &self,
        context: &InferContext<'db, 'ast>,
        diagnostic: Diagnostic,
    ) -> Result<(), Self::Error> {
        context.push_lint_diagnostic(diagnostic);
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct OrdinaryLintReportEligibilityEffects;

impl<'db, 'ast> SynchronousLintReportEligibilityEffects<'db, 'ast>
    for OrdinaryLintReportEligibilityEffects
{
    type Error = Infallible;

    fn policy(
        &self,
        context: &InferContext<'db, 'ast>,
        lint: LintId,
    ) -> Result<Option<(Severity, LintSource)>, Self::Error> {
        lint_severity_sync(context, lint, &OrdinaryLintEligibilityEffects)
    }

    fn suppression(
        &self,
        context: &InferContext<'db, 'ast>,
        lint: LintId,
        range: TextRange,
    ) -> Result<Option<FileSuppressionId>, Self::Error> {
        Ok(suppressions(context.db(), context.python_file())
            .find_suppression(range, lint)
            .map(|suppression| suppression.id()))
    }

    fn mark_used(
        &self,
        context: &InferContext<'db, 'ast>,
        id: FileSuppressionId,
    ) -> Result<(), Self::Error> {
        context.mark_lint_suppression_used(id);
        Ok(())
    }

    fn reachable(
        &self,
        context: &InferContext<'db, 'ast>,
        range: TextRange,
    ) -> Result<bool, Self::Error> {
        Ok(context.is_range_reachable(range))
    }

    fn eligible(
        &self,
        id: LintId,
        severity: Severity,
        source: LintSource,
        range: TextRange,
    ) -> Result<EligibleLint, Self::Error> {
        Ok(EligibleLint { id, severity, source, range })
    }
}
