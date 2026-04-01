use std::borrow::Cow;

use crate::document::RangeExt;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use lsp_types::request::Request;
use lsp_types::{Range, TextDocumentIdentifier, Url};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::find_node::covering_node;
use serde::{Deserialize, Serialize};
use ty_project::ProjectDatabase;
use ty_python_semantic::{DisplaySettings, HasType, SemanticModel};

pub(crate) struct ProvideTypeRequestHandler;

#[derive(Debug)]
pub enum ProvideTypeRequest {}

impl Request for ProvideTypeRequest {
    type Params = ProvideTypeParams;
    type Result = Option<ProvideTypeResponse>;
    const METHOD: &'static str = "types/provide-type";
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvideTypeParams {
    /// The text document.
    pub text_document: TextDocumentIdentifier,

    /// The ranges inside the text document.
    pub ranges: Vec<Range>,
}

#[derive(Debug, Eq, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvideTypeResponse {
    /// Fully qualified names of the types, one per input range
    pub types: Vec<Option<String>>,
}

impl RequestHandler for ProvideTypeRequestHandler {
    type RequestType = ProvideTypeRequest;
}

impl BackgroundDocumentRequestHandler for ProvideTypeRequestHandler {
    fn document_url(params: &ProvideTypeParams) -> Cow<'_, Url> {
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

        let parsed = parsed_module(db, file).load(db);
        let url = &params.text_document.uri;

        let model = SemanticModel::new(db, file);

        let types: Vec<Option<String>> = params
            .ranges
            .iter()
            .map(|range| {
                let range_offset = if let Some(range_offset) =
                    range.to_text_range(db, file, url, snapshot.encoding())
                {
                    range_offset
                } else {
                    return None;
                };

                let covering_node = covering_node(parsed.syntax().into(), range_offset);
                let node = match covering_node.find_first(|node| node.is_expression()) {
                    Ok(found) => found.node(),
                    Err(_) => return None,
                };
                let ty = node.as_expr_ref()?.inferred_type(&model)?;

                Some(
                    ty.display_with(db, DisplaySettings::default().fully_qualified())
                        .to_string(),
                )
            })
            .collect();

        Ok(Some(ProvideTypeResponse { types }))
    }
}

impl RetriableRequestHandler for ProvideTypeRequestHandler {}
