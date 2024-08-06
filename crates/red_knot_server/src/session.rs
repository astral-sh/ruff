//! Data model, state management, and configuration resolution.

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::anyhow;
use lsp_types::{ClientCapabilities, Url};

use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_db::program::{ProgramSettings, SearchPathSettings, TargetVersion};
use ruff_db::system::SystemPath;
use ruff_db::Db as _;

use crate::edit::{DocumentKey, NotebookDocument};
use crate::system::LSPSystem;
use crate::{PositionEncoding, TextDocument};

pub(crate) use self::capabilities::ResolvedClientCapabilities;
pub use self::index::DocumentQuery;
pub(crate) use self::settings::AllSettings;
pub use self::settings::ClientSettings;

mod capabilities;
pub(crate) mod index;
mod settings;

/// The global state for the LSP
pub struct Session {
    /// Used to retrieve information about open documents and settings.
    index: Option<Arc<index::Index>>,
    /// Maps workspace root paths to their respective databases.
    workspaces: BTreeMap<PathBuf, salsa::Handle<RootDatabase>>,
    /// The global position encoding, negotiated during LSP initialization.
    position_encoding: PositionEncoding,
    /// Tracks what LSP features the client supports and doesn't support.
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
}

impl Session {
    pub fn new(
        client_capabilities: &ClientCapabilities,
        position_encoding: PositionEncoding,
        global_settings: ClientSettings,
        workspace_folders: &[(Url, ClientSettings)],
    ) -> crate::Result<Self> {
        let mut workspaces = BTreeMap::new();
        let index = Arc::new(index::Index::new(global_settings));

        for (url, _) in workspace_folders {
            let path = url
                .to_file_path()
                .map_err(|()| anyhow!("Workspace URL is not a file or directory: {:?}", url))?;
            let system_path = SystemPath::from_std_path(&path)
                .ok_or_else(|| anyhow!("Workspace path is not a valid UTF-8 path: {:?}", path))?;
            let system = LSPSystem::new(index.clone());

            let metadata = WorkspaceMetadata::from_path(system_path, &system)?;
            // TODO(dhruvmanila): Get the values from the client settings
            let program_settings = ProgramSettings {
                target_version: TargetVersion::default(),
                search_paths: SearchPathSettings {
                    extra_paths: vec![],
                    workspace_root: system_path.to_path_buf(),
                    site_packages: vec![],
                    custom_typeshed: None,
                },
            };
            workspaces.insert(
                path,
                salsa::Handle::new(RootDatabase::new(metadata, program_settings, system)),
            );
        }

        Ok(Self {
            position_encoding,
            workspaces,
            index: Some(index),
            resolved_client_capabilities: Arc::new(ResolvedClientCapabilities::new(
                client_capabilities,
            )),
        })
    }

    pub(crate) fn workspace_db_for_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Option<&salsa::Handle<RootDatabase>> {
        self.workspaces
            .range(..=path.as_ref().to_path_buf())
            .next_back()
            .map(|(_, db)| db)
    }

    pub fn key_from_url(&self, url: Url) -> DocumentKey {
        self.index().key_from_url(url)
    }

    /// Creates a document snapshot with the URL referencing the document to snapshot.
    pub fn take_snapshot(&self, url: Url) -> Option<DocumentSnapshot> {
        let key = self.key_from_url(url);
        Some(DocumentSnapshot {
            resolved_client_capabilities: self.resolved_client_capabilities.clone(),
            document_ref: self.index().make_document_ref(key)?,
            position_encoding: self.position_encoding,
        })
    }

    /// Registers a notebook document at the provided `url`.
    /// If a document is already open here, it will be overwritten.
    pub fn open_notebook_document(&mut self, url: Url, document: NotebookDocument) {
        self.index_mut().open_notebook_document(url, document);
    }

    /// Registers a text document at the provided `url`.
    /// If a document is already open here, it will be overwritten.
    pub(crate) fn open_text_document(&mut self, url: Url, document: TextDocument) {
        self.index_mut().open_text_document(url, document);
    }

    /// De-registers a document, specified by its key.
    /// Calling this multiple times for the same document is a logic error.
    pub(crate) fn close_document(&mut self, key: &DocumentKey) -> crate::Result<()> {
        self.index_mut().close_document(key)?;
        Ok(())
    }

    fn index(&self) -> &index::Index {
        self.index.as_ref().unwrap()
    }

    fn index_mut(&mut self) -> MutIndexGuard {
        let index = self.index.take().unwrap();

        for db in self.workspaces.values_mut() {
            // Calling `get_mut` on `Handle<Database>` cancels all pending queries and waits for them to stop.
            let db = db.get_mut();

            // Remove the `index` from each database. This drops the count of `Arc<Index>` down to 1
            db.system_mut()
                .as_any_mut()
                .downcast_mut::<LSPSystem>()
                .unwrap()
                .take_index();
        }

        // There should now be exactly one reference to index which is self.index.
        let index = Arc::into_inner(index);

        MutIndexGuard {
            session: self,
            index,
        }
    }
}

struct MutIndexGuard<'a> {
    session: &'a mut Session,
    index: Option<index::Index>,
}

impl Deref for MutIndexGuard<'_> {
    type Target = index::Index;

    fn deref(&self) -> &Self::Target {
        self.index.as_ref().unwrap()
    }
}

impl DerefMut for MutIndexGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.index.as_mut().unwrap()
    }
}

impl Drop for MutIndexGuard<'_> {
    fn drop(&mut self) {
        if let Some(index) = self.index.take() {
            let index = Arc::new(index);
            for db in self.session.workspaces.values_mut() {
                let db = db.get_mut();
                db.system_mut()
                    .as_any_mut()
                    .downcast_mut::<LSPSystem>()
                    .unwrap()
                    .set_index(index.clone());
            }

            self.session.index = Some(index);
        }
    }
}

/// An immutable snapshot of `Session` that references
/// a specific document.
pub struct DocumentSnapshot {
    resolved_client_capabilities: Arc<ResolvedClientCapabilities>,
    document_ref: index::DocumentQuery,
    position_encoding: PositionEncoding,
}

impl DocumentSnapshot {
    pub(crate) fn resolved_client_capabilities(&self) -> &ResolvedClientCapabilities {
        &self.resolved_client_capabilities
    }

    pub fn query(&self) -> &index::DocumentQuery {
        &self.document_ref
    }

    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.position_encoding
    }
}
