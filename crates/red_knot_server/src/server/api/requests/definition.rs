use std::borrow::Cow;

use crate::{
    server::{
        api::traits::{BackgroundDocumentRequestHandler, RequestHandler},
        client::Notifier,
    },
    DocumentSnapshot,
};
use lsp_types::{request::GotoDefinition, GotoDefinitionParams, GotoDefinitionResponse, Url};
use red_knot_workspace::db::RootDatabase;
use ruff_db::files::{location::Location, File};
use ruff_source_file::{OneIndexed, SourceLocation};

pub(crate) struct GotoDefinitionHandler;

impl RequestHandler for GotoDefinitionHandler {
    type RequestType = GotoDefinition;
}

fn try_source_location_to_lsp_position(
    source_location: &SourceLocation,
) -> Option<lsp_types::Position> {
    let (Ok(u32_row), Ok(u32_col)) = (
        u32::try_from(source_location.row.to_zero_indexed()),
        u32::try_from(source_location.column.to_zero_indexed()),
    ) else {
        // TODO decide how to handle this failure
        return None;
    };

    Some(lsp_types::Position::new(u32_row, u32_col))
}
fn try_location_to_lsp_range(db: &RootDatabase, location: &Location) -> Option<lsp_types::Range> {
    let (start, end) = db.location_to_source_location_range(location);
    Some(lsp_types::Range {
        start: try_source_location_to_lsp_position(&start)?,
        end: try_source_location_to_lsp_position(&end)?,
    })
}

/// Try to generate a Url for an underlying file.
///
/// Currently only works for System paths
fn try_file_to_url(db: &RootDatabase, file: File) -> Option<Url> {
    // the following will return None for file paths that aren't System
    let path_buf = &file.path(db).clone().into_system_path_buf()?;
    // XXX is there a better trick to avoid building this String?
    let file_protocol_path: String = format!("file://{path_buf}");
    Some(Url::parse(&file_protocol_path).expect("Failed to parse a system path URL"))
}
fn try_location_to_lsp_location(
    db: &RootDatabase,
    location: &Location,
) -> Option<lsp_types::Location> {
    // TODO this code currently doesn't handle virtual path'd files or vendored files
    let uri = try_file_to_url(db, location.file)?;
    Some(lsp_types::Location {
        uri,
        range: try_location_to_lsp_range(db, location)?,
    })
}

fn lsp_position_to_source_location(position: lsp_types::Position) -> SourceLocation {
    // While it would be nice to just implement From here, that would require
    // ruff_source_file to know about lsp-types
    SourceLocation {
        row: OneIndexed::from_zero_indexed(position.line as usize),
        column: OneIndexed::from_zero_indexed(position.character as usize),
    }
}

impl BackgroundDocumentRequestHandler for GotoDefinitionHandler {
    fn document_url(params: &GotoDefinitionParams) -> Cow<lsp_types::Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        db: RootDatabase,
        _notifier: Notifier,
        params: GotoDefinitionParams,
    ) -> crate::server::api::Result<Option<GotoDefinitionResponse>> {
        let Some(file) = snapshot.file(&db) else {
            // XXX not sure if this should be considered an error or not...
            return Ok(None);
        };

        let Some(location) = db.location_of_definition_of_item_at_location(
            file,
            &lsp_position_to_source_location(params.text_document_position_params.position),
        ) else {
            return Ok(None);
        };

        let Some(lsp_location) = try_location_to_lsp_location(&db, &location) else {
            // TODO this branch currently will be hit for things like vendored files
            // but in some future we should always have an answer for "location -> lsp location",
            // and should error here
            return Ok(None);
        };

        Ok(Some(lsp_location.into()))
    }
}
