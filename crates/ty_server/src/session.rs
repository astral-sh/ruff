//! Data model, state management, and configuration resolution.

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::anyhow;
use lsp_types::{ClientCapabilities, TextDocumentContentChangeEvent, Url};
use options::GlobalOptions;
use ruff_db::Db;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::SystemPath;
use ty_project::{ProjectDatabase, ProjectMetadata};

pub(crate) use self::capabilities::ResolvedClientCapabilities;
pub use self::index::DocumentQuery;
pub(crate) use self::options::{AllOptions, ClientOptions};
use self::settings::ClientSettings;
use crate::document::{DocumentKey, DocumentVersion, NotebookDocument};
use crate::session::request_queue::RequestQueue;
use crate::system::{AnySystemPath, LSPSystem};
use crate::{PositionEncoding, TextDocument};

mod capabilities;
pub(crate) mod client;
pub(crate) mod index;
mod options;
mod request_queue;
mod settings;

/// The global state for the LSP
pub struct Session {
    /// Used to retrieve information about open documents and settings.
    ///
    /// This will be [`None`] when a mutable reference is held to the index via [`index_mut`]
    /// to prevent the index from being accessed while it is being modified. It will be restored
    /// when the mutable reference ([`MutIndexGuard`]) is dropped.
    ///
    /// [`index_mut`]: Session::index_mut
    index: Option<Arc<index::Index>>,

    /// Maps workspace folders to their respective project databases.
    projects_by_workspace_folder: BTreeMap<PathBuf, ProjectDatabase>,

    /// The global position encoding, negotiated during LSP initialization.
    position_encoding: PositionEncoding,

    /// Tracks what LSP features the client supports and doesn't support.
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,

    /// Tracks the pending requests between client and server.
    request_queue: RequestQueue,

    /// Has the client requested the server to shutdown.
    shutdown_requested: bool,
}

impl Session {
    pub(crate) fn new(
        client_capabilities: &ClientCapabilities,
        position_encoding: PositionEncoding,
        global_options: GlobalOptions,
        workspace_folders: &[(Url, ClientOptions)],
    ) -> crate::Result<Self> {
        let mut workspaces = BTreeMap::new();
        let index = Arc::new(index::Index::new(global_options.into_settings()));

        // TODO: Consider workspace settings
        for (url, _) in workspace_folders {
            let path = url
                .to_file_path()
                .map_err(|()| anyhow!("Workspace URL is not a file or directory: {:?}", url))?;
            let system_path = SystemPath::from_std_path(&path)
                .ok_or_else(|| anyhow!("Workspace path is not a valid UTF-8 path: {:?}", path))?;
            let system = LSPSystem::new(index.clone());

            // TODO(dhruvmanila): Get the values from the client settings
            let mut metadata = ProjectMetadata::discover(system_path, &system)?;
            metadata.apply_configuration_files(&system)?;

            // TODO(micha): Handle the case where the program settings are incorrect more gracefully.
            workspaces.insert(path, ProjectDatabase::new(metadata, system)?);
        }

        Ok(Self {
            position_encoding,
            projects_by_workspace_folder: workspaces,
            index: Some(index),
            resolved_client_capabilities: Arc::new(ResolvedClientCapabilities::new(
                client_capabilities,
            )),
            request_queue: RequestQueue::new(),
            shutdown_requested: false,
        })
    }

    pub(crate) fn request_queue(&self) -> &RequestQueue {
        &self.request_queue
    }

    pub(crate) fn request_queue_mut(&mut self) -> &mut RequestQueue {
        &mut self.request_queue
    }

    pub(crate) fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    pub(crate) fn set_shutdown_requested(&mut self, requested: bool) {
        self.shutdown_requested = requested;
    }

    // TODO(dhruvmanila): Ideally, we should have a single method for `workspace_db_for_path_mut`
    // and `default_workspace_db_mut` but the borrow checker doesn't allow that.
    // https://github.com/astral-sh/ruff/pull/13041#discussion_r1726725437

    /// Returns a reference to the project's [`ProjectDatabase`] corresponding to the given path,
    /// or the default project if no project is found for the path.
    pub(crate) fn project_db_or_default(&self, path: &AnySystemPath) -> &ProjectDatabase {
        path.as_system()
            .and_then(|path| self.project_db_for_path(path.as_std_path()))
            .unwrap_or_else(|| self.default_project_db())
    }

    /// Returns a reference to the project's [`ProjectDatabase`] corresponding to the given path, if
    /// any.
    pub(crate) fn project_db_for_path(&self, path: impl AsRef<Path>) -> Option<&ProjectDatabase> {
        self.projects_by_workspace_folder
            .range(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db)
    }

    /// Returns a mutable reference to the project [`ProjectDatabase`] corresponding to the given
    /// path, if any.
    pub(crate) fn project_db_for_path_mut(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Option<&mut ProjectDatabase> {
        self.projects_by_workspace_folder
            .range_mut(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db)
    }

    /// Returns a reference to the default project [`ProjectDatabase`]. The default project is the
    /// minimum root path in the project map.
    pub(crate) fn default_project_db(&self) -> &ProjectDatabase {
        // SAFETY: Currently, ty only support a single project.
        self.projects_by_workspace_folder.values().next().unwrap()
    }

    /// Returns a mutable reference to the default project [`ProjectDatabase`].
    pub(crate) fn default_project_db_mut(&mut self) -> &mut ProjectDatabase {
        // SAFETY: Currently, ty only support a single project.
        self.projects_by_workspace_folder
            .values_mut()
            .next()
            .unwrap()
    }

    pub(crate) fn key_from_url(&self, url: Url) -> crate::Result<DocumentKey> {
        self.index().key_from_url(url)
    }

    /// Creates a document snapshot with the URL referencing the document to snapshot.
    ///
    /// Returns `None` if the url can't be converted to a document key or if the document isn't open.
    pub fn take_snapshot(&self, url: Url) -> Option<DocumentSnapshot> {
        let key = self.key_from_url(url).ok()?;
        Some(DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            client_settings: self.index().global_settings(),
            document_ref: self.index().make_document_ref(&key)?,
            position_encoding: self.position_encoding,
        })
    }

    /// Iterates over the document keys for all open text documents.
    pub(super) fn text_document_keys(&self) -> impl Iterator<Item = DocumentKey> + '_ {
        self.index()
            .text_document_paths()
            .map(|path| DocumentKey::Text(path.clone()))
    }

    /// Registers a notebook document at the provided `path`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_notebook_document(
        &mut self,
        path: &AnySystemPath,
        document: NotebookDocument,
    ) {
        self.index_mut().open_notebook_document(path, document);
    }

    /// Registers a text document at the provided `path`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_text_document(&mut self, path: &AnySystemPath, document: TextDocument) {
        self.index_mut().open_text_document(path, document);
    }

    /// Updates a text document at the associated `key`.
    ///
    /// The document key must point to a text document, or this will throw an error.
    pub(crate) fn update_text_document(
        &mut self,
        key: &DocumentKey,
        content_changes: Vec<TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
    ) -> crate::Result<()> {
        let position_encoding = self.position_encoding;
        self.index_mut()
            .update_text_document(key, content_changes, new_version, position_encoding)
    }

    /// De-registers a document, specified by its key.
    /// Calling this multiple times for the same document is a logic error.
    pub(crate) fn close_document(&mut self, key: &DocumentKey) -> crate::Result<()> {
        self.index_mut().close_document(key)?;
        Ok(())
    }

    /// Returns a reference to the index.
    ///
    /// # Panics
    ///
    /// Panics if there's a mutable reference to the index via [`index_mut`].
    ///
    /// [`index_mut`]: Session::index_mut
    fn index(&self) -> &index::Index {
        self.index.as_ref().unwrap()
    }

    /// Returns a mutable reference to the index.
    ///
    /// This method drops all references to the index and returns a guard that will restore the
    /// references when dropped. This guard holds the only reference to the index and allows
    /// modifying it.
    fn index_mut(&mut self) -> MutIndexGuard {
        let index = self.index.take().unwrap();

        for db in self.projects_by_workspace_folder.values_mut() {
            // Remove the `index` from each database. This drops the count of `Arc<Index>` down to 1
            db.system_mut()
                .as_any_mut()
                .downcast_mut::<LSPSystem>()
                .unwrap()
                .take_index();
        }

        // There should now be exactly one reference to index which is self.index.
        let index = Arc::into_inner(index);

        MutIndexGuard {
            session: self,
            index,
        }
    }

    pub(crate) fn client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }
}

/// A guard that holds the only reference to the index and allows modifying it.
///
/// When dropped, this guard restores all references to the index.
struct MutIndexGuard<'a> {
    session: &'a mut Session,
    index: Option<index::Index>,
}

impl Deref for MutIndexGuard<'_> {
    type Target = index::Index;

    fn deref(&self) -> &Self::Target {
        self.index.as_ref().unwrap()
    }
}

impl DerefMut for MutIndexGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.index.as_mut().unwrap()
    }
}

impl Drop for MutIndexGuard<'_> {
    fn drop(&mut self) {
        if let Some(index) = self.index.take() {
            let index = Arc::new(index);
            for db in self.session.projects_by_workspace_folder.values_mut() {
                db.system_mut()
                    .as_any_mut()
                    .downcast_mut::<LSPSystem>()
                    .unwrap()
                    .set_index(index.clone());
            }

            self.session.index = Some(index);
        }
    }
}

/// An immutable snapshot of `Session` that references
/// a specific document.
#[derive(Debug)]
pub struct DocumentSnapshot {
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
    client_settings: Arc<ClientSettings>,
    document_ref: index::DocumentQuery,
    position_encoding: PositionEncoding,
}

impl DocumentSnapshot {
    pub(crate) fn resolved_client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    pub(crate) fn query(&self) -> &index::DocumentQuery {
        &self.document_ref
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    pub(crate) fn client_settings(&self) -> &ClientSettings {
        &self.client_settings
    }

    pub(crate) fn file(&self, db: &dyn Db) -> Option<File> {
        match AnySystemPath::try_from_url(self.document_ref.file_url()).ok()? {
            AnySystemPath::System(path) => system_path_to_file(db, path).ok(),
            AnySystemPath::SystemVirtual(virtual_path) => db
                .files()
                .try_virtual_file(&virtual_path)
                .map(|virtual_file| virtual_file.file()),
        }
    }
}
