use lsp_types::{ClientCapabilities, MarkupKind};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[expect(clippy::struct_excessive_bools)]
pub(crate) struct ResolvedClientCapabilities {
    pub(crate) code_action_deferred_edit_resolution: bool,
    pub(crate) apply_edit: bool,
    pub(crate) document_changes: bool,
    pub(crate) diagnostics_refresh: bool,
    pub(crate) inlay_refresh: bool,
    pub(crate) pull_diagnostics: bool,
    /// Whether `textDocument.typeDefinition.linkSupport` is `true`
    pub(crate) type_definition_link_support: bool,

    /// `true`, if the first markup kind in `textDocument.hover.contentFormat` is `Markdown`
    pub(crate) hover_prefer_markdown: bool,
}

impl ResolvedClientCapabilities {
    pub(super) fn new(client_capabilities: &ClientCapabilities) -> Self {
        let code_action_settings = client_capabilities
            .text_document
            .as_ref()
            .and_then(|doc_settings| doc_settings.code_action.as_ref());
        let code_action_data_support = code_action_settings
            .and_then(|code_action_settings| code_action_settings.data_support)
            .unwrap_or_default();
        let code_action_edit_resolution = code_action_settings
            .and_then(|code_action_settings| code_action_settings.resolve_support.as_ref())
            .is_some_and(|resolve_support| resolve_support.properties.contains(&"edit".into()));

        let apply_edit = client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.apply_edit)
            .unwrap_or_default();

        let document_changes = client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.workspace_edit.as_ref()?.document_changes)
            .unwrap_or_default();

        let declaration_link_support = client_capabilities
            .text_document
            .as_ref()
            .and_then(|document| document.type_definition?.link_support)
            .unwrap_or_default();

        let diagnostics_refresh = client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.diagnostics.as_ref()?.refresh_support)
            .unwrap_or_default();

        let inlay_refresh = client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.inlay_hint.as_ref()?.refresh_support)
            .unwrap_or_default();

        let pull_diagnostics = client_capabilities
            .text_document
            .as_ref()
            .and_then(|text_document| text_document.diagnostic.as_ref())
            .is_some();

        let hover_prefer_markdown = client_capabilities
            .text_document
            .as_ref()
            .and_then(|text_document| {
                Some(
                    text_document
                        .hover
                        .as_ref()?
                        .content_format
                        .as_ref()?
                        .contains(&MarkupKind::Markdown),
                )
            })
            .unwrap_or_default();

        Self {
            code_action_deferred_edit_resolution: code_action_data_support
                && code_action_edit_resolution,
            apply_edit,
            document_changes,
            diagnostics_refresh,
            inlay_refresh,
            pull_diagnostics,
            type_definition_link_support: declaration_link_support,
            hover_prefer_markdown,
        }
    }
}
