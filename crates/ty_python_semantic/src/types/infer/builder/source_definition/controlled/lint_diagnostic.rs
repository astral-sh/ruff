//! Controlled lint eligibility and explicit publication of uniquely owned diagnostics.

use ruff_db::diagnostic::{Diagnostic, DiagnosticMessage, Severity, Span};
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};

use super::storage::table_merge;
use super::lint_diagnostic_cost::{
    BufferQuotePreparation, ReportingMetadataOperation, buffer_quote_preparation, context_mutation_quote,
    context_storage_quote, context_reservation_preparation_quote, diagnostic_info_quote,
    exact_filled_string_message_quote, lint_metadata_quote, lint_text_parts_quote,
    prepared_vec_push_quote, reporting_metadata_quote, string_push_str_quote, string_with_capacity_quote,
    suppression_storage_preparation_quote, unique_diagnostic_mutation_quote,
    vec_reserve_exact_quote,
};
use super::{SourceAccess, SourceEffects};
use crate::lint::{LINT_DOCUMENTATION_URL_PREFIX, LintId, LintMetadata, LintSource};
use crate::suppression::selection::{
    DescentStorage, PendingIntervals, SelectionCursor, SelectionEffects, SelectionMode,
    SelectionStep, select_with,
};
use crate::suppression::{FileSuppressionId, Suppression};
use crate::types::context::lint_reporting::{
    EligibleLint, LintFinalizationEffects, LintReportEligibilityEffects, finalize_lint_report_with,
    lint_report_eligibility_with, verbose_lint_suffix,
};
use crate::types::context::{InferContext, LintReportMetadata};
use crate::types::infer::TypeInferenceBuilder;

/// Borrows the unpublished inference owner while lint eligibility and finalization can suspend.
struct LintReportEffects<'effects, 'access, 'run, 'db: 'run, 'ast, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    builder: &'effects TypeInferenceBuilder<'db, 'ast>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Moves an exactly filled string into a diagnostic message after admitting its carriers.
    ///
    /// The caller has prepaid the backing allocation and its eventual disposal. The string must
    /// fill its requested capacity, so this conversion cannot shrink or reallocate that backing.
    pub(in crate::types::infer::builder) async fn finish_lint_message(
        &self,
        text: String,
    ) -> RunResult<DiagnosticMessage> {
        self.local_quoted_with_fixed_transfers(
            const { exact_filled_string_message_quote() },
            || DiagnosticMessage::from(text.into_boxed_str()),
        ).await
    }

    /// Selects a report without constructing a diagnostic or publishing an inference result.
    pub(in crate::types::infer::builder) async fn begin_lint_report(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        lint: &'static LintMetadata,
        range: TextRange,
    ) -> RunResult<Option<LintReportMetadata>> {
        let effects = self
            .local_quoted_with_fixed_transfers(const { Ok((5, 5 * size_of::<(&Self, &TypeInferenceBuilder<'db, '_>)>())) }, || LintReportEffects { source: self, builder })
            .await?;
        let id = self
            .local_quoted_with_fixed_transfers(const { reporting_metadata_quote(ReportingMetadataOperation::LintId) }, || LintId::of(lint))
            .await?;
        let eligible = self
            .boxed_future_with_fixed_transfers(
                Ok((15, size_of::<[(&InferContext<'db, '_>, LintId, TextRange, &LintReportEffects<'_, '_, 'run, 'db, '_, A>); 2]>())),
                || lint_report_eligibility_with(&builder.context, id, range, &effects),
            )
            .await?
            .await?;
        let Some(eligible) = eligible else {
            return Ok(None);
        };
        let file = self.local_quoted_with_fixed_transfers(const { reporting_metadata_quote(ReportingMetadataOperation::File) }, || builder.context.file()).await?;
        let verbose = self
            .boxed_future_with_fixed_transfers(
                Ok((9, size_of::<[(&A, ruff_db::files::File); 2]>())),
                || self.access.verbose(file),
            )
            .await?
            .await?;
        self.local_quoted_with_fixed_transfers(
            const { lint_metadata_quote() },
            || Some(LintReportMetadata {
                id: eligible.id,
                severity: eligible.severity,
                source: eligible.source,
                primary_span: Span::from(file).with_range(eligible.range),
                verbose,
            }),
        ).await
    }

    /// Adds lint metadata and inserts a finished diagnostic after admitting destination storage.
    ///
    /// The caller keeps the diagnostic unique and reserves one spare subdiagnostic slot when
    /// `metadata.verbose` is true. Its existing contents have prepaid retirement; this operation
    /// pays for the URL and verbose information it adds. Failure disposes of the unpublished value.
    pub(in crate::types::infer::builder) async fn finish_lint_report(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        metadata: LintReportMetadata,
        diagnostic: Diagnostic,
    ) -> RunResult<()> {
        let effects = self
            .local_quoted_with_fixed_transfers(const { Ok((5, 5 * size_of::<(&Self, &TypeInferenceBuilder<'db, '_>)>())) }, || LintReportEffects { source: self, builder })
            .await?;
        self.boxed_future_with_fixed_transfers(
            Ok((15, size_of::<[(&InferContext<'db, '_>, LintReportMetadata, Diagnostic, &LintReportEffects<'_, '_, 'run, 'db, '_, A>); 2]>())),
            || finalize_lint_report_with(&builder.context, metadata, diagnostic, &effects),
        ).await?.await
    }

    /// Copies the common lint text's prefix, rule name and suffix into exact string storage.
    async fn lint_text(&self, parts: [&str; 3]) -> RunResult<String> {
        let (prefix, name, suffix, length) = self.local_quoted_with_fixed_transfers(
            const { lint_text_parts_quote() },
            || {
                let [prefix, name, suffix] = parts;
                let prefix = (prefix, prefix.len());
                let name = (name, name.len());
                let suffix = (suffix, suffix.len());
                let Some(length) = prefix.1.checked_add(name.1)
                    .and_then(|length| length.checked_add(suffix.1)) else {
                    return Err(RunError::Contract("lint message length overflow"));
                };
                Ok((prefix, name, suffix, length))
            },
        ).await??;
        let quote = self.local_quoted_with_fixed_transfers(
            const { buffer_quote_preparation(BufferQuotePreparation::StringWithCapacity) },
            || string_with_capacity_quote(length),
        ).await??;
        let mut text = self.local_quoted_with_fixed_transfers(
            Ok(quote),
            || String::with_capacity(length),
        ).await?;
        self.append_lint_text(&mut text, prefix).await?;
        self.append_lint_text(&mut text, name).await?;
        self.append_lint_text(&mut text, suffix).await?;
        Ok(text)
    }

    /// Appends one fragment after admitting its transfer into already prepared string capacity.
    async fn append_lint_text(&self, text: &mut String, part: (&str, usize)) -> RunResult<()> {
        let quote = self.local_quoted_with_fixed_transfers(
            const {
                match buffer_quote_preparation(BufferQuotePreparation::StringPushStr) {
                    Ok((work, bytes)) => match (work.checked_add(1), bytes.checked_add(size_of::<usize>())) {
                        (Some(work), Some(bytes)) => Ok((work, bytes)),
                        _ => Err(RunError::Contract("lint append preparation quotation overflow")),
                    },
                    Err(error) => Err(error),
                }
            },
            || string_push_str_quote(part.1),
        ).await??;
        self.local_quoted_with_fixed_transfers(
            Ok(quote),
            || text.push_str(part.0),
        ).await
    }

    /// Finds the ordinary preferred suppression through individually admitted interval steps.
    async fn select_lint_suppression(
        &self,
        context: &InferContext<'db, '_>,
        lint: LintId,
        range: TextRange,
    ) -> RunResult<Option<FileSuppressionId>> {
        let file = self.local_quoted_with_fixed_transfers(const { reporting_metadata_quote(ReportingMetadataOperation::File) }, || context.file()).await?;
        let suppressions = self.boxed_future_with_fixed_transfers(
            Ok((9, size_of::<[(&A, ruff_db::files::File); 2]>())),
            || self.access.suppressions(file),
        ).await?.await?;
        let mut cursor = self.local_quoted_with_fixed_transfers(
            const { Ok(SelectionCursor::new_quote()) },
            || SelectionCursor::new(suppressions, range, lint, SelectionMode::All),
        ).await?;
        let selected = self.boxed_future_with_fixed_transfers(
            Ok((8, size_of::<[(&mut SelectionCursor<'db>, &Self); 2]>())),
            || select_with(&mut cursor, self),
        ).await?.await?;
        self.local_quoted_with_fixed_transfers(const { reporting_metadata_quote(ReportingMetadataOperation::SuppressionId) }, || selected.map(Suppression::id)).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SelectionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn advance(&self, cursor: &mut SelectionCursor<'db>) -> RunResult<Option<SelectionStep<'db>>> {
        self.local_quoted_with_fixed_transfers(
            const { Ok(SelectionCursor::advance_quote()) },
            || cursor.advance(),
        ).await
    }

    async fn descend(&self, cursor: &mut SelectionCursor<'db>, intervals: PendingIntervals<'db>) -> RunResult<()> {
        let quote = self.local_quoted_with_fixed_transfers(
            const { Ok(SelectionCursor::descent_quote_quote()) },
            || cursor.storage().descent(intervals).and_then(DescentStorage::quote)
                .ok_or(RunError::Contract("lint suppression descent quotation overflow")),
        ).await??;
        self.local_quoted_with_fixed_transfers(Ok(quote), || cursor.descend(intervals)).await
    }

    async fn consider(&self, cursor: &mut SelectionCursor<'db>, candidate: &'db Suppression) -> RunResult<Option<&'db Suppression>> {
        self.local_quoted_with_fixed_transfers(
            const { Ok(SelectionCursor::consider_quote()) },
            || cursor.consider(candidate),
        ).await
    }

    async fn finish(&self, cursor: &SelectionCursor<'db>) -> RunResult<Option<&'db Suppression>> {
        self.local_quoted_with_fixed_transfers(
            const { Ok(SelectionCursor::finish_quote()) },
            || cursor.finish(),
        ).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> LintReportEligibilityEffects<'db, 'ast>
    for LintReportEffects<'_, '_, 'run, 'db, 'ast, A>
{
    type Error = RunError;

    async fn policy(
        &self,
        _context: &InferContext<'db, 'ast>,
        lint: LintId,
    ) -> RunResult<Option<(Severity, LintSource)>> {
        self.source.boxed_future_with_fixed_transfers(
            Ok((13, size_of::<[(&SourceEffects<'_, 'run, 'db, A>, &TypeInferenceBuilder<'db, 'ast>, LintId); 2]>())),
            || self.source.lint_severity_source(self.builder, lint),
        ).await?.await
    }

    async fn suppression(
        &self,
        context: &InferContext<'db, 'ast>,
        lint: LintId,
        range: TextRange,
    ) -> RunResult<Option<FileSuppressionId>> {
        self.source.select_lint_suppression(context, lint, range).await
    }

    async fn mark_used(
        &self,
        context: &InferContext<'db, 'ast>,
        id: FileSuppressionId,
    ) -> RunResult<()> {
        let (_, _, len, capacity) = self.source.local_quoted_with_fixed_transfers(
            const { context_storage_quote() }, || context.retained_diagnostics().storage(),
        ).await?;
        let quote = self.source.local_quoted_with_fixed_transfers(
            const { suppression_storage_preparation_quote() },
            || {
                let (quote, slots) = match table_merge::<FileSuppressionId>(len, capacity, 1, 0)
                    .ok_or(RunError::Contract("lint suppression storage overflow")) {
                    Ok(quote) => quote,
                    Err(error) => return Err(error),
                };
                // Existing entries were already owned. A replacement table needs its own future
                // retirement; an insertion into retained capacity adds only one live entry to retire.
                let retirement = if quote.bytes == 0 {
                    1
                } else {
                    let Some(retirement) = slots.checked_add(2) else {
                        return Err(RunError::Contract("lint suppression retirement overflow"));
                    };
                    retirement
                };
                let (borrow_work, borrow_bytes) = match const { context_mutation_quote() } {
                    Ok(quote) => quote,
                    Err(error) => return Err(error),
                };
                let Some(work) = quote.work.checked_add(retirement) else {
                    return Err(RunError::Contract("lint suppression work overflow"));
                };
                let Some(work) = work.checked_add(borrow_work) else {
                    return Err(RunError::Contract("lint suppression work overflow"));
                };
                let Some(bytes) = quote.bytes.checked_add(borrow_bytes) else {
                    return Err(RunError::Contract("lint suppression bytes overflow"));
                };
                let Some(bytes) = bytes.checked_add(size_of::<[FileSuppressionId; 4]>()) else {
                    return Err(RunError::Contract("lint suppression bytes overflow"));
                };
                Ok((work, bytes))
            },
        ).await??;
        self.source.local_quoted_with_fixed_transfers(
            Ok(quote),
            || context.mark_lint_suppression_used(id),
        ).await
    }

    async fn reachable(
        &self,
        _context: &InferContext<'db, 'ast>,
        range: TextRange,
    ) -> RunResult<bool> {
        self.source.boxed_future_with_fixed_transfers(
            Ok((13, size_of::<[(&SourceEffects<'_, 'run, 'db, A>, &TypeInferenceBuilder<'db, 'ast>, TextRange); 2]>())),
            || self.source.is_range_reachable_source(self.builder, range),
        ).await?.await
    }

    async fn eligible(
        &self,
        id: LintId,
        severity: Severity,
        source: LintSource,
        range: TextRange,
    ) -> RunResult<EligibleLint> {
        self.source.local_quoted_with_fixed_transfers(const { reporting_metadata_quote(ReportingMetadataOperation::Eligible) }, || EligibleLint { id, severity, source, range }).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> LintFinalizationEffects<'db, 'ast>
    for LintReportEffects<'_, '_, 'run, 'db, 'ast, A>
{
    type Error = RunError;

    async fn documentation(
        &self,
        diagnostic: &mut Diagnostic,
        metadata: &LintReportMetadata,
    ) -> RunResult<()> {
        let name = self.source.local_quoted_with_fixed_transfers(const { reporting_metadata_quote(ReportingMetadataOperation::Name) }, || metadata.id.name().as_str()).await?;
        let url = self.source.lint_text([LINT_DOCUMENTATION_URL_PREFIX, name, ""]).await?;
        self.source.local_quoted_with_fixed_transfers(
            const {
                match (unique_diagnostic_mutation_quote(), reporting_metadata_quote(ReportingMetadataOperation::DocumentationField)) {
                    (Ok((mutation_work, mutation_bytes)), Ok((field_work, field_bytes))) => {
                        match (mutation_work.checked_add(field_work), mutation_bytes.checked_add(field_bytes)) {
                            (Some(work), Some(bytes)) => Ok((work, bytes)),
                            _ => Err(RunError::Contract("lint documentation quotation overflow")),
                        }
                    }
                    (Err(error), _) | (_, Err(error)) => Err(error),
                }
            },
            || diagnostic.set_documentation_url(Some(url)),
        ).await
    }

    async fn verbose(&self, metadata: &LintReportMetadata) -> RunResult<bool> {
        self.source.local_quoted_with_fixed_transfers(const { reporting_metadata_quote(ReportingMetadataOperation::Verbose) }, || metadata.verbose).await
    }

    async fn information(
        &self,
        diagnostic: &mut Diagnostic,
        metadata: &LintReportMetadata,
    ) -> RunResult<()> {
        let (name, suffix) = self.source.local_quoted_with_fixed_transfers(
            const { reporting_metadata_quote(ReportingMetadataOperation::NameAndSuffix) }, || (metadata.id.name().as_str(), verbose_lint_suffix(metadata.source)),
        ).await?;
        let text = self.source.lint_text(["rule `", name, suffix]).await?;
        let message = self.source.finish_lint_message(text).await?;
        self.source.local_quoted_with_fixed_transfers(
            const { diagnostic_info_quote() },
            || diagnostic.info(message),
        ).await
    }

    async fn publish(
        &self,
        context: &InferContext<'db, 'ast>,
        diagnostic: Diagnostic,
    ) -> RunResult<()> {
        let (len, capacity, _, _) = self.source.local_quoted_with_fixed_transfers(
            const { context_storage_quote() }, || context.retained_diagnostics().storage(),
        ).await?;
        let quote = self.source.local_quoted_with_fixed_transfers(
            const { context_reservation_preparation_quote() },
            || {
                let (reserve_work, reserve_bytes) = match vec_reserve_exact_quote::<Diagnostic>(len, capacity, 1) {
                    Ok(quote) => quote,
                    Err(error) => return Err(error),
                };
                let (borrow_work, borrow_bytes) = match const { context_mutation_quote() } {
                    Ok(quote) => quote,
                    Err(error) => return Err(error),
                };
                let Some(work) = reserve_work.checked_add(borrow_work) else {
                    return Err(RunError::Contract("lint reservation work overflow"));
                };
                let Some(bytes) = reserve_bytes.checked_add(borrow_bytes) else {
                    return Err(RunError::Contract("lint reservation bytes overflow"));
                };
                let Some(bytes) = bytes.checked_add(size_of::<[usize; 4]>()) else {
                    return Err(RunError::Contract("lint reservation bytes overflow"));
                };
                Ok((work, bytes))
            },
        ).await??;
        self.source.local_quoted_with_fixed_transfers(
            Ok(quote),
            || context.reserve_lint_diagnostics(1),
        ).await?;
        // The helper retains the incoming owner until admission and child drainage finish.
        self.source.local_quoted_with_fixed_transfers(
            const {
                match (prepared_vec_push_quote::<Diagnostic>(), context_mutation_quote()) {
                    (Ok((push_work, push_bytes)), Ok((borrow_work, borrow_bytes))) => {
                        match (push_work.checked_add(borrow_work), push_bytes.checked_add(borrow_bytes)) {
                            (Some(work), Some(bytes)) => {
                                match (work.checked_add(1), bytes.checked_add(size_of::<[Diagnostic; 4]>())) {
                                    (Some(work), Some(bytes)) => Ok((work, bytes)),
                                    _ => Err(RunError::Contract("lint insertion quotation overflow")),
                                }
                            }
                            _ => Err(RunError::Contract("lint insertion quotation overflow")),
                        }
                    }
                    (Err(error), _) | (_, Err(error)) => Err(error),
                }
            },
            || context.push_lint_diagnostic(diagnostic),
        ).await
    }
}
