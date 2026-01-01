//! Data model, state management, and configuration resolution.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::ops::{Deref, DerefMut};
use std::panic::RefUnwindSafe;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use lsp_server::{Message, RequestId};
use lsp_types::notification::{DidChangeWatchedFiles, Exit, Notification};
use lsp_types::request::{
    DocumentDiagnosticRequest, RegisterCapability, Request, Shutdown, UnregisterCapability,
    WorkspaceDiagnosticRequest,
};
use lsp_types::{
    DiagnosticRegistrationOptions, DiagnosticServerCapabilities,
    DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, Registration, RegistrationParams,
    TextDocumentContentChangeEvent, Unregistration, UnregistrationParams, Url,
};
use ruff_db::Db;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_python_ast::PySourceType;
use ty_combine::Combine;
use ty_project::metadata::Options;
use ty_project::watch::{ChangeEvent, CreatedKind};
use ty_project::{ChangeResult, CheckMode, Db as _, ProjectDatabase, ProjectMetadata};

use index::DocumentError;
use options::GlobalOptions;
use ty_python_semantic::MisconfigurationMode;

pub(crate) use self::options::InitializationOptions;
pub use self::options::{ClientOptions, DiagnosticMode, WorkspaceOptions};
pub(crate) use self::settings::{GlobalSettings, WorkspaceSettings};
use crate::capabilities::{ResolvedClientCapabilities, server_diagnostic_options};
use crate::document::{DocumentKey, DocumentVersion, NotebookDocument};
use crate::server::{Action, publish_settings_diagnostics};
use crate::session::client::Client;
use crate::session::index::Document;
use crate::session::request_queue::RequestQueue;
use crate::system::{AnySystemPath, LSPSystem};
use crate::{PositionEncoding, TextDocument};
use index::Index;

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

    /// Initialization options that were provided by the client during server initialization.
    initialization_options: InitializationOptions,

    /// Resolved global settings that are shared across all workspaces.
    global_settings: Arc<GlobalSettings>,

    /// The global position encoding, negotiated during LSP initialization.
    position_encoding: PositionEncoding,

    /// Tracks what LSP features the client supports and doesn't support.
    resolved_client_capabilities: ResolvedClientCapabilities,

    /// Tracks the pending requests between client and server.
    request_queue: RequestQueue,

    /// Has the client requested the server to shutdown.
    shutdown_requested: bool,

    /// Whether the server has dynamically registered the diagnostic capability with the client.
    /// Is the connected client a `TestServer` instance.
    in_test: bool,

    deferred_messages: VecDeque<Message>,

    /// A revision counter. It gets incremented on every change to `Session` that
    /// could result in different workspace diagnostics.
    revision: u64,

    /// A pending workspace diagnostics request because there were no diagnostics
    /// or no changes when when the request ran last time.
    /// We'll re-run the request after every change to `Session` (see `revision`)
    /// to see if there are now changes and, if so, respond to the client.
    suspended_workspace_diagnostics_request: Option<SuspendedWorkspaceDiagnosticRequest>,

    /// Registrations is a set of LSP methods that have been dynamically registered with the
    /// client.
    registrations: HashSet<String>,
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
        resolved_client_capabilities: ResolvedClientCapabilities,
        position_encoding: PositionEncoding,
        workspace_urls: Vec<Url>,
        initialization_options: InitializationOptions,
        native_system: Arc<dyn System + 'static + Send + Sync + RefUnwindSafe>,
        in_test: bool,
    ) -> crate::Result<Self> {
        let index = Arc::new(Index::new());

        let mut workspaces = Workspaces::default();
        // Register workspaces with default settings - they'll be initialized with real settings
        // when workspace/configuration response is received
        for url in workspace_urls {
            workspaces.register(url)?;
        }

        Ok(Self {
            native_system,
            position_encoding,
            workspaces,
            deferred_messages: VecDeque::new(),
            index: Some(index),
            default_project: DefaultProject::new(),
            initialization_options,
            global_settings: Arc::new(GlobalSettings::default()),
            projects: BTreeMap::new(),
            resolved_client_capabilities,
            request_queue: RequestQueue::new(),
            shutdown_requested: false,
            in_test,
            suspended_workspace_diagnostics_request: None,
            revision: 0,
            registrations: HashSet::new(),
        })
    }

    pub(crate) fn request_queue(&self) -> &RequestQueue {
        &self.request_queue
    }

    pub(crate) fn request_queue_mut(&mut self) -> &mut RequestQueue {
        &mut self.request_queue
    }

    pub(crate) fn initialization_options(&self) -> &InitializationOptions {
        &self.initialization_options
    }

    pub(crate) fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    pub(crate) fn set_shutdown_requested(&mut self, requested: bool) {
        self.shutdown_requested = requested;
    }

    pub(crate) fn set_suspended_workspace_diagnostics_request(
        &mut self,
        request: SuspendedWorkspaceDiagnosticRequest,
        client: &Client,
    ) {
        self.suspended_workspace_diagnostics_request = Some(request);
        // Run the suspended workspace diagnostic request immediately in case there
        // were changes since the workspace diagnostics background thread queued
        // the action to suspend the workspace diagnostic request.
        self.resume_suspended_workspace_diagnostic_request(client);
    }

    pub(crate) fn take_suspended_workspace_diagnostic_request(
        &mut self,
    ) -> Option<SuspendedWorkspaceDiagnosticRequest> {
        self.suspended_workspace_diagnostics_request.take()
    }

    /// Resumes (retries) the workspace diagnostic request if there
    /// were any changes to the [`Session`] (the revision got bumped)
    /// since the workspace diagnostic request ran last time.
    ///
    /// The workspace diagnostic requests is ignored if the request
    /// was cancelled in the meantime.
    pub(crate) fn resume_suspended_workspace_diagnostic_request(&mut self, client: &Client) {
        self.suspended_workspace_diagnostics_request = self
            .suspended_workspace_diagnostics_request
            .take()
            .and_then(|request| {
                if !self.request_queue.incoming().is_pending(&request.id) {
                    // Clear out the suspended request if the request has been cancelled.
                    tracing::debug!("Skipping suspended workspace diagnostics request `{}` because it was cancelled", request.id);
                    return None;
                }

                request.resume_if_revision_changed(self.revision, client)
            });
    }

    /// Bumps the revision.
    ///
    /// The revision is used to track when workspace diagnostics may have changed and need to be re-run.
    /// It's okay if a bump doesn't necessarily result in new workspace diagnostics.
    ///
    /// In general, any change to a project database should bump the revision and so should
    /// any change to the document states (but also when the open workspaces change etc.).
    fn bump_revision(&mut self) {
        self.revision += 1;
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

    /// Returns an iterator, in arbitrary order, over all project databases
    /// in this session.
    pub(crate) fn project_dbs(&self) -> impl Iterator<Item = &ProjectDatabase> {
        self.projects
            .values()
            .map(|project_state| &project_state.db)
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

        self.bump_revision();

        self.project_db_mut(path)
            .apply_changes(changes, overrides.as_ref())
    }

    /// Returns a mutable iterator over all project databases that have been initialized to this point.
    ///
    /// This iterator will only yield the default project database if it has been used.
    pub(crate) fn projects_mut(&mut self) -> impl Iterator<Item = &'_ mut ProjectDatabase> + '_ {
        self.project_states_mut().map(|project| &mut project.db)
    }

    /// Returns a mutable iterator over all projects that have been initialized to this point.
    ///
    /// This iterator will only yield the default project if it has been used.
    pub(crate) fn project_states_mut(&mut self) -> impl Iterator<Item = &'_ mut ProjectState> + '_ {
        let default_project = self.default_project.try_get_mut();
        self.projects.values_mut().chain(default_project)
    }

    pub(crate) fn initialize_workspaces(
        &mut self,
        workspace_settings: Vec<(Url, ClientOptions)>,
        client: &Client,
    ) {
        assert!(!self.workspaces.all_initialized());

        // These are the options combined from all the global options received by the server for
        // each workspace via the workspace configuration request.
        let mut combined_global_options: Option<GlobalOptions> = None;

        for (url, options) in workspace_settings {
            // Combine the global options specified during initialization with the
            // workspace-specific options to create the final workspace options.
            let ClientOptions {
                global, workspace, ..
            } = self
                .initialization_options
                .options
                .clone()
                .combine(options.clone());

            tracing::debug!("Initializing workspace `{url}`: {workspace:#?}");

            let unknown_options = &options.unknown;
            if !unknown_options.is_empty() {
                warn_about_unknown_options(client, Some(&url), unknown_options);
            }

            combined_global_options.combine_with(Some(global));

            let Ok(root) = url.to_file_path() else {
                tracing::debug!("Ignoring workspace with non-path root: {url}");
                continue;
            };

            // Realistically I don't think this can fail because we got the path from a Url
            let root = match SystemPathBuf::from_path_buf(root) {
                Ok(root) => root,
                Err(root) => {
                    tracing::debug!(
                        "Ignoring workspace with non-UTF8 root: {root}",
                        root = root.display()
                    );
                    continue;
                }
            };

            let workspace_settings = workspace.into_settings(&root, client);
            let Some(workspace) = self.workspaces.initialize(&root, workspace_settings) else {
                continue;
            };

            // For now, create one project database per workspace.
            // In the future, index the workspace directories to find all projects
            // and create a project database for each.
            let system = LSPSystem::new(
                self.index.as_ref().unwrap().clone(),
                self.native_system.clone(),
            );

            let configuration_file = workspace
                .settings
                .project_options_overrides()
                .and_then(|settings| settings.config_file_override.as_ref());

            let metadata = if let Some(configuration_file) = configuration_file {
                ProjectMetadata::from_config_file(configuration_file.clone(), &root, &system)
            } else {
                ProjectMetadata::discover(&root, &system)
            };

            let project = metadata
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
                Ok(db) => (root, db),
                Err(err) => {
                    tracing::error!(
                        "Failed to create project for workspace `{url}`: {err:#}. \
                        Falling back to default settings"
                    );

                    client.show_error_message(format!(
                        "Failed to load project for workspace {url}. \
                        Please refer to the logs for more details.",
                    ));

                    let db_with_default_settings = ProjectMetadata::from_options(
                        Options::default(),
                        root,
                        None,
                        MisconfigurationMode::UseDefault,
                    )
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

        if let Some(global_options) = combined_global_options {
            let global_settings = global_options.into_settings();
            if global_settings.diagnostic_mode().is_workspace() {
                for project in self.projects.values_mut() {
                    project.db.set_check_mode(CheckMode::AllFiles);
                }
            }
            self.global_settings = Arc::new(global_settings);
        }

        self.register_capabilities(client);

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

    /// Registers the dynamic capabilities with the client as per the resolved global settings.
    ///
    /// ## Diagnostic capability
    ///
    /// This capability is used to enable / disable workspace diagnostics as per the
    /// `ty.diagnosticMode` global setting.
    ///
    /// ## Rename capability
    ///
    /// This capability is used to enable / disable rename functionality as per the
    /// `ty.experimental.rename` global setting.
    fn register_capabilities(&mut self, client: &Client) {
        static DIAGNOSTIC_REGISTRATION_ID: &str = "ty/textDocument/diagnostic";
        static FILE_WATCHER_REGISTRATION_ID: &str = "ty/workspace/didChangeWatchedFiles";

        let mut registrations = vec![];
        let mut unregistrations = vec![];

        if self
            .resolved_client_capabilities
            .supports_diagnostic_dynamic_registration()
        {
            if self
                .registrations
                .contains(DocumentDiagnosticRequest::METHOD)
            {
                unregistrations.push(Unregistration {
                    id: DIAGNOSTIC_REGISTRATION_ID.into(),
                    method: DocumentDiagnosticRequest::METHOD.into(),
                });
            }

            let diagnostic_mode = self.global_settings.diagnostic_mode;

            match diagnostic_mode {
                DiagnosticMode::Off => {
                    tracing::debug!(
                        "Skipping registration of diagnostic capability because diagnostics are turned off"
                    );
                }
                DiagnosticMode::OpenFilesOnly | DiagnosticMode::Workspace => {
                    tracing::debug!(
                        "Registering diagnostic capability with {diagnostic_mode:?} diagnostic mode"
                    );
                    registrations.push(Registration {
                        id: DIAGNOSTIC_REGISTRATION_ID.into(),
                        method: DocumentDiagnosticRequest::METHOD.into(),
                        register_options: Some(
                            serde_json::to_value(
                                DiagnosticServerCapabilities::RegistrationOptions(
                                    DiagnosticRegistrationOptions {
                                        diagnostic_options: server_diagnostic_options(
                                            diagnostic_mode.is_workspace(),
                                        ),
                                        ..Default::default()
                                    },
                                ),
                            )
                            .unwrap(),
                        ),
                    });
                }
            }
        }

        if let Some(register_options) = self.file_watcher_registration_options() {
            if self.registrations.contains(DidChangeWatchedFiles::METHOD) {
                unregistrations.push(Unregistration {
                    id: FILE_WATCHER_REGISTRATION_ID.into(),
                    method: DidChangeWatchedFiles::METHOD.into(),
                });
            }
            registrations.push(Registration {
                id: FILE_WATCHER_REGISTRATION_ID.into(),
                method: DidChangeWatchedFiles::METHOD.into(),
                register_options: Some(serde_json::to_value(register_options).unwrap()),
            });
        }

        // First, unregister any existing capabilities and then register or re-register them.
        self.unregister_dynamic_capability(client, unregistrations);
        self.register_dynamic_capability(client, registrations);
    }

    /// Registers a list of dynamic capabilities with the client.
    fn register_dynamic_capability(&mut self, client: &Client, registrations: Vec<Registration>) {
        if registrations.is_empty() {
            return;
        }

        for registration in &registrations {
            self.registrations.insert(registration.method.clone());
        }

        client.send_request::<RegisterCapability>(
            self,
            RegistrationParams { registrations },
            |_: &Client, ()| {
                tracing::debug!("Registered dynamic capabilities");
            },
        );
    }

    /// Unregisters a list of dynamic capabilities with the client.
    fn unregister_dynamic_capability(
        &mut self,
        client: &Client,
        unregistrations: Vec<Unregistration>,
    ) {
        if unregistrations.is_empty() {
            return;
        }

        for unregistration in &unregistrations {
            if !self.registrations.remove(&unregistration.method) {
                tracing::debug!(
                    "Unregistration for `{}` was requested, but it was not registered",
                    unregistration.method
                );
            }
        }

        client.send_request::<UnregisterCapability>(
            self,
            UnregistrationParams {
                unregisterations: unregistrations,
            },
            |_: &Client, ()| {
                tracing::debug!("Unregistered dynamic capabilities");
            },
        );
    }

    /// Try to register the file watcher provided by the client if the client supports it.
    ///
    /// Note that this should be called *after* workspaces/projects have been initialized.
    /// This is required because the globs we use for registering file watching take
    /// project search paths into account.
    fn file_watcher_registration_options(
        &self,
    ) -> Option<DidChangeWatchedFilesRegistrationOptions> {
        fn make_watcher(glob: &str) -> FileSystemWatcher {
            FileSystemWatcher {
                glob_pattern: lsp_types::GlobPattern::String(glob.into()),
                kind: Some(lsp_types::WatchKind::all()),
            }
        }

        fn make_relative_watcher(relative_to: &SystemPath, glob: &str) -> FileSystemWatcher {
            let base_uri = Url::from_file_path(relative_to.as_std_path())
                .expect("system path must be a valid URI");
            let glob_pattern = lsp_types::GlobPattern::Relative(lsp_types::RelativePattern {
                base_uri: lsp_types::OneOf::Right(base_uri),
                pattern: glob.to_string(),
            });
            FileSystemWatcher {
                glob_pattern,
                kind: Some(lsp_types::WatchKind::all()),
            }
        }

        if !self.client_capabilities().supports_file_watcher() {
            tracing::warn!(
                "Your LSP client doesn't support file watching: \
                 You may see stale results when files change outside the editor"
            );
            return None;
        }

        // We also want to watch everything in the search paths as
        // well. But this seems to require "relative" watcher support.
        // I had trouble getting this working without using a base uri.
        //
        // Specifically, I tried this for each search path:
        //
        //     make_watcher(&format!("{path}/**"))
        //
        // But while this seemed to work for the project root, it
        // simply wouldn't result in any file notifications for changes
        // to files outside of the project root.
        let watchers = if !self.client_capabilities().supports_relative_file_watcher() {
            tracing::warn!(
                "Your LSP client doesn't support file watching outside of project: \
                 You may see stale results when dependencies change"
            );
            // Initialize our list of watchers with the standard globs relative
            // to the project root if we can't use relative globs.
            vec![make_watcher("**")]
        } else {
            // Gather up all of our project roots and all of the corresponding
            // project root system paths, then deduplicate them relative to
            // one another. Then listen to everything.
            let roots = self.project_dbs().map(|db| db.project().root(db));
            let paths = self
                .project_dbs()
                .flat_map(|db| {
                    ty_module_resolver::system_module_search_paths(db).map(move |path| (db, path))
                })
                .filter(|(db, path)| !path.starts_with(db.project().root(*db)))
                .map(|(_, path)| path)
                .chain(roots);
            ruff_db::system::deduplicate_nested_paths(paths)
                .map(|path| make_relative_watcher(path, "**"))
                .collect()
        };
        Some(DidChangeWatchedFilesRegistrationOptions { watchers })
    }

    /// Creates a document snapshot with the URL referencing the document to snapshot.
    pub(crate) fn snapshot_document(&self, url: &Url) -> Result<DocumentSnapshot, DocumentError> {
        let index = self.index();
        let document_handle = index.document_handle(url)?;

        Ok(DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities,
            global_settings: self.global_settings.clone(),
            workspace_settings: document_handle
                .notebook_or_file_path()
                .as_system()
                .and_then(|path| self.workspaces.settings_for_path(path))
                .unwrap_or_else(|| Arc::new(WorkspaceSettings::default())),
            position_encoding: self.position_encoding,
            document: document_handle,
        })
    }

    /// Creates a snapshot of the current state of the [`Session`].
    pub(crate) fn snapshot_session(&self) -> SessionSnapshot {
        SessionSnapshot {
            projects: self
                .projects
                .values()
                .map(|project| &project.db)
                .cloned()
                .collect(),
            index: self.index.clone().unwrap(),
            global_settings: self.global_settings.clone(),
            position_encoding: self.position_encoding,
            in_test: self.in_test,
            resolved_client_capabilities: self.resolved_client_capabilities,
            revision: self.revision,
        }
    }

    /// Iterates over the document keys for all open text documents.
    pub(super) fn text_document_handles(&self) -> impl Iterator<Item = DocumentHandle> + '_ {
        self.index()
            .text_documents()
            .map(|(_, document)| DocumentHandle::from_text_document(document))
    }

    /// Returns a handle to the document specified by its URL.
    ///
    /// # Errors
    ///
    /// If the document is not found.
    pub(crate) fn document_handle(
        &self,
        url: &lsp_types::Url,
    ) -> Result<DocumentHandle, DocumentError> {
        self.index().document_handle(url)
    }

    /// Registers a notebook document at the provided `path`.
    /// If a document is already open here, it will be overwritten.
    ///
    /// Returns a handle to the opened document.
    pub(crate) fn open_notebook_document(&mut self, document: NotebookDocument) -> DocumentHandle {
        let handle = self.index_mut().open_notebook_document(document);
        self.open_document_in_db(&handle);
        handle
    }

    /// Registers a text document at the provided `path`.
    /// If a document is already open here, it will be overwritten.
    ///
    /// Returns a handle to the opened document.
    pub(crate) fn open_text_document(&mut self, document: TextDocument) -> DocumentHandle {
        let handle = self.index_mut().open_text_document(document);
        self.open_document_in_db(&handle);
        handle
    }

    fn open_document_in_db(&mut self, document: &DocumentHandle) {
        let path = document.notebook_or_file_path();

        // This is a "maybe" because the `File` might've not been interned yet i.e., the
        // `try_system` call will return `None` which doesn't mean that the file is new, it's just
        // that the server didn't need the file yet.
        let is_maybe_new_system_file = path.as_system().is_some_and(|system_path| {
            let db = self.project_db(path);
            db.files()
                .try_system(db, system_path)
                .is_none_or(|file| !file.exists(db))
        });

        match path {
            AnySystemPath::System(system_path) => {
                let event = if is_maybe_new_system_file {
                    ChangeEvent::Created {
                        path: system_path.clone(),
                        kind: CreatedKind::File,
                    }
                } else {
                    ChangeEvent::Opened(system_path.clone())
                };
                self.apply_changes(path, vec![event]);

                let db = self.project_db_mut(path);
                match system_path_to_file(db, system_path) {
                    Ok(file) => db.project().open_file(db, file),
                    Err(err) => tracing::warn!("Failed to open file {system_path}: {err}"),
                }
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                let db = self.project_db_mut(path);
                let virtual_file = db.files().virtual_file(db, virtual_path);
                db.project().open_file(db, virtual_file.file());
            }
        }

        self.bump_revision();
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
    fn index_mut(&mut self) -> MutIndexGuard<'_> {
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

    pub(crate) fn global_settings(&self) -> &GlobalSettings {
        &self.global_settings
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
    global_settings: Arc<GlobalSettings>,
    workspace_settings: Arc<WorkspaceSettings>,
    position_encoding: PositionEncoding,
    document: DocumentHandle,
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

    /// Returns the client settings for all workspaces.
    pub(crate) fn global_settings(&self) -> &GlobalSettings {
        &self.global_settings
    }

    /// Returns the client settings for the workspace that this document belongs to.
    pub(crate) fn workspace_settings(&self) -> &WorkspaceSettings {
        &self.workspace_settings
    }

    /// Returns the result of the document query for this snapshot.
    pub(crate) fn document(&self) -> &DocumentHandle {
        &self.document
    }

    pub(crate) fn url(&self) -> &lsp_types::Url {
        self.document.url()
    }

    pub(crate) fn to_notebook_or_file(&self, db: &dyn Db) -> Option<File> {
        let file = self.document.notebook_or_file(db);
        if file.is_none() {
            tracing::debug!(
                "Failed to resolve file: file not found for `{}`",
                self.document.url()
            );
        }
        file
    }

    pub(crate) fn notebook_or_file_path(&self) -> &AnySystemPath {
        self.document.notebook_or_file_path()
    }
}

/// An immutable snapshot of the current state of [`Session`].
pub(crate) struct SessionSnapshot {
    index: Arc<Index>,
    global_settings: Arc<GlobalSettings>,
    position_encoding: PositionEncoding,
    resolved_client_capabilities: ResolvedClientCapabilities,
    in_test: bool,
    revision: u64,

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

    pub(crate) fn global_settings(&self) -> &GlobalSettings {
        &self.global_settings
    }

    pub(crate) fn position_encoding(&self) -> PositionEncoding {
        self.position_encoding
    }

    pub(crate) fn resolved_client_capabilities(&self) -> ResolvedClientCapabilities {
        self.resolved_client_capabilities
    }

    pub(crate) const fn in_test(&self) -> bool {
        self.in_test
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
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
    ///
    /// [`initialize`]: Workspaces::initialize
    pub(crate) fn register(&mut self, url: Url) -> anyhow::Result<()> {
        let path = url
            .to_file_path()
            .map_err(|()| anyhow!("Workspace URL is not a file or directory: {url:?}"))?;

        // Realistically I don't think this can fail because we got the path from a Url
        let system_path = SystemPathBuf::from_path_buf(path)
            .map_err(|_| anyhow!("Workspace URL is not valid UTF8"))?;

        self.workspaces.insert(
            system_path,
            Workspace {
                url,
                settings: Arc::new(WorkspaceSettings::default()),
            },
        );

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
        path: &SystemPath,
        settings: WorkspaceSettings,
    ) -> Option<&mut Workspace> {
        if let Some(workspace) = self.workspaces.get_mut(path) {
            workspace.settings = Arc::new(settings);
            self.uninitialized -= 1;
            Some(workspace)
        } else {
            None
        }
    }

    /// Returns a reference to the workspace for the given path, [`None`] if there's no workspace
    /// registered for the path.
    pub(crate) fn for_path(&self, path: impl AsRef<SystemPath>) -> Option<&Workspace> {
        self.workspaces
            .range(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db)
    }

    /// Returns the client settings for the workspace at the given path, [`None`] if there's no
    /// workspace registered for the path.
    pub(crate) fn settings_for_path(
        &self,
        path: impl AsRef<SystemPath>,
    ) -> Option<Arc<WorkspaceSettings>> {
        self.for_path(path).map(Workspace::settings_arc)
    }

    pub(crate) fn urls(&self) -> impl Iterator<Item = &Url> + '_ {
        self.workspaces.values().map(Workspace::url)
    }

    /// Returns `true` if all workspaces have been [initialized].
    ///
    /// [initialized]: Workspaces::initialize
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
    settings: Arc<WorkspaceSettings>,
}

impl Workspace {
    pub(crate) fn url(&self) -> &Url {
        &self.url
    }

    pub(crate) fn settings(&self) -> &WorkspaceSettings {
        &self.settings
    }

    pub(crate) fn settings_arc(&self) -> Arc<WorkspaceSettings> {
        self.settings.clone()
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
                MisconfigurationMode::UseDefault,
            )
            .unwrap();

            ProjectState {
                db: ProjectDatabase::new(metadata, system).unwrap(),
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

/// A workspace diagnostic request that didn't yield any changes or diagnostic
/// when it ran the last time.
#[derive(Debug)]
pub(crate) struct SuspendedWorkspaceDiagnosticRequest {
    /// The LSP request id
    pub(crate) id: RequestId,

    /// The params passed to the `workspace/diagnostic` request.
    pub(crate) params: serde_json::Value,

    /// The session's revision when the request ran the last time.
    ///
    /// This is to prevent races between:
    /// * The background thread completes
    /// * A did change notification coming in
    /// * storing this struct on `Session`
    ///
    /// The revision helps us detect that a did change notification
    /// happened in the meantime, so that we can reschedule the
    /// workspace diagnostic request immediately.
    pub(crate) revision: u64,
}

impl SuspendedWorkspaceDiagnosticRequest {
    fn resume_if_revision_changed(self, current_revision: u64, client: &Client) -> Option<Self> {
        if self.revision == current_revision {
            return Some(self);
        }

        tracing::debug!("Resuming workspace diagnostics request after revision bump");
        client.queue_action(Action::RetryRequest(lsp_server::Request {
            id: self.id,
            method: WorkspaceDiagnosticRequest::METHOD.to_string(),
            params: self.params,
        }));

        None
    }
}

/// A handle to a document stored within [`Index`].
///
/// Allows identifying the document within the index but it also carries the URL used by the
/// client to reference the document as well as the version of the document.
///
/// It also exposes methods to get the file-path of the corresponding ty-file.
#[derive(Clone, Debug)]
pub(crate) enum DocumentHandle {
    Text {
        url: lsp_types::Url,
        path: AnySystemPath,
        version: DocumentVersion,
    },
    Notebook {
        url: lsp_types::Url,
        path: AnySystemPath,
        version: DocumentVersion,
    },
    Cell {
        url: lsp_types::Url,
        version: DocumentVersion,
        notebook_path: AnySystemPath,
    },
}

impl DocumentHandle {
    fn from_text_document(document: &TextDocument) -> Self {
        match document.notebook() {
            None => Self::Text {
                version: document.version(),
                url: document.url().clone(),
                path: DocumentKey::from_url(document.url()).into_file_path(),
            },
            Some(notebook) => Self::Cell {
                notebook_path: notebook.clone(),
                version: document.version(),
                url: document.url().clone(),
            },
        }
    }

    fn from_notebook_document(document: &NotebookDocument) -> Self {
        Self::Notebook {
            path: DocumentKey::from_url(document.url()).into_file_path(),
            url: document.url().clone(),
            version: document.version(),
        }
    }

    fn from_document(document: &Document) -> Self {
        match document {
            Document::Text(text) => Self::from_text_document(text),
            Document::Notebook(notebook) => Self::from_notebook_document(notebook),
        }
    }

    fn key(&self) -> DocumentKey {
        DocumentKey::from_url(self.url())
    }

    pub(crate) const fn version(&self) -> DocumentVersion {
        match self {
            Self::Text { version, .. }
            | Self::Notebook { version, .. }
            | Self::Cell { version, .. } => *version,
        }
    }

    /// The URL as used by the client to reference this document.
    pub(crate) fn url(&self) -> &lsp_types::Url {
        match self {
            Self::Text { url, .. } | Self::Notebook { url, .. } | Self::Cell { url, .. } => url,
        }
    }

    /// The path to the enclosing file for this document.
    ///
    /// This is the path corresponding to the URL, except for notebook cells where the
    /// path corresponds to the notebook file.
    pub(crate) fn notebook_or_file_path(&self) -> &AnySystemPath {
        match self {
            Self::Text { path, .. } | Self::Notebook { path, .. } => path,
            Self::Cell { notebook_path, .. } => notebook_path,
        }
    }

    #[expect(unused)]
    pub(crate) fn file_path(&self) -> Option<&AnySystemPath> {
        match self {
            Self::Text { path, .. } | Self::Notebook { path, .. } => Some(path),
            Self::Cell { .. } => None,
        }
    }

    #[expect(unused)]
    pub(crate) fn notebook_path(&self) -> Option<&AnySystemPath> {
        match self {
            DocumentHandle::Notebook { path, .. } => Some(path),
            DocumentHandle::Cell { notebook_path, .. } => Some(notebook_path),
            DocumentHandle::Text { .. } => None,
        }
    }

    /// Returns the salsa interned [`File`] for the document selected by this query.
    ///
    /// It returns [`None`] for the following cases:
    /// - For virtual file, if it's not yet opened
    /// - For regular file, if it does not exists or is a directory
    pub(crate) fn notebook_or_file(&self, db: &dyn Db) -> Option<File> {
        match &self.notebook_or_file_path() {
            AnySystemPath::System(path) => system_path_to_file(db, path).ok(),
            AnySystemPath::SystemVirtual(virtual_path) => db
                .files()
                .try_virtual_file(virtual_path)
                .map(|virtual_file| virtual_file.file()),
        }
    }

    pub(crate) fn is_cell(&self) -> bool {
        matches!(self, Self::Cell { .. })
    }

    pub(crate) fn is_cell_or_notebook(&self) -> bool {
        matches!(self, Self::Cell { .. } | Self::Notebook { .. })
    }

    pub(crate) fn update_text_document(
        &mut self,
        session: &mut Session,
        content_changes: Vec<TextDocumentContentChangeEvent>,
        new_version: DocumentVersion,
    ) -> crate::Result<()> {
        let position_encoding = session.position_encoding();
        {
            let mut index = session.index_mut();

            let document_mut = index.document_mut(&self.key())?;

            let Some(document) = document_mut.as_text_mut() else {
                anyhow::bail!("Text document path does not point to a text document");
            };

            if content_changes.is_empty() {
                document.update_version(new_version);
            } else {
                document.apply_changes(content_changes, new_version, position_encoding);
            }

            self.set_version(document.version());
        }

        self.update_in_db(session);

        Ok(())
    }

    pub(crate) fn update_notebook_document(
        &mut self,
        session: &mut Session,
        cells: Option<lsp_types::NotebookDocumentCellChange>,
        metadata: Option<lsp_types::LSPObject>,
        new_version: DocumentVersion,
    ) -> crate::Result<()> {
        let position_encoding = session.position_encoding();
        {
            let mut index = session.index_mut();

            index.update_notebook_document(
                &self.key(),
                cells,
                metadata,
                new_version,
                position_encoding,
            )?;

            self.set_version(new_version);
        }

        self.update_in_db(session);
        Ok(())
    }

    fn update_in_db(&self, session: &mut Session) {
        let path = self.notebook_or_file_path();
        let changes = match path {
            AnySystemPath::System(system_path) => {
                vec![ChangeEvent::file_content_changed(system_path.clone())]
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                vec![ChangeEvent::ChangedVirtual(virtual_path.clone())]
            }
        };

        session.apply_changes(path, changes);
    }

    fn set_version(&mut self, version: DocumentVersion) {
        let self_version = match self {
            DocumentHandle::Text { version, .. }
            | DocumentHandle::Notebook { version, .. }
            | DocumentHandle::Cell { version, .. } => version,
        };

        *self_version = version;
    }

    /// De-registers a document, specified by its key.
    /// Calling this multiple times for the same document is a logic error.
    ///
    /// Returns `true` if the client needs to clear the diagnostics for this document.
    pub(crate) fn close(&self, session: &mut Session) -> crate::Result<bool> {
        let is_cell = self.is_cell();
        let path = self.notebook_or_file_path();

        let removed_document = session.index_mut().close_document(&self.key())?;

        // Close the text or notebook file in the database but skip this
        // step for cells because closing a cell doesn't close its notebook.
        let requires_clear_diagnostics = if is_cell {
            true
        } else {
            let db = session.project_db_mut(path);

            match path {
                AnySystemPath::System(system_path) => {
                    if let Some(file) = db.files().try_system(db, system_path) {
                        db.project().close_file(db, file);

                        // In case we preferred the language given by the Client
                        // over the one detected by the file extension, remove the file
                        // from the project to handle cases where a user changes the language
                        // of a file (which results in a didClose and didOpen for the same path but with different languages).
                        if removed_document.language_id().is_some()
                            && system_path
                                .extension()
                                .and_then(PySourceType::try_from_extension)
                                .is_none()
                        {
                            db.project().remove_file(db, file);
                        }
                    } else {
                        // This can only fail when the path is a directory or it doesn't exists but the
                        // file should exists for this handler in this branch. This is because every
                        // close call is preceded by an open call, which ensures that the file is
                        // interned in the lookup table (`Files`).
                        tracing::warn!("Salsa file does not exists for {}", system_path);
                    }

                    // For non-virtual files, we clear diagnostics if:
                    //
                    // 1. The file does not belong to any workspace e.g., opening a random file from
                    //    outside the workspace because closing it acts like the file doesn't exists
                    // 2. The diagnostic mode is set to open-files only
                    session.workspaces().for_path(system_path).is_none()
                        || session
                            .global_settings()
                            .diagnostic_mode()
                            .is_open_files_only()
                }
                AnySystemPath::SystemVirtual(virtual_path) => {
                    if let Some(virtual_file) = db.files().try_virtual_file(virtual_path) {
                        db.project().close_file(db, virtual_file.file());
                        virtual_file.close(db);
                    } else {
                        tracing::warn!("Salsa virtual file does not exists for {}", virtual_path);
                    }

                    // Always clear diagnostics for virtual files, as they don't really exist on disk
                    // which means closing them is like deleting the file.
                    true
                }
            }
        };

        session.bump_revision();

        Ok(requires_clear_diagnostics)
    }
}

/// Warns about unknown options received by the server.
///
/// If `workspace_url` is `Some`, it indicates that the unknown options were received during a
/// workspace initialization, otherwise they were received during the server initialization.
pub(super) fn warn_about_unknown_options(
    client: &Client,
    workspace_url: Option<&Url>,
    unknown_options: &HashMap<String, serde_json::Value>,
) {
    let message = if let Some(workspace_url) = workspace_url {
        format!(
            "Received unknown options for workspace `{workspace_url}`: {}",
            serde_json::to_string_pretty(unknown_options)
                .unwrap_or_else(|_| format!("{unknown_options:?}"))
        )
    } else {
        format!(
            "Received unknown options during initialization: {}",
            serde_json::to_string_pretty(unknown_options)
                .unwrap_or_else(|_| format!("{unknown_options:?}"))
        )
    };
    tracing::warn!("{message}");
    client.show_warning_message(message);
}
