use crate::Session;
use crate::server::api;
use crate::server::connection::Connection;
use crate::server::schedule::Scheduler;
use lsp_server::Message;

pub(super) fn main_loop(
    mut session: Session,
    mut scheduler: Scheduler,
    connection: &Connection,
) -> crate::Result<()> {
    for msg in connection.incoming() {
        if connection.handle_shutdown(&msg)? {
            break;
        }
        let task = match msg {
            Message::Request(req) => api::request(req),
            Message::Notification(notification) => api::notification(notification),
            Message::Response(response) => scheduler.response(response),
        };
        scheduler.dispatch(task, &mut session);
    }

    Ok(())
}
