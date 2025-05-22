use std::borrow::Cow;

use lsp_types::request::DocumentDiagnosticRequest;
use lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport,
};

use crate::server::api::diagnostics::{Diagnostics, compute_diagnostics};
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::{Result, client_old::Notifier};
use crate::session::DocumentSnapshot;
use ty_project::ProjectDatabase;

pub(crate) struct DocumentDiagnosticRequestHandler;

impl RequestHandler for DocumentDiagnosticRequestHandler {
    type RequestType = DocumentDiagnosticRequest;
}

impl BackgroundDocumentRequestHandler for DocumentDiagnosticRequestHandler {
    fn document_url(params: &DocumentDiagnosticParams) -> Cow<lsp_types::Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        _params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    // SAFETY: Pull diagnostic requests are only called for text documents, not for
                    // notebook documents.
                    items: compute_diagnostics(db, &snapshot)
                        .map_or_else(Vec::new, Diagnostics::expect_text_document),
                },
            }),
        ))
    }
}
