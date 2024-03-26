use std::collections::BTreeSet;

use crate::edit::{DocumentVersion, ToRangeExt};
use crate::lint::DiagnosticFix;
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::{DocumentRef, DocumentSnapshot};
use crate::PositionEncoding;
use crate::DIAGNOSTIC_NAME;
use lsp_types::{self as types, request as req};
use ruff_text_size::Ranged;
use types::{CodeActionKind, CodeActionOrCommand, Url};

pub(crate) struct CodeAction;

impl super::RequestHandler for CodeAction {
    type RequestType = req::CodeActionRequest;
}

// The order for the variants here determines the order we
// add their results to the code action response
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SupportedCodeAction {
    QuickFix,
    FixAll,
    #[allow(dead_code)] // TODO: remove
    OrganizeImports,
}

#[derive(Clone, Debug)]
struct DiagnosticEdit {
    original_diagnostic: types::Diagnostic,
    diagnostic_fix: DiagnosticFix,
    document_edits: Vec<types::TextDocumentEdit>,
}

impl super::BackgroundDocumentRequestHandler for CodeAction {
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
        let document = snapshot.document();
        // compute the associated document edits for each diagnostic
        // these will get re-used when building the actual code actions afterwards
        let edits = diagnostic_edits(
            document,
            snapshot.url(),
            snapshot.encoding(),
            document.version(),
            params.context.diagnostics.into_iter(),
        )
        .collect::<Result<Vec<_>>>()?;

        let mut response: types::CodeActionResponse = types::CodeActionResponse::default();

        for action in available_actions {
            match action {
                SupportedCodeAction::QuickFix => response.extend(quick_fix(edits.iter())),
                SupportedCodeAction::FixAll => response.extend(fix_all(edits.iter())),
                SupportedCodeAction::OrganizeImports => {
                    todo!("Implement the `source.organizeImports` code action")
                }
            }
        }

        Ok(Some(response))
    }
}

fn diagnostic_edits<'d>(
    document: &'d DocumentRef,
    url: &'d Url,
    encoding: PositionEncoding,
    version: DocumentVersion,
    diagnostics: impl Iterator<Item = types::Diagnostic> + 'd,
) -> impl Iterator<Item = crate::server::Result<DiagnosticEdit>> + 'd {
    diagnostics
        .map(move |diagnostic| {
            let Some(data) = diagnostic.data.clone() else {
                return Ok(None);
            };
            let diagnostic_fix: crate::lint::DiagnosticFix = serde_json::from_value(data)
                .map_err(|err| anyhow::anyhow!("failed to deserialize diagnostic data: {err}"))
                .with_failure_code(lsp_server::ErrorCode::ParseError)?;
            let edits = diagnostic_fix
                .fix
                .edits()
                .iter()
                .map(|edit| types::TextEdit {
                    range: edit
                        .range()
                        .to_range(document.contents(), document.index(), encoding),
                    new_text: edit.content().unwrap_or_default().to_string(),
                });

            let document_edits = vec![types::TextDocumentEdit {
                text_document: types::OptionalVersionedTextDocumentIdentifier::new(
                    url.clone(),
                    version,
                ),
                edits: edits.map(types::OneOf::Left).collect(),
            }];
            Ok(Some(DiagnosticEdit {
                original_diagnostic: diagnostic,
                diagnostic_fix,
                document_edits,
            }))
        })
        .filter_map(Result::transpose)
}

fn quick_fix<'d>(
    edits: impl Iterator<Item = &'d DiagnosticEdit> + 'd,
) -> impl Iterator<Item = CodeActionOrCommand> + 'd {
    edits.map(|edit| {
        let title = edit
            .diagnostic_fix
            .kind
            .suggestion
            .clone()
            .unwrap_or(edit.diagnostic_fix.kind.name.clone());
        types::CodeActionOrCommand::CodeAction(types::CodeAction {
            title: format!("{DIAGNOSTIC_NAME}: {title}"),
            kind: Some(types::CodeActionKind::QUICKFIX),
            edit: Some(types::WorkspaceEdit {
                document_changes: Some(types::DocumentChanges::Edits(edit.document_edits.clone())),
                ..Default::default()
            }),
            ..Default::default()
        })
    })
}

impl SupportedCodeAction {
    fn kind(self) -> CodeActionKind {
        match self {
            Self::QuickFix => CodeActionKind::QUICKFIX,
            Self::FixAll => CodeActionKind::SOURCE_FIX_ALL,
            Self::OrganizeImports => CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
        }
    }
}

fn fix_all<'d>(
    edits: impl Iterator<Item = &'d DiagnosticEdit> + 'd,
) -> impl Iterator<Item = CodeActionOrCommand> + 'd {
    let edits_made: Vec<_> = edits
        .filter(|edit| {
            edit.diagnostic_fix
                .fix
                .applies(ruff_diagnostics::Applicability::Safe)
        })
        .collect();
    let diagnostics_fixed = edits_made
        .iter()
        .map(|edit| edit.original_diagnostic.clone())
        .collect();

    (!edits_made.is_empty())
        .then(move || {
            edits_made
                .into_iter()
                .flat_map(|edit| edit.document_edits.iter())
        })
        .map(|changes| {
            vec![
                types::CodeActionOrCommand::CodeAction(types::CodeAction {
                    title: format!("{DIAGNOSTIC_NAME}: Fix all auto-fixable problems"),
                    diagnostics: Some(diagnostics_fixed),
                    kind: Some(types::CodeActionKind::SOURCE_FIX_ALL),
                    edit: Some(types::WorkspaceEdit {
                        document_changes: Some(types::DocumentChanges::Edits(
                            changes.cloned().collect(),
                        )),
                        ..Default::default()
                    }),
                    ..Default::default()
                }), // TODO: implement command handler for the server
                    /*
                    types::CodeActionOrCommand::Command(types::Command {
                        ...
                    }
                     */
            ]
        })
        .into_iter()
        .flatten()
}

/// If `action_filter` is `None`, this returns the full list of supported code actions. Otherwise,
/// the list is filtered.
fn available_code_actions(
    action_filter: Option<Vec<CodeActionKind>>,
) -> BTreeSet<SupportedCodeAction> {
    const DEFAULT_ACTIONS: &[SupportedCodeAction] = &[
        SupportedCodeAction::QuickFix,
        SupportedCodeAction::FixAll,
        // SupportedCodeAction::OrganizeImports
    ];

    let Some(action_filter) = action_filter else {
        return DEFAULT_ACTIONS.iter().copied().collect();
    };

    DEFAULT_ACTIONS
        .iter()
        .filter(|action| {
            action_filter
                .iter()
                .any(|kind| action.kind().as_str().starts_with(kind.as_str()))
        })
        .copied()
        .collect()
}
