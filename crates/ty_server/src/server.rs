//! Scheduling, I/O, and API endpoints.

use self::schedule::spawn_main_loop;
use crate::PositionEncoding;
use crate::session::{AllOptions, ClientOptions, DiagnosticMode, Session};
use lsp_server::Connection;
use lsp_types::{
    ClientCapabilities, DeclarationCapability, DiagnosticOptions, DiagnosticServerCapabilities,
    HoverProviderCapability, InitializeParams, InlayHintOptions, InlayHintServerCapabilities,
    MessageType, SemanticTokensLegend, SemanticTokensOptions, SemanticTokensServerCapabilities,
    ServerCapabilities, SignatureHelpOptions, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TypeDefinitionProviderCapability, Url, WorkDoneProgressOptions,
};
use ruff_db::system::System;
use std::num::NonZeroUsize;
use std::panic::{PanicHookInfo, RefUnwindSafe};
use std::sync::Arc;

mod api;
mod main_loop;
mod schedule;

use crate::session::client::Client;
pub(crate) use api::Error;
pub(crate) use api::publish_settings_diagnostics;
pub(crate) use main_loop::{Action, ConnectionSender, Event, MainLoopReceiver, MainLoopSender};
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
    pub fn new(
        worker_threads: NonZeroUsize,
        connection: Connection,
        native_system: Arc<dyn System + 'static + Send + Sync + RefUnwindSafe>,
        initialize_logging: bool,
    ) -> crate::Result<Self> {
        let (id, init_value) = connection.initialize_start()?;
        let init_params: InitializeParams = serde_json::from_value(init_value)?;

        let AllOptions {
            global: global_options,
            workspace: mut workspace_options,
        } = AllOptions::from_value(
            init_params
                .initialization_options
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::default())),
        );

        let client_capabilities = init_params.capabilities;
        let position_encoding = Self::find_best_position_encoding(&client_capabilities);
        let server_capabilities =
            Self::server_capabilities(position_encoding, global_options.diagnostic_mode());

        let version = ruff_db::program_version().unwrap_or("Unknown");

        connection.initialize_finish(
            id,
            serde_json::json!({
                "capabilities": server_capabilities,
                "serverInfo": {
                    "name": crate::SERVER_NAME,
                    "version": version
                }
            }),
        )?;

        // The number 32 was chosen arbitrarily. The main goal was to have enough capacity to queue
        // some responses before blocking.
        let (main_loop_sender, main_loop_receiver) = crossbeam::channel::bounded(32);
        let client = Client::new(main_loop_sender.clone(), connection.sender.clone());

        if initialize_logging {
            crate::logging::init_logging(
                global_options.tracing.log_level.unwrap_or_default(),
                global_options.tracing.log_file.as_deref(),
            );
        }

        tracing::debug!("Version: {version}");

        let mut workspace_for_url = |url: Url| {
            let Some(workspace_settings) = workspace_options.as_mut() else {
                return (url, ClientOptions::default());
            };
            let settings = workspace_settings.remove(&url).unwrap_or_else(|| {
                tracing::warn!(
                    "No workspace options found for {}, using default options",
                    url
                );
                ClientOptions::default()
            });
            (url, settings)
        };

        let workspaces = init_params
            .workspace_folders
            .filter(|folders| !folders.is_empty())
            .map(|folders| {
                folders
                    .into_iter()
                    .map(|folder| workspace_for_url(folder.uri))
                    .collect()
            })
            .or_else(|| {
                let current_dir = native_system
                    .current_directory()
                    .as_std_path()
                    .to_path_buf();
                tracing::warn!(
                    "No workspace(s) were provided during initialization. \
                    Using the current working directory from the fallback system as a \
                    default workspace: {}",
                    current_dir.display()
                );
                let uri = Url::from_file_path(current_dir).ok()?;
                Some(vec![workspace_for_url(uri)])
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to get the current working directory while creating a \
                    default workspace."
                )
            })?;

        let workspaces = if workspaces.len() > 1 {
            let first_workspace = workspaces.into_iter().next().unwrap();
            tracing::warn!(
                "Multiple workspaces are not yet supported, using the first workspace: {}",
                &first_workspace.0
            );
            client.show_warning_message(format_args!(
                "Multiple workspaces are not yet supported, using the first workspace: {}",
                &first_workspace.0,
            ));
            vec![first_workspace]
        } else {
            workspaces
        };

        Ok(Self {
            connection,
            worker_threads,
            main_loop_receiver,
            main_loop_sender,
            session: Session::new(
                &client_capabilities,
                position_encoding,
                global_options,
                workspaces,
                native_system,
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

    fn server_capabilities(
        position_encoding: PositionEncoding,
        diagnostic_mode: DiagnosticMode,
    ) -> ServerCapabilities {
        ServerCapabilities {
            position_encoding: Some(position_encoding.into()),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some(crate::DIAGNOSTIC_NAME.into()),
                inter_file_dependencies: true,
                // TODO: Dynamically register for workspace diagnostics.
                workspace_diagnostics: diagnostic_mode.is_workspace(),
                ..Default::default()
            })),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    ..Default::default()
                },
            )),
            type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
            definition_provider: Some(lsp_types::OneOf::Left(true)),
            declaration_provider: Some(DeclarationCapability::Simple(true)),
            references_provider: Some(lsp_types::OneOf::Left(true)),
            document_highlight_provider: Some(lsp_types::OneOf::Left(true)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            signature_help_provider: Some(SignatureHelpOptions {
                trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                retrigger_characters: Some(vec![")".to_string()]),
                work_done_progress_options: lsp_types::WorkDoneProgressOptions::default(),
            }),
            inlay_hint_provider: Some(lsp_types::OneOf::Right(
                InlayHintServerCapabilities::Options(InlayHintOptions::default()),
            )),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                    legend: SemanticTokensLegend {
                        token_types: ty_ide::SemanticTokenType::all()
                            .iter()
                            .map(|token_type| token_type.as_lsp_concept().into())
                            .collect(),
                        token_modifiers: ty_ide::SemanticTokenModifier::all_names()
                            .iter()
                            .map(|&s| s.into())
                            .collect(),
                    },
                    range: Some(true),
                    full: Some(lsp_types::SemanticTokensFullOptions::Bool(true)),
                }),
            ),
            completion_provider: Some(lsp_types::CompletionOptions {
                trigger_characters: Some(vec!['.'.to_string()]),
                ..Default::default()
            }),
            ..Default::default()
        }
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
                client.show_message(
                    "The ty language server exited with a panic. See the logs for more details.",
                    MessageType::ERROR,
                );
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
