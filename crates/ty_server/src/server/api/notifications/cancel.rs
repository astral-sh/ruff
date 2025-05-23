use lsp_server::{ErrorCode, RequestId, ResponseError};
use lsp_types::CancelParams;
use lsp_types::notification::Cancel;

use crate::client::Client;
use crate::server::Result;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::Session;

pub(crate) struct CancelNotificationHandler;

impl NotificationHandler for CancelNotificationHandler {
    type NotificationType = Cancel;
}

impl SyncNotificationHandler for CancelNotificationHandler {
    fn run(session: &mut Session, client: &Client, params: CancelParams) -> Result<()> {
        let id: RequestId = match params.id {
            lsp_types::NumberOrString::Number(id) => id.into(),
            lsp_types::NumberOrString::String(id) => id.into(),
        };

        let method_name = session.request_queue_mut().incoming_mut().cancel(&id);

        if let Some(method_name) = method_name {
            tracing::debug!("Cancelled request id={id} method={method_name}");
            let error = ResponseError {
                code: ErrorCode::RequestCanceled as i32,
                message: "request was cancelled by client".to_owned(),
                data: None,
            };

            let _ = client.respond_err(id, error);
        }

        Ok(())
    }
}
