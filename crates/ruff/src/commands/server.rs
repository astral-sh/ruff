use std::num::NonZeroUsize;

use crate::ExitStatus;
use anyhow::Result;
use ruff_server::Server;

pub(crate) fn run_server(
    worker_threads: NonZeroUsize,
    preview: Option<bool>,
) -> Result<ExitStatus> {
    let server = Server::new(worker_threads, preview)?;

    server.run().map(|()| ExitStatus::Success)
}
