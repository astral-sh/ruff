use lsp_server::ErrorCode;
use lsp_types::notification::DidCloseTextDocument;
use lsp_types::DidCloseTextDocumentParams;

use crate::server::api::diagnostics::clear_diagnostics;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;

pub(crate) struct DidCloseTextDocumentHandler;

impl NotificationHandler for DidCloseTextDocumentHandler {
    type NotificationType = DidCloseTextDocument;
}

impl SyncNotificationHandler for DidCloseTextDocumentHandler {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        params: DidCloseTextDocumentParams,
    ) -> Result<()> {
        let key = session.key_from_url(params.text_document.uri);
        session
            .close_document(&key)
            .with_failure_code(ErrorCode::InternalError)?;

        // TODO(dhruvmanila): Close the file in the `RootDatabase`

        clear_diagnostics(key.url(), &notifier)?;

        Ok(())
    }
}
