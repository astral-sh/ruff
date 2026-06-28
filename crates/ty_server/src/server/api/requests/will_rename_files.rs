use std::collections::HashMap;

use lsp_types::{RenameFilesParams, TextEdit, Uri, WillRenameFilesRequest, WorkspaceEdit};
use ruff_db::system::{SystemPath, SystemPathBuf};
use ty_ide::will_rename;

use crate::document::ToRangeExt;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;
use crate::system::file_to_uri;

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
        let encoding = snapshot.position_encoding();

        // Parse every rename in the request once. `will_rename` itself decides
        // whether each path is a file or a directory (package).
        let renames: Vec<(SystemPathBuf, SystemPathBuf)> = params
            .files
            .iter()
            .filter_map(|file_rename| {
                let old = uri_to_system_path(&file_rename.old_uri)?;
                let new = uri_to_system_path(&file_rename.new_uri)?;
                Some((old, new))
            })
            .collect();
        if renames.is_empty() {
            return Ok(None);
        }

        let rename_refs: Vec<(&SystemPath, &SystemPath)> = renames
            .iter()
            .map(|(old, new)| (old.as_path(), new.as_path()))
            .collect();

        // Compute all edits per project in a single pass over each project's files.
        let mut all_changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();
        for db in snapshot.projects() {
            for edit in will_rename(db, &rename_refs) {
                let Some(uri) = file_to_uri(db, edit.file) else {
                    continue;
                };
                let Some(lsp_range) = edit.range.to_lsp_range(db, edit.file, encoding) else {
                    continue;
                };

                all_changes.entry(uri).or_default().push(TextEdit {
                    range: lsp_range.local_range(),
                    new_text: edit.new_text,
                });
            }
        }

        if all_changes.is_empty() {
            return Ok(None);
        }

        // LSP clients reject workspace edits with overlapping ranges in a single
        // document. Deduplicate identical edits and bail out entirely if any
        // genuine overlap remains.
        if !deduplicate_and_check_overlaps(&mut all_changes) {
            return Ok(None);
        }

        Ok(Some(WorkspaceEdit {
            changes: Some(all_changes),
            ..Default::default()
        }))
    }
}

impl RetriableRequestHandler for WillRenameFilesHandler {}

/// Convert a file URI string into a [`SystemPathBuf`], returning `None` for any
/// URI that is not a parseable local file path.
fn uri_to_system_path(raw: &str) -> Option<SystemPathBuf> {
    let uri = Uri::parse(raw).ok()?;
    let path = uri.to_file_path().ok()?;
    SystemPathBuf::from_path_buf(path).ok()
}

/// Sort and deduplicate the edits within each document, then verify that no two
/// remaining edits overlap. Returns `false` if any overlap is found, in which
/// case the entire workspace edit should be discarded.
fn deduplicate_and_check_overlaps(changes: &mut HashMap<Uri, Vec<TextEdit>>) -> bool {
    // Each document's edits are processed independently, so the hash-map
    // iteration order does not affect the result.
    #[expect(clippy::iter_over_hash_type)]
    for edits in changes.values_mut() {
        edits.sort_by_key(range_key);
        edits.dedup();

        for pair in edits.windows(2) {
            let previous_end = (pair[0].range.end.line, pair[0].range.end.character);
            let next_start = (pair[1].range.start.line, pair[1].range.start.character);
            if previous_end > next_start {
                return false;
            }
        }
    }
    true
}

fn range_key(edit: &TextEdit) -> (u32, u32, u32, u32) {
    (
        edit.range.start.line,
        edit.range.start.character,
        edit.range.end.line,
        edit.range.end.character,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use lsp_types::{Position, Range, TextEdit, Uri};

    use super::deduplicate_and_check_overlaps;

    fn edit(start: u32, end: u32, new_text: &str) -> TextEdit {
        TextEdit {
            range: Range {
                start: Position::new(0, start),
                end: Position::new(0, end),
            },
            new_text: new_text.to_string(),
        }
    }

    fn changes(edits: Vec<TextEdit>) -> HashMap<Uri, Vec<TextEdit>> {
        let mut map = HashMap::new();
        map.insert(Uri::from_str("file:///a.py").unwrap(), edits);
        map
    }

    #[test]
    fn identical_edits_are_deduplicated() {
        let mut map = changes(vec![edit(0, 3, "new"), edit(0, 3, "new")]);
        assert!(deduplicate_and_check_overlaps(&mut map));
        assert_eq!(map.values().next().unwrap().len(), 1);
    }

    #[test]
    fn non_overlapping_edits_are_kept() {
        let mut map = changes(vec![edit(4, 7, "b"), edit(0, 3, "a")]);
        assert!(deduplicate_and_check_overlaps(&mut map));
        assert_eq!(map.values().next().unwrap().len(), 2);
    }

    #[test]
    fn overlapping_edits_are_rejected() {
        let mut map = changes(vec![edit(0, 5, "a"), edit(3, 8, "b")]);
        assert!(!deduplicate_and_check_overlaps(&mut map));
    }

    #[test]
    fn conflicting_edits_on_same_range_are_rejected() {
        let mut map = changes(vec![edit(0, 3, "a"), edit(0, 3, "b")]);
        assert!(!deduplicate_and_check_overlaps(&mut map));
    }
}
