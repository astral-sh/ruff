//! The ty language server.
//!
//! ## Request handling
//!
//! The main loop communicates with the editor and delegates code analysis to background
//! workers. When the user hovers over a name, for example, a worker looks up its type
//! while the main loop stays free to handle the user's next edit. When the worker
//! returns the result, the main loop forwards it to the editor.
//!
//! ```text
//!           buffered stdin
//! [Editor] --------------> [Reader]
//!    ^                        |
//!    |                 [[ Input | 0 ]]
//!    |                        |
//!    |                        v
//!    |                   [Main loop] -----------------------> [[ Jobs | 2048 ]]
//!    |                      |    ^                                    |
//!    |                      |    |                                    v
//!    |                      |    +-- [[ Actions | unbounded ]] <- [Workers]
//!    |                      |                                         |
//!    |                      v                                         |
//!    |               [[ Writer | 0 ]] <--- progress notifications ----+
//!    |                      |
//!    |                      v
//!    +------------------ [Writer]
//!       buffered stdout
//! ```
//!
//! **Legend**
//!
//! - `[ ]` An active component or thread.
//! - `[[ name | capacity ]]` A channel. `0` means a direct handoff, `2048` allows
//!   2,048 waiting jobs per pool, and `unbounded` means no fixed limit.
//! - `-->` Direction of message flow.
//! - `buffered stdin` and `buffered stdout` OS pipes that buffer bytes.
//!
//! ## Back pressure
//!
//! The `Jobs` queue lets a hover request wait for a worker while the main loop handles
//! other messages. If the queue is already full, the main loop replies with
//! `RequestFailed` and the editor can drop the request or retry it later. Waiting for
//! space would prevent the main loop from forwarding completed responses or handling
//! `shutdown`.
//!
//! The `Actions` queue is unbounded so workers can return results even when the main
//! loop is waiting for them. For example, applying an edit can require the main loop to
//! wait for queued jobs to release their database snapshots. If all workers were blocked
//! trying to return results, those queued jobs could never run. In practice, most jobs
//! produce a single response. When responses accumulate, the main loop drains ready
//! actions before reading more requests from the editor.
//!
//! The zero-capacity `Input` and `Writer` channels let back pressure reach the OS pipes.
//! If the main loop falls behind, the reader waits and the editor's writes block once
//! stdin fills. If the editor stops reading, stdout can fill and the writer must finish
//! its current write before it can accept another response.
//!
//! ## Request cancellation
//!
//! The main loop queues a hover request in `Jobs`. If the editor cancels the request
//! while it is pending, the main loop marks it cancelled and replies immediately with
//! a cancellation error so the editor doesn't have to wait for a worker.
//!
//! - If the job is still queued, it stays in `Jobs` until a worker picks it up. The
//!   worker checks for cancellation at that point and skips the request.
//! - If a worker has already started processing the request, it returns the result
//!   through `Actions`. Before forwarding the result, the main loop checks
//!   whether the request is still pending. It discards the result because it already
//!   sent the editor a cancellation response.
//!
//! ## Salsa cancellation
//!
//! Applying an edit can cause Salsa to interrupt a worker analyzing an older database
//! snapshot. The editor may still need the result, so some requests, such as code
//! completion, support a retry. The worker sends the request back through `Actions`,
//! and the main loop automatically retries it with a fresh snapshot. If `Jobs` is full
//! or shutdown has started, the main loop replies with the request's cancellation error
//! (`ContentModified` for completions).

use std::{num::NonZeroUsize, sync::Arc};

use anyhow::Context;
use lsp_server::Connection;
use ruff_db::system::{OsSystem, SystemPathBuf};

use crate::db::Db;
pub use crate::logging::{LogLevel, init_logging};
pub use crate::server::Server;
pub use crate::session::{ClientOptions, DiagnosticMode, GlobalOptions, WorkspaceOptions};
pub use document::{NotebookDocument, PositionEncoding, TextDocument};
pub(crate) use session::Session;

mod capabilities;
mod db;
mod document;
mod logging;
mod server;
mod session;
mod system;

const SERVER_NAME: &str = "ty";
pub(crate) const DIAGNOSTIC_NAME: &str = "ty";

/// A common result type used in most cases where a
/// result type is needed.
pub(crate) type Result<T> = anyhow::Result<T>;

pub fn run_server() -> anyhow::Result<()> {
    let _ = print_interactive_warning();
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

fn print_interactive_warning() -> std::io::Result<()> {
    use std::io::{IsTerminal, Write};

    if std::io::stdin().is_terminal() {
        let mut stderr = std::io::stderr().lock();
        writeln!(
            stderr,
            "WARNING: the ty LSP server should not be run interactively"
        )?;
        writeln!(
            stderr,
            "See https://docs.astral.sh/ty/editors/ for how to configure your editor"
        )?;
    }
    Ok(())
}
