use crate::server::client::Notifier;
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct Cancel;

impl super::NotificationHandler for Cancel {
    type NotificationType = notif::Cancel;
}

impl super::SyncNotificationHandler for Cancel {
    #[tracing::instrument(skip_all)]
    fn run(
        _session: &mut Session,
        _notifier: Notifier,
        _params: types::CancelParams,
    ) -> Result<()> {
        // TODO(jane): Handle this once we have task cancellation in the scheduler.
        Ok(())
    }
}
