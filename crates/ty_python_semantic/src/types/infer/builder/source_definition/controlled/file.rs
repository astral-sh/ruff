//! Controlled file checking retains every collection until admitted finalization.

use ruff_db::diagnostic::{ConciseMessage, Diagnostic, UnifiedFile};
use ruff_db::files::File;
use ruff_db::parsed::ParsedModuleRef;
use ruff_db::source::SourceText;
use salsa::execution_probe::{ExecutionWork, RunError, RunResult};
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;
use ty_python_core::{ProgramFile, SemanticIndex};

use super::storage::{StorageQuote, dense_finish, sequence_merge, slots, table_merge};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::suppression::FileSuppressionId;
use crate::types::TypeContext;
use crate::types::check::{
    self, DiagnosticKey, FileCheckEffects, FileCheckFacts, FileCheckState, TypeCheckState,
};
use crate::types::infer::{ImplicitAliasInference, ScopeInference};

fn checked(quote: Option<StorageQuote>) -> RunResult<StorageQuote> {
    quote.ok_or(RunError::Contract("file storage quotation overflow"))
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn file_is_stub(
        &self,
        file: File,
    ) -> RunResult<bool> {
        let path = self.field(file.read_fields(self.db()).path()).await?;
        let work = Self::checked(path.as_str().len().checked_add(4))?;
        self.local(work, 0, || path.source_type().is_stub()).await
    }

    pub(in crate::types::infer) async fn check_file(
        &self,
        file: ProgramFile<'db>,
    ) -> RunResult<Result<Box<[Diagnostic]>, Diagnostic>> {
        self.check_file_program(file).await?;
        let mut owner = self.local(1, 0, FileCheckState::default).await?;
        let physical_file = self.physical_file(file).await?;
        let error = check::check_file_with(file, physical_file, &mut owner, self).await?;
        let mut quote = checked(dense_finish::<Diagnostic>(
            owner.diagnostics.len(),
            owner.diagnostics.capacity(),
        ))?;
        for backing in [
            owner.types.implicit_aliases.capacity(),
            Self::checked(slots(owner.types.checked_aliases.capacity()))?,
            Self::checked(slots(owner.types.reported.capacity()))?,
        ] {
            quote.work = Self::checked(quote.work.checked_add(backing))?;
        }
        quote.work = Self::checked(
            quote
                .work
                .checked_add(owner.types.suppressions.storage().1)
                .and_then(|work| work.checked_add(1)),
        )?;
        let mut owner = Some(owner);
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(quote.work)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: quote.bytes,
                })?;
                endpoint.check_completion()?;
                let owner = owner
                    .take()
                    .ok_or(RunError::Contract("file owner already consumed"))?;
                Ok(match error {
                    Some(error) => Err(error),
                    None => Ok(owner.diagnostics.into_boxed_slice()),
                })
            })
            .await)
    }

    async fn file_diagnostic_keys_work(
        &self,
        diagnostics: &crate::types::TypeCheckDiagnostics,
    ) -> RunResult<usize> {
        self.work(Self::checked(diagnostics.storage().0.checked_add(1))?)
            .await?;
        let mut work = 1usize;
        for diagnostic in diagnostics {
            work = Self::checked(
                work.checked_add(diagnostic.annotations().len())
                    .and_then(|work| work.checked_add(diagnostic.id().as_str().len()))
                    .and_then(|work| work.checked_add(4)),
            )?;
        }
        Ok(work)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FileCheckEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn source(&self, file: ProgramFile<'db>) -> RunResult<SourceText> {
        self.check_file_program(file).await?;
        let file = self.physical_file(file).await?;
        self.access.source_text(file).await
    }

    async fn module(&self, file: ProgramFile<'db>) -> RunResult<ParsedModuleRef> {
        self.access.parsed_module(file).await
    }

    async fn next_scope(
        &self,
        index: &SemanticIndex<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<ScopeId<'db>>> {
        // The retained iterator is a copied slice iterator, whose nth operation is constant-time.
        self.local(2, 0, || {
            let next = index.scope_ids().nth(*cursor);
            *cursor += usize::from(next.is_some());
            next
        })
        .await
    }

    async fn accepts_type_context(&self, scope: ScopeId<'db>) -> RunResult<bool> {
        let scope = self.scope_metadata(scope).await?;
        self.local(1, 0, || scope.accepts_type_context()).await
    }

    async fn scope(&self, scope: ScopeId<'db>) -> RunResult<&'db ScopeInference<'db>> {
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        self.access.scope(scope, TypeContext::default()).await
    }

    async fn merge_scope(
        &self,
        state: &mut TypeCheckState<'db>,
        inference: &ScopeInference<'db>,
    ) -> RunResult<()> {
        self.work(1).await?;
        let mut quote = checked(sequence_merge::<Definition<'db>>(
            state.implicit_aliases.len(),
            state.implicit_aliases.capacity(),
            inference.implicit_aliases().len(),
        ))?;
        if let Some(diagnostics) = inference.diagnostics() {
            let (length, capacity, used, used_capacity) = state.diagnostics.storage();
            let (incoming, _, incoming_used, incoming_capacity) = diagnostics.storage();
            quote = checked(quote.checked_add(checked(sequence_merge::<Diagnostic>(
                length, capacity, incoming,
            ))?))?;
            quote = checked(
                quote.checked_add(checked(
                    table_merge::<FileSuppressionId>(used, used_capacity, incoming_used, 0)
                        .map(|(quote, _)| quote),
                )?),
            )?;
            quote.work = Self::checked(
                quote
                    .work
                    .checked_add(Self::checked(slots(incoming_capacity))?)
                    .and_then(|work| work.checked_add(incoming)),
            )?;
            quote.work = Self::checked(
                quote
                    .work
                    .checked_add(self.diagnostic_retirement(diagnostics).await?),
            )?;
        }
        self.local(quote.work, quote.bytes, || {
            check::merge_scope(state, inference);
            #[cfg(test)]
            super::observations::observe(self.db(), super::observations::Event::FileScopeMerged);
            #[cfg(feature = "testing")]
            crate::analysis::testing::scope_merged(self.db());
        })
        .await
    }

    async fn collect_reported(&self, file: File, state: &mut TypeCheckState<'db>) -> RunResult<()> {
        let work = self.file_diagnostic_keys_work(&state.diagnostics).await?;
        let mut quote = checked(
            table_merge::<DiagnosticKey>(
                state.reported.len(),
                state.reported.capacity(),
                state.diagnostics.storage().0,
                0,
            )
            .map(|(quote, _)| quote),
        )?;
        quote.work = Self::checked(quote.work.checked_add(work))?;
        self.local(quote.work, quote.bytes, || {
            check::collect_reported(file, state)
        })
        .await
    }

    async fn next_alias(
        &self,
        state: &mut TypeCheckState<'db>,
    ) -> RunResult<Option<Definition<'db>>> {
        self.local(1, 0, || state.implicit_aliases.pop()).await
    }

    async fn first_alias_visit(
        &self,
        state: &mut TypeCheckState<'db>,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        let quote = checked(
            table_merge::<Definition<'db>>(
                state.checked_aliases.len(),
                state.checked_aliases.capacity(),
                1,
                0,
            )
            .map(|(quote, _)| quote),
        )?;
        self.local(quote.work, quote.bytes, || {
            state.checked_aliases.insert(definition)
        })
        .await
    }

    async fn alias(
        &self,
        _definition: Definition<'db>,
    ) -> RunResult<&'db ImplicitAliasInference<'db>> {
        self.unavailable(SourceOperation::FileImplicitAlias).await
    }

    async fn merge_alias(
        &self,
        _file: File,
        _state: &mut TypeCheckState<'db>,
        _inference: &ImplicitAliasInference<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::FileImplicitAlias).await
    }

    async fn next_semantic_error(
        &self,
        index: &SemanticIndex<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<usize>> {
        self.local(1, 0, || {
            check::next_position(index.semantic_syntax_errors().len(), cursor)
        })
        .await
    }

    async fn semantic_error(
        &self,
        _file: File,
        _index: &SemanticIndex<'db>,
        _position: usize,
        _state: &mut TypeCheckState<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::FileSemanticSyntaxDiagnostic)
            .await
    }

    async fn suppressions(
        &self,
        file: ProgramFile<'db>,
        state: &mut TypeCheckState<'db>,
    ) -> RunResult<()> {
        self.check_file_program(file).await?;
        let fields = self.access.endpoint().field_request_context();
        let python_file = self.field(file.read_fields(fields).python_file()).await?;
        let file = self.field(python_file.read_fields(fields).file()).await?;
        let suppressions = self.access.suppressions(file).await?;
        self.check_file_suppressions(
            python_file,
            suppressions,
            &mut state.diagnostics,
            &mut state.suppressions,
        )
        .await
    }

    async fn has_read_error(&self, source: &SourceText) -> RunResult<bool> {
        self.local(1, 0, || source.read_error().is_some()).await
    }

    async fn read_error(&self, _file: File, _source: &SourceText) -> RunResult<Option<Diagnostic>> {
        self.unavailable(SourceOperation::FileReadError).await
    }

    async fn next_parse_error(
        &self,
        module: &ParsedModuleRef,
        cursor: &mut usize,
    ) -> RunResult<Option<usize>> {
        self.local(1, 0, || check::next_position(module.errors().len(), cursor))
            .await
    }

    async fn parse_error(
        &self,
        _file: File,
        _module: &ParsedModuleRef,
        _position: usize,
        _state: &mut FileCheckState<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::FileParseDiagnostic).await
    }

    async fn next_unsupported_error(
        &self,
        module: &ParsedModuleRef,
        cursor: &mut usize,
    ) -> RunResult<Option<usize>> {
        self.local(1, 0, || {
            check::next_position(module.unsupported_syntax_errors().len(), cursor)
        })
        .await
    }

    async fn unsupported_error(
        &self,
        _file: File,
        _module: &ParsedModuleRef,
        _position: usize,
        _state: &mut FileCheckState<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::FileUnsupportedSyntaxDiagnostic)
            .await
    }

    async fn check_types(
        &self,
        file: ProgramFile<'db>,
        state: &mut FileCheckState<'db>,
    ) -> RunResult<()> {
        let index = self.access.semantic_index(file).await?;
        let physical_file = self.physical_file(file).await?;
        check::check_types_with(
            file,
            physical_file,
            index,
            &mut state.types,
            FileCheckFacts,
            self,
        )
        .await
    }

    async fn append_type_diagnostics(&self, state: &mut FileCheckState<'db>) -> RunResult<()> {
        let (length, _, _, used_capacity) = state.types.diagnostics.storage();
        let mut quote = checked(sequence_merge::<Diagnostic>(
            state.diagnostics.len(),
            state.diagnostics.capacity(),
            length,
        ))?;
        quote.work = Self::checked(quote.work.checked_add(Self::checked(slots(used_capacity))?))?;
        self.local(quote.work, quote.bytes, || {
            check::append_type_diagnostics(state)
        })
        .await
    }

    async fn sort(&self, file: File, state: &mut FileCheckState<'db>) -> RunResult<()> {
        self.work(Self::checked(state.diagnostics.len().checked_add(1))?)
            .await?;
        let mut key_work = 1usize;
        let mut message_bytes = 0usize;
        for diagnostic in &state.diagnostics {
            self.work(Self::checked(
                diagnostic.annotations().len().checked_add(2),
            )?)
            .await?;
            if diagnostic
                .primary_span()
                .is_some_and(|span| span.file() != &UnifiedFile::Ty(file))
            {
                return self.unavailable(SourceOperation::FileDiagnosticSort).await;
            }
            let message =
                match diagnostic.concise_message() {
                    ConciseMessage::MainDiagnostic(message) | ConciseMessage::Custom(message) => {
                        message.len()
                    }
                    ConciseMessage::Both { main, annotation } => {
                        let bytes = Self::checked(
                            main.len()
                                .checked_add(annotation.len())
                                .and_then(|n| n.checked_add(2)),
                        )?;
                        message_bytes =
                            Self::checked(message_bytes.checked_add(bytes.checked_mul(2).ok_or(
                                RunError::Contract("diagnostic sort quotation overflow"),
                            )?))?;
                        bytes
                    }
                };
            key_work = Self::checked(
                key_work
                    .checked_add(diagnostic.annotations().len())
                    .and_then(|n| n.checked_add(diagnostic.id().as_str().len()))
                    .and_then(|n| n.checked_add(message))
                    .and_then(|n| n.checked_add(8)),
            )?;
        }
        let comparisons = Self::checked(
            state
                .diagnostics
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_mul(n))
                .and_then(|n| n.checked_mul(24)),
        )?;
        let work = Self::checked(comparisons.checked_mul(key_work))?;
        let bytes = Self::checked(comparisons.checked_mul(message_bytes))?;
        self.local(work, bytes, || {
            check::sort(self.db(), &mut state.diagnostics)
        })
        .await
    }
}
