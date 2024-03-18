//! Scheduling, I/O, and API endpoints.

use anyhow::anyhow;
use lsp::Connection;
use lsp_server as lsp;
use lsp_types as types;
use types::ClientCapabilities;
use types::CodeActionKind;
use types::CodeActionOptions;
use types::DiagnosticOptions;
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
    session: Session,
}

impl Server {
    pub fn new() -> crate::Result<Self> {
        let (conn, threads) = lsp::Connection::stdio();

        let (id, params) = conn.initialize_start()?;

        let init_params: types::InitializeParams = serde_json::from_value(params)?;

        let client_capabilities = init_params.capabilities;
        let server_capabilities = Self::server_capabilities(&client_capabilities);

        let workspaces = init_params
            .workspace_folders
            .map(|folders| folders.into_iter().map(|folder| folder.uri).collect())
            .or_else(|| init_params.root_uri.map(|u| vec![u]))
            .ok_or_else(|| {
                anyhow!("No workspace or root URI was given in the LSP initialization parameters. The server cannot start.")
            })?;

        let initialize_data = serde_json::json!({
            "capabilities": server_capabilities,
            "serverInfo": {
                "name": crate::SERVER_NAME,
                "version": crate::version()
            }
        });

        conn.initialize_finish(id, initialize_data)?;

        Ok(Self {
            conn,
            threads,
            session: Session::new(&server_capabilities, &workspaces)?,
        })
    }

    pub fn run(self) -> crate::Result<()> {
        let result = event_loop_thread(move || Self::event_loop(&self.conn, self.session))?.join();
        self.threads.join()?;
        result
    }

    fn event_loop(connection: &Connection, session: Session) -> crate::Result<()> {
        // TODO(jane): Make thread count configurable
        let mut scheduler = schedule::Scheduler::new(session, 4, &connection.sender);
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
