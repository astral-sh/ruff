use lsp_server::{Notification, RequestId};
use serde_json::Value;

pub(crate) type ClientSender = crossbeam::channel::Sender<lsp_server::Message>;

pub(crate) struct Client {
    notifier: Notifier,
    responder: Responder,
}

#[derive(Clone)]
pub(crate) struct Notifier(ClientSender);

#[derive(Clone)]
pub(crate) struct Responder(ClientSender);

impl Client {
    pub(super) fn new(sender: &ClientSender) -> Self {
        Self {
            notifier: Notifier(sender.clone()),
            responder: Responder(sender.clone()),
        }
    }

    pub(super) fn notifier(&self) -> Notifier {
        self.notifier.clone()
    }

    pub(super) fn responder(&self) -> Responder {
        self.responder.clone()
    }
}

#[allow(dead_code)] // we'll need to use `Notifier` in the future
impl Notifier {
    pub(crate) fn notify<N>(&self, params: N::Params) -> crate::Result<()>
    where
        N: lsp_types::notification::Notification,
    {
        let method = N::METHOD.to_string();

        let message = lsp_server::Message::Notification(Notification::new(method, params));

        Ok(self.0.send(message)?)
    }

    pub(crate) fn notify_method(&self, method: String) -> crate::Result<()> {
        Ok(self
            .0
            .send(lsp_server::Message::Notification(Notification::new(
                method,
                Value::Null,
            )))?)
    }
}

impl Responder {
    pub(crate) fn respond<R>(
        &self,
        id: RequestId,
        result: crate::server::Result<R>,
    ) -> crate::Result<()>
    where
        R: serde::Serialize,
    {
        Ok(self.0.send(
            match result {
                Ok(res) => lsp_server::Response::new_ok(id, res),
                Err(crate::server::api::Error { code, error }) => {
                    lsp_server::Response::new_err(id, code as i32, format!("{error}"))
                }
            }
            .into(),
        )?)
    }
}
