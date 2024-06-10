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
    Debug,
    Format,
    FixAll,
    OrganizeImports,
}

pub(crate) struct ExecuteCommand;

#[derive(Deserialize)]
struct Argument {
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

        if let Command::Debug = command {
            let output = debug_information(session);
            #[allow(clippy::print_stderr)]
            {
                eprintln!("Debug information:\n{output}");
            }
            return Ok(None);
        }

        // check if we can apply a workspace edit
        if !session.resolved_client_capabilities().apply_edit {
            return Err(anyhow::anyhow!("Cannot execute the '{}' command: the client does not support `workspace/applyEdit`", command.label())).with_failure_code(ErrorCode::InternalError);
        }

        let mut arguments: Vec<Argument> = params
            .arguments
            .into_iter()
            .map(|value| serde_json::from_value(value).with_failure_code(ErrorCode::InvalidParams))
            .collect::<server::Result<_>>()?;

        arguments.sort_by(|a, b| a.uri.cmp(&b.uri));
        arguments.dedup_by(|a, b| a.uri == b.uri);

        let mut edit_tracker = WorkspaceEditTracker::new(session.resolved_client_capabilities());
        for Argument { uri, version } in arguments {
            let snapshot = session
                .take_snapshot(uri.clone())
                .ok_or(anyhow::anyhow!("Document snapshot not available for {uri}",))
                .with_failure_code(ErrorCode::InternalError)?;
            match command {
                Command::FixAll => {
                    let fixes = super::code_action_resolve::fix_all_edit(
                        snapshot.query(),
                        snapshot.encoding(),
                    )
                    .with_failure_code(ErrorCode::InternalError)?;
                    edit_tracker
                        .set_fixes_for_document(fixes, snapshot.query().version())
                        .with_failure_code(ErrorCode::InternalError)?;
                }
                Command::Format => {
                    let fixes = super::format::format_full_document(&snapshot)?;
                    edit_tracker
                        .set_fixes_for_document(fixes, version)
                        .with_failure_code(ErrorCode::InternalError)?;
                }
                Command::OrganizeImports => {
                    let fixes = super::code_action_resolve::organize_imports_edit(
                        snapshot.query(),
                        snapshot.encoding(),
                    )
                    .with_failure_code(ErrorCode::InternalError)?;
                    edit_tracker
                        .set_fixes_for_document(fixes, snapshot.query().version())
                        .with_failure_code(ErrorCode::InternalError)?;
                }
                Command::Debug => {
                    unreachable!("The debug command should have already been handled")
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
            Self::Debug => "Print debug information",
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
            "ruff.printDebugInformation" => Self::Debug,
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

fn debug_information(session: &Session) -> String {
    let path = std::env::current_exe()
        .map(|path| format!("{}", path.display()))
        .unwrap_or_else(|_| "<unavailable>".to_string());
    format!(
        r#"path = {path}
version = {version}
encoding = {encoding:?}
open_document_count = {doc_count}
active_workspace_count = {workspace_count}
configuration_files = {config_files:?}
{client_capabilities}
    "#,
        version = crate::version(),
        encoding = session.encoding(),
        client_capabilities = session.resolved_client_capabilities(),
        doc_count = session.count_documents(),
        workspace_count = session.count_workspaces(),
        config_files = session.list_config_files()
    )
}
