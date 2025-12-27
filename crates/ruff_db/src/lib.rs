#![warn(
    clippy::disallowed_methods,
    reason = "Prefer System trait methods over std methods"
)]

use crate::files::Files;
use crate::system::System;
use crate::vendored::VendoredFileSystem;
use ruff_python_ast::PythonVersion;
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;
use std::num::NonZeroUsize;
use ty_static::EnvVars;

pub mod cancellation;
pub mod diagnostic;
pub mod display;
pub mod file_revision;
pub mod files;
pub mod panic;
pub mod parsed;
pub mod source;
pub mod system;
#[cfg(feature = "testing")]
pub mod testing;
pub mod vendored;

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Instant, SystemTime, SystemTimeError};

#[cfg(target_arch = "wasm32")]
pub use web_time::{Instant, SystemTime, SystemTimeError};

pub type FxDashMap<K, V> = dashmap::DashMap<K, V, BuildHasherDefault<FxHasher>>;
pub type FxDashSet<K> = dashmap::DashSet<K, BuildHasherDefault<FxHasher>>;

static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Returns the version of the executing program if set.
pub fn program_version() -> Option<&'static str> {
    VERSION.get().map(|version| version.as_str())
}

/// Sets the version of the executing program.
///
/// ## Errors
/// If the version has already been initialized (can only be set once).
pub fn set_program_version(version: String) -> Result<(), String> {
    VERSION.set(version)
}

/// Most basic database that gives access to files, the host system, source code, and parsed AST.
#[salsa::db]
pub trait Db: salsa::Database {
    fn vendored(&self) -> &VendoredFileSystem;
    fn system(&self) -> &dyn System;
    fn files(&self) -> &Files;
    fn python_version(&self) -> PythonVersion;
}

/// Returns the maximum number of tasks that ty is allowed
/// to process in parallel.
///
/// Returns [`std::thread::available_parallelism`], unless the environment
/// variable `TY_MAX_PARALLELISM` or `RAYON_NUM_THREADS` is set. `TY_MAX_PARALLELISM` takes
/// precedence over `RAYON_NUM_THREADS`.
///
/// Falls back to `1` if `available_parallelism` is not available.
///
/// Setting `TY_MAX_PARALLELISM` to `2` only restricts the number of threads that ty spawns
/// to process work in parallel. For example, to index a directory or checking the files of a project.
/// ty can still spawn more threads for other tasks, e.g. to wait for a Ctrl+C signal or
/// watching the files for changes.
#[expect(
    clippy::disallowed_methods,
    reason = "We don't have access to System here, but this is also only used by the CLI and the server which always run on a real system."
)]
pub fn max_parallelism() -> NonZeroUsize {
    std::env::var(EnvVars::TY_MAX_PARALLELISM)
        .or_else(|_| std::env::var(EnvVars::RAYON_NUM_THREADS))
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism().unwrap_or_else(|_| NonZeroUsize::new(1).unwrap())
        })
}

/// Trait for types that can provide Rust documentation.
///
/// Use `derive(RustDoc)` to automatically implement this trait for types that have a static string documentation.
pub trait RustDoc {
    fn rust_doc() -> &'static str;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::Db;
    use crate::files::Files;
    use crate::system::TestSystem;
    use crate::system::{DbWithTestSystem, System};
    use crate::vendored::VendoredFileSystem;

    type Events = Arc<Mutex<Vec<salsa::Event>>>;

    /// Database that can be used for testing.
    ///
    /// Uses an in memory filesystem and it stubs out the vendored files by default.
    #[salsa::db]
    #[derive(Default, Clone)]
    pub(crate) struct TestDb {
        storage: salsa::Storage<Self>,
        files: Files,
        system: TestSystem,
        vendored: VendoredFileSystem,
        events: Events,
    }

    impl TestDb {
        pub(crate) fn new() -> Self {
            let events = Events::default();
            Self {
                storage: salsa::Storage::new(Some(Box::new({
                    let events = events.clone();
                    move |event| {
                        tracing::trace!("event: {:?}", event);
                        let mut events = events.lock().unwrap();
                        events.push(event);
                    }
                }))),
                system: TestSystem::default(),
                vendored: VendoredFileSystem::default(),
                events,
                files: Files::default(),
            }
        }

        /// Empties the internal store of salsa events that have been emitted,
        /// and returns them as a `Vec` (equivalent to [`std::mem::take`]).
        pub(crate) fn take_salsa_events(&mut self) -> Vec<salsa::Event> {
            let mut events = self.events.lock().unwrap();

            std::mem::take(&mut *events)
        }

        /// Clears the emitted salsa events.
        pub(crate) fn clear_salsa_events(&mut self) {
            self.take_salsa_events();
        }

        pub(crate) fn with_vendored(&mut self, vendored_file_system: VendoredFileSystem) {
            self.vendored = vendored_file_system;
        }
    }

    #[salsa::db]
    impl Db for TestDb {
        fn vendored(&self) -> &VendoredFileSystem {
            &self.vendored
        }

        fn system(&self) -> &dyn System {
            &self.system
        }

        fn files(&self) -> &Files {
            &self.files
        }

        fn python_version(&self) -> ruff_python_ast::PythonVersion {
            ruff_python_ast::PythonVersion::latest_ty()
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
    impl salsa::Database for TestDb {}
}
