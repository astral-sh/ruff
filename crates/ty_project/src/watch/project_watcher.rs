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
use crate::uv::UvMetadata;
use crate::watch::Watcher;

/// Wrapper around a [`Watcher`] that watches the relevant paths of a project.
pub struct ProjectWatcher {
    watcher: Watcher,

    /// The paths currently watched, in registration order.
    watched_paths: Vec<SystemPathBuf>,

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
            cache_key: None,
            has_errored_paths: false,
        };

        watcher.update(db);

        watcher
    }

    pub fn update(&mut self, db: &mut ProjectDatabase) {
        const MAX_RECONCILIATION_PASSES: usize = 10;

        for _ in 0..MAX_RECONCILIATION_PASSES {
            if !self.update_once(db) {
                return;
            }

            // `watch_paths` reads `.pth` files to find editable module search paths. If a
            // script's `site-packages` was unwatched when a `.pth` file changed from `/old`
            // to `/new`, this pass registered `/old` using stale contents. The refresh above
            // updates the `.pth` file; recompute the plan and register `/new` before returning.
            if self.cache_key == Some(watch_paths(db, db.project()).cache_key()) {
                return;
            }
        }

        tracing::warn!(
            "File watcher paths did not stabilize after {MAX_RECONCILIATION_PASSES} updates; changes outside the registered paths may be missed until the next update"
        );
    }

    /// Returns whether newly covered paths were refreshed.
    fn update_once(&mut self, db: &mut ProjectDatabase) -> bool {
        let watch_plan = watch_paths(db, db.project());

        if self.cache_key == Some(watch_plan.cache_key()) {
            return false;
        }

        let paths = watch_plan.paths();
        let previously_watched = self.watched_paths.clone();
        let mut watcher_paths = self.watcher.paths_mut();
        let mut newly_covered_paths = Vec::new();

        // On Linux, overlapping watches through a symlink report events using the path of the
        // last registered watch. If `/project/bar` points to `/bar`, the module search path
        // `/bar` must be watched after `/project` so imports receive events under `/bar`.
        // Configuration paths come last so their events use their explicit paths. Appending
        // preserves this precedence; other changes require re-registering in the new order.
        let first_new_path = if !self.has_errored_paths && paths.starts_with(&self.watched_paths) {
            self.watched_paths.len()
        } else {
            for path in self.watched_paths.drain(..) {
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
                // A previous recursive watch already covered this path, even if it must be
                // re-registered for precedence. Skip initial setup to avoid rescanning every
                // project file immediately after discovery.
                if self.cache_key.is_some()
                    && !previously_watched
                        .iter()
                        .any(|watched| path.starts_with(watched))
                {
                    newly_covered_paths.push(path.clone());
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

        self.cache_key = Some(watch_plan.cache_key());

        if newly_covered_paths.is_empty() {
            return false;
        }

        // A newly covered path may contain known files that changed while it was unwatched.
        // Registering the watch does not update their cached contents, so refresh them here.
        Files::sync_all_recursive(db, newly_covered_paths);

        true
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

/// The paths watched for a project and its scripts, with a key for detecting changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatchPaths {
    cache_key: u64,
    paths: Box<[SystemPathBuf]>,
}

impl WatchPaths {
    /// A key for changes to the watched paths.
    pub fn cache_key(&self) -> u64 {
        self.cache_key
    }

    /// Paths whose changes need watching.
    pub fn paths(&self) -> &[SystemPathBuf] {
        &self.paths
    }
}

/// Watches are registered in uv workspace, project, uv environment, module, then configuration
/// order. On Linux, the last registered watch determines the path reported for overlapping symlinks.
///
/// uv discovers the workspace root from `pyproject.toml`, so a successful metadata request tells
/// us where to watch even if `uv.lock` is created later. A member's ty project root may be inside
/// that workspace; changes to the lockfile at the workspace root can then change `members` and
/// `resolution` in `uv workspace metadata`. The selected uv environment can also be outside the
/// project root: its `pyvenv.cfg` can change the reported Python, and installed `.dist-info`
/// directories can change `module_owners`.
///
/// Project roots and explicitly included paths are watched because project files are discovered
/// by walking them. Module search paths come after them so imports are reported relative to their
/// search roots, rather than through symlinks inside the project. Configuration paths come last
/// so their events use the explicit paths checked for configuration changes.
#[salsa::tracked(returns(ref))]
pub fn watch_paths(db: &dyn Db, project: Project) -> WatchPaths {
    let project_path = project.root(db);
    let uv_workspace = project.metadata(db).uv_workspace();
    let workspace_root = uv_workspace
        .map(UvMetadata::workspace_root)
        .filter(|root| !root.starts_with(project_path));
    let uv_environment = uv_workspace
        .and_then(UvMetadata::environment)
        .filter(|root| !root.starts_with(project_path));

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

    // The workspace root can contain the project root. Register it first so the project watch
    // still reports events under the project's own path.
    let paths: Vec<_> = workspace_root
        .into_iter()
        .chain(included_paths)
        .chain(uv_environment)
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
