use crate::lint::fixes_for_diagnostics;
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::server::{AvailableCodeActions, SupportedCodeActionKind};
use crate::session::DocumentSnapshot;
use crate::DIAGNOSTIC_NAME;
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use types::{CodeActionKind, CodeActionOrCommand};

use super::code_action_resolve::resolve_edit_for_fix_all;

pub(crate) struct CodeActions;

impl super::RequestHandler for CodeActions {
    type RequestType = req::CodeActionRequest;
}

impl super::BackgroundDocumentRequestHandler for CodeActions {
    super::define_document_url!(params: &types::CodeActionParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _notifier: Notifier,
        params: types::CodeActionParams,
    ) -> Result<Option<types::CodeActionResponse>> {
        let available_actions = available_code_actions(params.context.only);
        // fast path - return early if no actions are available
        if available_actions.is_empty() {
            return Ok(None);
        }

        let mut response: types::CodeActionResponse = types::CodeActionResponse::default();

        if available_actions.contains(AvailableCodeActions::QUICK_FIX) {
            response.extend(
                quick_fix(&snapshot, params.context.diagnostics)
                    .with_failure_code(ErrorCode::InternalError)?,
            );
        }

        if available_actions.contains(AvailableCodeActions::SOURCE_FIX_ALL) {
            response.push(fix_all(&snapshot).with_failure_code(ErrorCode::InternalError)?);
        }

        if available_actions.contains(AvailableCodeActions::SOURCE_ORGANIZE_IMPORTS) {
            todo!("Implement the `source.organizeImports` code action");
        }

        Ok(Some(response))
    }
}

fn quick_fix(
    snapshot: &DocumentSnapshot,
    diagnostics: Vec<types::Diagnostic>,
) -> crate::Result<impl Iterator<Item = CodeActionOrCommand> + '_> {
    let document = snapshot.document();

    let fixes = fixes_for_diagnostics(
        document,
        snapshot.url(),
        snapshot.encoding(),
        document.version(),
        diagnostics,
    )
    .collect::<crate::Result<Vec<_>>>()?;

    Ok(fixes.into_iter().map(|fix| {
        types::CodeActionOrCommand::CodeAction(types::CodeAction {
            title: format!("{DIAGNOSTIC_NAME} ({}): {}", fix.code, fix.title),
            kind: Some(types::CodeActionKind::QUICKFIX),
            edit: Some(types::WorkspaceEdit {
                document_changes: Some(types::DocumentChanges::Edits(fix.document_edits.clone())),
                ..Default::default()
            }),
            diagnostics: Some(vec![fix.fixed_diagnostic.clone()]),
            data: Some(serde_json::to_value(snapshot.url()).expect("document url to serialize")),
            ..Default::default()
        })
    }))
}

fn fix_all(snapshot: &DocumentSnapshot) -> crate::Result<CodeActionOrCommand> {
    let mut action = types::CodeAction {
        title: format!("{DIAGNOSTIC_NAME}: Fix all auto-fixable problems"),
        kind: Some(types::CodeActionKind::SOURCE_FIX_ALL),
        // This will be resolved later
        edit: None,
        data: Some(serde_json::to_value(snapshot.url()).expect("document url to serialize")),
        ..Default::default()
    };

    if !snapshot
        .resolved_client_capabilities()
        .code_action_deferred_edit_resolution
    {
        let document = snapshot.document();

        // We need to resolve the `edit` field now if we can't defer resolution to later
        action = resolve_edit_for_fix_all(
            action,
            document,
            snapshot.url(),
            &snapshot.configuration().linter,
            snapshot.encoding(),
        )?;
    }

    Ok(types::CodeActionOrCommand::CodeAction(action))
}

/// If `action_filter` is `None`, this returns [`SupportedCodeAction::all()`]. Otherwise,
/// the list is filtered.
fn available_code_actions(action_filter: Option<Vec<CodeActionKind>>) -> AvailableCodeActions {
    let Some(action_filter) = action_filter else {
        return SupportedCodeActionKind::all()
            .fold(AvailableCodeActions::empty(), |available, kind| {
                available | kind.makes_available()
            });
    };

    SupportedCodeActionKind::all()
        .filter(|action| {
            action_filter
                .iter()
                .any(|kind| action.kind().as_str().starts_with(kind.as_str()))
        })
        .fold(AvailableCodeActions::empty(), |available, kind| {
            available | kind.makes_available()
        })
}
