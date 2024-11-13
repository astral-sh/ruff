use crate::db::{Db, RootDatabase};
use crate::watch;
use crate::watch::{CreatedKind, DeletedKind};
use crate::workspace::settings::Configuration;
use crate::workspace::{Workspace, WorkspaceMetadata};
use red_knot_python_semantic::Program;
use ruff_db::files::{system_path_to_file, File, Files};
use ruff_db::system::walk_directory::WalkState;
use ruff_db::system::SystemPath;
use ruff_db::Db as _;
use rustc_hash::FxHashSet;

impl RootDatabase {
    #[tracing::instrument(level = "debug", skip(self, changes, base_configuration))]
    pub fn apply_changes(
        &mut self,
        changes: Vec<watch::ChangeEvent>,
        base_configuration: Option<&Configuration>,
    ) {
        let mut workspace = self.workspace();
        let workspace_path = workspace.root(self).to_path_buf();
        let program = Program::get(self);
        let custom_stdlib_versions_path = program
            .custom_stdlib_search_path(self)
            .map(|path| path.join("VERSIONS"));

        let mut workspace_change = false;
        // Changes to a custom stdlib path's VERSIONS
        let mut custom_stdlib_change = false;
        // Packages that need reloading
        let mut changed_packages = FxHashSet::default();
        // Paths that were added
        let mut added_paths = FxHashSet::default();

        // Deduplicate the `sync` calls. Many file watchers emit multiple events for the same path.
        let mut synced_files = FxHashSet::default();
        let mut synced_recursively = FxHashSet::default();

        let mut sync_path = |db: &mut RootDatabase, path: &SystemPath| {
            if synced_files.insert(path.to_path_buf()) {
                File::sync_path(db, path);
            }
        };

        let mut sync_recursively = |db: &mut RootDatabase, path: &SystemPath| {
            if synced_recursively.insert(path.to_path_buf()) {
                Files::sync_recursively(db, path);
            }
        };

        for change in changes {
            if let Some(path) = change.system_path() {
                if matches!(
                    path.file_name(),
                    Some(".gitignore" | ".ignore" | "ruff.toml" | ".ruff.toml" | "pyproject.toml")
                ) {
                    // Changes to ignore files or settings can change the workspace structure or add/remove files
                    // from packages.
                    if let Some(package) = workspace.package(self, path) {
                        if package.root(self) == workspace.root(self) {
                            workspace_change = true;
                        }

                        changed_packages.insert(package);
                    } else {
                        workspace_change = true;
                    }

                    continue;
                }

                if Some(path) == custom_stdlib_versions_path.as_deref() {
                    custom_stdlib_change = true;
                }
            }

            match change {
                watch::ChangeEvent::Changed { path, kind: _ }
                | watch::ChangeEvent::Opened(path) => sync_path(self, &path),

                watch::ChangeEvent::Created { kind, path } => {
                    match kind {
                        CreatedKind::File => sync_path(self, &path),
                        CreatedKind::Directory | CreatedKind::Any => {
                            sync_recursively(self, &path);
                        }
                    }

                    if self.system().is_file(&path) {
                        // Add the parent directory because `walkdir` always visits explicitly passed files
                        // even if they match an exclude filter.
                        added_paths.insert(path.parent().unwrap().to_path_buf());
                    } else {
                        added_paths.insert(path);
                    }
                }

                watch::ChangeEvent::Deleted { kind, path } => {
                    let is_file = match kind {
                        DeletedKind::File => true,
                        DeletedKind::Directory => {
                            // file watchers emit an event for every deleted file. No need to scan the entire dir.
                            continue;
                        }
                        DeletedKind::Any => self
                            .files
                            .try_system(self, &path)
                            .is_some_and(|file| file.exists(self)),
                    };

                    if is_file {
                        sync_path(self, &path);

                        if let Some(package) = workspace.package(self, &path) {
                            if let Some(file) = self.files().try_system(self, &path) {
                                package.remove_file(self, file);
                            }
                        }
                    } else {
                        sync_recursively(self, &path);

                        if custom_stdlib_versions_path
                            .as_ref()
                            .is_some_and(|versions_path| versions_path.starts_with(&path))
                        {
                            custom_stdlib_change = true;
                        }

                        if let Some(package) = workspace.package(self, &path) {
                            changed_packages.insert(package);
                        } else {
                            workspace_change = true;
                        }
                    }
                }

                watch::ChangeEvent::CreatedVirtual(path)
                | watch::ChangeEvent::ChangedVirtual(path) => {
                    File::sync_virtual_path(self, &path);
                }

                watch::ChangeEvent::DeletedVirtual(path) => {
                    if let Some(virtual_file) = self.files().try_virtual_file(&path) {
                        virtual_file.close(self);
                    }
                }

                watch::ChangeEvent::Rescan => {
                    workspace_change = true;
                    Files::sync_all(self);
                    break;
                }
            }
        }

        if workspace_change {
            match WorkspaceMetadata::discover(&workspace_path, self.system(), base_configuration) {
                Ok(metadata) => {
                    if metadata.root() == workspace.root(self) {
                        tracing::debug!("Reloading workspace after structural change");
                        // TODO: Handle changes in the program settings.
                        workspace.reload(self, metadata);
                    } else {
                        tracing::debug!("Replace workspace after structural change");
                        workspace = Workspace::from_metadata(self, metadata);
                        self.workspace = Some(workspace);
                    }
                }
                Err(error) => {
                    tracing::error!(
                        "Failed to load workspace, keeping old workspace configuration: {error}"
                    );
                }
            }

            return;
        } else if custom_stdlib_change {
            let search_paths = workspace.search_path_settings(self).clone();
            if let Err(error) = program.update_search_paths(self, &search_paths) {
                tracing::error!("Failed to set the new search paths: {error}");
            }
        }

        let mut added_paths = added_paths.into_iter().filter(|path| {
            let Some(package) = workspace.package(self, path) else {
                return false;
            };

            // Skip packages that need reloading
            !changed_packages.contains(&package)
        });

        // Use directory walking to discover newly added files.
        if let Some(path) = added_paths.next() {
            let mut walker = self.system().walk_directory(&path);

            for extra_path in added_paths {
                walker = walker.add(&extra_path);
            }

            let added_paths = std::sync::Mutex::new(Vec::default());

            walker.run(|| {
                Box::new(|entry| {
                    let Ok(entry) = entry else {
                        return WalkState::Continue;
                    };

                    if !entry.file_type().is_file() {
                        return WalkState::Continue;
                    }

                    let mut paths = added_paths.lock().unwrap();

                    paths.push(entry.into_path());

                    WalkState::Continue
                })
            });

            for path in added_paths.into_inner().unwrap() {
                let package = workspace.package(self, &path);
                let file = system_path_to_file(self, &path);

                if let (Some(package), Ok(file)) = (package, file) {
                    package.add_file(self, file);
                }
            }
        }

        // Reload
        for package in changed_packages {
            package.reload_files(self);
        }
    }
}
