#![allow(dead_code)]

use anyhow::Context;
pub use document::{DocumentKey, NotebookDocument, PositionEncoding, TextDocument};
pub use session::{ClientSettings, DocumentQuery, DocumentSnapshot, Session};
use std::num::NonZeroUsize;

use crate::server::Server;

#[macro_use]
mod message;

mod document;
mod find_node;
mod logging;
mod semantic;
mod server;
mod session;
mod system;

pub(crate) const SERVER_NAME: &str = "red-knot";
pub(crate) const DIAGNOSTIC_NAME: &str = "Red Knot";

/// A common result type used in most cases where a
/// result type is needed.
pub(crate) type Result<T> = anyhow::Result<T>;

pub(crate) fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn run_server() -> anyhow::Result<()> {
    let four = NonZeroUsize::new(4).unwrap();

    // by default, we set the number of worker threads to `num_cpus`, with a maximum of 4.
    let worker_threads = std::thread::available_parallelism()
        .unwrap_or(four)
        .max(four);

    Server::new(worker_threads)
        .context("Failed to start server")?
        .run()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use red_knot_project::DEFAULT_LINT_REGISTRY;
    use red_knot_python_semantic::lint::{LintRegistry, RuleSelection};
    use red_knot_python_semantic::{default_lint_registry, Db as SemanticDb};
    use ruff_db::files::Files;
    use ruff_db::system::{DbWithTestSystem, System, TestSystem};
    use ruff_db::vendored::VendoredFileSystem;
    use ruff_db::{Db as SourceDb, Upcast};
    use salsa::Event;
    use std::sync::Arc;

    #[salsa::db]
    #[derive(Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        events: Arc<std::sync::Mutex<Vec<Event>>>,
        files: Files,
        system: TestSystem,
        rule_selection: Arc<RuleSelection>,
        vendored: VendoredFileSystem,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            Self {
                storage: salsa::Storage::default(),
                system: TestSystem::default(),
                vendored: red_knot_vendored::file_system().clone(),
                rule_selection: Arc::new(RuleSelection::from_registry(default_lint_registry())),
                files: Files::default(),
                events: Arc::default(),
            }
        }
    }

    impl TestDb {
        /// Takes the salsa events.
        ///
        /// ## Panics
        /// If there are any pending salsa snapshots.
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let inner = Arc::get_mut(&mut self.events).expect("no pending salsa snapshots");

            let events = inner.get_mut().unwrap();
            std::mem::take(&mut *events)
        }
    }

    impl DbWithTestSystem for TestDb {
        fn test_system(&self) -> &TestSystem {
            &self.system
        }

        fn test_system_mut(&mut self) -> &mut TestSystem {
            &mut self.system
        }
    }

    #[salsa::db]
    impl SourceDb for TestDb {
        fn vendored(&self) -> &VendoredFileSystem {
            &self.vendored
        }

        fn system(&self) -> &dyn System {
            &self.system
        }

        fn files(&self) -> &Files {
            &self.files
        }
    }

    impl Upcast<dyn SemanticDb> for TestDb {
        fn upcast(&self) -> &(dyn SemanticDb + 'static) {
            self
        }
        fn upcast_mut(&mut self) -> &mut (dyn SemanticDb + 'static) {
            self
        }
    }

    impl Upcast<dyn SourceDb> for TestDb {
        fn upcast(&self) -> &(dyn SourceDb + 'static) {
            self
        }
        fn upcast_mut(&mut self) -> &mut (dyn SourceDb + 'static) {
            self
        }
    }

    #[salsa::db]
    impl red_knot_python_semantic::Db for TestDb {
        fn is_file_open(&self, file: ruff_db::files::File) -> bool {
            !file.path(self).is_vendored_path()
        }

        fn rule_selection(&self) -> Arc<RuleSelection> {
            self.rule_selection.clone()
        }

        fn lint_registry(&self) -> &LintRegistry {
            &DEFAULT_LINT_REGISTRY
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {
        fn salsa_event(&self, event: &dyn Fn() -> Event) {
            let mut events = self.events.lock().unwrap();
            events.push(event());
        }
    }
}
