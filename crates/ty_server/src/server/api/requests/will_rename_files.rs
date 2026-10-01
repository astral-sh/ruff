use std::collections::HashMap;

use lsp_types::{
    FileRename as LspFileRename, RenameFilesParams, TextEdit, Uri, WillRenameFilesRequest,
    WorkspaceEdit,
};
use ruff_db::files::system_path_to_file;
use ruff_db::system::SystemPathBuf;
use ty_ide::{FileRename, will_rename_files};
use ty_project::{Db as _, ProjectDatabase};

use crate::document::FileRangeExt;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;

/// Handles `workspace/willRenameFiles` for Python module files.
pub(crate) struct WillRenameFilesHandler;

impl RequestHandler for WillRenameFilesHandler {
    type RequestType = WillRenameFilesRequest;
}

impl BackgroundRequestHandler for WillRenameFilesHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: RenameFilesParams,
    ) -> crate::server::Result<Option<WorkspaceEdit>> {
        Ok(workspace_edit(snapshot, &params))
    }
}

impl RetriableRequestHandler for WillRenameFilesHandler {
    const RETRY_ON_CANCELLATION: bool = true;
}

fn workspace_edit(snapshot: &SessionSnapshot, params: &RenameFilesParams) -> Option<WorkspaceEdit> {
    let mut changes = HashMap::new();

    for db in snapshot.projects() {
        let renames: Vec<_> = params
            .files
            .iter()
            .filter_map(|rename| prepare_rename(db, rename))
            .collect();
        if renames.is_empty() {
            continue;
        }

        let project = db.project();
        let result = will_rename_files(
            db,
            &renames,
            project
                .files(db)
                .into_iter()
                // Open documents, such as virtual notebooks, may be absent from the project index.
                .chain(project.open_files(db).iter().copied()),
        );
        project_lsp_edits(db, snapshot.position_encoding(), result, &mut changes);
    }

    normalize_lsp_edits(&mut changes);

    (!changes.is_empty()).then(|| WorkspaceEdit::new(Some(changes), None, None))
}

/// Converts an LSP rename request into our internal representation of the same.
///
/// Returns `None` if the request is not supported:
///
/// - Either the source or destination URI is not a `file:` URI or cannot be
///   converted to a UTF-8 filesystem path on this platform
/// - The source is not an open document and its path is missing, inaccessible,
///   or represents a directory rather than a file
fn prepare_rename(db: &ProjectDatabase, rename: &LspFileRename) -> Option<FileRename> {
    let old_path = file_uri_to_path(&rename.old_uri)?;
    let new_path = file_uri_to_path(&rename.new_uri)?;
    let file = system_path_to_file(db, &old_path).ok()?;
    Some(FileRename { file, new_path })
}

/// Maps byte ranges in the computed edits to language server client positions.
///
/// Edits that cannot be mapped successfully are skipped.
fn project_lsp_edits(
    db: &ProjectDatabase,
    encoding: crate::PositionEncoding,
    edits: Vec<ty_ide::FileRenameEdit>,
    changes: &mut HashMap<Uri, Vec<TextEdit>>,
) {
    for edit in edits {
        if let Some(range) = edit.range.to_lsp_range(db, encoding)
            && let Some(location) = range.into_location()
        {
            changes
                .entry(location.uri)
                .or_default()
                .push(TextEdit::new(location.range, edit.value));
        }
    }
}

/// Per the LSP, removes overlapping edits from the given collection.
fn normalize_lsp_edits(changes: &mut HashMap<Uri, Vec<TextEdit>>) {
    changes.retain(|_, edits| {
        edits.sort_unstable_by(|left, right| {
            left.range
                .cmp(&right.range)
                .then_with(|| left.new_text.cmp(&right.new_text))
        });
        edits.dedup();

        let mut normalized = Vec::with_capacity(edits.len());
        let mut pending = std::mem::take(edits).into_iter().peekable();
        while let Some(edit) = pending.next() {
            let mut end = edit.range.end;
            let mut conflicting = false;

            // Follow the entire overlap chain. Keeping the last edit in such a chain would
            // choose an arbitrary replacement over the conflicting edits that precede it.
            while let Some(next) = pending.peek()
                && next.range.start < end
            {
                let Some(next) = pending.next() else {
                    break;
                };
                end = end.max(next.range.end);
                conflicting = true;
            }

            if !conflicting {
                normalized.push(edit);
            }
        }

        *edits = normalized;
        !edits.is_empty()
    });
}

fn file_uri_to_path(uri: &Uri) -> Option<SystemPathBuf> {
    if uri.scheme() == "file" {
        SystemPathBuf::from_path_buf(uri.to_file_path().ok()?).ok()
    } else {
        None
    }
}
