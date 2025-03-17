use std::fmt::Write;
use std::str::FromStr;

use crate::edit::WorkspaceEditTracker;
use crate::server::api::LSPResult;
use crate::server::schedule::Task;
use crate::server::{client, SupportedCommand};
use crate::session::Session;
use crate::{edit::DocumentVersion, server};
use crate::{DocumentKey, DIAGNOSTIC_NAME};
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req, TextDocumentIdentifier};
use serde::Deserialize;

pub(crate) struct ExecuteCommand;

#[derive(Deserialize)]
struct Argument {
    uri: types::Url,
    version: DocumentVersion,
}

/// The argument schema for the `ruff.printDebugInformation` command.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DebugCommandArgument {
    /// The URI of the document to print debug information for.
    ///
    /// When provided, both document-specific debug information and global information are printed.
    /// If not provided ([None]), only global debug information is printed.
    text_document: Option<TextDocumentIdentifier>,
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
        let command = SupportedCommand::from_str(&params.command)
            .with_failure_code(ErrorCode::InvalidParams)?;

        if command == SupportedCommand::Debug {
            // TODO: Currently we only use the first argument i.e., the first document that's
            // provided but we could expand this to consider all *open* documents.
            let argument: DebugCommandArgument = params.arguments.into_iter().next().map_or_else(
                || Ok(DebugCommandArgument::default()),
                |value| serde_json::from_value(value).with_failure_code(ErrorCode::InvalidParams),
            )?;
            return Ok(Some(serde_json::Value::String(
                debug_information(session, argument.text_document)
                    .with_failure_code(ErrorCode::InternalError)?,
            )));
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

/// Returns a string with debug information about the session and the document at the given URI.
fn debug_information(
    session: &Session,
    text_document: Option<TextDocumentIdentifier>,
) -> crate::Result<String> {
    let executable = std::env::current_exe()
        .map(|path| format!("{}", path.display()))
        .unwrap_or_else(|_| "<unavailable>".to_string());

    let mut buffer = String::new();

    writeln!(
        buffer,
        "Global:
executable = {executable}
version = {version}
position_encoding = {encoding:?}
workspace_root_folders = {workspace_folders:#?}
indexed_configuration_files = {config_files:#?}
open_documents_len = {open_documents_len}
client_capabilities = {client_capabilities:#?}
",
        version = crate::version(),
        encoding = session.encoding(),
        workspace_folders = session.workspace_root_folders().collect::<Vec<_>>(),
        config_files = session.config_file_paths().collect::<Vec<_>>(),
        open_documents_len = session.open_documents_len(),
        client_capabilities = session.resolved_client_capabilities(),
    )?;

    if let Some(TextDocumentIdentifier { uri }) = text_document {
        let Some(snapshot) = session.take_snapshot(uri.clone()) else {
            writeln!(buffer, "Unable to take a snapshot of the document at {uri}")?;
            return Ok(buffer);
        };
        let query = snapshot.query();

        writeln!(
            buffer,
            "Open document:
uri = {uri}
kind = {kind}
version = {version}
client_settings = {client_settings:#?}
config_path = {config_path:?}
{settings}
            ",
            uri = uri.clone(),
            kind = match session.key_from_url(uri) {
                DocumentKey::Notebook(_) => "Notebook",
                DocumentKey::NotebookCell(_) => "NotebookCell",
                DocumentKey::Text(_) => "Text",
            },
            version = query.version(),
            client_settings = snapshot.client_settings(),
            config_path = query.settings().path(),
            settings = query.settings(),
        )?;
    } else {
        writeln!(
            buffer,
            "global_client_settings = {:#?}",
            session.global_client_settings()
        )?;
    }

    Ok(buffer)
}
