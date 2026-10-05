//! File checking shares its scope, alias, suppression, and diagnostic ordering.

use std::convert::Infallible;
use std::time::Duration;

use ruff_db::Instant;
use ruff_db::diagnostic::{Diagnostic, DiagnosticId, UnifiedFile};
use ruff_db::files::File;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_db::source::{SourceText, source_text};
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;
use ty_python_core::{ProgramFile, SemanticIndex, semantic_index};

use super::infer::{
    ImplicitAliasInference, ScopeInference, implicit_alias_parameters, infer_implicit_alias_type,
};
use super::{TypeCheckDiagnostics, TypeContext, infer_scope_types};
use crate::suppression::check_suppressions;
use crate::suppression::source::SuppressionCheckState;
use crate::{Db, IOErrorDiagnostic, add_inferred_python_version_hint_to_diagnostic};

pub(crate) type DiagnosticKey = (DiagnosticId, Option<TextRange>);

#[derive(Default)]
pub(crate) struct TypeCheckState<'db> {
    pub(crate) diagnostics: TypeCheckDiagnostics,
    pub(crate) implicit_aliases: Vec<Definition<'db>>,
    pub(crate) checked_aliases: FxHashSet<Definition<'db>>,
    pub(crate) reported: FxHashSet<DiagnosticKey>,
    pub(crate) suppressions: SuppressionCheckState<'db>,
}

#[derive(Default)]
pub(crate) struct FileCheckState<'db> {
    pub(crate) types: TypeCheckState<'db>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

pub(crate) fn diagnostic_key(file: File, diagnostic: &Diagnostic) -> Option<DiagnosticKey> {
    let span = diagnostic.primary_span()?;
    (span.file() == &UnifiedFile::Ty(file)).then_some((diagnostic.id(), span.range()))
}

pub(crate) struct FileCheckFacts;

shared_semantic_family! {
    #[synchronous(SynchronousFileCheckEffects)]
    pub(super) trait FileCheckEffects<'db> {
        type Error;
        #[operation(source)]
        async fn source(&self, file: ProgramFile<'db>) -> Result<SourceText, Self::Error>;
        #[operation(source)]
        async fn module(&self, file: ProgramFile<'db>) -> Result<ParsedModuleRef, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_scope(&self, index: &SemanticIndex<'db>, cursor: &mut usize) -> Result<Option<ScopeId<'db>>, Self::Error>;
        #[operation(local)]
        async fn accepts_type_context(&self, scope: ScopeId<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn scope(&self, scope: ScopeId<'db>) -> Result<&'db ScopeInference<'db>, Self::Error>;
        #[operation(local)]
        async fn merge_scope(&self, state: &mut TypeCheckState<'db>, inference: &ScopeInference<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn collect_reported(&self, file: File, state: &mut TypeCheckState<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_alias(&self, state: &mut TypeCheckState<'db>) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(local)]
        async fn first_alias_visit(&self, state: &mut TypeCheckState<'db>, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn alias(&self, definition: Definition<'db>) -> Result<&'db ImplicitAliasInference<'db>, Self::Error>;
        #[operation(local)]
        async fn merge_alias(&self, file: File, state: &mut TypeCheckState<'db>, inference: &ImplicitAliasInference<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_semantic_error(&self, index: &SemanticIndex<'db>, cursor: &mut usize) -> Result<Option<usize>, Self::Error>;
        #[operation(source)]
        async fn semantic_error(&self, file: File, index: &SemanticIndex<'db>, position: usize, state: &mut TypeCheckState<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn suppressions(&self, file: ProgramFile<'db>, state: &mut TypeCheckState<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn has_read_error(&self, source: &SourceText) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn read_error(&self, file: File, source: &SourceText) -> Result<Option<Diagnostic>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_parse_error(&self, module: &ParsedModuleRef, cursor: &mut usize) -> Result<Option<usize>, Self::Error>;
        #[operation(source)]
        async fn parse_error(&self, file: File, module: &ParsedModuleRef, position: usize, state: &mut FileCheckState<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_unsupported_error(&self, module: &ParsedModuleRef, cursor: &mut usize) -> Result<Option<usize>, Self::Error>;
        #[operation(source)]
        async fn unsupported_error(&self, file: File, module: &ParsedModuleRef, position: usize, state: &mut FileCheckState<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn check_types(&self, file: ProgramFile<'db>, state: &mut FileCheckState<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn append_type_diagnostics(&self, state: &mut FileCheckState<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn sort(&self, file: File, state: &mut FileCheckState<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl FileCheckFacts {
        fn has_aliases(&self, state: &TypeCheckState<'_>) -> bool { !state.implicit_aliases.is_empty() }
    }

    #[synchronous(check_types_sync)]
    #[capabilities(effects = FileCheckEffects, facts = FileCheckFacts)]
    #[passive_values()]
    pub(super) async fn check_types_with<'db, E: FileCheckEffects<'db>>(
        file: ProgramFile<'db>, source_file: File, index: &SemanticIndex<'db>, state: &mut TypeCheckState<'db>, facts: FileCheckFacts, effects: &E,
    ) -> Result<(), E::Error> {
        #[passive_state]
        let mut scopes = 0;
        #[cursor_loop]
        while let Some(scope) = effects.next_scope(index, &mut scopes).await? {
            // Contextual scopes are inferred while checking their enclosing scope.
            if effects.accepts_type_context(scope).await? {
                continue;
            }
            let inference = effects.scope(scope).await?;
            effects.merge_scope(state, inference).await?;
        }

        // Aliases can be referenced across scopes or through mutually recursive aliases.
        // Keep their diagnostics outside the recursive inference queries.
        if facts.has_aliases(state) {
            effects.collect_reported(source_file, state).await?;
        }
        #[cursor_loop]
        while let Some(definition) = effects.next_alias(state).await? {
            if !effects.first_alias_visit(state, definition).await? {
                continue;
            }
            let inference = effects.alias(definition).await?;
            effects.merge_alias(source_file, state, inference).await?;
        }

        #[passive_state]
        let mut syntax = 0;
        #[cursor_loop]
        while let Some(position) = effects.next_semantic_error(index, &mut syntax).await? {
            effects.semantic_error(source_file, index, position, state).await?;
        }
        effects.suppressions(file, state).await?;
        Ok(())
    }

    #[synchronous(check_file_sync)]
    #[capabilities(effects = FileCheckEffects)]
    #[passive_values(Some, None)]
    pub(super) async fn check_file_with<'db, E: FileCheckEffects<'db>>(
        file: ProgramFile<'db>, source_file: File, state: &mut FileCheckState<'db>, effects: &E,
    ) -> Result<Option<Diagnostic>, E::Error> {
        let source = effects.source(file).await?;
        if effects.has_read_error(&source).await? {
            return effects.read_error(source_file, &source).await;
        }
        let module = effects.module(file).await?;
        #[passive_state]
        let mut parse = 0;
        #[cursor_loop]
        while let Some(position) = effects.next_parse_error(&module, &mut parse).await? {
            effects.parse_error(source_file, &module, position, state).await?;
        }
        #[passive_state]
        let mut unsupported = 0;
        #[cursor_loop]
        while let Some(position) = effects.next_unsupported_error(&module, &mut unsupported).await? {
            effects.unsupported_error(source_file, &module, position, state).await?;
        }
        effects.check_types(file, state).await?;
        effects.append_type_diagnostics(state).await?;
        effects.sort(source_file, state).await?;
        Ok(None)
    }
}

pub(crate) fn next_position(length: usize, cursor: &mut usize) -> Option<usize> {
    if *cursor < length {
        let position = *cursor;
        *cursor += 1;
        Some(position)
    } else {
        None
    }
}

pub(crate) fn merge_scope<'db>(state: &mut TypeCheckState<'db>, inference: &ScopeInference<'db>) {
    if let Some(diagnostics) = inference.diagnostics() {
        state.diagnostics.extend(diagnostics);
    }
    state
        .implicit_aliases
        .extend_from_slice(inference.implicit_aliases());
}

pub(crate) fn collect_reported(file: File, state: &mut TypeCheckState<'_>) {
    state.reported.extend(
        (&state.diagnostics)
            .into_iter()
            .filter_map(|diagnostic| diagnostic_key(file, diagnostic)),
    );
}

pub(super) fn merge_alias<'db>(
    file: File,
    state: &mut TypeCheckState<'db>,
    inference: &ImplicitAliasInference<'db>,
) {
    // Runtime-value inference can report the same error with different wording. Compare with
    // earlier results only: one alias can produce distinct errors at the same rule and location.
    state
        .diagnostics
        .extend_filtered(&inference.diagnostics, |diagnostic| {
            diagnostic_key(file, diagnostic).is_none_or(|key| !state.reported.contains(&key))
        });
    state.reported.extend(
        (&inference.diagnostics)
            .into_iter()
            .filter_map(|diagnostic| diagnostic_key(file, diagnostic)),
    );
    state
        .implicit_aliases
        .extend_from_slice(&inference.implicit_aliases);
}

pub(crate) fn append_type_diagnostics(state: &mut FileCheckState<'_>) {
    state
        .diagnostics
        .extend(std::mem::take(&mut state.types.diagnostics));
}

pub(crate) fn sort(db: &dyn Db, diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_unstable_by(|a, b| a.rendering_sort_key(db).cmp(&b.rendering_sort_key(db)));
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

struct InlineFileCheckEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> SynchronousFileCheckEffects<'db> for InlineFileCheckEffects<'db> {
    type Error = Infallible;
    fn source(&self, file: ProgramFile<'db>) -> Result<SourceText, Infallible> {
        Ok(source_text(self.db, file.file(self.db)))
    }
    fn module(&self, file: ProgramFile<'db>) -> Result<ParsedModuleRef, Infallible> {
        Ok(parsed_module(self.db, file.python_file(self.db)).load(self.db))
    }
    fn next_scope(
        &self,
        index: &SemanticIndex<'db>,
        cursor: &mut usize,
    ) -> Result<Option<ScopeId<'db>>, Infallible> {
        let next = index.scope_ids().nth(*cursor);
        *cursor += usize::from(next.is_some());
        Ok(next)
    }
    fn accepts_type_context(&self, scope: ScopeId<'db>) -> Result<bool, Infallible> {
        Ok(scope.accepts_type_context(self.db))
    }
    fn scope(&self, scope: ScopeId<'db>) -> Result<&'db ScopeInference<'db>, Infallible> {
        Ok(infer_scope_types(self.db, scope, TypeContext::default()))
    }
    fn merge_scope(
        &self,
        state: &mut TypeCheckState<'db>,
        inference: &ScopeInference<'db>,
    ) -> Result<(), Infallible> {
        merge_scope(state, inference);
        Ok(())
    }
    fn collect_reported(
        &self,
        file: File,
        state: &mut TypeCheckState<'db>,
    ) -> Result<(), Infallible> {
        collect_reported(file, state);
        Ok(())
    }
    fn next_alias(
        &self,
        state: &mut TypeCheckState<'db>,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        Ok(state.implicit_aliases.pop())
    }
    fn first_alias_visit(
        &self,
        state: &mut TypeCheckState<'db>,
        definition: Definition<'db>,
    ) -> Result<bool, Infallible> {
        Ok(state.checked_aliases.insert(definition))
    }
    fn alias(
        &self,
        definition: Definition<'db>,
    ) -> Result<&'db ImplicitAliasInference<'db>, Infallible> {
        Ok(infer_implicit_alias_type(
            self.db,
            definition,
            implicit_alias_parameters(self.db, definition),
        ))
    }
    fn merge_alias(
        &self,
        file: File,
        state: &mut TypeCheckState<'db>,
        inference: &ImplicitAliasInference<'db>,
    ) -> Result<(), Infallible> {
        merge_alias(file, state, inference);
        Ok(())
    }
    fn next_semantic_error(
        &self,
        index: &SemanticIndex<'db>,
        cursor: &mut usize,
    ) -> Result<Option<usize>, Infallible> {
        Ok(next_position(index.semantic_syntax_errors().len(), cursor))
    }
    fn semantic_error(
        &self,
        file: File,
        index: &SemanticIndex<'db>,
        position: usize,
        state: &mut TypeCheckState<'db>,
    ) -> Result<(), Infallible> {
        let error = &index.semantic_syntax_errors()[position];
        state
            .diagnostics
            .push(Diagnostic::invalid_syntax(file, error, error));
        Ok(())
    }
    fn suppressions(
        &self,
        file: ProgramFile<'db>,
        state: &mut TypeCheckState<'db>,
    ) -> Result<(), Infallible> {
        let diagnostics = check_suppressions(
            self.db,
            file.python_file(self.db),
            std::mem::take(&mut state.diagnostics),
        );
        state.diagnostics.extend_diagnostics(diagnostics);
        Ok(())
    }
    fn has_read_error(&self, source: &SourceText) -> Result<bool, Infallible> {
        Ok(source.read_error().is_some())
    }
    fn read_error(
        &self,
        file: File,
        source: &SourceText,
    ) -> Result<Option<Diagnostic>, Infallible> {
        Ok(source.read_error().map(|error| {
            IOErrorDiagnostic {
                file,
                error: error.clone(),
            }
            .to_diagnostic()
        }))
    }
    fn next_parse_error(
        &self,
        module: &ParsedModuleRef,
        cursor: &mut usize,
    ) -> Result<Option<usize>, Infallible> {
        Ok(next_position(module.errors().len(), cursor))
    }
    fn parse_error(
        &self,
        file: File,
        module: &ParsedModuleRef,
        position: usize,
        state: &mut FileCheckState<'db>,
    ) -> Result<(), Infallible> {
        let error = &module.errors()[position];
        state
            .diagnostics
            .push(Diagnostic::invalid_syntax(file, &error.error, error));
        Ok(())
    }
    fn next_unsupported_error(
        &self,
        module: &ParsedModuleRef,
        cursor: &mut usize,
    ) -> Result<Option<usize>, Infallible> {
        Ok(next_position(
            module.unsupported_syntax_errors().len(),
            cursor,
        ))
    }
    fn unsupported_error(
        &self,
        file: File,
        module: &ParsedModuleRef,
        position: usize,
        state: &mut FileCheckState<'db>,
    ) -> Result<(), Infallible> {
        let error = &module.unsupported_syntax_errors()[position];
        let mut diagnostic = Diagnostic::invalid_syntax(file, error, error);
        add_inferred_python_version_hint_to_diagnostic(
            self.db,
            file,
            &mut diagnostic,
            "parsing syntax",
        );
        state.diagnostics.push(diagnostic);
        Ok(())
    }
    fn check_types(
        &self,
        file: ProgramFile<'db>,
        state: &mut FileCheckState<'db>,
    ) -> Result<(), Infallible> {
        check_types_into(self.db, file, &mut state.types);
        Ok(())
    }
    fn append_type_diagnostics(&self, state: &mut FileCheckState<'db>) -> Result<(), Infallible> {
        append_type_diagnostics(state);
        Ok(())
    }
    fn sort(&self, _file: File, state: &mut FileCheckState<'db>) -> Result<(), Infallible> {
        sort(self.db, &mut state.diagnostics);
        Ok(())
    }
}

pub(crate) fn check_types(db: &dyn Db, file: ProgramFile<'_>) -> Vec<Diagnostic> {
    let mut state = TypeCheckState::default();
    check_types_into(db, file, &mut state);
    state.diagnostics.into_diagnostics()
}

fn check_types_into<'db>(db: &'db dyn Db, file: ProgramFile<'db>, state: &mut TypeCheckState<'db>) {
    let source_file = file.file(db);
    let _span = tracing::trace_span!("check_types", ?source_file).entered();
    tracing::debug!("Checking file '{path}'", path = source_file.path(db));
    let start = Instant::now();
    infallible(check_types_sync(
        file,
        source_file,
        semantic_index(db, file),
        state,
        FileCheckFacts,
        &InlineFileCheckEffects { db },
    ));
    let elapsed = start.elapsed();
    if elapsed >= Duration::from_millis(100) {
        tracing::info!(
            "Checking file `{path}` took more than 100ms ({elapsed:?})",
            path = source_file.path(db)
        );
    }
}

pub(crate) fn check_file(
    db: &dyn Db,
    file: ProgramFile<'_>,
) -> Result<Box<[Diagnostic]>, Diagnostic> {
    let source_file = file.file(db);
    let mut state = FileCheckState::default();
    if let Some(error) = infallible(check_file_sync(
        file,
        source_file,
        &mut state,
        &InlineFileCheckEffects { db },
    )) {
        Err(error)
    } else {
        Ok(state.diagnostics.into_boxed_slice())
    }
}
