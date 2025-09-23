use crate::server::Result;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::{Session, client::Client};
use lsp_types as types;
use lsp_types::notification as notif;
pub(crate) struct DidChangeConfiguration;

impl NotificationHandler for DidChangeConfiguration {
    type NotificationType = notif::DidChangeConfiguration;
}

impl SyncNotificationHandler for DidChangeConfiguration {
    fn run(
        _session: &mut Session,
        _client: &Client,
        _params: types::DidChangeConfigurationParams,
    ) -> Result<()> {
        tracing::info!("fn triggered");
        Ok(())
    }
}
