//! Manages uv environments for standalone Python scripts.
//!
//! A script can declare its Python requirements and dependencies in an inline metadata block.
//! Synchronizing that metadata with uv produces an environment whose Python version and installed
//! packages determine how ty checks the script.
//!
//! Discovering which files need synchronization requires reading their contents. Initializing every
//! script before checking a project would therefore require inspecting every candidate file upfront.
//! We also want to avoid initializing environments for files that never need checking.
//!
//! Instead, project checks discover and initialize script environments lazily. When checking first
//! encounters a script without an environment, it runs uv and waits for the result before continuing.
//! Waiting is necessary because the script's dependencies and Python version must be known to
//! produce accurate diagnostics.
//!
//! Each script's virtual environment is represented by a stable [`ScriptEnvironment`] Salsa input.
//! Updating that input invalidates semantic queries that depend on the script's Python version or
//! module search paths, ensuring that checks are rerun after synchronization.

use std::hash::Hasher;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Condvar, Mutex, MutexGuard};
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_db::FxDashMap;
use ruff_db::files::File;
use ruff_db::system::SystemPathBuf;

use super::script_tag;
use crate::uv::{ScriptSyncTask, Uv, UvMetadata, UvSyncService};
use crate::{Db, ProgressReporter, UseUv};

const CANCELLATION_CHECK_INTERVAL: Duration = Duration::from_millis(1);

/// Returns the Salsa input representing `file`'s script environment.
///
/// Project checks normally synchronize the script's virtual environment before it is needed. This
/// function never invokes uv; it returns the [`ScriptEnvironment`] input so semantic queries can
/// depend on it.
///
/// If no [`ScriptEnvironment`] exists, creates one without uv metadata. Its identity
/// remains the same when synchronization later provides that metadata, just as a [`File`]
/// continues to identify the same path when a previously nonexistent file is created. Updating
/// the existing input ensures Salsa invalidates queries that read it before initialization.
///
/// If a blocking synchronization is already running, waits for it to finish instead of creating
/// a second [`ScriptEnvironment`] input.
///
/// Returns `None` if script integration is disabled or the script is not an actual file on disk.
pub(super) fn script_environment(db: &dyn Db, file: File) -> Option<ScriptEnvironment> {
    db.script_environments().environment(db, file)
}

/// Manages and synchronizes PEP 723 script environments using `uv metadata`.
#[derive(Clone, Default)]
pub struct ScriptEnvironments {
    inner: Arc<ScriptEnvironmentsInner>,
}

impl ScriptEnvironments {
    pub(crate) fn new(use_uv: UseUv) -> Self {
        Self {
            inner: Arc::new(ScriptEnvironmentsInner {
                use_uv,
                ..ScriptEnvironmentsInner::default()
            }),
        }
    }

    /// Initializes `file`'s environment before checking it.
    ///
    /// Discovering scripts requires reading file contents, so initializing every environment before
    /// a project check would require eagerly inspecting every candidate file.
    ///
    /// Instead, project checks initialize virtual environments lazily as they encounter scripts.
    /// If no [`ScriptEnvironment`] exists, this method synchronizes the virtual environment and
    /// waits for uv before creating the input. An existing [`ScriptEnvironment`] is reused.
    ///
    /// Concurrent callers wait for the initial environment creation to finish.
    pub(crate) fn initialize_blocking(
        &self,
        db: &dyn Db,
        file: File,
        reporter: &dyn ProgressReporter,
    ) {
        if !self.is_enabled() {
            return;
        }

        let Some(task) = script_sync_task(db, file) else {
            return;
        };
        let entry = self.entry(file);

        let mut state = entry.state.lock();
        loop {
            match &*state {
                ScriptEnvironmentState::Current { .. } => {
                    return;
                }

                // Another caller is synchronizing the virtual environment and will create its
                // `ScriptEnvironment` input when uv finishes.
                // Wait for that caller instead of starting a second synchronization.
                ScriptEnvironmentState::InitializingBlocking { cache_key } => {
                    debug_assert_eq!(
                        *cache_key,
                        task.request.cache_key(),
                        "concurrent initializations must use the same script environment cache key"
                    );
                    // If the other caller is cancelled, it restores the entry to `Vacant`. Check the
                    // state again after waiting so this caller can initialize the environment instead.
                    state = entry.wait_until_initialized(db, state);
                }

                ScriptEnvironmentState::Vacant => {
                    let cache_key = task.request.cache_key();
                    let claim = InitializationClaim::new(&entry, state, cache_key);

                    db.unwind_if_revision_cancelled();
                    tracing::debug!(
                        "Initializing script environment for `{}`",
                        task.request.path()
                    );

                    // Run uv and show progress until the synchronization finishes.
                    let output = {
                        let _progress = reporter.for_script(db, task.file);
                        self.inner.sync_service.run_blocking(db, task)
                    };

                    // Create the `ScriptEnvironment` input from uv's output, retaining its cache key
                    // and any initialization error.
                    let (uv_metadata, initialization_error) = parse_metadata_output(db, output);
                    let environment = ScriptEnvironment::new(
                        db,
                        Some(cache_key),
                        uv_metadata,
                        initialization_error,
                    );

                    // Store the `ScriptEnvironment` input and wake waiting callers. Dropping the
                    // claim without completing it would instead restore the entry to `Vacant`.
                    claim.complete(environment);
                    return;
                }
            }
        }
    }

    fn environment(&self, db: &dyn Db, file: File) -> Option<ScriptEnvironment> {
        if !self.is_enabled() || file.path(db).as_system_path().is_none() {
            return None;
        }

        let entry = self.entry(file);
        let mut state = entry.state.lock();

        loop {
            match *state {
                ScriptEnvironmentState::Vacant => {
                    let environment = ScriptEnvironment::new(db, None, None, None);
                    *state = ScriptEnvironmentState::Current { environment };
                    return Some(environment);
                }
                ScriptEnvironmentState::InitializingBlocking { .. } => {
                    state = entry.wait_until_initialized(db, state);
                }
                ScriptEnvironmentState::Current { environment } => {
                    return Some(environment);
                }
            }
        }
    }

    fn is_enabled(&self) -> bool {
        self.inner.use_uv.script_environments_enabled()
    }

    fn entry(&self, file: File) -> Arc<ScriptEnvironmentEntry> {
        // Return an owned entry so the map's shard lock is released before the caller waits
        // for synchronization. Otherwise, unrelated scripts in the same shard would also be blocked.
        Arc::clone(self.inner.by_file.entry(file).or_default().value())
    }
}

impl std::panic::RefUnwindSafe for ScriptEnvironments {}

/// Identifies when a script's environment needs to be synchronized.
///
/// Matching keys allow an existing environment or synchronization request to be reused without
/// invoking uv again. The key changes when the script's PEP 723 metadata or configured Python
/// override changes; edits elsewhere in the script leave it unchanged.
pub(crate) type ScriptEnvironmentCacheKey = u64;

/// The stable Salsa identity for a script's environment.
///
/// Like [`File`], this input can exist before the resource it represents is initialized. If
/// semantic analysis reaches a script before synchronization, [`script_environment`] creates
/// this input without invoking uv.
///
/// Later synchronization updates the same input. Keeping its identity stable ensures that Salsa
/// invalidates queries which observed the earlier
/// environment when its Python version, module search paths, or initialization error changes.
#[salsa::input(heap_size=ruff_memory_usage::heap_size)]
#[derive(Debug)]
pub(super) struct ScriptEnvironment {
    /// The cache key of the most recently completed synchronization.
    ///
    /// `None` means the environment has not been synchronized yet. Both successful and failed
    /// synchronizations store their cache key, preventing repeated uv invocations until the script
    /// metadata or Python override changes.
    #[returns(copy)]
    synchronized_cache_key: Option<ScriptEnvironmentCacheKey>,

    /// The environment metadata returned by the most recent successful synchronization.
    ///
    /// `None` means the environment has not been synchronized or synchronization failed.
    /// [`initialization_error`](Self::initialization_error) distinguishes those cases.
    #[returns(as_ref)]
    pub(super) uv_metadata: Option<UvMetadata>,

    /// The error from the most recent synchronization.
    ///
    /// `None` if synchronization has not completed or completed successfully.
    #[returns(as_deref)]
    pub(super) initialization_error: Option<Box<str>>,
}

#[derive(Default)]
struct ScriptEnvironmentsInner {
    use_uv: UseUv,
    by_file: FxDashMap<File, Arc<ScriptEnvironmentEntry>>,
    sync_service: UvSyncService,
}

#[derive(Default)]
struct ScriptEnvironmentEntry {
    state: Mutex<ScriptEnvironmentState>,

    /// Wakes callers when a blocking synchronization finishes or is cancelled.
    ///
    /// A cancelled synchronization creates no [`ScriptEnvironment`], allowing a waiting caller
    /// to retry.
    initialized: Condvar,
}

impl ScriptEnvironmentEntry {
    /// Waits for a blocking synchronization to finish or be cancelled.
    ///
    /// Database updates cancel running operations and wait for them to finish before modifying
    /// Salsa inputs. Periodically checking for cancellation allows a waiting caller to unwind;
    /// otherwise, the update could wait indefinitely for an operation blocked on this condition.
    ///
    /// Also yields to Rayon while waiting so its worker thread can run other queued work instead
    /// of remaining idle until synchronization completes.
    fn wait_until_initialized<'entry>(
        &'entry self,
        db: &dyn Db,
        mut state: MutexGuard<'entry, ScriptEnvironmentState>,
    ) -> MutexGuard<'entry, ScriptEnvironmentState> {
        loop {
            // A database update cannot proceed until this cancelled operation unwinds.
            db.unwind_if_revision_cancelled();
            if !matches!(*state, ScriptEnvironmentState::InitializingBlocking { .. }) {
                return state;
            }

            if self
                .initialized
                .wait_for(&mut state, CANCELLATION_CHECK_INTERVAL)
                .timed_out()
            {
                db.unwind_if_revision_cancelled();
                drop(state);

                // Let Rayon run other queued work while synchronization is pending. Release the
                // state lock first because that work may need the same entry to make progress.
                rayon::yield_now();
                state = self.state.lock();
            }
        }
    }
}

/// The synchronization state of one script environment.
///
/// Ensures that at most one blocking synchronization runs for a script at a time.
#[derive(Default)]
enum ScriptEnvironmentState {
    /// No [`ScriptEnvironment`] exists and no synchronization is running.
    ///
    /// This is the initial state. A cancelled blocking synchronization also restores this state,
    /// allowing another caller to initialize the environment.
    #[default]
    Vacant,

    /// A caller is waiting for uv to initialize the script's environment.
    ///
    /// The caller waits for uv, creates the [`ScriptEnvironment`] input from its result, and wakes
    /// any other callers waiting for the same script.
    InitializingBlocking {
        /// Identifies the script metadata and Python override passed to uv.
        cache_key: ScriptEnvironmentCacheKey,
    },

    /// The last synchronized environment, or the default environment before synchronization.
    ///
    /// A completed synchronization stores its metadata and any initialization error in the
    /// [`ScriptEnvironment`] input. If semantic analysis reaches a script before synchronization
    /// has been requested, [`script_environment`] creates an input without uv metadata instead.
    ///
    /// The environment does not necessarily match the latest script metadata.
    Current { environment: ScriptEnvironment },
}

/// Ensures a blocking synchronization always releases its claim on the script.
///
/// The synchronization runs without holding the entry's state lock. On completion, this guard
/// stores the new [`ScriptEnvironment`] and wakes waiting callers.
///
/// If the caller is cancelled or unwinds before completing synchronization, dropping the guard
/// restores the entry to `Vacant` and wakes waiting callers so another caller can retry.
#[must_use]
struct InitializationClaim<'entry>(Option<&'entry ScriptEnvironmentEntry>);

impl<'entry> InitializationClaim<'entry> {
    fn new(
        entry: &'entry ScriptEnvironmentEntry,
        mut state: MutexGuard<'entry, ScriptEnvironmentState>,
        cache_key: ScriptEnvironmentCacheKey,
    ) -> Self {
        *state = ScriptEnvironmentState::InitializingBlocking { cache_key };
        Self(Some(entry))
    }

    fn complete(mut self, environment: ScriptEnvironment) {
        self.finish(ScriptEnvironmentState::Current { environment });
    }

    fn finish(&mut self, next: ScriptEnvironmentState) {
        if let Some(entry) = self.0.take() {
            let mut state = entry.state.lock();
            debug_assert!(matches!(
                *state,
                ScriptEnvironmentState::InitializingBlocking { .. }
            ));
            *state = next;
            drop(state);
            entry.initialized.notify_all();
        }
    }
}

impl Drop for InitializationClaim<'_> {
    fn drop(&mut self) {
        self.finish(ScriptEnvironmentState::Vacant);
    }
}

fn parse_metadata_output(
    db: &dyn Db,
    output: std::io::Result<std::process::Output>,
) -> (Option<UvMetadata>, Option<Box<str>>) {
    match Uv::parse_metadata_output(db.system(), output) {
        Ok(metadata) => (Some(metadata), None),
        Err(error) => (None, Some(error.to_string().into_boxed_str())),
    }
}

fn script_sync_task(db: &dyn Db, file: File) -> Option<ScriptSyncTask> {
    let path = file.path(db).as_system_path()?;
    let tag = script_tag(db, file)?;
    let python = script_python(db);

    // Hash the metadata text directly to avoid parsing it solely to compute the cache key.
    // Formatting-only metadata changes may therefore trigger an unnecessary synchronization.
    let mut hasher = CacheKeyHasher::new();
    tag.metadata().cache_key(&mut hasher);
    python.cache_key(&mut hasher);

    Some(ScriptSyncTask::new(
        file,
        path.to_path_buf(),
        python,
        hasher.finish(),
    ))
}

fn script_python(db: &dyn Db) -> Option<SystemPathBuf> {
    let metadata = db.project().metadata(db);

    metadata
        .override_options()
        .and_then(|options| options.environment.as_ref())
        .and_then(|environment| environment.python.as_ref())
        .map(|python| python.absolute(metadata.root(), db.system()))
}
