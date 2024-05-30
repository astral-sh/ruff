use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::schedule::Task;
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
        notifier: Notifier,
        requester: &mut Requester,
        params: types::DidChangeWatchedFilesParams,
    ) -> Result<()> {
        for change in &params.changes {
            session.reload_settings(&change.uri);
        }

        if !params.changes.is_empty() {
            if session.resolved_client_capabilities().workspace_refresh {
                requester
                    .request::<types::request::WorkspaceDiagnosticRefresh>((), |()| Task::nothing())
                    .with_failure_code(lsp_server::ErrorCode::InternalError)?;
            } else {
                // publish diagnostics for text documents
                for url in session.text_document_urls() {
                    let snapshot = session
                        .take_snapshot(url.clone())
                        .expect("snapshot should be available");
                    publish_diagnostics_for_document(&snapshot, &notifier)?;
                }
            }

            // always publish diagnostics for notebook files (since they don't use pull diagnostics)
            for url in session.notebook_document_urls() {
                let snapshot = session
                    .take_snapshot(url.clone())
                    .expect("snapshot should be available");
                publish_diagnostics_for_document(&snapshot, &notifier)?;
            }
        }

        Ok(())
    }
}
