use tower_lsp::LspService;

use crate::server::Server;

mod diagnostic;
mod document;
mod encoding;
mod server;
mod session;

/// Creates a LSP server that reads from stdin and writes the output to stdout.
pub fn stdio() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let (service, socket) = LspService::new(Server::new);
        tower_lsp::Server::new(stdin, stdout, socket)
            .serve(service)
            .await;
    });
}
