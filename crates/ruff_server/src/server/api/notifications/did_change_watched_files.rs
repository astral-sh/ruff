use crate::server::api::LSPResult;
use crate::server::client::Notifier;
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidChangeWatchedFiles;

impl super::NotificationHandler for DidChangeWatchedFiles {
    type NotificationType = notif::DidChangeWatchedFiles;
}

impl super::SyncNotificationHandler for DidChangeWatchedFiles {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        params: types::DidChangeWatchedFilesParams,
    ) -> Result<()> {
        for change in params.changes {
            session
                .reload_configuration(&change.uri)
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;
        }
        Ok(())
    }
}
