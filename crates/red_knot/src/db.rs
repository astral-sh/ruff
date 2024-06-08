use std::sync::Arc;

pub use jars::{HasJar, HasJars};
pub use query::{QueryError, QueryResult};
pub use runtime::DbRuntime;
pub use storage::JarsStorage;

use crate::files::FileId;
use crate::lint::{LintSemanticStorage, LintSyntaxStorage};
use crate::module::ModuleResolver;
use crate::parse::ParsedStorage;
use crate::semantic::SemanticIndexStorage;
use crate::semantic::TypeStore;
use crate::source::SourceStorage;

mod jars;
mod query;
mod runtime;
mod storage;

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
    #[must_use]
    fn snapshot(&self) -> Snapshot<Self>;
}

pub trait DbWithJar<Jar>: Database + HasJar<Jar> {}

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

pub trait Upcast<T: ?Sized> {
    fn upcast(&self) -> &T;
}

// Red knot specific databases code.

pub trait SourceDb: DbWithJar<SourceJar> {
    // queries
    fn file_id(&self, path: &std::path::Path) -> FileId;

    fn file_path(&self, file_id: FileId) -> Arc<std::path::Path>;
}

pub trait SemanticDb: SourceDb + DbWithJar<SemanticJar> + Upcast<dyn SourceDb> {}

pub trait LintDb: SemanticDb + DbWithJar<LintJar> + Upcast<dyn SemanticDb> {}

pub trait Db: LintDb + Upcast<dyn LintDb> {}

#[derive(Debug, Default)]
pub struct SourceJar {
    pub sources: SourceStorage,
    pub parsed: ParsedStorage,
}

#[derive(Debug, Default)]
pub struct SemanticJar {
    pub module_resolver: ModuleResolver,
    pub semantic_indices: SemanticIndexStorage,
    pub type_store: TypeStore,
}

#[derive(Debug, Default)]
pub struct LintJar {
    pub lint_syntax: LintSyntaxStorage,
    pub lint_semantic: LintSemanticStorage,
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use crate::db::{
        Database, DbRuntime, DbWithJar, HasJar, HasJars, JarsStorage, LintDb, LintJar, QueryResult,
        SourceDb, SourceJar, Upcast,
    };
    use crate::files::{FileId, Files};

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

    impl HasJar<LintJar> for TestDb {
        fn jar(&self) -> QueryResult<&LintJar> {
            Ok(&self.jars()?.2)
        }

        fn jar_mut(&mut self) -> &mut LintJar {
            &mut self.jars_mut().2
        }
    }

    impl SourceDb for TestDb {
        fn file_id(&self, path: &Path) -> FileId {
            self.files.intern(path)
        }

        fn file_path(&self, file_id: FileId) -> Arc<Path> {
            self.files.path(file_id)
        }
    }

    impl DbWithJar<SourceJar> for TestDb {}

    impl Upcast<dyn SourceDb> for TestDb {
        fn upcast(&self) -> &(dyn SourceDb + 'static) {
            self
        }
    }

    impl SemanticDb for TestDb {}

    impl DbWithJar<SemanticJar> for TestDb {}

    impl Upcast<dyn SemanticDb> for TestDb {
        fn upcast(&self) -> &(dyn SemanticDb + 'static) {
            self
        }
    }

    impl LintDb for TestDb {}

    impl Upcast<dyn LintDb> for TestDb {
        fn upcast(&self) -> &(dyn LintDb + 'static) {
            self
        }
    }

    impl DbWithJar<LintJar> for TestDb {}

    impl HasJars for TestDb {
        type Jars = (SourceJar, SemanticJar, LintJar);

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
}
