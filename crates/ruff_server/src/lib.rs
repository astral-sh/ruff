//! ## The Ruff Language Server

use std::num::NonZeroUsize;

use anyhow::Context as _;
pub use edit::{DocumentKey, NotebookDocument, PositionEncoding, TextDocument};
use lsp_types::CodeActionKind;
pub use server::{ConnectionSender, MainLoopSender, Server};
pub use session::{Client, ClientOptions, DocumentQuery, DocumentSnapshot, GlobalOptions, Session};
pub use workspace::{Workspace, Workspaces};

use crate::server::ConnectionInitializer;

mod edit;
mod fix;
mod format;
mod lint;
mod logging;
mod resolve;
mod server;
mod session;
mod workspace;

pub(crate) const SERVER_NAME: &str = "ruff";
pub(crate) const DIAGNOSTIC_NAME: &str = "Ruff";

pub(crate) const SOURCE_FIX_ALL_RUFF: CodeActionKind = CodeActionKind::new("source.fixAll.ruff");
pub(crate) const SOURCE_ORGANIZE_IMPORTS_RUFF: CodeActionKind =
    CodeActionKind::new("source.organizeImports.ruff");
pub(crate) const NOTEBOOK_SOURCE_FIX_ALL_RUFF: CodeActionKind =
    CodeActionKind::new("notebook.source.fixAll.ruff");
pub(crate) const NOTEBOOK_SOURCE_ORGANIZE_IMPORTS_RUFF: CodeActionKind =
    CodeActionKind::new("notebook.source.organizeImports.ruff");

/// A common result type used in most cases where a
/// result type is needed.
pub(crate) type Result<T> = anyhow::Result<T>;

pub(crate) fn version() -> &'static str {
    ruff_linter::VERSION
}

pub fn run(preview: Option<bool>) -> Result<()> {
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
