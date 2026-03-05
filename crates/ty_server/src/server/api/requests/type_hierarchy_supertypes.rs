use lsp_types::request::TypeHierarchySupertypes;
use lsp_types::{TypeHierarchyItem, TypeHierarchySupertypesParams};

use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::server::api::type_hierarchy::hierarchy_handler;
use crate::session::SessionSnapshot;
use crate::session::client::Client;

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
