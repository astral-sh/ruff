use std::num::NonZeroUsize;

use anyhow::Context;
use lsp_server::Connection;

use crate::server::Server;
pub use document::{NotebookDocument, PositionEncoding, TextDocument};
pub(crate) use session::{DocumentQuery, Session};

mod document;
mod logging;
mod server;
mod session;
mod system;

pub(crate) const SERVER_NAME: &str = "ty";
pub(crate) const DIAGNOSTIC_NAME: &str = "ty";

/// A common result type used in most cases where a
/// result type is needed.
pub(crate) type Result<T> = anyhow::Result<T>;

pub fn run_server() -> anyhow::Result<()> {
    let four = NonZeroUsize::new(4).unwrap();

    // by default, we set the number of worker threads to `num_cpus`, with a maximum of 4.
    let worker_threads = std::thread::available_parallelism()
        .unwrap_or(four)
        .min(four);

    let (connection, io_threads) = Connection::stdio();

    let server_result = Server::new(worker_threads, connection)
        .context("Failed to start server")?
        .run();

    let io_result = io_threads.join();

    let result = match (server_result, io_result) {
        (Ok(()), Ok(())) => Ok(()),
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
