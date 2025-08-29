use std::borrow::Cow;

use lsp_types::request::DocumentSymbolRequest;
use lsp_types::{DocumentSymbol, DocumentSymbolParams, SymbolInformation, Url};
use ruff_db::source::{line_index, source_text};
use ruff_source_file::LineIndex;
use ty_ide::{HierarchicalSymbols, SymbolId, SymbolInfo, document_symbols};
use ty_project::ProjectDatabase;

use crate::document::{PositionEncoding, ToRangeExt};
use crate::server::api::symbols::{convert_symbol_kind, convert_to_lsp_symbol_information};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct DocumentSymbolRequestHandler;

impl RequestHandler for DocumentSymbolRequestHandler {
    type RequestType = DocumentSymbolRequest;
}

impl BackgroundDocumentRequestHandler for DocumentSymbolRequestHandler {
    fn document_url(params: &DocumentSymbolParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: DocumentSymbolParams,
    ) -> crate::server::Result<Option<lsp_types::DocumentSymbolResponse>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.file(db) else {
            return Ok(None);
        };

        let source = source_text(db, file);
        let line_index = line_index(db, file);

        // Check if the client supports hierarchical document symbols
        let supports_hierarchical = snapshot
            .resolved_client_capabilities()
            .supports_hierarchical_document_symbols();

        let symbols = document_symbols(db, file);
        if symbols.is_empty() {
            return Ok(None);
        }

        if supports_hierarchical {
            let symbols = symbols.to_hierarchical();
            let lsp_symbols: Vec<DocumentSymbol> = symbols
                .iter()
                .map(|(id, symbol)| {
                    convert_to_lsp_document_symbol(
                        &symbols,
                        id,
                        symbol,
                        &source,
                        &line_index,
                        snapshot.encoding(),
                    )
                })
                .collect();

            Ok(Some(lsp_types::DocumentSymbolResponse::Nested(lsp_symbols)))
        } else {
            // Return flattened symbols as SymbolInformation
            let lsp_symbols: Vec<SymbolInformation> = symbols
                .iter()
                .map(|(_, symbol)| {
                    convert_to_lsp_symbol_information(
                        symbol,
                        &params.text_document.uri,
                        &source,
                        &line_index,
                        snapshot.encoding(),
                    )
                })
                .collect();

            Ok(Some(lsp_types::DocumentSymbolResponse::Flat(lsp_symbols)))
        }
    }
}

impl RetriableRequestHandler for DocumentSymbolRequestHandler {}

fn convert_to_lsp_document_symbol(
    symbols: &HierarchicalSymbols,
    id: SymbolId,
    symbol: SymbolInfo<'_>,
    source: &str,
    line_index: &LineIndex,
    encoding: PositionEncoding,
) -> DocumentSymbol {
    let symbol_kind = convert_symbol_kind(symbol.kind);

    DocumentSymbol {
        name: symbol.name.into_owned(),
        detail: None,
        kind: symbol_kind,
        tags: None,
        #[allow(deprecated)]
        deprecated: None,
        range: symbol.full_range.to_lsp_range(source, line_index, encoding),
        selection_range: symbol.name_range.to_lsp_range(source, line_index, encoding),
        children: Some(
            symbols
                .children(id)
                .map(|(child_id, child)| {
                    convert_to_lsp_document_symbol(
                        symbols, child_id, child, source, line_index, encoding,
                    )
                })
                .collect(),
        ),
    }
}
