use lsp_server::ErrorCode;
use lsp_types::notification::DidCloseTextDocument;
use lsp_types::{DidCloseTextDocumentParams, TextDocumentIdentifier};

use crate::server::Result;
use crate::server::api::LSPResult;
use crate::server::api::diagnostics::clear_diagnostics_if_needed;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;

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

        let document = session
            .document_handle(&uri)
            .with_failure_code(ErrorCode::InternalError)?;

        let should_clear_diagnostics = document
            .close(session)
            .with_failure_code(ErrorCode::InternalError)?;

        if should_clear_diagnostics {
            clear_diagnostics_if_needed(&document, session, client);
        }

        Ok(())
    }
}
