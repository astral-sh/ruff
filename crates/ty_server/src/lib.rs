use std::{num::NonZeroUsize, sync::Arc};

use anyhow::Context;
use lsp_server::Connection;
use ruff_db::system::{OsSystem, SystemPathBuf};

use crate::db::Db;
pub use crate::logging::{LogLevel, init_logging};
pub use crate::server::{PartialWorkspaceProgress, PartialWorkspaceProgressParams, Server};
pub use crate::session::{ClientOptions, DiagnosticMode, WorkspaceOptions};
pub use document::{NotebookDocument, PositionEncoding, TextDocument};
pub(crate) use session::Session;

mod capabilities;
mod db;
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

    let cwd = {
        let cwd = std::env::current_dir().context("Failed to get the current working directory")?;
        SystemPathBuf::from_path_buf(cwd).map_err(|path| {
            anyhow::anyhow!(
                "The current working directory `{}` contains non-Unicode characters. \
                    ty only supports Unicode paths.",
                path.display()
            )
        })?
    };

    // This is to complement the `LSPSystem` if the document is not available in the index.
    let fallback_system = Arc::new(OsSystem::new(cwd));

    let server_result = Server::new(worker_threads, connection, fallback_system, false)
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
