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
use ty_ide::provide_types;
use ty_project::ProjectDatabase;

pub(crate) struct ProvideTypeRequestHandler;

/// The `types/provide-type` request is sent from the client to the server to get types of expressions
/// in a document.
/// Each range in `ranges` represents a start and an end of an expression for which the type
/// is requested.
///
/// A fully qualified name of a type is returned for each `range`.
/// Everything that can be fully qualified should be fully qualified, including class and function
/// names, function types, and type parameters
/// This name follows the format of Python type annotations, except in some cases in which it's easier
/// to represent them differently. For example, callables are represented
/// as `def mod.f(x: builtins.str) -> builtins.int`
///
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

        let url = Self::document_url(&params);
        let types = provide_types(
            db,
            file,
            params
                .ranges
                .iter()
                .map(|range| range.to_text_range(db, file, &url, snapshot.encoding())),
        );

        Ok(Some(ProvideTypeResponse { types }))
    }
}

impl RetriableRequestHandler for ProvideTypeRequestHandler {}
