use std::path::Path;
use std::sync::Arc;

use crate::files::FileId;
use crate::lint::{Diagnostics, LintSyntaxStorage};
use crate::module::{Module, ModuleData, ModuleName, ModuleResolver, ModuleSearchPath};
use crate::parse::{Parsed, ParsedStorage};
use crate::source::{Source, SourceStorage};
use crate::symbols::{SymbolTable, SymbolTablesStorage};

pub trait SourceDb {
    // queries
    fn file_id(&self, path: &std::path::Path) -> FileId;

    fn file_path(&self, file_id: FileId) -> Arc<std::path::Path>;

    fn source(&self, file_id: FileId) -> Source;

    fn parse(&self, file_id: FileId) -> Parsed;

    fn lint_syntax(&self, file_id: FileId) -> Diagnostics;
}

pub trait SemanticDb: SourceDb {
    // queries
    fn resolve_module(&self, name: ModuleName) -> Option<Module>;

    fn symbol_table(&self, file_id: FileId) -> Arc<SymbolTable>;

    // mutations
    fn path_to_module(&mut self, path: &Path) -> Option<Module>;

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
}

/// Gives access to a specific jar in the database.
///
/// Nope, the terminology isn't borrowed from Java but from Salsa <https://salsa-rs.github.io/salsa/>,
/// which is an analogy to storing the salsa in different jars.
///
/// The basic idea is that each crate can define its own jar and the jars can be combined to a single
/// database in the top level crate. Each crate also defines its own `Database` trait. The combination of
/// `Database` trait and the jar allows to write queries in isolation without having to know how they get composed at the upper levels.
///
/// Salsa further defines a `HasIngredient` trait which slices the jar to a specific storage (e.g. a specific cache).
/// We don't need this just jet because we write our queries by hand. We may want a similar trait if we decide
/// to use a macro to generate the queries.
pub trait HasJar<T> {
    /// Gives a read-only reference to the jar.
    fn jar(&self) -> &T;

    /// Gives a mutable reference to the jar.
    fn jar_mut(&mut self) -> &mut T;
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::db::{HasJar, SourceDb, SourceJar};
    use crate::files::{FileId, Files};
    use crate::lint::{lint_syntax, Diagnostics};
    use crate::module::{
        add_module, path_to_module, resolve_module, set_module_search_paths, Module, ModuleData,
        ModuleName, ModuleSearchPath,
    };
    use crate::parse::{parse, Parsed};
    use crate::source::{source_text, Source};
    use crate::symbols::{symbol_table, SymbolTable};
    use std::path::Path;
    use std::sync::Arc;

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
        fn jar(&self) -> &SourceJar {
            &self.source
        }

        fn jar_mut(&mut self) -> &mut SourceJar {
            &mut self.source
        }
    }

    impl HasJar<SemanticJar> for TestDb {
        fn jar(&self) -> &SemanticJar {
            &self.semantic
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

    impl SemanticDb for TestDb {
        fn resolve_module(&self, name: ModuleName) -> Option<Module> {
            resolve_module(self, name)
        }

        fn symbol_table(&self, file_id: FileId) -> Arc<SymbolTable> {
            symbol_table(self, file_id)
        }

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
}
