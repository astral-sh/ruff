use lsp_server as lsp;

pub type ConnectionSender = crossbeam::channel::Sender<lsp::Message>;

/// A builder for `Connection` that handles LSP initialization.
pub(crate) struct ConnectionInitializer {
    connection: lsp::Connection,
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
    ) -> crate::Result<lsp_server::Connection> {
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
        Ok(self.connection)
    }
}
