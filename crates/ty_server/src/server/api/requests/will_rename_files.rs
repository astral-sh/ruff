use std::collections::HashMap;

use lsp_types::{RenameFilesParams, TextEdit, Uri, WillRenameFilesRequest, WorkspaceEdit};
use ruff_db::system::SystemPathBuf;
use ruff_db::{Db as _, files::File};
use ruff_text_size::TextRange;
use ty_ide::{PathRename, will_rename_paths};
use ty_project::{Db as _, ProjectDatabase};

use crate::document::ToRangeExt;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;

/// Handles `workspace/willRenameFiles` requests for Python modules and directories.
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
        let mut renames_by_project = vec![Vec::new(); snapshot.projects().len()];

        for rename in params.files {
            let (Some(old_path), Some(new_path)) = (
                file_uri_to_path(&rename.old_uri),
                file_uri_to_path(&rename.new_uri),
            ) else {
                continue;
            };
            let Some(project_index) = snapshot.project_index_for_path(&old_path) else {
                continue;
            };
            if snapshot
                .enclosing_project_index_for_path(&new_path)
                .is_some_and(|destination| destination != project_index)
            {
                continue;
            }
            let db = &snapshot.projects()[project_index];

            let path_rename = if db.system().is_directory(&old_path) {
                PathRename::directory(old_path, new_path)
            } else {
                let Some(extension @ ("py" | "pyi")) = old_path.extension() else {
                    continue;
                };
                if new_path.extension() != Some(extension) {
                    continue;
                }
                PathRename::file(old_path, new_path)
            };
            renames_by_project[project_index].push(path_rename);
        }

        let encoding = snapshot.position_encoding();
        let mut all_changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();

        for (project_index, renames) in renames_by_project.into_iter().enumerate() {
            if renames.is_empty() {
                continue;
            }
            let db = &snapshot.projects()[project_index];
            if snapshot.language_services_disabled(project_index) {
                continue;
            }

            let project = db.project();
            let indexed_files = project.files(db);
            let open_files = project.open_files(db);
            let edits = will_rename_paths(
                db,
                &renames,
                (&indexed_files)
                    .into_iter()
                    .chain(open_files.iter().copied()),
                |file| file_is_in_rename_scope(snapshot, db, project_index, file),
            );
            append_lsp_edits(db, encoding, edits, &mut all_changes);
        }

        all_changes.retain(|_, edits| normalize_edits(edits));
        if all_changes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(WorkspaceEdit {
                changes: Some(all_changes),
                ..Default::default()
            }))
        }
    }
}

impl RetriableRequestHandler for WillRenameFilesHandler {
    const RETRY_ON_CANCELLATION: bool = true;
}

fn append_lsp_edits(
    db: &ProjectDatabase,
    encoding: crate::PositionEncoding,
    edits: Vec<ty_ide::FileRenameEdit>,
    changes: &mut HashMap<Uri, Vec<TextEdit>>,
) {
    let mut edits_by_file: HashMap<File, Vec<(TextRange, String)>> = HashMap::new();
    for edit in edits {
        let (file, range, new_text) = edit.into_parts();
        edits_by_file
            .entry(file)
            .or_default()
            .push((range, new_text));
    }

    #[expect(
        clippy::iter_over_hash_type,
        reason = "Document edits are normalized before returning"
    )]
    for (file, edits) in edits_by_file {
        let Some(converted) = edits
            .into_iter()
            .map(|(range, new_text)| {
                let location = range.to_lsp_range(db, file, encoding)?.into_location()?;
                Some((
                    location.uri,
                    TextEdit {
                        range: location.range,
                        new_text,
                    },
                ))
            })
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        for (uri, edit) in converted {
            changes.entry(uri).or_default().push(edit);
        }
    }
}

fn normalize_edits(edits: &mut Vec<TextEdit>) -> bool {
    edits.sort_by(|left, right| {
        left.range
            .start
            .cmp(&right.range.start)
            .then_with(|| left.range.end.cmp(&right.range.end))
            .then_with(|| left.new_text.cmp(&right.new_text))
    });
    edits.dedup();
    !edits.windows(2).any(|edits| {
        edits[0].range.start == edits[1].range.start || edits[0].range.end > edits[1].range.start
    })
}

fn file_uri_to_path(uri: &str) -> Option<SystemPathBuf> {
    let uri = Uri::parse(uri).ok()?;
    SystemPathBuf::from_path_buf(uri.to_file_path().ok()?).ok()
}

fn file_is_in_rename_scope(
    snapshot: &SessionSnapshot,
    db: &ProjectDatabase,
    project_index: usize,
    file: File,
) -> bool {
    file.path(db).as_system_path().is_none_or(|path| {
        snapshot
            .enclosing_project_index_for_path(path)
            .is_none_or(|index| index == project_index)
    })
}
