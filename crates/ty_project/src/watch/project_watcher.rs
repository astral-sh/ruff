use std::fmt::{Formatter, Write};
use std::hash::Hasher;

use tracing::info;

use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_db::Upcast;
use ty_python_semantic::system_module_search_paths;

use crate::db::{Db, ProjectDatabase};
use crate::watch::Watcher;

/// Wrapper around a [`Watcher`] that watches the relevant paths of a project.
pub struct ProjectWatcher {
    watcher: Watcher,

    /// The paths that need to be watched. This includes paths for which setting up file watching failed.
    watched_paths: Vec<SystemPathBuf>,

    /// True if registering a watcher for any path failed.
    has_errored_paths: bool,

    /// Cache key over the paths that need watching. It allows short-circuiting if the paths haven't changed.
    cache_key: Option<u64>,
}

impl ProjectWatcher {
    /// Create a new project watcher.
    pub fn new(watcher: Watcher, db: &ProjectDatabase) -> Self {
        let mut watcher = Self {
            watcher,
            watched_paths: Vec::new(),
            cache_key: None,
            has_errored_paths: false,
        };

        watcher.update(db);

        watcher
    }

    pub fn update(&mut self, db: &ProjectDatabase) {
        let search_paths: Vec<_> = system_module_search_paths(db.upcast()).collect();
        let project_path = db.project().root(db);

        let new_cache_key = Self::compute_cache_key(project_path, &search_paths);

        if self.cache_key == Some(new_cache_key) {
            return;
        }

        // Unregister all watch paths because ordering is important for linux because
        // it only emits an event for the last added watcher if a subtree is covered by multiple watchers.
        // A path can be covered by multiple watchers if a subdirectory symlinks to a path that's covered by another watch path:
        // ```text
        // - bar
        //   - baz.py
        // - project
        //   - bar -> /bar
        //   - foo.py
        // ```
        for path in self.watched_paths.drain(..) {
            if let Err(error) = self.watcher.unwatch(&path) {
                info!("Failed to remove the file watcher for path `{path}`: {error}");
            }
        }

        self.has_errored_paths = false;

        let config_paths = db
            .project()
            .metadata(db)
            .extra_configuration_paths()
            .iter()
            .map(SystemPathBuf::as_path);

        // Watch both the project root and any paths provided by the user on the CLI (removing any redundant nested paths).
        // This is necessary to observe changes to files that are outside the project root.
        // We always need to watch the project root to observe changes to its configuration.
        let included_paths = ruff_db::system::deduplicate_nested_paths(
            std::iter::once(project_path).chain(
                db.project()
                    .included_paths_list(db)
                    .iter()
                    .map(SystemPathBuf::as_path),
            ),
        );

        // Find the non-overlapping module search paths and filter out paths that are already covered by the project.
        // Module search paths are already canonicalized.
        let unique_module_paths = ruff_db::system::deduplicate_nested_paths(
            search_paths
                .into_iter()
                .filter(|path| !path.starts_with(project_path)),
        );

        // Now add the new paths, first starting with the project path and then
        // adding the library search paths, and finally the paths for configurations.
        for path in included_paths
            .chain(unique_module_paths)
            .chain(config_paths)
        {
            // Log a warning. It's not worth aborting if registering a single folder fails because
            // Ruff otherwise stills works as expected.
            if let Err(error) = self.watcher.watch(path) {
                // TODO: Log a user-facing warning.
                tracing::warn!("Failed to setup watcher for path `{path}`: {error}. You have to restart Ruff after making changes to files under this path or you might see stale results.");
                self.has_errored_paths = true;
            } else {
                self.watched_paths.push(path.to_path_buf());
            }
        }

        info!(
            "Set up file watchers for {}",
            DisplayWatchedPaths {
                paths: &self.watched_paths
            }
        );

        self.cache_key = Some(new_cache_key);
    }

    fn compute_cache_key(project_root: &SystemPath, search_paths: &[&SystemPath]) -> u64 {
        let mut cache_key_hasher = CacheKeyHasher::new();
        search_paths.cache_key(&mut cache_key_hasher);
        project_root.cache_key(&mut cache_key_hasher);

        cache_key_hasher.finish()
    }

    /// Returns `true` if setting up watching for any path failed.
    pub fn has_errored_paths(&self) -> bool {
        self.has_errored_paths
    }

    pub fn flush(&self) {
        self.watcher.flush();
    }

    pub fn stop(self) {
        self.watcher.stop();
    }
}

struct DisplayWatchedPaths<'a> {
    paths: &'a [SystemPathBuf],
}

impl std::fmt::Display for DisplayWatchedPaths<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_char('[')?;

        let mut iter = self.paths.iter();
        if let Some(first) = iter.next() {
            write!(f, "\"{first}\"")?;

            for path in iter {
                write!(f, ", \"{path}\"")?;
            }
        }

        f.write_char(']')
    }
}
