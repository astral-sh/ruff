//! Scheduling, I/O, and API endpoints.

use self::schedule::spawn_main_loop;
use crate::PositionEncoding;
use crate::capabilities::{ResolvedClientCapabilities, server_capabilities};
use crate::session::{InitializationOptions, Session, warn_about_unknown_options};
use anyhow::Context;
use lsp_server::Connection;
use lsp_types::{ClientCapabilities, InitializeParams, MessageType, Url};
use ruff_db::system::System;
use std::num::NonZeroUsize;
use std::panic::{PanicHookInfo, RefUnwindSafe};
use std::sync::Arc;

mod api;
mod lazy_work_done_progress;
mod main_loop;
mod schedule;

use crate::session::client::Client;
pub(crate) use api::Error;
pub(crate) use api::publish_settings_diagnostics;
pub(crate) use main_loop::{
    Action, ConnectionSender, Event, MainLoopReceiver, MainLoopSender, SendRequest,
};
pub(crate) type Result<T> = std::result::Result<T, api::Error>;
pub use api::{PartialWorkspaceProgress, PartialWorkspaceProgressParams};

pub struct Server {
    connection: Connection,
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
        in_test: bool,
    ) -> crate::Result<Self> {
        let (id, init_value) = connection.initialize_start()?;

        let InitializeParams {
            initialization_options,
            capabilities: client_capabilities,
            workspace_folders,
            ..
        } = serde_json::from_value(init_value)
            .context("Failed to deserialize initialization parameters")?;

        let (initialization_options, deserialization_error) =
            InitializationOptions::from_value(initialization_options);

        if !in_test {
            crate::logging::init_logging(
                initialization_options.log_level.unwrap_or_default(),
                initialization_options.log_file.as_deref(),
            );
        }

        if let Some(error) = deserialization_error {
            tracing::error!("Failed to deserialize initialization options: {error}");
        }

        tracing::debug!("Initialization options: {initialization_options:#?}");

        let resolved_client_capabilities = ResolvedClientCapabilities::new(&client_capabilities);

        tracing::debug!("Resolved client capabilities: {resolved_client_capabilities}");

        let position_encoding = Self::find_best_position_encoding(&client_capabilities);
        let server_capabilities =
            server_capabilities(position_encoding, resolved_client_capabilities);

        let version = ruff_db::program_version().unwrap_or("Unknown");
        tracing::info!("Version: {version}");

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

        let unknown_options = &initialization_options.options.unknown;
        if !unknown_options.is_empty() {
            warn_about_unknown_options(&client, None, unknown_options);
        }

        // Get workspace URLs without settings - settings will come from workspace/configuration
        let workspace_urls = workspace_folders
            .filter(|folders| !folders.is_empty())
            .map(|folders| {
                folders
                    .into_iter()
                    .map(|folder| folder.uri)
                    .collect::<Vec<_>>()
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
                Some(vec![uri])
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to get the current working directory while creating a \
                    default workspace."
                )
            })?;

        let workspace_urls = if workspace_urls.len() > 1 {
            let first_workspace = workspace_urls.into_iter().next().unwrap();
            tracing::warn!(
                "Multiple workspaces are not yet supported, using the first workspace: {}",
                &first_workspace
            );
            client.show_warning_message(format_args!(
                "Multiple workspaces are not yet supported, using the first workspace: {}",
                &first_workspace,
            ));
            vec![first_workspace]
        } else {
            workspace_urls
        };

        Ok(Self {
            connection,
            worker_threads,
            main_loop_receiver,
            main_loop_sender,
            session: Session::new(
                resolved_client_capabilities,
                position_encoding,
                workspace_urls,
                initialization_options,
                native_system,
                in_test,
            )?,
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
