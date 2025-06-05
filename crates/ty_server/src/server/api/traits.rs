//! A stateful LSP implementation that calls into the ty API.

use crate::session::client::Client;
use crate::session::{DocumentSnapshot, Session};

use lsp_types::notification::Notification as LSPNotification;
use lsp_types::request::Request;
use ty_project::ProjectDatabase;

/// A supertrait for any server request handler.
pub(super) trait RequestHandler {
    type RequestType: Request;
    const METHOD: &'static str = <<Self as RequestHandler>::RequestType as Request>::METHOD;
}

/// A request handler that needs mutable access to the session.
/// This will block the main message receiver loop, meaning that no
/// incoming requests or notifications will be handled while `run` is
/// executing. Try to avoid doing any I/O or long-running computations.
pub(super) trait SyncRequestHandler: RequestHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        params: <<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> super::Result<<<Self as RequestHandler>::RequestType as Request>::Result>;
}

/// A request handler that can be run on a background thread.
pub(super) trait BackgroundDocumentRequestHandler: RequestHandler {
    /// Whether this request be retried if it was cancelled due to a modification to the Salsa database.
    const RETRY_ON_CANCELLATION: bool = false;

    fn document_url(
        params: &<<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> std::borrow::Cow<lsp_types::Url>;

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
        client: &Client,
        params: <<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> super::Result<<<Self as RequestHandler>::RequestType as Request>::Result>;

    fn salsa_cancellation_error() -> lsp_server::ResponseError {
        lsp_server::ResponseError {
            code: lsp_server::ErrorCode::ContentModified as i32,
            message: "content modified".to_string(),
            data: None,
        }
    }
}

/// A supertrait for any server notification handler.
pub(super) trait NotificationHandler {
    type NotificationType: LSPNotification;
    const METHOD: &'static str =
        <<Self as NotificationHandler>::NotificationType as LSPNotification>::METHOD;
}

/// A notification handler that needs mutable access to the session.
/// This will block the main message receiver loop, meaning that no
/// incoming requests or notifications will be handled while `run` is
/// executing. Try to avoid doing any I/O or long-running computations.
pub(super) trait SyncNotificationHandler: NotificationHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        params: <<Self as NotificationHandler>::NotificationType as LSPNotification>::Params,
    ) -> super::Result<()>;
}

/// A notification handler that can be run on a background thread.
pub(super) trait BackgroundDocumentNotificationHandler: NotificationHandler {
    /// `document_url` can be implemented automatically with
    /// `define_document_url!(params: &<YourParameterType>)` in the trait
    /// implementation.
    fn document_url(
        params: &<<Self as NotificationHandler>::NotificationType as LSPNotification>::Params,
    ) -> std::borrow::Cow<lsp_types::Url>;

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        client: &Client,
        params: <<Self as NotificationHandler>::NotificationType as LSPNotification>::Params,
    ) -> super::Result<()>;
}
