use crate::PositionEncoding;
use crate::server::api::diagnostics::{Diagnostics, to_lsp_diagnostic};
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::server::lazy_work_done_progress::LazyWorkDoneProgress;
use crate::server::{Action, Result};
use crate::session::client::Client;
use crate::session::index::Index;
use crate::session::{SessionSnapshot, SuspendedWorkspaceDiagnosticRequest};
use crate::system::file_to_url;
use lsp_server::RequestId;
use lsp_types::request::WorkspaceDiagnosticRequest;
use lsp_types::{
    FullDocumentDiagnosticReport, PreviousResultId, ProgressToken,
    UnchangedDocumentDiagnosticReport, Url, WorkspaceDiagnosticParams, WorkspaceDiagnosticReport,
    WorkspaceDiagnosticReportPartialResult, WorkspaceDiagnosticReportResult,
    WorkspaceDocumentDiagnosticReport, WorkspaceFullDocumentDiagnosticReport,
    WorkspaceUnchangedDocumentDiagnosticReport, notification::Notification,
};
use ruff_db::diagnostic::Diagnostic;
use ruff_db::files::File;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use ty_project::{Db, ProgressReporter};

pub(crate) struct WorkspaceDiagnosticRequestHandler;

impl RequestHandler for WorkspaceDiagnosticRequestHandler {
    type RequestType = WorkspaceDiagnosticRequest;
}

impl BackgroundRequestHandler for WorkspaceDiagnosticRequestHandler {
    fn run(
        snapshot: &SessionSnapshot,
        client: &Client,
        params: WorkspaceDiagnosticParams,
    ) -> Result<WorkspaceDiagnosticReportResult> {
        tracing::debug!("Computing workspace diagnostics");
        let index = snapshot.index();

        if !index.global_settings().diagnostic_mode().is_workspace() {
            tracing::debug!("Workspace diagnostics is disabled; returning empty report");
            return Ok(WorkspaceDiagnosticReportResult::Report(
                WorkspaceDiagnosticReport { items: vec![] },
            ));
        }

        let writer = ResponseWriter::new(
            params.partial_result_params.partial_result_token,
            params.previous_result_ids,
            snapshot,
            client,
        );

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
        let mut reporter = WorkspaceDiagnosticsProgressReporter::new(work_done_progress, writer);

        for db in snapshot.projects() {
            db.check_with_reporter(&mut reporter);
        }

        Ok(reporter.into_final_report())
    }

    fn process(
        id: &RequestId,
        snapshot: SessionSnapshot,
        client: &Client,
        params: WorkspaceDiagnosticParams,
    ) {
        // if streaming: it's a partial result if we did send any changes to the client but a regular report otherwise.
        // for non-streaming, it's always a full report.
        // We can simply test if all items are unchanged
        let result = Self::run(&snapshot, client, params.clone());

        // Test if this is a no-op result, in which case we should long-poll the request and
        // only respond once some diagnostics have changed to get the latest result ids.
        //
        // Bull response: This the simple case. Simply test if all diagnostics are unchanged (or empty)
        // Streaming: This trickier but follows the same principle.
        // * If the server sent any partial results, then `result` is a `Partial` result (in which
        //   case we shouldn't do any long polling because some diagnostics changed).
        // * If this is a full report, then check if all items are unchanged (or empty), the same as for
        //   the non-streaming case.
        if let Ok(WorkspaceDiagnosticReportResult::Report(full)) = &result {
            let all_unchanged = full
                .items
                .iter()
                .all(|item| matches!(item, WorkspaceDocumentDiagnosticReport::Unchanged(_)));

            if all_unchanged {
                tracing::debug!(
                    "Suspending workspace diagnostic request, all diagnostics are unchanged or the project has no diagnostics"
                );

                client.queue_action(Action::SuspendWorkspaceDiagnostics(Box::new(
                    SuspendedWorkspaceDiagnosticRequest {
                        id: id.clone(),
                        params: serde_json::to_value(&params).unwrap(),
                        revision: snapshot.revision(),
                    },
                )));

                // Don't respond, keep the response open (long polling).
                return;
            }

            tracing::debug!(
                "Respond to workspace diagnostic request with full report because some diagnostics changed"
            );
        } else {
            tracing::debug!(
                "Respond to workspace diagnostic request because it's a partial result or an error"
            );
        }

        client.respond(id, result);
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

/// ty progress reporter that streams the diagnostics to the client
/// and sends progress reports (checking X/Y files).
///
/// Diagnostics are only streamed if the client sends a partial result token.
struct WorkspaceDiagnosticsProgressReporter<'a> {
    total_files: usize,
    checked_files: AtomicUsize,
    work_done: LazyWorkDoneProgress,
    response: std::sync::Mutex<ResponseWriter<'a>>,
}

impl<'a> WorkspaceDiagnosticsProgressReporter<'a> {
    fn new(work_done: LazyWorkDoneProgress, response: ResponseWriter<'a>) -> Self {
        Self {
            total_files: 0,
            checked_files: AtomicUsize::new(0),
            work_done,
            response: std::sync::Mutex::new(response),
        }
    }

    fn into_final_report(self) -> WorkspaceDiagnosticReportResult {
        let writer = self.response.into_inner().unwrap();
        writer.into_final_report()
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

impl ProgressReporter for WorkspaceDiagnosticsProgressReporter<'_> {
    fn set_files(&mut self, files: usize) {
        self.total_files += files;
        self.report_progress();
    }

    fn report_checked_file(&self, db: &dyn Db, file: File, diagnostics: &[Diagnostic]) {
        let checked = self.checked_files.fetch_add(1, Ordering::Relaxed) + 1;

        if checked % 100 == 0 || checked == self.total_files {
            // Report progress every 100 files or when all files are checked
            self.report_progress();
        }

        let mut response = self.response.lock().unwrap();

        // Don't report empty diagnostics. We clear previous diagnostics in `into_response`
        // which also handles the case where a file no longer has diagnostics because
        // it's no longer part of the project.
        if !diagnostics.is_empty() {
            response.write_diagnostics_for_file(db, file, diagnostics);
        }

        response.maybe_flush();
    }

    fn report_diagnostics(&mut self, db: &dyn Db, diagnostics: Vec<Diagnostic>) {
        let mut by_file: BTreeMap<File, Vec<Diagnostic>> = BTreeMap::new();

        for diagnostic in diagnostics {
            if let Some(file) = diagnostic.primary_span().map(|span| span.expect_ty_file()) {
                by_file.entry(file).or_default().push(diagnostic);
            } else {
                tracing::debug!(
                    "Ignoring diagnostic without a file: {diagnostic}",
                    diagnostic = diagnostic.primary_message()
                );
            }
        }

        let response = self.response.get_mut().unwrap();

        for (file, diagnostics) in by_file {
            response.write_diagnostics_for_file(db, file, &diagnostics);
        }
        response.maybe_flush();
    }
}

#[derive(Debug)]
struct ResponseWriter<'a> {
    mode: ReportingMode,
    index: &'a Index,
    position_encoding: PositionEncoding,
    previous_result_ids: BTreeMap<Url, String>,
}

impl<'a> ResponseWriter<'a> {
    fn new(
        partial_result_token: Option<ProgressToken>,
        previous_result_ids: Vec<PreviousResultId>,
        snapshot: &'a SessionSnapshot,
        client: &Client,
    ) -> Self {
        let index = snapshot.index();
        let position_encoding = snapshot.position_encoding();

        let mode = if let Some(token) = partial_result_token {
            ReportingMode::Streaming(Streaming {
                first: true,
                client: client.clone(),
                token,
                is_test: snapshot.in_test(),
                last_flush: Instant::now(),
                changed: Vec::new(),
                unchanged: Vec::with_capacity(previous_result_ids.len()),
            })
        } else {
            ReportingMode::Bulk(Vec::new())
        };

        let previous_result_ids = previous_result_ids
            .into_iter()
            .map(|prev| (prev.uri, prev.value))
            .collect();

        Self {
            mode,
            index,
            position_encoding,
            previous_result_ids,
        }
    }

    fn write_diagnostics_for_file(&mut self, db: &dyn Db, file: File, diagnostics: &[Diagnostic]) {
        let Some(url) = file_to_url(db, file) else {
            tracing::debug!("Failed to convert file to URL at {}", file.path(db));
            return;
        };

        let version = self
            .index
            .key_from_url(url.clone())
            .ok()
            .and_then(|key| self.index.make_document_ref(key).ok())
            .map(|doc| i64::from(doc.version()));

        let result_id = Diagnostics::result_id_from_hash(diagnostics);

        let is_unchanged = if let Some(previous) = self.previous_result_ids.remove(&url) {
            if previous == result_id {
                true
            } else {
                tracing::debug!(
                    "Result id for {} changed from {} to {}",
                    url,
                    previous,
                    result_id
                );
                false
            }
        } else {
            tracing::debug!("No previous result id for {}", url);
            false
        };

        // let is_unchanged = self
        //     .previous_result_ids
        //     .remove(&url)
        //     .is_some_and(|previous_result_id| previous_result_id == result_id);

        let report = if is_unchanged {
            WorkspaceDocumentDiagnosticReport::Unchanged(
                WorkspaceUnchangedDocumentDiagnosticReport {
                    uri: url,
                    version,
                    unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                        result_id,
                    },
                },
            )
        } else {
            let lsp_diagnostics = diagnostics
                .iter()
                .map(|diagnostic| to_lsp_diagnostic(db, diagnostic, self.position_encoding))
                .collect::<Vec<_>>();

            WorkspaceDocumentDiagnosticReport::Full(WorkspaceFullDocumentDiagnosticReport {
                uri: url,
                version,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: Some(result_id),
                    items: lsp_diagnostics,
                },
            })
        };

        self.write_report(report);
    }

    fn write_report(&mut self, report: WorkspaceDocumentDiagnosticReport) {
        match &mut self.mode {
            ReportingMode::Streaming(streaming) => {
                streaming.write_report(report);
            }
            ReportingMode::Bulk(all) => {
                all.push(report);
            }
        }
    }

    /// Flush any pending reports if streaming diagnostics.
    ///
    /// Note: The flush is throttled when streaming.
    fn maybe_flush(&mut self) {
        match &mut self.mode {
            ReportingMode::Streaming(streaming) => streaming.maybe_flush(),
            ReportingMode::Bulk(_) => {}
        }
    }

    /// Creates the final response after all files have been processed.
    ///
    /// The result can be a partial or full report depending on whether the server's streaming
    /// diagnostics and if it already sent some diagnostics.
    fn into_final_report(mut self) -> WorkspaceDiagnosticReportResult {
        let mut items = Vec::new();

        // Handle files that had diagnostics in previous request but no longer have any
        // Any remaining entries in previous_results are files that were fixed
        for (previous_url, previous_result_id) in self.previous_result_ids {
            // This file had diagnostics before but doesn't now, so we need to report it as having no diagnostics
            let version = self
                .index
                .key_from_url(previous_url.clone())
                .ok()
                .and_then(|key| self.index.make_document_ref(key).ok())
                .map(|doc| i64::from(doc.version()));

            tracing::debug!("Reporting empty diagnostics for {}", previous_url);

            let new_result_id = Diagnostics::result_id_from_hash(&[]);

            // VS code keeps sending the previous result id for files that it has seen before
            // even if we sent a full report in an earlier response say that the diagnostics
            // are now empty. That's why we need to keep sending a result ID even if the diagnostics are empty.
            // so that we don't keep sending Full reports if the file still has no diagnostics (which
            // breaks long polling
            let report = if new_result_id == previous_result_id {
                WorkspaceDocumentDiagnosticReport::Unchanged(
                    WorkspaceUnchangedDocumentDiagnosticReport {
                        uri: previous_url,
                        version,
                        unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                            result_id: new_result_id,
                        },
                    },
                )
            } else {
                WorkspaceDocumentDiagnosticReport::Full(WorkspaceFullDocumentDiagnosticReport {
                    uri: previous_url,
                    version,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: Some(new_result_id),
                        items: vec![], // No diagnostics
                    },
                })
            };

            items.push(report);
        }

        match &mut self.mode {
            ReportingMode::Streaming(streaming) => {
                items.extend(
                    std::mem::take(&mut streaming.changed)
                        .into_iter()
                        .map(WorkspaceDocumentDiagnosticReport::Full),
                );
                items.extend(
                    std::mem::take(&mut streaming.unchanged)
                        .into_iter()
                        .map(WorkspaceDocumentDiagnosticReport::Unchanged),
                );
            }
            ReportingMode::Bulk(all) => {
                all.extend(items);
                items = std::mem::take(all);
            }
        }

        self.mode.create_result(items)
    }
}

#[derive(Debug)]
enum ReportingMode {
    /// Streams the diagnostics to the client as they are computed (file by file).
    /// Requires that the client provides a partial result token.
    Streaming(Streaming),

    /// For clients that don't support streaming diagnostics. Collects all workspace
    /// diagnostics and sends them in the `workspace/diagnostic` response.
    Bulk(Vec<WorkspaceDocumentDiagnosticReport>),
}

impl ReportingMode {
    fn create_result(
        &mut self,
        items: Vec<WorkspaceDocumentDiagnosticReport>,
    ) -> WorkspaceDiagnosticReportResult {
        match self {
            ReportingMode::Streaming(streaming) => streaming.create_result(items),
            ReportingMode::Bulk(..) => {
                WorkspaceDiagnosticReportResult::Report(WorkspaceDiagnosticReport { items })
            }
        }
    }
}

#[derive(Debug)]
struct Streaming {
    first: bool,
    client: Client,
    /// The partial result token.
    token: ProgressToken,
    /// Throttles the flush reports to not happen more than once every 100ms.
    last_flush: Instant,
    is_test: bool,
    /// The reports for files with changed diagnostics.
    /// The implementation uses batching to avoid too many
    /// requests for large projects (can slow down the entire
    /// analysis).
    changed: Vec<WorkspaceFullDocumentDiagnosticReport>,
    /// All the unchanged reports. Don't stream them,
    /// since nothing has changed.
    unchanged: Vec<WorkspaceUnchangedDocumentDiagnosticReport>,
}

impl Streaming {
    fn write_report(&mut self, report: WorkspaceDocumentDiagnosticReport) {
        match report {
            WorkspaceDocumentDiagnosticReport::Full(full) => {
                self.changed.push(full);
            }
            WorkspaceDocumentDiagnosticReport::Unchanged(unchanged) => {
                self.unchanged.push(unchanged);
            }
        }
    }

    fn maybe_flush(&mut self) {
        if self.changed.is_empty() {
            return;
        }

        // Flush every ~50ms or whenever we have two items and this is a test run.
        let should_flush = if self.is_test {
            self.changed.len() >= 2
        } else {
            self.last_flush.elapsed().as_millis() >= 50
        };
        if !should_flush {
            return;
        }

        let items = self
            .changed
            .drain(..)
            .map(WorkspaceDocumentDiagnosticReport::Full)
            .collect();

        let report = self.create_result(items);
        self.client
            .send_notification::<PartialWorkspaceProgress>(PartialWorkspaceProgressParams {
                token: self.token.clone(),
                value: report,
            });
        self.last_flush = Instant::now();
    }

    fn create_result(
        &mut self,
        items: Vec<WorkspaceDocumentDiagnosticReport>,
    ) -> WorkspaceDiagnosticReportResult {
        // As per the LSP spec:
        // > partial result: The first literal send need to be a WorkspaceDiagnosticReport followed
        // > by `n` WorkspaceDiagnosticReportPartialResult literals defined as follows:
        if self.first {
            self.first = false;
            WorkspaceDiagnosticReportResult::Report(WorkspaceDiagnosticReport { items })
        } else {
            WorkspaceDiagnosticReportResult::Partial(WorkspaceDiagnosticReportPartialResult {
                items,
            })
        }
    }
}

/// The `$/progress` notification for partial workspace diagnostics.
///
/// This type is missing in `lsp_types`. That's why we define it here.
pub struct PartialWorkspaceProgress;

impl Notification for PartialWorkspaceProgress {
    type Params = PartialWorkspaceProgressParams;
    const METHOD: &'static str = "$/progress";
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct PartialWorkspaceProgressParams {
    pub token: ProgressToken,
    pub value: WorkspaceDiagnosticReportResult,
}
