use crate::server::api::diagnostics::clear_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidClose;

impl super::NotificationHandler for DidClose {
    type NotificationType = notif::DidCloseTextDocument;
}

impl super::SyncNotificationHandler for DidClose {
    #[tracing::instrument(skip_all, fields(file=%uri))]
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        types::DidCloseTextDocumentParams {
            text_document: types::TextDocumentIdentifier { uri },
        }: types::DidCloseTextDocumentParams,
    ) -> Result<()> {
        // Publish an empty diagnostic report for the document if the client does not support pull diagnostics.
        // This will de-register any existing diagnostics.
        if !session.resolved_client_capabilities().pull_diagnostics {
            let snapshot = session
                .take_snapshot(&uri)
                .ok_or_else(|| {
                    anyhow::anyhow!("Unable to take snapshot for document with URL {uri}")
                })
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;
            clear_diagnostics_for_document(&snapshot, &notifier)?;
        }

        session
            .close_document(&uri)
            .with_failure_code(lsp_server::ErrorCode::InternalError)
    }
}
