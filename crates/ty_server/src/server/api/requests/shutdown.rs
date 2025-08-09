use crate::Session;
use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::session::client::Client;

use lsp_types::{WorkspaceDiagnosticReport, WorkspaceDiagnosticReportResult};

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

        Ok(())
    }
}
