use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use anyhow::anyhow;
use lsp_server::ErrorCode;
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
        _requester: &mut Requester,
        params: types::DidChangeWorkspaceFoldersParams,
    ) -> Result<()> {
        for types::WorkspaceFolder { ref uri, .. } in params.event.added {
            let workspace_path = uri
                .to_file_path()
                .map_err(|()| anyhow!("expected document URI {uri} to be a valid file path"))
                .with_failure_code(ErrorCode::InvalidParams)?;

            session.open_workspace_folder(workspace_path);
        }
        for types::WorkspaceFolder { ref uri, .. } in params.event.removed {
            let workspace_path = uri
                .to_file_path()
                .map_err(|()| anyhow!("expected document URI {uri} to be a valid file path"))
                .with_failure_code(ErrorCode::InvalidParams)?;
            session
                .close_workspace_folder(&workspace_path)
                .with_failure_code(lsp_server::ErrorCode::InvalidParams)?;
        }
        Ok(())
    }
}
