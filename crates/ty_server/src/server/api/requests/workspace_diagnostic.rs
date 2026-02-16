use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
use ruff_db::source::source_text;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use ty_project::{ProgressReporter, ProjectDatabase};

use crate::PositionEncoding;
use crate::capabilities::ResolvedClientCapabilities;
use crate::document::DocumentKey;
use crate::server::api::diagnostics::{Diagnostics, to_lsp_diagnostic};
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::server::lazy_work_done_progress::LazyWorkDoneProgress;
use crate::server::{Action, Result};
use crate::session::client::Client;
use crate::session::index::Index;
use crate::session::{GlobalSettings, SessionSnapshot, SuspendedWorkspaceDiagnosticRequest};
use crate::system::file_to_url;

/// Handler for [Workspace diagnostics](workspace-diagnostics)
///
/// Workspace diagnostics are special in many ways compared to other request handlers.
/// This is mostly due to the fact that computing them is expensive. Because of that,
/// the LSP supports multiple optimizations of which we all make use:
///
/// ## Partial results
///
/// Many clients support partial results. They allow a server
/// to send multiple responses (in the form of `$/progress` notifications) for
/// the same request. We use partial results to stream the results for
/// changed files. This has the obvious benefit is that users
/// don't need to wait for the entire check to complete before they see any diagnostics.
/// The other benefit of "chunking" the work also helps client to incrementally
/// update (and repaint) the diagnostics instead of all at once.
/// We did see lags in VS code for projects with 10k+ diagnostics before implementing
/// this improvement.
///
/// ## Result IDs
///
/// The server can compute a result id for every file which the client
/// sends back in the next pull or workspace diagnostic request. The way we use
/// the result id is that we compute a fingerprint of the file's diagnostics (a hash)
/// and compare it with the result id sent by the server. We know that
/// the diagnostics for a file are unchanged (the client still has the most recent review)
/// if the ids compare equal.
///
/// Result IDs are also useful to identify files for which ty no longer emits
/// any diagnostics. For example, file A contained a syntax error that has now been fixed
/// by the user. The client will send us a result id for file A but we won't match it with
/// any new diagnostics because all errors in the file were fixed. The fact that we can't
/// match up the result ID tells us that we need to clear the diagnostics on the client
/// side by sending an empty diagnostic report (report without any diagnostics). We'll set the
/// result id to `None` so that the client stops sending us a result id for this file.
///
/// Sending unchanged instead of the full diagnostics for files that haven't changed
/// helps reduce the data that's sent from the server to the client and it also enables long-polling
/// (see the next section).
///
/// ## Long polling
///
/// As of today (1st of August 2025), VS code's LSP client automatically schedules a
/// workspace diagnostic request every two seconds because it doesn't know *when* to pull
/// for new workspace diagnostics (it doesn't know what actions invalidate the diagnostics).
/// However, running the workspace diagnostics every two seconds is wasting a lot of CPU cycles (and battery life as a result)
/// if the user's only browsing the project (it requires ty to iterate over all files).
/// That's why we implement long polling (as recommended in the LSP) for workspace diagnostics.
///
/// The basic idea of long-polling is that the server doesn't respond if there are no diagnostics
/// or all diagnostics are unchanged. Instead, the server keeps the request open (it doesn't respond)
/// and only responses when the diagnostics change. This puts the server in full control of when
/// to recheck a workspace and a client can simply wait for the response to come in.
///
/// One challenge with long polling for ty's server architecture is that we can't just keep
/// the background thread running because holding on to the [`ProjectDatabase`] references
/// prevents notifications from acquiring the exclusive db lock (or the long polling background thread
/// panics if a notification tries to do so). What we do instead is that this request handler
/// doesn't send a response if there are no diagnostics or all are unchanged and it
/// sets a "[snapshot](SuspendedWorkspaceDiagnosticRequest)" of the workspace diagnostic request on the [`Session`].
/// The second part to this is in the notification request handling. ty retries the
/// suspended workspace diagnostic request (if any) after every notification if the notification
/// changed the [`Session`]'s state.
///
/// [workspace-diagnostics](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspace_diagnostic)
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
        if !snapshot.global_settings().diagnostic_mode().is_workspace() {
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

    fn handle_request(
        id: &RequestId,
        snapshot: SessionSnapshot,
        client: &Client,
        params: WorkspaceDiagnosticParams,
    ) {
        let result = Self::run(&snapshot, client, params.clone());

        // Test if this is a no-op result, in which case we should long-poll the request and
        // only respond once some diagnostics have changed to get the latest result ids.
        //
        // Bulk response: This the simple case. Simply test if all diagnostics are unchanged (or empty)
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

                // Don't respond, keep the request open (long polling).
                return;
            }
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
    work_done: LazyWorkDoneProgress,
    state: Mutex<ProgressReporterState<'a>>,
}

impl<'a> WorkspaceDiagnosticsProgressReporter<'a> {
    fn new(work_done: LazyWorkDoneProgress, response: ResponseWriter<'a>) -> Self {
        Self {
            state: Mutex::new(ProgressReporterState {
                total_files: 0,
                checked_files: 0,
                last_response_sent: Instant::now(),
                response,
            }),
            work_done,
        }
    }

    fn into_final_report(self) -> WorkspaceDiagnosticReportResult {
        let state = self.state.into_inner().unwrap();
        state.response.into_final_report()
    }
}

impl ProgressReporter for WorkspaceDiagnosticsProgressReporter<'_> {
    fn set_files(&mut self, files: usize) {
        let state = self.state.get_mut().unwrap();
        state.total_files += files;
        state.report_progress(&self.work_done);
    }

    fn report_checked_file(&self, db: &ProjectDatabase, file: File, diagnostics: &[Diagnostic]) {
        // Another thread might have panicked at this point because of a salsa cancellation which
        // poisoned the result. If the response is poisoned, just don't report and wait for our thread
        // to unwind with a salsa cancellation next.
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        state.checked_files += 1;

        if state.checked_files == state.total_files {
            state.report_progress(&self.work_done);
        } else if state.last_response_sent.elapsed() >= Duration::from_millis(50) {
            state.last_response_sent = Instant::now();

            state.report_progress(&self.work_done);
        }

        // Don't report empty diagnostics. We clear previous diagnostics in `into_response`
        // which also handles the case where a file no longer has diagnostics because
        // it's no longer part of the project.
        if !diagnostics.is_empty() {
            state
                .response
                .write_diagnostics_for_file(db, file, diagnostics);
        }

        state.response.maybe_flush();
    }

    fn report_diagnostics(&mut self, db: &ProjectDatabase, diagnostics: Vec<Diagnostic>) {
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

        let response = &mut self.state.get_mut().unwrap().response;

        for (file, diagnostics) in by_file {
            response.write_diagnostics_for_file(db, file, &diagnostics);
        }
        response.maybe_flush();
    }
}

struct ProgressReporterState<'a> {
    total_files: usize,
    checked_files: usize,
    last_response_sent: Instant,
    response: ResponseWriter<'a>,
}

impl ProgressReporterState<'_> {
    fn report_progress(&self, work_done: &LazyWorkDoneProgress) {
        let checked = self.checked_files;
        let total = self.total_files;

        #[allow(clippy::cast_possible_truncation)]
        let percentage = if total > 0 {
            Some((checked * 100 / total) as u32)
        } else {
            None
        };

        work_done.report_progress(format!("{checked}/{total} files"), percentage);

        if checked == total {
            work_done.set_finish_message(format!("Checked {total} files"));
        }
    }
}

#[derive(Debug)]
struct ResponseWriter<'a> {
    mode: ReportingMode,
    index: &'a Index,
    position_encoding: PositionEncoding,
    client_capabilities: ResolvedClientCapabilities,
    // It's important that we use `AnySystemPath` over `Url` here because
    // `file_to_url` isn't guaranteed to return the exact same URL as the one provided
    // by the client.
    previous_result_ids: FxHashMap<DocumentKey, (Url, String)>,
    global_settings: &'a GlobalSettings,
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
            .map(|prev| (DocumentKey::from_url(&prev.uri), (prev.uri, prev.value)))
            .collect();

        Self {
            mode,
            index,
            position_encoding,
            client_capabilities: snapshot.resolved_client_capabilities(),
            previous_result_ids,
            global_settings: snapshot.global_settings(),
        }
    }

    fn write_diagnostics_for_file(
        &mut self,
        db: &ProjectDatabase,
        file: File,
        diagnostics: &[Diagnostic],
    ) {
        let Some(url) = file_to_url(db, file) else {
            tracing::debug!("Failed to convert file path to URL at {}", file.path(db));
            return;
        };

        if source_text(db, file).is_notebook() {
            // Notebooks only support publish diagnostics.
            // and we can't convert text ranges to notebook ranges unless
            // the document is open in the editor, in which case
            // we publish the diagnostics already.
            return;
        }

        let key = DocumentKey::from_url(&url);
        let version = self
            .index
            .document_handle(&url)
            .map(|doc| i64::from(doc.version()))
            .ok();

        let result_id = Diagnostics::result_id_from_hash(diagnostics);

        let previous_result_id = self.previous_result_ids.remove(&key).map(|(_url, id)| id);

        let report = match result_id {
            Some(new_id) if Some(&new_id) == previous_result_id.as_ref() => {
                WorkspaceDocumentDiagnosticReport::Unchanged(
                    WorkspaceUnchangedDocumentDiagnosticReport {
                        uri: url,
                        version,
                        unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                            result_id: new_id,
                        },
                    },
                )
            }
            new_id => {
                let lsp_diagnostics = diagnostics
                    .iter()
                    .filter_map(|diagnostic| {
                        Some(
                            to_lsp_diagnostic(
                                db,
                                diagnostic,
                                self.position_encoding,
                                self.client_capabilities,
                                self.global_settings,
                            )?
                            .1,
                        )
                    })
                    .collect::<Vec<_>>();

                WorkspaceDocumentDiagnosticReport::Full(WorkspaceFullDocumentDiagnosticReport {
                    uri: url,
                    version,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: new_id,
                        items: lsp_diagnostics,
                    },
                })
            }
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
        for (key, (previous_url, previous_result_id)) in self.previous_result_ids {
            // This file had diagnostics before but doesn't now, so we need to report it as having no diagnostics
            let version = self
                .index
                .document(&key)
                .ok()
                .map(|doc| i64::from(doc.version()));

            let new_result_id = Diagnostics::result_id_from_hash(&[]);

            let report = match new_result_id {
                Some(new_id) if new_id == previous_result_id => {
                    WorkspaceDocumentDiagnosticReport::Unchanged(
                        WorkspaceUnchangedDocumentDiagnosticReport {
                            uri: previous_url,
                            version,
                            unchanged_document_diagnostic_report:
                                UnchangedDocumentDiagnosticReport { result_id: new_id },
                        },
                    )
                }
                new_id => {
                    WorkspaceDocumentDiagnosticReport::Full(WorkspaceFullDocumentDiagnosticReport {
                        uri: previous_url,
                        version,
                        full_document_diagnostic_report: FullDocumentDiagnosticReport {
                            result_id: new_id,
                            items: vec![], // No diagnostics
                        },
                    })
                }
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
