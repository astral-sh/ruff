//! Returns import and reference edits before the client renames Python files.
//!
//! Each rename is analyzed in the workspace that owns both paths. Candidates include indexed and
//! open files plus the renamed Python files.
//!
//! Invalid request entries and edits that cannot be converted to LSP locations are omitted while
//! independent edits are retained. The server warns once per request for omissions it detects,
//! including overlapping edits. The IDE planner can also leave unsupported references unchanged;
//! it does not report those omissions to this handler.

use std::collections::{BTreeMap, HashMap};

use lsp_types::{
    FileRename as LspFileRename, RenameFilesParams, TextEdit, Uri, WillRenameFilesRequest,
    WorkspaceEdit,
};
use percent_encoding::percent_decode_str;
use ruff_db::files::{File, FileError, system_path_to_file};
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

fn workspace_edit(snapshot: &SessionSnapshot, params: RenameFilesParams) -> WorkspaceEditResult {
    let mut omissions = Omissions::default();
    let mut groups: BTreeMap<usize, Vec<FileRename>> = BTreeMap::new();

    // Keep each rename within one owning workspace so its paths and references use the same
    // database. An invalid entry does not prevent independent entries from being processed.
    for rename in params.files {
        match prepare_rename(snapshot, rename) {
            Ok(Some(prepared)) => groups
                .entry(prepared.project)
                .or_default()
                .push(prepared.rename),
            Ok(None) => {}
            Err(omission) => omission.record(&mut omissions),
        }
    }

    let mut changes = HashMap::new();
    for (owner, renames) in groups {
        let db = &snapshot.projects()[owner];
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
        // Moved files can be excluded from indexing, and open files can contain unsaved text.
        // Include both alongside indexed files so the planner sees all three sources.
        let result = will_rename_files(
            db,
            &renames,
            project
                .files(db)
                .into_iter()
                .chain(project.open_files(db).iter().copied())
                .chain(renames.iter().map(|rename| rename.file)),
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

/// Validates one Python file rename and identifies its owning workspace.
///
/// Returns `Ok(None)` for unsupported entries. An error records
/// an omission that should warn the client; semantic support is decided later by the IDE planner.
fn prepare_rename(
    snapshot: &SessionSnapshot,
    rename: LspFileRename,
) -> Result<Option<PreparedRename>, RenameOmission> {
    if !uri_is_python_file(&rename.old_uri) {
        return Ok(None);
    }
    let old_path =
        file_uri_to_path(&rename.old_uri).ok_or(RenameOmission::NonLocalUri(rename.old_uri))?;

    let project = snapshot
        .enclosing_project_index(&old_path)
        .ok_or_else(|| RenameOmission::OutsideWorkspace(old_path.clone()))?;
    let file = match system_path_to_file(&snapshot.projects()[project], &old_path) {
        Ok(file) => file,
        Err(FileError::IsADirectory) => return Ok(None),
        Err(_) => return Err(RenameOmission::UnreadableFile(old_path)),
    };
    let new_path =
        file_uri_to_path(&rename.new_uri).ok_or(RenameOmission::NonLocalUri(rename.new_uri))?;
    if snapshot.enclosing_project_index(&new_path) != Some(project) {
        return Err(RenameOmission::CrossesWorkspaceOwnership { old_path, new_path });
    }
    if new_path.extension() != old_path.extension() {
        return Err(RenameOmission::ChangedPythonExtension { old_path, new_path });
    }
    Ok(Some(PreparedRename {
        project,
        rename: FileRename { file, new_path },
    }))
}

struct PreparedRename {
    project: usize,
    rename: FileRename,
}

enum RenameOmission {
    NonLocalUri(Uri),
    UnreadableFile(SystemPathBuf),
    ChangedPythonExtension {
        old_path: SystemPathBuf,
        new_path: SystemPathBuf,
    },
    OutsideWorkspace(SystemPathBuf),
    CrossesWorkspaceOwnership {
        old_path: SystemPathBuf,
        new_path: SystemPathBuf,
    },
}

impl RenameOmission {
    fn record(self, omissions: &mut Omissions) {
        match self {
            Self::NonLocalUri(uri) => {
                omissions.uri("a rename URI is not a local file path", uri);
            }
            Self::UnreadableFile(path) => {
                omissions.path("a moved source cannot be registered", path);
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
        }
    }
}

/// Converts byte ranges to client positions, retaining a file's edits only if all can be converted.
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

/// Sorts and deduplicates edits, then removes overlapping groups while retaining independent edits.
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

fn file_uri_to_path(uri: &Uri) -> Option<SystemPathBuf> {
    SystemPathBuf::from_path_buf(uri.to_file_path().ok()?).ok()
}

/// Recognizes a Python extension even when the URI is not a valid local file path.
fn uri_is_python_file(uri: &Uri) -> bool {
    let path: Vec<_> = percent_decode_str(uri.path()).collect();
    let Some(name) = path.rsplit(|byte| *byte == b'/').next() else {
        return false;
    };
    let Some(dot) = name.iter().rposition(|byte| *byte == b'.') else {
        return false;
    };
    let extension = &name[dot + 1..];
    extension == b"py" || extension == b"pyi"
}

#[derive(Default)]
struct Omissions {
    any: bool,
}

impl Omissions {
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
