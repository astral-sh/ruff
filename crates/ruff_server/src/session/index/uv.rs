use std::path::PathBuf;
use std::sync::{Arc, Weak};

use crossbeam::channel::{Sender, unbounded};
use lsp_types::{FileChangeType, FileEvent, Uri};
use ruff_linter::rules::isort::categorize::WorkspaceImports;
use ruff_workspace::uv::UvImportDiscovery;
use rustc_hash::{FxHashMap, FxHashSet};

use super::{DocumentController, RuffSettings, WorkspaceSettingsIndex};
use crate::server::{Event, MainLoopSender};

#[derive(Debug)]
pub(crate) struct UvImports {
    discovery: Weak<UvImportDiscovery>,
    project: PathBuf,
    imports: Option<WorkspaceImports>,
}

/// The editor can serve diagnostics with the last known imports while uv is running.
pub(super) struct UvProjects {
    worker: Sender<(Weak<UvImportDiscovery>, PathBuf)>,
    discovery: Arc<UvImportDiscovery>,
    requested: FxHashSet<PathBuf>,
    imports: FxHashMap<PathBuf, WorkspaceImports>,
    documents: FxHashMap<Uri, (PathBuf, Arc<RuffSettings>)>,
}

impl UvProjects {
    pub(super) fn spawn(events: MainLoopSender) -> std::io::Result<Self> {
        let (worker, requests) = unbounded::<(Weak<UvImportDiscovery>, PathBuf)>();
        // Keep uv off both the main loop and the pool that serves editor requests.
        std::thread::Builder::new()
            .name("ruff:uv".into())
            .spawn(move || {
                for (discovery, project) in requests {
                    let Some(current) = discovery.upgrade() else {
                        continue;
                    };
                    let imports = current.discover(&project);
                    if events
                        .send(Event::UvImports(UvImports {
                            discovery,
                            project,
                            imports,
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self::new(worker))
    }

    fn new(worker: Sender<(Weak<UvImportDiscovery>, PathBuf)>) -> Self {
        Self {
            worker,
            discovery: Arc::default(),
            requested: FxHashSet::default(),
            imports: FxHashMap::default(),
            documents: FxHashMap::default(),
        }
    }

    pub(super) fn needs_reload(&self, changes: &[FileEvent]) -> bool {
        changes.iter().any(|change| {
            let Ok(path) = change.uri.to_file_path() else {
                return false;
            };
            if matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("py" | "pyi")
            ) {
                // Editing a module's contents does not change its import name.
                change.kind != FileChangeType::Changed
                    && self.requested.iter().any(|project| {
                        self.imports.get(project).map_or_else(
                            || path.starts_with(project),
                            |imports| imports.contains_editable_path(&path),
                        )
                    })
            } else {
                true
            }
        })
    }

    pub(super) fn invalidate(&mut self) {
        self.discovery = Arc::default();
        self.requested.clear();
        self.documents.clear();
    }

    pub(super) fn track(&mut self, uri: &Uri, workspaces: &WorkspaceSettingsIndex) {
        let Ok(path) = uri.to_file_path() else { return };
        let Some(workspace) = workspaces.for_path(&path) else {
            return;
        };
        let mut settings = workspace.ruff_settings.get(&path);
        if !UvImportDiscovery::uses_import_sections(&settings) {
            return;
        }
        let Some(project) = path.parent().and_then(UvImportDiscovery::project_root) else {
            return;
        };
        if self.requested.insert(project.to_owned()) {
            if self
                .worker
                .send((Arc::downgrade(&self.discovery), project.to_owned()))
                .is_err()
            {
                tracing::error!("uv metadata worker has stopped");
            }
        }
        if let Some(imports) = self.imports.get(project) {
            Arc::make_mut(&mut settings).set_workspace_imports(Some(imports.clone()));
        }
        self.documents
            .insert(uri.clone(), (project.to_owned(), settings));
    }

    pub(super) fn settings(&self, uri: &Uri) -> Option<&Arc<RuffSettings>> {
        self.documents.get(uri).map(|(_, settings)| settings)
    }

    pub(super) fn remove(&mut self, uri: &Uri) {
        self.documents.remove(uri);
    }

    pub(super) fn retain_open(&mut self, documents: &FxHashMap<Uri, DocumentController>) {
        self.documents.retain(|uri, _| documents.contains_key(uri));
    }

    pub(super) fn update(&mut self, result: UvImports) -> bool {
        if !result.discovery.ptr_eq(&Arc::downgrade(&self.discovery)) {
            return false;
        }
        let previous = self
            .imports
            .get(&result.project)
            .map(WorkspaceImports::fingerprint);
        let changed = previous != result.imports.as_ref().map(WorkspaceImports::fingerprint);
        let mut open = false;
        if changed {
            #[expect(
                clippy::iter_over_hash_type,
                reason = "each document's settings are updated independently"
            )]
            for (project, settings) in self.documents.values_mut() {
                if project == &result.project {
                    Arc::make_mut(settings).set_workspace_imports(result.imports.clone());
                    open = true;
                }
            }
        }
        if let Some(imports) = result.imports {
            self.imports.insert(result.project, imports);
        } else {
            self.imports.remove(&result.project);
        }
        open
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;

    use anyhow::Result;
    use serde_json::json;

    use super::*;
    use crate::TextDocument;
    use crate::edit::DocumentKey;
    use crate::session::Client;
    use crate::session::index::Index;
    use crate::session::options::GlobalOptions;
    use crate::workspace::Workspace;

    #[test]
    fn editor_only_queues_the_uv_project_for_each_document() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = dunce::canonicalize(temp.path())?;
        let nested = root.join("nested");
        fs::create_dir(&nested)?;
        fs::write(root.join("pyproject.toml"), "")?;
        fs::write(nested.join("pyproject.toml"), "")?;
        let uri = |path: &Path| Uri::from_file_path(path).unwrap();
        let client = Client::new(unbounded().0, unbounded().0);
        let options: GlobalOptions =
            serde_json::from_value(json!({"configurationPreference": "editorOnly"}))?;
        let global = options.into_settings(client.clone());
        let (worker, requests) = unbounded();
        let mut index = Index {
            uv: Some(UvProjects::new(worker)),
            ..Index::default()
        };
        index
            .settings
            .register_workspace(&Workspace::new(uri(&root)), &global, &client)?;

        index.open_text_document(
            uri(&nested.join("example.py")),
            TextDocument::new(String::new(), 1),
        );
        let (discovery, project) = requests.try_recv()?;
        assert_eq!(project, nested);

        // A second document is available before uv has answered the first request.
        let peer = uri(&root.join("peer.py"));
        index.open_text_document(peer.clone(), TextDocument::new(String::new(), 1));
        assert_eq!(requests.try_recv()?.1, root);
        assert!(
            index
                .make_document_ref(DocumentKey::Text(peer), &global)
                .is_some()
        );

        index.reload_settings(
            &[FileEvent::new(
                uri(&nested.join("uv.lock")),
                FileChangeType::Changed,
            )],
            &client,
        );
        assert!(!index.update_uv_imports(UvImports {
            discovery,
            project,
            imports: Some(WorkspaceImports::new(BTreeMap::new(), Vec::new())),
        }));
        Ok(())
    }
}
