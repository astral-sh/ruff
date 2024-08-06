use std::borrow::Cow;

use lsp_types::request::DocumentDiagnosticRequest;
use lsp_types::{
    Diagnostic, DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, Range, RelatedFullDocumentDiagnosticReport, Url,
};

use red_knot_workspace::db::RootDatabase;

use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;

pub(crate) struct DocumentDiagnosticRequestHandler;

impl RequestHandler for DocumentDiagnosticRequestHandler {
    type RequestType = DocumentDiagnosticRequest;
}

impl BackgroundDocumentRequestHandler for DocumentDiagnosticRequestHandler {
    fn document_url(params: &DocumentDiagnosticParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        db: Option<RootDatabase>,
        _notifier: Notifier,
        _params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let diagnostics = db
            .map(|db| compute_diagnostics(&snapshot, &db))
            .unwrap_or_default();

        Ok(DocumentDiagnosticReportResult::Report(
            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items: diagnostics,
                },
            }),
        ))
    }
}

fn compute_diagnostics(snapshot: &DocumentSnapshot, db: &RootDatabase) -> Vec<Diagnostic> {
    let Some(file) = snapshot.file(db) else {
        return vec![];
    };
    let Ok(diagnostics) = db.check_file(file) else {
        return vec![];
    };

    diagnostics
        .as_slice()
        .iter()
        .map(|message| Diagnostic {
            range: Range::default(),
            severity: None,
            tags: None,
            code: None,
            code_description: None,
            source: Some("red-knot".into()),
            message: message.to_string(),
            related_information: None,
            data: None,
        })
        .collect()
}
