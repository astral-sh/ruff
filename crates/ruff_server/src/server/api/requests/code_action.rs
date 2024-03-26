use crate::edit::{DocumentVersion, ToRangeExt};
use crate::server::api::LSPResult;
use crate::server::{client::Notifier, Result};
use crate::session::{DocumentRef, DocumentSnapshot};
use crate::DIAGNOSTIC_NAME;
use crate::{PositionEncoding, SOURCE_FIX_ALL_RUFF, SOURCE_ORGANIZE_IMPORTS_RUFF};
use lsp_types::{self as types, request as req};
use ruff_diagnostics::Applicability;
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

/// Describes a fix for `fixed_diagnostic` that applies `document_edits` to the source.
#[derive(Clone, Debug)]
struct DiagnosticFix {
    fixed_diagnostic: types::Diagnostic,
    title: String,
    code: String,
    applicability: Applicability,
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
        let fixes = fixes_for_diagnostics(
            document,
            snapshot.url(),
            snapshot.encoding(),
            document.version(),
            params.context.diagnostics,
        )
        .collect::<Result<Vec<_>>>()?;

        let mut response: types::CodeActionResponse = types::CodeActionResponse::default();

        if available_actions.contains(AvailableCodeActions::QUICK_FIX) {
            response.extend(quick_fix(fixes.as_slice()));
        }

        if available_actions.contains(AvailableCodeActions::SOURCE_FIX_ALL) {
            response.extend(fix_all(fixes.as_slice()));
        }

        if available_actions.contains(AvailableCodeActions::SOURCE_ORGANIZE_IMPORTS) {
            todo!("Implement the `source.organizeImports` code action");
        }

        Ok(Some(response))
    }
}

fn fixes_for_diagnostics<'d>(
    document: &'d DocumentRef,
    url: &'d Url,
    encoding: PositionEncoding,
    version: DocumentVersion,
    diagnostics: Vec<types::Diagnostic>,
) -> impl Iterator<Item = crate::server::Result<DiagnosticFix>> + 'd {
    diagnostics
        .into_iter()
        .map(move |mut diagnostic| {
            let Some(data) = diagnostic.data.take() else {
                return Ok(None);
            };
            let fixed_diagnostic = diagnostic;
            let associated_data: crate::lint::AssociatedDiagnosticData =
                serde_json::from_value(data)
                    .map_err(|err| anyhow::anyhow!("failed to deserialize diagnostic data: {err}"))
                    .with_failure_code(lsp_server::ErrorCode::ParseError)?;
            let edits = associated_data
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
            Ok(Some(DiagnosticFix {
                fixed_diagnostic,
                applicability: associated_data.fix.applicability(),
                code: associated_data.code,
                title: associated_data
                    .kind
                    .suggestion
                    .unwrap_or(associated_data.kind.name),
                document_edits,
            }))
        })
        .filter_map(Result::transpose)
}

fn quick_fix(fixes: &[DiagnosticFix]) -> impl Iterator<Item = CodeActionOrCommand> + '_ {
    fixes.iter().map(|fix| {
        types::CodeActionOrCommand::CodeAction(types::CodeAction {
            title: format!("{DIAGNOSTIC_NAME} ({}): {}", fix.code, fix.title),
            kind: Some(types::CodeActionKind::QUICKFIX),
            edit: Some(types::WorkspaceEdit {
                document_changes: Some(types::DocumentChanges::Edits(fix.document_edits.clone())),
                ..Default::default()
            }),
            diagnostics: Some(vec![fix.fixed_diagnostic.clone()]),
            ..Default::default()
        })
    })
}

fn fix_all(fixes: &[DiagnosticFix]) -> Option<CodeActionOrCommand> {
    let edits_made: Vec<_> = fixes
        .iter()
        .filter(|fix| fix.applicability.is_safe())
        .collect();

    if edits_made.is_empty() {
        return None;
    }

    let diagnostics_fixed = edits_made
        .iter()
        .map(|fix| fix.fixed_diagnostic.clone())
        .collect();

    // TODO: return vec with additional `applyAutofix` command.
    Some(types::CodeActionOrCommand::CodeAction(types::CodeAction {
        title: format!("{DIAGNOSTIC_NAME}: Fix all auto-fixable problems"),
        diagnostics: Some(diagnostics_fixed),
        kind: Some(types::CodeActionKind::SOURCE_FIX_ALL),
        edit: Some(types::WorkspaceEdit {
            document_changes: Some(types::DocumentChanges::Edits(
                edits_made
                    .into_iter()
                    .flat_map(|fixes| fixes.document_edits.iter())
                    .cloned()
                    .collect(),
            )),
            ..Default::default()
        }),
        ..Default::default()
    }))
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
