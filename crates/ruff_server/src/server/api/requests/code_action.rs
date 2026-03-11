use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use ruff_python_ast::SourceType;
use rustc_hash::FxHashSet;
use types::{CodeActionKind, CodeActionOrCommand};

use crate::DIAGNOSTIC_NAME;
use crate::edit::WorkspaceEditTracker;
use crate::lint::{DiagnosticFix, fixes_for_diagnostics};
use crate::resolve::is_document_excluded_for_linting;
use crate::server::Result;
use crate::server::SupportedCodeAction;
use crate::server::api::LSPResult;
use crate::session::{Client, DocumentSnapshot};

use super::code_action_resolve::{resolve_edit_for_fix_all, resolve_edit_for_organize_imports};

pub(crate) struct CodeActions;

impl super::RequestHandler for CodeActions {
    type RequestType = req::CodeActionRequest;
}

impl super::BackgroundDocumentRequestHandler for CodeActions {
    super::define_document_url!(params: &types::CodeActionParams);
    fn run_with_snapshot(
        snapshot: DocumentSnapshot,
        _client: &Client,
        params: types::CodeActionParams,
    ) -> Result<Option<types::CodeActionResponse>> {
        let mut response: types::CodeActionResponse = types::CodeActionResponse::default();

        let query = snapshot.query();

        // Don't provide code actions for non-Python documents (e.g., markdown files).
        let SourceType::Python(_) = query.source_type() else {
            return Ok(Some(response));
        };

        let document_path = query.virtual_file_path();
        let settings = query.settings();

        if is_document_excluded_for_linting(
            &document_path,
            &settings.file_resolver,
            &settings.linter,
            query.text_document_language_id(),
        ) {
            return Ok(Some(response));
        }

        let supported_code_actions = supported_code_actions(params.context.only.clone());

        let fixes = fixes_for_diagnostics(params.context.diagnostics)
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

        if snapshot.client_settings().fix_all() {
            if supported_code_actions.contains(&SupportedCodeAction::SourceFixAll) {
                if snapshot.is_notebook_cell() {
                    // This is ignore here because the client requests this code action for each
                    // cell in parallel and the server would send a workspace edit with the same
                    // content which would result in applying the same edit multiple times
                    // resulting in (possibly) duplicate code.
                    tracing::debug!("Ignoring `source.fixAll` code action for a notebook cell");
                } else {
                    response.push(fix_all(&snapshot).with_failure_code(ErrorCode::InternalError)?);
                }
            } else if supported_code_actions.contains(&SupportedCodeAction::NotebookSourceFixAll) {
                response
                    .push(notebook_fix_all(&snapshot).with_failure_code(ErrorCode::InternalError)?);
            }
        }

        if snapshot.client_settings().organize_imports() {
            if supported_code_actions.contains(&SupportedCodeAction::SourceOrganizeImports) {
                if snapshot.is_notebook_cell() {
                    // This is ignore here because the client requests this code action for each
                    // cell in parallel and the server would send a workspace edit with the same
                    // content which would result in applying the same edit multiple times
                    // resulting in (possibly) duplicate code.
                    tracing::debug!(
                        "Ignoring `source.organizeImports` code action for a notebook cell"
                    );
                } else {
                    response.push(
                        organize_imports(&snapshot).with_failure_code(ErrorCode::InternalError)?,
                    );
                }
            } else if supported_code_actions
                .contains(&SupportedCodeAction::NotebookSourceOrganizeImports)
            {
                response.push(
                    notebook_organize_imports(&snapshot)
                        .with_failure_code(ErrorCode::InternalError)?,
                );
            }
        }

        Ok(Some(response))
    }
}

fn quick_fix(
    snapshot: &DocumentSnapshot,
    fixes: &[DiagnosticFix],
) -> crate::Result<Vec<CodeActionOrCommand>> {
    let document = snapshot.query();

    fixes
        .iter()
        .filter(|fix| !fix.edits.is_empty())
        .map(|fix| {
            let mut tracker = WorkspaceEditTracker::new(snapshot.resolved_client_capabilities());

            let document_url = snapshot.query().make_key().into_url();

            tracker.set_edits_for_document(
                document_url.clone(),
                document.version(),
                fix.edits.clone(),
            )?;

            Ok(types::CodeActionOrCommand::CodeAction(types::CodeAction {
                title: format!("{DIAGNOSTIC_NAME} ({}): {}", fix.code, fix.title),
                kind: Some(types::CodeActionKind::QUICKFIX),
                edit: Some(tracker.into_workspace_edit()),
                diagnostics: Some(vec![fix.fixed_diagnostic.clone()]),
                data: Some(
                    serde_json::to_value(document_url).expect("document url should serialize"),
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
            let edit = fix.noqa_edit.clone()?;

            let mut tracker = WorkspaceEditTracker::new(snapshot.resolved_client_capabilities());

            tracker
                .set_edits_for_document(
                    snapshot.query().make_key().into_url(),
                    snapshot.query().version(),
                    vec![edit],
                )
                .ok()?;

            Some(types::CodeActionOrCommand::CodeAction(types::CodeAction {
                title: format!("{DIAGNOSTIC_NAME} ({}): Disable for this line", fix.code),
                kind: Some(types::CodeActionKind::QUICKFIX),
                edit: Some(tracker.into_workspace_edit()),
                diagnostics: Some(vec![fix.fixed_diagnostic.clone()]),
                data: Some(
                    serde_json::to_value(snapshot.query().make_key().into_url())
                        .expect("document url should serialize"),
                ),
                ..Default::default()
            }))
        })
        .collect()
}

fn fix_all(snapshot: &DocumentSnapshot) -> crate::Result<CodeActionOrCommand> {
    let document = snapshot.query();

    let (edit, data) = if snapshot
        .resolved_client_capabilities()
        .code_action_deferred_edit_resolution
    {
        // The editor will request the edit in a `CodeActionsResolve` request
        (
            None,
            Some(
                serde_json::to_value(snapshot.query().make_key().into_url())
                    .expect("document url should serialize"),
            ),
        )
    } else {
        (
            Some(resolve_edit_for_fix_all(
                document,
                snapshot.resolved_client_capabilities(),
                snapshot.encoding(),
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

fn notebook_fix_all(snapshot: &DocumentSnapshot) -> crate::Result<CodeActionOrCommand> {
    let document = snapshot.query();

    let (edit, data) = if snapshot
        .resolved_client_capabilities()
        .code_action_deferred_edit_resolution
    {
        // The editor will request the edit in a `CodeActionsResolve` request
        (
            None,
            Some(
                serde_json::to_value(snapshot.query().make_key().into_url())
                    .expect("document url should serialize"),
            ),
        )
    } else {
        (
            Some(resolve_edit_for_fix_all(
                document,
                snapshot.resolved_client_capabilities(),
                snapshot.encoding(),
            )?),
            None,
        )
    };

    Ok(CodeActionOrCommand::CodeAction(types::CodeAction {
        title: format!("{DIAGNOSTIC_NAME}: Fix all auto-fixable problems"),
        kind: Some(crate::NOTEBOOK_SOURCE_FIX_ALL_RUFF),
        edit,
        data,
        ..Default::default()
    }))
}

fn organize_imports(snapshot: &DocumentSnapshot) -> crate::Result<CodeActionOrCommand> {
    let document = snapshot.query();

    let (edit, data) = if snapshot
        .resolved_client_capabilities()
        .code_action_deferred_edit_resolution
    {
        // The edit will be resolved later in the `CodeActionsResolve` request
        (
            None,
            Some(
                serde_json::to_value(snapshot.query().make_key().into_url())
                    .expect("document url should serialize"),
            ),
        )
    } else {
        (
            Some(resolve_edit_for_organize_imports(
                document,
                snapshot.resolved_client_capabilities(),
                snapshot.encoding(),
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

fn notebook_organize_imports(snapshot: &DocumentSnapshot) -> crate::Result<CodeActionOrCommand> {
    let document = snapshot.query();

    let (edit, data) = if snapshot
        .resolved_client_capabilities()
        .code_action_deferred_edit_resolution
    {
        // The edit will be resolved later in the `CodeActionsResolve` request
        (
            None,
            Some(
                serde_json::to_value(snapshot.query().make_key().into_url())
                    .expect("document url should serialize"),
            ),
        )
    } else {
        (
            Some(resolve_edit_for_organize_imports(
                document,
                snapshot.resolved_client_capabilities(),
                snapshot.encoding(),
            )?),
            None,
        )
    };

    Ok(CodeActionOrCommand::CodeAction(types::CodeAction {
        title: format!("{DIAGNOSTIC_NAME}: Organize imports"),
        kind: Some(crate::NOTEBOOK_SOURCE_ORGANIZE_IMPORTS_RUFF),
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

#[cfg(test)]
mod tests {
    use lsp_types::{ClientCapabilities, Url};

    use crate::server::api::traits::BackgroundDocumentRequestHandler;
    use crate::session::{Client, GlobalOptions};
    use crate::{PositionEncoding, TextDocument, Workspace, Workspaces};

    use super::*;

    fn create_session_and_snapshot(
        file_name: &str,
        language_id: &str,
        content: &str,
    ) -> (crate::Session, Url) {
        let (main_loop_sender, _) = crossbeam::channel::unbounded();
        let (client_sender, _) = crossbeam::channel::unbounded();
        let client = Client::new(main_loop_sender, client_sender);

        let workspace_dir = std::env::temp_dir();
        let workspace_url = Url::from_file_path(&workspace_dir).unwrap();

        let options = GlobalOptions::default();
        let global = options.into_settings(client.clone());

        let mut session = crate::Session::new(
            &ClientCapabilities::default(),
            PositionEncoding::UTF16,
            global,
            &Workspaces::new(vec![
                Workspace::new(workspace_url).with_options(crate::ClientOptions::default()),
            ]),
            &client,
        )
        .unwrap();

        let file_url = Url::from_file_path(workspace_dir.join(file_name)).unwrap();
        let document =
            TextDocument::new(content.to_string(), 0).with_language_id(language_id);
        session.open_text_document(file_url.clone(), document);

        (session, file_url)
    }

    fn empty_code_action_params(url: Url) -> types::CodeActionParams {
        types::CodeActionParams {
            text_document: types::TextDocumentIdentifier { uri: url },
            range: types::Range::default(),
            context: types::CodeActionContext {
                diagnostics: vec![],
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: types::WorkDoneProgressParams::default(),
            partial_result_params: types::PartialResultParams::default(),
        }
    }

    #[test]
    fn no_code_actions_for_markdown() {
        let (session, file_url) = create_session_and_snapshot("test.md", "markdown", "# Hello");

        let snapshot = session.take_snapshot(file_url.clone()).unwrap();

        let (main_loop_sender, _) = crossbeam::channel::unbounded();
        let (client_sender, _) = crossbeam::channel::unbounded();
        let client = Client::new(main_loop_sender, client_sender);

        let result =
            CodeActions::run_with_snapshot(snapshot, &client, empty_code_action_params(file_url))
                .unwrap();

        let actions = result.expect("Expected Some response");
        assert!(
            actions.is_empty(),
            "Expected no code actions for markdown file, got: {actions:?}"
        );
    }

    #[test]
    fn code_actions_for_python() {
        let (session, file_url) =
            create_session_and_snapshot("test.py", "python", "import os\n");

        let snapshot = session.take_snapshot(file_url.clone()).unwrap();

        let (main_loop_sender, _) = crossbeam::channel::unbounded();
        let (client_sender, _) = crossbeam::channel::unbounded();
        let client = Client::new(main_loop_sender, client_sender);

        let result =
            CodeActions::run_with_snapshot(snapshot, &client, empty_code_action_params(file_url))
                .unwrap();

        let actions = result.expect("Expected Some response");
        assert!(
            !actions.is_empty(),
            "Expected code actions for Python file, got none"
        );
    }
}
