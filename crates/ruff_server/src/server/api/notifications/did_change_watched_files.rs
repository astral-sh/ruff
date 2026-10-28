use crate::server::Result;
use crate::server::api::refresh_diagnostics;
use crate::session::{Client, Session};
use lsp_types::{self as types, DidChangeWatchedFilesNotification};

pub(crate) struct DidChangeWatchedFiles;

impl super::NotificationHandler for DidChangeWatchedFiles {
    type NotificationType = DidChangeWatchedFilesNotification;
}

impl super::SyncNotificationHandler for DidChangeWatchedFiles {
    fn run(
        session: &mut Session,
        client: &Client,
        params: types::DidChangeWatchedFilesParams,
    ) -> Result<()> {
        if session.reload_settings(&params.changes, client) {
            refresh_diagnostics(session, client)?;
        }

        Ok(())
    }
}
