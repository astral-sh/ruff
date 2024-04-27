mod jars;
mod query;
mod storage;

use std::path::Path;
use std::sync::Arc;

use crate::files::FileId;
use crate::lint::{Diagnostics, LintSemanticStorage, LintSyntaxStorage};
use crate::module::{Module, ModuleData, ModuleName, ModuleResolver, ModuleSearchPath};
use crate::parse::{Parsed, ParsedStorage};
use crate::source::{Source, SourceStorage};
use crate::symbols::{SymbolId, SymbolTable, SymbolTablesStorage};
use crate::types::{Type, TypeStore};

pub use jars::HasJar;
pub use query::{QueryError, QueryResult};

pub trait SourceDb {
    // queries
    fn file_id(&self, path: &std::path::Path) -> FileId;

    fn file_path(&self, file_id: FileId) -> Arc<std::path::Path>;

    fn source(&self, file_id: FileId) -> QueryResult<Source>;

    fn parse(&self, file_id: FileId) -> QueryResult<Parsed>;

    fn lint_syntax(&self, file_id: FileId) -> QueryResult<Diagnostics>;
}

pub trait SemanticDb: SourceDb {
    // queries
    fn resolve_module(&self, name: ModuleName) -> QueryResult<Option<Module>>;

    fn file_to_module(&self, file_id: FileId) -> QueryResult<Option<Module>>;

    fn path_to_module(&self, path: &Path) -> QueryResult<Option<Module>>;

    fn symbol_table(&self, file_id: FileId) -> QueryResult<Arc<SymbolTable>>;

    fn infer_symbol_type(&self, file_id: FileId, symbol_id: SymbolId) -> QueryResult<Type>;

    fn lint_semantic(&self, file_id: FileId) -> QueryResult<Diagnostics>;

    // mutations

    fn add_module(&mut self, path: &Path) -> Option<(Module, Vec<Arc<ModuleData>>)>;

    fn set_module_search_paths(&mut self, paths: Vec<ModuleSearchPath>);
}

pub trait Db: SemanticDb {}

#[derive(Debug, Default)]
pub struct SourceJar {
    pub sources: SourceStorage,
    pub parsed: ParsedStorage,
    pub lint_syntax: LintSyntaxStorage,
}

#[derive(Debug, Default)]
pub struct SemanticJar {
    pub module_resolver: ModuleResolver,
    pub symbol_tables: SymbolTablesStorage,
    pub type_store: TypeStore,
    pub lint_semantic: LintSemanticStorage,
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use crate::db::{HasJar, QueryResult, SourceDb, SourceJar};
    use crate::files::{FileId, Files};
    use crate::lint::{lint_semantic, lint_syntax, Diagnostics};
    use crate::module::{
        add_module, file_to_module, path_to_module, resolve_module, set_module_search_paths,
        Module, ModuleData, ModuleName, ModuleSearchPath,
    };
    use crate::parse::{parse, Parsed};
    use crate::source::{source_text, Source};
    use crate::symbols::{symbol_table, SymbolId, SymbolTable};
    use crate::types::{infer_symbol_type, Type};

    use super::{SemanticDb, SemanticJar};

    // This can be a partial database used in a single crate for testing.
    // It would hold fewer data than the full database.
    #[derive(Debug, Default)]
    pub(crate) struct TestDb {
        files: Files,
        source: SourceJar,
        semantic: SemanticJar,
    }

    impl HasJar<SourceJar> for TestDb {
        fn jar(&self) -> QueryResult<&SourceJar> {
            Ok(&self.source)
        }

        fn jar_mut(&mut self) -> &mut SourceJar {
            &mut self.source
        }
    }

    impl HasJar<SemanticJar> for TestDb {
        fn jar(&self) -> QueryResult<&SemanticJar> {
            Ok(&self.semantic)
        }

        fn jar_mut(&mut self) -> &mut SemanticJar {
            &mut self.semantic
        }
    }

    impl SourceDb for TestDb {
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

        fn lint_syntax(&self, file_id: FileId) -> QueryResult<Diagnostics> {
            lint_syntax(self, file_id)
        }
    }

    impl SemanticDb for TestDb {
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

        fn lint_semantic(&self, file_id: FileId) -> QueryResult<Diagnostics> {
            lint_semantic(self, file_id)
        }

        fn add_module(&mut self, path: &Path) -> Option<(Module, Vec<Arc<ModuleData>>)> {
            add_module(self, path)
        }

        fn set_module_search_paths(&mut self, paths: Vec<ModuleSearchPath>) {
            set_module_search_paths(self, paths);
        }
    }
}
