use std::{collections::HashMap, str::FromStr};

use crate::server::api::LSPResult;
use crate::server::client;
use crate::server::schedule::Task;
use crate::session::Session;
use crate::DIAGNOSTIC_NAME;
use crate::{edit::DocumentVersion, server};
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use serde::Deserialize;
use types::TextDocumentEdit;

#[derive(Debug)]
enum Command {
    Format,
    FixAll,
    OrganizeImports,
}

pub(crate) struct ExecuteCommand;

#[derive(Deserialize)]
struct TextDocumentArgument {
    uri: types::Url,
    version: DocumentVersion,
}

#[derive(Debug)]
enum EditTracker {
    DocumentChanges(Vec<types::TextDocumentEdit>),
    Changes(HashMap<types::Url, Vec<types::TextEdit>>),
}

impl super::RequestHandler for ExecuteCommand {
    type RequestType = req::ExecuteCommand;
}

impl super::SyncRequestHandler for ExecuteCommand {
    fn run(
        session: &mut Session,
        _notifier: client::Notifier,
        requester: &mut client::Requester,
        params: types::ExecuteCommandParams,
    ) -> server::Result<Option<serde_json::Value>> {
        let command =
            Command::from_str(&params.command).with_failure_code(ErrorCode::InvalidParams)?;

        // check if we can apply a workspace edit
        if !session.resolved_client_capabilities().apply_edit {
            return Err(anyhow::anyhow!("Cannot execute the '{}' command: the client does not support `workspace/applyEdit`", command.label())).with_failure_code(ErrorCode::InternalError);
        }

        let mut edit_tracker =
            EditTracker::new(session.resolved_client_capabilities().document_changes);
        for arg in params.arguments {
            let TextDocumentArgument { uri, version } =
                serde_json::from_value(arg).with_failure_code(ErrorCode::InvalidParams)?;
            let snapshot = session
                .take_snapshot(&uri)
                .ok_or(anyhow::anyhow!("Document snapshot not available for {uri}",))
                .with_failure_code(ErrorCode::InternalError)?;
            match command {
                Command::FixAll => {
                    let edits = super::code_action_resolve::fix_all_edit(
                        snapshot.document(),
                        &snapshot.configuration().linter,
                        snapshot.encoding(),
                    )
                    .with_failure_code(ErrorCode::InternalError)?;
                    edit_tracker.add_edits_for_document(uri, version, edits);
                }
                Command::Format => {
                    let response = super::format::format_document(&snapshot)?;
                    if let Some(edits) = response {
                        edit_tracker.add_edits_for_document(uri, version, edits);
                    }
                }
                Command::OrganizeImports => {
                    let edits = super::code_action_resolve::organize_imports_edit(
                        snapshot.document(),
                        &snapshot.configuration().linter,
                        snapshot.encoding(),
                    )
                    .with_failure_code(ErrorCode::InternalError)?;
                    edit_tracker.add_edits_for_document(uri, version, edits);
                }
            }
        }

        if !edit_tracker.is_empty() {
            apply_edit(
                requester,
                command.label(),
                edit_tracker.into_workspace_edit(),
            )
            .with_failure_code(ErrorCode::InternalError)?;
        }

        Ok(None)
    }
}

impl Command {
    fn label(&self) -> &str {
        match self {
            Self::FixAll => "Fix all auto-fixable problems",
            Self::Format => "Format document",
            Self::OrganizeImports => "Format imports",
        }
    }
}

impl FromStr for Command {
    type Err = anyhow::Error;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Ok(match name {
            "ruff.applyAutofix" => Self::FixAll,
            "ruff.applyFormat" => Self::Format,
            "ruff.applyOrganizeImports" => Self::OrganizeImports,
            _ => return Err(anyhow::anyhow!("Invalid command `{name}`")),
        })
    }
}

impl EditTracker {
    fn new(document_changes_supported: bool) -> Self {
        if document_changes_supported {
            Self::DocumentChanges(Vec::default())
        } else {
            Self::Changes(HashMap::default())
        }
    }

    fn add_edits_for_document(
        &mut self,
        uri: types::Url,
        version: DocumentVersion,
        new_edits: Vec<types::TextEdit>,
    ) {
        match self {
            Self::DocumentChanges(document_edits) => {
                if let Some(existing_edits) = document_edits
                    .iter_mut()
                    .find(|document| document.text_document.uri == uri)
                {
                    // A single task should only ever be operating on one version of a document. To operate on multiple simultaneous document versions
                    // is a logic error.
                    debug_assert_eq!(existing_edits.text_document.version, Some(version));
                    existing_edits
                        .edits
                        .extend(new_edits.into_iter().map(types::OneOf::Left));
                } else {
                    document_edits.push(TextDocumentEdit {
                        text_document: types::OptionalVersionedTextDocumentIdentifier {
                            uri,
                            version: Some(version),
                        },
                        edits: new_edits.into_iter().map(types::OneOf::Left).collect(),
                    });
                }
            }
            Self::Changes(changes) => {
                if let Some(existing_edits) = changes.get_mut(&uri) {
                    existing_edits.extend(new_edits);
                } else {
                    changes.insert(uri, new_edits);
                }
            }
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::DocumentChanges(document_edits) => document_edits.is_empty(),
            Self::Changes(changes) => changes.is_empty(),
        }
    }

    fn into_workspace_edit(self) -> types::WorkspaceEdit {
        match self {
            Self::DocumentChanges(document_edits) => types::WorkspaceEdit {
                document_changes: Some(types::DocumentChanges::Edits(document_edits)),
                ..Default::default()
            },
            Self::Changes(changes) => types::WorkspaceEdit::new(changes),
        }
    }
}

fn apply_edit(
    requester: &mut client::Requester,
    label: &str,
    edit: types::WorkspaceEdit,
) -> crate::Result<()> {
    requester.request::<req::ApplyWorkspaceEdit>(
        types::ApplyWorkspaceEditParams {
            label: Some(format!("{DIAGNOSTIC_NAME}: {label}")),
            edit,
        },
        |response| {
            if !response.applied {
                let reason = response
                    .failure_reason
                    .unwrap_or_else(|| String::from("unspecified reason"));
                tracing::error!("Failed to apply workspace edit: {}", reason);
            }
            Task::nothing()
        },
    )
}
