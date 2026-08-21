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

use std::sync::{Arc, OnceLock};

use ruff_db::FxDashMap;
use ruff_db::files::File;
use ruff_db::system::SystemPathBuf;

use super::script_tag;
use crate::uv::{MetadataTarget, Uv, UvMetadata, UvMetadataError, uv_executable_error};
use crate::{Db, ProgressReporter, UseUv};

/// Returns the Salsa input representing `file`'s script environment.
///
/// Project checks normally synchronize the script's virtual environment before it is needed. This
/// function never invokes uv; it returns the [`ScriptEnvironment`] input so semantic queries can
/// depend on it.
///
/// If no [`ScriptEnvironment`] exists, creates one without uv metadata. Its identity remains the
/// same when synchronization later provides that metadata, just as a [`File`] continues to identify
/// the same path when a previously nonexistent file is created.
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

    /// Initializes `file`'s environment before a project check analyzes it.
    ///
    /// Project checks can discover scripts while iterating their files, so they initialize those
    /// scripts synchronously. This may invoke uv and creates the [`ScriptEnvironment`] input that
    /// semantic queries depend on.
    ///
    /// Concurrent callers wait for the initial environment creation to finish.
    pub(crate) fn initialize_blocking(
        &self,
        db: &dyn Db,
        file: File,
        reporter: &dyn ProgressReporter,
    ) {
        if !self.is_enabled() || script_tag(db, file).is_none() {
            return;
        }

        let Some(path) = file.path(db).as_system_path() else {
            return;
        };

        self.get_or_init_with(file, || {
            let python = script_python(db);

            let _progress = reporter.for_script(db, file);

            let metadata = Uv::new(db.system())
                .map_err(uv_executable_error)
                .map_err(UvMetadataError::Invocation)
                .and_then(|uv| {
                    uv.metadata(
                        db.system(),
                        MetadataTarget::Script {
                            path,
                            python: python.as_deref(),
                        },
                    )
                });

            match metadata {
                Ok(metadata) => ScriptEnvironment::new(db, Some(metadata), None),
                Err(error) => {
                    ScriptEnvironment::new(db, None, Some(error.to_string().into_boxed_str()))
                }
            }
        });
    }

    fn environment(&self, db: &dyn Db, file: File) -> Option<ScriptEnvironment> {
        if !self.is_enabled() || file.path(db).as_system_path().is_none() {
            return None;
        }

        Some(self.get_or_init_with(file, || ScriptEnvironment::new(db, None, None)))
    }

    fn get_or_init_with(
        &self,
        file: File,
        initialize: impl FnOnce() -> ScriptEnvironment,
    ) -> ScriptEnvironment {
        // Drop the map's shard guard before invoking uv so unrelated scripts sharing that shard
        // can initialize concurrently.
        let environment = Arc::clone(self.inner.by_file.entry(file).or_default().value());
        *environment.get_or_init(initialize)
    }

    fn is_enabled(&self) -> bool {
        self.inner.use_uv.script_environments_enabled()
    }
}

impl std::panic::RefUnwindSafe for ScriptEnvironments {}

/// The stable Salsa identity for a script's environment.
///
/// Like [`File`], this input can exist before the resource it represents is initialized. If
/// semantic analysis reaches a script before synchronization, [`script_environment`] creates
/// this input without invoking uv.
///
/// Keeping its identity stable ensures that Salsa invalidates queries when the environment's
/// Python version, module search paths, or initialization error changes.
#[salsa::input(heap_size=ruff_memory_usage::heap_size)]
pub(super) struct ScriptEnvironment {
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
    by_file: FxDashMap<File, Arc<OnceLock<ScriptEnvironment>>>,
}

fn script_python(db: &dyn Db) -> Option<SystemPathBuf> {
    let metadata = db.project().metadata(db);

    metadata
        .override_options()
        .and_then(|options| options.environment.as_ref())
        .and_then(|environment| environment.python.as_ref())
        .map(|python| python.absolute(metadata.root(), db.system()))
}
