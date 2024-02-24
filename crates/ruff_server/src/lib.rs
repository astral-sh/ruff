//! ## The Ruff Language Server

/* `pub use` statements */
pub use edit::{Document, PositionEncoding};
pub use server::Server;

/* modules */
mod edit;
mod format;
mod lint;
mod server;
mod session;

/* consts */
pub(crate) const SERVER_NAME: &str = "ruff";
pub(crate) const DIAGNOSTIC_NAME: &str = "Ruff";

/* types */
/// A common result type used in most cases where a
/// result type is needed.
pub(crate) type Result<T> = anyhow::Result<T>;

/* functions */
pub(crate) fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
