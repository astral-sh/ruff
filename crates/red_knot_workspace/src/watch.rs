use ruff_db::system::{SystemPath, SystemPathBuf};
pub use watcher::{directory_watcher, EventHandler, Watcher};
pub use workspace_watcher::WorkspaceWatcher;

mod watcher;
mod workspace_watcher;

/// Classification of a file system change event.
///
/// ## Renaming a path
/// Renaming a path creates a [`ChangeEvent::Deleted`] event for the old path and/or a [`ChangeEvent::Created`] for the new location.
/// Whether both events are created or just one of them depends from where to where the path was moved:
///
/// * Inside the watched directory: Both events are created.
/// * From a watched directory to a non-watched directory: Only a [`ChangeEvent::Deleted`] event is created.
/// * From a non-watched directory to a watched directory: Only a [`ChangeEvent::Created`] event is created.
///
/// ## Renaming a directory
/// It's up to the file watcher implementation to aggregate the rename event for a directory to a single rename
/// event instead of emitting an event for each file or subdirectory in that path.
#[derive(Debug, PartialEq, Eq)]
pub enum ChangeEvent {
    /// A new path was created
    Created {
        path: SystemPathBuf,
        kind: CreatedKind,
    },

    /// The content or metadata of a path was changed.
    Changed {
        path: SystemPathBuf,
        kind: ChangedKind,
    },

    /// A path was deleted.
    Deleted {
        path: SystemPathBuf,
        kind: DeletedKind,
    },

    /// The file watcher failed to observe some changes and now is out of sync with the file system.
    ///
    /// This can happen if many files are changed at once. The consumer should rescan all files to catch up
    /// with the file system.
    Rescan,
}

impl ChangeEvent {
    pub fn file_name(&self) -> Option<&str> {
        self.path().and_then(|path| path.file_name())
    }

    pub fn path(&self) -> Option<&SystemPath> {
        match self {
            ChangeEvent::Created { path, .. }
            | ChangeEvent::Changed { path, .. }
            | ChangeEvent::Deleted { path, .. } => Some(path),
            ChangeEvent::Rescan => None,
        }
    }
}

/// Classification of an event that creates a new path.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CreatedKind {
    /// A file was created.
    File,

    /// A directory was created.
    Directory,

    /// A file, directory, or any other kind of path was created.
    Any,
}

/// Classification of an event related to a content or metadata change.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ChangedKind {
    /// The content of a file was changed.
    FileContent,

    /// The metadata of a file was changed.
    FileMetadata,

    /// Either the content or metadata of a path was changed.
    Any,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DeletedKind {
    File,
    Directory,
    Any,
}
