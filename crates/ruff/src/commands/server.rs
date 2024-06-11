use std::num::NonZeroUsize;

use crate::ExitStatus;
use anyhow::Result;
use ruff_server::Server;

pub(crate) fn run_server(preview: bool, worker_threads: NonZeroUsize) -> Result<ExitStatus> {
    if !preview {
        tracing::error!("--preview needs to be provided as a command line argument while the server is still unstable.\nFor example: `ruff server --preview`");
        return Ok(ExitStatus::Error);
    }

    let server = Server::new(worker_threads)?;

    server.run().map(|()| ExitStatus::Success)
}
