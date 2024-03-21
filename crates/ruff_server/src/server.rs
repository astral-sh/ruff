//! Scheduling, I/O, and API endpoints.

use std::num::NonZeroUsize;
use std::sync::atomic::AtomicI32;
use std::sync::atomic::Ordering;
use std::time::Duration;

use lsp::Connection;
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

use self::schedule::event_loop_thread;
use crate::session::Session;
use crate::PositionEncoding;

mod api;
mod client;
mod schedule;

pub(crate) type Result<T> = std::result::Result<T, api::Error>;

pub struct Server {
    conn: lsp::Connection,
    threads: lsp::IoThreads,
    worker_threads: NonZeroUsize,
    session: Session,
    next_request_id: AtomicI32,
}

impl Server {
    pub fn new(worker_threads: NonZeroUsize) -> crate::Result<Self> {
        let (conn, threads) = lsp::Connection::stdio();

        let (id, params) = conn.initialize_start()?;

        let init_params: types::InitializeParams = serde_json::from_value(params)?;

        let client_capabilities = init_params.capabilities;
        let server_capabilities = Self::server_capabilities(&client_capabilities);

        let dynamic_registration = client_capabilities
            .workspace
            .and_then(|workspace| workspace.did_change_watched_files)
            .and_then(|watched_files| watched_files.dynamic_registration)
            .unwrap_or_default();

        let workspaces = init_params
            .workspace_folders
            .map(|folders| folders.into_iter().map(|folder| folder.uri).collect())
            .or_else(|| init_params.root_uri.map(|u| vec![u]))
            .or_else(|| {
                tracing::debug!("No root URI or workspace(s) were provided during initialization. Using the current working directory as a default workspace...");
                Some(vec![types::Url::from_file_path(std::env::current_dir().ok()?).ok()?])
            })
            .ok_or_else(|| {
                anyhow::anyhow!("Failed to get the current working directory while creating a default workspace.")
            })?;

        let initialize_data = serde_json::json!({
            "capabilities": server_capabilities,
            "serverInfo": {
                "name": crate::SERVER_NAME,
                "version": crate::version()
            }
        });

        let next_request_id = AtomicI32::from(1);

        conn.initialize_finish(id, initialize_data)?;

        if dynamic_registration {
            // Register capabilities
            conn.sender
                .send(lsp_server::Message::Request(lsp_server::Request {
                    id: next_request_id.fetch_add(1, Ordering::Relaxed).into(),
                    method: "client/registerCapability".into(),
                    params: serde_json::to_value(lsp_types::RegistrationParams {
                        registrations: vec![lsp_types::Registration {
                            id: "ruff-server-watch".into(),
                            method: "workspace/didChangeWatchedFiles".into(),
                            register_options: Some(serde_json::to_value(
                                DidChangeWatchedFilesRegistrationOptions {
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
                                },
                            )?),
                        }],
                    })?,
                }))?;

            // Flush response from the client (to avoid an unexpected response appearing in the event loop)
            let _ = conn.receiver.recv_timeout(Duration::from_secs(5)).map_err(|_| {
                tracing::error!("Timed out while waiting for client to acknowledge registration of dynamic capabilities");
            });
        } else {
            tracing::warn!("LSP client does not support dynamic file watcher registration - automatic configuration reloading will not be available.");
        }

        Ok(Self {
            conn,
            threads,
            worker_threads,
            session: Session::new(&server_capabilities, &workspaces)?,
            next_request_id,
        })
    }

    pub fn run(self) -> crate::Result<()> {
        let result = event_loop_thread(move || {
            Self::event_loop(
                &self.conn,
                self.session,
                self.worker_threads,
                self.next_request_id,
            )
        })?
        .join();
        self.threads.join()?;
        result
    }

    #[allow(clippy::needless_pass_by_value)] // this is because we aren't using `next_request_id` yet.
    fn event_loop(
        connection: &Connection,
        session: Session,
        worker_threads: NonZeroUsize,
        _next_request_id: AtomicI32,
    ) -> crate::Result<()> {
        let mut scheduler = schedule::Scheduler::new(session, worker_threads, &connection.sender);
        for msg in &connection.receiver {
            let task = match msg {
                lsp::Message::Request(req) => {
                    if connection.handle_shutdown(&req)? {
                        return Ok(());
                    }
                    api::request(req)
                }
                lsp::Message::Notification(notification) => api::notification(notification),
                lsp::Message::Response(response) => {
                    tracing::error!(
                        "Expected request or notification, got response instead: {response:?}"
                    );
                    continue;
                }
            };
            scheduler.dispatch(task);
        }
        Ok(())
    }

    fn server_capabilities(client_capabilities: &ClientCapabilities) -> types::ServerCapabilities {
        let position_encoding = client_capabilities
            .general
            .as_ref()
            .and_then(|general_capabilities| general_capabilities.position_encodings.as_ref())
            .and_then(|encodings| {
                encodings
                    .iter()
                    .filter_map(|encoding| PositionEncoding::try_from(encoding).ok())
                    .max() // this selects the highest priority position encoding
            })
            .unwrap_or_default();
        types::ServerCapabilities {
            position_encoding: Some(position_encoding.into()),
            code_action_provider: Some(types::CodeActionProviderCapability::Options(
                CodeActionOptions {
                    code_action_kinds: Some(vec![
                        CodeActionKind::QUICKFIX,
                        CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                    ]),
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: Some(true),
                    },
                    resolve_provider: Some(false),
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
