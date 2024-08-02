use std::borrow::Cow;

use lsp_types::request::DocumentDiagnosticRequest;
use lsp_types::{
    Diagnostic, DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, Range, RelatedFullDocumentDiagnosticReport, Url,
};

use red_knot_workspace::db::RootDatabase;
use ruff_db::files::system_path_to_file;

use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use crate::system::url_to_system_path;

pub(crate) struct DocumentDiagnosticRequestHandler;

impl RequestHandler for DocumentDiagnosticRequestHandler {
    type RequestType = DocumentDiagnosticRequest;
}

impl BackgroundDocumentRequestHandler for DocumentDiagnosticRequestHandler {
    fn document_url(params: &DocumentDiagnosticParams) -> Cow<Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        _snapshot: DocumentSnapshot,
        db: Option<salsa::Handle<RootDatabase>>,
        _notifier: Notifier,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let diagnostics = db
            .map(|db| compute_diagnostics(&params.text_document.uri, &db))
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

fn compute_diagnostics(url: &Url, db: &RootDatabase) -> Vec<Diagnostic> {
    let Ok(path) = url_to_system_path(url) else {
        return vec![];
    };
    let Ok(file) = system_path_to_file(db, path) else {
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
