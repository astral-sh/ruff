use std::path::Path;
use std::sync::Arc;

use crate::db::{Db, HasJar, SemanticDb, SemanticJar, SourceDb, SourceJar};
use crate::files::{FileId, Files};
use crate::lint::{lint_syntax, Diagnostics, LintSyntaxStorage};
use crate::module::{
    add_module, path_to_module, resolve_module, set_module_search_paths, Module, ModuleData,
    ModuleName, ModuleResolver, ModuleSearchPath,
};
use crate::parse::{parse, Parsed, ParsedStorage};
use crate::source::{source_text, Source, SourceStorage};
use crate::symbols::{symbol_table, SymbolTable, SymbolTablesStorage};

#[derive(Debug)]
pub struct Program {
    files: Files,
    source: SourceJar,
    semantic: SemanticJar,
}

impl Program {
    pub fn new(module_search_paths: Vec<ModuleSearchPath>, files: Files) -> Self {
        Self {
            source: SourceJar {
                sources: SourceStorage::default(),
                parsed: ParsedStorage::default(),
                lint_syntax: LintSyntaxStorage::default(),
            },
            semantic: SemanticJar {
                module_resolver: ModuleResolver::new(module_search_paths),
                symbol_tables: SymbolTablesStorage::default(),
            },
            files,
        }
    }

    pub fn apply_changes<I>(&mut self, changes: I)
    where
        I: IntoIterator<Item = FileChange>,
    {
        for change in changes {
            self.semantic
                .module_resolver
                .remove_module(&self.file_path(change.id));
            self.semantic.symbol_tables.remove(&change.id);
            self.source.sources.remove(&change.id);
            self.source.parsed.remove(&change.id);
            self.source.lint_syntax.remove(&change.id);
        }
    }
}

impl SourceDb for Program {
    fn file_id(&self, path: &Path) -> FileId {
        self.files.intern(path)
    }

    fn file_path(&self, file_id: FileId) -> Arc<Path> {
        self.files.path(file_id)
    }

    fn source(&self, file_id: FileId) -> Source {
        source_text(self, file_id)
    }

    fn parse(&self, file_id: FileId) -> Parsed {
        parse(self, file_id)
    }

    fn lint_syntax(&self, file_id: FileId) -> Diagnostics {
        lint_syntax(self, file_id)
    }
}

impl SemanticDb for Program {
    fn resolve_module(&self, name: ModuleName) -> Option<Module> {
        resolve_module(self, name)
    }

    fn symbol_table(&self, file_id: FileId) -> Arc<SymbolTable> {
        symbol_table(self, file_id)
    }

    // Mutations
    fn path_to_module(&mut self, path: &Path) -> Option<Module> {
        path_to_module(self, path)
    }

    fn add_module(&mut self, path: &Path) -> Option<(Module, Vec<Arc<ModuleData>>)> {
        add_module(self, path)
    }

    fn set_module_search_paths(&mut self, paths: Vec<ModuleSearchPath>) {
        set_module_search_paths(self, paths);
    }
}

impl Db for Program {}

impl HasJar<SourceJar> for Program {
    fn jar(&self) -> &SourceJar {
        &self.source
    }

    fn jar_mut(&mut self) -> &mut SourceJar {
        &mut self.source
    }
}

impl HasJar<SemanticJar> for Program {
    fn jar(&self) -> &SemanticJar {
        &self.semantic
    }

    fn jar_mut(&mut self) -> &mut SemanticJar {
        &mut self.semantic
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
