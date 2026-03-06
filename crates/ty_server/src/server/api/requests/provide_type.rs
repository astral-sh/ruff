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
use ruff_db::source::{line_index, source_text};
use ruff_python_ast::AnyNodeRef;
use ruff_python_ast::find_node::covering_node;
use serde::{Deserialize, Serialize};
use ty_project::ProjectDatabase;
use ty_python_semantic::types::Type;
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
    pub tys: Vec<String>, // TODO: type parameters
}

impl RequestHandler for ProvideTypeRequestHandler {
    type RequestType = ProvideTypeRequest;
}

impl BackgroundDocumentRequestHandler for ProvideTypeRequestHandler {
    fn document_url(params: &ProvideTypeParams) -> Cow<Url> {
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

        let source = source_text(db, file);
        let line_index = line_index(db, file);
        let parsed = parsed_module(db, file).load(db);
        let url = Self::document_url(&params);

        let model = SemanticModel::new(db, file);

        let tys: Vec<String> = params
            .ranges
            .iter()
            .map(|range| {
                let range_offset = if let Some(range_offset) =
                    range.to_text_range(db, file, &url, snapshot.encoding())
                {
                    range_offset
                } else {
                    return String::new();
                };

                let covering_node = covering_node(parsed.syntax().into(), range_offset);
                let node = match covering_node.find_first(|node| node.is_expression()) {
                    Ok(found) => found.node(),
                    Err(_) => return String::new(),
                };

                let ty = match node {
                    AnyNodeRef::ExprBoolOp(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprNamed(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprBinOp(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprUnaryOp(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprLambda(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprIf(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprDict(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprSet(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprListComp(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprSetComp(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprDictComp(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprGenerator(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprAwait(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprYield(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprYieldFrom(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprCompare(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprCall(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprFString(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprTString(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprStringLiteral(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprBytesLiteral(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprNumberLiteral(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprBooleanLiteral(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprNoneLiteral(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprEllipsisLiteral(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprAttribute(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprSubscript(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprStarred(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprName(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprList(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprTuple(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprSlice(expr) => expr.inferred_type(&model),
                    AnyNodeRef::ExprIpyEscapeCommand(expr) => expr.inferred_type(&model),
                    _ => return String::new(),
                };

                match ty {
                    Some(ty) => ty
                        .display_with(db, DisplaySettings::default().fully_qualified())
                        .to_string(),
                    None => String::new(),
                }
            })
            .collect();

        Ok(Some(ProvideTypeResponse { tys }))
    }
}

impl RetriableRequestHandler for ProvideTypeRequestHandler {}
