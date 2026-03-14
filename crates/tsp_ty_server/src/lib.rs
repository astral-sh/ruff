//! TSP (Type Server Protocol) wrapper for `ty_server`.
//!
//! This crate provides a wrapper around `ty_server` that adds support for
//! Pylance's Type Server Protocol (TSP). TSP requests (`typeServer/*`) are
//! handled via a `RequestExtension` that integrates directly into `ty_server`'s
//! main loop, providing read-only access to project database snapshots.
//!
//! # Architecture
//!
//! The server uses `ty_server`'s extension mechanism:
//! 1. `TspExtension` implements `RequestExtension` to handle `typeServer/*` methods
//! 2. `ty_server` calls the extension for any method it doesn't recognize
//! 3. The extension receives read-only project database snapshots for type queries
//!
//! ```text
//! ┌─────────────┐     ┌─────────────────────────────────────┐
//! │   Client    │────▶│  ty_server + TspExtension           │
//! │  (stdio)    │◀────│  (handles LSP + typeServer/*)       │
//! └─────────────┘     └─────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use tsp_ty_server::run_server;
//!
//! fn main() -> anyhow::Result<()> {
//!     run_server()
//! }
//! ```

mod extension;
mod handlers;
mod snapshot;
mod stub_generator;
mod type_serializer;
mod typeshed_cache;

// type_serializer is used internally by handlers

pub use extension::TspExtension;
pub use snapshot::SnapshotManager;

use std::{num::NonZeroUsize, sync::Arc};

use anyhow::Context;
use lsp_server::Connection;
use ruff_db::system::{OsSystem, SystemPathBuf};

/// Run the TSP-enabled ty server.
///
/// This is the main entry point for the `tsp-ty` binary. It sets up the
/// server with TSP support and runs the main loop.
///
/// # Architecture
///
/// This function creates a `ty_server` with `TspExtension`:
/// 1. Creates a stdio connection to the client
/// 2. Creates a `TspExtension` to handle `typeServer/*` requests
/// 3. Runs `ty_server` with the extension
pub fn run_server() -> anyhow::Result<()> {
    let four = NonZeroUsize::new(4).unwrap();

    // By default, set the number of worker threads to `num_cpus`, with a maximum of 4.
    let worker_threads = std::thread::available_parallelism()
        .unwrap_or(four)
        .min(four);

    let cwd = {
        let cwd = std::env::current_dir().context("Failed to get the current working directory")?;
        SystemPathBuf::from_path_buf(cwd).map_err(|path| {
            anyhow::anyhow!(
                "The current working directory `{}` contains non-Unicode characters. \
                    tsp-ty only supports Unicode paths.",
                path.display()
            )
        })?
    };

    // This is to complement the `LSPSystem` if the document is not available in the index.
    let fallback_system = Arc::new(OsSystem::new(cwd));

    // Create the stdio connection
    let (connection, io_threads) = Connection::stdio();

    // Create the TSP extension
    let snapshot_manager = SnapshotManager::new();
    let extension = Arc::new(TspExtension::new(snapshot_manager));

    // Run ty_server with the TSP extension
    let server_result = ty_server::Server::with_extension(
        worker_threads,
        connection,
        fallback_system,
        false,
        Some(extension),
    )
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
        tracing::warn!("TSP server shut down with an error: {err}");
    } else {
        tracing::info!("TSP server shut down");
    }

    result
}
