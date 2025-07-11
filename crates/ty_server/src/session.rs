//! Data model, state management, and configuration resolution.

use std::collections::{BTreeMap, VecDeque};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use anyhow::{Context, anyhow};
use index::DocumentQueryError;
use lsp_server::Message;
use lsp_types::{ClientCapabilities, TextDocumentContentChangeEvent, Url};
use options::GlobalOptions;
use ruff_db::Db;
use ruff_db::files::File;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ty_project::metadata::Options;
use ty_project::{ProjectDatabase, ProjectMetadata};

pub(crate) use self::capabilities::ResolvedClientCapabilities;
pub use self::index::DocumentQuery;
pub(crate) use self::options::{AllOptions, ClientOptions, DiagnosticMode};
pub(crate) use self::settings::ClientSettings;
use crate::document::{DocumentKey, DocumentVersion, NotebookDocument};
use crate::session::request_queue::RequestQueue;
use crate::system::{AnySystemPath, LSPSystem};
use crate::{PositionEncoding, TextDocument};
use index::Index;

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
    index: Option<Arc<Index>>,

    /// Maps workspace folders to their respective workspace.
    workspaces: Workspaces,

    /// The projects across all workspaces.
    projects: BTreeMap<SystemPathBuf, ProjectDatabase>,

    default_project: ProjectDatabase,

    /// The global position encoding, negotiated during LSP initialization.
    position_encoding: PositionEncoding,

    /// Tracks what LSP features the client supports and doesn't support.
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,

    /// Tracks the pending requests between client and server.
    request_queue: RequestQueue,

    /// Has the client requested the server to shutdown.
    shutdown_requested: bool,

    deferred_messages: VecDeque<Message>,
}

impl Session {
    pub(crate) fn new(
        client_capabilities: &ClientCapabilities,
        position_encoding: PositionEncoding,
        global_options: GlobalOptions,
        workspace_folders: Vec<(Url, ClientOptions)>,
    ) -> crate::Result<Self> {
        let index = Arc::new(Index::new(global_options.into_settings()));

        let mut workspaces = Workspaces::default();
        for (url, options) in workspace_folders {
            workspaces.register(url, options)?;
        }

        let default_project = {
            let system = LSPSystem::new(index.clone());
            let metadata = ProjectMetadata::from_options(
                Options::default(),
                system.current_directory().to_path_buf(),
                None,
            )
            .unwrap();
            ProjectDatabase::new(metadata, system).unwrap()
        };

        Ok(Self {
            position_encoding,
            workspaces,
            deferred_messages: VecDeque::new(),
            index: Some(index),
            default_project,
            projects: BTreeMap::new(),
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

    /// The LSP specification doesn't allow configuration requests during initialization,
    /// but we need access to the configuration to resolve the settings in turn to create the
    /// project databases. This will become more important in the future when we support
    /// persistent caching. It's then crucial that we have the correct settings to select the
    /// right cache.
    ///
    /// We work around this by queueing up all messages that arrive between the `initialized` notification
    /// and the completion of workspace initialization (which waits for the client's configuration response).
    ///
    /// This queuing is only necessary when registering *new* workspaces. Changes to configurations
    /// don't need to go through the same process because we can update the existing
    /// database in place.
    ///
    /// See <https://github.com/Microsoft/language-server-protocol/issues/567#issuecomment-2085131917>
    pub(crate) fn should_defer_message(&mut self, message: Message) -> Option<Message> {
        if self.workspaces.all_initialized() {
            Some(message)
        } else {
            match &message {
                Message::Request(request) => {
                    tracing::debug!(
                        "Deferring `{}` request until all workspaces are initialized",
                        request.method
                    );
                }
                Message::Response(_) => {
                    // We still want to get client responses even during workspace initialization.
                    return Some(message);
                }
                Message::Notification(notification) => {
                    tracing::debug!(
                        "Deferring `{}` notification until all workspaces are initialized",
                        notification.method
                    );
                }
            }

            self.deferred_messages.push_back(message);
            None
        }
    }

    pub(crate) fn workspaces(&self) -> &Workspaces {
        &self.workspaces
    }

    // TODO(dhruvmanila): Ideally, we should have a single method for `workspace_db_for_path_mut`
    // and `default_workspace_db_mut` but the borrow checker doesn't allow that.
    // https://github.com/astral-sh/ruff/pull/13041#discussion_r1726725437

    /// Returns a reference to the project's [`ProjectDatabase`] corresponding to the given path,
    /// or the default project if no project is found for the path.
    pub(crate) fn project_db_or_default(&self, path: &AnySystemPath) -> &ProjectDatabase {
        path.as_system()
            .and_then(|path| self.project_db_for_path(path))
            .unwrap_or_else(|| self.default_project_db())
    }

    /// Returns a reference to the project's [`ProjectDatabase`] corresponding to the given path, if
    /// any.
    pub(crate) fn project_db_for_path(
        &self,
        path: impl AsRef<SystemPath>,
    ) -> Option<&ProjectDatabase> {
        self.projects
            .range(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db)
    }

    /// Returns a mutable reference to the project [`ProjectDatabase`] corresponding to the given
    /// path, if any.
    pub(crate) fn project_db_for_path_mut(
        &mut self,
        path: impl AsRef<SystemPath>,
    ) -> Option<&mut ProjectDatabase> {
        self.projects
            .range_mut(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db)
    }

    /// Returns a reference to the default project [`ProjectDatabase`]. The default project is the
    /// minimum root path in the project map.
    pub(crate) fn default_project_db(&self) -> &ProjectDatabase {
        &self.default_project
    }

    /// Returns a mutable reference to the default project [`ProjectDatabase`].
    pub(crate) fn default_project_db_mut(&mut self) -> &mut ProjectDatabase {
        &mut self.default_project
    }

    fn projects_mut(&mut self) -> impl Iterator<Item = &'_ mut ProjectDatabase> + '_ {
        self.projects
            .values_mut()
            .chain(std::iter::once(&mut self.default_project))
    }

    /// Returns the [`DocumentKey`] for the given URL.
    ///
    /// Refer to [`Index::key_from_url`] for more details.
    pub(crate) fn key_from_url(&self, url: Url) -> Result<DocumentKey, Url> {
        self.index().key_from_url(url)
    }

    pub(crate) fn initialize_workspaces(&mut self, workspace_settings: Vec<(Url, ClientOptions)>) {
        assert!(!self.workspaces.all_initialized());

        for (url, options) in workspace_settings {
            let Some(workspace) = self.workspaces.initialize(&url, options) else {
                continue;
            };
            // For now, create one project database per workspace.
            // In the future, index the workspace directories to find all projects
            // and create a project database for each.
            let system = LSPSystem::new(self.index.as_ref().unwrap().clone());
            let system_path = workspace.root();

            let root = system_path.to_path_buf();
            let project = ProjectMetadata::discover(&root, &system)
                .context("Failed to find project configuration")
                .and_then(|mut metadata| {
                    // TODO(dhruvmanila): Merge the client options with the project metadata options.
                    metadata
                        .apply_configuration_files(&system)
                        .context("Failed to apply configuration files")?;
                    ProjectDatabase::new(metadata, system)
                        .context("Failed to create project database")
                });

            // TODO(micha): Handle the case where the program settings are incorrect more gracefully.
            // The easiest is to ignore those projects but to show a message to the user that we do so.
            // Ignoring the projects has the effect that we'll use the default project for those files.
            // The only challenge with this is that we need to register the project when the configuration
            // becomes valid again. But that's a case we need to handle anyway for good mono repository support.
            match project {
                Ok(project) => {
                    self.projects.insert(root, project);
                }
                Err(err) => {
                    tracing::warn!("Failed to create project database for `{root}`: {err}",);
                }
            }
        }

        assert!(
            self.workspaces.all_initialized(),
            "All workspaces should be initialized after calling `initialize_workspaces`"
        );
    }

    pub(crate) fn take_deferred_messages(&mut self) -> Option<Message> {
        if self.workspaces.all_initialized() {
            self.deferred_messages.pop_front()
        } else {
            None
        }
    }

    /// Creates a document snapshot with the URL referencing the document to snapshot.
    pub(crate) fn take_document_snapshot(&self, url: Url) -> DocumentSnapshot {
        let index = self.index();
        DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            client_settings: index.global_settings(),
            position_encoding: self.position_encoding,
            document_query_result: self
                .key_from_url(url)
                .map_err(DocumentQueryError::InvalidUrl)
                .and_then(|key| index.make_document_ref(key)),
        }
    }

    /// Creates a snapshot of the current state of the [`Session`].
    pub(crate) fn take_session_snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            projects: self.projects.values().cloned().collect(),
            index: self.index.clone().unwrap(),
            position_encoding: self.position_encoding,
        }
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
    fn index(&self) -> &Index {
        self.index.as_ref().unwrap()
    }

    /// Returns a mutable reference to the index.
    ///
    /// This method drops all references to the index and returns a guard that will restore the
    /// references when dropped. This guard holds the only reference to the index and allows
    /// modifying it.
    fn index_mut(&mut self) -> MutIndexGuard {
        let index = self.index.take().unwrap();

        for db in self.projects_mut() {
            // Remove the `index` from each database. This drops the count of `Arc<Index>` down to 1
            db.system_mut()
                .as_any_mut()
                .downcast_mut::<LSPSystem>()
                .unwrap()
                .take_index();
        }

        // There should now be exactly one reference to index which is self.index.
        let index = Arc::into_inner(index).unwrap();

        MutIndexGuard {
            session: self,
            index: Some(index),
        }
    }

    pub(crate) fn client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    pub(crate) fn global_settings(&self) -> Arc<ClientSettings> {
        self.index().global_settings()
    }
}

/// A guard that holds the only reference to the index and allows modifying it.
///
/// When dropped, this guard restores all references to the index.
struct MutIndexGuard<'a> {
    session: &'a mut Session,
    index: Option<Index>,
}

impl Deref for MutIndexGuard<'_> {
    type Target = Index;

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
            for db in self.session.projects_mut() {
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

/// An immutable snapshot of [`Session`] that references a specific document.
#[derive(Debug)]
pub(crate) struct DocumentSnapshot {
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
    client_settings: Arc<ClientSettings>,
    position_encoding: PositionEncoding,
    document_query_result: Result<DocumentQuery, DocumentQueryError>,
}

impl DocumentSnapshot {
    /// Returns the resolved client capabilities that were captured during initialization.
    pub(crate) fn resolved_client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    /// Returns the position encoding that was negotiated during initialization.
    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    /// Returns the client settings for this document.
    pub(crate) fn client_settings(&self) -> &ClientSettings {
        &self.client_settings
    }

    /// Returns the result of the document query for this snapshot.
    pub(crate) fn document(&self) -> Result<&DocumentQuery, &DocumentQueryError> {
        self.document_query_result.as_ref()
    }

    pub(crate) fn file(&self, db: &dyn Db) -> Result<File, FileLookupError> {
        let document = match self.document() {
            Ok(document) => document,
            Err(err) => return Err(FileLookupError::DocumentQuery(err.clone())),
        };
        document
            .file(db)
            .ok_or_else(|| FileLookupError::NotFound(document.file_url().clone()))
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FileLookupError {
    #[error("file not found for url `{0}`")]
    NotFound(Url),
    #[error(transparent)]
    DocumentQuery(DocumentQueryError),
}

/// An immutable snapshot of the current state of [`Session`].
pub(crate) struct SessionSnapshot {
    projects: Vec<ProjectDatabase>,
    index: Arc<Index>,
    position_encoding: PositionEncoding,
}

impl SessionSnapshot {
    pub(crate) fn projects(&self) -> &[ProjectDatabase] {
        &self.projects
    }

    pub(crate) fn index(&self) -> &Index {
        &self.index
    }

    pub(crate) fn position_encoding(&self) -> PositionEncoding {
        self.position_encoding
    }
}

#[derive(Debug, Default)]
pub(crate) struct Workspaces {
    workspaces: BTreeMap<Url, Workspace>,
    uninitialized: usize,
}

impl Workspaces {
    pub(crate) fn register(&mut self, url: Url, options: ClientOptions) -> anyhow::Result<()> {
        let path = url
            .to_file_path()
            .map_err(|()| anyhow!("Workspace URL is not a file or directory: {url:?}"))?;

        // Realistically I don't think this can fail because we got the path from a Url
        let system_path = SystemPathBuf::from_path_buf(path)
            .map_err(|_| anyhow!("Workspace URL is not valid UTF8"))?;

        self.workspaces.insert(
            url,
            Workspace {
                options,
                root: system_path,
            },
        );

        self.uninitialized += 1;

        Ok(())
    }

    pub(crate) fn initialize(
        &mut self,
        url: &Url,
        options: ClientOptions,
    ) -> Option<&mut Workspace> {
        if let Some(workspace) = self.workspaces.get_mut(url) {
            workspace.options = options;
            self.uninitialized -= 1;
            Some(workspace)
        } else {
            None
        }
    }

    pub(crate) fn urls(&self) -> impl Iterator<Item = &Url> + '_ {
        self.workspaces.keys()
    }

    pub(crate) fn all_initialized(&self) -> bool {
        self.uninitialized == 0
    }
}

impl<'a> IntoIterator for &'a Workspaces {
    type Item = (&'a Url, &'a Workspace);
    type IntoIter = std::collections::btree_map::Iter<'a, Url, Workspace>;

    fn into_iter(self) -> Self::IntoIter {
        self.workspaces.iter()
    }
}

#[derive(Debug)]
pub(crate) struct Workspace {
    root: SystemPathBuf,
    options: ClientOptions,
}

impl Workspace {
    pub(crate) fn root(&self) -> &SystemPath {
        &self.root
    }
}
