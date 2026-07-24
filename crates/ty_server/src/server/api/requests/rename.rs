use std::borrow::Cow;
use std::collections::HashMap;

use lsp_types::RenameRequest;
use lsp_types::{RenameParams, TextEdit, Uri, WorkspaceEdit};
use ty_ide::rename;
use ty_project::{ProjectDatabase, SemanticDb as _};

use crate::document::{PositionExt, ToLink};
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

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let Some(offset) = params.text_document_position_params.position.to_text_size(
            db,
            file,
            snapshot.uri(),
            snapshot.encoding(),
        ) else {
            return Ok(None);
        };

        let Some(rename_results) = rename(db, db.program_file(file), offset, &params.new_name)
        else {
            return Ok(None);
        };

        // Group text edits by file
        let mut changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();

        for reference in rename_results {
            if let Some(location) = reference.to_location(db, snapshot.encoding()) {
                let edit = TextEdit {
                    range: location.range,
                    new_text: params.new_name.clone(),
                };

                changes.entry(location.uri).or_default().push(edit);
            }
        }

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
