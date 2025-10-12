use crate::Session;
use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::session::client::Client;

use lsp_types::{WorkspaceDiagnosticReport, WorkspaceDiagnosticReportResult};
use salsa::Database;

pub(crate) struct ShutdownHandler;

impl RequestHandler for ShutdownHandler {
    type RequestType = lsp_types::request::Shutdown;
}

impl SyncRequestHandler for ShutdownHandler {
    fn run(session: &mut Session, client: &Client, _params: ()) -> crate::server::Result<()> {
        tracing::debug!("Received shutdown request, waiting for exit notification");

        // Respond to any pending workspace diagnostic requests
        if let Some(suspended_workspace_request) =
            session.take_suspended_workspace_diagnostic_request()
        {
            client.respond(
                &suspended_workspace_request.id,
                Ok(WorkspaceDiagnosticReportResult::Report(
                    WorkspaceDiagnosticReport::default(),
                )),
            );
        }

        session.set_shutdown_requested(true);

        // Trigger cancellation for every db to cancel any compute intensive background tasks
        // (e.g. workspace diagnostics or workspace symbols).
        for db in session.projects_mut() {
            db.trigger_cancellation();
        }

        Ok(())
    }
}
