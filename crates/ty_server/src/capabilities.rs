use lsp_types::{
    self as types, ClientCapabilities, CodeActionKind, CodeActionOptions, CompletionOptions,
    DeclarationCapability, DiagnosticOptions, DiagnosticServerCapabilities,
    HoverProviderCapability, InlayHintOptions, InlayHintServerCapabilities, MarkupKind,
    NotebookCellSelector, NotebookSelector, OneOf, RenameOptions, SelectionRangeProviderCapability,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensServerCapabilities, ServerCapabilities, SignatureHelpOptions,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TypeDefinitionProviderCapability, WorkDoneProgressOptions,
};
use std::str::FromStr;

use crate::PositionEncoding;

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
        const FILE_WATCHER_SUPPORT = 1 << 12;
        const RELATIVE_FILE_WATCHER_SUPPORT = 1 << 13;
        const DIAGNOSTIC_DYNAMIC_REGISTRATION = 1 << 14;
        const WORKSPACE_CONFIGURATION = 1 << 15;
        const COMPLETION_ITEM_LABEL_DETAILS_SUPPORT = 1 << 16;
        const DIAGNOSTIC_RELATED_INFORMATION = 1 << 17;
        const PREFER_MARKDOWN_IN_COMPLETION = 1 << 18;
    }
}

impl std::fmt::Display for ResolvedClientCapabilities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_list();
        for (name, _) in self.iter_names() {
            f.entry(&name);
        }
        f.finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SupportedCommand {
    Debug,
}

impl SupportedCommand {
    /// Returns the identifier of the command.
    const fn identifier(self) -> &'static str {
        match self {
            SupportedCommand::Debug => "ty.printDebugInformation",
        }
    }

    /// Returns all the commands that the server currently supports.
    const fn all() -> [SupportedCommand; 1] {
        [SupportedCommand::Debug]
    }
}

impl FromStr for SupportedCommand {
    type Err = anyhow::Error;

    fn from_str(name: &str) -> anyhow::Result<Self, Self::Err> {
        Ok(match name {
            "ty.printDebugInformation" => Self::Debug,
            _ => return Err(anyhow::anyhow!("Invalid command `{name}`")),
        })
    }
}

impl ResolvedClientCapabilities {
    /// Returns `true` if the client supports workspace diagnostic refresh.
    pub(crate) const fn supports_workspace_diagnostic_refresh(self) -> bool {
        self.contains(Self::WORKSPACE_DIAGNOSTIC_REFRESH)
    }

    /// Returns `true` if the client supports workspace configuration.
    pub(crate) const fn supports_workspace_configuration(self) -> bool {
        self.contains(Self::WORKSPACE_CONFIGURATION)
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

    /// Returns `true` if the client supports file watcher capabilities.
    pub(crate) const fn supports_file_watcher(self) -> bool {
        self.contains(Self::FILE_WATCHER_SUPPORT)
    }

    /// Returns `true` if the client supports relative file watcher capabilities.
    ///
    /// This permits specifying a "base uri" that a glob is interpreted
    /// relative to.
    pub(crate) const fn supports_relative_file_watcher(self) -> bool {
        self.contains(Self::RELATIVE_FILE_WATCHER_SUPPORT)
    }

    /// Returns `true` if the client supports dynamic registration for diagnostic capabilities.
    pub(crate) const fn supports_diagnostic_dynamic_registration(self) -> bool {
        self.contains(Self::DIAGNOSTIC_DYNAMIC_REGISTRATION)
    }

    /// Returns `true` if the client has related information support for diagnostics.
    pub(crate) const fn supports_diagnostic_related_information(self) -> bool {
        self.contains(Self::DIAGNOSTIC_RELATED_INFORMATION)
    }

    /// Returns `true` if the client supports "label details" in completion items.
    pub(crate) const fn supports_completion_item_label_details(self) -> bool {
        self.contains(Self::COMPLETION_ITEM_LABEL_DETAILS_SUPPORT)
    }

    /// Returns `true` if the client prefers Markdown over plain text in completion items.
    pub(crate) const fn prefers_markdown_in_completion(self) -> bool {
        self.contains(Self::PREFER_MARKDOWN_IN_COMPLETION)
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
            .and_then(|workspace| workspace.configuration)
            .unwrap_or_default()
        {
            flags |= Self::WORKSPACE_CONFIGURATION;
        }

        if workspace
            .and_then(|workspace| workspace.inlay_hint.as_ref()?.refresh_support)
            .unwrap_or_default()
        {
            flags |= Self::INLAY_HINT_REFRESH;
        }

        if let Some(capabilities) =
            workspace.and_then(|workspace| workspace.did_change_watched_files.as_ref())
        {
            if capabilities.dynamic_registration == Some(true) {
                flags |= Self::FILE_WATCHER_SUPPORT;
            }
            if capabilities.relative_pattern_support == Some(true) {
                flags |= Self::RELATIVE_FILE_WATCHER_SUPPORT;
            }
        }

        if let Some(diagnostic) =
            text_document.and_then(|text_document| text_document.diagnostic.as_ref())
        {
            flags |= Self::PULL_DIAGNOSTICS;

            if diagnostic.dynamic_registration == Some(true) {
                flags |= Self::DIAGNOSTIC_DYNAMIC_REGISTRATION;
            }
        }

        if let Some(publish_diagnostics) =
            text_document.and_then(|text_document| text_document.publish_diagnostics.as_ref())
        {
            if publish_diagnostics.related_information == Some(true) {
                flags |= Self::DIAGNOSTIC_RELATED_INFORMATION;
            }
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
                Some(
                    text_document
                        .completion
                        .as_ref()?
                        .completion_item
                        .as_ref()?
                        .documentation_format
                        .as_ref()?
                        .contains(&MarkupKind::Markdown),
                )
            })
            .unwrap_or_default()
        {
            flags |= Self::PREFER_MARKDOWN_IN_COMPLETION;
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

        if text_document
            .and_then(|text_document| text_document.completion.as_ref())
            .and_then(|completion| completion.completion_item.as_ref())
            .and_then(|completion_item| completion_item.label_details_support)
            .unwrap_or_default()
        {
            flags |= Self::COMPLETION_ITEM_LABEL_DETAILS_SUPPORT;
        }

        flags
    }
}

/// Creates the server capabilities based on the resolved client capabilities and resolved global
/// settings from the initialization options.
pub(crate) fn server_capabilities(
    position_encoding: PositionEncoding,
    resolved_client_capabilities: ResolvedClientCapabilities,
) -> ServerCapabilities {
    let diagnostic_provider =
        if resolved_client_capabilities.supports_diagnostic_dynamic_registration() {
            // If the client supports dynamic registration, we will register the diagnostic
            // capabilities dynamically based on the `ty.diagnosticMode` setting.
            None
        } else {
            // Otherwise, we always advertise support for workspace and pull diagnostics.
            Some(DiagnosticServerCapabilities::Options(
                server_diagnostic_options(true),
            ))
        };

    ServerCapabilities {
        position_encoding: Some(position_encoding.into()),
        code_action_provider: Some(types::CodeActionProviderCapability::Options(
            CodeActionOptions {
                code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                ..CodeActionOptions::default()
            },
        )),

        execute_command_provider: Some(types::ExecuteCommandOptions {
            commands: SupportedCommand::all()
                .map(|command| command.identifier().to_string())
                .to_vec(),
            work_done_progress_options: WorkDoneProgressOptions {
                work_done_progress: Some(false),
            },
        }),

        diagnostic_provider,
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::INCREMENTAL),
                ..Default::default()
            },
        )),
        type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        declaration_provider: Some(DeclarationCapability::Simple(true)),
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(server_rename_options())),
        document_highlight_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
            retrigger_characters: Some(vec![")".to_string()]),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
            InlayHintOptions::default(),
        ))),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                work_done_progress_options: WorkDoneProgressOptions::default(),
                legend: SemanticTokensLegend {
                    token_types: ty_ide::SemanticTokenType::all()
                        .iter()
                        .map(|token_type| token_type.as_lsp_concept().into())
                        .collect(),
                    token_modifiers: ty_ide::SemanticTokenModifier::all_names()
                        .iter()
                        .map(|&s| s.into())
                        .collect(),
                },
                range: Some(true),
                full: Some(SemanticTokensFullOptions::Bool(true)),
            },
        )),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!['.'.to_string()]),
            ..Default::default()
        }),
        selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        notebook_document_sync: Some(OneOf::Left(lsp_types::NotebookDocumentSyncOptions {
            save: Some(false),
            notebook_selector: [NotebookSelector::ByCells {
                notebook: None,
                cells: vec![NotebookCellSelector {
                    language: "python".to_string(),
                }],
            }]
            .to_vec(),
        })),
        ..Default::default()
    }
}

/// Creates the default [`DiagnosticOptions`] for the server.
pub(crate) fn server_diagnostic_options(workspace_diagnostics: bool) -> DiagnosticOptions {
    DiagnosticOptions {
        identifier: Some(crate::DIAGNOSTIC_NAME.to_string()),
        inter_file_dependencies: true,
        workspace_diagnostics,
        work_done_progress_options: WorkDoneProgressOptions {
            // Currently, the server only supports reporting work done progress for "workspace"
            // diagnostic mode.
            work_done_progress: Some(workspace_diagnostics),
        },
    }
}

pub(crate) fn server_rename_options() -> RenameOptions {
    RenameOptions {
        prepare_provider: Some(true),
        work_done_progress_options: WorkDoneProgressOptions::default(),
    }
}
