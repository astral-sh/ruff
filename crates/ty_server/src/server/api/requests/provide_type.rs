use std::borrow::Cow;

use crate::document::RangeExt;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use lsp_types::{LspRequestMethod, MessageDirection, Range, Request, TextDocumentIdentifier, Uri};
use serde::{Deserialize, Serialize};
use ty_ide::provide_types;
use ty_project::ProjectDatabase;

pub(crate) struct ProvideTypeRequestHandler;

/// The `types/provide-type` request is sent from the client to the server to get types of expressions
/// in a document.
/// Each range in `ranges` represents a start and an end of an expression for which the type
/// is requested.
///
/// Each result is a complete, fully-qualified printing of the expression's type. Exact `float` and
/// `complex` instances use their public classes, runtime PEP 695 alias objects use their canonical
/// runtime class, and direct synthesized-protocol intersection constraints are omitted. No other
/// type is widened, resolved, or omitted. The formal grammar and complete normalization contract
/// are documented by `ty_python_semantic::types::print_type_for_provide_type`. A result is `null`
/// when the range has no expression type or the type cannot be printed after applying those
/// normalizations.
///
#[derive(Debug)]
pub enum ProvideTypeRequest {}

impl Request for ProvideTypeRequest {
    type Params = ProvideTypeParams;
    type Result = Option<ProvideTypeResponse>;
    const METHOD: LspRequestMethod<'static> = LspRequestMethod::new("types/provide-type");
    const MESSAGE_DIRECTION: MessageDirection = MessageDirection::ClientToServer;
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvideTypeParams {
    /// The text document.
    pub text_document: TextDocumentIdentifier,

    /// The ranges inside the text document.
    pub ranges: Vec<Range>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvideTypeResponse {
    /// Fully qualified printed types, one per input range.
    pub types: Vec<Option<String>>,
}

impl RequestHandler for ProvideTypeRequestHandler {
    type RequestType = ProvideTypeRequest;
}

impl BackgroundDocumentRequestHandler for ProvideTypeRequestHandler {
    fn document_uri(params: &ProvideTypeParams) -> Cow<'_, Uri> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: ProvideTypeParams,
    ) -> crate::server::Result<Option<ProvideTypeResponse>> {
        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let types = provide_types(
            db,
            file,
            params
                .ranges
                .iter()
                .map(|range| range.to_text_range(db, file, snapshot.uri(), snapshot.encoding())),
        );

        Ok(Some(ProvideTypeResponse { types }))
    }
}

impl RetriableRequestHandler for ProvideTypeRequestHandler {}
