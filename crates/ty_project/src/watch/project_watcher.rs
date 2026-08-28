use std::fmt::{Formatter, Write};
use std::hash::Hasher;

use tracing::info;

use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_db::Db as _;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ty_module_resolver::system_module_search_paths;

use crate::Project;
use crate::db::{Db, ProjectDatabase};
use crate::script::Script;
use crate::watch::Watcher;

/// Wrapper around a [`Watcher`] that watches the relevant paths of a project.
pub struct ProjectWatcher {
    watcher: Watcher,

    /// The watched paths, including paths retained to keep known files up to date.
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
        let project = db.project();
        let new_cache_key = watch_paths_cache_key(db, project);

        if self.cache_key == Some(new_cache_key) {
            return;
        }

        let project_path = project.root(db);
        let config_paths = project.metadata(db).extra_configuration_paths();

        // Watch both the project root and any paths provided by the user on the CLI (removing any redundant nested paths).
        // This is necessary to observe changes to files that are outside the project root.
        // We always need to watch the project root to observe changes to its configuration.
        let mut paths: Vec<_> = ruff_db::system::deduplicate_nested_paths(
            std::iter::once(project_path).chain(
                project
                    .included_paths_list(db)
                    .iter()
                    .map(SystemPathBuf::as_path),
            ),
        )
        .map(SystemPath::to_path_buf)
        .collect();
        let included_paths_len = paths.len();

        // Find the non-overlapping module search paths and filter out paths that are already covered by the project.
        // Module search paths are already canonicalized.
        let unique_module_paths = ruff_db::system::deduplicate_nested_paths(
            module_search_paths(db, project)
                .into_iter()
                .filter(|path| !path.starts_with(project_path)),
        );

        paths.extend(
            unique_module_paths
                .chain(config_paths)
                .map(SystemPath::to_path_buf),
        );

        // Removing a search path does not discard its cached files. Keep watching them so
        // restoring that search path cannot reuse stale file contents or directory listings.
        let retained_paths: Vec<_> = self
            .watched_paths
            .iter()
            .filter(|path| {
                !paths.iter().any(|current| path.starts_with(current))
                    && db.files().has_known_files_under(db, path)
            })
            .cloned()
            .collect();
        // Current module paths take precedence over retained paths for overlapping symlinks.
        paths.splice(included_paths_len..included_paths_len, retained_paths);

        if paths == self.watched_paths {
            self.cache_key = Some(new_cache_key);
            return;
        }

        let mut watcher_paths = self.watcher.paths_mut();

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
            if let Err(error) = watcher_paths.remove(&path) {
                info!("Failed to remove the file watcher for path `{path}`: {error}");
            }
        }

        self.has_errored_paths = false;

        // Register project paths first, then retained and current module paths, and finally
        // configuration paths.
        for path in paths {
            if let Err(error) = watcher_paths.add(&path) {
                // TODO: Log a user-facing warning.
                tracing::warn!(
                    "Failed to setup watcher for path `{path}`: {error}. You have to restart ty after making changes to files under this path or you might see stale results."
                );
                self.has_errored_paths = true;
            } else {
                self.watched_paths.push(path);
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

        self.cache_key = Some(new_cache_key);
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

#[salsa::tracked(returns(copy))]
fn watch_paths_cache_key(db: &dyn Db, project: Project) -> u64 {
    let mut search_paths = module_search_paths(db, project);
    // Script order and duplicate search paths do not change what needs watching.
    search_paths.sort_unstable();
    search_paths.dedup();

    let mut hasher = CacheKeyHasher::new();
    search_paths.cache_key(&mut hasher);
    project.root(db).cache_key(&mut hasher);
    project.included_paths_list(db).cache_key(&mut hasher);
    for path in project.metadata(db).extra_configuration_paths() {
        path.cache_key(&mut hasher);
    }
    hasher.finish()
}

fn module_search_paths(db: &dyn Db, project: Project) -> Vec<&SystemPath> {
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
    search_paths
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

#[cfg(test)]
mod tests {
    use ruff_db::system::{DbWithWritableSystem as _, SystemPathBuf};
    use ruff_db::testing::assert_function_query_was_not_run;

    use crate::db::testing::TestDb;
    use crate::{Db as _, ProjectMetadata};

    use super::watch_paths_cache_key;

    #[test]
    fn cache_key_is_reused_after_code_edits() -> anyhow::Result<()> {
        let mut db = TestDb::new(ProjectMetadata::new(
            "test",
            SystemPathBuf::from("/project"),
        ));
        db.write_file("/project/ordinary.py", "value = 1")?;
        db.write_dedented(
            "/project/script.py",
            r"
            # /// script
            # dependencies = []
            # ///
            value = 1
            ",
        )?;
        let project = db.project();
        // The CLI indexes files before setting up the watcher.
        project.files(&db);
        let key = watch_paths_cache_key(&db, project);

        db.write_file("/project/ordinary.py", "value = 2")?;
        db.write_dedented(
            "/project/script.py",
            r"
            # /// script
            # dependencies = []
            # ///
            value = 2
            ",
        )?;
        db.take_salsa_events();

        assert_eq!(watch_paths_cache_key(&db, project), key);
        let events = db.take_salsa_events();
        assert_function_query_was_not_run(&db, watch_paths_cache_key, project, &events);
        Ok(())
    }
}
