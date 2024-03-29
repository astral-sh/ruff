use std::borrow::Cow;

use crate::lint::{fixes_for_diagnostics, DiagnosticFix};
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::server::{AvailableCodeActions, SupportedCodeActionKind};
use crate::session::DocumentSnapshot;
use crate::PositionEncoding;
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use ruff_linter::settings::LinterSettings;
use types::WorkspaceEdit;

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
        action: types::CodeAction,
    ) -> Result<types::CodeAction> {
        let document = snapshot.document();

        let supported: SupportedCodeActionKind = action
            .kind
            .clone()
            .ok_or(anyhow::anyhow!("No kind was given for code action"))
            .with_failure_code(ErrorCode::InvalidParams)?
            .try_into()
            .map_err(|()| anyhow::anyhow!("Code action was of an invalid kind"))
            .with_failure_code(ErrorCode::InvalidParams)?;
        let available_action = supported.makes_available();

        // ensures that only one code action kind was made available
        debug_assert!(
            (available_action & (available_action - AvailableCodeActions::all()))
                == AvailableCodeActions::empty()
        );

        if available_action == AvailableCodeActions::SOURCE_FIX_ALL {
            resolve_edit_for_fix_all(
                action,
                document,
                snapshot.url(),
                &snapshot.configuration().linter,
                snapshot.encoding(),
            )
            .with_failure_code(ErrorCode::InternalError)
        } else {
            Err(anyhow::anyhow!("")).with_failure_code(ErrorCode::InvalidParams)
        }
    }
}

pub(super) fn resolve_edit_for_fix_all(
    mut action: types::CodeAction,
    document: &crate::edit::Document,
    url: &types::Url,
    linter_settings: &LinterSettings,
    encoding: PositionEncoding,
) -> crate::Result<types::CodeAction> {
    let diagnostics = crate::lint::check(document, linter_settings, encoding);

    let fixes = fixes_for_diagnostics(document, url, encoding, document.version(), diagnostics)
        .collect::<crate::Result<Vec<_>>>()?;

    action.edit = fix_all_edit(fixes.as_slice());

    Ok(action)
}

fn fix_all_edit(fixes: &[DiagnosticFix]) -> Option<WorkspaceEdit> {
    let edits_made: Vec<_> = fixes
        .iter()
        .filter(|fix| fix.applicability.is_safe())
        .collect();

    if edits_made.is_empty() {
        return None;
    }

    Some(types::WorkspaceEdit {
        document_changes: Some(types::DocumentChanges::Edits(
            edits_made
                .into_iter()
                .flat_map(|fixes| fixes.document_edits.iter())
                .cloned()
                .collect(),
        )),
        ..Default::default()
    })
}
