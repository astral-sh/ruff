pub use project_watcher::ProjectWatcher;
use ruff_db::system::{SystemPath, SystemPathBuf, SystemVirtualPathBuf};
pub use watcher::{directory_watcher, EventHandler, Watcher};

mod project_watcher;
mod watcher;

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
    /// The file corresponding to the given path was opened in an editor.
    Opened(SystemPathBuf),

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

    /// A new virtual path was created.
    CreatedVirtual(SystemVirtualPathBuf),

    /// The content of a virtual path was changed.
    ChangedVirtual(SystemVirtualPathBuf),

    /// A virtual path was deleted.
    DeletedVirtual(SystemVirtualPathBuf),

    /// The file watcher failed to observe some changes and now is out of sync with the file system.
    ///
    /// This can happen if many files are changed at once. The consumer should rescan all files to catch up
    /// with the file system.
    Rescan,
}

impl ChangeEvent {
    /// Creates a new [`Changed`] event for the file content at the given path.
    ///
    /// [`Changed`]: ChangeEvent::Changed
    pub fn file_content_changed(path: SystemPathBuf) -> ChangeEvent {
        ChangeEvent::Changed {
            path,
            kind: ChangedKind::FileContent,
        }
    }

    pub fn file_name(&self) -> Option<&str> {
        self.system_path().and_then(|path| path.file_name())
    }

    pub fn system_path(&self) -> Option<&SystemPath> {
        match self {
            ChangeEvent::Opened(path)
            | ChangeEvent::Created { path, .. }
            | ChangeEvent::Changed { path, .. }
            | ChangeEvent::Deleted { path, .. } => Some(path),
            _ => None,
        }
    }

    pub const fn is_rescan(&self) -> bool {
        matches!(self, ChangeEvent::Rescan)
    }

    pub const fn is_created(&self) -> bool {
        matches!(self, ChangeEvent::Created { .. })
    }

    pub const fn is_changed(&self) -> bool {
        matches!(self, ChangeEvent::Changed { .. })
    }

    pub const fn is_deleted(&self) -> bool {
        matches!(self, ChangeEvent::Deleted { .. })
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
