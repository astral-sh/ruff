//! ## The Ruff Language Server

pub use edit::{DocumentKey, NotebookDocument, PositionEncoding, TextDocument};
use lsp_types::CodeActionKind;
pub use server::Server;
pub use session::{ClientSettings, DocumentQuery, DocumentSnapshot, Session};
pub use workspace::{Workspace, Workspaces};

#[macro_use]
mod message;

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
