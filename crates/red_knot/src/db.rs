mod jars;
mod query;
mod runtime;
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

pub use jars::{HasJar, HasJars};
pub use query::{QueryError, QueryResult};
pub use runtime::DbRuntime;
pub use storage::JarsStorage;

pub trait Database {
    /// Returns a reference to the runtime of the current worker.
    fn runtime(&self) -> &DbRuntime;

    /// Returns a mutable reference to the runtime. Only one worker can hold a mutable reference to the runtime.
    fn runtime_mut(&mut self) -> &mut DbRuntime;

    /// Returns `Ok` if the queries have not been cancelled and `Err(QueryError::Cancelled)` otherwise.
    fn cancelled(&self) -> QueryResult<()> {
        self.runtime().cancelled()
    }

    /// Returns `true` if the queries have been cancelled.
    fn is_cancelled(&self) -> bool {
        self.runtime().is_cancelled()
    }
}

/// Database that supports running queries from multiple threads.
pub trait ParallelDatabase: Database + Send {
    /// Creates a snapshot of the database state that can be used to query the database in another thread.
    ///
    /// The snapshot is a read-only view of the database but query results are shared between threads.
    /// All queries will be automatically cancelled when applying any mutations (calling [`HasJars::jars_mut`])
    /// to the database (not the snapshot, because they're readonly).
    ///
    /// ## Creating a snapshot
    ///
    /// Creating a snapshot of the database's jars is cheap but creating a snapshot of
    /// other state stored on the database might require deep-cloning data. That's why you should
    /// avoid creating snapshots in a hot function (e.g. don't create a snapshot for each file, instead
    /// create a snapshot when scheduling the check of an entire program).
    ///
    /// ## Salsa compatibility
    /// Salsa prohibits creating a snapshot while running a local query (it's fine if other workers run a query) [[source](https://github.com/salsa-rs/salsa/issues/80)].
    /// We should avoid creating snapshots while running a query because we might want to adopt Salsa in the future (if we can figure out persistent caching).
    /// Unfortunately, the infrastructure doesn't provide an automated way of knowing when a query is run, that's
    /// why we have to "enforce" this constraint manually.
    fn snapshot(&self) -> Snapshot<Self>;
}

/// Readonly snapshot of a database.
///
/// ## Dead locks
/// A snapshot should always be dropped as soon as it is no longer necessary to run queries.
/// Storing the snapshot without running a query or periodically checking if cancellation was requested
/// can lead to deadlocks because mutating the [`Database`] requires cancels all pending queries
/// and waiting for all [`Snapshot`]s to be dropped.
#[derive(Debug)]
pub struct Snapshot<DB: ?Sized>
where
    DB: ParallelDatabase,
{
    db: DB,
}

impl<DB> Snapshot<DB>
where
    DB: ParallelDatabase,
{
    pub fn new(db: DB) -> Self {
        Snapshot { db }
    }
}

impl<DB> std::ops::Deref for Snapshot<DB>
where
    DB: ParallelDatabase,
{
    type Target = DB;

    fn deref(&self) -> &DB {
        &self.db
    }
}

// Red knot specific databases code.

pub trait SourceDb: Database {
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

    use crate::db::{
        Database, DbRuntime, HasJar, HasJars, JarsStorage, ParallelDatabase, QueryResult, Snapshot,
        SourceDb, SourceJar,
    };
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
        jars: JarsStorage<Self>,
    }

    impl HasJar<SourceJar> for TestDb {
        fn jar(&self) -> QueryResult<&SourceJar> {
            Ok(&self.jars()?.0)
        }

        fn jar_mut(&mut self) -> &mut SourceJar {
            &mut self.jars_mut().0
        }
    }

    impl HasJar<SemanticJar> for TestDb {
        fn jar(&self) -> QueryResult<&SemanticJar> {
            Ok(&self.jars()?.1)
        }

        fn jar_mut(&mut self) -> &mut SemanticJar {
            &mut self.jars_mut().1
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

    impl HasJars for TestDb {
        type Jars = (SourceJar, SemanticJar);

        fn jars(&self) -> QueryResult<&Self::Jars> {
            self.jars.jars()
        }

        fn jars_mut(&mut self) -> &mut Self::Jars {
            self.jars.jars_mut()
        }
    }

    impl Database for TestDb {
        fn runtime(&self) -> &DbRuntime {
            self.jars.runtime()
        }

        fn runtime_mut(&mut self) -> &mut DbRuntime {
            self.jars.runtime_mut()
        }
    }

    impl ParallelDatabase for TestDb {
        fn snapshot(&self) -> Snapshot<Self> {
            Snapshot::new(Self {
                files: self.files.clone(),
                jars: self.jars.snapshot(),
            })
        }
    }
}
