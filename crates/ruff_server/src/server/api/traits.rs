//! Traits for handling requests and notifications from the LSP client.
//!
//! This module defines the trait abstractions used by the language server to handle incoming
//! requests and notifications from clients. It provides a type-safe way to implement LSP handlers
//! with different execution models (synchronous or asynchronous) and automatic retry capabilities.
//!
//! All request and notification handlers must implement the base traits [`RequestHandler`] and
//! [`NotificationHandler`], respectively, which associate them with specific LSP request or
//! notification types. These base traits are then extended by more specific traits that define
//! the execution model of the handler.
//!
//! The [`SyncRequestHandler`] and [`SyncNotificationHandler`] traits are for handlers that
//! executes synchronously on the main loop, providing mutable access to the [`Session`] that
//! contains the current state of the server. This is useful for handlers that need to modify
//! the server state such as when the content of a file changes.
//!
//! The [`BackgroundDocumentRequestHandler`] and [`BackgroundDocumentNotificationHandler`] traits
//! are for handlers that operate on a single document and can be executed on a background thread.
//! These handlers will have access to a snapshot of the document at the time of the request or
//! notification, allowing them to perform operations without blocking the main loop.
//!
//! The [`SyncNotificationHandler`] is the most common trait that would be used because most
//! notifications are specific to a single document and require updating the server state.
//! Similarly, the [`BackgroundDocumentRequestHandler`] is the most common request handler that
//! would be used as most requests are document-specific and can be executed in the background.
//!
//! See the `./requests` and `./notifications` directories for concrete implementations of these
//! traits in action.

use crate::session::{Client, DocumentSnapshot, Session};

use lsp_types::notification::Notification as LSPNotification;
use lsp_types::request::Request;

/// A supertrait for any server request handler.
pub(super) trait RequestHandler {
    type RequestType: Request;
    const METHOD: &'static str = <<Self as RequestHandler>::RequestType as Request>::METHOD;
}

/// A request handler that needs mutable access to the session.
///
/// This will block the main message receiver loop, meaning that no incoming requests or
/// notifications will be handled while `run` is executing. Try to avoid doing any I/O or
/// long-running computations.
pub(super) trait SyncRequestHandler: RequestHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        params: <<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> super::Result<<<Self as RequestHandler>::RequestType as Request>::Result>;
}

/// A request handler that can be run on a background thread.
///
/// This handler is specific to requests that operate on a single document.
pub(super) trait BackgroundDocumentRequestHandler: RequestHandler {
    /// Returns the URL of the document that this request handler operates on.
    ///
    /// This method can be implemented automatically using the [`define_document_url`] macro.
    ///
    /// [`define_document_url`]: super::define_document_url
    fn document_url(
        params: &<<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> std::borrow::Cow<'_, lsp_types::Url>;

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        client: &Client,
        params: <<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> super::Result<<<Self as RequestHandler>::RequestType as Request>::Result>;
}

/// A supertrait for any server notification handler.
pub(super) trait NotificationHandler {
    type NotificationType: LSPNotification;
    const METHOD: &'static str =
        <<Self as NotificationHandler>::NotificationType as LSPNotification>::METHOD;
}

/// A notification handler that needs mutable access to the session.
///
/// This will block the main message receiver loop, meaning that no incoming requests or
/// notifications will be handled while `run` is executing. Try to avoid doing any I/O or
/// long-running computations.
pub(super) trait SyncNotificationHandler: NotificationHandler {
    fn run(
        session: &mut Session,
        client: &Client,
        params: <<Self as NotificationHandler>::NotificationType as LSPNotification>::Params,
    ) -> super::Result<()>;
}

/// A notification handler that can be run on a background thread.
pub(super) trait BackgroundDocumentNotificationHandler: NotificationHandler {
    /// Returns the URL of the document that this notification handler operates on.
    ///
    /// This method can be implemented automatically using the [`define_document_url`] macro.
    ///
    /// [`define_document_url`]: super::define_document_url
    fn document_url(
        params: &<<Self as NotificationHandler>::NotificationType as LSPNotification>::Params,
    ) -> std::borrow::Cow<'_, lsp_types::Url>;

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        client: &Client,
        params: <<Self as NotificationHandler>::NotificationType as LSPNotification>::Params,
    ) -> super::Result<()>;
}
