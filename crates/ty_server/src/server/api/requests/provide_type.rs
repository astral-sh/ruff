use std::borrow::Cow;

use crate::document::RangeExt;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;
use lsp_types::{LspRequestMethod, MessageDirection, Range, Request, TextDocumentIdentifier, Uri};
use serde::{Deserialize, Serialize};
use ty_ide::provide_type;
use ty_project::ProjectDatabase;

pub(crate) struct ProvideTypeRequestHandler;

/// The `types/provide-type` request returns public type representations for expressions in a
/// document. Each range in `ranges` identifies one expression.
///
/// This is an endpoint-specific, deliberately lossy representation. It is not a serialization of
/// ty's internal type model and should not be used as a general-purpose type display format. The
/// result is a single-line, fully-qualified, Python-derived type expression intended for clients
/// to parse. The syntax and normalization contract are documented by
/// `ty_python_semantic::types::print_type`.
///
/// Python allows multiple declarations to have the same qualified name. To distinguish them, the
/// result adds `@1`, `@2`, and so on to the duplicated name in source order. For example, two
/// module-level classes named `C` are printed as `module.C@1` and `module.C@2`. The suffix is not
/// stable across edits.
///
/// A result is `null` when the range has no expression type or no supported representation remains
/// after the documented normalizations.
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
    /// Endpoint-specific public type representations, one per input range.
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

        let types = params
            .ranges
            .iter()
            .map(|range| {
                range
                    .to_text_range(db, file, snapshot.uri(), snapshot.encoding())
                    .and_then(|range| provide_type(db, file, range))
            })
            .collect();

        Ok(Some(ProvideTypeResponse { types }))
    }
}

impl RetriableRequestHandler for ProvideTypeRequestHandler {}
