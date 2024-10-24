use std::borrow::Cow;

use crate::server::api::traits::{BackgroundDocumentRequestHandler, RequestHandler};
use lsp_types::{request::GotoDefinition, GotoDefinitionParams, GotoDefinitionResponse, Url};
// XXX the one place where I'm using something from red_knot_python_semantic
// maybe need to just move the type?
use red_knot_workspace::db::RootDatabase;
use ruff_db::files::{location::Location, File};
use ruff_source_file::SourceLocation;

pub(crate) struct GotoDefinitionHandler;

impl RequestHandler for GotoDefinitionHandler {
    type RequestType = GotoDefinition;
}

fn source_location_to_lsp_position(source_location: SourceLocation) -> lsp_types::Position {
    return lsp_types::Position::new(
        // XXX very wrong probably
        source_location.row.to_zero_indexed() as u32,
        source_location.column.to_zero_indexed() as u32,
    );
}
fn location_to_lsp_range(db: &RootDatabase, location: Location) -> lsp_types::Range {
    let (start, end) = db.location_to_source_location_range(location);
    lsp_types::Range {
        start: source_location_to_lsp_position(start),
        end: source_location_to_lsp_position(end),
    }
}

/// Try to generate a Url for an underlying file.
///
/// Currently only works for System paths
fn try_file_to_url(db: &RootDatabase, file: File) -> Option<Url> {
    // the following will return None for file paths that aren't System
    let path_buf = &file.path(db).clone().into_system_path_buf()?;
    // XXX is there a better trick to avoid building this String?
    let file_protocol_path: String = format!("file://{}", path_buf);
    Some(Url::parse(&file_protocol_path).expect("Failed to parse a system path URL"))
}
fn try_location_to_lsp_location(
    db: &RootDatabase,
    location: Location,
) -> Option<lsp_types::Location> {
    // TODO this code currently doesn't handle virtual path'd files or vendored files
    let uri = try_file_to_url(db, location.file)?;
    Some(lsp_types::Location {
        uri,
        range: location_to_lsp_range(db, location),
    })
}

impl BackgroundDocumentRequestHandler for GotoDefinitionHandler {
    fn document_url(params: &GotoDefinitionParams) -> std::borrow::Cow<lsp_types::Url> {
        Cow::Borrowed(&params.text_document_position_params.text_document.uri)
    }

    fn run_with_snapshot(
        snapshot: crate::DocumentSnapshot,
        db: RootDatabase,
        _notifier: crate::server::client::Notifier,
        params: GotoDefinitionParams,
    ) -> crate::server::api::Result<Option<GotoDefinitionResponse>> {
        let Some(file) = snapshot.file(&db) else {
            // XXX not sure if this should be considered an error or not...
            return Ok(None);
        };

        let Some(definition) =
            db.definition_at_location(file, params.text_document_position_params.position)
        else {
            return Ok(None);
        };

        let Some(lsp_location) = try_location_to_lsp_location(&db, location) else {
            // TODO this branch currently will be hit for things like vendored files
            // but in some future we should always have an answer for "location -> lsp location",
            // and should error here
            return Ok(None);
        };

        Ok(Some(lsp_location.into()))
    }
}
