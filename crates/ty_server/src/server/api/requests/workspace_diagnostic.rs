use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::server::Result;
use crate::server::api::diagnostics::to_lsp_diagnostic;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;
use crate::system::file_to_url;
use lsp_types::request::WorkspaceDiagnosticRequest;
use lsp_types::{
    FullDocumentDiagnosticReport, UnchangedDocumentDiagnosticReport, Url,
    WorkspaceDiagnosticParams, WorkspaceDiagnosticReport, WorkspaceDiagnosticReportResult,
    WorkspaceDocumentDiagnosticReport, WorkspaceFullDocumentDiagnosticReport,
    WorkspaceUnchangedDocumentDiagnosticReport,
};

pub(crate) struct WorkspaceDiagnosticRequestHandler;

impl RequestHandler for WorkspaceDiagnosticRequestHandler {
    type RequestType = WorkspaceDiagnosticRequest;
}

impl BackgroundRequestHandler for WorkspaceDiagnosticRequestHandler {
    fn run(
        snapshot: SessionSnapshot,
        _client: &Client,
        params: WorkspaceDiagnosticParams,
    ) -> Result<WorkspaceDiagnosticReportResult> {
        let index = snapshot.index();

        if !index.global_settings().diagnostic_mode().is_workspace() {
            // VS Code sends us the workspace diagnostic request every 2 seconds, so these logs can
            // be quite verbose.
            tracing::debug!("Workspace diagnostics is disabled; returning empty report");
            return Ok(WorkspaceDiagnosticReportResult::Report(
                WorkspaceDiagnosticReport { items: vec![] },
            ));
        }

        // Create a map of previous result IDs for efficient lookup
        let mut previous_results: BTreeMap<_, _> = params
            .previous_result_ids
            .into_iter()
            .map(|prev| (prev.uri, prev.value))
            .collect();

        let mut items = Vec::new();

        for db in snapshot.projects() {
            let diagnostics = db.check();

            // Group diagnostics by URL
            let mut diagnostics_by_url: BTreeMap<Url, Vec<_>> = BTreeMap::default();

            for diagnostic in diagnostics {
                if let Some(span) = diagnostic.primary_span() {
                    let file = span.expect_ty_file();
                    let Some(url) = file_to_url(db, file) else {
                        tracing::debug!("Failed to convert file to URL at {}", file.path(db));
                        continue;
                    };
                    diagnostics_by_url.entry(url).or_default().push(diagnostic);
                }
            }

            items.reserve(diagnostics_by_url.len());

            // Convert to workspace diagnostic report format
            for (url, file_diagnostics) in diagnostics_by_url {
                let version = index
                    .key_from_url(url.clone())
                    .ok()
                    .and_then(|key| index.make_document_ref(key).ok())
                    .map(|doc| i64::from(doc.version()));

                // Generate result ID based on raw diagnostic content only
                let mut hasher = DefaultHasher::new();
                file_diagnostics.hash(&mut hasher);
                let result_id = format!("{:x}", hasher.finish());

                // Convert diagnostics to LSP format
                let lsp_diagnostics = file_diagnostics
                    .into_iter()
                    .map(|diagnostic| {
                        to_lsp_diagnostic(db, &diagnostic, snapshot.position_encoding())
                    })
                    .collect::<Vec<_>>();

                // Check if this file's diagnostics have changed since the previous request
                if let Some(previous_result_id) = previous_results.remove(&url) {
                    if previous_result_id == result_id {
                        // Diagnostics haven't changed, return unchanged report
                        items.push(WorkspaceDocumentDiagnosticReport::Unchanged(
                            WorkspaceUnchangedDocumentDiagnosticReport {
                                uri: url,
                                version,
                                unchanged_document_diagnostic_report:
                                    UnchangedDocumentDiagnosticReport { result_id },
                            },
                        ));
                        continue;
                    }
                }

                // Diagnostics have changed or this is the first request, return full report
                items.push(WorkspaceDocumentDiagnosticReport::Full(
                    WorkspaceFullDocumentDiagnosticReport {
                        uri: url,
                        version,
                        full_document_diagnostic_report: FullDocumentDiagnosticReport {
                            result_id: Some(result_id),
                            items: lsp_diagnostics,
                        },
                    },
                ));
            }
        }

        // Handle files that had diagnostics in previous request but no longer have any
        // Any remaining entries in previous_results are files that were fixed
        for (previous_url, _previous_result_id) in previous_results {
            // This file had diagnostics before but doesn't now, so we need to report it as having no diagnostics
            let version = index
                .key_from_url(previous_url.clone())
                .ok()
                .and_then(|key| index.make_document_ref(key).ok())
                .map(|doc| i64::from(doc.version()));

            items.push(WorkspaceDocumentDiagnosticReport::Full(
                WorkspaceFullDocumentDiagnosticReport {
                    uri: previous_url,
                    version,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: None, // No result ID needed for empty diagnostics
                        items: vec![],   // No diagnostics
                    },
                },
            ));
        }

        Ok(WorkspaceDiagnosticReportResult::Report(
            WorkspaceDiagnosticReport { items },
        ))
    }
}

impl RetriableRequestHandler for WorkspaceDiagnosticRequestHandler {
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
