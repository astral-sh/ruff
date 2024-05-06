//! Scheduling, I/O, and API endpoints.

use std::num::NonZeroUsize;

use lsp_server as lsp;
use lsp_types as types;
use types::ClientCapabilities;
use types::CodeActionKind;
use types::CodeActionOptions;
use types::DiagnosticOptions;
use types::DidChangeWatchedFilesRegistrationOptions;
use types::FileSystemWatcher;
use types::OneOf;
use types::TextDocumentSyncCapability;
use types::TextDocumentSyncKind;
use types::TextDocumentSyncOptions;
use types::WorkDoneProgressOptions;
use types::WorkspaceFoldersServerCapabilities;

use self::connection::Connection;
use self::connection::ConnectionInitializer;
use self::schedule::event_loop_thread;
use self::schedule::Scheduler;
use self::schedule::Task;
use crate::session::AllSettings;
use crate::session::ClientSettings;
use crate::session::Session;
use crate::PositionEncoding;

mod api;
mod client;
mod connection;
mod schedule;

pub(crate) use connection::ClientSender;

pub(crate) type Result<T> = std::result::Result<T, api::Error>;

pub struct Server {
    connection: Connection,
    client_capabilities: ClientCapabilities,
    worker_threads: NonZeroUsize,
    session: Session,
}

impl Server {
    pub fn new(worker_threads: NonZeroUsize) -> crate::Result<Self> {
        let connection = ConnectionInitializer::stdio();

        let (id, init_params) = connection.initialize_start()?;

        let client_capabilities = init_params.capabilities;
        let position_encoding = Self::find_best_position_encoding(&client_capabilities);
        let server_capabilities = Self::server_capabilities(position_encoding);

        let connection = connection.initialize_finish(
            id,
            &server_capabilities,
            crate::SERVER_NAME,
            crate::version(),
        )?;

        crate::message::init_messenger(connection.make_sender());

        let AllSettings {
            global_settings,
            mut workspace_settings,
        } = AllSettings::from_value(init_params.initialization_options.unwrap_or_default());

        let mut workspace_for_uri = |uri| {
            let Some(workspace_settings) = workspace_settings.as_mut() else {
                return (uri, ClientSettings::default());
            };
            let settings = workspace_settings.remove(&uri).unwrap_or_else(|| {
                tracing::warn!("No workspace settings found for {uri}");
                ClientSettings::default()
            });
            (uri, settings)
        };

        let workspaces = init_params
            .workspace_folders
            .map(|folders| folders.into_iter().map(|folder| {
                workspace_for_uri(folder.uri)
            }).collect())
            .or_else(|| {
                tracing::debug!("No workspace(s) were provided during initialization. Using the current working directory as a default workspace...");
                let uri = types::Url::from_file_path(std::env::current_dir().ok()?).ok()?;
                Some(vec![workspace_for_uri(uri)])
            })
            .ok_or_else(|| {
                anyhow::anyhow!("Failed to get the current working directory while creating a default workspace.")
            })?;

        Ok(Self {
            connection,
            worker_threads,
            session: Session::new(
                &client_capabilities,
                position_encoding,
                global_settings,
                workspaces,
            )?,
            client_capabilities,
        })
    }

    pub fn run(self) -> crate::Result<()> {
        event_loop_thread(move || {
            Self::event_loop(
                &self.connection,
                &self.client_capabilities,
                self.session,
                self.worker_threads,
            )?;
            self.connection.close()?;
            Ok(())
        })?
        .join()
    }

    #[allow(clippy::needless_pass_by_value)] // this is because we aren't using `next_request_id` yet.
    fn event_loop(
        connection: &Connection,
        client_capabilities: &ClientCapabilities,
        mut session: Session,
        worker_threads: NonZeroUsize,
    ) -> crate::Result<()> {
        let mut scheduler =
            schedule::Scheduler::new(&mut session, worker_threads, connection.make_sender());

        Self::try_register_capabilities(client_capabilities, &mut scheduler);
        for msg in connection.incoming() {
            if connection.handle_shutdown(&msg)? {
                break;
            }
            let task = match msg {
                lsp::Message::Request(req) => api::request(req),
                lsp::Message::Notification(notification) => api::notification(notification),
                lsp::Message::Response(response) => scheduler.response(response),
            };
            scheduler.dispatch(task);
        }

        Ok(())
    }

    fn try_register_capabilities(
        client_capabilities: &ClientCapabilities,
        scheduler: &mut Scheduler,
    ) {
        let dynamic_registration = client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.did_change_watched_files)
            .and_then(|watched_files| watched_files.dynamic_registration)
            .unwrap_or_default();
        if dynamic_registration {
            // Register all dynamic capabilities here

            // `workspace/didChangeWatchedFiles`
            // (this registers the configuration file watcher)
            let params = lsp_types::RegistrationParams {
                registrations: vec![lsp_types::Registration {
                    id: "ruff-server-watch".into(),
                    method: "workspace/didChangeWatchedFiles".into(),
                    register_options: Some(
                        serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                            watchers: vec![
                                FileSystemWatcher {
                                    glob_pattern: types::GlobPattern::String(
                                        "**/.?ruff.toml".into(),
                                    ),
                                    kind: None,
                                },
                                FileSystemWatcher {
                                    glob_pattern: types::GlobPattern::String(
                                        "**/pyproject.toml".into(),
                                    ),
                                    kind: None,
                                },
                            ],
                        })
                        .unwrap(),
                    ),
                }],
            };

            let response_handler = |()| {
                tracing::info!("Configuration file watcher successfully registered");
                Task::nothing()
            };

            if let Err(err) = scheduler
                .request::<lsp_types::request::RegisterCapability>(params, response_handler)
            {
                tracing::error!("An error occurred when trying to register the configuration file watcher: {err}");
            }
        } else {
            tracing::warn!("LSP client does not support dynamic capability registration - automatic configuration reloading will not be available.");
        }
    }

    fn find_best_position_encoding(client_capabilities: &ClientCapabilities) -> PositionEncoding {
        client_capabilities
            .general
            .as_ref()
            .and_then(|general_capabilities| general_capabilities.position_encodings.as_ref())
            .and_then(|encodings| {
                encodings
                    .iter()
                    .filter_map(|encoding| PositionEncoding::try_from(encoding).ok())
                    .max() // this selects the highest priority position encoding
            })
            .unwrap_or_default()
    }

    fn server_capabilities(position_encoding: PositionEncoding) -> types::ServerCapabilities {
        types::ServerCapabilities {
            position_encoding: Some(position_encoding.into()),
            code_action_provider: Some(types::CodeActionProviderCapability::Options(
                CodeActionOptions {
                    code_action_kinds: Some(
                        SupportedCodeAction::all()
                            .map(SupportedCodeAction::to_kind)
                            .collect(),
                    ),
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: Some(true),
                    },
                    resolve_provider: Some(true),
                },
            )),
            workspace: Some(types::WorkspaceServerCapabilities {
                workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                    supported: Some(true),
                    change_notifications: Some(OneOf::Left(true)),
                }),
                file_operations: None,
            }),
            document_formatting_provider: Some(OneOf::Left(true)),
            document_range_formatting_provider: Some(OneOf::Left(true)),
            diagnostic_provider: Some(types::DiagnosticServerCapabilities::Options(
                DiagnosticOptions {
                    identifier: Some(crate::DIAGNOSTIC_NAME.into()),
                    // multi-file analysis could change this
                    inter_file_dependencies: false,
                    workspace_diagnostics: false,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: Some(true),
                    },
                },
            )),
            hover_provider: Some(types::HoverProviderCapability::Simple(true)),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    will_save: Some(false),
                    will_save_wait_until: Some(false),
                    ..Default::default()
                },
            )),
            ..Default::default()
        }
    }
}

/// The code actions we support.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SupportedCodeAction {
    /// Maps to the `quickfix` code action kind. Quick fix code actions are shown under
    /// their respective diagnostics. Quick fixes are only created where the fix applicability is
    /// at least [`ruff_diagnostics::Applicability::Unsafe`].
    QuickFix,
    /// Maps to the `source.fixAll` and `source.fixAll.ruff` code action kinds.
    /// This is a source action that applies all safe fixes to the currently open document.
    SourceFixAll,
    /// Maps to `source.organizeImports` and `source.organizeImports.ruff` code action kinds.
    /// This is a source action that applies import sorting fixes to the currently open document.
    #[allow(dead_code)] // TODO: remove
    SourceOrganizeImports,
}

impl SupportedCodeAction {
    /// Returns the LSP code action kind that map to this code action.
    fn to_kind(self) -> CodeActionKind {
        match self {
            Self::QuickFix => CodeActionKind::QUICKFIX,
            Self::SourceFixAll => crate::SOURCE_FIX_ALL_RUFF,
            Self::SourceOrganizeImports => crate::SOURCE_ORGANIZE_IMPORTS_RUFF,
        }
    }

    fn from_kind(kind: CodeActionKind) -> impl Iterator<Item = Self> {
        Self::all().filter(move |supported_kind| {
            supported_kind.to_kind().as_str().starts_with(kind.as_str())
        })
    }

    /// Returns all code actions kinds that the server currently supports.
    fn all() -> impl Iterator<Item = Self> {
        [
            Self::QuickFix,
            Self::SourceFixAll,
            Self::SourceOrganizeImports,
        ]
        .into_iter()
    }
}
