use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use lsp_types::{self as types, request as req};
use types::{
    DocumentDiagnosticReportResult, FullDocumentDiagnosticReport,
    RelatedFullDocumentDiagnosticReport,
};

pub(crate) struct DocumentDiagnostic;

impl super::RequestHandler for DocumentDiagnostic {
    type RequestType = req::DocumentDiagnosticRequest;
}

impl super::BackgroundDocumentRequestHandler for DocumentDiagnostic {
    super::define_document_url!(params: &types::DocumentDiagnosticParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        _params: types::DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let diagnostics = if snapshot.client_settings().lint() {
            crate::lint::check(
                snapshot.document(),
                &snapshot.configuration().linter,
                snapshot.encoding(),
            )
        } else {
            vec![]
        };

        Ok(DocumentDiagnosticReportResult::Report(
            types::DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    // TODO(jane): eventually this will be important for caching diagnostic information.
                    result_id: None,
                    items: diagnostics,
                },
            }),
        ))
    }
}
