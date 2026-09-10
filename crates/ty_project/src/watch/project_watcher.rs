use std::fmt::{Formatter, Write};
use std::hash::Hasher;

use tracing::info;

use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_db::files::Files;
use ruff_db::system::{SystemPath, SystemPathBuf};
use rustc_hash::FxHashSet;
use ty_module_resolver::system_module_search_paths;

use crate::Project;
use crate::db::{Db, ProjectDatabase};
use crate::script::Script;
use crate::watch::Watcher;

/// Wrapper around a [`Watcher`] that watches the relevant paths of a project.
pub struct ProjectWatcher {
    watcher: Watcher,

    /// The paths currently watched, in registration order.
    watched_paths: Vec<SystemPathBuf>,

    /// Paths that were unwatched and must be refreshed if they are watched again.
    inactive_paths: FxHashSet<SystemPathBuf>,

    /// True if registering a watcher for any path failed.
    has_errored_paths: bool,

    /// Cache key over the paths requested by the project and its scripts.
    cache_key: Option<u64>,
}

impl ProjectWatcher {
    /// Create a new project watcher.
    pub fn new(watcher: Watcher, db: &mut ProjectDatabase) -> Self {
        let mut watcher = Self {
            watcher,
            watched_paths: Vec::new(),
            inactive_paths: FxHashSet::default(),
            cache_key: None,
            has_errored_paths: false,
        };

        watcher.update(db);

        watcher
    }

    pub fn update(&mut self, db: &mut ProjectDatabase) {
        let watch_paths = watch_paths(db, db.project());

        if self.cache_key == Some(watch_paths.cache_key) {
            return;
        }

        let paths = &watch_paths.paths;
        let mut watcher_paths = self.watcher.paths_mut();
        let mut rewatched_paths = Vec::new();

        // On Linux, overlapping watches through a symlink report events using the path of the
        // last registered watch. If `/project/bar` points to `/bar`, the module search path
        // `/bar` must be watched after `/project` so imports receive events under `/bar`.
        // Configuration paths come last so their events use their explicit paths. Appending
        // preserves this precedence; other changes require re-registering in the new order.
        let first_new_path = if !self.has_errored_paths && paths.starts_with(&self.watched_paths) {
            self.watched_paths.len()
        } else {
            let requested: FxHashSet<_> = paths.iter().collect();
            for path in self.watched_paths.drain(..) {
                if !requested.contains(&path) {
                    self.inactive_paths.insert(path.clone());
                }
                if let Err(error) = watcher_paths.remove(&path) {
                    info!("Failed to remove the file watcher for path `{path}`: {error}");
                }
            }
            0
        };

        self.has_errored_paths = false;

        for path in paths.iter().skip(first_new_path) {
            if let Err(error) = watcher_paths.add(path) {
                // TODO: Log a user-facing warning.
                tracing::warn!(
                    "Failed to setup watcher for path `{path}`: {error}. You have to restart ty after making changes to files under this path or you might see stale results."
                );
                self.has_errored_paths = true;
            } else {
                if self.inactive_paths.remove(path) {
                    rewatched_paths.push(path.clone());
                }
                self.watched_paths.push(path.clone());
            }
        }

        if let Err(error) = watcher_paths.commit() {
            tracing::warn!(
                "Failed to apply file watcher updates: {error}. You have to restart ty after making changes to watched files or you might see stale results."
            );
            self.has_errored_paths = true;
        }

        info!(
            "Set up file watchers for {}",
            DisplayWatchedPaths {
                paths: &self.watched_paths
            }
        );

        self.cache_key = Some(watch_paths.cache_key);

        // Changes to an unwatched path may not have updated Files. Refresh after registering
        // the watch so changes made during the refresh can still produce watcher events.
        Files::sync_all_recursive(db, rewatched_paths);
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct WatchPaths {
    cache_key: u64,
    paths: Box<[SystemPathBuf]>,
}

/// Watches are registered in project, module, then configuration order. On Linux, the last
/// registered watch determines the path reported for overlapping symlinks.
///
/// Project roots and explicitly included paths come first because project files are discovered
/// by walking them. Module search paths come next so imports are reported relative to their
/// search roots, rather than through symlinks inside the project. Configuration paths come last
/// so their events use the explicit paths checked for configuration changes.
#[salsa::tracked(returns(ref))]
fn watch_paths(db: &dyn Db, project: Project) -> WatchPaths {
    let project_path = project.root(db);

    // Watch both the project root and any paths provided by the user on the CLI (removing any redundant nested paths).
    // This is necessary to observe changes to files that are outside the project root.
    // We always need to watch the project root to observe changes to its configuration.
    let included_paths = ruff_db::system::deduplicate_nested_paths(
        std::iter::once(project_path).chain(
            project
                .included_paths_list(db)
                .iter()
                .map(SystemPathBuf::as_path),
        ),
    );

    let environment = project.program(db).resolver_environment(db);
    let mut search_paths: Vec<_> = system_module_search_paths(db, environment).collect();
    for file in project.script_files(db).iter() {
        if let Some(script) = Script::for_file(db, file) {
            search_paths.extend(system_module_search_paths(
                db,
                script.program(db).resolver_environment(db),
            ));
        }
    }

    // The project watch covers search paths inside its root. Deduplicate the others so
    // shared or nested search paths are registered once in stable order.
    let unique_module_paths = ruff_db::system::deduplicate_nested_paths(
        search_paths
            .into_iter()
            .filter(|path| !path.starts_with(project_path)),
    );

    let paths: Vec<_> = included_paths
        .chain(unique_module_paths)
        .chain(project.metadata(db).extra_configuration_paths())
        .map(SystemPath::to_path_buf)
        .collect();

    // Keep the last occurrence so later groups retain their registration precedence.
    let mut seen = FxHashSet::default();
    let mut unique_paths = Vec::new();
    for path in paths.into_iter().rev() {
        if seen.insert(path.clone()) {
            unique_paths.push(path);
        }
    }
    unique_paths.reverse();
    let paths = unique_paths.into_boxed_slice();

    let mut hasher = CacheKeyHasher::new();
    paths.cache_key(&mut hasher);
    WatchPaths {
        cache_key: hasher.finish(),
        paths,
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
