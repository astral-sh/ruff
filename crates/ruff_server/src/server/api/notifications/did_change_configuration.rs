use crate::server::client::Notifier;
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidChangeConfiguration;

impl super::NotificationHandler for DidChangeConfiguration {
    type NotificationType = notif::DidChangeConfiguration;
}

impl super::SyncNotificationHandler for DidChangeConfiguration {
    fn run(
        _session: &mut Session,
        _notifier: Notifier,
        _params: types::DidChangeConfigurationParams,
    ) -> Result<()> {
        // TODO(jane): get this wired up after the pre-release
        Ok(())
    }
}
