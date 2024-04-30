use ruff_formatter::PrintedRange;
use ruff_text_size::TextRange;
use std::path::Path;
use std::sync::Arc;

use crate::db::{
    Database, Db, DbRuntime, HasJar, HasJars, JarsStorage, LintDb, LintJar, ParallelDatabase,
    QueryResult, SemanticDb, SemanticJar, Snapshot, SourceDb, SourceJar,
};
use crate::files::{FileId, Files};
use crate::format::{
    check_formatted, format_file, format_file_range, FormatDb, FormatError, FormatJar,
    FormattedFile,
};
use crate::lint::{lint_semantic, lint_syntax, Diagnostics};
use crate::module::{
    add_module, file_to_module, path_to_module, resolve_module, set_module_search_paths, Module,
    ModuleData, ModuleName, ModuleSearchPath,
};
use crate::parse::{parse, Parsed};
use crate::source::{source_text, Source};
use crate::symbols::{symbol_table, SymbolId, SymbolTable};
use crate::types::{infer_symbol_type, Type};
use crate::Workspace;

pub mod check;
mod format;

#[derive(Debug)]
pub struct Program {
    jars: JarsStorage<Program>,
    files: Files,
    workspace: Workspace,
}

impl Program {
    pub fn new(workspace: Workspace) -> Self {
        Self {
            jars: JarsStorage::default(),
            files: Files::default(),
            workspace,
        }
    }

    pub fn apply_changes<I>(&mut self, changes: I)
    where
        I: IntoIterator<Item = FileChange>,
    {
        let (source, semantic, lint, format) = self.jars_mut();
        for change in changes {
            semantic.module_resolver.remove_module(change.id);
            semantic.symbol_tables.remove(&change.id);
            source.sources.remove(&change.id);
            source.parsed.remove(&change.id);
            // TODO: remove all dependent modules as well
            semantic.type_store.remove_module(change.id);
            lint.lint_syntax.remove(&change.id);
            lint.lint_semantic.remove(&change.id);
            format.formatted.remove(&change.id);
        }
    }

    pub fn files(&self) -> &Files {
        &self.files
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }
}

impl SourceDb for Program {
    fn file_id(&self, path: &Path) -> FileId {
        self.files.intern(path)
    }

    fn file_path(&self, file_id: FileId) -> Arc<Path> {
        self.files.path(file_id)
    }

    fn source(&self, file_id: FileId) -> QueryResult<Source> {
        source_text(self, file_id)
    }

    fn parse(&self, file_id: FileId) -> QueryResult<Parsed> {
        parse(self, file_id)
    }
}

impl SemanticDb for Program {
    fn resolve_module(&self, name: ModuleName) -> QueryResult<Option<Module>> {
        resolve_module(self, name)
    }

    fn file_to_module(&self, file_id: FileId) -> QueryResult<Option<Module>> {
        file_to_module(self, file_id)
    }

    fn path_to_module(&self, path: &Path) -> QueryResult<Option<Module>> {
        path_to_module(self, path)
    }

    fn symbol_table(&self, file_id: FileId) -> QueryResult<Arc<SymbolTable>> {
        symbol_table(self, file_id)
    }

    fn infer_symbol_type(&self, file_id: FileId, symbol_id: SymbolId) -> QueryResult<Type> {
        infer_symbol_type(self, file_id, symbol_id)
    }

    // Mutations
    fn add_module(&mut self, path: &Path) -> Option<(Module, Vec<Arc<ModuleData>>)> {
        add_module(self, path)
    }

    fn set_module_search_paths(&mut self, paths: Vec<ModuleSearchPath>) {
        set_module_search_paths(self, paths);
    }
}

impl LintDb for Program {
    fn lint_syntax(&self, file_id: FileId) -> QueryResult<Diagnostics> {
        lint_syntax(self, file_id)
    }

    fn lint_semantic(&self, file_id: FileId) -> QueryResult<Diagnostics> {
        lint_semantic(self, file_id)
    }
}

impl FormatDb for Program {
    fn format_file(&self, file_id: FileId) -> Result<FormattedFile, FormatError> {
        format_file(self, file_id)
    }

    fn format_file_range(
        &self,
        file_id: FileId,
        range: TextRange,
    ) -> Result<PrintedRange, FormatError> {
        format_file_range(self, file_id, range)
    }

    fn check_file_formatted(&self, file_id: FileId) -> Result<Diagnostics, FormatError> {
        check_formatted(self, file_id)
    }
}

impl Db for Program {}

impl Database for Program {
    fn runtime(&self) -> &DbRuntime {
        self.jars.runtime()
    }

    fn runtime_mut(&mut self) -> &mut DbRuntime {
        self.jars.runtime_mut()
    }
}

impl ParallelDatabase for Program {
    fn snapshot(&self) -> Snapshot<Self> {
        Snapshot::new(Self {
            jars: self.jars.snapshot(),
            files: self.files.clone(),
            workspace: self.workspace.clone(),
        })
    }
}

impl HasJars for Program {
    type Jars = (SourceJar, SemanticJar, LintJar, FormatJar);

    fn jars(&self) -> QueryResult<&Self::Jars> {
        self.jars.jars()
    }

    fn jars_mut(&mut self) -> &mut Self::Jars {
        self.jars.jars_mut()
    }
}

impl HasJar<SourceJar> for Program {
    fn jar(&self) -> QueryResult<&SourceJar> {
        Ok(&self.jars()?.0)
    }

    fn jar_mut(&mut self) -> &mut SourceJar {
        &mut self.jars_mut().0
    }
}

impl HasJar<SemanticJar> for Program {
    fn jar(&self) -> QueryResult<&SemanticJar> {
        Ok(&self.jars()?.1)
    }

    fn jar_mut(&mut self) -> &mut SemanticJar {
        &mut self.jars_mut().1
    }
}

impl HasJar<LintJar> for Program {
    fn jar(&self) -> QueryResult<&LintJar> {
        Ok(&self.jars()?.2)
    }

    fn jar_mut(&mut self) -> &mut LintJar {
        &mut self.jars_mut().2
    }
}

impl HasJar<FormatJar> for Program {
    fn jar(&self) -> QueryResult<&FormatJar> {
        Ok(&self.jars()?.3)
    }

    fn jar_mut(&mut self) -> &mut FormatJar {
        &mut self.jars_mut().3
    }
}

#[derive(Copy, Clone, Debug)]
pub struct FileChange {
    id: FileId,
    kind: FileChangeKind,
}

impl FileChange {
    pub fn new(file_id: FileId, kind: FileChangeKind) -> Self {
        Self { id: file_id, kind }
    }

    pub fn file_id(&self) -> FileId {
        self.id
    }

    pub fn kind(&self) -> FileChangeKind {
        self.kind
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
}
