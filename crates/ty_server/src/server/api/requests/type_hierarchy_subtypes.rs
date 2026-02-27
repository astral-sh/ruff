use lsp_types::request::TypeHierarchySubtypes;
use lsp_types::{TypeHierarchyItem, TypeHierarchySubtypesParams};

use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::server::api::type_hierarchy::hierarchy_handler;
use crate::session::SessionSnapshot;
use crate::session::client::Client;

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
