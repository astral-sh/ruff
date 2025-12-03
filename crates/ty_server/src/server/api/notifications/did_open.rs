use lsp_types::notification::DidOpenTextDocument;
use lsp_types::{DidOpenTextDocumentParams, TextDocumentItem};

use crate::TextDocument;
use crate::server::Result;
use crate::server::api::diagnostics::publish_diagnostics_if_needed;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;

pub(crate) struct DidOpenTextDocumentHandler;

impl NotificationHandler for DidOpenTextDocumentHandler {
    type NotificationType = DidOpenTextDocument;
}

impl SyncNotificationHandler for DidOpenTextDocumentHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        params: DidOpenTextDocumentParams,
    ) -> Result<()> {
        let DidOpenTextDocumentParams {
            text_document:
                TextDocumentItem {
                    uri,
                    text,
                    version,
                    language_id,
                },
        } = params;

        let document = session.open_text_document(
            TextDocument::new(uri, text, version).with_language_id(&language_id),
        );

        publish_diagnostics_if_needed(&document, session, client);

        Ok(())
    }
}
