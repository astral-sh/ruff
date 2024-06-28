use std::num::NonZeroUsize;

use crate::ExitStatus;
use anyhow::Result;
use ruff_server::Server;

pub(crate) fn run_server(_preview: bool, worker_threads: NonZeroUsize) -> Result<ExitStatus> {
    let server = Server::new(worker_threads)?;

    server.run().map(|()| ExitStatus::Success)
}
