use std::any::Any;
use std::fmt;
use std::fmt::Display;
use std::hash::{DefaultHasher, Hash, Hasher as _};
use std::panic::RefUnwindSafe;
use std::sync::Arc;

use crate::Db;
use crate::document::{DocumentKey, LanguageId};
use crate::session::index::{Document, Index};
use lsp_types::Url;
use ruff_db::file_revision::FileRevision;
use ruff_db::files::{File, FilePath};
use ruff_db::system::walk_directory::WalkDirectoryBuilder;
use ruff_db::system::{
    CaseSensitivity, DirectoryEntry, FileType, GlobError, Metadata, PatternError, Result, System,
    SystemPath, SystemPathBuf, SystemVirtualPath, SystemVirtualPathBuf, WritableSystem,
};
use ruff_notebook::{Notebook, NotebookError};
use ruff_python_ast::PySourceType;
use ty_ide::cached_vendored_path;

/// Returns a [`Url`] for the given [`File`].
pub(crate) fn file_to_url(db: &dyn Db, file: File) -> Option<Url> {
    match file.path(db) {
        FilePath::System(system) => Url::from_file_path(system.as_std_path()).ok(),
        FilePath::SystemVirtual(path) => Url::parse(path.as_str()).ok(),
        FilePath::Vendored(path) => {
            let system_path = cached_vendored_path(db, path)?;

            Url::from_file_path(system_path.as_std_path()).ok()
        }
    }
}

/// Represents either a [`SystemPath`] or a [`SystemVirtualPath`].
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum AnySystemPath {
    System(SystemPathBuf),
    SystemVirtual(SystemVirtualPathBuf),
}

impl AnySystemPath {
    pub(crate) const fn as_system(&self) -> Option<&SystemPathBuf> {
        match self {
            AnySystemPath::System(system_path_buf) => Some(system_path_buf),
            AnySystemPath::SystemVirtual(_) => None,
        }
    }

    #[expect(unused)]
    pub(crate) const fn as_virtual(&self) -> Option<&SystemVirtualPath> {
        match self {
            AnySystemPath::SystemVirtual(path) => Some(path.as_path()),
            AnySystemPath::System(_) => None,
        }
    }
}

impl fmt::Display for AnySystemPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnySystemPath::System(system_path) => write!(f, "{system_path}"),
            AnySystemPath::SystemVirtual(virtual_path) => write!(f, "{virtual_path}"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LSPSystem {
    /// A read-only copy of the index where the server stores all the open documents and settings.
    ///
    /// This will be [`None`] when a mutable reference is held to the index via [`index_mut`]
    /// method to prevent the index from being accessed while it is being modified. It will be
    /// restored when the mutable reference is dropped.
    ///
    /// [`index_mut`]: crate::Session::index_mut
    index: Option<Arc<Index>>,

    /// A native system implementation.
    ///
    /// This is used to delegate method calls that are not handled by the LSP system. It is also
    /// used as a fallback when the documents are not found in the LSP index.
    native_system: Arc<dyn System + 'static + Send + Sync + RefUnwindSafe>,
}

impl LSPSystem {
    pub(crate) fn new(
        index: Arc<Index>,
        native_system: Arc<dyn System + 'static + Send + Sync + RefUnwindSafe>,
    ) -> Self {
        Self {
            index: Some(index),
            native_system,
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

    fn document(&self, path: AnySystemPath) -> Option<&Document> {
        let index = self.index();
        index.document(&DocumentKey::from(path)).ok()
    }

    fn source_type_from_document(
        document: &Document,
        extension: Option<&str>,
    ) -> Option<PySourceType> {
        match document {
            Document::Text(text) => match text.language_id()? {
                LanguageId::Python => Some(
                    extension
                        .and_then(PySourceType::try_from_extension)
                        .unwrap_or(PySourceType::Python),
                ),
                LanguageId::Other => None,
            },
            Document::Notebook(_) => Some(PySourceType::Ipynb),
        }
    }

    pub(crate) fn system_path_to_document(&self, path: &SystemPath) -> Option<&Document> {
        let any_path = AnySystemPath::System(path.to_path_buf());
        self.document(any_path)
    }

    pub(crate) fn system_virtual_path_to_document(
        &self,
        path: &SystemVirtualPath,
    ) -> Option<&Document> {
        let any_path = AnySystemPath::SystemVirtual(path.to_path_buf());
        self.document(any_path)
    }
}

impl System for LSPSystem {
    fn path_metadata(&self, path: &SystemPath) -> Result<Metadata> {
        let document = self.system_path_to_document(path);

        if let Some(document) = document {
            Ok(Metadata::new(
                document_revision(document, self.index()),
                None,
                FileType::File,
            ))
        } else {
            self.native_system.path_metadata(path)
        }
    }

    fn canonicalize_path(&self, path: &SystemPath) -> Result<SystemPathBuf> {
        self.native_system.canonicalize_path(path)
    }

    fn path_exists_case_sensitive(&self, path: &SystemPath, prefix: &SystemPath) -> bool {
        self.native_system.path_exists_case_sensitive(path, prefix)
    }

    fn source_type(&self, path: &SystemPath) -> Option<PySourceType> {
        let document = self.system_path_to_document(path)?;
        Self::source_type_from_document(document, path.extension())
    }

    fn virtual_path_source_type(&self, path: &SystemVirtualPath) -> Option<PySourceType> {
        let document = self.system_virtual_path_to_document(path)?;
        Self::source_type_from_document(document, path.extension())
    }

    fn read_to_string(&self, path: &SystemPath) -> Result<String> {
        let document = self.system_path_to_document(path);

        match document {
            Some(Document::Text(document)) => Ok(document.contents().to_string()),
            _ => self.native_system.read_to_string(path),
        }
    }

    fn read_to_notebook(&self, path: &SystemPath) -> std::result::Result<Notebook, NotebookError> {
        let document = self.system_path_to_document(path);

        match document {
            Some(Document::Text(document)) => Notebook::from_source_code(document.contents()),
            Some(Document::Notebook(notebook)) => Ok(notebook.to_ruff_notebook(self.index())),
            None => self.native_system.read_to_notebook(path),
        }
    }

    fn read_virtual_path_to_string(&self, path: &SystemVirtualPath) -> Result<String> {
        let document = self
            .system_virtual_path_to_document(path)
            .ok_or_else(|| virtual_path_not_found(path))?;

        if let Document::Text(document) = &document {
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
            .system_virtual_path_to_document(path)
            .ok_or_else(|| virtual_path_not_found(path))?;

        match document {
            Document::Text(document) => Notebook::from_source_code(document.contents()),
            Document::Notebook(notebook) => Ok(notebook.to_ruff_notebook(self.index())),
        }
    }

    fn current_directory(&self) -> &SystemPath {
        self.native_system.current_directory()
    }

    fn user_config_directory(&self) -> Option<SystemPathBuf> {
        self.native_system.user_config_directory()
    }

    fn cache_dir(&self) -> Option<SystemPathBuf> {
        self.native_system.cache_dir()
    }

    fn read_directory<'a>(
        &'a self,
        path: &SystemPath,
    ) -> Result<Box<dyn Iterator<Item = Result<DirectoryEntry>> + 'a>> {
        self.native_system.read_directory(path)
    }

    fn walk_directory(&self, path: &SystemPath) -> WalkDirectoryBuilder {
        self.native_system.walk_directory(path)
    }

    fn glob(
        &self,
        pattern: &str,
    ) -> std::result::Result<
        Box<dyn Iterator<Item = std::result::Result<SystemPathBuf, GlobError>> + '_>,
        PatternError,
    > {
        self.native_system.glob(pattern)
    }

    fn as_writable(&self) -> Option<&dyn WritableSystem> {
        self.native_system.as_writable()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn case_sensitivity(&self) -> CaseSensitivity {
        self.native_system.case_sensitivity()
    }

    fn env_var(&self, name: &str) -> std::result::Result<String, std::env::VarError> {
        self.native_system.env_var(name)
    }

    fn dyn_clone(&self) -> Box<dyn System> {
        Box::new(self.clone())
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
fn document_revision(document: &Document, index: &Index) -> FileRevision {
    // The file revision is just an opaque number which doesn't have any significant meaning other
    // than that the file has changed if the revisions are different.
    #[expect(clippy::cast_sign_loss)]
    match document {
        Document::Text(text) => FileRevision::new(text.version() as u128),
        Document::Notebook(notebook) => {
            // VS Code doesn't always bump the notebook version when the cell content changes.
            // Specifically, I noticed that VS Code re-uses the same version when:
            // 1. Adding a new cell
            // 2. Pasting some code that has an error
            //
            // The notification updating the cell content on paste re-used the same version as when the cell was added.
            // Because of that, hash all cell versions and the notebook versions together.
            let mut hasher = DefaultHasher::new();
            for cell_url in notebook.cell_urls() {
                if let Ok(cell) = index.document(&DocumentKey::from_url(cell_url)) {
                    cell.version().hash(&mut hasher);
                }
            }

            // Use higher 64 bits for notebook version and lower 64 bits for cell revisions
            let notebook_version_high = (notebook.version() as u128) << 64;
            let cell_versions_low = u128::from(hasher.finish()) & 0xFFFF_FFFF_FFFF_FFFF;
            let combined_revision = notebook_version_high | cell_versions_low;

            FileRevision::new(combined_revision)
        }
    }
}
