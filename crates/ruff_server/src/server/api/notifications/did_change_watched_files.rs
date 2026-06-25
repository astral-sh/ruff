use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::publish_diagnostics_for_document;
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
        session.reload_settings(&params.changes, client);

        if !params.changes.is_empty() {
            if session.resolved_client_capabilities().workspace_refresh {
                client
                    .send_request::<types::DiagnosticRefreshRequest>(session, (), |_, ()| ())
                    .with_failure_code(lsp_server::ErrorCode::InternalError)?;
            } else {
                // publish diagnostics for text documents
                for uri in session.text_document_uris() {
                    let snapshot = session
                        .take_snapshot(uri.clone())
                        .expect("snapshot should be available");
                    publish_diagnostics_for_document(&snapshot, client)?;
                }
            }

            // always publish diagnostics for notebook files (since they don't use pull diagnostics)
            for uri in session.notebook_document_uris() {
                let snapshot = session
                    .take_snapshot(uri.clone())
                    .expect("snapshot should be available");
                publish_diagnostics_for_document(&snapshot, client)?;
            }
        }

        Ok(())
    }
}
