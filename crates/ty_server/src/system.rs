use std::any::Any;
use std::fmt::Display;
use std::sync::Arc;

use lsp_types::Url;
use ruff_db::file_revision::FileRevision;
use ruff_db::files::{File, FilePath};
use ruff_db::system::walk_directory::WalkDirectoryBuilder;
use ruff_db::system::{
    CaseSensitivity, DirectoryEntry, FileType, GlobError, Metadata, OsSystem, PatternError, Result,
    System, SystemPath, SystemPathBuf, SystemVirtualPath, SystemVirtualPathBuf,
};
use ruff_notebook::{Notebook, NotebookError};
use ty_python_semantic::Db;

use crate::session::index::Index;
use crate::DocumentQuery;

/// Converts the given [`Url`] to an [`AnySystemPath`].
///
/// If the URL scheme is `file`, then the path is converted to a [`SystemPathBuf`]. Otherwise, the
/// URL is converted to a [`SystemVirtualPathBuf`].
///
/// This fails in the following cases:
/// * The URL cannot be converted to a file path (refer to [`Url::to_file_path`]).
/// * If the URL is not a valid UTF-8 string.
pub(crate) fn url_to_any_system_path(url: &Url) -> std::result::Result<AnySystemPath, ()> {
    if url.scheme() == "file" {
        Ok(AnySystemPath::System(
            SystemPathBuf::from_path_buf(url.to_file_path()?).map_err(|_| ())?,
        ))
    } else {
        Ok(AnySystemPath::SystemVirtual(
            SystemVirtualPath::new(url.as_str()).to_path_buf(),
        ))
    }
}

pub(crate) fn file_to_url(db: &dyn Db, file: File) -> Option<Url> {
    match file.path(db) {
        FilePath::System(system) => Url::from_file_path(system.as_std_path()).ok(),
        FilePath::SystemVirtual(path) => Url::parse(path.as_str()).ok(),
        // TODO: Not yet supported, consider an approach similar to Sorbet's custom paths
        // https://sorbet.org/docs/sorbet-uris
        FilePath::Vendored(_) => None,
    }
}

/// Represents either a [`SystemPath`] or a [`SystemVirtualPath`].
#[derive(Debug)]
pub(crate) enum AnySystemPath {
    System(SystemPathBuf),
    SystemVirtual(SystemVirtualPathBuf),
}

#[derive(Debug)]
pub(crate) struct LSPSystem {
    /// A read-only copy of the index where the server stores all the open documents and settings.
    ///
    /// This will be [`None`] when a mutable reference is held to the index via [`index_mut`]
    /// method to prevent the index from being accessed while it is being modified. It will be
    /// restored when the mutable reference is dropped.
    ///
    /// [`index_mut`]: crate::Session::index_mut
    index: Option<Arc<Index>>,

    /// A system implementation that uses the local file system.
    os_system: OsSystem,
}

impl LSPSystem {
    pub(crate) fn new(index: Arc<Index>) -> Self {
        let cwd = std::env::current_dir().unwrap();
        let os_system = OsSystem::new(SystemPathBuf::from_path_buf(cwd).unwrap());

        Self {
            index: Some(index),
            os_system,
        }
    }

    /// Takes the index out of the system.
    pub(crate) fn take_index(&mut self) -> Option<Arc<Index>> {
        self.index.take()
    }

    /// Sets the index for the system.
    pub(crate) fn set_index(&mut self, index: Arc<Index>) {
        self.index = Some(index);
    }

    /// Returns a reference to the contained index.
    ///
    /// # Panics
    ///
    /// Panics if the index is `None`.
    fn index(&self) -> &Index {
        self.index.as_ref().unwrap()
    }

    fn make_document_ref(&self, url: Url) -> Option<DocumentQuery> {
        let index = self.index();
        let key = index.key_from_url(url);
        index.make_document_ref(key)
    }

    fn system_path_to_document_ref(&self, path: &SystemPath) -> Result<Option<DocumentQuery>> {
        let url = Url::from_file_path(path.as_std_path()).map_err(|()| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Failed to convert system path to URL: {path:?}"),
            )
        })?;
        Ok(self.make_document_ref(url))
    }

    fn system_virtual_path_to_document_ref(
        &self,
        path: &SystemVirtualPath,
    ) -> Result<Option<DocumentQuery>> {
        let url = Url::parse(path.as_str()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Failed to convert virtual path to URL: {path:?}"),
            )
        })?;
        Ok(self.make_document_ref(url))
    }
}

impl System for LSPSystem {
    fn path_metadata(&self, path: &SystemPath) -> Result<Metadata> {
        let document = self.system_path_to_document_ref(path)?;

        if let Some(document) = document {
            Ok(Metadata::new(
                document_revision(&document),
                None,
                FileType::File,
            ))
        } else {
            self.os_system.path_metadata(path)
        }
    }

    fn canonicalize_path(&self, path: &SystemPath) -> Result<SystemPathBuf> {
        self.os_system.canonicalize_path(path)
    }

    fn path_exists_case_sensitive(&self, path: &SystemPath, prefix: &SystemPath) -> bool {
        self.os_system.path_exists_case_sensitive(path, prefix)
    }

    fn read_to_string(&self, path: &SystemPath) -> Result<String> {
        let document = self.system_path_to_document_ref(path)?;

        match document {
            Some(DocumentQuery::Text { document, .. }) => Ok(document.contents().to_string()),
            _ => self.os_system.read_to_string(path),
        }
    }

    fn read_to_notebook(&self, path: &SystemPath) -> std::result::Result<Notebook, NotebookError> {
        let document = self.system_path_to_document_ref(path)?;

        match document {
            Some(DocumentQuery::Text { document, .. }) => {
                Notebook::from_source_code(document.contents())
            }
            Some(DocumentQuery::Notebook { notebook, .. }) => Ok(notebook.make_ruff_notebook()),
            None => self.os_system.read_to_notebook(path),
        }
    }

    fn read_virtual_path_to_string(&self, path: &SystemVirtualPath) -> Result<String> {
        let document = self
            .system_virtual_path_to_document_ref(path)?
            .ok_or_else(|| virtual_path_not_found(path))?;

        if let DocumentQuery::Text { document, .. } = &document {
            Ok(document.contents().to_string())
        } else {
            Err(not_a_text_document(path))
        }
    }

    fn read_virtual_path_to_notebook(
        &self,
        path: &SystemVirtualPath,
    ) -> std::result::Result<Notebook, NotebookError> {
        let document = self
            .system_virtual_path_to_document_ref(path)?
            .ok_or_else(|| virtual_path_not_found(path))?;

        match document {
            DocumentQuery::Text { document, .. } => Notebook::from_source_code(document.contents()),
            DocumentQuery::Notebook { notebook, .. } => Ok(notebook.make_ruff_notebook()),
        }
    }

    fn current_directory(&self) -> &SystemPath {
        self.os_system.current_directory()
    }

    fn user_config_directory(&self) -> Option<SystemPathBuf> {
        self.os_system.user_config_directory()
    }

    fn read_directory<'a>(
        &'a self,
        path: &SystemPath,
    ) -> Result<Box<dyn Iterator<Item = Result<DirectoryEntry>> + 'a>> {
        self.os_system.read_directory(path)
    }

    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder {
        self.os_system.walk_directory(path)
    }

    fn glob(
        &self,
        pattern: &str,
    ) -> std::result::Result<
        Box<dyn Iterator<Item = std::result::Result<SystemPathBuf, GlobError>>>,
        PatternError,
    > {
        self.os_system.glob(pattern)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn case_sensitivity(&self) -> CaseSensitivity {
        self.os_system.case_sensitivity()
    }
}

fn not_a_text_document(path: impl Display) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("Input is not a text document: {path}"),
    )
}

fn virtual_path_not_found(path: impl Display) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!("Virtual path does not exist: {path}"),
    )
}

/// Helper function to get the [`FileRevision`] of the given document.
fn document_revision(document: &DocumentQuery) -> FileRevision {
    // The file revision is just an opaque number which doesn't have any significant meaning other
    // than that the file has changed if the revisions are different.
    #[expect(clippy::cast_sign_loss)]
    FileRevision::new(document.version() as u128)
}
