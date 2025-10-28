use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::clear_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;
use lsp_server::ErrorCode;
use lsp_types::notification::DidCloseTextDocument;
use lsp_types::{DidCloseTextDocumentParams, TextDocumentIdentifier};
use ruff_db::Db as _;
use ty_project::Db as _;

pub(crate) struct DidCloseTextDocumentHandler;

impl NotificationHandler for DidCloseTextDocumentHandler {
    type NotificationType = DidCloseTextDocument;
}

impl SyncNotificationHandler for DidCloseTextDocumentHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        params: DidCloseTextDocumentParams,
    ) -> Result<()> {
        let DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
        } = params;

        let key = match session.key_from_url(uri) {
            Ok(key) => key,
            Err(uri) => {
                tracing::debug!("Failed to create document key from URI: {}", uri);
                return Ok(());
            }
        };

        session
            .close_document(&key)
            .with_failure_code(ErrorCode::InternalError)?;

        let path = key.path();
        let db = session.project_db_mut(path);

        match path {
            AnySystemPath::System(system_path) => {
                if let Some(file) = db.files().try_system(db, system_path) {
                    db.project().close_file(db, file);
                } else {
                    // This can only fail when the path is a directory or it doesn't exists but the
                    // file should exists for this handler in this branch. This is because every
                    // close call is preceded by an open call, which ensures that the file is
                    // interned in the lookup table (`Files`).
                    tracing::warn!("Salsa file does not exists for {}", system_path);
                }

                // For non-virtual files, we clear diagnostics if:
                //
                // 1. The file does not belong to any workspace e.g., opening a random file from
                //    outside the workspace because closing it acts like the file doesn't exists
                // 2. The diagnostic mode is set to open-files only
                if session.workspaces().for_path(system_path).is_none()
                    || session
                        .global_settings()
                        .diagnostic_mode()
                        .is_open_files_only()
                {
                    clear_diagnostics(session, &key, client);
                }
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                if let Some(virtual_file) = db.files().try_virtual_file(virtual_path) {
                    db.project().close_file(db, virtual_file.file());
                    virtual_file.close(db);
                } else {
                    tracing::warn!("Salsa virtual file does not exists for {}", virtual_path);
                }

                // Always clear diagnostics for virtual files, as they don't really exist on disk
                // which means closing them is like deleting the file.
                clear_diagnostics(session, &key, client);
            }
        }

        Ok(())
    }
}
