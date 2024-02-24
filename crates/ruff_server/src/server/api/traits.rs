//! A stateful LSP implementation that calls into the Ruff API.

use crate::server::client::{Notifier, Responder};
use crate::session::{Session, SessionSnapshot};

use lsp_server as server;
use lsp_types::notification::Notification as LSPNotification;
use lsp_types::request::Request as LSPRequest;

/// A supertrait for any server request handler.
pub(super) trait Request {
    type RequestType: LSPRequest;
    const METHOD: &'static str = <<Self as Request>::RequestType as LSPRequest>::METHOD;

    /// Tries to cast a serialized request from the server into
    /// a parameter type for a specific request handler.
    /// It is *highly* recommended to not override this function in your
    /// implementation.
    fn cast(
        request: server::Request,
    ) -> std::result::Result<
        (
            lsp_server::RequestId,
            <<Self as Request>::RequestType as LSPRequest>::Params,
        ),
        server::ExtractError<server::Request>,
    > {
        request.extract(Self::METHOD)
    }

    /// Sends back a response to the server using a [`Responder`].
    /// `R` should be the expected response type for this request.
    /// It is *highly* recommended to not override this function in your
    /// implementation.
    fn respond<R>(
        id: lsp_server::RequestId,
        result: crate::server::Result<R>,
        responder: &Responder,
    ) where
        R: serde::Serialize,
    {
        if let Err(err) = responder.respond(id, result) {
            tracing::error!("Failed to send response: {err}");
        }
    }
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
/// `document_url` can be implemented automatically with
/// `define_document_url!(params: &<YourParameterType>)` in the trait
/// implementation.
pub(super) trait BackgroundRequest: Request {
    fn document_url(
        params: &<<Self as Request>::RequestType as LSPRequest>::Params,
    ) -> &lsp_types::Url;

    fn run_with_snapshot(
        snapshot: SessionSnapshot,
        notifier: Notifier,
        params: <<Self as Request>::RequestType as LSPRequest>::Params,
    ) -> super::Result<<<Self as Request>::RequestType as LSPRequest>::Result>;
}

/// A supertrait for any server notification handler.
pub(super) trait Notification {
    type NotificationType: LSPNotification;
    const METHOD: &'static str =
        <<Self as Notification>::NotificationType as LSPNotification>::METHOD;

    /// Tries to cast a serialized request from the server into
    /// a parameter type for a specific request handler.
    /// It is *highly* recommended to not override this function in your
    /// implementation.
    fn cast(
        notification: server::Notification,
    ) -> std::result::Result<
        (
            String,
            <<Self as Notification>::NotificationType as LSPNotification>::Params,
        ),
        server::ExtractError<server::Notification>,
    > {
        Ok((
            Self::METHOD.to_string(),
            notification.extract(Self::METHOD)?,
        ))
    }

    /// This is not supposed to do anything besides reporting errors, since
    /// notifications don't need send anything back to the client.
    /// [`Notification`] needs this method for method name compatibility
    /// with [`Request`].
    /// It is *highly* recommended to not override this function in your
    /// implementation.
    fn respond(method: String, result: crate::server::Result<()>, _responder: &Responder) {
        if let Err(err) = result {
            tracing::error!("Background notification failed: {err}");
        } else {
            tracing::debug!("`{method}` notification handler finished successfully");
        }
    }
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
/// `document_url` can be implemented automatically with
/// `define_document_url!(params: &<YourParameterType>)` in the trait
/// implementation.
pub(super) trait BackgroundNotification: Notification {
    fn document_url(
        params: &<<Self as Notification>::NotificationType as LSPNotification>::Params,
    ) -> &lsp_types::Url;

    fn run_with_snapshot(
        snapshot: SessionSnapshot,
        notifier: Notifier,
        params: <<Self as Notification>::NotificationType as LSPNotification>::Params,
    ) -> super::Result<()>;
}
