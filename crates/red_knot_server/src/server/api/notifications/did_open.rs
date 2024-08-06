use lsp_types::notification::DidOpenTextDocument;
use lsp_types::DidOpenTextDocumentParams;

use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::TextDocument;

pub(crate) struct DidOpenTextDocumentHandler;

impl NotificationHandler for DidOpenTextDocumentHandler {
    type NotificationType = DidOpenTextDocument;
}

impl SyncNotificationHandler for DidOpenTextDocumentHandler {
    fn run(
        session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: DidOpenTextDocumentParams,
    ) -> Result<()> {
        let document = TextDocument::new(params.text_document.text, params.text_document.version);
        session.open_text_document(params.text_document.uri, document);

        // TODO(dhruvmanila): Open the file in the `RootDatabase`

        // Publish diagnostics if the client doesn't support pull diagnostics

        Ok(())
    }
}
