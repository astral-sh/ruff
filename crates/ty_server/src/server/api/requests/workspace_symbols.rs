use lsp_types::request::WorkspaceSymbolRequest;
use lsp_types::{WorkspaceSymbolParams, WorkspaceSymbolResponse};
use ty_ide::{WorkspaceSymbolInfo, workspace_symbols};

use crate::server::api::symbols::convert_to_lsp_symbol_information;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;

pub(crate) struct WorkspaceSymbolRequestHandler;

impl RequestHandler for WorkspaceSymbolRequestHandler {
    type RequestType = WorkspaceSymbolRequest;
}

impl BackgroundRequestHandler for WorkspaceSymbolRequestHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: WorkspaceSymbolParams,
    ) -> crate::server::Result<Option<WorkspaceSymbolResponse>> {
        let query = &params.query;
        let mut all_symbols = Vec::new();

        // Iterate through all projects in the session
        for db in snapshot.projects() {
            // Get workspace symbols matching the query
            let start = std::time::Instant::now();
            let workspace_symbol_infos = workspace_symbols(db, query);
            tracing::debug!(
                "Found {len} workspace symbols in {elapsed:?}",
                len = workspace_symbol_infos.len(),
                elapsed = std::time::Instant::now().duration_since(start)
            );

            // Convert to LSP SymbolInformation
            for workspace_symbol_info in workspace_symbol_infos {
                let WorkspaceSymbolInfo { symbol, file } = workspace_symbol_info;

                // Get position encoding from session
                let encoding = snapshot.position_encoding();

                let Some(symbol) = convert_to_lsp_symbol_information(db, file, symbol, encoding)
                else {
                    tracing::debug!(
                        "Failed to convert symbol '{}' to LSP symbol information",
                        file.path(db)
                    );
                    continue;
                };

                all_symbols.push(symbol);
            }
        }

        if all_symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(WorkspaceSymbolResponse::Flat(all_symbols)))
        }
    }
}

impl RetriableRequestHandler for WorkspaceSymbolRequestHandler {}
