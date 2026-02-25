use std::borrow::Cow;

use lsp_types::request::{TypeHierarchyPrepare, TypeHierarchySubtypes, TypeHierarchySupertypes};
use lsp_types::{
    SymbolKind, TypeHierarchyItem, TypeHierarchyPrepareParams, TypeHierarchySubtypesParams,
    TypeHierarchySupertypesParams, Url,
};
use ruff_db::files::{File, system_path_to_file, vendored_path_to_file};
use ruff_db::system::SystemPathBuf;
use ruff_text_size::TextSize;
use ty_project::ProjectDatabase;

use crate::PositionEncoding;
use crate::document::{PositionExt, ToRangeExt};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, BackgroundRequestHandler, RequestHandler,
    RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::SessionSnapshot;
use crate::session::client::Client;
use crate::system::file_to_url;

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

/// Handles a `typeHierarchy/supertypes` request.
///
/// Note that this implements the `BackgroundRequestHandler` because the
/// request might be for a symbol in a document that is not open in the current
/// session.
pub(crate) struct TypeHierarchySupertypesRequestHandler;

impl RequestHandler for TypeHierarchySupertypesRequestHandler {
    type RequestType = TypeHierarchySupertypes;
}

impl BackgroundRequestHandler for TypeHierarchySupertypesRequestHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: TypeHierarchySupertypesParams,
    ) -> crate::server::Result<Option<Vec<TypeHierarchyItem>>> {
        Ok(hierarchy_handler(
            snapshot,
            &params.item,
            ty_ide::type_hierarchy_supertypes,
        ))
    }
}

impl RetriableRequestHandler for TypeHierarchySupertypesRequestHandler {}

/// Handles a `typeHierarchy/subtypes` request.
///
/// Note that this implements the `BackgroundRequestHandler` because the
/// request might be for a symbol in a document that is not open in the current
/// session.
pub(crate) struct TypeHierarchySubtypesRequestHandler;

impl RequestHandler for TypeHierarchySubtypesRequestHandler {
    type RequestType = TypeHierarchySubtypes;
}

impl BackgroundRequestHandler for TypeHierarchySubtypesRequestHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: TypeHierarchySubtypesParams,
    ) -> crate::server::Result<Option<Vec<TypeHierarchyItem>>> {
        Ok(hierarchy_handler(
            snapshot,
            &params.item,
            ty_ide::type_hierarchy_subtypes,
        ))
    }
}

impl RetriableRequestHandler for TypeHierarchySubtypesRequestHandler {}

/// The subtype and supertype implementation.
///
/// `hierarchy_types` should be either `ty_ide::type_hierarchy_subtypes`
/// or `ty_ide::type_hierarchy_supertypes`.
fn hierarchy_handler(
    snapshot: &SessionSnapshot,
    requested_item: &TypeHierarchyItem,
    hierarchy_types: fn(&dyn ty_project::Db, File, TextSize) -> Vec<ty_ide::TypeHierarchyItem>,
) -> Option<Vec<TypeHierarchyItem>> {
    let encoding = snapshot.position_encoding();

    // We don't actually know which project the request
    // came from, so just look for results across all
    // projects.
    let mut items = vec![];
    for db in snapshot.projects() {
        let Some((file, offset)) = resolve_item_location(db, requested_item, encoding) else {
            continue;
        };
        items.extend(
            hierarchy_types(db, file, offset)
                .into_iter()
                .filter_map(|item| convert_to_lsp_item(db, item, encoding)),
        );
    }
    if items.is_empty() { None } else { Some(items) }
}

/// Attempts to resolve the location in the provided
/// type hierarchy item into `ty_ide` types. This includes
/// mapping system paths back into their proper vendored
/// path types (if applicable).
fn resolve_item_location(
    db: &ProjectDatabase,
    item: &TypeHierarchyItem,
    encoding: PositionEncoding,
) -> Option<(File, TextSize)> {
    let system_path = SystemPathBuf::from_path_buf(item.uri.to_file_path().ok()?).ok()?;

    let file = if let Some(ref vendored_root) = ty_ide::cached_vendored_root(db)
        && let Some(vendored_path) = ty_ide::map_system_to_vendored(vendored_root, &system_path)
    {
        match vendored_path_to_file(db, vendored_path) {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!(
                    "Could not resolve type hierarchy item location \
                     for vendored file path `{vendored_path}`: {err}"
                );
                return None;
            }
        }
    } else {
        match system_path_to_file(db, &system_path) {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!(
                    "Could not resolve type hierarchy item location \
                     for system file path `{system_path}`: {err}"
                );
                return None;
            }
        }
    };

    let offset = item
        .selection_range
        .start
        .to_text_size(db, file, &item.uri, encoding)?;
    Some((file, offset))
}

fn convert_to_lsp_item(
    db: &ProjectDatabase,
    item: ty_ide::TypeHierarchyItem,
    encoding: PositionEncoding,
) -> Option<TypeHierarchyItem> {
    let uri = file_to_url(db, item.file)?;
    let full_range = item.full_range.to_lsp_range(db, item.file, encoding)?;
    let selection_range = item.selection_range.to_lsp_range(db, item.file, encoding)?;

    Some(TypeHierarchyItem {
        name: item.name.into(),
        kind: SymbolKind::CLASS,
        tags: None,
        detail: item.detail,
        uri,
        range: full_range.local_range(),
        selection_range: selection_range.local_range(),
        data: None,
    })
}
