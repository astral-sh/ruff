//! Scheduling, I/O, and API endpoints.

use std::num::NonZeroUsize;
// The new PanicInfoHook name requires MSRV >= 1.82
#[expect(deprecated)]
use std::panic::PanicInfo;

use lsp_server::Message;
use lsp_types::{
    ClientCapabilities, DiagnosticOptions, DiagnosticServerCapabilities,
    DidChangeWatchedFilesRegistrationOptions, FileSystemWatcher, HoverProviderCapability,
    InlayHintOptions, InlayHintServerCapabilities, MessageType, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TypeDefinitionProviderCapability, Url,
};

use self::connection::{Connection, ConnectionInitializer};
use self::schedule::event_loop_thread;
use crate::PositionEncoding;
use crate::session::{AllSettings, ClientSettings, Experimental, Session};

mod api;
mod client;
mod connection;
mod schedule;

use crate::message::try_show_message;
use crate::server::schedule::Task;
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

        let AllSettings {
            global_settings,
            mut workspace_settings,
        } = AllSettings::from_value(
            init_params
                .initialization_options
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::default())),
        );

        let client_capabilities = init_params.capabilities;
        let position_encoding = Self::find_best_position_encoding(&client_capabilities);
        let server_capabilities =
            Self::server_capabilities(position_encoding, global_settings.experimental.as_ref());

        let connection = connection.initialize_finish(
            id,
            &server_capabilities,
            crate::SERVER_NAME,
            crate::version(),
        )?;

        crate::message::init_messenger(connection.make_sender());
        crate::logging::init_logging(
            global_settings.tracing.log_level.unwrap_or_default(),
            global_settings.tracing.log_file.as_deref(),
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

        let workspaces = if workspaces.len() > 1 {
            let first_workspace = workspaces.into_iter().next().unwrap();
            tracing::warn!(
                "Multiple workspaces are not yet supported, using the first workspace: {}",
                &first_workspace.0
            );
            show_warn_msg!(
                "Multiple workspaces are not yet supported, using the first workspace: {}",
                &first_workspace.0
            );
            vec![first_workspace]
        } else {
            workspaces
        };

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
        // The new PanicInfoHook name requires MSRV >= 1.82
        #[expect(deprecated)]
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
                "The ty language server exited with a panic. See the logs for more details."
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

    fn event_loop(
        connection: &Connection,
        client_capabilities: &ClientCapabilities,
        mut session: Session,
        worker_threads: NonZeroUsize,
    ) -> crate::Result<()> {
        let mut scheduler =
            schedule::Scheduler::new(&mut session, worker_threads, connection.make_sender());

        let fs_watcher = client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.did_change_watched_files?.dynamic_registration)
            .unwrap_or_default();

        if fs_watcher {
            let registration = lsp_types::Registration {
                id: "workspace/didChangeWatchedFiles".to_owned(),
                method: "workspace/didChangeWatchedFiles".to_owned(),
                register_options: Some(
                    serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                        watchers: vec![
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String("**/ty.toml".into()),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String(
                                    "**/.gitignore".into(),
                                ),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String("**/.ignore".into()),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String(
                                    "**/pyproject.toml".into(),
                                ),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String("**/*.py".into()),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String("**/*.pyi".into()),
                                kind: None,
                            },
                            FileSystemWatcher {
                                glob_pattern: lsp_types::GlobPattern::String("**/*.ipynb".into()),
                                kind: None,
                            },
                        ],
                    })
                    .unwrap(),
                ),
            };
            let response_handler = |()| {
                tracing::info!("File watcher successfully registered");
                Task::nothing()
            };

            if let Err(err) = scheduler.request::<lsp_types::request::RegisterCapability>(
                lsp_types::RegistrationParams {
                    registrations: vec![registration],
                },
                response_handler,
            ) {
                tracing::error!(
                    "An error occurred when trying to register the configuration file watcher: {err}"
                );
            }
        } else {
            tracing::warn!("The client does not support file system watching.");
        }

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

    fn server_capabilities(
        position_encoding: PositionEncoding,
        experimental: Option<&Experimental>,
    ) -> ServerCapabilities {
        ServerCapabilities {
            position_encoding: Some(position_encoding.into()),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some(crate::DIAGNOSTIC_NAME.into()),
                inter_file_dependencies: true,
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
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            inlay_hint_provider: Some(lsp_types::OneOf::Right(
                InlayHintServerCapabilities::Options(InlayHintOptions::default()),
            )),
            completion_provider: experimental
                .is_some_and(Experimental::is_completions_enabled)
                .then_some(lsp_types::CompletionOptions {
                    ..Default::default()
                }),
            ..Default::default()
        }
    }
}
