use std::sync::{Arc, OnceLock};

use ruff_db::FxDashMap;
use ruff_db::files::File;
use ruff_db::system::SystemPathBuf;
use ty_static::EnvVars;

use super::script_tag;
use crate::metadata::uv::{MetadataTarget, Uv, UvMetadata, UvMetadataError, uv_executable_error};
use crate::{Db, ProgressReporter};

/// Lazily initialized script environments (results of calling `uv workspace metadata --script`).
///
/// A script's uv metadata lives outside Salsa, so it is modeled as an input that is created on
/// demand before the script is checked. Updating the input is the responsibility of the host
/// because doing so requires a mutable database.
#[derive(Clone, Default)]
pub struct ScriptEnvironments {
    inner: Arc<ScriptEnvironmentsInner>,
}

impl ScriptEnvironments {
    /// Ensures that a usable environment exists for `file`.
    ///
    /// Concurrent callers wait for the initial environment creation to finish.
    pub(crate) fn ensure_environment_initialized(
        &self,
        db: &dyn Db,
        file: File,
        reporter: Option<&dyn ProgressReporter>,
    ) {
        if !script_integration_enabled(db) || script_tag(db, file).is_none() {
            return;
        }

        let Some(path) = file.path(db).as_system_path() else {
            return;
        };

        self.get_or_init_with(file, || {
            let python = script_python(db);

            let _progress = reporter.and_then(|reporter| reporter.for_script(db, file));

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

    /// Returns the environment prepared for `file` by the host.
    pub(super) fn environment(&self, db: &dyn Db, file: File) -> Option<ScriptEnvironment> {
        if !script_integration_enabled(db) || file.path(db).as_system_path().is_none() {
            return None;
        }

        let Some(shared_environment) = self.inner.by_file.get(&file) else {
            panic!("script environment was not initialized by its host");
        };
        let environment = shared_environment.value().get().copied();
        assert!(
            environment.is_some(),
            "script environment was not initialized by its host"
        );
        environment
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
}

impl std::panic::RefUnwindSafe for ScriptEnvironments {}

/// Stable input recording script-specific uv metadata or an initialization failure.
#[salsa::input(heap_size=ruff_memory_usage::heap_size)]
pub(super) struct ScriptEnvironment {
    #[returns(as_ref)]
    pub(super) uv_metadata: Option<UvMetadata>,

    #[returns(as_deref)]
    pub(super) initialization_error: Option<Box<str>>,
}

#[derive(Default)]
struct ScriptEnvironmentsInner {
    by_file: FxDashMap<File, Arc<OnceLock<ScriptEnvironment>>>,
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
