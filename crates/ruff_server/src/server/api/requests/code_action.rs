use crate::edit::WorkspaceEditTracker;
use crate::lint::{fixes_for_diagnostics, DiagnosticFix};
use crate::server::api::LSPResult;
use crate::server::SupportedCodeAction;
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use crate::DIAGNOSTIC_NAME;
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use rustc_hash::FxHashSet;
use types::{CodeActionKind, CodeActionOrCommand};

use super::code_action_resolve::{resolve_edit_for_fix_all, resolve_edit_for_organize_imports};

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
        let mut response: types::CodeActionResponse = types::CodeActionResponse::default();

        let supported_code_actions = supported_code_actions(params.context.only.clone());

        let fixes = fixes_for_diagnostics(
            snapshot.document(),
            snapshot.encoding(),
            params.context.diagnostics,
        )
        .with_failure_code(ErrorCode::InternalError)?;

        if snapshot.client_settings().fix_violation()
            && supported_code_actions.contains(&SupportedCodeAction::QuickFix)
        {
            response
                .extend(quick_fix(&snapshot, &fixes).with_failure_code(ErrorCode::InternalError)?);
        }

        if snapshot.client_settings().noqa_comments()
            && supported_code_actions.contains(&SupportedCodeAction::QuickFix)
        {
            response.extend(noqa_comments(&snapshot, &fixes));
        }

        if snapshot.client_settings().fix_all()
            && supported_code_actions.contains(&SupportedCodeAction::SourceFixAll)
        {
            response.push(fix_all(&snapshot).with_failure_code(ErrorCode::InternalError)?);
        }

        if snapshot.client_settings().organize_imports()
            && supported_code_actions.contains(&SupportedCodeAction::SourceOrganizeImports)
        {
            response.push(organize_imports(&snapshot).with_failure_code(ErrorCode::InternalError)?);
        }

        Ok(Some(response))
    }
}

fn quick_fix(
    snapshot: &DocumentSnapshot,
    fixes: &[DiagnosticFix],
) -> crate::Result<Vec<CodeActionOrCommand>> {
    let document = snapshot.document();
    fixes
        .iter()
        .filter(|fix| fix.edits.is_some())
        .map(|fix| {
            let mut tracker = WorkspaceEditTracker::new(snapshot.resolved_client_capabilities());

            tracker.set_edits_for_document(
                snapshot.url().clone(),
                document.version(),
                fix.edits
                    .as_ref()
                    .expect("should only be iterating over fixes with available edits")
                    .clone(),
            )?;

            Ok(types::CodeActionOrCommand::CodeAction(types::CodeAction {
                title: format!("{DIAGNOSTIC_NAME} ({}): {}", fix.code, fix.title),
                kind: Some(types::CodeActionKind::QUICKFIX),
                edit: Some(tracker.into_workspace_edit()),
                diagnostics: Some(vec![fix.fixed_diagnostic.clone()]),
                data: Some(
                    serde_json::to_value(snapshot.url()).expect("document url to serialize"),
                ),
                ..Default::default()
            }))
        })
        .collect()
}

fn noqa_comments(snapshot: &DocumentSnapshot, fixes: &[DiagnosticFix]) -> Vec<CodeActionOrCommand> {
    fixes
        .iter()
        .filter_map(|fix| {
            let edit = fix.noqa_edit.as_ref()?.clone();

            let mut tracker = WorkspaceEditTracker::new(snapshot.resolved_client_capabilities());

            tracker
                .set_edits_for_document(
                    snapshot.url().clone(),
                    snapshot.document().version(),
                    vec![edit],
                )
                .ok()?;

            Some(types::CodeActionOrCommand::CodeAction(types::CodeAction {
                title: format!("{DIAGNOSTIC_NAME} ({}): Disable for this line", fix.code),
                kind: Some(types::CodeActionKind::QUICKFIX),
                edit: Some(tracker.into_workspace_edit()),
                diagnostics: Some(vec![fix.fixed_diagnostic.clone()]),
                data: Some(
                    serde_json::to_value(snapshot.url()).expect("document url to serialize"),
                ),
                ..Default::default()
            }))
        })
        .collect()
}

fn fix_all(snapshot: &DocumentSnapshot) -> crate::Result<CodeActionOrCommand> {
    let document = snapshot.document();

    let (edit, data) = if snapshot
        .resolved_client_capabilities()
        .code_action_deferred_edit_resolution
    {
        // The editor will request the edit in a `CodeActionsResolve` request
        (
            None,
            Some(serde_json::to_value(snapshot.url()).expect("document url to serialize")),
        )
    } else {
        (
            Some(resolve_edit_for_fix_all(
                document,
                snapshot.resolved_client_capabilities(),
                snapshot.url(),
                snapshot.settings().linter(),
                snapshot.encoding(),
                document.version(),
            )?),
            None,
        )
    };

    Ok(CodeActionOrCommand::CodeAction(types::CodeAction {
        title: format!("{DIAGNOSTIC_NAME}: Fix all auto-fixable problems"),
        kind: Some(crate::SOURCE_FIX_ALL_RUFF),
        edit,
        data,
        ..Default::default()
    }))
}

fn organize_imports(snapshot: &DocumentSnapshot) -> crate::Result<CodeActionOrCommand> {
    let document = snapshot.document();

    let (edit, data) = if snapshot
        .resolved_client_capabilities()
        .code_action_deferred_edit_resolution
    {
        // The edit will be resolved later in the `CodeActionsResolve` request
        (
            None,
            Some(serde_json::to_value(snapshot.url()).expect("document url to serialize")),
        )
    } else {
        (
            Some(resolve_edit_for_organize_imports(
                document,
                snapshot.resolved_client_capabilities(),
                snapshot.url(),
                snapshot.settings().linter(),
                snapshot.encoding(),
                document.version(),
            )?),
            None,
        )
    };

    Ok(CodeActionOrCommand::CodeAction(types::CodeAction {
        title: format!("{DIAGNOSTIC_NAME}: Organize imports"),
        kind: Some(crate::SOURCE_ORGANIZE_IMPORTS_RUFF),
        edit,
        data,
        ..Default::default()
    }))
}

/// If `action_filter` is `None`, this returns [`SupportedCodeActionKind::all()`]. Otherwise,
/// the list is filtered.
fn supported_code_actions(
    action_filter: Option<Vec<CodeActionKind>>,
) -> FxHashSet<SupportedCodeAction> {
    let Some(action_filter) = action_filter else {
        return SupportedCodeAction::all().collect();
    };

    action_filter
        .into_iter()
        .flat_map(SupportedCodeAction::from_kind)
        .collect()
}
