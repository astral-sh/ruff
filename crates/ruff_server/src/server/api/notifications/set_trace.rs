use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;
use lsp_types as types;
use lsp_types::notification as notif;

pub(crate) struct SetTrace;

impl super::NotificationHandler for SetTrace {
    type NotificationType = notif::SetTrace;
}

impl super::SyncNotificationHandler for SetTrace {
    fn run(
        _session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: types::SetTraceParams,
    ) -> Result<()> {
        crate::trace::set_trace_value(params.value);
        Ok(())
    }
}
