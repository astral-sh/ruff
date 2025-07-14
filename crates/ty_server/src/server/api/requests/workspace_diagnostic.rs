use std::panic::AssertUnwindSafe;

use lsp_types::request::WorkspaceDiagnosticRequest;
use lsp_types::{
    FullDocumentDiagnosticReport, Url, WorkspaceDiagnosticParams, WorkspaceDiagnosticReport,
    WorkspaceDiagnosticReportResult, WorkspaceDocumentDiagnosticReport,
    WorkspaceFullDocumentDiagnosticReport,
};
use rustc_hash::FxHashMap;
use ty_project::CheckMode;

use crate::server::Result;
use crate::server::api::diagnostics::to_lsp_diagnostic;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;
use crate::system::file_to_url;

pub(crate) struct WorkspaceDiagnosticRequestHandler;

impl RequestHandler for WorkspaceDiagnosticRequestHandler {
    type RequestType = WorkspaceDiagnosticRequest;
}

impl BackgroundRequestHandler for WorkspaceDiagnosticRequestHandler {
    fn run(
        snapshot: AssertUnwindSafe<SessionSnapshot>,
        _client: &Client,
        _params: WorkspaceDiagnosticParams,
    ) -> Result<WorkspaceDiagnosticReportResult> {
        let index = snapshot.index();

        if !index.global_settings().diagnostic_mode().is_workspace() {
            tracing::trace!("Workspace diagnostics is disabled; returning empty report");
            return Ok(WorkspaceDiagnosticReportResult::Report(
                WorkspaceDiagnosticReport { items: vec![] },
            ));
        }

        let mut items = Vec::new();

        for db in snapshot.projects() {
            let diagnostics = db.check_with_mode(CheckMode::AllFiles);

            // Group diagnostics by URL
            let mut diagnostics_by_url: FxHashMap<Url, Vec<_>> = FxHashMap::default();

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

                // Convert diagnostics to LSP format
                let lsp_diagnostics = file_diagnostics
                    .into_iter()
                    .map(|diagnostic| {
                        to_lsp_diagnostic(db, &diagnostic, snapshot.position_encoding())
                    })
                    .collect::<Vec<_>>();

                items.push(WorkspaceDocumentDiagnosticReport::Full(
                    WorkspaceFullDocumentDiagnosticReport {
                        uri: url,
                        version,
                        full_document_diagnostic_report: FullDocumentDiagnosticReport {
                            // TODO: We don't implement result ID caching yet
                            result_id: None,
                            items: lsp_diagnostics,
                        },
                    },
                ));
            }
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
