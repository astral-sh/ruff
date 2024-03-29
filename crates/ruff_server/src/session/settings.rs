use lsp_types::ClientCapabilities;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ResolvedClientCapabilities {
    pub(crate) code_action_deferred_edit_resolution: bool,
}

impl ResolvedClientCapabilities {
    pub(super) fn new(client_capabilities: &ClientCapabilities) -> Self {
        Self {
            code_action_deferred_edit_resolution: client_capabilities
                .text_document
                .as_ref()
                .and_then(|doc_settings| doc_settings.code_action.as_ref())
                .and_then(|code_action_settings| code_action_settings.resolve_support.as_ref())
                .is_some_and(|resolve_support| resolve_support.properties.contains(&"edit".into())),
        }
    }
}
