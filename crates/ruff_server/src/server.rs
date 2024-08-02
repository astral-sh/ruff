//! Scheduling, I/O, and API endpoints.

use lsp_server as lsp;
use lsp_types as types;
use std::num::NonZeroUsize;
use std::panic::PanicInfo;
use std::str::FromStr;
use types::ClientCapabilities;
use types::CodeActionKind;
use types::CodeActionOptions;
use types::DiagnosticOptions;
use types::DidChangeWatchedFilesRegistrationOptions;
use types::FileSystemWatcher;
use types::NotebookCellSelector;
use types::NotebookDocumentSyncOptions;
use types::NotebookSelector;
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

use crate::message::try_show_message;
pub(crate) use connection::ClientSender;

pub(crate) type Result<T> = std::result::Result<T, api::Error>;

pub struct Server {
    connection: Connection,
    client_capabilities: ClientCapabilities,
    worker_threads: NonZeroUsize,
    session: Session,
}

impl Server {
    pub fn new(worker_threads: NonZeroUsize, preview: Option<bool>) -> crate::Result<Self> {
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

        if let Some(trace) = init_params.trace {
            crate::trace::set_trace_value(trace);
        }

        crate::message::init_messenger(connection.make_sender());

        let mut all_settings = AllSettings::from_value(
            init_params
                .initialization_options
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::default())),
        );
        if let Some(preview) = preview {
            all_settings.set_preview(preview);
        }
        let AllSettings {
            global_settings,
            mut workspace_settings,
        } = all_settings;

        crate::trace::init_tracing(
            connection.make_sender(),
            global_settings
                .tracing
                .log_level
                .unwrap_or(crate::trace::LogLevel::Info),
            global_settings.tracing.log_file.as_deref(),
            init_params.client_info.as_ref(),
        );

        let mut workspace_for_url = |url: lsp_types::Url| {
            let Some(workspace_settings) = workspace_settings.as_mut() else {
                return (url, ClientSettings::default());
            };
            let settings = workspace_settings.remove(&url).unwrap_or_else(|| {
                tracing::warn!("No workspace settings found for {}", url);
                ClientSettings::default()
            });
            (url, settings)
        };

        let workspaces = init_params
            .workspace_folders
            .filter(|folders| !folders.is_empty())
            .map(|folders| folders.into_iter().map(|folder| {
                workspace_for_url(folder.uri)
            }).collect())
            .or_else(|| {
                tracing::warn!("No workspace(s) were provided during initialization. Using the current working directory as a default workspace...");
                let uri = types::Url::from_file_path(std::env::current_dir().ok()?).ok()?;
                Some(vec![workspace_for_url(uri)])
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
        type PanicHook = Box<dyn Fn(&PanicInfo<'_>) + 'static + Sync + Send>;
        struct RestorePanicHook {
            hook: Option<PanicHook>,
        }

        impl Drop for RestorePanicHook {
            fn drop(&mut self) {
                if let Some(hook) = self.hook.take() {
                    std::panic::set_hook(hook);
                }
            }
        }

        // unregister any previously registered panic hook
        // The hook will be restored when this function exits.
        let _ = RestorePanicHook {
            hook: Some(std::panic::take_hook()),
        };

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

            try_show_message(
                "The Ruff language server exited with a panic. See the logs for more details."
                    .to_string(),
                lsp_types::MessageType::ERROR,
            )
            .ok();
        }));

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
                                        "**/.ruff.toml".into(),
                                    ),
                                    kind: None,
                                },
                                FileSystemWatcher {
                                    glob_pattern: types::GlobPattern::String("**/ruff.toml".into()),
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
