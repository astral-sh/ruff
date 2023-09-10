use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tokio::task::spawn_blocking;
use tower_lsp::lsp_types::{
    Diagnostic, DidChangeTextDocumentParams, Position, TextDocumentContentChangeEvent, Url,
    WorkspaceFolder,
};
use tower_lsp::Client;

use ruff_linter::linter::lint_only;
use ruff_linter::settings::flags::Noqa;
use ruff_linter::source_kind::SourceKind;
use ruff_python_ast::PySourceType;
use ruff_source_file::LineIndex;
use ruff_workspace::resolver::{python_files_in_path, PyprojectConfig, Resolver};
use ruff_workspace::Settings;

use crate::diagnostic::to_lsp_diagnostic;
use crate::document::Document;
use crate::encoding::{text_range, AtomicPositionEncoding, PositionEncoding};

type Documents = HashMap<Url, Document>;

#[derive(Debug)]
pub(crate) struct Session {
    client: Client,

    /// The agreed encoding for document position information between client and server.
    position_encoding: AtomicPositionEncoding,

    documents: RwLock<Documents>,

    configuration: RwLock<ClientConfiguration>,
}

impl Session {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            client,
            position_encoding: AtomicPositionEncoding::default(),
            documents: RwLock::default(),
            configuration: RwLock::new(ClientConfiguration::default()),
        }
    }

    pub(crate) async fn document(&self, uri: &Url) -> Option<Document> {
        self.documents().await.get(uri).cloned()
    }

    pub(crate) async fn documents(&self) -> RwLockReadGuard<Documents> {
        self.documents.read().await
    }

    pub(crate) async fn documents_mut(&self) -> RwLockWriteGuard<Documents> {
        self.documents.write().await
    }

    pub(crate) async fn configuration(&self) -> RwLockReadGuard<ClientConfiguration> {
        self.configuration.read().await
    }

    pub(crate) async fn configuration_mut(&self) -> RwLockWriteGuard<ClientConfiguration> {
        self.configuration.write().await
    }

    pub(crate) async fn update_document(
        &self,
        changes: DidChangeTextDocumentParams,
    ) -> Result<Document> {
        let text_document = changes.text_document;
        let mut documents = self.documents_mut().await;

        let Some(document) = documents.get_mut(&text_document.uri) else {
            tracing::error!(
                "Failed to update not opened document {:?}.",
                text_document.uri
            );
            return Err(anyhow!("Unknown document with uri {:?}", text_document.uri));
        };

        // Fast path for a single edit that replaces the whole document (no need to build any line index)
        if let [TextDocumentContentChangeEvent {
            range: None, text, ..
        }] = changes.content_changes.as_slice()
        {
            tracing::debug!("Fast path, replacing content of entire document");
            document.update(text, text_document.version);
            return Ok(document.clone());
        }

        let mut line_index = Cow::Borrowed(document.line_index());
        let encoding = self.position_encoding();

        // Copy the string to ensure the operation is atomic, in case it fails.
        let mut text = document.text().to_string();

        let mut last_position = Position {
            line: u32::MAX,
            character: u32::MAX,
        };

        // Edits must be applied in order because the offsets are relative to the last edit.
        // > The actual content changes. The content changes describe single state
        //   changes to the document. So if there are two content changes c1 (at
        //   array index 0) and c2 (at array index 1) for a document in state S then
        //   c1 moves the document from S to S' and c2 from S' to S''. So c1 is
        //   computed on the state S and c2 is computed on the state S'.
        // https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#didChangeTextDocumentParams
        for edit in changes.content_changes {
            if let Some(range) = edit.range {
                if last_position <= range.end {
                    // Rebuild the line index if the previous edit modified content before the current end position.
                    line_index = Cow::Owned(LineIndex::from_source_text(&text));
                }

                last_position = range.start;

                let range = text_range(range, &text, &line_index, encoding);

                text.replace_range(
                    usize::from(range.start())..usize::from(range.end()),
                    &edit.text,
                );
            } else {
                text = edit.text;
                last_position = Position {
                    line: 0,
                    character: 0,
                }
            }
        }

        document.update(text, text_document.version);

        Ok(document.clone())
    }

    pub(crate) async fn update_workspaces(
        &self,
        added: Vec<WorkspaceFolder>,
        removed: Vec<WorkspaceFolder>,
    ) -> anyhow::Result<()> {
        let mut configuration = self.configuration.write().await;

        let workspaces = &mut configuration.workspaces;

        let mut new_workspaces = added;

        for workspace in workspaces.drain(..) {
            tracing::debug!(
                "Remove workspace {}: {}.",
                workspace.name(),
                workspace.uri()
            );
            if removed
                .iter()
                .any(|removed| &removed.uri == workspace.uri())
            {
                continue;
            }

            new_workspaces.push(WorkspaceFolder {
                uri: workspace.uri().clone(),
                name: workspace.name().to_owned(),
            });
        }

        for workspace_folder in new_workspaces {
            let add_workspace_span = tracing::trace_span!(
                "Add workspace folder",
                name = workspace_folder.name,
                uri = %workspace_folder.uri
            );

            add_workspace_span.in_scope(|| {
                let path = self.file_path(&workspace_folder.uri);

                // TODO error handling
                let configuration = match WorkspaceConfiguration::from_path(&path) {
                    Ok(configuration) => Some(Arc::new(configuration)),
                    Err(error) => {
                        tracing::error!(
                        "Failed to load configuration for workspace root {path:?}. Error: {error}."
                    );
                        return;
                    }
                };

                workspaces.push(Workspace {
                    uri: workspace_folder.uri,
                    name: workspace_folder.name,
                    path,
                    configuration,
                });
            });
        }

        Ok(())
    }

    pub(crate) async fn insert_document(&self, uri: Url, document: Document) {
        self.documents_mut().await.insert(uri, document);
    }

    pub(crate) async fn remove_document(&self, uri: &Url) -> Option<Document> {
        self.documents_mut().await.remove(uri)
    }

    pub(crate) fn position_encoding(&self) -> PositionEncoding {
        self.position_encoding.get()
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub(crate) fn set_position_encoding(&self, position_encoding: PositionEncoding) {
        self.position_encoding.set(position_encoding);
    }

    pub(crate) async fn update_diagnostics(&self, uri: Url, document: &Document) -> Result<()> {
        let diagnostics = self.pull_diagnostics(&uri, document).await?;

        self.client()
            .publish_diagnostics(uri, diagnostics, Some(document.version()))
            .await;

        Ok(())
    }

    pub(crate) async fn pull_diagnostics(
        &self,
        uri: &Url,
        document: &Document,
    ) -> Result<Vec<Diagnostic>> {
        let source = SourceKind::Python(document.text().to_string());

        let path = self.file_path(uri);

        let Ok(configuration) = self.resolve_configuration(&path).await else {
            return Err(anyhow!(
                "Linting is disabled, failed to resolve workspace for path {path:?}"
            ));
        };

        // TODO test if file is included / excluded.

        let results = spawn_blocking(move || {
            let settings = configuration.settings(&path);

            lint_only(
                &path,
                None,
                &settings.linter,
                Noqa::Enabled,
                &source,
                PySourceType::Python,
            )
        })
        .await?;

        // TODO detect right source type

        let (messages, _) = results.data;

        let mut diagnostics = Vec::with_capacity(messages.len());

        let encoding = self.position_encoding();
        for message in messages {
            diagnostics.push(to_lsp_diagnostic(message, document, encoding)?);
        }

        Ok(diagnostics)
    }

    #[allow(clippy::unused_self)]
    pub(crate) fn file_path(&self, uri: &Url) -> PathBuf {
        match uri.to_file_path() {
            Err(_) => {
                // If we can't create a path, it's probably because the file doesn't exist.
                // It can be a newly created file that it's not on disk
                PathBuf::from(uri.path())
            }
            Ok(path) => path,
        }
    }

    pub(crate) fn client(&self) -> &Client {
        &self.client
    }

    /// Resolves the workspace for the given `path` or `None` if the file isn't part of the open workspace.
    pub(crate) async fn resolve_configuration(
        &self,
        path: &Path,
    ) -> anyhow::Result<Arc<WorkspaceConfiguration>> {
        let configuration = self.configuration.read().await;

        let workspace = configuration
            .workspaces
            .iter()
            .find(|workspace| path.starts_with(workspace.path()));

        if let Some(workspace) = workspace {
            workspace.configuration().ok_or_else(|| {
                anyhow!(
                    "Configuration for workspace {} was not loaded.",
                    workspace.uri()
                )
            })
        } else {
            tracing::debug!("Resolve ad-hoc configuration for non-workspace file {path:?}");
            // File that doesn't belong to any workspace. Ad hoc discovery of the configuration.
            let configuration = PyprojectConfig::resolve(false, None, &(), Some(path))?;
            let (_, resolver) = python_files_in_path(&[path.to_path_buf()], &configuration, &())?;

            Ok(Arc::new(WorkspaceConfiguration {
                resolver,
                configuration,
            }))
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ClientConfiguration {
    pub(crate) supports_change_watched_files: bool,
    pub(crate) workspaces: Vec<Workspace>,
}

#[derive(Debug, Clone)]
pub(crate) struct Workspace {
    uri: Url,
    name: String,
    path: PathBuf,
    /// The loaded configuration for this workspace or `None` if the configuration could not be loaded.
    configuration: Option<Arc<WorkspaceConfiguration>>,
}

impl Workspace {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn uri(&self) -> &Url {
        &self.uri
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn configuration(&self) -> Option<Arc<WorkspaceConfiguration>> {
        self.configuration.as_ref().cloned()
    }
}

#[derive(Debug)]
pub(crate) struct WorkspaceConfiguration {
    resolver: Resolver,
    configuration: PyprojectConfig,
}

impl WorkspaceConfiguration {
    pub(crate) fn settings(&self, path: &Path) -> &Settings {
        self.resolver.resolve(path, &self.configuration)
    }

    pub(crate) fn from_path(path: &Path) -> anyhow::Result<Self> {
        let configuration = PyprojectConfig::resolve(false, None, &(), Some(path))?;

        let (_, resolver) = python_files_in_path(&[path.to_path_buf()], &configuration, &())?;

        Ok(Self {
            resolver,
            configuration,
        })
    }
}
