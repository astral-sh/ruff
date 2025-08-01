use lsp_types::request::WorkspaceDiagnosticRequest;
use lsp_types::{
    FullDocumentDiagnosticReport, UnchangedDocumentDiagnosticReport, Url,
    WorkspaceDiagnosticParams, WorkspaceDiagnosticReport, WorkspaceDiagnosticReportResult,
    WorkspaceDocumentDiagnosticReport, WorkspaceFullDocumentDiagnosticReport,
    WorkspaceUnchangedDocumentDiagnosticReport,
};
use ruff_db::files::File;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use ty_project::ProgressReporter;

use crate::server::Result;
use crate::server::api::diagnostics::{Diagnostics, to_lsp_diagnostic};
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::server::lazy_work_done_progress::LazyWorkDoneProgress;
use crate::session::SessionSnapshot;
use crate::session::client::Client;
use crate::system::file_to_url;

pub(crate) struct WorkspaceDiagnosticRequestHandler;

impl RequestHandler for WorkspaceDiagnosticRequestHandler {
    type RequestType = WorkspaceDiagnosticRequest;
}

impl BackgroundRequestHandler for WorkspaceDiagnosticRequestHandler {
    fn run(
        snapshot: SessionSnapshot,
        client: &Client,
        params: WorkspaceDiagnosticParams,
    ) -> Result<WorkspaceDiagnosticReportResult> {
        if !snapshot.global_settings().diagnostic_mode().is_workspace() {
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

        // Use the work done progress token from the client request, if provided
        // Note: neither VS Code nor Zed currently support this,
        // see https://github.com/microsoft/vscode-languageserver-node/issues/528
        // That's why we fall back to server-initiated progress if no token is provided.
        let work_done_progress = LazyWorkDoneProgress::new(
            client,
            params.work_done_progress_params.work_done_token,
            "Checking",
            snapshot.resolved_client_capabilities(),
        );

        // Collect all diagnostics from all projects with their database references
        let mut items = Vec::new();
        let index = snapshot.index();

        for db in snapshot.projects() {
            let diagnostics = db.check_with_reporter(
                &mut WorkspaceDiagnosticsProgressReporter::new(work_done_progress.clone()),
            );

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
                let result_id = Diagnostics::result_id_from_hash(&file_diagnostics);

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

                // Convert diagnostics to LSP format
                let lsp_diagnostics = file_diagnostics
                    .into_iter()
                    .map(|diagnostic| {
                        to_lsp_diagnostic(db, &diagnostic, snapshot.position_encoding())
                    })
                    .collect::<Vec<_>>();

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

struct WorkspaceDiagnosticsProgressReporter {
    total_files: usize,
    checked_files: AtomicUsize,
    work_done: LazyWorkDoneProgress,
}

impl WorkspaceDiagnosticsProgressReporter {
    fn new(work_done: LazyWorkDoneProgress) -> Self {
        Self {
            total_files: 0,
            checked_files: AtomicUsize::new(0),
            work_done,
        }
    }

    fn report_progress(&self) {
        let checked = self.checked_files.load(Ordering::Relaxed);
        let total = self.total_files;

        #[allow(clippy::cast_possible_truncation)]
        let percentage = if total > 0 {
            Some((checked * 100 / total) as u32)
        } else {
            None
        };

        self.work_done
            .report_progress(format!("{checked}/{total} files"), percentage);

        if checked == total {
            self.work_done
                .set_finish_message(format!("Checked {total} files"));
        }
    }
}

impl ProgressReporter for WorkspaceDiagnosticsProgressReporter {
    fn set_files(&mut self, files: usize) {
        self.total_files += files;
        self.report_progress();
    }

    fn report_file(&self, _file: &File) {
        let checked = self.checked_files.fetch_add(1, Ordering::Relaxed) + 1;

        if checked % 10 == 0 || checked == self.total_files {
            // Report progress every 10 files or when all files are checked
            self.report_progress();
        }
    }
}
