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
//! notification, allowing them to perform operations without blocking the main loop. There is also
//! the [`BackgroundRequestHandler`] trait for handlers that operate on the entire session, which
//! includes all the workspaces, instead of a single document and can also be executed on a
//! background thread like fetching the workspace diagnostics.
//!
//! The [`RetriableRequestHandler`] trait is a marker trait for handlers that can be retried if the
//! Salsa database is modified during execution.
//!
//! The [`SyncNotificationHandler`] is the most common trait that would be used because most
//! notifications are specific to a single document and require updating the server state.
//! Similarly, the [`BackgroundDocumentRequestHandler`] is the most common request handler that
//! would be used as most requests are document-specific and can be executed in the background.
//!
//! See the `./requests` and `./notifications` directories for concrete implementations of these
//! traits in action.

use crate::session::client::Client;
use crate::session::{DocumentSnapshot, Session, SessionSnapshot};
use lsp_server::RequestId;
use std::borrow::Cow;

use lsp_types::Url;
use lsp_types::notification::Notification;
use lsp_types::request::Request;
use ty_project::ProjectDatabase;

/// A supertrait for any server request handler.
pub(super) trait RequestHandler {
    type RequestType: Request;
    const METHOD: &'static str = <<Self as RequestHandler>::RequestType>::METHOD;
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

pub(super) trait RetriableRequestHandler: RequestHandler {
    /// Whether this request can be cancelled if the Salsa database is modified.
    const RETRY_ON_CANCELLATION: bool = false;

    /// The error to return if the request was cancelled due to a modification to the Salsa
    /// database.
    ///
    /// By default, this returns a [`ContentModified`] error to indicate that the content of a
    /// document has changed since the request was made.
    ///
    /// [`ContentModified`]: lsp_server::ErrorCode::ContentModified
    fn salsa_cancellation_error() -> lsp_server::ResponseError {
        lsp_server::ResponseError {
            code: lsp_server::ErrorCode::ContentModified as i32,
            message: "content modified".to_string(),
            data: None,
        }
    }
}

/// A request handler that can be run on a background thread.
///
/// This handler is specific to requests that operate on a single document.
pub(super) trait BackgroundDocumentRequestHandler: RetriableRequestHandler {
    /// Returns the URL of the document that this request handler operates on.
    fn document_url(
        params: &<<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> Cow<'_, Url>;

    /// Processes the request parameters and returns the LSP request result.
    ///
    /// This is the main method that handlers implement. It takes the request parameters
    /// from the client and computes the appropriate response data for the LSP request.
    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        client: &Client,
        params: <<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> super::Result<<<Self as RequestHandler>::RequestType as Request>::Result>;

    /// Handles the entire request lifecycle and sends the response to the client.
    ///
    /// It allows handlers to customize how the server sends the response to the client.
    fn handle_request(
        id: &RequestId,
        db: &ProjectDatabase,
        snapshot: DocumentSnapshot,
        client: &Client,
        params: <<Self as RequestHandler>::RequestType as Request>::Params,
    ) {
        let result = Self::run_with_snapshot(db, &snapshot, client, params);

        if let Err(err) = &result {
            tracing::error!("An error occurred with request ID {id}: {err}");
            client.show_error_message("ty encountered a problem. Check the logs for more details.");
        }

        client.respond(id, result);
    }
}

/// A request handler that can be run on a background thread.
///
/// Unlike [`BackgroundDocumentRequestHandler`], this handler operates on the entire session,
/// which includes all the workspaces, without being tied to a specific document. It is useful for
/// operations that require access to the entire session state, such as fetching workspace
/// diagnostics.
pub(super) trait BackgroundRequestHandler: RetriableRequestHandler {
    /// Processes the request parameters and returns the LSP request result.
    ///
    /// This is the main method that handlers implement. It takes the request parameters
    /// from the client and computes the appropriate response data for the LSP request.
    fn run(
        snapshot: &SessionSnapshot,
        client: &Client,
        params: <<Self as RequestHandler>::RequestType as Request>::Params,
    ) -> super::Result<<<Self as RequestHandler>::RequestType as Request>::Result>;

    /// Handles the request lifecycle and sends the response to the client.
    ///
    /// It allows handlers to customize how the server sends the response to the client.
    fn handle_request(
        id: &RequestId,
        snapshot: SessionSnapshot,
        client: &Client,
        params: <<Self as RequestHandler>::RequestType as Request>::Params,
    ) {
        let result = Self::run(&snapshot, client, params);

        if let Err(err) = &result {
            tracing::error!("An error occurred with request ID {id}: {err}");
            client.show_error_message("ty encountered a problem. Check the logs for more details.");
        }

        client.respond(id, result);
    }
}

/// A supertrait for any server notification handler.
pub(super) trait NotificationHandler {
    type NotificationType: Notification;
    const METHOD: &'static str = <<Self as NotificationHandler>::NotificationType>::METHOD;
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
        params: <<Self as NotificationHandler>::NotificationType as Notification>::Params,
    ) -> super::Result<()>;
}

/// A notification handler that can be run on a background thread.
pub(super) trait BackgroundDocumentNotificationHandler: NotificationHandler {
    /// Returns the URL of the document that this notification handler operates on.
    fn document_url(
        params: &<<Self as NotificationHandler>::NotificationType as Notification>::Params,
    ) -> Cow<'_, Url>;

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        client: &Client,
        params: <<Self as NotificationHandler>::NotificationType as Notification>::Params,
    ) -> super::Result<()>;
}
