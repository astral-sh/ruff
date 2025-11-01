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

    // Hardest part is how to map the ranges back to notebook documents
    // The issue is that we can't fetch the notebook index of an arbitrary notebook
    // But then again, we **know** it's a notebook.
    // One option would be to stuff the URI into `ruff_notebook` as an extra
    // field on `cell`. Doesn't seem like the WORST ever?
    // But then there's also the issue that ty and the LSP might disagree
    // what's considered a notebook, resulting in ty returning a regular document for
    // a document that's supposed to be a notebook. Resulting in a mismatch between
    // So how can we channel through the entire notebook? But also make this work
    // If we kept the cells within notebook, would it then be easier?
    // Maybe, we could snapshot the entire notebook and call ruff notebook on it.
    // But that only works for the current notebook and not for **any** notebook.
    // In the end, we need a way to retrieve the metadata from any notebook.
    // A method on lsp system doesn't sound that horrible?
    // what happens if we don't report the diagnostics on the cell?
    // The issue with that is that a notebook has no rows?
    // We can't import other notebooks, so related should only be limited to the current file.

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let diagnostics = compute_diagnostics(db, snapshot);

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
                        items: diagnostics.to_lsp_diagnostics(db).expect_text_document(),
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
