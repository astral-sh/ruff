use lsp_types::{DidOpenTextDocumentNotification, DidOpenTextDocumentParams, TextDocumentItem};

use crate::TextDocument;
use crate::server::Result;
use crate::server::api::diagnostics::publish_diagnostics_if_needed;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;
use crate::session::client::Client;

pub(crate) struct DidOpenTextDocumentHandler;

impl NotificationHandler for DidOpenTextDocumentHandler {
    type NotificationType = DidOpenTextDocumentNotification;
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

        let text_doc = TextDocument::new(uri, text, version, language_id);
        let document = session.open_text_document(text_doc);
        publish_diagnostics_if_needed(&document, session, client);

        Ok(())
    }
}
