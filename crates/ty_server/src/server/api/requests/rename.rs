use std::borrow::Cow;
use std::collections::HashMap;

use lsp_types::RenameRequest;
use lsp_types::{RenameParams, Uri, WorkspaceEdit};
use ty_ide::{RangedValue, rename};
use ty_project::{ProjectDatabase, SemanticDb as _};

use crate::document::{PositionExt, extend_lsp_edits};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct RenameRequestHandler;

impl RequestHandler for RenameRequestHandler {
    type RequestType = RenameRequest;
}

impl BackgroundDocumentRequestHandler for RenameRequestHandler {
    fn document_uri(params: &RenameParams) -> Cow<'_, Uri> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: RenameParams,
    ) -> crate::server::Result<Option<WorkspaceEdit>> {
        if snapshot
            .workspace_settings()
            .is_language_services_disabled()
        {
            return Ok(None);
        }

        let Some(file) = snapshot.document().to_notebook_or_file(db) else {
            return Ok(None);
        };

        let Some(offset) = params.text_document_position_params.position.to_text_size(
            db,
            file,
            snapshot.document().uri(),
            snapshot.encoding(),
        ) else {
            return Ok(None);
        };

        let Some(rename_results) = rename(db, db.program_file(file), offset, &params.new_name)
        else {
            return Ok(None);
        };

        let edits = rename_results.into_iter().map(|reference| RangedValue {
            range: reference.file_range(),
            value: params.new_name.as_str(),
        });
        let mut changes = HashMap::new();
        extend_lsp_edits(db, snapshot.encoding(), edits, &mut changes);

        if changes.is_empty() {
            return Ok(None);
        }

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }
}

impl RetriableRequestHandler for RenameRequestHandler {}
