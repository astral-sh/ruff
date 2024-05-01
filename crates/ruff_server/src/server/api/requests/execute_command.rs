use std::str::FromStr;

use crate::edit::WorkspaceEditTracker;
use crate::server::api::LSPResult;
use crate::server::client;
use crate::server::schedule::Task;
use crate::session::Session;
use crate::DIAGNOSTIC_NAME;
use crate::{edit::DocumentVersion, server};
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use serde::Deserialize;

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

        let mut arguments: Vec<TextDocumentArgument> = params
            .arguments
            .into_iter()
            .map(|value| serde_json::from_value(value).with_failure_code(ErrorCode::InvalidParams))
            .collect::<server::Result<_>>()?;

        arguments.sort_by(|a, b| a.uri.cmp(&b.uri));
        arguments.dedup_by(|a, b| a.uri == b.uri);

        let mut edit_tracker = WorkspaceEditTracker::new(session.resolved_client_capabilities());
        for TextDocumentArgument { uri, version } in arguments {
            let snapshot = session
                .take_snapshot(&uri)
                .ok_or(anyhow::anyhow!("Document snapshot not available for {uri}",))
                .with_failure_code(ErrorCode::InternalError)?;
            match command {
                Command::FixAll => {
                    let edits = super::code_action_resolve::fix_all_edit(
                        snapshot.document(),
                        snapshot.url(),
                        snapshot.settings().linter(),
                        snapshot.encoding(),
                    )
                    .with_failure_code(ErrorCode::InternalError)?;
                    edit_tracker
                        .set_edits_for_document(uri, version, edits)
                        .with_failure_code(ErrorCode::InternalError)?;
                }
                Command::Format => {
                    let response = super::format::format_document(&snapshot)?;
                    if let Some(edits) = response {
                        edit_tracker
                            .set_edits_for_document(uri, version, edits)
                            .with_failure_code(ErrorCode::InternalError)?;
                    }
                }
                Command::OrganizeImports => {
                    let edits = super::code_action_resolve::organize_imports_edit(
                        snapshot.document(),
                        snapshot.url(),
                        snapshot.settings().linter(),
                        snapshot.encoding(),
                    )
                    .with_failure_code(ErrorCode::InternalError)?;
                    edit_tracker
                        .set_edits_for_document(uri, version, edits)
                        .with_failure_code(ErrorCode::InternalError)?;
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
                tracing::error!("Failed to apply workspace edit: {reason}");
                show_err_msg!("Ruff was unable to apply edits: {reason}");
            }
            Task::nothing()
        },
    )
}
