//! A stateful LSP implementation that calls into the Ruff API.

use crate::server::client::Notifier;
use crate::session::{DocumentSnapshot, Session};

use lsp_types::notification::Notification as LSPNotification;
use lsp_types::request::Request as LSPRequest;

/// A supertrait for any server request handler.
pub(super) trait Request {
    type RequestType: LSPRequest;
    const METHOD: &'static str = <<Self as Request>::RequestType as LSPRequest>::METHOD;
}

/// A request handler that needs mutable access to the session.
/// This will block the main message receiver loop, meaning that no
/// incoming requests or notifications will be handled while `run` is
/// executing. Try to avoid doing any I/O or long-running computations.
pub(super) trait SyncRequest: Request {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        params: <<Self as Request>::RequestType as LSPRequest>::Params,
    ) -> super::Result<<<Self as Request>::RequestType as LSPRequest>::Result>;
}

/// A request handler that can be run on a background thread.
pub(super) trait BackgroundDocumentRequest: Request {
    /// `document_url` can be implemented automatically with
    /// `define_document_url!(params: &<YourParameterType>)` in the trait
    /// implementation.
    fn document_url(
        params: &<<Self as Request>::RequestType as LSPRequest>::Params,
    ) -> &lsp_types::Url;

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        notifier: Notifier,
        params: <<Self as Request>::RequestType as LSPRequest>::Params,
    ) -> super::Result<<<Self as Request>::RequestType as LSPRequest>::Result>;
}

/// A supertrait for any server notification handler.
pub(super) trait Notification {
    type NotificationType: LSPNotification;
    const METHOD: &'static str =
        <<Self as Notification>::NotificationType as LSPNotification>::METHOD;
}

/// A notification handler that needs mutable access to the session.
/// This will block the main message receiver loop, meaning that no
/// incoming requests or notifications will be handled while `run` is
/// executing. Try to avoid doing any I/O or long-running computations.
pub(super) trait SyncNotification: Notification {
    fn run(
        session: &mut Session,
        notifier: Notifier,
        params: <<Self as Notification>::NotificationType as LSPNotification>::Params,
    ) -> super::Result<()>;
}

/// A notification handler that can be run on a background thread.
pub(super) trait BackgroundDocumentNotification: Notification {
    /// `document_url` can be implemented automatically with
    /// `define_document_url!(params: &<YourParameterType>)` in the trait
    /// implementation.
    fn document_url(
        params: &<<Self as Notification>::NotificationType as LSPNotification>::Params,
    ) -> &lsp_types::Url;

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        notifier: Notifier,
        params: <<Self as Notification>::NotificationType as LSPNotification>::Params,
    ) -> super::Result<()>;
}
