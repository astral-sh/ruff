//! Scheduling, I/O, and API endpoints.

use lsp_server::Connection;
use lsp_types as types;
use lsp_types::InitializeParams;
use lsp_types::MessageType;
use std::num::NonZeroUsize;
use std::panic::PanicHookInfo;
use std::str::FromStr;
use std::sync::Arc;
use types::ClientCapabilities;
use types::CodeActionKind;
use types::CodeActionOptions;
use types::DiagnosticOptions;
use types::NotebookCellSelector;
use types::NotebookDocumentSyncOptions;
use types::NotebookSelector;
use types::OneOf;
use types::TextDocumentSyncCapability;
use types::TextDocumentSyncKind;
use types::TextDocumentSyncOptions;
use types::WorkDoneProgressOptions;
use types::WorkspaceFoldersServerCapabilities;

pub(crate) use self::connection::ConnectionInitializer;
pub use self::connection::ConnectionSender;
use self::schedule::spawn_main_loop;
use crate::PositionEncoding;
pub use crate::server::main_loop::MainLoopSender;
pub(crate) use crate::server::main_loop::{Event, MainLoopReceiver};
use crate::session::{AllOptions, Client, Session};
use crate::workspace::Workspaces;
pub(crate) use api::Error;

mod api;
mod connection;
mod main_loop;
mod schedule;

pub(crate) type Result<T> = std::result::Result<T, api::Error>;

pub struct Server {
    connection: Connection,
    client_capabilities: ClientCapabilities,
    worker_threads: NonZeroUsize,
    main_loop_receiver: MainLoopReceiver,
    main_loop_sender: MainLoopSender,
    session: Session,
}

impl Server {
    pub(crate) fn new(
        worker_threads: NonZeroUsize,
        connection: ConnectionInitializer,
        preview: Option<bool>,
    ) -> crate::Result<Self> {
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

        let (main_loop_sender, main_loop_receiver) = crossbeam::channel::bounded(32);

        let InitializeParams {
            initialization_options,
            workspace_folders,
            ..
        } = init_params;

        let client = Client::new(main_loop_sender.clone(), connection.sender.clone());
        let mut all_options = AllOptions::from_value(
            initialization_options
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::default())),
            &client,
        );

        if let Some(preview) = preview {
            all_options.set_preview(preview);
        }

        let AllOptions {
            global: global_options,
            workspace: workspace_options,
        } = all_options;

        crate::logging::init_logging(
            global_options.tracing.log_level.unwrap_or_default(),
            global_options.tracing.log_file.as_deref(),
        );

        let workspaces = Workspaces::from_workspace_folders(
            workspace_folders,
            workspace_options.unwrap_or_default(),
        )?;

        tracing::debug!("Negotiated position encoding: {position_encoding:?}");

        let global = global_options.into_settings(client.clone());

        Ok(Self {
            connection,
            worker_threads,
            main_loop_sender,
            main_loop_receiver,
            session: Session::new(
                &client_capabilities,
                position_encoding,
                global,
                &workspaces,
                &client,
            )?,
            client_capabilities,
        })
    }

    pub fn run(mut self) -> crate::Result<()> {
        let client = Client::new(
            self.main_loop_sender.clone(),
            self.connection.sender.clone(),
        );

        let _panic_hook = ServerPanicHookHandler::new(client);

        spawn_main_loop(move || self.main_loop())?.join()
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
            execute_command_provider: Some(types::ExecuteCommandOptions {
                commands: SupportedCommand::all()
                    .map(|command| command.identifier().to_string())
                    .to_vec(),
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: Some(false),
                },
            }),
            hover_provider: Some(types::HoverProviderCapability::Simple(true)),
            notebook_document_sync: Some(types::OneOf::Left(NotebookDocumentSyncOptions {
                save: Some(false),
                notebook_selector: [NotebookSelector::ByCells {
                    notebook: None,
                    cells: vec![NotebookCellSelector {
                        language: "python".to_string(),
                    }],
                }]
                .to_vec(),
            })),
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
    SourceOrganizeImports,
    /// Maps to the `notebook.source.fixAll` and `notebook.source.fixAll.ruff` code action kinds.
    /// This is a source action, specifically for notebooks, that applies all safe fixes
    /// to the currently open document.
    NotebookSourceFixAll,
    /// Maps to `source.organizeImports` and `source.organizeImports.ruff` code action kinds.
    /// This is a source action, specifically for notebooks, that applies import sorting fixes
    /// to the currently open document.
    NotebookSourceOrganizeImports,
}

impl SupportedCodeAction {
    /// Returns the LSP code action kind that map to this code action.
    fn to_kind(self) -> CodeActionKind {
        match self {
            Self::QuickFix => CodeActionKind::QUICKFIX,
            Self::SourceFixAll => crate::SOURCE_FIX_ALL_RUFF,
            Self::SourceOrganizeImports => crate::SOURCE_ORGANIZE_IMPORTS_RUFF,
            Self::NotebookSourceFixAll => crate::NOTEBOOK_SOURCE_FIX_ALL_RUFF,
            Self::NotebookSourceOrganizeImports => crate::NOTEBOOK_SOURCE_ORGANIZE_IMPORTS_RUFF,
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
            Self::NotebookSourceFixAll,
            Self::NotebookSourceOrganizeImports,
        ]
        .into_iter()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SupportedCommand {
    Debug,
    Format,
    FixAll,
    OrganizeImports,
}

impl SupportedCommand {
    const fn label(self) -> &'static str {
        match self {
            Self::FixAll => "Fix all auto-fixable problems",
            Self::Format => "Format document",
            Self::OrganizeImports => "Format imports",
            Self::Debug => "Print debug information",
        }
    }

    /// Returns the identifier of the command.
    const fn identifier(self) -> &'static str {
        match self {
            SupportedCommand::Format => "ruff.applyFormat",
            SupportedCommand::FixAll => "ruff.applyAutofix",
            SupportedCommand::OrganizeImports => "ruff.applyOrganizeImports",
            SupportedCommand::Debug => "ruff.printDebugInformation",
        }
    }

    /// Returns all the commands that the server currently supports.
    const fn all() -> [SupportedCommand; 4] {
        [
            SupportedCommand::Format,
            SupportedCommand::FixAll,
            SupportedCommand::OrganizeImports,
            SupportedCommand::Debug,
        ]
    }
}

impl FromStr for SupportedCommand {
    type Err = anyhow::Error;

    fn from_str(name: &str) -> anyhow::Result<Self, Self::Err> {
        Ok(match name {
            "ruff.applyAutofix" => Self::FixAll,
            "ruff.applyFormat" => Self::Format,
            "ruff.applyOrganizeImports" => Self::OrganizeImports,
            "ruff.printDebugInformation" => Self::Debug,
            _ => return Err(anyhow::anyhow!("Invalid command `{name}`")),
        })
    }
}

type PanicHook = Box<dyn Fn(&PanicHookInfo<'_>) + 'static + Sync + Send>;

struct ServerPanicHookHandler {
    hook: Option<PanicHook>,
    // Hold on to the strong reference for as long as the panic hook is set.
    _client: Arc<Client>,
}

impl ServerPanicHookHandler {
    fn new(client: Client) -> Self {
        let hook = std::panic::take_hook();
        let client = Arc::new(client);

        // Use a weak reference to the client because it must be dropped when exiting or the
        // io-threads join hangs forever (because client has a reference to the connection sender).
        let hook_client = Arc::downgrade(&client);

        // When we panic, try to notify the client.
        std::panic::set_hook(Box::new(move |panic_info| {
            use std::io::Write;

            let backtrace = std::backtrace::Backtrace::force_capture();
            tracing::error!("{panic_info}\n{backtrace}");

            // we also need to print to stderr directly for when using `$logTrace` because
            // the message won't be sent to the client.
            // But don't use `eprintln` because `eprintln` itself may panic if the pipe is broken.
            let mut stderr = std::io::stderr().lock();
            writeln!(stderr, "{panic_info}\n{backtrace}").ok();

            if let Some(client) = hook_client.upgrade() {
                client
                    .show_message(
                        "The Ruff language server exited with a panic. See the logs for more details.",
                        MessageType::ERROR,
                    )
                    .ok();
            }
        }));

        Self {
            hook: Some(hook),
            _client: client,
        }
    }
}

impl Drop for ServerPanicHookHandler {
    fn drop(&mut self) {
        if std::thread::panicking() {
            // Calling `std::panic::set_hook` while panicking results in a panic.
            return;
        }

        if let Some(hook) = self.hook.take() {
            std::panic::set_hook(hook);
        }
    }
}
