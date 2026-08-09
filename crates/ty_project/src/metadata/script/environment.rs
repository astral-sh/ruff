use std::hash::Hasher;
use std::sync::Arc;

use parking_lot::{Condvar, Mutex, MutexGuard};
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_db::FxDashMap;
use ruff_db::files::{File, Files};
use ruff_db::system::SystemPathBuf;
use salsa::Setter;
use ty_static::EnvVars;

use super::script_tag;
use crate::metadata::uv::{
    ScriptEnvironmentCacheKey, ScriptSyncResult, ScriptSyncTask, Uv, UvExecutor, UvMetadata,
};
use crate::{Db, ProgressReporter};

/// Lazily initialized script environments (results of calling `uv workspace metadata --script`).
///
/// A script's uv metadata lives outside Salsa, so it is modeled as an input that is created on
/// demand before the script is checked. Updating the input is the responsibility of the host
/// because doing so requires a mutable database.
///
/// Watch mode refreshes environments after script metadata changes on disk. Checks continue using
/// the last completed environment while a refresh runs.
#[derive(Clone, Default)]
pub struct ScriptEnvironments {
    inner: Arc<ScriptEnvironmentsInner>,
}

impl ScriptEnvironments {
    pub(crate) fn executor(&self) -> &UvExecutor {
        &self.inner.executor
    }

    /// Ensures that a usable environment exists for `file`.
    ///
    /// Concurrent callers wait for the initial environment creation to finish.
    pub(crate) fn ensure_environment_initialized(
        &self,
        db: &dyn Db,
        file: File,
        reporter: Option<&dyn ProgressReporter>,
    ) {
        if !script_integration_enabled(db) {
            return;
        }

        let Some(task) = self.sync_task(db, file) else {
            return;
        };
        let shared_entry = self.entry(file);
        let cache_key = task.cache_key;

        let mut state = shared_entry.state.lock();
        loop {
            match *state {
                ScriptEnvironmentEntryState::Initializing => {
                    state = shared_entry.wait_until_initialized(state);
                }
                ScriptEnvironmentEntryState::Ready(_)
                | ScriptEnvironmentEntryState::Refreshing { .. } => return,
                ScriptEnvironmentEntryState::Vacant => {
                    *state = ScriptEnvironmentEntryState::Initializing;
                    let claim = InitializationClaim::new(&shared_entry);
                    drop(state);

                    tracing::debug!("Initializing script environment for `{}`", task.path);

                    let progress = reporter.and_then(|reporter| reporter.for_script(db, file));
                    let output = self.inner.executor.run(db.system(), task);
                    drop(progress);

                    let (uv_metadata, initialization_error) =
                        script_environment_metadata(db, output);
                    let environment = ScriptEnvironment::new(
                        db,
                        Some(cache_key),
                        uv_metadata,
                        initialization_error,
                    );
                    claim.complete(environment);
                    return;
                }
            }
        }
    }

    /// Creates a stable provisional input and returns synchronization work when needed.
    pub fn prepare_sync(&self, db: &dyn Db, file: File) -> Option<ScriptSyncTask> {
        if !script_integration_enabled(db) {
            return None;
        }

        let task = self.sync_task(db, file)?;
        let cache_key = task.cache_key;
        let shared_entry = self.entry(file);
        let mut state = shared_entry.state.lock();

        let environment = match *state {
            ScriptEnvironmentEntryState::Initializing => {
                // A blocking check already owns the first synchronization. All inputs to the
                // cache key belong to its Salsa snapshot, so a different key would require
                // cancellation of that snapshot before this request can run.
                tracing::trace!(
                    "Script environment synchronization for `{}` is already running",
                    task.path
                );
                return None;
            }
            ScriptEnvironmentEntryState::Vacant => ScriptEnvironment::new(db, None, None, None),
            ScriptEnvironmentEntryState::Ready(environment)
            | ScriptEnvironmentEntryState::Refreshing { environment, .. } => environment,
        };

        if environment.synchronized_cache_key(db) == Some(cache_key) {
            if let ScriptEnvironmentEntryState::Refreshing { desired, .. } = &mut *state {
                // Keep the in-flight call running, but discard its result if this remains the
                // desired cache key when it completes.
                *desired = cache_key;
            }
            tracing::trace!(
                "Script environment for `{}` is already synchronized",
                task.path
            );
            return None;
        }

        match &mut *state {
            ScriptEnvironmentEntryState::Vacant | ScriptEnvironmentEntryState::Ready(_) => {
                *state = ScriptEnvironmentEntryState::Refreshing {
                    environment,
                    desired: cache_key,
                };
                tracing::debug!(
                    "Requested script environment synchronization for `{}`",
                    task.path
                );
                Some(task)
            }
            ScriptEnvironmentEntryState::Refreshing { desired, .. } if *desired == cache_key => {
                tracing::trace!(
                    "Script environment synchronization for `{}` is already requested",
                    task.path
                );
                None
            }
            ScriptEnvironmentEntryState::Refreshing { desired, .. } => {
                tracing::debug!(
                    "Replaced pending script environment synchronization for `{}`",
                    task.path
                );
                *desired = cache_key;
                None
            }
            ScriptEnvironmentEntryState::Initializing => None,
        }
    }

    /// Applies a current synchronization result.
    ///
    /// Returns whether the environment changed and any replacement work that remains.
    pub fn complete_sync(
        &self,
        db: &mut dyn Db,
        result: ScriptSyncResult,
    ) -> (bool, Option<ScriptSyncTask>) {
        let ScriptSyncResult {
            mut task,
            output,
            progress: _progress,
        } = result;
        let Some(shared_entry) = self.existing_entry(task.file) else {
            tracing::debug!(
                "Discarded synchronization result for unknown script `{}`",
                task.path
            );
            return (false, None);
        };

        let environment = {
            let mut state = shared_entry.state.lock();
            let ScriptEnvironmentEntryState::Refreshing {
                environment,
                desired,
            } = *state
            else {
                tracing::debug!(
                    "Discarded unexpected script environment synchronization result for `{}`",
                    task.path
                );
                return (false, None);
            };

            if desired != task.cache_key {
                if environment.synchronized_cache_key(db) == Some(desired) {
                    *state = ScriptEnvironmentEntryState::Ready(environment);
                    tracing::debug!(
                        "Discarded obsolete script environment synchronization result for `{}`",
                        task.path
                    );
                    return (false, None);
                }

                tracing::debug!(
                    "Discarded superseded script environment synchronization result for `{}`",
                    task.path
                );
                task.cache_key = desired;
                return (false, Some(task));
            }

            environment
        };

        apply_sync_result(db, environment, &task, output);

        let mut state = shared_entry.state.lock();
        match &mut *state {
            ScriptEnvironmentEntryState::Refreshing { desired, .. }
                if *desired != task.cache_key =>
            {
                tracing::debug!(
                    "Script environment synchronization for `{}` was superseded while applying its result",
                    task.path
                );
                task.cache_key = *desired;
                (true, Some(task))
            }
            ScriptEnvironmentEntryState::Refreshing { environment, .. } => {
                let environment = *environment;
                *state = ScriptEnvironmentEntryState::Ready(environment);
                (true, None)
            }
            ScriptEnvironmentEntryState::Vacant
            | ScriptEnvironmentEntryState::Initializing
            | ScriptEnvironmentEntryState::Ready(_) => {
                debug_assert!(
                    false,
                    "script synchronization stopped while applying its result"
                );
                (true, None)
            }
        }
    }

    /// Returns whether an environment has already been discovered for `file`.
    pub fn contains(&self, file: File) -> bool {
        self.existing_entry(file)
            .is_some_and(|entry| entry.state.lock().environment().is_some())
    }

    /// Returns all files whose environments have already been discovered.
    pub fn files(&self) -> Vec<File> {
        let entries: Vec<_> = self
            .inner
            .by_file
            .iter()
            .map(|entry| (*entry.key(), Arc::clone(entry.value())))
            .collect();

        entries
            .into_iter()
            .filter_map(|(file, entry)| entry.state.lock().environment().map(|_| file))
            .collect()
    }

    /// Returns the environment prepared for `file` by the host.
    pub(super) fn environment(&self, db: &dyn Db, file: File) -> Option<ScriptEnvironment> {
        if !script_integration_enabled(db) || file.path(db).as_system_path().is_none() {
            return None;
        }

        let Some(shared_entry) = self.existing_entry(file) else {
            panic!("script environment was not initialized by its host");
        };
        let state = shared_entry.state.lock();
        let state = shared_entry.wait_until_initialized(state);
        let environment = state.environment();
        assert!(
            environment.is_some(),
            "script environment was not initialized by its host"
        );
        environment
    }

    fn existing_entry(&self, file: File) -> Option<SharedScriptEnvironmentEntry> {
        let entry = self.inner.by_file.get(&file)?;
        Some(Arc::clone(entry.value()))
    }

    fn entry(&self, file: File) -> SharedScriptEnvironmentEntry {
        // Drop the map's shard guard before initializing an entry so unrelated scripts can
        // initialize concurrently even when their files occupy the same map shard.
        Arc::clone(self.inner.by_file.entry(file).or_default().value())
    }

    fn sync_task(&self, db: &dyn Db, file: File) -> Option<ScriptSyncTask> {
        let path = file.path(db).as_system_path()?.to_path_buf();
        let tag = script_tag(db, file)?;
        let python = script_python(db);
        let mut hasher = CacheKeyHasher::new();
        tag.metadata().cache_key(&mut hasher);
        python.cache_key(&mut hasher);

        Some(ScriptSyncTask {
            file,
            path,
            python,
            cache_key: hasher.finish(),
        })
    }
}

impl std::panic::RefUnwindSafe for ScriptEnvironments {}

/// The last completed environment synchronization for a standalone script.
#[salsa::input(heap_size=ruff_memory_usage::heap_size)]
#[derive(Debug)]
pub(super) struct ScriptEnvironment {
    /// The cache key for the last completed `uv metadata` synchronization.
    ///
    /// `None` means that no synchronization has completed yet. The input may be provisional while
    /// the host schedules the first synchronization.
    #[returns(copy)]
    synchronized_cache_key: Option<ScriptEnvironmentCacheKey>,

    /// The metadata for the most recently completed `uv metadata` call (parsed).
    ///
    /// `None` means that no uv metadata is available. This can be because initialization is still
    /// pending, is inapplicable, or failed; `initialization_error` distinguishes the failure case.
    /// `Some` may still describe a script without an environment.
    #[returns(as_ref)]
    pub(super) uv_metadata: Option<UvMetadata>,

    /// The error message when `uv metadata` failed.
    ///
    /// `None` if the metadata sync hasn't completed yet or the most recent sync was successful.
    #[returns(as_deref)]
    pub(super) initialization_error: Option<Box<str>>,
}

#[derive(Default)]
struct ScriptEnvironmentsInner {
    by_file: FxDashMap<File, SharedScriptEnvironmentEntry>,
    executor: UvExecutor,
}

type SharedScriptEnvironmentEntry = Arc<ScriptEnvironmentEntry>;

#[derive(Default)]
struct ScriptEnvironmentEntry {
    state: Mutex<ScriptEnvironmentEntryState>,
    initialized: Condvar,
}

impl ScriptEnvironmentEntry {
    /// Waits for a blocking initial synchronization to finish.
    fn wait_until_initialized<'entry>(
        &'entry self,
        mut state: MutexGuard<'entry, ScriptEnvironmentEntryState>,
    ) -> MutexGuard<'entry, ScriptEnvironmentEntryState> {
        while matches!(*state, ScriptEnvironmentEntryState::Initializing) {
            self.initialized.wait(&mut state);
        }
        state
    }
}

/// The environment and in-flight synchronization for a standalone script.
///
/// This state lives next to the Salsa input rather than in the worker service because each project
/// database independently synchronizes its files.
#[derive(Copy, Clone, Default)]
enum ScriptEnvironmentEntryState {
    #[default]
    Vacant,

    /// A blocking check owns the script's initial synchronization.
    Initializing,

    /// The stable Salsa input when no synchronization is running.
    ///
    /// Its synchronized cache key is absent if the LSP created a provisional environment before
    /// requesting synchronization.
    Ready(ScriptEnvironment),

    /// A `uv workspace metadata` call is running or queued.
    Refreshing {
        /// The stable input visible while uv synchronizes the desired script metadata.
        environment: ScriptEnvironment,

        /// The latest explicitly requested cache key.
        ///
        /// The script path, uv executable, and Python override are fixed for the lifetime of a
        /// project database. If those settings become dynamic, this state must retain the complete
        /// replacement task instead.
        desired: ScriptEnvironmentCacheKey,
    },
}

impl ScriptEnvironmentEntryState {
    fn environment(&self) -> Option<ScriptEnvironment> {
        match *self {
            Self::Ready(environment) | Self::Refreshing { environment, .. } => Some(environment),
            Self::Vacant | Self::Initializing => None,
        }
    }
}

/// Ownership of a script's blocking initial synchronization.
///
/// Like Salsa's query claim, this outlives the short-held state lock. Dropping the claim during
/// cancellation or unwinding releases the synchronization and wakes every waiter.
#[must_use]
struct InitializationClaim<'entry>(Option<&'entry ScriptEnvironmentEntry>);

impl<'entry> InitializationClaim<'entry> {
    fn new(entry: &'entry ScriptEnvironmentEntry) -> Self {
        Self(Some(entry))
    }

    fn complete(mut self, environment: ScriptEnvironment) {
        self.finish(ScriptEnvironmentEntryState::Ready(environment));
    }

    fn finish(&mut self, next: ScriptEnvironmentEntryState) {
        if let Some(entry) = self.0.take() {
            let mut state = entry.state.lock();
            debug_assert!(matches!(*state, ScriptEnvironmentEntryState::Initializing));
            *state = next;
            drop(state);
            entry.initialized.notify_all();
        }
    }
}

impl Drop for InitializationClaim<'_> {
    fn drop(&mut self) {
        self.finish(ScriptEnvironmentEntryState::Vacant);
    }
}

fn apply_sync_result(
    db: &mut dyn Db,
    environment: ScriptEnvironment,
    task: &ScriptSyncTask,
    output: std::io::Result<std::process::Output>,
) {
    debug_assert_ne!(
        environment.synchronized_cache_key(db),
        Some(task.cache_key),
        "a synchronized cache key should not be scheduled again"
    );
    let previous_root = environment
        .uv_metadata(db)
        .and_then(UvMetadata::environment)
        .map(ToOwned::to_owned);
    let (uv_metadata, initialization_error) = script_environment_metadata(db, output);

    if let Some(root) = previous_root {
        // uv may change installed packages without changing the environment path.
        Files::sync_all_recursive(db, [root]);
    }

    environment.set_uv_metadata(db).to(uv_metadata);
    environment
        .set_initialization_error(db)
        .to(initialization_error);
    // Publish the cache key last because it identifies the metadata and error as the completed
    // result for this script source.
    environment
        .set_synchronized_cache_key(db)
        .to(Some(task.cache_key));

    tracing::debug!("Updated script environment metadata for `{}`", task.path);
}

fn script_environment_metadata(
    db: &dyn Db,
    output: std::io::Result<std::process::Output>,
) -> (Option<UvMetadata>, Option<Box<str>>) {
    match Uv::parse_metadata_output(db.system(), output) {
        Ok(metadata) => (Some(metadata), None),
        Err(error) => (None, Some(error.to_string().into_boxed_str())),
    }
}

fn script_integration_enabled(db: &dyn Db) -> bool {
    matches!(
        db.system().env_var(EnvVars::TY_UV).as_deref(),
        Ok("1" | "true" | "scripts")
    )
}

fn script_python(db: &dyn Db) -> Option<SystemPathBuf> {
    let metadata = db.project().metadata(db);

    metadata
        .override_options
        .as_deref()
        .and_then(|options| options.environment.as_ref())
        .and_then(|environment| environment.python.as_ref())
        .map(|python| python.absolute(metadata.root(), db.system()))
}

#[cfg(test)]
mod tests {
    use std::io;

    use anyhow::Context;
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithWritableSystem, SystemPath};
    use ty_static::EnvVars;

    use super::{ScriptEnvironments, ScriptSyncResult};
    use crate::db::testing::TestDb;
    use crate::{Db as _, ProjectMetadata};

    #[test]
    fn superseded_synchronization_only_applies_the_latest_cache_key() -> anyhow::Result<()> {
        let root = SystemPath::new("/project").to_path_buf();
        let path = root.join("script.py");
        let mut db = TestDb::new(ProjectMetadata::new("test", root));
        db.writable_system().set_env_var(EnvVars::TY_UV, "scripts");
        db.write_file(&path, "# /// script\n# dependencies = []\n# ///\n")?;
        let file = system_path_to_file(&db, &path)?;
        let environments: ScriptEnvironments = db.script_environments().clone();

        let first = environments
            .prepare_sync(&db, file)
            .context("the initial script metadata should be synchronized")?;
        assert!(environments.prepare_sync(&db, file).is_none());

        db.write_file(&path, "# /// script\n# dependencies = [\"anyio\"]\n# ///\n")?;
        assert!(environments.prepare_sync(&db, file).is_none());
        let second_cache_key = environments
            .sync_task(&db, file)
            .context("the script should have synchronization work")?
            .cache_key;

        // Source visible to Salsa can change without another synchronization request, as it does
        // for unsaved LSP edits. Reschedule the explicitly requested cache key, not that newer
        // source.
        db.write_file(&path, "# /// script\n# dependencies = [\"idna\"]\n# ///\n")?;
        let third_cache_key = environments
            .sync_task(&db, file)
            .context("the script should have synchronization work")?
            .cache_key;
        assert_ne!(second_cache_key, third_cache_key);

        let (changed, next) = environments.complete_sync(
            &mut db,
            ScriptSyncResult {
                task: first,
                output: Err(io::Error::other("uv failed")),
                progress: None,
            },
        );
        assert!(!changed);
        let second =
            next.context("the latest synchronization should be scheduled after the stale one")?;
        assert_eq!(second.cache_key, second_cache_key);

        let (changed, next) = environments.complete_sync(
            &mut db,
            ScriptSyncResult {
                task: second,
                output: Err(io::Error::other("uv failed")),
                progress: None,
            },
        );
        assert!(changed);
        assert!(next.is_none());
        let third = environments
            .prepare_sync(&db, file)
            .context("the newer source should synchronize when explicitly requested")?;
        assert_eq!(third.cache_key, third_cache_key);
        let (changed, next) = environments.complete_sync(
            &mut db,
            ScriptSyncResult {
                task: third,
                output: Err(io::Error::other("uv failed")),
                progress: None,
            },
        );
        assert!(changed);
        assert!(next.is_none());
        assert!(environments.prepare_sync(&db, file).is_none());

        Ok(())
    }
}
