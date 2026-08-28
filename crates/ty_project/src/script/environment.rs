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
//! CLI watch mode also schedules synchronization in the background after filesystem changes, but
//! delays the next check until those synchronizations have completed. Scripts discovered during
//! the subsequent check are initialized lazily. Repeated changes to a script are combined so only
//! the latest requested synchronization runs after the current one.
//!
//! Each script's virtual environment is represented by a stable [`ScriptEnvironment`] Salsa input.
//! Updating that input invalidates semantic queries that depend on the script's Python version or
//! module search paths, ensuring that checks are rerun after synchronization.

use std::hash::Hasher;
use std::sync::Arc;
use std::time::Duration;

use crossbeam::channel::Receiver;
use parking_lot::{Condvar, Mutex, MutexGuard};
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_db::FxDashMap;
use ruff_db::files::{File, Files};
use ruff_db::system::SystemPathBuf;
use salsa::Setter;

use super::script_tag;
use crate::uv::{
    ScriptSyncRequest, ScriptSyncResult, ScriptSyncTask, Uv, UvMetadata, UvSyncService,
};
use crate::{Db, ProgressReporter, ScriptSyncProgress, UseUv};

const CANCELLATION_CHECK_INTERVAL: Duration = Duration::from_millis(1);

type ProgressFactory<'factory> =
    dyn Fn(&dyn Db, File) -> Option<Box<dyn ScriptSyncProgress>> + 'factory;

/// Returns the Salsa input representing `file`'s script environment.
///
/// The CLI and language server normally synchronize the script's virtual environment before it
/// is needed. This function never invokes uv; it returns the [`ScriptEnvironment`] input so
/// semantic queries can depend on it.
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

    /// Returns a receiver for background synchronization wakeups.
    ///
    /// A wakeup indicates that synchronization results may be ready to process with
    /// [`poll_sync`](Self::poll_sync). Wakeups are coalesced, so one signal can represent
    /// multiple completed synchronizations.
    ///
    /// The CLI and language-server main loops wait on this receiver alongside their other events.
    /// When signaled, they call [`poll_sync`](Self::poll_sync) to update script environments and
    /// recheck the affected files.
    pub fn sync_wakeups(&self) -> Receiver<()> {
        self.inner.sync_wakeups.clone()
    }

    /// Initializes `file`'s environment before checking it.
    ///
    /// Discovering scripts requires reading file contents, so initializing every environment before
    /// a project check would require eagerly inspecting every candidate file. After a directory
    /// change, it could also require traversing the directory and reading each file before applying
    /// the change, increasing CLI and language-server latency.
    ///
    /// Instead, project checks initialize virtual environments lazily as they encounter scripts.
    /// If no [`ScriptEnvironment`] exists, this method synchronizes the virtual environment and
    /// waits for uv before creating the input. An existing [`ScriptEnvironment`] is reused.
    ///
    /// Background synchronization works differently: it creates the [`ScriptEnvironment`] input
    /// before invoking uv so other operations can use it while synchronization runs. Applying the
    /// result must therefore update an existing Salsa input. Before allowing that update, Salsa
    /// cancels active checks and waits for them to finish. This method cannot wait for the update
    /// because the current check must finish before the update can proceed.
    ///
    /// Returns [`Pending`] if no usable environment exists yet, allowing callers to decide whether
    /// to proceed. For example, a caller can skip diagnostics that would be incorrect without the
    /// script's dependencies. Applying the synchronization result schedules another check once the
    /// environment becomes available.
    ///
    /// [`Pending`]: ScriptEnvironmentAvailability::Pending
    pub(crate) fn initialize_blocking(
        &self,
        db: &dyn Db,
        file: File,
        reporter: &dyn ProgressReporter,
    ) -> ScriptEnvironmentAvailability {
        if !self.is_enabled() {
            return ScriptEnvironmentAvailability::Available;
        }

        let Some(task) = script_sync_task(db, file) else {
            return ScriptEnvironmentAvailability::Available;
        };
        let entry = self.entry(file);

        let mut state = entry.state.lock();
        loop {
            match &*state {
                ScriptEnvironmentState::Current { .. } => {
                    return ScriptEnvironmentAvailability::Available;
                }

                ScriptEnvironmentState::SynchronizingInBackground { availability, .. } => {
                    return *availability;
                }

                // Another caller is synchronizing the virtual environment and will create its
                // `ScriptEnvironment` input when uv finishes.
                // Wait for that caller instead of starting a second synchronization.
                ScriptEnvironmentState::InitializingBlocking { .. } => {
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
                        let _progress = reporter.for_script(db, file);
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
                    return ScriptEnvironmentAvailability::Available;
                }
            }
        }
    }

    /// Returns whether `file`'s environment is [`Pending`](ScriptEnvironmentAvailability::Pending).
    pub(crate) fn is_initialization_pending(&self, db: &dyn Db, file: File) -> bool {
        if !self.is_enabled() || script_tag(db, file).is_none() {
            return false;
        }

        self.existing_entry(file)
            .is_some_and(|entry| entry.state.lock().availability().is_pending())
    }

    /// Requests background synchronization for `file`'s environment.
    ///
    /// If this call creates the script's first `ScriptEnvironment`, `availability` determines
    /// whether it can be used while synchronization runs. An existing `ScriptEnvironment`
    /// remains available, even if its virtual environment has not previously been synchronized.
    ///
    /// If another synchronization is already running, records the latest request to run afterward
    /// and reuses the existing progress indicator. Otherwise, creates a new progress indicator and
    /// submits the synchronization.
    ///
    /// Blocks while the worker queue is full, applying backpressure to the caller.
    pub fn request_sync(
        &self,
        db: &mut dyn Db,
        file: File,
        availability: ScriptEnvironmentAvailability,
        make_progress: &ProgressFactory<'_>,
    ) {
        if !self.is_enabled() {
            return;
        }

        let Some(task) = script_sync_task(db, file) else {
            return;
        };
        let entry: Arc<ScriptEnvironmentEntry> = self.entry(file);
        let mut state = entry.state.lock();

        let (environment, availability) = match &mut *state {
            ScriptEnvironmentState::InitializingBlocking { cache_key: running } => {
                // Both requests must observe the same script metadata and Python override. Changing
                // either requires a Salsa update, which first cancels the blocking initialization
                // and waits for it to finish. A different cache key therefore cannot be observed
                // while the blocking initialization is still running.
                assert_eq!(
                    *running,
                    task.request.cache_key(),
                    "concurrent initializations must use the same script environment cache key"
                );
                tracing::trace!(
                    "Script environment synchronization for `{}` is already running",
                    task.request.path()
                );
                return;
            }
            ScriptEnvironmentState::Vacant => {
                (ScriptEnvironment::new(db, None, None, None), availability)
            }
            ScriptEnvironmentState::Current { environment } => {
                let synchronized_cache_key = environment.synchronized_cache_key(db);
                let already_synchronized = synchronized_cache_key == Some(task.request.cache_key());

                if already_synchronized {
                    tracing::trace!(
                        "Script environment for `{}` is already synchronized",
                        task.request.path()
                    );
                    return;
                }

                (*environment, ScriptEnvironmentAvailability::Available)
            }
            ScriptEnvironmentState::SynchronizingInBackground { sync, .. } => {
                if !sync.update_next_request(task.request.clone()) {
                    tracing::trace!(
                        "Script environment synchronization for `{}` is already requested",
                        task.request.path()
                    );
                } else {
                    tracing::debug!(
                        "Updated pending script environment synchronization for `{}`",
                        task.request.path()
                    );
                }

                return;
            }
        };

        let progress = make_progress(db, file);
        *state = ScriptEnvironmentState::SynchronizingInBackground {
            environment,
            availability,
            sync: InFlightSync {
                active_cache_key: task.request.cache_key(),
                next_request: None,
            },
        };

        tracing::debug!(
            "Requested script environment synchronization for `{}`",
            task.request.path()
        );

        // Scheduling can block while the worker queue is full; release the entry lock first.
        drop(state);

        self.inner
            .sync_service
            .schedule_one(db.system(), task, progress);
    }

    /// Processes completed background synchronizations and updates their script environments.
    ///
    /// Background workers cannot apply their results because updating an existing Salsa input
    /// requires mutable access to the database. The CLI and language-server main loops call this
    /// method after receiving a [`sync_wakeups`](Self::sync_wakeups) notification.
    ///
    /// If a newer synchronization was requested while the current one was running, discards the
    /// outdated result and schedules the newer request instead, transferring the existing progress
    /// indicator. Scheduling can block while the worker queue is full.
    ///
    /// Returns the files whose environments were updated so callers can recheck them.
    pub fn poll_sync(&self, db: &mut dyn Db) -> Vec<File> {
        // Updating a Salsa input waits for outstanding snapshots to be dropped. Cancel
        // them before taking an entry lock, which their queries may need to finish.
        db.trigger_cancellation();
        let mut changed_files = Vec::new();

        while let Ok(result) = self.inner.sync_results.try_recv() {
            let ScriptSyncResult {
                task,
                output,
                progress,
            } = result;
            let file = task.file;
            let request = task.request;
            let Some(entry) = self.existing_entry(file) else {
                panic!(
                    "received a synchronization result for unknown script `{}`",
                    request.path(),
                );
            };

            let mut state = entry.state.lock();
            let ScriptEnvironmentState::SynchronizingInBackground {
                environment, sync, ..
            } = &mut *state
            else {
                panic!(
                    "synchronization result for `{}` does not match any task currently in flight",
                    request.path(),
                );
            };
            assert_eq!(
                sync.active_cache_key,
                request.cache_key(),
                "synchronization result for `{}` does not match the task currently in flight",
                request.path()
            );

            if let Some(next) = sync.next_request.take() {
                // uv updates the same environment on disk for every version of this script. If the
                // metadata changes A -> B -> A, the B synchronization may already have modified the
                // environment. Run A again even though its cache key matches the last completed
                // synchronization.
                sync.active_cache_key = next.cache_key();

                tracing::debug!(
                    "Discarded superseded script environment synchronization result for `{}`",
                    request.path()
                );

                // Scheduling can block while the worker queue is full; release the entry lock first.
                drop(state);

                self.inner.sync_service.schedule_one(
                    db.system(),
                    ScriptSyncTask {
                        file,
                        request: next,
                    },
                    progress,
                );
                continue;
            }

            let environment = *environment;
            apply_sync_result(db, environment, &request, output);
            *state = ScriptEnvironmentState::Current { environment };
            changed_files.push(file);
        }

        changed_files
    }

    /// Returns whether any script environment is being initialized or synchronized.
    ///
    /// Can be used to delay checking until all requested synchronizations have completed.
    pub fn has_pending_synchronizations(&self) -> bool {
        self.inner.by_file.iter().any(|entry| {
            matches!(
                *entry.state.lock(),
                ScriptEnvironmentState::InitializingBlocking { .. }
                    | ScriptEnvironmentState::SynchronizingInBackground { .. }
            )
        })
    }

    /// Returns every file for which a `ScriptEnvironment` input has been created.
    pub fn files(&self) -> Vec<File> {
        self.inner
            .by_file
            .iter()
            .filter_map(|entry| entry.state.lock().environment().map(|_| *entry.key()))
            .collect()
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
                ScriptEnvironmentState::Current { environment }
                | ScriptEnvironmentState::SynchronizingInBackground { environment, .. } => {
                    return Some(environment);
                }
            }
        }
    }

    fn is_enabled(&self) -> bool {
        self.inner.use_uv.script_environments_enabled()
    }

    fn existing_entry(&self, file: File) -> Option<Arc<ScriptEnvironmentEntry>> {
        let entry = self.inner.by_file.get(&file)?;
        Some(Arc::clone(entry.value()))
    }

    fn entry(&self, file: File) -> Arc<ScriptEnvironmentEntry> {
        // Return an owned entry so the map's shard lock is released before the caller waits
        // for synchronization. Otherwise, unrelated scripts in the same shard would also be blocked.
        Arc::clone(self.inner.by_file.entry(file).or_default().value())
    }
}

impl std::panic::RefUnwindSafe for ScriptEnvironments {}

/// Whether a script environment is suitable for operations that depend on its dependencies.
///
/// When opening a script, the initial environment lacks its declared dependencies and can produce
/// incorrect results. Operations affected by those dependencies, such as semantic diagnostics,
/// should be deferred. Operations such as semantic tokens can continue.
///
/// During a resync, the previous environment remains a reasonable approximation. If an unsaved edit
/// turns an ordinary file into a script, its default environment is also used: it already reflects
/// the script's settings, and synchronization is deferred until the file is saved.
#[derive(Clone, Copy)]
pub enum ScriptEnvironmentAvailability {
    /// Operations that require the script's dependencies should be deferred.
    Pending,

    /// The default or previously synchronized environment can be used.
    Available,
}

impl ScriptEnvironmentAvailability {
    pub(crate) const fn is_pending(self) -> bool {
        matches!(self, Self::Pending)
    }
}

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
/// The CLI or language server later synchronizes the script and updates the same input. Keeping
/// its identity stable ensures that Salsa invalidates queries which observed the earlier
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

struct ScriptEnvironmentsInner {
    use_uv: UseUv,
    by_file: FxDashMap<File, Arc<ScriptEnvironmentEntry>>,
    sync_service: UvSyncService,
    sync_results: Receiver<ScriptSyncResult>,
    sync_wakeups: Receiver<()>,
}

impl Default for ScriptEnvironmentsInner {
    fn default() -> Self {
        let (results_sender, sync_results) = crossbeam::channel::unbounded();
        let (wake_sender, sync_wakeups) = crossbeam::channel::bounded(1);
        Self {
            use_uv: UseUv::default(),
            by_file: FxDashMap::default(),
            sync_service: UvSyncService::new(results_sender, wake_sender),
            sync_results,
            sync_wakeups,
        }
    }
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
    /// Does not run other Rayon tasks while waiting: a nested task could depend on an
    /// initialization suspended on the same worker's stack.
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

            self.initialized
                .wait_for(&mut state, CANCELLATION_CHECK_INTERVAL);
        }
    }
}

/// The synchronization state of one script environment.
///
/// Distinguishes blocking initialization from background synchronization because they create
/// and update Salsa inputs differently. Also ensures that at most one synchronization runs for
/// a script at a time.
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
    /// Unlike background synchronization, this does not create a [`ScriptEnvironment`] in advance.
    /// The caller waits for uv, creates the input from its result, and wakes any other callers
    /// waiting for the same script.
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
    /// For example, when an unsaved edit adds script metadata to a file, synchronization is
    /// deferred until the file is saved. Checking continues with the default environment so
    /// existing diagnostics do not disappear in the meantime.
    ///
    /// The environment does not necessarily match the latest script metadata. The CLI or language
    /// server is responsible for requesting synchronization when an updated environment is needed.
    Current { environment: ScriptEnvironment },

    /// A background synchronization is initializing or updating the script's virtual environment.
    ///
    /// Unlike blocking initialization, a [`ScriptEnvironment`] input already exists while uv runs.
    /// If no input exists yet, one is created before synchronization starts so semantic queries
    /// have a stable Salsa identity to depend on.
    ///
    /// The background worker cannot update the [`ScriptEnvironment`] because modifying a Salsa
    /// input requires mutable access to the database. Instead, the CLI or language-server main loop
    /// updates it when synchronization finishes.
    SynchronizingInBackground {
        /// The [`ScriptEnvironment`] input that will receive the synchronization result.
        environment: ScriptEnvironment,

        availability: ScriptEnvironmentAvailability,

        /// The active synchronization and the next request, if any.
        sync: InFlightSync,
    },
}

impl ScriptEnvironmentState {
    fn availability(&self) -> ScriptEnvironmentAvailability {
        match self {
            Self::InitializingBlocking { .. } => ScriptEnvironmentAvailability::Pending,
            Self::Vacant | Self::Current { .. } => ScriptEnvironmentAvailability::Available,
            Self::SynchronizingInBackground { availability, .. } => *availability,
        }
    }

    fn environment(&self) -> Option<ScriptEnvironment> {
        match self {
            Self::Current { environment, .. }
            | Self::SynchronizingInBackground { environment, .. } => Some(*environment),
            Self::Vacant | Self::InitializingBlocking { .. } => None,
        }
    }
}

/// An active synchronization and the latest request to run after it.
///
/// At most one additional request is retained. If the script changes repeatedly while uv is
/// running, newer requests replace the pending request instead of accumulating in a queue.
struct InFlightSync {
    active_cache_key: ScriptEnvironmentCacheKey,
    next_request: Option<ScriptSyncRequest>,
}

impl InFlightSync {
    /// Updates the synchronization to run after the active request.
    ///
    /// If `request` matches the active synchronization, removes any previously requested follow-up.
    /// Otherwise, replaces the follow-up with `request`.
    ///
    /// Returns whether the requested synchronization changed.
    fn update_next_request(&mut self, request: ScriptSyncRequest) -> bool {
        let desired = self
            .next_request
            .as_ref()
            .map_or(self.active_cache_key, ScriptSyncRequest::cache_key);
        if desired == request.cache_key() {
            return false;
        }

        self.next_request = if self.active_cache_key == request.cache_key() {
            None
        } else {
            Some(request)
        };
        true
    }
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

/// Applies a completed synchronization to the existing [`ScriptEnvironment`] input.
///
/// Stores the returned metadata or initialization error and records the synchronized cache key.
/// Updating the input invalidates semantic queries that depend on the script's virtual
/// environment.
fn apply_sync_result(
    db: &mut dyn Db,
    environment: ScriptEnvironment,
    request: &ScriptSyncRequest,
    output: std::io::Result<std::process::Output>,
) {
    let previous_root = environment
        .uv_metadata(db)
        .and_then(UvMetadata::environment)
        .map(ToOwned::to_owned);
    let recovering_from_error = environment.initialization_error(db).is_some();
    let (uv_metadata, initialization_error) = parse_metadata_output(db, output);
    let current_root = uv_metadata.as_ref().and_then(UvMetadata::environment);

    if let Some(root) = previous_root
        .as_deref()
        .or_else(|| current_root.filter(|_| recovering_from_error))
    {
        // uv can install, update, or remove packages without changing the virtual-environment path.
        // Refresh files under that path so semantic queries see the updated package contents.
        // After a failed synchronization, recover the path from the new metadata because the
        // previous metadata was cleared along with its virtual-environment path.
        //
        // FIXME: This is overbroad. A file watcher can tell us precisely what changed.
        // Changes inside virtual environments should instead be watched and processed through `ProjectDatabase::apply_changes`.
        // Using a file watcher also ensures that virtual environment changes in
        // scripts without using uv are detected.
        Files::sync_all_recursive(db, [root]);
    }

    if environment.uv_metadata(db) != uv_metadata.as_ref() {
        environment.set_uv_metadata(db).to(uv_metadata);
    }

    if environment.initialization_error(db) != initialization_error.as_deref() {
        environment
            .set_initialization_error(db)
            .to(initialization_error);
    }

    let cache_key = Some(request.cache_key());
    if environment.synchronized_cache_key(db) != cache_key {
        environment.set_synchronized_cache_key(db).to(cache_key);
    }

    tracing::debug!(
        "Applied script environment synchronization result for `{}`",
        request.path()
    );
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

#[cfg(test)]
mod tests {
    use anyhow::Context;
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithWritableSystem, SystemPath};

    use super::script_environment;
    use crate::db::testing::TestDb;
    use crate::{Db as _, ProjectMetadata, UseUv};

    #[test]
    fn semantic_lookup_creates_a_stable_default_environment() -> anyhow::Result<()> {
        let root = SystemPath::new("/project").to_path_buf();
        let path = root.join("script.py");
        let metadata = ProjectMetadata::new("test", root).with_use_uv(UseUv::Scripts);
        let mut db = TestDb::new(metadata);
        db.write_file(&path, "# /// script\n# dependencies = []\n# ///\n")?;
        let file = system_path_to_file(&db, &path)?;
        let environments = db.script_environments().clone();

        // Semantic queries can reach a script before its environment has been synchronized. They
        // create one stable Salsa input with no uv metadata or initialization error.
        let environment = script_environment(&db, file).context("expected a script environment")?;
        assert_eq!(script_environment(&db, file), Some(environment));
        assert_eq!(environment.synchronized_cache_key(&db), None);
        assert_eq!(environment.uv_metadata(&db), None);
        assert_eq!(environment.initialization_error(&db), None);
        assert!(!environments.is_initialization_pending(&db, file));

        Ok(())
    }

    #[cfg(feature = "test-uv")]
    mod uv {
        use std::process::Command;
        use std::thread;
        use std::time::{Duration, Instant};

        use anyhow::Context;
        use ruff_db::files::{File, system_path_to_file};
        use ruff_db::system::{
            DbWithTestSystem, DbWithWritableSystem, OsSystem, System as _, SystemPath,
            SystemPathBuf,
        };
        use salsa::Database as _;
        use ty_static::EnvVars;

        use super::super::{ScriptEnvironmentAvailability, script_environment};
        use crate::db::testing::TestDb;
        use crate::{Db as _, ProjectMetadata, UseUv};

        #[test]
        fn initial_background_synchronization_is_pending_until_completion() -> anyhow::Result<()> {
            let mut case = UvTestCase::new(
                r#"
                # /// script
                # requires-python = ">=3.12"
                # dependencies = ["attrs==25.4.0"]
                # ///
                from attrs import define
                "#,
            )?;
            let environments = case.db.script_environments().clone();

            environments.request_sync(
                &mut case.db,
                case.file,
                ScriptEnvironmentAvailability::Pending,
                &|_, _| None,
            );
            assert!(environments.is_initialization_pending(&case.db, case.file));

            assert_eq!(case.wait_for_synchronizations()?, vec![case.file]);
            assert!(!environments.is_initialization_pending(&case.db, case.file));
            case.assert_can_import("attrs")?;

            Ok(())
        }

        #[test]
        fn existing_environment_remains_available_during_background_synchronization()
        -> anyhow::Result<()> {
            let mut case = UvTestCase::new(
                r#"
                # /// script
                # requires-python = ">=3.12"
                # dependencies = ["attrs==25.4.0"]
                # ///
                from attrs import define
                "#,
            )?;
            let environments = case.db.script_environments().clone();
            let environment = script_environment(&case.db, case.file)
                .context("expected a default script environment")?;

            environments.request_sync(
                &mut case.db,
                case.file,
                ScriptEnvironmentAvailability::Pending,
                &|_, _| None,
            );
            assert_eq!(script_environment(&case.db, case.file), Some(environment));
            assert!(!environments.is_initialization_pending(&case.db, case.file));

            assert_eq!(case.wait_for_synchronizations()?, vec![case.file]);
            assert_eq!(script_environment(&case.db, case.file), Some(environment));
            case.assert_can_import("attrs")?;

            Ok(())
        }

        #[test]
        fn returning_to_previous_metadata_resynchronizes_the_environment() -> anyhow::Result<()> {
            let initial = r#"
            # /// script
            # requires-python = ">=3.12"
            # dependencies = ["attrs==25.4.0"]
            # ///
            from attrs import define
            "#;
            let mut case = UvTestCase::new(initial)?;
            let environments = case.db.script_environments().clone();
            environments.request_sync(
                &mut case.db,
                case.file,
                ScriptEnvironmentAvailability::Pending,
                &|_, _| None,
            );
            assert_eq!(case.wait_for_synchronizations()?, vec![case.file]);
            case.assert_can_import("attrs")?;

            case.db.write_dedented(
                case.path.as_str(),
                r#"
                # /// script
                # requires-python = ">=3.12"
                # dependencies = ["anyio"]
                # ///
                from attrs import define
                "#,
            )?;
            let wakeups = environments.sync_wakeups();
            environments.request_sync(
                &mut case.db,
                case.file,
                ScriptEnvironmentAvailability::Pending,
                &|_, _| None,
            );

            // Wait until uv has changed the virtual environment, but leave its result unapplied.
            wakeups
                .recv_timeout(Duration::from_secs(30))
                .context("intermediate script synchronization did not finish")?;

            // Restoring the original metadata must reinstall its dependencies, even though its
            // cache key matches the last synchronization whose result was applied.
            case.db.write_dedented(case.path.as_str(), initial)?;
            environments.request_sync(
                &mut case.db,
                case.file,
                ScriptEnvironmentAvailability::Pending,
                &|_, _| None,
            );

            let mut changed = environments.poll_sync(&mut case.db);
            changed.extend(case.wait_for_synchronizations()?);
            assert_eq!(changed, vec![case.file]);
            case.assert_can_import("attrs")?;

            Ok(())
        }

        #[test]
        fn background_result_cancels_snapshots_before_locking_entry() -> anyhow::Result<()> {
            let mut case = UvTestCase::new(
                r#"
                # /// script
                # requires-python = ">=3.12"
                # dependencies = []
                # ///
                "#,
            )?;
            let environments = case.db.script_environments().clone();
            environments.request_sync(
                &mut case.db,
                case.file,
                ScriptEnvironmentAvailability::Pending,
                &|_, _| None,
            );
            environments
                .sync_wakeups()
                .recv_timeout(Duration::from_secs(30))
                .context("script synchronization did not finish")?;

            let entry = environments
                .existing_entry(case.file)
                .context("expected a script environment entry")?;
            let snapshot = case.db.clone();
            let reader = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(5);
                while salsa::Cancelled::catch(|| snapshot.unwind_if_revision_cancelled()).is_ok() {
                    assert!(Instant::now() < deadline, "snapshot was not cancelled");
                    thread::sleep(Duration::from_millis(1));
                }

                // A cancelled query may need this lock before it can drop its snapshot. Use a
                // timeout so a regression fails instead of deadlocking the test itself.
                assert!(
                    entry.state.try_lock_for(Duration::from_secs(1)).is_some(),
                    "the entry lock was held while waiting for a cancelled snapshot"
                );
                drop(snapshot);
            });

            assert_eq!(environments.poll_sync(&mut case.db), vec![case.file]);
            reader
                .join()
                .map_err(|_| anyhow::anyhow!("reader panicked"))?;
            Ok(())
        }

        struct UvTestCase {
            _temp_dir: tempfile::TempDir,
            db: TestDb,
            file: File,
            path: SystemPathBuf,
        }

        impl UvTestCase {
            fn new(source: &str) -> anyhow::Result<Self> {
                let temp_dir = tempfile::tempdir()?;
                let root = SystemPath::from_std_path(temp_dir.path())
                    .context("temporary directory is not a valid UTF-8 path")?
                    .to_path_buf();
                let metadata =
                    ProjectMetadata::new("test", root.clone()).with_use_uv(UseUv::Scripts);
                let mut db = TestDb::new(metadata);
                db.use_system(OsSystem::new(&root));

                let uv = OsSystem::default().which("uv")?;
                db.test_system().set_env_var(EnvVars::UV, uv.as_str());
                for name in [
                    EnvVars::VIRTUAL_ENV,
                    EnvVars::CONDA_PREFIX,
                    EnvVars::CONDA_DEFAULT_ENV,
                    EnvVars::CONDA_ROOT,
                    EnvVars::PYTHONPATH,
                ] {
                    db.test_system().remove_env_var(name);
                }

                let path = root.join("script.py");
                db.write_dedented(path.as_str(), source)?;
                let file = system_path_to_file(&db, &path)?;

                Ok(Self {
                    _temp_dir: temp_dir,
                    db,
                    file,
                    path,
                })
            }

            fn wait_for_synchronizations(&mut self) -> anyhow::Result<Vec<File>> {
                let environments = self.db.script_environments().clone();
                let wakeups = environments.sync_wakeups();
                let mut changed = Vec::new();

                while environments.has_pending_synchronizations() {
                    wakeups
                        .recv_timeout(Duration::from_secs(30))
                        .context("script synchronization did not finish")?;
                    changed.extend(environments.poll_sync(&mut self.db));
                }

                Ok(changed)
            }

            fn assert_can_import(&self, module: &str) -> anyhow::Result<()> {
                let environment = script_environment(&self.db, self.file)
                    .context("expected a script environment")?;
                let metadata = environment.uv_metadata(&self.db).with_context(|| {
                    format!(
                        "script synchronization did not produce uv metadata: {:?}",
                        environment.initialization_error(&self.db)
                    )
                })?;
                let root = metadata
                    .environment()
                    .context("uv metadata did not include a virtual environment")?;
                let python = if cfg!(windows) {
                    root.join("Scripts/python.exe")
                } else {
                    root.join("bin/python")
                };
                let output = Command::new(python.as_std_path())
                    .args(["-c", &format!("import {module}")])
                    .output()?;

                anyhow::ensure!(
                    output.status.success(),
                    "failed to import `{module}` from the synchronized environment: {}",
                    String::from_utf8_lossy(&output.stderr)
                );

                Ok(())
            }
        }
    }
}
