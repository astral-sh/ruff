use crate::server::Server;
use anyhow::Context;
pub use document::{DocumentKey, NotebookDocument, PositionEncoding, TextDocument};
pub use session::{ClientSettings, DocumentQuery, DocumentSnapshot, Session};
use std::num::NonZeroUsize;

#[macro_use]
mod message;

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

pub(crate) fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn run_server() -> anyhow::Result<()> {
    let four = NonZeroUsize::new(4).unwrap();

    // by default, we set the number of worker threads to `num_cpus`, with a maximum of 4.
    let worker_threads = std::thread::available_parallelism()
        .unwrap_or(four)
        .min(four);

    Server::new(worker_threads)
        .context("Failed to start server")?
        .run()?;

    Ok(())
}
