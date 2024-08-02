#![allow(dead_code)]

pub use edit::{DocumentKey, NotebookDocument, PositionEncoding, TextDocument};
pub use server::Server;
pub use session::{ClientSettings, DocumentQuery, DocumentSnapshot, Session};

#[macro_use]
mod message;

mod edit;
mod server;
mod session;
mod system;
mod trace;

pub(crate) const SERVER_NAME: &str = "red-knot";
pub(crate) const DIAGNOSTIC_NAME: &str = "Red Knot";

/// A common result type used in most cases where a
/// result type is needed.
pub(crate) type Result<T> = anyhow::Result<T>;

pub(crate) fn version() -> &'static str {
    ruff_linter::VERSION
}
