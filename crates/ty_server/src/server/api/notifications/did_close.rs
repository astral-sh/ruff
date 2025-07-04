use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::clear_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;
use crate::system::AnySystemPath;
use lsp_server::ErrorCode;
use lsp_types::DidCloseTextDocumentParams;
use lsp_types::notification::DidCloseTextDocument;
use ruff_db::Db as _;
use ruff_db::files::system_path_to_file;
use ty_project::Db as _;
use ty_project::watch::ChangeEvent;

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
        let Ok(key) = session.key_from_url(params.text_document.uri.clone()) else {
            tracing::debug!(
                "Failed to create document key from URI: {}",
                params.text_document.uri
            );
            return Ok(());
        };
        session
            .close_document(&key)
            .with_failure_code(ErrorCode::InternalError)?;

        match key.path() {
            AnySystemPath::System(system_path) => {
                let db = match session.project_db_for_path_mut(system_path) {
                    Some(db) => db,
                    None => session.default_project_db_mut(),
                };
                let Ok(file) = system_path_to_file(db, system_path) else {
                    // This can only fail when the path is a directory or it doesn't exists but the
                    // file should exists for this handler in this branch.
                    tracing::warn!("Failed to create a salsa file for {}", system_path);
                    return Ok(());
                };
                db.project().close_file(db, file);
            }
            AnySystemPath::SystemVirtual(virtual_path) => {
                let db = session.default_project_db_mut();
                // TODO: This seems redundant now that we also try to close the file in the
                // project in that we can directly do `virtual_path.close(db)` in the following
                // branch instead which is what this event does as well.
                db.apply_changes(
                    vec![ChangeEvent::DeletedVirtual(virtual_path.clone())],
                    None,
                );
                if let Some(virtual_file) = db.files().try_virtual_file(virtual_path) {
                    db.project().close_file(db, virtual_file.file());
                }
            }
        }

        if !session.global_settings().diagnostic_mode().is_workspace() {
            // The server needs to clear the diagnostics regardless of whether the client supports
            // pull diagnostics or not. This is because the client only has the capability to fetch
            // the diagnostics but does not automatically clear them when a document is closed.
            clear_diagnostics(&key, client);
        }

        Ok(())
    }
}
