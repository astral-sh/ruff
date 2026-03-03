use std::borrow::Cow;

use lsp_types::request::TypeHierarchyPrepare;
use lsp_types::{TypeHierarchyItem, TypeHierarchyPrepareParams, Url};
use ty_project::ProjectDatabase;

use crate::document::PositionExt;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::server::api::type_hierarchy::convert_to_lsp_item;
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

/// Handles a `textDocument/prepareTypeHierarchy` request.
///
/// This is the "initial" request for identifying the type hierarchy of a
/// symbol in a document. In particular, it identifies the actual target based
/// on the current cursor position and returns a single "type hierarchy item"
/// corresponding to that symbol.
///
/// From there, a subsequent request can be made by the client to get either
/// the subtypes or supertypes of that symbol.
pub(crate) struct PrepareTypeHierarchyRequestHandler;

impl RequestHandler for PrepareTypeHierarchyRequestHandler {
    type RequestType = TypeHierarchyPrepare;
}

impl BackgroundDocumentRequestHandler for PrepareTypeHierarchyRequestHandler {
    fn document_url(params: &TypeHierarchyPrepareParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: TypeHierarchyPrepareParams,
    ) -> crate::server::Result<Option<Vec<TypeHierarchyItem>>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let Some(offset) = params.text_document_position_params.position.to_text_size(
            db,
            file,
            snapshot.url(),
            snapshot.encoding(),
        ) else {
            return Ok(None);
        };

        let Some(item) = ty_ide::prepare_type_hierarchy(db, file, offset) else {
            return Ok(None);
        };

        let Some(lsp_item) = convert_to_lsp_item(db, item, snapshot.encoding()) else {
            return Ok(None);
        };

        Ok(Some(vec![lsp_item]))
    }
}

impl RetriableRequestHandler for PrepareTypeHierarchyRequestHandler {}
