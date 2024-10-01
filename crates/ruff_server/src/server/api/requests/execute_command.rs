use std::str::FromStr;

use crate::edit::WorkspaceEditTracker;
use crate::server::api::LSPResult;
use crate::server::schedule::Task;
use crate::server::{client, SupportedCommand};
use crate::session::Session;
use crate::DIAGNOSTIC_NAME;
use crate::{edit::DocumentVersion, server};
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use serde::Deserialize;

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
        notifier: client::Notifier,
        requester: &mut client::Requester,
        params: types::ExecuteCommandParams,
    ) -> server::Result<Option<serde_json::Value>> {
        let command = SupportedCommand::from_str(&params.command)
            .with_failure_code(ErrorCode::InvalidParams)?;

        if command == SupportedCommand::Debug {
            let output = debug_information(session);
            notifier
                .notify::<types::notification::LogMessage>(types::LogMessageParams {
                    message: output,
                    typ: types::MessageType::INFO,
                })
                .with_failure_code(ErrorCode::InternalError)?;
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
            let Some(snapshot) = session.take_snapshot(uri.clone()) else {
                tracing::error!("Document at {uri} could not be opened");
                show_err_msg!("Ruff does not recognize this file");
                return Ok(None);
            };
            match command {
                SupportedCommand::FixAll => {
                    let fixes = super::code_action_resolve::fix_all_edit(
                        snapshot.query(),
                        snapshot.encoding(),
                    )
                    .with_failure_code(ErrorCode::InternalError)?;
                    edit_tracker
                        .set_fixes_for_document(fixes, snapshot.query().version())
                        .with_failure_code(ErrorCode::InternalError)?;
                }
                SupportedCommand::Format => {
                    let fixes = super::format::format_full_document(&snapshot)?;
                    edit_tracker
                        .set_fixes_for_document(fixes, version)
                        .with_failure_code(ErrorCode::InternalError)?;
                }
                SupportedCommand::OrganizeImports => {
                    let fixes = super::code_action_resolve::organize_imports_edit(
                        snapshot.query(),
                        snapshot.encoding(),
                    )
                    .with_failure_code(ErrorCode::InternalError)?;
                    edit_tracker
                        .set_fixes_for_document(fixes, snapshot.query().version())
                        .with_failure_code(ErrorCode::InternalError)?;
                }
                SupportedCommand::Debug => {
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
    let executable = std::env::current_exe()
        .map(|path| format!("{}", path.display()))
        .unwrap_or_else(|_| "<unavailable>".to_string());
    format!(
        "executable = {executable}
version = {version}
encoding = {encoding:?}
open_document_count = {doc_count}
active_workspace_count = {workspace_count}
configuration_files = {config_files:?}
{client_capabilities}",
        version = crate::version(),
        encoding = session.encoding(),
        client_capabilities = session.resolved_client_capabilities(),
        doc_count = session.num_documents(),
        workspace_count = session.num_workspaces(),
        config_files = session.list_config_files()
    )
}
