//! ## The Ruff Language Server

pub use edit::{Document, PositionEncoding};
pub use server::Server;

mod edit;
mod format;
mod lint;
mod server;
mod session;

pub(crate) const SERVER_NAME: &str = "ruff";
pub(crate) const DIAGNOSTIC_NAME: &str = "Ruff";

/// A common result type used in most cases where a
/// result type is needed.
pub(crate) type Result<T> = anyhow::Result<T>;

pub(crate) fn version() -> &'static str {
    ruff_linter::VERSION
}
