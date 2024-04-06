use lsp_types::ClientCapabilities;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ResolvedClientCapabilities {
    pub(crate) code_action_deferred_edit_resolution: bool,
    pub(crate) apply_edit: bool,
    pub(crate) document_changes: bool,
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
            .and_then(|workspace| workspace.workspace_edit.as_ref())
            .and_then(|workspace_edit| workspace_edit.document_changes)
            .unwrap_or_default();

        Self {
            code_action_deferred_edit_resolution: code_action_data_support
                && code_action_edit_resolution,
            apply_edit,
            document_changes,
        }
    }
}
