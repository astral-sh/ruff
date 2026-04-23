use crate::server::Action;
use crate::server::Result;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::client::Client;
use crate::session::{ClientOptions, Session};
use lsp_types::notification as notif;
use lsp_types::{self as types, ConfigurationParams, Url};

pub(crate) struct DidChangeConfiguration;

impl NotificationHandler for DidChangeConfiguration {
    type NotificationType = notif::DidChangeConfiguration;
}

impl SyncNotificationHandler for DidChangeConfiguration {
    // This is implemented as the pull-based model, settings included with the notification are
    // not considered.
    fn run(
        session: &mut Session,
        client: &Client,
        _params: types::DidChangeConfigurationParams,
    ) -> Result<()> {
        tracing::debug!("Received workspace/didChangeConfiguration");

        let workspace_urls: Vec<Url> = session
            .workspaces()
            .into_iter()
            .map(|(_, workspace)| workspace.url().clone())
            .collect();

        let items: Vec<types::ConfigurationItem> = workspace_urls
            .iter()
            .map(|workspace| types::ConfigurationItem {
                scope_uri: Some(workspace.clone()),
                section: Some("ty".to_string()),
            })
            .collect();

        tracing::debug!("Sending workspace/configuration requests to client");
        client.send_request::<lsp_types::request::WorkspaceConfiguration>(
            session,
            ConfigurationParams { items },
            |client, result: Vec<serde_json::value::Value>| {
                // This shouldn't fail because, as per the spec, the client needs to provide a
                // `null` value even if it cannot provide a configuration for a workspace.
                assert_eq!(
                    result.len(),
                    workspace_urls.len(),
                    "Mismatch in number of workspace URLs ({}) and configuration results ({})",
                    workspace_urls.len(),
                    result.len()
                );

                let workspaces_with_options: Vec<(Url, ClientOptions)> = workspace_urls
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

                tracing::debug!(
                    "Received new configuration options {:?}",
                    workspaces_with_options,
                );

                client.queue_action(Action::UpdateWorkspaceConfigs(workspaces_with_options));
            },
        );

        Ok(())
    }
}
