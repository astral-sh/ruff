use crate::lint::fixes_for_diagnostics;
use crate::server::api::LSPResult;
use crate::server::SupportedCodeActionKind;
use crate::server::{client::Notifier, Result};
use crate::session::DocumentSnapshot;
use crate::DIAGNOSTIC_NAME;
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use rustc_hash::FxHashSet;
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
        let mut response: types::CodeActionResponse = types::CodeActionResponse::default();

        let supported_code_actions = supported_code_actions(params.context.only);

        tracing::error!("{supported_code_actions:?}");

        if supported_code_actions.contains(&SupportedCodeActionKind::QuickFix) {
            response.extend(
                quick_fix(&snapshot, params.context.diagnostics)
                    .with_failure_code(ErrorCode::InternalError)?,
            );
        }

        if supported_code_actions.contains(&SupportedCodeActionKind::SourceFixAll) {
            response.push(fix_all(&snapshot).with_failure_code(ErrorCode::InternalError)?);
        }

        if supported_code_actions.contains(&SupportedCodeActionKind::SourceOrganizeImports) {
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
    )?;

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

/// If `action_filter` is `None`, this returns [`SupportedCodeActionKind::all()`]. Otherwise,
/// the list is filtered.
fn supported_code_actions(
    action_filter: Option<Vec<CodeActionKind>>,
) -> FxHashSet<SupportedCodeActionKind> {
    let Some(action_filter) = action_filter else {
        return SupportedCodeActionKind::all().collect();
    };

    SupportedCodeActionKind::all()
        .filter(move |action| {
            action_filter.iter().any(|filter| {
                action
                    .kinds()
                    .iter()
                    .any(|kind| kind.as_str().starts_with(filter.as_str()))
            })
        })
        .collect()
}
