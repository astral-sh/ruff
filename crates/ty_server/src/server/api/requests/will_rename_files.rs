//! LSP exposure for best-effort Python file and regular-package renames.
//!
//! Each supported entry is analyzed in the workspace that owns both paths. A renamed folder may
//! not contain another workspace. Candidates include indexed and open files plus every physical
//! Python source moved with a folder. Unsupported entries and failed workspace groups or LSP
//! documents are omitted without suppressing independent edits.
//! When that leaves known relevant work unchanged, the server sends one warning for the request.

use std::collections::{BTreeMap, HashMap};

use lsp_types::{
    FileRename, RenameFilesParams, TextEdit, Uri, WillRenameFilesRequest, WorkspaceEdit,
};
use percent_encoding::percent_decode_str;
use ruff_db::Db as _;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ty_ide::{PathRename, will_rename_paths};
use ty_project::{Db as _, ProjectDatabase};

use crate::document::FileRangeExt;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;

/// Handles `workspace/willRenameFiles` for supported Python modules and packages.
pub(crate) struct WillRenameFilesHandler;

impl RequestHandler for WillRenameFilesHandler {
    type RequestType = WillRenameFilesRequest;
}

impl BackgroundRequestHandler for WillRenameFilesHandler {
    fn run(
        snapshot: &SessionSnapshot,
        client: &Client,
        params: RenameFilesParams,
    ) -> crate::server::Result<Option<WorkspaceEdit>> {
        let result = workspace_edit(snapshot, params);
        if result.known_omissions {
            client.show_warning_message(INCOMPLETE_RENAME_WARNING);
        }
        Ok(result.edit)
    }
}

impl RetriableRequestHandler for WillRenameFilesHandler {
    const RETRY_ON_CANCELLATION: bool = true;
}

const INCOMPLETE_RENAME_WARNING: &str = "ty could not safely update every affected Python import or module reference. Some references may remain unchanged after this file operation.";

fn workspace_edit(snapshot: &SessionSnapshot, params: RenameFilesParams) -> WorkspaceEditResult {
    let mut known_omissions = false;
    let Some(system) = snapshot.projects().first().map(ProjectDatabase::system) else {
        if params
            .files
            .iter()
            .any(|rename| !uri_is_non_python_file(&rename.old_uri))
        {
            omit(&mut known_omissions, "no project is available");
        }
        return WorkspaceEditResult {
            edit: None,
            known_omissions,
        };
    };
    let mut groups: BTreeMap<usize, Vec<PendingRename>> = BTreeMap::new();

    for rename in params.files {
        match prepare_rename(snapshot, system, rename) {
            Ok(Some(prepared)) => groups
                .entry(prepared.project)
                .or_default()
                .push(prepared.pending),
            Ok(None) => {}
            Err(omission) => omission.record(&mut known_omissions),
        }
    }

    let mut changes = HashMap::new();
    for (owner, group) in groups {
        if snapshot.language_services_disabled(owner) {
            note_omission_path(
                "language services are disabled for the workspace",
                snapshot.workspace_root(owner),
            );
            continue;
        }
        let db = &snapshot.projects()[owner];
        let mut moved_files = Vec::new();
        let mut renames = Vec::new();
        for pending in group {
            let mut files = Vec::with_capacity(pending.moved.len());
            let mut valid = true;
            for path in pending.moved {
                if snapshot.enclosing_project_index(&path) != Some(owner) {
                    omit_path(
                        &mut known_omissions,
                        "a moved source belongs to another workspace",
                        &path,
                    );
                    valid = false;
                    break;
                }
                let Ok(file) = system_path_to_file(db, &path) else {
                    omit_path(
                        &mut known_omissions,
                        "a moved source cannot be registered",
                        &path,
                    );
                    valid = false;
                    break;
                };
                files.push(file);
            }
            if valid {
                moved_files.extend(files);
                renames.push(pending.rename);
            }
        }
        if renames.is_empty() {
            continue;
        }
        let project = db.project();
        let result = will_rename_paths(
            db,
            &renames,
            project
                .files(db)
                .into_iter()
                .chain(project.open_files(db).iter().copied())
                .chain(moved_files),
            |file| {
                file.path(db)
                    .as_system_path()
                    .is_none_or(|path| snapshot.enclosing_project_index(path) == Some(owner))
            },
        );
        known_omissions |= result.has_known_omissions();
        project_lsp_edits(
            db,
            snapshot.position_encoding(),
            result.into_edits(),
            &mut changes,
            &mut known_omissions,
        );
    }
    normalize_lsp_edits(&mut changes, &mut known_omissions);
    WorkspaceEditResult {
        edit: (!changes.is_empty()).then(|| WorkspaceEdit::new(Some(changes), None, None)),
        known_omissions,
    }
}

struct WorkspaceEditResult {
    edit: Option<WorkspaceEdit>,
    known_omissions: bool,
}

fn prepare_rename(
    snapshot: &SessionSnapshot,
    system: &dyn System,
    rename: FileRename,
) -> Result<Option<PreparedRename>, RenameOmission> {
    let old_path = match file_uri_to_path(&rename.old_uri) {
        Some(path) => path,
        None if uri_is_non_python_file(&rename.old_uri) => return Ok(None),
        None => return Err(RenameOmission::NonLocalUri(rename.old_uri)),
    };

    let directory = system.is_directory(&old_path);
    if !directory && !matches!(old_path.extension(), Some("py" | "pyi")) {
        return Ok(None);
    }

    let new_path =
        file_uri_to_path(&rename.new_uri).ok_or(RenameOmission::NonLocalUri(rename.new_uri))?;
    let moved = if directory {
        let files = python_files_in_directory(system, &old_path)
            .ok_or_else(|| RenameOmission::UnreadableDirectory(old_path.clone()))?;
        if files.is_empty() {
            return Ok(None);
        }
        files
    } else {
        if new_path.extension() != old_path.extension() {
            return Err(RenameOmission::ChangedPythonExtension { old_path, new_path });
        }
        Vec::new()
    };

    let project = snapshot
        .enclosing_project_index(&old_path)
        .ok_or_else(|| RenameOmission::OutsideWorkspace(old_path.clone()))?;
    if snapshot.enclosing_project_index(&new_path) != Some(project) {
        return Err(RenameOmission::CrossesWorkspaceOwnership { old_path, new_path });
    }
    if !moved.is_empty() && snapshot.contains_other_workspace(project, &old_path) {
        return Err(RenameOmission::ContainsAnotherWorkspace(old_path));
    }

    let rename = if directory {
        PathRename::directory(old_path, new_path)
    } else {
        PathRename::file(old_path, new_path)
    };

    Ok(Some(PreparedRename {
        project,
        pending: PendingRename { rename, moved },
    }))
}

struct PreparedRename {
    project: usize,
    pending: PendingRename,
}

enum RenameOmission {
    NonLocalUri(String),
    UnreadableDirectory(SystemPathBuf),
    ChangedPythonExtension {
        old_path: SystemPathBuf,
        new_path: SystemPathBuf,
    },
    OutsideWorkspace(SystemPathBuf),
    CrossesWorkspaceOwnership {
        old_path: SystemPathBuf,
        new_path: SystemPathBuf,
    },
    ContainsAnotherWorkspace(SystemPathBuf),
}

impl RenameOmission {
    fn record(self, known_omissions: &mut bool) {
        match self {
            Self::NonLocalUri(uri) => omit_uri(
                known_omissions,
                "a rename URI is not a local file path",
                uri,
            ),
            Self::UnreadableDirectory(path) => omit_path(
                known_omissions,
                "a renamed directory cannot be read completely",
                path,
            ),
            Self::ChangedPythonExtension { old_path, new_path } => omit_rename(
                known_omissions,
                "a Python file rename changes its extension",
                old_path,
                new_path,
            ),
            Self::OutsideWorkspace(path) => omit_path(
                known_omissions,
                "a renamed source is outside every workspace",
                path,
            ),
            Self::CrossesWorkspaceOwnership { old_path, new_path } => omit_rename(
                known_omissions,
                "a rename crosses workspace ownership",
                old_path,
                new_path,
            ),
            Self::ContainsAnotherWorkspace(path) => omit_path(
                known_omissions,
                "a renamed directory contains another workspace",
                path,
            ),
        }
    }
}

struct PendingRename {
    rename: PathRename,
    moved: Vec<SystemPathBuf>,
}

fn project_lsp_edits(
    db: &ProjectDatabase,
    encoding: crate::PositionEncoding,
    edits: Vec<ty_ide::FileRenameEdit>,
    changes: &mut HashMap<Uri, Vec<TextEdit>>,
    known_omissions: &mut bool,
) {
    let mut by_file: HashMap<File, Vec<_>> = HashMap::new();
    for edit in edits {
        by_file.entry(edit.range.file()).or_default().push(edit);
    }
    for edits in by_file.into_values() {
        let mut projected = Vec::with_capacity(edits.len());
        let mut valid = true;
        for edit in edits {
            let file = edit.range.file();
            let Some(range) = edit.range.to_lsp_range(db, encoding) else {
                omit_path(
                    known_omissions,
                    "an edit cannot be converted to an LSP location",
                    file.path(db),
                );
                valid = false;
                break;
            };
            let Some(location) = range.into_location() else {
                omit_path(
                    known_omissions,
                    "an edit cannot be converted to an LSP location",
                    file.path(db),
                );
                valid = false;
                break;
            };
            projected.push((location.uri, TextEdit::new(location.range, edit.value)));
        }
        if valid {
            for (uri, edit) in projected {
                changes.entry(uri).or_default().push(edit);
            }
        }
    }
}

fn normalize_lsp_edits(changes: &mut HashMap<Uri, Vec<TextEdit>>, known_omissions: &mut bool) {
    changes.retain(|uri, edits| {
        edits.sort_unstable_by_key(|edit| edit.range);
        edits.dedup();
        if edits
            .windows(2)
            .any(|pair| pair[1].range.start < pair[0].range.end)
        {
            omit_uri(known_omissions, "projected edits overlap", uri);
            false
        } else {
            true
        }
    });
}

fn python_files_in_directory(system: &dyn System, root: &SystemPath) -> Option<Vec<SystemPathBuf>> {
    let mut files = Vec::new();
    for entry in system.read_directory(root).ok()? {
        let entry = entry.ok()?;
        let file_type = entry.file_type();
        let path = entry.into_path();
        if file_type.is_file() && matches!(path.extension(), Some("py" | "pyi")) {
            files.push(path);
        } else if file_type.is_directory() {
            files.extend(python_files_in_directory(system, &path)?);
        }
    }
    Some(files)
}

fn file_uri_to_path(uri: &str) -> Option<SystemPathBuf> {
    let uri = Uri::parse(uri).ok()?;
    SystemPathBuf::from_path_buf(uri.to_file_path().ok()?).ok()
}

fn uri_is_non_python_file(uri: &str) -> bool {
    Uri::parse(uri)
        .ok()
        .and_then(|uri| {
            let path: Vec<_> = percent_decode_str(uri.path()).collect();
            let name = path.rsplit(|byte| *byte == b'/').next()?;
            let dot = name.iter().rposition(|byte| *byte == b'.')?;
            let extension = &name[dot + 1..];
            Some(extension != b"py" && extension != b"pyi")
        })
        .unwrap_or(false)
}

fn omit(known_omissions: &mut bool, reason: &'static str) {
    note_omission(reason);
    *known_omissions = true;
}

fn omit_path(known_omissions: &mut bool, reason: &'static str, path: impl std::fmt::Display) {
    note_omission_path(reason, path);
    *known_omissions = true;
}

fn omit_uri(known_omissions: &mut bool, reason: &'static str, uri: impl std::fmt::Display) {
    tracing::debug!(
        reason,
        uri = %uri,
        "Omitting part of `workspace/willRenameFiles`"
    );
    *known_omissions = true;
}

fn omit_rename(
    known_omissions: &mut bool,
    reason: &'static str,
    old_path: impl std::fmt::Display,
    new_path: impl std::fmt::Display,
) {
    tracing::debug!(
        reason,
        old_path = %old_path,
        new_path = %new_path,
        "Omitting part of `workspace/willRenameFiles`"
    );
    *known_omissions = true;
}

fn note_omission(reason: &'static str) {
    tracing::debug!(reason, "Omitting part of `workspace/willRenameFiles`");
}

fn note_omission_path(reason: &'static str, path: impl std::fmt::Display) {
    tracing::debug!(
        reason,
        path = %path,
        "Omitting part of `workspace/willRenameFiles`"
    );
}
