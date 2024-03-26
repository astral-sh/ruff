use crate::edit::{DocumentVersion, ToRangeExt};
use crate::lint::DiagnosticFix;
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::{DocumentRef, DocumentSnapshot};
use crate::DIAGNOSTIC_NAME;
use crate::{PositionEncoding, SOURCE_FIX_ALL_RUFF, SOURCE_ORGANIZE_IMPORTS_RUFF};
use lsp_types::{self as types, request as req};
use ruff_text_size::Ranged;
use types::{CodeActionKind, CodeActionOrCommand, Url};

bitflags::bitflags! {
    struct AvailableCodeActions: u8 {
        const QUICK_FIX = 0b0000_0001;
        const SOURCE_FIX_ALL = 0b0000_0010;
        const SOURCE_ORGANIZE_IMPORTS = 0b0000_0100;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SupportedCodeActionKind {
    QuickFix,
    SourceFixAll,
    SourceFixAllRuff,
    #[allow(dead_code)] // TODO: remove
    SourceOrganizeImports,
    #[allow(dead_code)] // TODO: remove
    SourceOrganizeImportsRuff,
}

#[derive(Clone, Debug)]
struct DiagnosticEdit {
    original_diagnostic: types::Diagnostic,
    diagnostic_fix: DiagnosticFix,
    document_edits: Vec<types::TextDocumentEdit>,
}

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
        let document = snapshot.document();
        // compute the associated document edits for each diagnostic
        // these will get re-used when building the actual code actions afterwards
        let edits = diagnostic_edits(
            document,
            snapshot.url(),
            snapshot.encoding(),
            document.version(),
            params.context.diagnostics,
        )
        .collect::<Result<Vec<_>>>()?;

        let mut response: types::CodeActionResponse = types::CodeActionResponse::default();

        if available_actions.contains(AvailableCodeActions::QUICK_FIX) {
            response.extend(quick_fix(edits.as_slice()));
        }

        if available_actions.contains(AvailableCodeActions::SOURCE_FIX_ALL) {
            response.extend(fix_all(edits.as_slice()));
        }

        if available_actions.contains(AvailableCodeActions::SOURCE_ORGANIZE_IMPORTS) {
            todo!("Implement the `source.organizeImports` code action");
        }

        Ok(Some(response))
    }
}

fn diagnostic_edits<'d>(
    document: &'d DocumentRef,
    url: &'d Url,
    encoding: PositionEncoding,
    version: DocumentVersion,
    diagnostics: Vec<types::Diagnostic>,
) -> impl Iterator<Item = crate::server::Result<DiagnosticEdit>> + 'd {
    diagnostics
        .into_iter()
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

fn quick_fix(edits: &[DiagnosticEdit]) -> impl Iterator<Item = CodeActionOrCommand> + '_ {
    edits.iter().map(|edit| {
        let code = &edit.diagnostic_fix.code;
        let title = edit
            .diagnostic_fix
            .kind
            .suggestion
            .as_deref()
            .unwrap_or(&edit.diagnostic_fix.kind.name);
        types::CodeActionOrCommand::CodeAction(types::CodeAction {
            title: format!("{DIAGNOSTIC_NAME} ({code}): {title}"),
            kind: Some(types::CodeActionKind::QUICKFIX),
            edit: Some(types::WorkspaceEdit {
                document_changes: Some(types::DocumentChanges::Edits(edit.document_edits.clone())),
                ..Default::default()
            }),
            ..Default::default()
        })
    })
}

impl SupportedCodeActionKind {
    fn kind(self) -> CodeActionKind {
        match self {
            Self::QuickFix => CodeActionKind::QUICKFIX,
            Self::SourceFixAll => CodeActionKind::SOURCE_FIX_ALL,
            Self::SourceFixAllRuff => SOURCE_FIX_ALL_RUFF,
            Self::SourceOrganizeImports => CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
            Self::SourceOrganizeImportsRuff => SOURCE_ORGANIZE_IMPORTS_RUFF,
        }
    }

    fn makes_available(self) -> AvailableCodeActions {
        match self {
            Self::QuickFix => AvailableCodeActions::QUICK_FIX,
            Self::SourceFixAll | Self::SourceFixAllRuff => AvailableCodeActions::SOURCE_FIX_ALL,
            Self::SourceOrganizeImports | Self::SourceOrganizeImportsRuff => {
                AvailableCodeActions::SOURCE_ORGANIZE_IMPORTS
            }
        }
    }

    fn all() -> impl Iterator<Item = Self> {
        [
            Self::QuickFix,
            Self::SourceFixAll,
            Self::SourceFixAllRuff,
            // Self::SourceOrganizeImports,
            // Self::SourceOrganizeImportsRuff
        ]
        .into_iter()
    }
}

fn fix_all(edits: &[DiagnosticEdit]) -> Option<CodeActionOrCommand> {
    let edits_made: Vec<_> = edits
        .iter()
        .filter(|edit| {
            edit.diagnostic_fix
                .fix
                .applies(ruff_diagnostics::Applicability::Safe)
        })
        .collect();

    if edits_made.is_empty() {
        return None;
    }

    let diagnostics_fixed = edits_made
        .iter()
        .map(|edit| edit.original_diagnostic.clone())
        .collect();

    // TODO: return vec with `applyAutofix` command.
    Some(types::CodeActionOrCommand::CodeAction(types::CodeAction {
        title: format!("{DIAGNOSTIC_NAME}: Fix all auto-fixable problems"),
        diagnostics: Some(diagnostics_fixed),
        kind: Some(types::CodeActionKind::SOURCE_FIX_ALL),
        edit: Some(types::WorkspaceEdit {
            document_changes: Some(types::DocumentChanges::Edits(
                edits_made
                    .into_iter()
                    .flat_map(|edit| edit.document_edits.iter())
                    .cloned()
                    .collect(),
            )),
            ..Default::default()
        }),
        ..Default::default()
    }))
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
