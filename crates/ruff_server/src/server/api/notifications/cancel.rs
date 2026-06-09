use lsp_server::RequestId;
use lsp_types::CancelNotification;
use lsp_types::CancelParams;

use crate::server::Result;
use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::session::{Client, Session};

pub(crate) struct CancelNotificationHandler;

impl NotificationHandler for CancelNotificationHandler {
    type NotificationType = CancelNotification;
}

impl SyncNotificationHandler for CancelNotificationHandler {
    fn run(session: &mut Session, client: &Client, params: CancelParams) -> Result<()> {
        let id: RequestId = match params.id {
            lsp_types::Id::Int(id) => id.into(),
            lsp_types::Id::String(id) => id.into(),
        };

        let _ = client.cancel(session, id);

        Ok(())
    }
}
