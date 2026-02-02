use crate::NotebookDocument;
use crate::session::index::Document;
use crate::system::LSPSystem;
use ruff_db::Db as _;
use ruff_db::files::{File, FilePath};
use ty_project::{Db as ProjectDb, ProjectDatabase};

#[salsa::db]
pub(crate) trait Db: ProjectDb {
    /// Returns the LSP [`Document`] corresponding to `File` or
    /// `None` if the file isn't open in the editor.
    fn document(&self, file: File) -> Option<&Document>;

    /// Returns the LSP [`NotebookDocument`] corresponding to `File` or
    /// `None` if the file isn't open in the editor or if it isn't a notebook.
    fn notebook_document(&self, file: File) -> Option<&NotebookDocument> {
        self.document(file)?.as_notebook()
    }
}

#[salsa::db]
impl Db for ProjectDatabase {
    fn document(&self, file: File) -> Option<&Document> {
        self.system()
            .as_any()
            .downcast_ref::<LSPSystem>()
            .and_then(|system| match file.path(self) {
                FilePath::System(path) => system.system_path_to_document(path),
                FilePath::SystemVirtual(path) => system.system_virtual_path_to_document(path),
                FilePath::Vendored(_) => None,
            })
    }
}
