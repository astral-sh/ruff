use lsp_types::{ClientCapabilities, MarkupKind};

bitflags::bitflags! {
    /// Represents the resolved client capabilities for the language server.
    ///
    /// This tracks various capabilities that the client supports.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub(crate) struct ResolvedClientCapabilities: u32 {
        const WORKSPACE_DIAGNOSTIC_REFRESH = 1 << 0;
        const INLAY_HINT_REFRESH = 1 << 1;
        const PULL_DIAGNOSTICS = 1 << 2;
        const TYPE_DEFINITION_LINK_SUPPORT = 1 << 3;
        const DEFINITION_LINK_SUPPORT = 1 << 4;
        const DECLARATION_LINK_SUPPORT = 1 << 5;
        const PREFER_MARKDOWN_IN_HOVER = 1 << 6;
        const MULTILINE_SEMANTIC_TOKENS = 1 << 7;
        const SIGNATURE_LABEL_OFFSET_SUPPORT = 1 << 8;
        const SIGNATURE_ACTIVE_PARAMETER_SUPPORT = 1 << 9;
        const HIERARCHICAL_DOCUMENT_SYMBOL_SUPPORT = 1 << 10;
        const WORK_DONE_PROGRESS = 1 << 11;
        const DID_CHANGE_WATCHED_FILES_DYNAMIC_REGISTRATION= 1 << 12;
    }
}

impl ResolvedClientCapabilities {
    /// Returns `true` if the client supports workspace diagnostic refresh.
    pub(crate) const fn supports_workspace_diagnostic_refresh(self) -> bool {
        self.contains(Self::WORKSPACE_DIAGNOSTIC_REFRESH)
    }

    /// Returns `true` if the client supports inlay hint refresh.
    pub(crate) const fn supports_inlay_hint_refresh(self) -> bool {
        self.contains(Self::INLAY_HINT_REFRESH)
    }

    /// Returns `true` if the client supports pull diagnostics.
    pub(crate) const fn supports_pull_diagnostics(self) -> bool {
        self.contains(Self::PULL_DIAGNOSTICS)
    }

    /// Returns `true` if the client supports definition links in goto type definition.
    pub(crate) const fn supports_type_definition_link(self) -> bool {
        self.contains(Self::TYPE_DEFINITION_LINK_SUPPORT)
    }

    /// Returns `true` if the client supports definition links in goto definition.
    pub(crate) const fn supports_definition_link(self) -> bool {
        self.contains(Self::DEFINITION_LINK_SUPPORT)
    }

    /// Returns `true` if the client supports definition links in goto declaration.
    pub(crate) const fn supports_declaration_link(self) -> bool {
        self.contains(Self::DECLARATION_LINK_SUPPORT)
    }

    /// Returns `true` if the client prefers markdown in hover responses.
    pub(crate) const fn prefers_markdown_in_hover(self) -> bool {
        self.contains(Self::PREFER_MARKDOWN_IN_HOVER)
    }

    /// Returns `true` if the client supports multiline semantic tokens.
    pub(crate) const fn supports_multiline_semantic_tokens(self) -> bool {
        self.contains(Self::MULTILINE_SEMANTIC_TOKENS)
    }

    /// Returns `true` if the client supports signature label offsets in signature help.
    pub(crate) const fn supports_signature_label_offset(self) -> bool {
        self.contains(Self::SIGNATURE_LABEL_OFFSET_SUPPORT)
    }

    /// Returns `true` if the client supports per-signature active parameter in signature help.
    pub(crate) const fn supports_signature_active_parameter(self) -> bool {
        self.contains(Self::SIGNATURE_ACTIVE_PARAMETER_SUPPORT)
    }

    /// Returns `true` if the client supports hierarchical document symbols.
    pub(crate) const fn supports_hierarchical_document_symbols(self) -> bool {
        self.contains(Self::HIERARCHICAL_DOCUMENT_SYMBOL_SUPPORT)
    }

    /// Returns `true` if the client supports work done progress.
    pub(crate) const fn supports_work_done_progress(self) -> bool {
        self.contains(Self::WORK_DONE_PROGRESS)
    }

    /// Returns `true` if the client supports dynamic registration for watched files changes.
    pub(crate) const fn supports_did_change_watched_files_dynamic_registration(self) -> bool {
        self.contains(Self::DID_CHANGE_WATCHED_FILES_DYNAMIC_REGISTRATION)
    }

    pub(super) fn new(client_capabilities: &ClientCapabilities) -> Self {
        let mut flags = Self::empty();

        let workspace = client_capabilities.workspace.as_ref();
        let text_document = client_capabilities.text_document.as_ref();

        if workspace
            .and_then(|workspace| workspace.diagnostics.as_ref()?.refresh_support)
            .unwrap_or_default()
        {
            flags |= Self::WORKSPACE_DIAGNOSTIC_REFRESH;
        }

        if workspace
            .and_then(|workspace| workspace.inlay_hint.as_ref()?.refresh_support)
            .unwrap_or_default()
        {
            flags |= Self::INLAY_HINT_REFRESH;
        }

        if text_document.is_some_and(|text_document| text_document.diagnostic.is_some()) {
            flags |= Self::PULL_DIAGNOSTICS;
        }

        if text_document
            .and_then(|text_document| text_document.type_definition?.link_support)
            .unwrap_or_default()
        {
            flags |= Self::TYPE_DEFINITION_LINK_SUPPORT;
        }

        if text_document
            .and_then(|text_document| text_document.definition?.link_support)
            .unwrap_or_default()
        {
            flags |= Self::DEFINITION_LINK_SUPPORT;
        }

        if text_document
            .and_then(|text_document| text_document.declaration?.link_support)
            .unwrap_or_default()
        {
            flags |= Self::DECLARATION_LINK_SUPPORT;
        }

        if text_document
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
            .unwrap_or_default()
        {
            flags |= Self::PREFER_MARKDOWN_IN_HOVER;
        }

        if text_document
            .and_then(|text_document| {
                text_document
                    .semantic_tokens
                    .as_ref()?
                    .multiline_token_support
            })
            .unwrap_or_default()
        {
            flags |= Self::MULTILINE_SEMANTIC_TOKENS;
        }

        if text_document
            .and_then(|text_document| {
                text_document
                    .signature_help
                    .as_ref()?
                    .signature_information
                    .as_ref()?
                    .parameter_information
                    .as_ref()?
                    .label_offset_support
            })
            .unwrap_or_default()
        {
            flags |= Self::SIGNATURE_LABEL_OFFSET_SUPPORT;
        }

        if text_document
            .and_then(|text_document| {
                text_document
                    .signature_help
                    .as_ref()?
                    .signature_information
                    .as_ref()?
                    .active_parameter_support
            })
            .unwrap_or_default()
        {
            flags |= Self::SIGNATURE_ACTIVE_PARAMETER_SUPPORT;
        }

        if text_document
            .and_then(|text_document| {
                text_document
                    .document_symbol
                    .as_ref()?
                    .hierarchical_document_symbol_support
            })
            .unwrap_or_default()
        {
            flags |= Self::HIERARCHICAL_DOCUMENT_SYMBOL_SUPPORT;
        }

        if client_capabilities
            .window
            .as_ref()
            .and_then(|window| window.work_done_progress)
            .unwrap_or_default()
        {
            flags |= Self::WORK_DONE_PROGRESS;
        }

        if client_capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.did_change_watched_files?.dynamic_registration)
            .unwrap_or_default()
        {
            flags |= Self::DID_CHANGE_WATCHED_FILES_DYNAMIC_REGISTRATION;
        }

        flags
    }
}
