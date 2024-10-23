use std::borrow::Cow;

use lsp_types::request::DocumentDiagnosticRequest;
use lsp_types::{
    Diagnostic, DiagnosticSeverity, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, FullDocumentDiagnosticReport, Position, Range,
    RelatedFullDocumentDiagnosticReport, Url,
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
        db: RootDatabase,
        _notifier: Notifier,
        _params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let diagnostics = compute_diagnostics(&snapshot, &db);

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
        tracing::info!(
            "No file found for snapshot for `{}`",
            snapshot.query().file_url()
        );
        return vec![];
    };

    let diagnostics = match db.check_file(file) {
        Ok(diagnostics) => diagnostics,
        Err(cancelled) => {
            tracing::info!("Diagnostics computation {cancelled}");
            return vec![];
        }
    };

    diagnostics
        .as_slice()
        .iter()
        .map(|message| to_lsp_diagnostic(message))
        .collect()
}

fn to_lsp_diagnostic(message: &str) -> Diagnostic {
    let words = message.split(':').collect::<Vec<_>>();

    let (range, message) = match words.as_slice() {
        [_, _, line, column, message] | [_, line, column, message] => {
            let line = line.parse::<u32>().unwrap_or_default().saturating_sub(1);
            let column = column.parse::<u32>().unwrap_or_default();
            (
                Range::new(
                    Position::new(line, column.saturating_sub(1)),
                    Position::new(line, column),
                ),
                message.trim(),
            )
        }
        _ => (Range::default(), message),
    };

    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        tags: None,
        code: None,
        code_description: None,
        source: Some("red-knot".into()),
        message: message.to_string(),
        related_information: None,
        data: None,
    }
}
