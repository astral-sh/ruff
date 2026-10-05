//! Suppression validation with the temporary candidates retained by the file owner.

use ruff_db::PythonFile;
use ruff_db::source::SourceText;
use salsa::execution_probe::{RunError, RunResult};

use super::storage::{sequence_merge, table_merge};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::suppression::source::{
    self, SuppressionCheckState, SuppressionEffects, SuppressionFacts, SuppressionLint,
};
use crate::suppression::{FileSuppressionId, Suppression, Suppressions};
use crate::types::TypeCheckDiagnostics;

struct FileSuppressionEffects<'a, 'access, 'run, 'db: 'run, A> {
    source: &'a SourceEffects<'access, 'run, 'db, A>,
    file: PythonFile<'db>,
    suppressions: &'db Suppressions,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn check_file_suppressions(
        &self,
        file: PythonFile<'db>,
        suppressions: &'db Suppressions,
        diagnostics: &mut TypeCheckDiagnostics,
        state: &mut SuppressionCheckState<'db>,
    ) -> RunResult<()> {
        source::check_suppressions_with(
            diagnostics,
            state,
            SuppressionFacts,
            &FileSuppressionEffects {
                source: self,
                file,
                suppressions,
            },
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SuppressionEffects<'db>
    for FileSuppressionEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn is_lint_disabled(&self, lint: SuppressionLint) -> RunResult<bool> {
        let fields = self.source.access.endpoint().field_request_context();
        let file = self
            .source
            .field(self.file.read_fields(fields).file())
            .await?;
        let rules = self.source.access.rule_selection(file).await?;
        let len = self.source.local(1, 0, || rules.iter().len()).await?;
        let work = SourceEffects::<A>::checked(len.checked_mul(4).and_then(|n| n.checked_add(4)))?;
        self.source
            .local(work, 0, || source::is_lint_disabled(rules, lint))
            .await
    }

    async fn next_unknown(&self, cursor: &mut usize) -> RunResult<Option<usize>> {
        self.source
            .local(1, 0, || source::next_unknown(self.suppressions, cursor))
            .await
    }

    async fn report_unknown(
        &self,
        _diagnostics: &mut TypeCheckDiagnostics,
        _index: usize,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::FileSuppressionUnknownRule)
            .await
    }

    async fn next_invalid(&self, cursor: &mut usize) -> RunResult<Option<usize>> {
        self.source
            .local(1, 0, || source::next_invalid(self.suppressions, cursor))
            .await
    }

    async fn report_invalid(
        &self,
        _diagnostics: &mut TypeCheckDiagnostics,
        _index: usize,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::FileSuppressionInvalid)
            .await
    }

    async fn next_suppression(&self, cursor: &mut usize) -> RunResult<Option<&'db Suppression>> {
        self.source
            .local(2, 0, || source::next_suppression(self.suppressions, cursor))
            .await
    }

    async fn preferred_suppression(
        &self,
        _suppression: &'db Suppression,
        _unused: bool,
    ) -> RunResult<Option<FileSuppressionId>> {
        self.source
            .unavailable(SourceOperation::FileSuppressionSelection)
            .await
    }

    async fn mark_used(
        &self,
        diagnostics: &mut TypeCheckDiagnostics,
        id: FileSuppressionId,
    ) -> RunResult<()> {
        self.source.work(1).await?;
        let (_, _, length, capacity) = diagnostics.storage();
        let (quote, _) = table_merge::<FileSuppressionId>(length, capacity, 1, 0)
            .ok_or(RunError::Contract("suppression marking quotation overflow"))?;
        self.source
            .local(quote.work, quote.bytes, || diagnostics.mark_used(id))
            .await
    }

    async fn report_blanket(
        &self,
        _diagnostics: &mut TypeCheckDiagnostics,
        _suppression: &'db Suppression,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::FileSuppressionBlanket)
            .await
    }

    async fn prepare_unused(
        &self,
        diagnostics: &TypeCheckDiagnostics,
        state: &mut SuppressionCheckState<'db>,
    ) -> RunResult<()> {
        self.source.work(1).await?;
        let requested = source::unused_capacity(self.suppressions, diagnostics);
        let (length, capacity) = state.storage();
        let quote = sequence_merge::<&Suppression>(length, capacity, requested).ok_or(
            RunError::Contract("unused suppression allocation quotation overflow"),
        )?;
        self.source
            .local(quote.work, quote.bytes, || state.reserve(requested))
            .await
    }

    async fn is_used(
        &self,
        diagnostics: &TypeCheckDiagnostics,
        suppression: &'db Suppression,
    ) -> RunResult<bool> {
        self.source.work(1).await?;
        let capacity = diagnostics.storage().3;
        let work =
            SourceEffects::<A>::checked(capacity.checked_mul(4).and_then(|n| n.checked_add(4)))?;
        self.source
            .local(work, 0, || diagnostics.is_used(suppression.id()))
            .await
    }

    async fn push_unused(
        &self,
        state: &mut SuppressionCheckState<'db>,
        suppression: &'db Suppression,
    ) -> RunResult<()> {
        self.source.work(1).await?;
        let (length, capacity) = state.storage();
        let quote = sequence_merge::<&Suppression>(length, capacity, 1).ok_or(
            RunError::Contract("unused suppression growth quotation overflow"),
        )?;
        self.source
            .local(quote.work, quote.bytes, || state.push(suppression))
            .await
    }

    async fn source(&self) -> RunResult<SourceText> {
        let fields = self.source.access.endpoint().field_request_context();
        let file = self
            .source
            .field(self.file.read_fields(fields).file())
            .await?;
        self.source.access.source_text(file).await
    }

    async fn next_unused(
        &self,
        state: &mut SuppressionCheckState<'db>,
    ) -> RunResult<Option<&'db Suppression>> {
        self.source.local(1, 0, || state.next()).await
    }

    async fn report_unused(
        &self,
        _diagnostics: &mut TypeCheckDiagnostics,
        _state: &mut SuppressionCheckState<'db>,
        _suppression: &'db Suppression,
        _source: &SourceText,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::FileSuppressionUnused)
            .await
    }
}
