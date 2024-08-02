use crate::db::RootDatabase;
use crate::watch::Watcher;
use ruff_db::system::SystemPathBuf;
use rustc_hash::FxHashSet;
use std::fmt::{Formatter, Write};
use tracing::info;

/// Wrapper around a [`Watcher`] that watches the relevant paths of a workspace.
pub struct WorkspaceWatcher {
    watcher: Watcher,

    /// The paths that need to be watched. This includes paths for which setting up file watching failed.
    watched_paths: FxHashSet<SystemPathBuf>,

    /// Paths that should be watched but setting up the watcher failed for some reason.
    /// This should be rare.
    errored_paths: Vec<SystemPathBuf>,
}

impl WorkspaceWatcher {
    /// Create a new workspace watcher.
    pub fn new(watcher: Watcher, db: &RootDatabase) -> Self {
        let mut watcher = Self {
            watcher,
            watched_paths: FxHashSet::default(),
            errored_paths: Vec::new(),
        };

        watcher.update(db);

        watcher
    }

    pub fn update(&mut self, db: &RootDatabase) {
        let new_watch_paths = db.workspace().paths_to_watch(db);

        let mut added_folders = new_watch_paths.difference(&self.watched_paths).peekable();
        let mut removed_folders = self.watched_paths.difference(&new_watch_paths).peekable();

        if added_folders.peek().is_none() && removed_folders.peek().is_none() {
            return;
        }

        for added_folder in added_folders {
            // Log a warning. It's not worth aborting if registering a single folder fails because
            // Ruff otherwise stills works as expected.
            if let Err(error) = self.watcher.watch(added_folder) {
                // TODO: Log a user-facing warning.
                tracing::warn!("Failed to setup watcher for path '{added_folder}': {error}. You have to restart Ruff after making changes to files under this path or you might see stale results.");
                self.errored_paths.push(added_folder.clone());
            }
        }

        for removed_path in removed_folders {
            if let Some(index) = self
                .errored_paths
                .iter()
                .position(|path| path == removed_path)
            {
                self.errored_paths.swap_remove(index);
                continue;
            }

            if let Err(error) = self.watcher.unwatch(removed_path) {
                info!("Failed to remove the file watcher for the path '{removed_path}: {error}.");
            }
        }

        info!(
            "Set up file watchers for {}",
            DisplayWatchedPaths {
                paths: &new_watch_paths
            }
        );

        self.watched_paths = new_watch_paths;
    }

    /// Returns `true` if setting up watching for any path failed.
    pub fn has_errored_paths(&self) -> bool {
        !self.errored_paths.is_empty()
    }

    pub fn flush(&self) {
        self.watcher.flush();
    }

    pub fn stop(self) {
        self.watcher.stop();
    }
}

struct DisplayWatchedPaths<'a> {
    paths: &'a FxHashSet<SystemPathBuf>,
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
