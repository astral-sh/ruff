use lsp_server as lsp;
use lsp_types::{notification::Notification, request::Request};

pub(crate) type ConnectionSender = crossbeam::channel::Sender<lsp::Message>;
type ConnectionReceiver = crossbeam::channel::Receiver<lsp::Message>;

/// A builder for `Connection` that handles LSP initialization.
pub(crate) struct ConnectionInitializer {
    connection: lsp::Connection,
}

/// Handles inbound and outbound messages with the client.
pub(crate) struct Connection {
    sender: ConnectionSender,
    receiver: ConnectionReceiver,
}

impl ConnectionInitializer {
    /// Create a new LSP server connection over stdin/stdout.
    pub(crate) fn stdio() -> (Self, lsp::IoThreads) {
        let (connection, threads) = lsp::Connection::stdio();
        (Self { connection }, threads)
    }

    /// Starts the initialization process with the client by listening for an initialization request.
    /// Returns a request ID that should be passed into `initialize_finish` later,
    /// along with the initialization parameters that were provided.
    pub(super) fn initialize_start(
        &self,
    ) -> crate::Result<(lsp::RequestId, lsp_types::InitializeParams)> {
        let (id, params) = self.connection.initialize_start()?;
        Ok((id, serde_json::from_value(params)?))
    }

    /// Finishes the initialization process with the client,
    /// returning an initialized `Connection`.
    pub(super) fn initialize_finish(
        self,
        id: lsp::RequestId,
        server_capabilities: &lsp_types::ServerCapabilities,
        name: &str,
        version: &str,
    ) -> crate::Result<Connection> {
        self.connection.initialize_finish(
            id,
            serde_json::json!({
                "capabilities": server_capabilities,
                "serverInfo": {
                    "name": name,
                    "version": version
                }
            }),
        )?;
        let Self {
            connection: lsp::Connection { sender, receiver },
        } = self;
        Ok(Connection { sender, receiver })
    }
}

impl Connection {
    /// Make a new `ClientSender` for sending messages to the client.
    pub(super) fn sender(&self) -> ConnectionSender {
        self.sender.clone()
    }

    pub(super) fn send(&self, msg: lsp::Message) -> crate::Result<()> {
        self.sender.send(msg)?;
        Ok(())
    }

    /// An iterator over incoming messages from the client.
    pub(super) fn incoming(&self) -> &crossbeam::channel::Receiver<lsp::Message> {
        &self.receiver
    }

    /// Check and respond to any incoming shutdown requests; returns`true` if the server should be shutdown.
    pub(super) fn handle_shutdown(&self, message: &lsp::Message) -> crate::Result<bool> {
        match message {
            lsp::Message::Request(lsp::Request { id, method, .. })
                if method == lsp_types::request::Shutdown::METHOD =>
            {
                self.sender
                    .send(lsp::Response::new_ok(id.clone(), ()).into())?;
                tracing::info!("Shutdown request received. Waiting for an exit notification...");

                loop {
                    match &self
                        .receiver
                        .recv_timeout(std::time::Duration::from_secs(30))?
                    {
                        lsp::Message::Notification(lsp::Notification { method, .. })
                            if method == lsp_types::notification::Exit::METHOD =>
                        {
                            tracing::info!("Exit notification received. Server shutting down...");
                            return Ok(true);
                        }
                        lsp::Message::Request(lsp::Request { id, method, .. }) => {
                            tracing::warn!(
                                "Server received unexpected request {method} ({id}) while waiting for exit notification",
                            );
                            self.sender.send(lsp::Message::Response(lsp::Response::new_err(
                                id.clone(),
                                lsp::ErrorCode::InvalidRequest as i32,
                                "Server received unexpected request while waiting for exit notification".to_string(),
                            )))?;
                        }
                        message => {
                            tracing::warn!(
                                "Server received unexpected message while waiting for exit notification: {message:?}"
                            );
                        }
                    }
                }
            }
            lsp::Message::Notification(lsp::Notification { method, .. })
                if method == lsp_types::notification::Exit::METHOD =>
            {
                anyhow::bail!(
                    "Server received an exit notification before a shutdown request was sent. Exiting..."
                );
            }
            _ => Ok(false),
        }
    }
}
