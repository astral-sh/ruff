use crate::Session;
use crate::server::api::traits::{RequestHandler, SyncRequestHandler};
use crate::session::Client;

pub(crate) struct ShutdownHandler;

impl RequestHandler for ShutdownHandler {
    type RequestType = lsp_types::request::Shutdown;
}

impl SyncRequestHandler for ShutdownHandler {
    fn run(session: &mut Session, _client: &Client, _params: ()) -> crate::server::Result<()> {
        tracing::debug!("Received shutdown request, waiting for shutdown notification");
        session.set_shutdown_requested(true);
        Ok(())
    }
}
