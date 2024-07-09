use rustc_hash::FxHashSet;

use ruff_db::files::File;
use ruff_db::system::{SystemPath, SystemPathBuf};

use crate::db::Jar;

pub mod db;
pub mod lint;
pub mod program;
pub mod target_version;
pub mod watch;

#[derive(Debug, Clone)]
pub struct Workspace {
    root: SystemPathBuf,
    /// The files that are open in the workspace.
    ///
    /// * Editor: The files that are actively being edited in the editor (the user has a tab open with the file).
    /// * CLI: The resolved files passed as arguments to the CLI.
    open_files: FxHashSet<File>,
}

impl Workspace {
    pub fn new(root: SystemPathBuf) -> Self {
        Self {
            root,
            open_files: FxHashSet::default(),
        }
    }

    pub fn root(&self) -> &SystemPath {
        self.root.as_path()
    }

    // TODO having the content in workspace feels wrong.
    pub fn open_file(&mut self, file_id: File) {
        self.open_files.insert(file_id);
    }

    pub fn close_file(&mut self, file_id: File) {
        self.open_files.remove(&file_id);
    }

    // TODO introduce an `OpenFile` type instead of using an anonymous tuple.
    pub fn open_files(&self) -> impl Iterator<Item = File> + '_ {
        self.open_files.iter().copied()
    }

    pub fn is_file_open(&self, file_id: File) -> bool {
        self.open_files.contains(&file_id)
    }
}
