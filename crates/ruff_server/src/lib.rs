//! ## The Ruff Language Server

pub use edit::{Document, PositionEncoding};
use lsp_types::CodeActionKind;
pub use server::Server;

mod edit;
mod fix;
mod format;
mod lint;
#[macro_use]
mod message;
mod server;
mod session;

pub(crate) const SERVER_NAME: &str = "ruff";
pub(crate) const DIAGNOSTIC_NAME: &str = "Ruff";

pub(crate) const SOURCE_FIX_ALL_RUFF: CodeActionKind = CodeActionKind::new("source.fixAll.ruff");
pub(crate) const SOURCE_ORGANIZE_IMPORTS_RUFF: CodeActionKind =
    CodeActionKind::new("source.organizeImports.ruff");

/// A common result type used in most cases where a
/// result type is needed.
pub(crate) type Result<T> = anyhow::Result<T>;

pub(crate) fn version() -> &'static str {
    ruff_linter::VERSION
}
