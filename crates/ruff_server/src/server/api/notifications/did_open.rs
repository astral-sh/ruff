use crate::server::api::diagnostics::publish_diagnostics_for_document;
use crate::server::api::LSPResult;
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use crate::TextDocument;
use anyhow::anyhow;
use lsp_server::ErrorCode;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct DidOpen;

impl super::NotificationHandler for DidOpen {
    type NotificationType = notif::DidOpenTextDocument;
}

impl super::SyncNotificationHandler for DidOpen {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        _requester: &mut Requester,
        types::DidOpenTextDocumentParams {
            text_document:
                types::TextDocumentItem {
                    ref uri,
                    text,
                    version,
                    ..
                },
        }: types::DidOpenTextDocumentParams,
    ) -> Result<()> {
        let document_path: std::path::PathBuf = uri
            .to_file_path()
            .map_err(|()| anyhow!("expected document URI {uri} to be a valid file path"))
            .with_failure_code(ErrorCode::InvalidParams)?;

        let document = TextDocument::new(text, version);

        session.open_text_document(document_path, document);

        // Publish diagnostics if the client doesnt support pull diagnostics
        if !session.resolved_client_capabilities().pull_diagnostics {
            let snapshot = session
                .take_snapshot(uri)
                .ok_or_else(|| {
                    anyhow::anyhow!("Unable to take snapshot for document with URL {uri}")
                })
                .with_failure_code(lsp_server::ErrorCode::InternalError)?;
            publish_diagnostics_for_document(&snapshot, &notifier)?;
        }

        Ok(())
    }
}
