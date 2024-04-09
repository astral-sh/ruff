use crate::files::FileId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
mod check;
pub mod files;

#[derive(Debug)]
pub struct Workspace {
    /// TODO this should be a resolved path. We should probably use a newtype wrapper that guarantees that
    /// PATH is a UTF-8 path and is normalized.
    root: PathBuf,
    /// The files that are open in the workspace.
    ///
    /// * Editor: The files that are actively being edited in the editor (the user has a tab open with the file).
    /// * CLI: The resolved files passed as arguments to the CLI.
    open_files: FxHashMap<FileId, OpenFileData>,
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            open_files: FxHashMap::default(),
        }
    }

    pub fn root(&self) -> &Path {
        self.root.as_path()
    }

    // TODO having the content in workspace feels wrong.
    pub fn open_file(&mut self, file_id: FileId, content: String) {
        self.open_files.insert(
            file_id,
            OpenFileData {
                content,
                version: 0,
            },
        );
    }

    pub fn close_file(&mut self, file_id: FileId) {
        self.open_files.remove(&file_id);
    }

    // TODO introduce an `OpenFile` type instead of using an anonymous tuple.
    pub fn open_files(&self) -> impl Iterator<Item = (FileId, OpenFile)> + '_ {
        self.open_files.iter().map(|(file_id, file)| {
            (
                *file_id,
                OpenFile {
                    content: &file.content,
                    version: file.version,
                },
            )
        })
    }

    pub fn is_file_open(&self, file_id: FileId) -> bool {
        self.open_files.contains_key(&file_id)
    }
}

#[derive(Debug)]
struct OpenFileData {
    content: String,
    version: i64,
}

pub struct OpenFile<'a> {
    content: &'a str,
    version: i64,
}
