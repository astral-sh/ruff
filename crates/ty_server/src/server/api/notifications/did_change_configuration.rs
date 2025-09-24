use crate::server::Result;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::client::Client;
use crate::session::{ClientOptions, Session};
use lsp_types::notification as notif;
use lsp_types::{self as types, ConfigurationParams};
use serde_json::Value;
pub(crate) struct DidChangeConfiguration;

impl NotificationHandler for DidChangeConfiguration {
    type NotificationType = notif::DidChangeConfiguration;
}

//server receives a didChangeConfiguration notification from the client. He should pull the
//current client settings and refresh all workspace settings with the new values.
impl SyncNotificationHandler for DidChangeConfiguration {
    fn run(
        session: &mut Session,
        client: &Client,
        params: types::DidChangeConfigurationParams,
    ) -> Result<()> {
        //the client send a didChangeConfiguration with an empty change event.
        //see https://github.com/microsoft/vscode-languageserver-node/issues/380#issuecomment-414691493
        //see https://github.com/microsoft/language-server-protocol/issues/676
        assert!(params.settings.is_null());

        //The server should pull for settings. We send a workspace/configuration request to the
        //client.
        let urls = session.workspaces().urls().cloned().collect::<Vec<_>>();

        let items = urls
            .iter()
            .map(|root| lsp_types::ConfigurationItem {
                scope_uri: Some(root.clone()),
                section: Some("ty".to_string()),
            })
            .collect();

        client.send_request::<lsp_types::request::WorkspaceConfiguration>(
            session,
            ConfigurationParams { items },
            |_, result: Vec<Value>| {
                tracing::debug!("Received workspace configurations, initializing workspaces");

                // This shouldn't fail because, as per the spec, the client needs to provide a
                // `null` value even if it cannot provide a configuration for a workspace.
                assert_eq!(
                    result.len(),
                    urls.len(),
                    "Mismatch in number of workspace URLs ({}) and configuration results ({})",
                    urls.len(),
                    result.len()
                );

                let workspaces_with_options: Vec<_> = urls
                    .into_iter()
                    .zip(result)
                    .map(|(url, value)| {
                        if value.is_null() {
                            tracing::debug!(
                                "No workspace options provided for {url}, using default options"
                            );
                            return (url, ClientOptions::default());
                        }
                        let options: ClientOptions =
                            serde_json::from_value(value).unwrap_or_else(|err| {
                                tracing::error!(
                                    "Failed to deserialize workspace options for {url}: {err}. \
                                        Using default options"
                                );
                                ClientOptions::default()
                            });
                        (url, options)
                    })
                    .collect();
                tracing::info!("WORKSPACES: {:?}", workspaces_with_options);
            },
        );
        tracing::info!("GLOBAL: {:?}", session.global_settings());
        tracing::info!("INIT: {:?}", session.initialization_options());
        Ok(())
    }
}
