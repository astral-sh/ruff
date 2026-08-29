//! Manages the Python environments used to check projects and standalone scripts.
//!
//! ty needs to know which Python version and packages are available when checking a file. For
//! project files, uv provides metadata about the workspace's environment. Standalone scripts can
//! declare their own requirements in inline metadata, so uv may need to create and synchronize
//! separate environments for them.
//!
//! The project environment is resolved during initial discovery. The file index identifies
//! standalone scripts by reading their inline metadata, but does not synchronize their environments.
//!
//! The CLI requests synchronization for indexed scripts and applies the results before checking.
//! Checks use the environment available when they run; they never invoke uv. Waiting before the
//! check is necessary because the script's dependencies and Python version must be known to
//! produce accurate diagnostics.
//!
//! The language server requests synchronization for indexed scripts when opening a project,
//! including scripts that are not open in the editor. It also requests synchronization when
//! scripts are discovered, opened or saved, or when a file-watcher event reports a change to a
//! closed script. Project metadata is refreshed when configuration changes.
//!
//! These operations run in the background because synchronizing scripts can create environments
//! and install packages. Waiting for them would increase the latency of other editor requests;
//! for example, semantic tokens should remain available while synchronization is running.
//! Scripts are discovered independently of diagnostics because workspace symbols and references
//! also need their environments.
//! This avoids a rust-analyzer-like experience where editor operations wait for `cargo check` to
//! complete before becoming available.
//!
//! While a script's initial environment is unavailable, the language server defers its semantic
//! diagnostics to avoid incorrect missing-dependency errors. Document pull requests receive an
//! empty diagnostic report, while workspace diagnostic requests are suspended. After applying the
//! synchronization result, the server resumes suspended requests and refreshes diagnostics.
//!
//! Refreshing an available environment does not defer diagnostics. Projects and scripts continue
//! using their most recently applied environments until the refresh results are applied.
//!
//! uv reads files from disk, not from the editor. If a user adds a script metadata block to an
//! open file, ty does not request synchronization until the file is saved. It keeps checking the
//! file using ty's settings, so existing diagnostics stay visible. Once the file is saved and uv
//! finishes synchronization, ty checks it again using the environment returned by uv.
//!
//! CLI watch mode also schedules requests in the background after filesystem changes, but delays
//! the next check until those requests have completed. Repeated changes to a project or script
//! are combined so only the latest requested update runs after the current one.
//!
//! The main loop applies project metadata by rediscovering the existing project, including when uv
//! fails. Each script's virtual environment is represented by a stable [`ScriptEnvironment`] Salsa
//! input. Updating these inputs invalidates semantic queries that depend on the Python version or
//! module search paths, ensuring that checks are rerun after synchronization.
//!
//! Applying an update cancels active queries and waits for their database snapshots to be dropped.
//! A query waiting for an existing environment input to be updated would therefore prevent that
//! update. Semantic queries therefore use the available environment without waiting for
//! synchronization. The host applies results through [`UvEnvironments::poll_sync`].
//!
//! # Scheduling and capacity
//!
//! [`UvEnvironments::request_sync`] owns request construction, coalescing, and submission in one
//! call. The request and result queues have no fixed capacity, so submission does not wait for uv.
//! Each project or script has at most one job queued, running, or awaiting result processing. A
//! newer request replaces the latest follow-up rather than adding another job. Queue lengths are
//! therefore bounded by the number of projects and scripts, not the number of changes. The worker
//! count limits concurrent uv processes, but the queues do not apply backpressure to the host.
//!
//! A newer request cancels the previous job if a worker has not started it yet. Running uv processes
//! are allowed to finish. Both cancelled and executed jobs report completion; only then can
//! `poll_sync` submit the latest follow-up, keeping the same progress reporter. This avoids both
//! overlapping synchronizations for one environment and accumulating cancelled queue entries.

use std::hash::Hasher;
use std::sync::Arc;

use crossbeam::channel::Receiver;
use parking_lot::Mutex;
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_db::FxDashMap;
use ruff_db::cancellation::CancellationTokenSource;
use ruff_db::files::{File, Files};
use ruff_db::system::{SystemPath, SystemPathBuf};
use salsa::Setter;

use crate::script::script_tag;
use crate::uv::{
    ScriptSyncRequest, ScriptSyncTask, Uv, UvMetadata, UvMetadataResult, UvMetadataService,
    UvSyncTask,
};
use crate::{Db, ProjectReloadResult, ProjectSyncProgressFactory, UseUv, UvSyncProgress};

type ProgressFactory<'factory> =
    dyn Fn(&dyn Db, File) -> Option<Box<dyn UvSyncProgress>> + 'factory;

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
/// Returns `None` if script integration is disabled or the script is not an actual file on disk.
pub(crate) fn script_environment(db: &dyn Db, file: File) -> Option<ScriptEnvironment> {
    db.uv_environments().environment(db, file)
}

/// Coordinates project and PEP 723 script environments using `uv metadata`.
#[derive(Clone, Default)]
pub struct UvEnvironments {
    inner: Arc<UvEnvironmentsInner>,
}

impl UvEnvironments {
    pub(crate) fn new(use_uv: UseUv) -> Self {
        Self {
            inner: Arc::new(UvEnvironmentsInner {
                use_uv,
                ..UvEnvironmentsInner::default()
            }),
        }
    }

    /// Requests fresh workspace metadata for project rediscovery.
    pub fn request_project_sync(
        &self,
        db: &dyn Db,
        path: &SystemPath,
        make_progress: &ProjectSyncProgressFactory<'_>,
    ) {
        let (progress, cancellation) = {
            let mut project = self.inner.project.lock();
            if let Some(sync) = project.as_mut() {
                sync.next_request = Some(path.to_path_buf());
                sync.cancellation.cancel();
                return;
            }

            let progress = make_progress(db, db.project());
            let cancellation = CancellationTokenSource::new();
            let token = cancellation.token();
            *project = Some(ProjectSync {
                next_request: None,
                cancellation,
            });
            (progress, token)
        };

        tracing::debug!("Requested workspace metadata for `{path}`");
        self.inner.sync_service.schedule_one(
            db.system(),
            UvSyncTask::Workspace(path.to_path_buf()),
            cancellation,
            progress,
        );
    }

    /// Returns a receiver for background synchronization wakeups.
    ///
    /// A wakeup indicates that synchronization results may be ready to process with
    /// [`poll_sync`](Self::poll_sync). Wakeups are coalesced, so one signal can represent
    /// multiple completed synchronizations.
    ///
    /// The CLI and language-server main loops wait on this receiver alongside their other events.
    /// When signaled, they call [`poll_sync`](Self::poll_sync) to apply project and script results
    /// and refresh the affected diagnostics.
    pub fn sync_wakeups(&self) -> Receiver<()> {
        self.inner.sync_wakeups.clone()
    }

    /// Returns whether `file`'s environment is [`Pending`](ScriptEnvironmentAvailability::Pending).
    ///
    /// A `false` result does not guarantee that initialization has finished. It may not have been
    /// requested yet, and another database handle can request it after this call. Callers must submit
    /// any required initial synchronization before scheduling operations that rely on this check.
    ///
    /// Refreshing an available environment does not make it pending. A pending environment stays
    /// pending until [`poll_sync`](Self::poll_sync) applies its result, even if uv has finished.
    ///
    /// Salsa does not track changes to the pending state.
    pub fn is_initialization_pending(&self, db: &dyn Db, file: File) -> bool {
        if !self.is_enabled() || script_tag(db, file).is_none() {
            return false;
        }

        self.existing_entry(file).is_some_and(|entry| {
            matches!(
                *entry.lock(),
                ScriptEnvironmentState::Synchronizing {
                    availability: ScriptEnvironmentAvailability::Pending,
                    ..
                }
            )
        })
    }

    /// Returns whether any script's initial environment is unavailable.
    ///
    /// Like [`Self::is_initialization_pending`], this only covers requested synchronizations.
    /// Refreshing an available environment does not make it unavailable again. Project
    /// environments are initialized during discovery, before background requests are scheduled.
    pub fn has_pending_initializations(&self) -> bool {
        self.inner.scripts.iter().any(|entry| {
            matches!(
                *entry.lock(),
                ScriptEnvironmentState::Synchronizing {
                    availability: ScriptEnvironmentAvailability::Pending,
                    ..
                }
            )
        })
    }

    /// Requests background synchronization for `file`'s environment.
    ///
    /// If this call creates the script's first `ScriptEnvironment`, `availability` determines
    /// whether it can be used while synchronization runs. An existing `ScriptEnvironment`
    /// remains available, even if its virtual environment has not previously been synchronized.
    ///
    /// If another synchronization is pending, records the latest request to run afterward and
    /// cancels the queued job if it has not started. Running uv processes are allowed to finish.
    /// The follow-up reuses the existing progress reporter. Otherwise, creates a new progress
    /// reporter and submits the synchronization.
    ///
    /// Submission does not wait for uv or queue space.
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
        let mut state = entry.lock();

        let (environment, availability) = match &mut *state {
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
            ScriptEnvironmentState::Synchronizing { sync, .. } => {
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
        let cancellation = CancellationTokenSource::new();
        let token = cancellation.token();
        *state = ScriptEnvironmentState::Synchronizing {
            environment,
            availability,
            sync: InFlightSync {
                active_cache_key: task.request.cache_key(),
                next_request: None,
                cancellation,
            },
        };

        tracing::debug!(
            "Requested script environment synchronization for `{}`",
            task.request.path()
        );

        drop(state);

        self.inner.sync_service.schedule_one(
            db.system(),
            UvSyncTask::Script(task),
            token,
            progress,
        );
    }

    /// Applies completed background requests to their projects or script environments.
    ///
    /// Background workers cannot apply their results because updating an existing Salsa input
    /// requires mutable access to the database. The CLI and language-server main loops call this
    /// method after receiving a [`sync_wakeups`](Self::sync_wakeups) notification.
    ///
    /// If a newer synchronization was requested while the current one was pending, discards the
    /// outdated result and schedules the newer request instead, transferring the existing progress
    /// reporter. Cancelled jobs also schedule their replacement without changing the environment.
    ///
    /// Reports project completions and changed scripts so callers can refresh diagnostics.
    pub fn poll_sync(&self, db: &mut dyn Db) -> UvSyncChanges {
        // Updating a Salsa input waits for outstanding snapshots to be dropped. Cancel
        // them before taking an entry lock, which their queries may need to finish.
        db.trigger_cancellation();
        let mut changes = UvSyncChanges::default();

        while let Ok(result) = self.inner.sync_results.try_recv() {
            let UvMetadataResult {
                task,
                output,
                progress,
            } = result;
            match task {
                UvSyncTask::Workspace(path) => {
                    let mut project_sync = self.inner.project.lock();
                    let next = project_sync
                        .as_mut()
                        .and_then(|sync| sync.next_request.take());
                    let output = match (next, output) {
                        (None, Some(output)) => output,
                        (next, _) => {
                            tracing::debug!("Discarded superseded workspace metadata for `{path}`");
                            let cancellation = CancellationTokenSource::new();
                            let token = cancellation.token();
                            *project_sync = Some(ProjectSync {
                                next_request: None,
                                cancellation,
                            });
                            drop(project_sync);

                            self.inner.sync_service.schedule_one(
                                db.system(),
                                UvSyncTask::Workspace(next.unwrap_or(path)),
                                token,
                                progress,
                            );
                            continue;
                        }
                    };
                    drop(project_sync);
                    let project = db.project();
                    let environment = match Uv::parse_metadata_output(db.system(), output) {
                        Ok(metadata) => ProjectEnvironment {
                            metadata: Some(metadata),
                            error: None,
                        },
                        // Keep the last working uv metadata so a failed refresh does not change
                        // the environment used for checking. Report the new error instead.
                        Err(error) => ProjectEnvironment {
                            error: Some(error.to_string().into_boxed_str()),
                            ..project.metadata(db).environment().clone()
                        },
                    };
                    changes.project = Some(match project.rediscover(db, &path, environment) {
                        Ok(result) => result,
                        Err(error) => {
                            let error = anyhow::Error::new(error);
                            tracing::error!(
                                "Failed to load project, keeping old project configuration: {error:#}"
                            );
                            ProjectReloadResult::Unchanged
                        }
                    });
                    *self.inner.project.lock() = None;
                }
                UvSyncTask::Script(task) => {
                    let file = task.file;
                    let request = task.request;
                    let Some(entry) = self.existing_entry(file) else {
                        panic!(
                            "received a synchronization result for unknown script `{}`",
                            request.path(),
                        );
                    };

                    let mut state = entry.lock();
                    let ScriptEnvironmentState::Synchronizing {
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

                    let output = match (sync.next_request.take(), output) {
                        (None, Some(output)) => output,
                        (next, _) => {
                            // uv updates the same environment on disk for every version of this script. If the
                            // metadata changes A -> B -> A, the B synchronization may already have modified the
                            // environment. Run A again even though its cache key matches the last completed
                            // synchronization.
                            //
                            // A cancelled request can become current again after another edit. Retry it
                            // when there is no newer request; cancellation does not update the environment.
                            let next = next.unwrap_or(request);
                            sync.active_cache_key = next.cache_key();
                            sync.cancellation = CancellationTokenSource::new();
                            let token = sync.cancellation.token();

                            tracing::debug!(
                                "Discarded superseded script environment synchronization result for `{}`",
                                next.path()
                            );

                            drop(state);

                            self.inner.sync_service.schedule_one(
                                db.system(),
                                UvSyncTask::Script(ScriptSyncTask {
                                    file,
                                    request: next,
                                }),
                                token,
                                progress,
                            );
                            continue;
                        }
                    };

                    let environment = *environment;
                    apply_sync_result(db, environment, &request, output);
                    *state = ScriptEnvironmentState::Current { environment };
                    changes.scripts.push(file);
                }
            }

            if let Some(progress) = progress {
                progress.completed();
            }
        }

        changes
    }

    /// Returns whether any project or script synchronization is pending.
    ///
    /// A request stays pending until [`poll_sync`](Self::poll_sync) applies its result, even if uv
    /// has already finished.
    ///
    /// The result reflects the current state. A new synchronization can be requested after this
    /// method returns.
    pub fn has_pending_synchronizations(&self) -> bool {
        self.inner.project.lock().is_some()
            || self
                .inner
                .scripts
                .iter()
                .any(|entry| matches!(*entry.lock(), ScriptEnvironmentState::Synchronizing { .. }))
    }

    fn environment(&self, db: &dyn Db, file: File) -> Option<ScriptEnvironment> {
        if !self.is_enabled() || file.path(db).as_system_path().is_none() {
            return None;
        }

        let entry = self.entry(file);
        let mut state = entry.lock();

        match *state {
            ScriptEnvironmentState::Vacant => {
                let environment = ScriptEnvironment::new(db, None, None, None);
                *state = ScriptEnvironmentState::Current { environment };
                Some(environment)
            }
            ScriptEnvironmentState::Current { environment }
            | ScriptEnvironmentState::Synchronizing { environment, .. } => Some(environment),
        }
    }

    fn is_enabled(&self) -> bool {
        self.inner.use_uv.script_environments_enabled()
    }

    fn existing_entry(&self, file: File) -> Option<Arc<ScriptEnvironmentEntry>> {
        let entry = self.inner.scripts.get(&file)?;
        Some(Arc::clone(entry.value()))
    }

    fn entry(&self, file: File) -> Arc<ScriptEnvironmentEntry> {
        // Return an owned entry so the map's shard lock is released before the caller locks the
        // script's state. Otherwise, unrelated scripts in the same shard would also be blocked.
        Arc::clone(self.inner.scripts.entry(file).or_default().value())
    }
}

impl std::panic::RefUnwindSafe for UvEnvironments {}

/// Changes applied by polling completed uv metadata requests.
#[derive(Debug, Default)]
pub struct UvSyncChanges {
    pub scripts: Vec<File>,
    /// `Some` also reports completion when rediscovery leaves the project unchanged or fails.
    pub project: Option<ProjectReloadResult>,
}

impl UvSyncChanges {
    pub fn is_empty(&self) -> bool {
        self.scripts.is_empty() && self.project.is_none()
    }
}

/// Applied workspace metadata and the error from its latest request.
/// Both fields are absent when no workspace metadata has been requested.
#[derive(Debug, Default, Clone, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct ProjectEnvironment {
    pub(crate) metadata: Option<UvMetadata>,
    pub(crate) error: Option<Box<str>>,
}

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
    /// The host should skip semantic diagnostics until synchronization finishes.
    Pending,

    /// The default or previously synchronized environment can be used.
    Available,
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
pub(crate) struct ScriptEnvironment {
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
    pub(crate) uv_metadata: Option<UvMetadata>,

    /// The error from the most recent synchronization.
    ///
    /// `None` if synchronization has not completed or completed successfully.
    #[returns(as_deref)]
    pub(crate) initialization_error: Option<Box<str>>,
}

struct UvEnvironmentsInner {
    use_uv: UseUv,
    project: Mutex<Option<ProjectSync>>,
    scripts: FxDashMap<File, Arc<ScriptEnvironmentEntry>>,
    sync_service: UvMetadataService,
    sync_results: Receiver<UvMetadataResult>,
    sync_wakeups: Receiver<()>,
}

impl Default for UvEnvironmentsInner {
    fn default() -> Self {
        let (results_sender, sync_results) = crossbeam::channel::unbounded();
        let (wake_sender, sync_wakeups) = crossbeam::channel::bounded(1);
        Self {
            use_uv: UseUv::default(),
            project: Mutex::default(),
            scripts: FxDashMap::default(),
            sync_service: UvMetadataService::new(results_sender, wake_sender),
            sync_results,
            sync_wakeups,
        }
    }
}

struct ProjectSync {
    next_request: Option<SystemPathBuf>,
    cancellation: CancellationTokenSource,
}

type ScriptEnvironmentEntry = Mutex<ScriptEnvironmentState>;

/// The synchronization state of one script environment.
///
/// Ensures that at most one synchronization runs for a script at a time.
#[derive(Default)]
enum ScriptEnvironmentState {
    /// No [`ScriptEnvironment`] exists and no synchronization is running.
    #[default]
    Vacant,

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
    /// If no input exists yet, one is created before synchronization starts so semantic queries
    /// have a stable Salsa identity to depend on.
    ///
    /// The background worker cannot update the [`ScriptEnvironment`] because modifying a Salsa
    /// input requires mutable access to the database. Instead, the CLI or language-server main loop
    /// updates it when synchronization finishes.
    Synchronizing {
        /// The [`ScriptEnvironment`] input that will receive the synchronization result.
        environment: ScriptEnvironment,

        availability: ScriptEnvironmentAvailability,

        /// The active synchronization and the next request, if any.
        sync: InFlightSync,
    },
}

/// An active synchronization and the latest request to run after it.
///
/// At most one additional request is retained. If the script changes repeatedly while uv is
/// running, newer requests replace the pending request instead of accumulating in a queue.
struct InFlightSync {
    active_cache_key: ScriptEnvironmentCacheKey,
    next_request: Option<ScriptSyncRequest>,
    /// Signals the worker to skip this request if it has not started.
    cancellation: CancellationTokenSource,
}

impl InFlightSync {
    /// Updates the synchronization to run after the active request.
    ///
    /// If `request` matches the active synchronization, removes any previously requested follow-up.
    /// Otherwise, replaces the follow-up with `request`. A changed request cancels the queued job;
    /// an already running uv process is allowed to finish.
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
        self.cancellation.cancel();
        true
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
    let (uv_metadata, initialization_error) = match Uv::parse_metadata_output(db.system(), output) {
        Ok(metadata) => (Some(metadata), None),
        Err(error) => (None, Some(error.to_string().into_boxed_str())),
    };
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
    use ruff_db::Db as _;
    use ruff_db::files::{File, system_path_to_file};
    use ruff_db::system::{DbWithWritableSystem, SystemPath};
    use salsa::Setter;
    use salsa::plumbing::AsId;
    use serde_json::{Value, json};
    use ty_python_semantic::Db as _;

    use super::{UvMetadata, script_environment};
    use crate::db::testing::TestDb;
    use crate::{Db as _, ProjectMetadata, UseUv};

    #[test]
    fn semantic_lookup_creates_a_stable_default_environment() -> anyhow::Result<()> {
        let root = SystemPath::new("/project").to_path_buf();
        let path = root.join("script.py");
        let metadata = ProjectMetadata::new("test", root).with_use_uv(UseUv::Scripts);
        let mut db = TestDb::new(metadata);
        db.write_dedented(
            path.as_str(),
            r#"
            # /// script
            # dependencies = []
            # ///
            "#,
        )?;
        let file = system_path_to_file(&db, &path)?;
        let environments = db.uv_environments().clone();

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

    #[test]
    fn dependency_metadata_changes_recheck_unchanged_imports() -> anyhow::Result<()> {
        let root = SystemPath::new(if cfg!(windows) {
            "C:/project"
        } else {
            "/project"
        });
        let path = root.join("script.py");
        let environment = root.join(".venv");
        let site_packages = environment.join(if cfg!(windows) {
            "Lib/site-packages"
        } else {
            "lib/python3.13/site-packages"
        });
        let indirect = r#"
            # /// script
            # dependencies = ['parent']
            # [tool.ty.rules]
            # missing-direct-dependency = 'error'
            # ///
            import leaf
            "#;
        let declared = indirect.replace("['parent']", "['parent', 'leaf']");
        let metadata = ProjectMetadata::new("test", root.to_path_buf()).with_use_uv(UseUv::Scripts);
        let mut db = TestDb::new(metadata);
        db.write_dedented(
            environment.join("pyvenv.cfg").as_str(),
            &format!(
                r#"
                home = {root}
                include-system-site-packages = false
                version = 3.13.5
                "#,
            ),
        )?;
        db.write_file(site_packages.join("leaf.py"), "")?;
        db.write_dedented(path.as_str(), indirect)?;
        let file = system_path_to_file(&db, &path)?;

        let indirect_metadata = dependency_metadata(root, &path, &["parent"]);
        apply_dependency_metadata(&mut db, file, &indirect_metadata)?;
        let diagnostics = db.check_file(file);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .id()
                .is_lint_named("missing-direct-dependency")
        );

        // Before synchronization, dependency checks keep using the previous declarations.
        db.write_dedented(path.as_str(), &declared)?;
        assert_eq!(db.check_file(file).len(), 1);

        let declared_metadata = dependency_metadata(root, &path, &["parent", "leaf"]);
        apply_dependency_metadata(&mut db, file, &declared_metadata)?;
        assert!(db.check_file(file).is_empty());

        db.write_dedented(path.as_str(), indirect)?;
        assert!(db.check_file(file).is_empty());
        let program = db.program_file(file).program(&db).as_id();

        // Only the synchronization result changes after the preceding check. Its dependency
        // declarations must invalidate the cached diagnostic even though `Program` is unchanged.
        apply_dependency_metadata(&mut db, file, &indirect_metadata)?;
        assert_eq!(db.program_file(file).program(&db).as_id(), program);
        let diagnostics = db.check_file(file);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .id()
                .is_lint_named("missing-direct-dependency")
        );

        Ok(())
    }

    fn dependency_metadata(root: &SystemPath, path: &SystemPath, dependencies: &[&str]) -> Value {
        json!({
            "schema": {"version": "preview"},
            "workspace_root": root.as_str(),
            "environment": {"root": root.join(".venv"), "python": {"version": "3.13.5"}},
            "script": {"path": path.as_str(), "id": "script+test"},
            "resolution": {
                "script+test": {
                    "kind": "script",
                    "dependencies": dependencies.iter().map(|id| json!({"id": id})).collect::<Vec<_>>()
                },
                "parent": {
                    "kind": "package", "name": "parent", "dependencies": [{"id": "leaf"}]
                },
                "leaf": {"kind": "package", "name": "leaf", "dependencies": []}
            },
            "module_owners": {
                "leaf": [{"package_id": "leaf"}]
            }
        })
    }

    fn apply_dependency_metadata(
        db: &mut TestDb,
        file: File,
        metadata: &Value,
    ) -> anyhow::Result<()> {
        let metadata = UvMetadata::from_metadata(&serde_json::to_vec(metadata)?, db.system())?;
        let environment = script_environment(db, file).context("expected a script environment")?;
        environment.set_uv_metadata(db).to(Some(metadata));
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
        use ty_python_semantic::Db as _;
        use ty_static::EnvVars;

        use super::super::{ScriptEnvironmentAvailability, UvSyncChanges, script_environment};
        use crate::db::testing::TestDb;
        use crate::{Db as _, ProjectMetadata, UseUv};

        #[test]
        fn newer_project_refresh_discards_old_metadata() -> anyhow::Result<()> {
            let mut case = UvTestCase::project(
                r#"
                [project]
                name = 'example'
                version = '0.1.0'
                requires-python = '>=3.8'
                "#,
            )?;
            let root = case.db.project().root(&case.db).to_path_buf();
            let environments = case.db.uv_environments().clone();
            environments.request_project_sync(&case.db, &root, &|_, _| None);

            // Leave the first result unapplied, then add a dependency-free workspace member.
            environments
                .sync_wakeups()
                .recv_timeout(Duration::from_secs(30))?;
            assert!(environments.has_pending_synchronizations());
            let member_root = root.join("member");
            case.db.write_dedented(
                member_root.join("pyproject.toml").as_str(),
                r#"
                [project]
                name = 'member'
                version = '0.1.0'
                requires-python = '>=3.8'
                "#,
            )?;
            case.db.write_dedented(
                case.path.as_str(),
                r#"
                [project]
                name = 'example'
                version = '0.1.0'
                requires-python = '>=3.8'

                [tool.uv.workspace]
                members = ['member']
                "#,
            )?;
            case.sync_workspace()?;
            environments.request_project_sync(&case.db, &root, &|_, _| None);

            let mut changes = environments.poll_sync(&mut case.db);
            if changes.project.is_none() {
                changes = case.wait_for_synchronizations()?;
            }
            assert!(changes.project.is_some());
            assert!(!environments.has_pending_synchronizations());

            let environment = case.db.project().metadata(&case.db).environment();
            assert_eq!(environment.error, None);
            assert_eq!(
                environment
                    .metadata
                    .as_ref()
                    .context("missing uv metadata")?
                    .members()
                    .iter()
                    .find(|member| member.name.as_ref() == "member")
                    .map(|member| member.path.as_path()),
                Some(member_root.as_path())
            );
            Ok(())
        }

        #[test]
        fn initial_background_synchronization_is_pending_until_completion() -> anyhow::Result<()> {
            let mut case = UvTestCase::script(
                r#"
                # /// script
                # requires-python = ">=3.12"
                # dependencies = ["attrs==25.4.0"]
                # ///
                from attrs import define
                "#,
            )?;
            let environments = case.db.uv_environments().clone();

            environments.request_sync(
                &mut case.db,
                case.file,
                ScriptEnvironmentAvailability::Pending,
                &|_, _| None,
            );
            assert!(environments.is_initialization_pending(&case.db, case.file));

            assert_eq!(case.wait_for_synchronizations()?.scripts, vec![case.file]);
            assert!(!environments.is_initialization_pending(&case.db, case.file));
            case.assert_can_import("attrs")?;

            Ok(())
        }

        #[test]
        fn existing_environment_remains_available_during_background_synchronization()
        -> anyhow::Result<()> {
            let mut case = UvTestCase::script(
                r#"
                # /// script
                # requires-python = ">=3.12"
                # dependencies = ["attrs==25.4.0"]
                # ///
                from attrs import define
                "#,
            )?;
            let environments = case.db.uv_environments().clone();

            // Semantic analysis can reach a script before the host requests synchronization.
            // The missing-import diagnostic must clear when the environment becomes available.
            let _ = case.db.program_file(case.file);
            let diagnostics = crate::check_file(&case.db, case.file);
            assert_eq!(diagnostics.len(), 1);
            assert_eq!(diagnostics[0].id().as_str(), "unresolved-import");

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

            assert_eq!(case.wait_for_synchronizations()?.scripts, vec![case.file]);
            assert_eq!(script_environment(&case.db, case.file), Some(environment));
            case.assert_can_import("attrs")?;
            assert!(crate::check_file(&case.db, case.file).is_empty());

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
            let mut case = UvTestCase::script(initial)?;
            let environments = case.db.uv_environments().clone();
            environments.request_sync(
                &mut case.db,
                case.file,
                ScriptEnvironmentAvailability::Pending,
                &|_, _| None,
            );
            assert_eq!(case.wait_for_synchronizations()?.scripts, vec![case.file]);
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

            let mut changed = environments.poll_sync(&mut case.db).scripts;
            changed.extend(case.wait_for_synchronizations()?.scripts);
            assert_eq!(changed, vec![case.file]);
            case.assert_can_import("attrs")?;

            Ok(())
        }

        #[test]
        fn background_result_cancels_snapshots_before_locking_entry() -> anyhow::Result<()> {
            let mut case = UvTestCase::script(
                r#"
                # /// script
                # requires-python = ">=3.12"
                # dependencies = []
                # ///
                "#,
            )?;
            let environments = case.db.uv_environments().clone();
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
                    entry.try_lock_for(Duration::from_secs(1)).is_some(),
                    "the entry lock was held while waiting for a cancelled snapshot"
                );
                drop(snapshot);
            });

            assert_eq!(
                environments.poll_sync(&mut case.db).scripts,
                vec![case.file]
            );
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
            fn script(source: &str) -> anyhow::Result<Self> {
                Self::new("script.py", source, UseUv::Scripts)
            }

            fn project(source: &str) -> anyhow::Result<Self> {
                let case = Self::new("pyproject.toml", source, UseUv::On)?;
                case.sync_workspace()?;
                Ok(case)
            }

            fn wait_for_synchronizations(&mut self) -> anyhow::Result<UvSyncChanges> {
                let environments = self.db.uv_environments().clone();
                let wakeups = environments.sync_wakeups();
                let mut changes = UvSyncChanges::default();

                while environments.has_pending_synchronizations() {
                    wakeups
                        .recv_timeout(Duration::from_secs(30))
                        .context("uv synchronization did not finish")?;
                    let completed = environments.poll_sync(&mut self.db);
                    changes.scripts.extend(completed.scripts);
                    changes.project = completed.project.or(changes.project);
                }

                Ok(changes)
            }

            fn sync_workspace(&self) -> anyhow::Result<()> {
                let output = Command::new(self.db.test_system().env_var(EnvVars::UV)?)
                    .current_dir(self.db.project().root(&self.db))
                    .args(["sync", "--offline"])
                    .output()?;
                anyhow::ensure!(
                    output.status.success(),
                    "uv sync failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(())
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

            fn new(file_name: &str, source: &str, use_uv: UseUv) -> anyhow::Result<Self> {
                let temp_dir = tempfile::tempdir()?;
                let root = SystemPath::from_std_path(temp_dir.path())
                    .context("temporary directory is not a valid UTF-8 path")?;
                // uv resolves symlinks, including macOS's symlinked temporary directory.
                let root = OsSystem::default().canonicalize_path(root)?;
                let metadata = ProjectMetadata::new("test", root.clone()).with_use_uv(use_uv);
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

                let path = root.join(file_name);
                db.write_dedented(path.as_str(), source)?;
                let file = system_path_to_file(&db, &path)?;

                Ok(Self {
                    _temp_dir: temp_dir,
                    db,
                    file,
                    path,
                })
            }
        }
    }
}
