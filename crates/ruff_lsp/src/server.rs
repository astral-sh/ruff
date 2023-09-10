use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Debug, Display};

use anyhow::anyhow;
use serde_json::json;
use tokio::task::spawn_blocking;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result as LspResult};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOptions, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, DidChangeTextDocumentParams,
    DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentFormattingParams,
    GeneralClientCapabilities, GlobPattern, InitializeParams, InitializeResult, InitializedParams,
    MessageType, OneOf, PositionEncodingKind, Registration, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, WorkspaceEdit, WorkspaceFolder,
    WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
};
use tower_lsp::{Client, LanguageServer};

use ruff_diagnostics::Applicability;
use ruff_linter::RUFF_PKG_VERSION;
use ruff_python_ast::PySourceType;
use ruff_python_formatter::{format_module_source, FormatModuleError};
use ruff_text_size::Ranged;

use crate::diagnostic::DiagnosticData;
use crate::document::Document;
use crate::encoding::{text_diff_to_edits, text_range_to_range, PositionEncoding};
use crate::session::Session;

pub(crate) struct Server {
    session: Session,
}

impl Server {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            session: Session::new(client),
        }
    }

    async fn log_if_err<E>(&self, message: &str, result: Result<(), E>)
    where
        E: std::fmt::Display,
    {
        if let Err(error) = result {
            self.log_err(message, error).await;
        }
    }

    async fn log_err<E>(&self, message: &str, error: E)
    where
        E: std::fmt::Display,
    {
        tracing::error!("LSP Operation failed: {error}");

        self.session
            .client()
            .log_message(MessageType::ERROR, message)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Server {
    #[tracing::instrument(level="debug", skip_all, err, fields(client=?params.client_info))]
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        let position_encoding = negotiate_encoding(params.capabilities.general.as_ref());

        self.session.set_position_encoding(position_encoding);

        let mut workspaces = params.workspace_folders.unwrap_or_default();

        if let Some(root_uri) = params.root_uri {
            if let Some(first_workplace) = workspaces.first() {
                if first_workplace.uri != root_uri {
                    workspaces.push(WorkspaceFolder {
                        uri: root_uri,
                        name: "ROOT".to_string(),
                    });
                }
            }
        }

        if let Err(error) = self.session.update_workspaces(workspaces, Vec::new()).await {
            tracing::error!("Failed to update workspaces: {error}.");
        }

        let mut configuration = self.session.configuration_mut().await;

        configuration.supports_change_watched_files = params
            .capabilities
            .workspace
            .unwrap_or_default()
            .did_change_watched_files
            .unwrap_or_default()
            .dynamic_registration
            .unwrap_or_default();

        let init = InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(position_encoding.into()),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                document_formatting_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            CodeActionKind::QUICKFIX,
                            CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                        ]),
                        ..CodeActionOptions::default()
                    },
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: String::from(env!("CARGO_PKG_NAME")),
                version: Some(RUFF_PKG_VERSION.to_string()),
            }),
        };

        Ok(init)
    }

    #[tracing::instrument(skip_all)]
    async fn initialized(&self, _params: InitializedParams) {
        if self
            .session
            .configuration()
            .await
            .supports_change_watched_files
        {
            self.log_if_err(
                "Failed to set-up configuration watcher",
                self.session
                    .client()
                    .register_capability(vec![Registration {
                        id: "ruff_settings_watcher".to_string(),
                        method: "workspace/didChangeWatchedFiles".to_string(),
                        register_options: Some(json!(
                            tower_lsp::lsp_types::DidChangeWatchedFilesRegistrationOptions {
                                watchers: vec![tower_lsp::lsp_types::FileSystemWatcher {
                                    glob_pattern: GlobPattern::String(
                                        "**/{ruff,pyproject}.toml".to_string()
                                    ),
                                    kind: Some(tower_lsp::lsp_types::WatchKind::all()),
                                }],
                            }
                        )),
                    }])
                    .await,
            )
            .await;
        }
    }

    #[tracing::instrument(skip_all, fields(file=%params.text_document.uri))]
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let text_document = params.text_document;

        let document = Document::new(text_document.text, text_document.version);

        self.session
            .insert_document(text_document.uri.clone(), document.clone())
            .await;

        self.log_if_err(
            "Failed to pull diagnostics",
            self.session
                .update_diagnostics(text_document.uri, &document)
                .await,
        )
        .await;
    }

    #[tracing::instrument(skip_all, fields(file=%params.text_document.uri))]
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();

        let updated = match self.session.update_document(params).await {
            Ok(updated) => updated,
            Err(err) => {
                self.log_err("Failed to update the file content.", err)
                    .await;
                return;
            }
        };

        self.log_if_err(
            "Failed to update diagnostics",
            self.session.update_diagnostics(uri, &updated).await,
        )
        .await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let Some(document) = self.session.document(&params.text_document.uri).await else {
            tracing::error!("Missing document {}", params.text_document.uri);
            return;
        };

        self.log_if_err(
            "Failed to update diagnostics",
            self.session
                .update_diagnostics(params.text_document.uri, &document)
                .await,
        )
        .await;
    }

    #[tracing::instrument(skip_all, fields(file=%params.text_document.uri))]
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if let Some(document) = self
            .session
            .remove_document(&params.text_document.uri)
            .await
        {
            // Clear existing diagnostics as it is recommended by the LSP specification for tools
            // that operate on a per file level (rather than project).
            self.session
                .client()
                .publish_diagnostics(
                    params.text_document.uri,
                    Vec::new(),
                    Some(document.version()),
                )
                .await;
        }
    }

    #[tracing::instrument(skip_all, fields(file=%params.text_document.uri))]
    async fn formatting(
        &self,
        params: DocumentFormattingParams,
    ) -> LspResult<Option<Vec<TextEdit>>> {
        let document = self
            .session
            .document(&params.text_document.uri)
            .await
            .ok_or_else(|| Error::new(ErrorCode::InternalError))?;

        let path = self.session.file_path(&params.text_document.uri);

        let configuration = self
            .session
            .resolve_configuration(&path)
            .await
            .map_err(into_lsp_error)?;

        let encoding = self.session.position_encoding();

        spawn_blocking(move || {
            let settings = configuration.settings(&path);
            let options = settings
                .formatter
                .to_format_options(PySourceType::Python, document.text());

            // TODO this should call into a more high level API that detects the configuration for the file
            // and formats it.

            let formatted = match format_module_source(document.text(), options) {
                Ok(formatted) => formatted,
                // TODO explicit error logging may not be necessary when `format_module` logs errors
                Err(FormatModuleError::ParseError(parse_error)) => {
                    tracing::debug!(
                        "Failed to format file because of parsing error {parse_error:?}"
                    );
                    return Ok(None);
                }
                Err(FormatModuleError::LexError(lex_error)) => {
                    tracing::debug!(
                        "Failed to format file because of a lexing error {lex_error:?}"
                    );
                    return Ok(None);
                }
                Err(err) => {
                    tracing::error!("Formatting failed with error {err:?}.");
                    return Err(err);
                }
            };

            let code = formatted.into_code();

            if code == document.text() {
                Ok(None)
            } else {
                let diff = similar::TextDiff::from_lines(document.text(), &code);
                let edits = text_diff_to_edits(&diff, &document, encoding);

                Ok(Some(edits))
            }
        })
        .await
        .map_err(into_lsp_error)?
        .map_err(into_lsp_error)
    }

    #[tracing::instrument(skip_all, err)]
    async fn code_action(&self, params: CodeActionParams) -> LspResult<Option<CodeActionResponse>> {
        let document = self
            .session
            .document(&params.text_document.uri)
            .await
            .ok_or_else(|| {
                into_lsp_error(anyhow!("Missing document {:?}", params.text_document.uri))
            })?;

        let mut code_actions = Vec::new();
        let encoding = self.session.position_encoding();

        for diagnostic in params.context.diagnostics {
            if diagnostic.source.as_deref() != Some("ruff") {
                continue;
            }

            let Some(data) = diagnostic.data.clone() else {
                continue;
            };

            let data = match serde_json::from_value::<DiagnosticData>(data) {
                Ok(data) => data,
                Err(error) => {
                    self.log_err("Failed to convert diagnostic", error).await;
                    continue;
                }
            };

            if data.fix.applicability() == Applicability::Manual || data.fix.edits().is_empty() {
                continue;
            }

            let title = if let Some(suggestion) = data.suggestion {
                suggestion
            } else {
                "FIX TODO".to_string()
            };

            let mut edits = Vec::new();

            for edit in data.fix.edits() {
                edits.push(TextEdit {
                    range: text_range_to_range(edit.range(), &document, encoding),
                    new_text: edit.content().unwrap_or_default().to_owned(),
                });
            }

            code_actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title,
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic]),
                edit: Some(WorkspaceEdit {
                    changes: Some(HashMap::from_iter([(
                        params.text_document.uri.clone(),
                        edits,
                    )])),
                    ..WorkspaceEdit::default()
                }),
                command: None,
                is_preferred: Some(true),
                disabled: None,
                data: None,
            }));
        }

        Ok(Some(code_actions))
    }

    #[tracing::instrument(skip_all)]
    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        self.log_if_err(
            "Failed to update workspace folders",
            self.session
                .update_workspaces(params.event.added, params.event.removed)
                .await,
        )
        .await;
    }

    #[tracing::instrument(skip_all)]
    async fn did_change_watched_files(&self, _params: DidChangeWatchedFilesParams) {
        self.log_if_err(
            "Failed to updated configurations",
            self.session.update_workspaces(Vec::new(), Vec::new()).await,
        )
        .await;
    }

    #[tracing::instrument(skip_all, err)]
    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }
}

/// Picks the positional encoding depending on the provided client capabilities.
///
/// The preferred position encoding is UTF8 because it requires no conversion.
fn negotiate_encoding(capabilities: Option<&GeneralClientCapabilities>) -> PositionEncoding {
    let position_encodings = capabilities
        .and_then(|general| general.position_encodings.as_ref())
        .into_iter()
        .flatten();

    let mut seen_utf32 = false;

    for encoding in position_encodings {
        if encoding == &PositionEncodingKind::UTF8 {
            return PositionEncoding::UTF8;
        } else if encoding == &PositionEncodingKind::UTF32 {
            seen_utf32 = true;
        }
    }

    if seen_utf32 {
        PositionEncoding::UTF32
    } else {
        PositionEncoding::UTF16
    }
}

pub(crate) fn into_lsp_error(msg: impl Display + Debug) -> Error {
    let mut error = Error::internal_error();
    error.message = Cow::Owned(msg.to_string());
    error.data = Some(format!("{msg:?}").into());
    error
}
