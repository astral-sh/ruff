use std::num::NonZeroUsize;

use crate::ExitStatus;
use anyhow::{Context as _, Result};
use ruff_server::Server;

pub(crate) fn run_server(
    worker_threads: NonZeroUsize,
    preview: Option<bool>,
) -> Result<ExitStatus> {
    let four = NonZeroUsize::new(4).unwrap();

    // by default, we set the number of worker threads to `num_cpus`, with a maximum of 4.
    let worker_threads = std::thread::available_parallelism()
        .unwrap_or(four)
        .min(four);

    let (connection, io_threads) = ConnectionInitializer::stdio();

    let server_result = Server::new(worker_threads, connection, preview)
        .context("Failed to start server")?
        .run();

    let io_result = io_threads.join();

    let result = match (server_result, io_result) {
        (Ok(()), Ok(())) => Ok(ExitStatus::Success),
        (Err(server), Err(io)) => Err(server).context(format!("IO thread error: {io}")),
        (Err(server), _) => Err(server),
        (_, Err(io)) => Err(io).context("IO thread error"),
    };

    if let Err(err) = result.as_ref() {
        tracing::warn!("Server shut down with an error: {err}");
    } else {
        tracing::info!("Server shut down");
    }

    result
}
