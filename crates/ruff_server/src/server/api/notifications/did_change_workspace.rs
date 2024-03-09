use crate::server::api::LSPResult;
use crate::server::client::Notifier;
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidChangeWorkspace;

impl super::NotificationHandler for DidChangeWorkspace {
    type NotificationType = notif::DidChangeWorkspaceFolders;
}

impl super::SyncNotificationHandler for DidChangeWorkspace {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        params: types::DidChangeWorkspaceFoldersParams,
    ) -> Result<()> {
        for new in params.event.added {
            session
                .open_workspace_folder(&new.uri)
                .with_failure_code(lsp_server::ErrorCode::InvalidParams)?;
        }
        for removed in params.event.removed {
            session
                .close_workspace_folder(&removed.uri)
                .with_failure_code(lsp_server::ErrorCode::InvalidParams)?;
        }
        Ok(())
    }
}
