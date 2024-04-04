use std::collections::HashMap;

use crate::server;
use crate::server::api::LSPResult;
use crate::server::client;
use crate::server::schedule::Task;
use crate::session::Session;
use crate::DIAGNOSTIC_NAME;
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use serde::Deserialize;

enum Command {
    Format,
    FixAll,
    OrganizeImports,
}

pub(crate) struct ExecuteCommand;

#[derive(Deserialize)]
struct TextDocumentArgument {
    uri: types::Url,
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
        let Some(command) = Command::from_str(&params.command) else {
            return Err(anyhow::anyhow!("")).with_failure_code(ErrorCode::InvalidParams);
        };

        let mut changes = HashMap::new();
        let documents =
            args_as_text_documents(params.arguments).with_failure_code(ErrorCode::InvalidParams)?;
        for document in documents {
            let snapshot = session
                .take_snapshot(&document.uri)
                .ok_or(anyhow::anyhow!(
                    "Document snapshot not available for {}",
                    document.uri
                ))
                .with_failure_code(ErrorCode::InternalError)?;
            match command {
                Command::FixAll => {
                    let edits = super::code_action_resolve::fix_all_edit(
                        snapshot.document(),
                        &snapshot.configuration().linter,
                        snapshot.encoding(),
                    )
                    .with_failure_code(ErrorCode::InternalError)?;
                    changes.insert(document.uri, edits);
                }
                Command::Format => {
                    let response = super::format::format_document(&snapshot)?;
                    if let Some(edits) = response {
                        changes.insert(document.uri, edits);
                    }
                }
                Command::OrganizeImports => {
                    let edits = super::code_action_resolve::organize_imports_edit(
                        snapshot.document(),
                        &snapshot.configuration().linter,
                        snapshot.encoding(),
                    )
                    .with_failure_code(ErrorCode::InternalError)?;
                    changes.insert(document.uri, edits);
                }
            }
        }

        if !changes.is_empty() {
            // check if we can apply a workspace edit
            if !session.resolved_client_capabilities().apply_edit {
                return Err(anyhow::anyhow!("Cannot send workspace edit to client: the client does not support `workspace/applyEdit`")).with_failure_code(ErrorCode::InternalError);
            }
            apply_edit(
                requester,
                command.label(),
                types::WorkspaceEdit::new(changes),
            )
            .with_failure_code(ErrorCode::InternalError)?;
        }

        Ok(None)
    }
}

impl Command {
    fn from_str(command: &str) -> Option<Command> {
        Some(match command {
            "ruff.applyAutofix" => Self::FixAll,
            "ruff.applyFormat" => Self::Format,
            "ruff.applyOrganizeImports" => Self::OrganizeImports,
            _ => return None,
        })
    }

    fn label(&self) -> &str {
        match self {
            Self::FixAll => "Fix all auto-fixable problems",
            Self::Format => "Format document",
            Self::OrganizeImports => "Format imports",
        }
    }
}

fn args_as_text_documents(
    args: Vec<serde_json::Value>,
) -> crate::Result<Vec<TextDocumentArgument>> {
    args.into_iter()
        .map(|value| Ok(serde_json::from_value(value)?))
        .collect()
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
