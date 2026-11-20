//! Condensed diagnostic reports for language-server test assertions.

use lsp_types::{
    DiagnosticSeverity, DocumentDiagnosticReport, FullDocumentDiagnosticReport, Message,
    WorkspaceDiagnosticReport, WorkspaceDocumentDiagnosticReport,
};

use crate::pull_diagnostics::sort_workspace_diagnostic_response;

/// Formats a document report as diagnostic ranges, severities, and messages.
pub(crate) fn condensed_document_diagnostic_snapshot(report: DocumentDiagnosticReport) -> String {
    match report {
        DocumentDiagnosticReport::RelatedFullDocumentDiagnosticReport(full) => {
            condensed_full_document_diagnostic_report(full.full_document_diagnostic_report)
                .join("\n")
        }
        // NOTE: It might be worth providing more details for these
        // cases, but I don't think there's currently a use case for
        // it.
        DocumentDiagnosticReport::RelatedUnchangedDocumentDiagnosticReport(_) => {
            "UNCHANGED".to_string()
        }
    }
}

/// A helper routine for creating a snapshot for a collection of
/// workspace diagnostics.
///
/// We mostly use this in our workspace folder tests to check that the
/// LSP is correctly recognizing and reporting diagnostics for each
/// workspace folder. This isn't really meant to test the diagnostics
/// themselves, hence the condensed output.
pub(crate) fn condensed_workspace_diagnostic_snapshot(
    mut report: WorkspaceDiagnosticReport,
) -> String {
    sort_workspace_diagnostic_response(&mut report);
    let items = report.items;
    items
        .into_iter()
        .map(|item| match item {
            WorkspaceDocumentDiagnosticReport::WorkspaceFullDocumentDiagnosticReport(
                doc_report,
            ) => {
                let diagnostics = condensed_full_document_diagnostic_report(
                    doc_report.full_document_diagnostic_report,
                )
                .join("\n\t");
                format!("{}\n\t{diagnostics}", doc_report.uri)
            }
            WorkspaceDocumentDiagnosticReport::WorkspaceUnchangedDocumentDiagnosticReport(
                doc_report,
            ) => {
                format!("{}\n\tUNCHANGED", doc_report.uri)
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

fn condensed_full_document_diagnostic_report(report: FullDocumentDiagnosticReport) -> Vec<String> {
    report
        .items
        .into_iter()
        .map(|d| {
            let range = format!(
                "{start_line}:{start_char}..{end_line}:{end_char}",
                start_line = d.range.start.line,
                start_char = d.range.start.character,
                end_line = d.range.end.line,
                end_char = d.range.end.character,
            );
            let severity = match d.severity {
                Some(DiagnosticSeverity::Error) => "ERROR",
                Some(DiagnosticSeverity::Warning) => "WARNING",
                Some(DiagnosticSeverity::Information) => "INFORMATION",
                Some(DiagnosticSeverity::Hint) => "HINT",
                Some(DiagnosticSeverity::Custom(_)) | None => "unknown",
            };
            let Message::String(message) = d.message else {
                panic!(
                    "Only string-type diagnostic messages supported, got: {:?}",
                    d.message
                );
            };
            format!("{range}[{severity}]: {message}")
        })
        .collect()
}
