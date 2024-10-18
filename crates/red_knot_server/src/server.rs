//! Scheduling, I/O, and API endpoints.

use std::num::NonZeroUsize;
#[allow(deprecated)]
use std::panic::PanicInfo;

use lsp_server::Message;
use lsp_types::{
    ClientCapabilities, DiagnosticOptions, DiagnosticServerCapabilities, MessageType,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    Url,
};

use self::connection::{Connection, ConnectionInitializer};
use self::schedule::event_loop_thread;
use crate::session::{AllSettings, ClientSettings, Session};
use crate::PositionEncoding;

mod api;
mod client;
mod connection;
mod schedule;

use crate::message::try_show_message;
pub(crate) use connection::ClientSender;

pub(crate) type Result<T> = std::result::Result<T, api::Error>;

pub(crate) struct Server {
    connection: Connection,
    client_capabilities: ClientCapabilities,
    worker_threads: NonZeroUsize,
    session: Session,
}

impl Server {
    pub(crate) fn new(worker_threads: NonZeroUsize) -> crate::Result<Self> {
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

        let AllSettings {
            global_settings,
            mut workspace_settings,
        } = AllSettings::from_value(
            init_params
                .initialization_options
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::default())),
        );

        crate::trace::init_tracing(
            connection.make_sender(),
            global_settings
                .tracing
                .log_level
                .unwrap_or(crate::trace::LogLevel::Info),
            global_settings.tracing.log_file.as_deref(),
            init_params.client_info.as_ref(),
        );

        let mut workspace_for_url = |url: Url| {
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
                let uri = Url::from_file_path(std::env::current_dir().ok()?).ok()?;
                Some(vec![workspace_for_url(uri)])
            })
            .ok_or_else(|| {
                anyhow::anyhow!("Failed to get the current working directory while creating a default workspace.")
            })?;

        if workspaces.len() > 1 {
            // TODO(dhruvmanila): Support multi-root workspaces
            anyhow::bail!("Multi-root workspaces are not supported yet");
        }

        Ok(Self {
            connection,
            worker_threads,
            session: Session::new(
                &client_capabilities,
                position_encoding,
                global_settings,
                &workspaces,
            )?,
            client_capabilities,
        })
    }

    pub(crate) fn run(self) -> crate::Result<()> {
        #[allow(deprecated)]
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
                MessageType::ERROR,
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
        _client_capabilities: &ClientCapabilities,
        mut session: Session,
        worker_threads: NonZeroUsize,
    ) -> crate::Result<()> {
        let mut scheduler =
            schedule::Scheduler::new(&mut session, worker_threads, connection.make_sender());

        for msg in connection.incoming() {
            if connection.handle_shutdown(&msg)? {
                break;
            }
            let task = match msg {
                Message::Request(req) => api::request(req),
                Message::Notification(notification) => api::notification(notification),
                Message::Response(response) => scheduler.response(response),
            };
            scheduler.dispatch(task);
        }

        Ok(())
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

    fn server_capabilities(position_encoding: PositionEncoding) -> ServerCapabilities {
        ServerCapabilities {
            position_encoding: Some(position_encoding.into()),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some(crate::DIAGNOSTIC_NAME.into()),
                ..Default::default()
            })),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    ..Default::default()
                },
            )),
            ..Default::default()
        }
    }
}
