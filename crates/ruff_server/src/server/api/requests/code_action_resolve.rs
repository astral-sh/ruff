use std::borrow::Cow;

use crate::server::api::LSPResult;
use crate::server::SupportedCodeAction;
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
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

        let action_kind: SupportedCodeAction = action
            .kind
            .clone()
            .ok_or(anyhow::anyhow!("No kind was given for code action"))
            .with_failure_code(ErrorCode::InvalidParams)?
            .try_into()
            .map_err(|()| anyhow::anyhow!("Code action was of an invalid kind"))
            .with_failure_code(ErrorCode::InvalidParams)?;

        action.edit = match action_kind {
            SupportedCodeAction::SourceFixAll => Some(
                resolve_edit_for_fix_all(
                    document,
                    snapshot.url(),
                    &snapshot.configuration().linter,
                    snapshot.encoding(),
                )
                .with_failure_code(ErrorCode::InternalError)?,
            ),
            SupportedCodeAction::SourceOrganizeImports => Some(
                resolve_edit_for_organize_imports(
                    document,
                    snapshot.url(),
                    snapshot.configuration().linter.clone(),
                    snapshot.encoding(),
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
    url: &types::Url,
    linter_settings: &LinterSettings,
    encoding: PositionEncoding,
) -> crate::Result<types::WorkspaceEdit> {
    Ok(types::WorkspaceEdit {
        changes: Some(
            [(
                url.clone(),
                crate::fix::fix_all(document, linter_settings, encoding)?,
            )]
            .into_iter()
            .collect(),
        ),
        ..Default::default()
    })
}

pub(super) fn resolve_edit_for_organize_imports(
    document: &crate::edit::Document,
    url: &types::Url,
    mut linter_settings: ruff_linter::settings::LinterSettings,
    encoding: PositionEncoding,
) -> crate::Result<types::WorkspaceEdit> {
    linter_settings.rules = [
        Rule::UnusedImport,          // I001
        Rule::MissingRequiredImport, // I002
    ]
    .into_iter()
    .collect();

    let diagnostics = crate::lint::check(document, &linter_settings, encoding);

    let fixes = crate::lint::fixes_for_diagnostics(
        document,
        url,
        encoding,
        document.version(),
        diagnostics,
    )?;

    Ok(types::WorkspaceEdit {
        document_changes: Some(types::DocumentChanges::Edits(
            fixes
                .into_iter()
                .flat_map(|fix| fix.document_edits.into_iter())
                .collect(),
        )),
        ..Default::default()
    })
}
