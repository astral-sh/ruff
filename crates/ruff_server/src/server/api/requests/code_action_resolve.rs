use std::borrow::Cow;

use crate::edit::{DocumentVersion, WorkspaceEditTracker};
use crate::server::api::LSPResult;
use crate::server::SupportedCodeAction;
use crate::server::{client::Notifier, Result};
use crate::session::{DocumentSnapshot, ResolvedClientCapabilities};
use crate::PositionEncoding;
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use ruff_linter::codes::Rule;
use ruff_linter::settings::LinterSettings;

pub(crate) struct CodeActionResolve;

impl super::RequestHandler for CodeActionResolve {
    type RequestType = req::CodeActionResolveRequest;
}

impl super::BackgroundDocumentRequestHandler for CodeActionResolve {
    fn document_url(params: &types::CodeAction) -> Cow<types::Url> {
        let uri: lsp_types::Url = serde_json::from_value(params.data.clone().unwrap_or_default())
            .expect("code actions should have a URI in their data fields");
        std::borrow::Cow::Owned(uri)
    }
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        mut action: types::CodeAction,
    ) -> Result<types::CodeAction> {
        let document = snapshot.document();

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
            SupportedCodeAction::SourceFixAll => Some(
                resolve_edit_for_fix_all(
                    document,
                    snapshot.resolved_client_capabilities(),
                    snapshot.url(),
                    snapshot.settings().linter(),
                    snapshot.encoding(),
                    document.version(),
                )
                .with_failure_code(ErrorCode::InternalError)?,
            ),
            SupportedCodeAction::SourceOrganizeImports => Some(
                resolve_edit_for_organize_imports(
                    document,
                    snapshot.resolved_client_capabilities(),
                    snapshot.url(),
                    snapshot.settings().linter(),
                    snapshot.encoding(),
                    document.version(),
                )
                .with_failure_code(ErrorCode::InternalError)?,
            ),
            SupportedCodeAction::QuickFix => {
                return Err(anyhow::anyhow!(
                    "Got a code action that should not need additional resolution: {action_kind:?}"
                ))
                .with_failure_code(ErrorCode::InvalidParams)
            }
        };

        Ok(action)
    }
}

pub(super) fn resolve_edit_for_fix_all(
    document: &crate::edit::Document,
    client_capabilities: &ResolvedClientCapabilities,
    url: &types::Url,
    linter_settings: &LinterSettings,
    encoding: PositionEncoding,
    version: DocumentVersion,
) -> crate::Result<types::WorkspaceEdit> {
    let mut tracker = WorkspaceEditTracker::new(client_capabilities);
    tracker.set_edits_for_document(
        url.clone(),
        version,
        fix_all_edit(document, url, linter_settings, encoding)?,
    )?;
    Ok(tracker.into_workspace_edit())
}

pub(super) fn fix_all_edit(
    document: &crate::edit::Document,
    document_url: &types::Url,
    linter_settings: &LinterSettings,
    encoding: PositionEncoding,
) -> crate::Result<Vec<types::TextEdit>> {
    crate::fix::fix_all(document, document_url, linter_settings, encoding)
}

pub(super) fn resolve_edit_for_organize_imports(
    document: &crate::edit::Document,
    client_capabilities: &ResolvedClientCapabilities,
    url: &types::Url,
    linter_settings: &ruff_linter::settings::LinterSettings,
    encoding: PositionEncoding,
    version: DocumentVersion,
) -> crate::Result<types::WorkspaceEdit> {
    let mut tracker = WorkspaceEditTracker::new(client_capabilities);
    tracker.set_edits_for_document(
        url.clone(),
        version,
        organize_imports_edit(document, url, linter_settings, encoding)?,
    )?;
    Ok(tracker.into_workspace_edit())
}

pub(super) fn organize_imports_edit(
    document: &crate::edit::Document,
    document_url: &types::Url,
    linter_settings: &LinterSettings,
    encoding: PositionEncoding,
) -> crate::Result<Vec<types::TextEdit>> {
    let mut linter_settings = linter_settings.clone();
    linter_settings.rules = [
        Rule::UnsortedImports,       // I001
        Rule::MissingRequiredImport, // I002
    ]
    .into_iter()
    .collect();

    crate::fix::fix_all(document, document_url, &linter_settings, encoding)
}
