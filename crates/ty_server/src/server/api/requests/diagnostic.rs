use std::borrow::Cow;

use lsp_types::request::DocumentDiagnosticRequest;
use lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, RelatedFullDocumentDiagnosticReport,
    RelatedUnchangedDocumentDiagnosticReport, UnchangedDocumentDiagnosticReport, Url,
};

use crate::server::Result;
use crate::server::api::diagnostics::compute_diagnostics;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use ty_project::ProjectDatabase;

pub(crate) struct DocumentDiagnosticRequestHandler;

impl RequestHandler for DocumentDiagnosticRequestHandler {
    type RequestType = DocumentDiagnosticRequest;
}

impl BackgroundDocumentRequestHandler for DocumentDiagnosticRequestHandler {
    fn document_url(params: &DocumentDiagnosticParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        if snapshot.global_settings().diagnostic_mode().is_off() {
            return Ok(DocumentDiagnosticReportResult::Report(
                DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport::default()),
            ));
        }

        let diagnostics = compute_diagnostics(db, snapshot.document(), snapshot.encoding());

        let Some(diagnostics) = diagnostics else {
            return Ok(DocumentDiagnosticReportResult::Report(
                DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport::default()),
            ));
        };

        let result_id = diagnostics.result_id();

        let report = match result_id {
            Some(new_id) if Some(&new_id) == params.previous_result_id.as_ref() => {
                DocumentDiagnosticReport::Unchanged(RelatedUnchangedDocumentDiagnosticReport {
                    related_documents: None,
                    unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                        result_id: new_id,
                    },
                })
            }
            new_id => {
                DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: new_id,
                        // SAFETY: Pull diagnostic requests are only called for text documents, not for
                        // notebook documents.
                        items: diagnostics
                            .to_lsp_diagnostics(
                                db,
                                snapshot.resolved_client_capabilities(),
                                snapshot.global_settings(),
                            )
                            .expect_text_document(),
                    },
                })
            }
        };

        Ok(DocumentDiagnosticReportResult::Report(report))
    }
}

impl RetriableRequestHandler for DocumentDiagnosticRequestHandler {
    fn salsa_cancellation_error() -> lsp_server::ResponseError {
        lsp_server::ResponseError {
            code: lsp_server::ErrorCode::ServerCancelled as i32,
            message: "server cancelled the request".to_owned(),
            data: serde_json::to_value(lsp_types::DiagnosticServerCancellationData {
                retrigger_request: true,
            })
            .ok(),
        }
    }
}
