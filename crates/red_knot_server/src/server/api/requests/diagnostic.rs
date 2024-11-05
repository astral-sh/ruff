use std::borrow::Cow;

use lsp_types::request::DocumentDiagnosticRequest;
use lsp_types::{
    Diagnostic, DiagnosticSeverity, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, FullDocumentDiagnosticReport, NumberOrString, Range,
    RelatedFullDocumentDiagnosticReport, Url,
};

use red_knot_workspace::db::{Db, RootDatabase};
use ruff_db::diagnostic::{CompileDiagnostic, Diagnostic as _, Severity};
use ruff_db::source::{line_index, source_text};

use crate::edit::ToRangeExt as _;
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
        .map(|message| to_lsp_diagnostic(db, message, snapshot.encoding()))
        .collect()
}

fn to_lsp_diagnostic(
    db: &dyn Db,
    diagnostic: &CompileDiagnostic,
    encoding: crate::PositionEncoding,
) -> Diagnostic {
    let range = if let Some(range) = diagnostic.range() {
        let index = line_index(db.upcast(), diagnostic.file());
        let source = source_text(db.upcast(), diagnostic.file());

        range.to_range(&source, &index, encoding)
    } else {
        Range::default()
    };

    let severity = match diagnostic.severity() {
        Severity::Info => DiagnosticSeverity::INFORMATION,
        Severity::Error => DiagnosticSeverity::ERROR,
    };

    Diagnostic {
        range,
        severity: Some(severity),
        tags: None,
        code: Some(NumberOrString::String(diagnostic.rule().to_string())),
        code_description: None,
        source: Some("red-knot".into()),
        message: diagnostic.message().into_owned(),
        related_information: None,
        data: None,
    }
}
