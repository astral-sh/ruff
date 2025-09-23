use crate::server::Result;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidChangeConfiguration;

impl NotificationHandler for DidChangeConfiguration {
    type NotificationType = notif::DidChangeConfiguration;
}

impl SyncNotificationHandler for DidChangeConfiguration {
    fn run(
        session: &mut Session,
        _client: &Client,
        params: types::DidChangeConfigurationParams,
    ) -> Result<()> {
        //the client send a didChangeConfiguration with an empty change event. The server should
        //pulls for settings.
        //see https://github.com/microsoft/vscode-languageserver-node/issues/380#issuecomment-414691493
        //see https://github.com/microsoft/language-server-protocol/issues/676
        assert!(params.settings.is_null());

        let urls = session.workspaces().urls().cloned().collect::<Vec<_>>();

        tracing::info!("GLOBAL: {:?}", session.global_settings());
        tracing::info!("INIT: {:?}", session.initialization_options());
        tracing::info!("WORKSPACES: {:?}", urls);
        Ok(())
    } 
}
