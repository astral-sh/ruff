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

#[derive(Debug)]
pub(crate) enum ProvideTypeRequest {}

impl Request for ProvideTypeRequest {
    type Params = ProvideTypeParams;
    type Result = Option<ProvideTypeResponse>;
    const METHOD: LspRequestMethod<'static> = LspRequestMethod::new("types/provide-type");
    const MESSAGE_DIRECTION: MessageDirection = MessageDirection::ClientToServer;
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProvideTypeParams {
    /// The text document.
    pub text_document: TextDocumentIdentifier,

    /// The ranges inside the text document.
    pub ranges: Vec<Range>,
}

#[derive(Debug, Eq, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProvideTypeResponse {
    /// Fully qualified names of the types, one per input range
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

        let uri = Self::document_uri(&params);
        let types = provide_types(
            db,
            file,
            params
                .ranges
                .iter()
                .map(|range| range.to_text_range(db, file, &uri, snapshot.encoding())),
        );

        Ok(Some(ProvideTypeResponse { types }))
    }
}

impl RetriableRequestHandler for ProvideTypeRequestHandler {}
