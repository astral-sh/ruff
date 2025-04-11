use std::borrow::Cow;

use lsp_types::request::DocumentDiagnosticRequest;
use lsp_types::{
    Diagnostic, DiagnosticSeverity, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, FullDocumentDiagnosticReport, NumberOrString, Range,
    RelatedFullDocumentDiagnosticReport, Url,
};

use crate::document::{FileRangeExt, ToRangeExt};
use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use red_knot_project::{Db, ProjectDatabase};
use ruff_db::diagnostic::Severity;
use ruff_db::files::FileRange;
use ruff_db::source::{line_index, source_text};

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
        db: ProjectDatabase,
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

fn compute_diagnostics(snapshot: &DocumentSnapshot, db: &ProjectDatabase) -> Vec<Diagnostic> {
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
    diagnostic: &ruff_db::diagnostic::Diagnostic,
    encoding: crate::PositionEncoding,
) -> Diagnostic {
    let range = if let Some(span) = diagnostic.primary_span() {
        let index = line_index(db.upcast(), span.file());
        let source = source_text(db.upcast(), span.file());

        span.range()
            .map(|range| range.to_lsp_range(&source, &index, encoding))
            .unwrap_or_default()
    } else {
        Range::default()
    };

    let severity = match diagnostic.severity() {
        Severity::Info => DiagnosticSeverity::INFORMATION,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Error | Severity::Fatal => DiagnosticSeverity::ERROR,
    };

    let related_information = diagnostic
        .non_primary_annotations()
        .chain(
            diagnostic
                .sub_diagnostics()
                .iter()
                .flat_map(ruff_db::diagnostic::SubDiagnostic::annotations),
        )
        .filter_map(|annotation| {
            let span = annotation.get_span();
            let range = FileRange::try_from(span).ok()?;

            let location = range.to_location(db, encoding)?;

            Some(lsp_types::DiagnosticRelatedInformation {
                location,
                message: annotation.get_message()?.to_string(),
            })
        })
        .collect();

    Diagnostic {
        range,
        severity: Some(severity),
        tags: None,
        code: Some(NumberOrString::String(diagnostic.id().to_string())),
        code_description: None,
        source: Some("red-knot".into()),
        message: diagnostic.primary_message().to_string(),
        related_information: Some(related_information),
        data: None,
    }
}
