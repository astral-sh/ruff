use std::collections::HashMap;

use lsp_types::{
    FileRename as LspFileRename, RenameFilesParams, TextEdit, Uri, WillRenameFilesRequest,
    WorkspaceEdit,
};
use ruff_db::files::system_path_to_file;
use ruff_db::system::SystemPathBuf;
use ty_ide::{FileRename, will_rename_files};
use ty_project::ProjectDatabase;

use crate::document::extend_lsp_edits;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::client::Client;
use crate::session::{Session, SessionSnapshot};

/// Handles `workspace/willRenameFiles` for Python module files.
pub(crate) struct WillRenameFilesHandler;

impl RequestHandler for WillRenameFilesHandler {
    type RequestType = WillRenameFilesRequest;
}

impl BackgroundRequestHandler for WillRenameFilesHandler {
    fn prepare_params(session: &Session, params: &mut RenameFilesParams) {
        params.files.retain(|rename| {
            let Some(old_path) = file_uri_to_path(&rename.old_uri) else {
                return false;
            };

            session.allows_file_rename_edits(&old_path)
        });
    }

    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: RenameFilesParams,
    ) -> crate::server::Result<Option<WorkspaceEdit>> {
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

            let result = will_rename_files(db, &renames);
            extend_lsp_edits(db, snapshot.position_encoding(), result, &mut changes);
        }

        // Different projects can propose conflicting edits for the same file, so normalize only
        // after collecting every project's results.
        normalize_lsp_edits(&mut changes);

        Ok((!changes.is_empty()).then(|| WorkspaceEdit::new(Some(changes), None, None)))
    }
}

impl RetriableRequestHandler for WillRenameFilesHandler {
    const RETRY_ON_CANCELLATION: bool = true;
}

/// Converts an LSP rename request into our internal representation of the same.
fn prepare_rename(db: &ProjectDatabase, rename: &LspFileRename) -> Option<FileRename> {
    let old_path = file_uri_to_path(&rename.old_uri)?;
    let new_path = file_uri_to_path(&rename.new_uri)?;
    let file = system_path_to_file(db, &old_path).ok()?;
    Some(FileRename { file, new_path })
}

/// Deduplicates edits and drops files with overlapping edits.
fn normalize_lsp_edits(changes: &mut HashMap<Uri, Vec<TextEdit>>) {
    changes.retain(|_, edits| {
        edits.sort_unstable_by(|left, right| {
            left.range
                .cmp(&right.range)
                .then_with(|| left.new_text.cmp(&right.new_text))
        });
        edits.dedup();

        !edits.is_empty()
            && !edits
                .windows(2)
                .any(|pair| pair[1].range.start < pair[0].range.end)
    });
}

fn file_uri_to_path(uri: &Uri) -> Option<SystemPathBuf> {
    if uri.scheme() == "file" {
        SystemPathBuf::from_path_buf(uri.to_file_path().ok()?).ok()
    } else {
        None
    }
}
