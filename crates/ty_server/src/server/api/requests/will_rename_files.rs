//! LSP exposure for best-effort Python file and regular-package renames.
//!
//! Each supported entry is analyzed in the workspace that owns both paths. A renamed folder may
//! not contain another workspace. Candidates include indexed and open files plus every physical
//! Python source moved with a folder. Unsupported entries and failed workspace groups or LSP
//! documents are omitted without suppressing independent edits. The server sends one warning when
//! it omits a request entry or cannot project an edit.

use std::collections::{BTreeMap, HashMap};

use lsp_server::RequestId;
use lsp_types::{
    FileRename, RenameFilesParams, TextEdit, Uri, WillRenameFilesRequest, WorkspaceEdit,
};
use percent_encoding::percent_decode_str;
use ruff_db::Db as _;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ty_ide::{PathRename, will_rename_paths};
use ty_project::{Db as _, ProjectDatabase};
use ty_python_semantic::should_record_place_loads;

use crate::document::FileRangeExt;
use crate::server::Action;
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
    fn handle_request(
        id: &RequestId,
        snapshot: SessionSnapshot,
        client: &Client,
        params: RenameFilesParams,
    ) {
        let files = files_needing_recording(&snapshot, &params);
        if !files.is_empty() {
            let revision = snapshot.revision();
            // Drop our database snapshots before the main thread changes Salsa inputs.
            // Otherwise its cancellation could wait for this task while we queue the action.
            drop(snapshot);
            client.queue_action(Action::EnablePlaceLoadRecording {
                revision,
                files,
                request: lsp_server::Request::new(
                    id.clone(),
                    Self::METHOD.as_str().to_owned(),
                    params,
                ),
            });
            return;
        }

        client.respond(id, Self::run(&snapshot, client, params));
    }

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

const INCOMPLETE_RENAME_WARNING: &str = "ty could not safely update all affected Python code. Some imports, references, or exports may remain unchanged after this file operation.";

/// Discovers recording requirements without inferring types. After the main thread enables them,
/// the request is retried against a fresh snapshot; subsequent requests usually need no preparation.
fn files_needing_recording(
    snapshot: &SessionSnapshot,
    params: &RenameFilesParams,
) -> Vec<(usize, Vec<File>)> {
    let Some(system) = snapshot.projects().first().map(ProjectDatabase::system) else {
        return Vec::new();
    };
    let mut by_project: BTreeMap<usize, Vec<File>> = BTreeMap::new();
    for rename in &params.files {
        let Ok(Some(prepared)) = prepare_rename(snapshot, system, rename.clone()) else {
            continue;
        };
        let owner = prepared.project;
        if snapshot.language_services_disabled(owner) {
            continue;
        }
        let db = &snapshot.projects()[owner];
        by_project.entry(owner).or_default().extend(
            prepared
                .pending
                .moved
                .into_iter()
                .filter_map(|path| system_path_to_file(db, &path).ok()),
        );
    }

    by_project
        .into_iter()
        .filter_map(|(owner, moved)| {
            let db = &snapshot.projects()[owner];
            let project = db.project();
            let mut files: Vec<_> = project
                .files(db)
                .into_iter()
                .chain(project.open_files(db).iter().copied())
                .chain(moved)
                .filter(|file| {
                    file.path(db)
                        .as_system_path()
                        .is_none_or(|path| snapshot.enclosing_project_index(path) == Some(owner))
                })
                .filter(|file| !should_record_place_loads(db, *file))
                .collect();
            files.sort_unstable();
            files.dedup();
            (!files.is_empty()).then_some((owner, files))
        })
        .collect()
}

fn workspace_edit(snapshot: &SessionSnapshot, params: RenameFilesParams) -> WorkspaceEditResult {
    let mut omissions = Omissions::default();
    let Some(system) = snapshot.projects().first().map(ProjectDatabase::system) else {
        if params
            .files
            .iter()
            .any(|rename| !uri_is_non_python_file(&rename.old_uri))
        {
            omissions.record("no project is available");
        }
        return WorkspaceEditResult {
            edit: None,
            known_omissions: omissions.any,
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
            Err(omission) => omission.record(&mut omissions),
        }
    }

    let mut changes = HashMap::new();
    for (owner, group) in groups {
        let db = &snapshot.projects()[owner];
        let mut moved_files = Vec::new();
        let mut renames = Vec::new();
        for pending in group {
            let mut files = Vec::with_capacity(pending.moved.len());
            let mut valid = true;
            for path in pending.moved {
                if snapshot.enclosing_project_index(&path) != Some(owner) {
                    omissions.path("a moved source belongs to another workspace", &path);
                    valid = false;
                    break;
                }
                let Ok(file) = system_path_to_file(db, &path) else {
                    omissions.path("a moved source cannot be registered", &path);
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
        let in_scope = |file: File| {
            file.path(db)
                .as_system_path()
                .is_none_or(|path| snapshot.enclosing_project_index(path) == Some(owner))
        };
        if snapshot.language_services_disabled(owner) {
            omissions.path(
                "language services are disabled for the workspace",
                snapshot.workspace_root(owner),
            );
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
            in_scope,
        );
        project_lsp_edits(
            db,
            snapshot.position_encoding(),
            result,
            &mut changes,
            &mut omissions,
        );
    }
    normalize_lsp_edits(&mut changes, &mut omissions);
    WorkspaceEditResult {
        edit: (!changes.is_empty()).then(|| WorkspaceEdit::new(Some(changes), None, None)),
        known_omissions: omissions.any,
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

    let project = snapshot
        .enclosing_project_index(&old_path)
        .ok_or_else(|| RenameOmission::OutsideWorkspace(old_path.clone()))?;
    if snapshot.enclosing_project_index(&new_path) != Some(project) {
        return Err(RenameOmission::CrossesWorkspaceOwnership { old_path, new_path });
    }
    if directory && snapshot.contains_other_workspace(project, &old_path) {
        return Err(RenameOmission::ContainsAnotherWorkspace(old_path));
    }

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
        vec![old_path.clone()]
    };

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
    NonLocalUri(Uri),
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
    fn record(self, omissions: &mut Omissions) {
        match self {
            Self::NonLocalUri(uri) => {
                omissions.uri("a rename URI is not a local file path", uri);
            }
            Self::UnreadableDirectory(path) => {
                omissions.path("a renamed directory cannot be read completely", path);
            }
            Self::ChangedPythonExtension { old_path, new_path } => omissions.rename(
                "a Python file rename changes its extension",
                old_path,
                new_path,
            ),
            Self::OutsideWorkspace(path) => {
                omissions.path("a renamed source is outside every workspace", path);
            }
            Self::CrossesWorkspaceOwnership { old_path, new_path } => {
                omissions.rename("a rename crosses workspace ownership", old_path, new_path);
            }
            Self::ContainsAnotherWorkspace(path) => {
                omissions.path("a renamed directory contains another workspace", path);
            }
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
    omissions: &mut Omissions,
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
                omissions.path(
                    "an edit cannot be converted to an LSP location",
                    file.path(db),
                );
                valid = false;
                break;
            };
            let Some(location) = range.into_location() else {
                omissions.path(
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

fn normalize_lsp_edits(changes: &mut HashMap<Uri, Vec<TextEdit>>, omissions: &mut Omissions) {
    changes.retain(|uri, edits| {
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
            while let Some(next) = pending.peek()
                && next.range.start < end
            {
                let Some(next) = pending.next() else {
                    break;
                };
                end = end.max(next.range.end);
                conflicting = true;
            }
            if conflicting {
                omissions.uri("projected edits overlap", uri);
            } else {
                normalized.push(edit);
            }
        }
        *edits = normalized;
        !edits.is_empty()
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

fn file_uri_to_path(uri: &Uri) -> Option<SystemPathBuf> {
    SystemPathBuf::from_path_buf(uri.to_file_path().ok()?).ok()
}

fn uri_is_non_python_file(uri: &Uri) -> bool {
    let path: Vec<_> = percent_decode_str(uri.path()).collect();
    let Some(name) = path.rsplit(|byte| *byte == b'/').next() else {
        return false;
    };
    let Some(dot) = name.iter().rposition(|byte| *byte == b'.') else {
        return false;
    };
    let extension = &name[dot + 1..];
    extension != b"py" && extension != b"pyi"
}

#[derive(Default)]
struct Omissions {
    any: bool,
}

impl Omissions {
    fn record(&mut self, reason: &'static str) {
        tracing::debug!(reason, "Omitting part of `workspace/willRenameFiles`");
        self.any = true;
    }

    fn path(&mut self, reason: &'static str, path: impl std::fmt::Display) {
        tracing::debug!(
            reason,
            path = %path,
            "Omitting part of `workspace/willRenameFiles`"
        );
        self.any = true;
    }

    fn uri(&mut self, reason: &'static str, uri: impl std::fmt::Display) {
        tracing::debug!(
            reason,
            uri = %uri,
            "Omitting part of `workspace/willRenameFiles`"
        );
        self.any = true;
    }

    fn rename(
        &mut self,
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
        self.any = true;
    }
}
