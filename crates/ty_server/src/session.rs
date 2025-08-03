//! Data model, state management, and configuration resolution.

use anyhow::{Context, anyhow};
use index::DocumentQueryError;
use lsp_server::Message;
use lsp_types::notification::{Exit, Notification};
use lsp_types::request::{Request, Shutdown};
use lsp_types::{ClientCapabilities, TextDocumentContentChangeEvent, Url};
use options::GlobalOptions;
use ruff_db::Db;
use ruff_db::files::File;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use std::collections::{BTreeMap, VecDeque};
use std::ops::{Deref, DerefMut};
use std::panic::RefUnwindSafe;
use std::sync::Arc;
use ty_project::metadata::Options;
use ty_project::watch::ChangeEvent;
use ty_project::{ChangeResult, Db as _, ProjectDatabase, ProjectMetadata};

pub(crate) use self::capabilities::ResolvedClientCapabilities;
pub(crate) use self::index::DocumentQuery;
pub(crate) use self::options::AllOptions;
pub use self::options::{ClientOptions, DiagnosticMode};
pub(crate) use self::settings::ClientSettings;
use crate::document::{DocumentKey, DocumentVersion, NotebookDocument};
use crate::server::publish_settings_diagnostics;
use crate::session::client::Client;
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
pub(crate) struct Session {
    /// A native system to use with the [`LSPSystem`].
    native_system: Arc<dyn System + 'static + Send + Sync + RefUnwindSafe>,

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
    projects: BTreeMap<SystemPathBuf, ProjectState>,

    /// The project to use for files outside any workspace. For example, if the user
    /// opens the project `<home>/my_project` in VS code but they then opens a Python file from their Desktop.
    /// This file isn't part of the active workspace, nor is it part of any project. But we still want
    /// to provide some basic functionality like navigation, completions, syntax highlighting, etc.
    /// That's what we use the default project for.
    default_project: DefaultProject,

    /// The global position encoding, negotiated during LSP initialization.
    position_encoding: PositionEncoding,

    /// Tracks what LSP features the client supports and doesn't support.
    resolved_client_capabilities: ResolvedClientCapabilities,

    /// Tracks the pending requests between client and server.
    request_queue: RequestQueue,

    /// Has the client requested the server to shutdown.
    shutdown_requested: bool,

    deferred_messages: VecDeque<Message>,
}

/// LSP State for a Project
pub(crate) struct ProjectState {
    /// Files that we have outstanding otherwise-untracked pushed diagnostics for.
    ///
    /// In `CheckMode::OpenFiles` we still read some files that the client hasn't
    /// told us to open. Notably settings files like `pyproject.toml`. In this
    /// mode the client will never pull diagnostics for that file, and because
    /// the file isn't formally "open" we also don't have a reliable signal to
    /// refresh diagnostics for it either.
    ///
    /// However diagnostics for those files include things like "you typo'd your
    /// configuration for the LSP itself", so it's really important that we tell
    /// the user about them! So we remember which ones we have emitted diagnostics
    /// for so that we can clear the diagnostics for all of them before we go
    /// to update any of them.
    pub(crate) untracked_files_with_pushed_diagnostics: Vec<Url>,

    // Note: This field should be last to ensure the `db` gets dropped last.
    // The db drop order matters because we call `Arc::into_inner` on some Arc's
    // and we use Salsa's cancellation to guarantee that there's only a single reference to the `Arc`.
    // However, this requires that the db drops last.
    // This shouldn't matter here because the db's stored in the session are the
    // only reference we want to hold on, but better be safe than sorry ;).
    pub(crate) db: ProjectDatabase,
}

impl Session {
    pub(crate) fn new(
        client_capabilities: &ClientCapabilities,
        position_encoding: PositionEncoding,
        global_options: GlobalOptions,
        workspace_folders: Vec<(Url, ClientOptions)>,
        native_system: Arc<dyn System + 'static + Send + Sync + RefUnwindSafe>,
    ) -> crate::Result<Self> {
        let index = Arc::new(Index::new(global_options.into_settings()));

        let mut workspaces = Workspaces::default();
        for (url, workspace_options) in workspace_folders {
            workspaces.register(url, workspace_options.into_settings())?;
        }

        Ok(Self {
            native_system,
            position_encoding,
            workspaces,
            deferred_messages: VecDeque::new(),
            index: Some(index),
            default_project: DefaultProject::new(),
            projects: BTreeMap::new(),
            resolved_client_capabilities: ResolvedClientCapabilities::new(client_capabilities),
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
                    if request.method == Shutdown::METHOD {
                        return Some(message);
                    }
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
                    if notification.method == Exit::METHOD {
                        return Some(message);
                    }
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

    /// Returns a reference to the project's [`ProjectDatabase`] in which the given `path` belongs.
    ///
    /// If the path is a system path, it will return the project database that is closest to the
    /// given path, or the default project if no project is found for the path.
    ///
    /// If the path is a virtual path, it will return the first project database in the session.
    pub(crate) fn project_db(&self, path: &AnySystemPath) -> &ProjectDatabase {
        &self.project_state(path).db
    }

    /// Returns a mutable reference to the project's [`ProjectDatabase`] in which the given `path`
    /// belongs.
    ///
    /// Refer to [`project_db`] for more details on how the project is selected.
    ///
    /// [`project_db`]: Session::project_db
    pub(crate) fn project_db_mut(&mut self, path: &AnySystemPath) -> &mut ProjectDatabase {
        &mut self.project_state_mut(path).db
    }

    /// Returns a reference to the project's [`ProjectDatabase`] corresponding to the given path, if
    /// any.
    pub(crate) fn project_db_for_path(
        &self,
        path: impl AsRef<SystemPath>,
    ) -> Option<&ProjectDatabase> {
        self.project_state_for_path(path).map(|state| &state.db)
    }

    /// Returns a reference to the project's [`ProjectState`] in which the given `path` belongs.
    ///
    /// If the path is a system path, it will return the project database that is closest to the
    /// given path, or the default project if no project is found for the path.
    ///
    /// If the path is a virtual path, it will return the first project database in the session.
    pub(crate) fn project_state(&self, path: &AnySystemPath) -> &ProjectState {
        match path {
            AnySystemPath::System(system_path) => {
                self.project_state_for_path(system_path).unwrap_or_else(|| {
                    self.default_project
                        .get(self.index.as_ref(), &self.native_system)
                })
            }
            AnySystemPath::SystemVirtual(_virtual_path) => {
                // TODO: Currently, ty only supports single workspace but we need to figure out
                // which project should this virtual path belong to when there are multiple
                // projects: https://github.com/astral-sh/ty/issues/794
                self.projects
                    .iter()
                    .next()
                    .map(|(_, project)| project)
                    .unwrap()
            }
        }
    }

    /// Returns a mutable reference to the project's [`ProjectState`] in which the given `path`
    /// belongs.
    ///
    /// Refer to [`project_db`] for more details on how the project is selected.
    ///
    /// [`project_db`]: Session::project_db
    pub(crate) fn project_state_mut(&mut self, path: &AnySystemPath) -> &mut ProjectState {
        match path {
            AnySystemPath::System(system_path) => self
                .projects
                .range_mut(..=system_path.to_path_buf())
                .next_back()
                .map(|(_, project)| project)
                .unwrap_or_else(|| {
                    self.default_project
                        .get_mut(self.index.as_ref(), &self.native_system)
                }),
            AnySystemPath::SystemVirtual(_virtual_path) => {
                // TODO: Currently, ty only supports single workspace but we need to figure out
                // which project should this virtual path belong to when there are multiple
                // projects: https://github.com/astral-sh/ty/issues/794
                self.projects
                    .iter_mut()
                    .next()
                    .map(|(_, project)| project)
                    .unwrap()
            }
        }
    }

    /// Returns a reference to the project's [`ProjectState`] corresponding to the given path, if
    /// any.
    pub(crate) fn project_state_for_path(
        &self,
        path: impl AsRef<SystemPath>,
    ) -> Option<&ProjectState> {
        self.projects
            .range(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, project)| project)
    }

    pub(crate) fn apply_changes(
        &mut self,
        path: &AnySystemPath,
        changes: Vec<ChangeEvent>,
    ) -> ChangeResult {
        let overrides = path.as_system().and_then(|root| {
            self.workspaces()
                .for_path(root)?
                .settings()
                .project_options_overrides()
                .cloned()
        });

        self.project_db_mut(path)
            .apply_changes(changes, overrides.as_ref())
    }

    /// Returns a mutable iterator over all project databases that have been initialized to this point.
    ///
    /// This iterator will only yield the default project database if it has been used.
    fn projects_mut(&mut self) -> impl Iterator<Item = &'_ mut ProjectDatabase> + '_ {
        self.project_states_mut().map(|project| &mut project.db)
    }

    /// Returns a mutable iterator over all projects that have been initialized to this point.
    ///
    /// This iterator will only yield the default project if it has been used.
    pub(crate) fn project_states_mut(&mut self) -> impl Iterator<Item = &'_ mut ProjectState> + '_ {
        let default_project = self.default_project.try_get_mut();
        self.projects.values_mut().chain(default_project)
    }

    /// Returns the [`DocumentKey`] for the given URL.
    ///
    /// Refer to [`Index::key_from_url`] for more details.
    pub(crate) fn key_from_url(&self, url: Url) -> Result<DocumentKey, Url> {
        self.index().key_from_url(url)
    }

    pub(crate) fn initialize_workspaces(
        &mut self,
        workspace_settings: Vec<(Url, ClientOptions)>,
        client: &Client,
    ) {
        assert!(!self.workspaces.all_initialized());
        for (url, options) in workspace_settings {
            tracing::debug!("Initializing workspace `{url}`");

            let settings = options.into_settings();
            let Some((root, workspace)) = self.workspaces.initialize(&url, settings) else {
                continue;
            };

            // For now, create one project database per workspace.
            // In the future, index the workspace directories to find all projects
            // and create a project database for each.
            let system = LSPSystem::new(
                self.index.as_ref().unwrap().clone(),
                self.native_system.clone(),
            );

            let project = ProjectMetadata::discover(&root, &system)
                .context("Failed to discover project configuration")
                .and_then(|mut metadata| {
                    metadata
                        .apply_configuration_files(&system)
                        .context("Failed to apply configuration files")?;

                    if let Some(overrides) = workspace.settings.project_options_overrides() {
                        metadata.apply_overrides(overrides);
                    }

                    ProjectDatabase::new(metadata, system.clone())
                });

            let (root, db) = match project {
                Ok(mut db) => {
                    db.set_check_mode(workspace.settings.diagnostic_mode().into_check_mode());
                    (root, db)
                }
                Err(err) => {
                    tracing::error!(
                        "Failed to create project for `{root}`: {err:#}. Falling back to default settings"
                    );

                    client.show_error_message(format!(
                        "Failed to load project rooted at {root}. Please refer to the logs for more details.",
                    ));

                    let db_with_default_settings =
                        ProjectMetadata::from_options(Options::default(), root, None)
                            .context("Failed to convert default options to metadata")
                            .and_then(|metadata| ProjectDatabase::new(metadata, system))
                            .expect("Default configuration to be valid");
                    let default_root = db_with_default_settings
                        .project()
                        .root(&db_with_default_settings)
                        .to_path_buf();

                    (default_root, db_with_default_settings)
                }
            };

            // Carry forward diagnostic state if any exists
            let previous = self.projects.remove(&root);
            let untracked = previous
                .map(|state| state.untracked_files_with_pushed_diagnostics)
                .unwrap_or_default();
            self.projects.insert(
                root.clone(),
                ProjectState {
                    db,
                    untracked_files_with_pushed_diagnostics: untracked,
                },
            );

            publish_settings_diagnostics(self, client, root);
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
            resolved_client_capabilities: self.resolved_client_capabilities,
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
            projects: self
                .projects
                .values()
                .map(|project| &project.db)
                .cloned()
                .collect(),
            index: self.index.clone().unwrap(),
            position_encoding: self.position_encoding,
            resolved_client_capabilities: self.resolved_client_capabilities,
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

    pub(crate) fn client_capabilities(&self) -> ResolvedClientCapabilities {
        self.resolved_client_capabilities
    }

    pub(crate) fn global_settings(&self) -> Arc<ClientSettings> {
        self.index().global_settings()
    }

    pub(crate) fn position_encoding(&self) -> PositionEncoding {
        self.position_encoding
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
    resolved_client_capabilities: ResolvedClientCapabilities,
    client_settings: Arc<ClientSettings>,
    position_encoding: PositionEncoding,
    document_query_result: Result<DocumentQuery, DocumentQueryError>,
}

impl DocumentSnapshot {
    /// Returns the resolved client capabilities that were captured during initialization.
    pub(crate) fn resolved_client_capabilities(&self) -> ResolvedClientCapabilities {
        self.resolved_client_capabilities
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

    pub(crate) fn file(&self, db: &dyn Db) -> Option<File> {
        let document = match self.document() {
            Ok(document) => document,
            Err(err) => {
                tracing::debug!("Failed to resolve file: {}", err);
                return None;
            }
        };
        let file = document.file(db);
        if file.is_none() {
            tracing::debug!(
                "Failed to resolve file: file not found for path `{}`",
                document.file_path()
            );
        }
        file
    }
}

/// An immutable snapshot of the current state of [`Session`].
pub(crate) struct SessionSnapshot {
    index: Arc<Index>,
    position_encoding: PositionEncoding,
    resolved_client_capabilities: ResolvedClientCapabilities,

    /// IMPORTANT: It's important that the databases come last, or at least,
    /// after any `Arc` that we try to extract or mutate in-place using `Arc::into_inner`
    /// and that relies on Salsa's cancellation to guarantee that there's now only a
    /// single reference to it (e.g. see [`Session::index_mut`]).
    ///
    /// Making this field come last guarantees that the db's `Drop` handler is
    /// dropped after all other fields, which ensures that
    /// Salsa's cancellation blocks until all fields are dropped (and not only
    /// waits for the db to be dropped while we still hold on to the `Index`).
    projects: Vec<ProjectDatabase>,
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

    pub(crate) fn resolved_client_capabilities(&self) -> ResolvedClientCapabilities {
        self.resolved_client_capabilities
    }
}

#[derive(Debug, Default)]
pub(crate) struct Workspaces {
    workspaces: BTreeMap<SystemPathBuf, Workspace>,
    uninitialized: usize,
}

impl Workspaces {
    /// Registers a new workspace with the given URL and default settings for the workspace.
    ///
    /// It's the caller's responsibility to later call [`initialize`] with the resolved settings
    /// for this workspace. Registering and initializing a workspace is a two-step process because
    /// the workspace are announced to the server during the `initialize` request, but the
    /// resolved settings are only available after the client has responded to the `workspace/configuration`
    /// request.
    pub(crate) fn register(&mut self, url: Url, settings: ClientSettings) -> anyhow::Result<()> {
        let path = url
            .to_file_path()
            .map_err(|()| anyhow!("Workspace URL is not a file or directory: {url:?}"))?;

        // Realistically I don't think this can fail because we got the path from a Url
        let system_path = SystemPathBuf::from_path_buf(path)
            .map_err(|_| anyhow!("Workspace URL is not valid UTF8"))?;

        self.workspaces
            .insert(system_path, Workspace { url, settings });

        self.uninitialized += 1;

        Ok(())
    }

    /// Initializes the workspace with the resolved client settings for the workspace.
    ///
    /// ## Returns
    ///
    /// `None` if URL doesn't map to a valid path or if the workspace is not registered.
    pub(crate) fn initialize(
        &mut self,
        url: &Url,
        settings: ClientSettings,
    ) -> Option<(SystemPathBuf, &mut Workspace)> {
        let path = url.to_file_path().ok()?;

        // Realistically I don't think this can fail because we got the path from a Url
        let system_path = SystemPathBuf::from_path_buf(path).ok()?;

        if let Some(workspace) = self.workspaces.get_mut(&system_path) {
            workspace.settings = settings;
            self.uninitialized -= 1;
            Some((system_path, workspace))
        } else {
            None
        }
    }

    pub(crate) fn for_path(&self, path: impl AsRef<SystemPath>) -> Option<&Workspace> {
        self.workspaces
            .range(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db)
    }

    pub(crate) fn urls(&self) -> impl Iterator<Item = &Url> + '_ {
        self.workspaces.values().map(Workspace::url)
    }

    pub(crate) fn all_initialized(&self) -> bool {
        self.uninitialized == 0
    }
}

impl<'a> IntoIterator for &'a Workspaces {
    type Item = (&'a SystemPathBuf, &'a Workspace);
    type IntoIter = std::collections::btree_map::Iter<'a, SystemPathBuf, Workspace>;

    fn into_iter(self) -> Self::IntoIter {
        self.workspaces.iter()
    }
}

#[derive(Debug)]
pub(crate) struct Workspace {
    /// The workspace root URL as sent by the client during initialization.
    url: Url,
    settings: ClientSettings,
}

impl Workspace {
    pub(crate) fn url(&self) -> &Url {
        &self.url
    }

    pub(crate) fn settings(&self) -> &ClientSettings {
        &self.settings
    }
}

/// Thin wrapper around the default project database that ensures it only gets initialized
/// when it's first accessed.
///
/// There are a few advantages to this:
///
/// 1. Salsa has a fast-path for query lookups for the first created database.
///    We really want that to be the actual project database and not our fallback database.
/// 2. The logs when the server starts can be confusing if it once shows it uses Python X (for the default db)
///    but then has another log that it uses Python Y (for the actual project db).
struct DefaultProject(std::sync::OnceLock<ProjectState>);

impl DefaultProject {
    pub(crate) fn new() -> Self {
        DefaultProject(std::sync::OnceLock::new())
    }

    pub(crate) fn get(
        &self,
        index: Option<&Arc<Index>>,
        fallback_system: &Arc<dyn System + 'static + Send + Sync + RefUnwindSafe>,
    ) -> &ProjectState {
        self.0.get_or_init(|| {
            tracing::info!("Initializing the default project");

            let index = index.unwrap();
            let system = LSPSystem::new(index.clone(), fallback_system.clone());
            let metadata = ProjectMetadata::from_options(
                Options::default(),
                system.current_directory().to_path_buf(),
                None,
            )
            .unwrap();

            let mut db = ProjectDatabase::new(metadata, system).unwrap();
            db.set_check_mode(index.global_settings().diagnostic_mode().into_check_mode());

            ProjectState {
                db,
                untracked_files_with_pushed_diagnostics: Vec::new(),
            }
        })
    }

    pub(crate) fn get_mut(
        &mut self,
        index: Option<&Arc<Index>>,
        fallback_system: &Arc<dyn System + 'static + Send + Sync + RefUnwindSafe>,
    ) -> &mut ProjectState {
        let _ = self.get(index, fallback_system);

        // SAFETY: The `OnceLock` is guaranteed to be initialized at this point because
        // we called `get` above, which initializes it if it wasn't already.
        self.0.get_mut().unwrap()
    }

    pub(crate) fn try_get_mut(&mut self) -> Option<&mut ProjectState> {
        self.0.get_mut()
    }
}
