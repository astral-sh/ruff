use lsp_types::notification::SetTrace;
use lsp_types::SetTraceParams;

use crate::server::api::traits::{NotificationHandler, SyncNotificationHandler};
use crate::server::client::{Notifier, Requester};
use crate::server::Result;
use crate::session::Session;

pub(crate) struct SetTraceHandler;

impl NotificationHandler for SetTraceHandler {
    type NotificationType = SetTrace;
}

impl SyncNotificationHandler for SetTraceHandler {
    fn run(
        _session: &mut Session,
        _notifier: Notifier,
        _requester: &mut Requester,
        params: SetTraceParams,
    ) -> Result<()> {
        crate::trace::set_trace_value(params.value);
        Ok(())
    }
}
