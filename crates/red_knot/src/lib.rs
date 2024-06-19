use rustc_hash::FxHashSet;

use ruff_db::file_system::{FileSystemPath, FileSystemPathBuf};
use ruff_db::vfs::VfsFile;

use crate::db::Jar;

pub mod db;
pub mod lint;
pub mod program;
pub mod watch;

#[derive(Debug, Clone)]
pub struct Workspace {
    root: FileSystemPathBuf,
    /// The files that are open in the workspace.
    ///
    /// * Editor: The files that are actively being edited in the editor (the user has a tab open with the file).
    /// * CLI: The resolved files passed as arguments to the CLI.
    open_files: FxHashSet<VfsFile>,
}

impl Workspace {
    pub fn new(root: FileSystemPathBuf) -> Self {
        Self {
            root,
            open_files: FxHashSet::default(),
        }
    }

    pub fn root(&self) -> &FileSystemPath {
        self.root.as_path()
    }

    // TODO having the content in workspace feels wrong.
    pub fn open_file(&mut self, file_id: VfsFile) {
        self.open_files.insert(file_id);
    }

    pub fn close_file(&mut self, file_id: VfsFile) {
        self.open_files.remove(&file_id);
    }

    // TODO introduce an `OpenFile` type instead of using an anonymous tuple.
    pub fn open_files(&self) -> impl Iterator<Item = VfsFile> + '_ {
        self.open_files.iter().copied()
    }

    pub fn is_file_open(&self, file_id: VfsFile) -> bool {
        self.open_files.contains(&file_id)
    }
}
