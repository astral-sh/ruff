use crate::server::Result;
use crate::server::api::LSPResult;
use crate::session::{Client, Session};
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidChangeWorkspace;

impl super::NotificationHandler for DidChangeWorkspace {
    type NotificationType = notif::DidChangeWorkspaceFolders;
}

impl super::SyncNotificationHandler for DidChangeWorkspace {
    fn run(
        session: &mut Session,
        client: &Client,
        params: types::DidChangeWorkspaceFoldersParams,
    ) -> Result<()> {
        for types::WorkspaceFolder { uri, .. } in params.event.added {
            session
                .open_workspace_folder(uri, client)
                .with_failure_code(lsp_server::ErrorCode::InvalidParams)?;
        }
        for types::WorkspaceFolder { uri, .. } in params.event.removed {
            session
                .close_workspace_folder(&uri)
                .with_failure_code(lsp_server::ErrorCode::InvalidParams)?;
        }
        Ok(())
    }
}
