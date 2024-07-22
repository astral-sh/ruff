use std::borrow::Cow;

use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};

use ruff_linter::codes::Rule;

use crate::edit::WorkspaceEditTracker;
use crate::fix::Fixes;
use crate::server::api::LSPResult;
use crate::server::SupportedCodeAction;
use crate::server::{client::Notifier, Result};
use crate::session::{DocumentQuery, DocumentSnapshot, ResolvedClientCapabilities};
use crate::PositionEncoding;

pub(crate) struct CodeActionResolve;

impl super::RequestHandler for CodeActionResolve {
    type RequestType = req::CodeActionResolveRequest;
}

impl super::BackgroundDocumentRequestHandler for CodeActionResolve {
    fn document_url(params: &types::CodeAction) -> Cow<types::Url> {
        let uri: lsp_types::Url = serde_json::from_value(params.data.clone().unwrap_or_default())
            .expect("code actions should have a URI in their data fields");
        Cow::Owned(uri)
    }
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        mut action: types::CodeAction,
    ) -> Result<types::CodeAction> {
        let query = snapshot.query();

        let code_actions = SupportedCodeAction::from_kind(
            action
                .kind
                .clone()
                .ok_or(anyhow::anyhow!("No kind was given for code action"))
                .with_failure_code(ErrorCode::InvalidParams)?,
        )
        .collect::<Vec<_>>();

        // Ensure that the code action maps to _exactly one_ supported code action
        let [action_kind] = code_actions.as_slice() else {
            return Err(anyhow::anyhow!(
                "Code action resolver did not expect code action kind {:?}",
                action.kind.as_ref().unwrap()
            ))
            .with_failure_code(ErrorCode::InvalidParams);
        };

        action.edit = match action_kind {
            SupportedCodeAction::SourceFixAll | SupportedCodeAction::NotebookSourceFixAll => Some(
                resolve_edit_for_fix_all(
                    query,
                    snapshot.resolved_client_capabilities(),
                    snapshot.encoding(),
                )
                .with_failure_code(ErrorCode::InternalError)?,
            ),
            SupportedCodeAction::SourceOrganizeImports
            | SupportedCodeAction::NotebookSourceOrganizeImports => Some(
                resolve_edit_for_organize_imports(
                    query,
                    snapshot.resolved_client_capabilities(),
                    snapshot.encoding(),
                )
                .with_failure_code(ErrorCode::InternalError)?,
            ),
            SupportedCodeAction::QuickFix => {
                // The client may ask us to resolve a code action, as it has no way of knowing
                // whether e.g. `command` field will be filled out by the resolution callback.
                return Ok(action);
            }
        };

        Ok(action)
    }
}

pub(super) fn resolve_edit_for_fix_all(
    query: &DocumentQuery,
    client_capabilities: &ResolvedClientCapabilities,
    encoding: PositionEncoding,
) -> crate::Result<types::WorkspaceEdit> {
    let mut tracker = WorkspaceEditTracker::new(client_capabilities);
    tracker.set_fixes_for_document(fix_all_edit(query, encoding)?, query.version())?;
    Ok(tracker.into_workspace_edit())
}

pub(super) fn fix_all_edit(
    query: &DocumentQuery,
    encoding: PositionEncoding,
) -> crate::Result<Fixes> {
    crate::fix::fix_all(query, query.settings().linter(), encoding)
}

pub(super) fn resolve_edit_for_organize_imports(
    query: &DocumentQuery,
    client_capabilities: &ResolvedClientCapabilities,
    encoding: PositionEncoding,
) -> crate::Result<types::WorkspaceEdit> {
    let mut tracker = WorkspaceEditTracker::new(client_capabilities);
    tracker.set_fixes_for_document(organize_imports_edit(query, encoding)?, query.version())?;
    Ok(tracker.into_workspace_edit())
}

pub(super) fn organize_imports_edit(
    query: &DocumentQuery,
    encoding: PositionEncoding,
) -> crate::Result<Fixes> {
    let mut linter_settings = query.settings().linter().clone();
    linter_settings.rules = [
        Rule::UnsortedImports,       // I001
        Rule::MissingRequiredImport, // I002
    ]
    .into_iter()
    .collect();

    crate::fix::fix_all(query, &linter_settings, encoding)
}
