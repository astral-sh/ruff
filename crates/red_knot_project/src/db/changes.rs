use crate::db::{Db, ProjectDatabase};
use crate::metadata::options::Options;
use crate::watch::{ChangeEvent, CreatedKind, DeletedKind};
use crate::{Project, ProjectMetadata};

use red_knot_python_semantic::Program;
use ruff_db::files::{system_path_to_file, File, Files};
use ruff_db::system::walk_directory::WalkState;
use ruff_db::system::SystemPath;
use ruff_db::Db as _;
use rustc_hash::FxHashSet;

impl ProjectDatabase {
    #[tracing::instrument(level = "debug", skip(self, changes, cli_options))]
    pub fn apply_changes(&mut self, changes: Vec<ChangeEvent>, cli_options: Option<&Options>) {
        let mut project = self.project();
        let project_path = project.root(self).to_path_buf();
        let program = Program::get(self);
        let custom_stdlib_versions_path = program
            .custom_stdlib_search_path(self)
            .map(|path| path.join("VERSIONS"));

        // Are there structural changes to the project
        let mut project_changed = false;
        // Changes to a custom stdlib path's VERSIONS
        let mut custom_stdlib_change = false;
        // Paths that were added
        let mut added_paths = FxHashSet::default();

        // Deduplicate the `sync` calls. Many file watchers emit multiple events for the same path.
        let mut synced_files = FxHashSet::default();
        let mut synced_recursively = FxHashSet::default();

        let mut sync_path = |db: &mut ProjectDatabase, path: &SystemPath| {
            if synced_files.insert(path.to_path_buf()) {
                File::sync_path(db, path);
            }
        };

        let mut sync_recursively = |db: &mut ProjectDatabase, path: &SystemPath| {
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
                    // Changes to ignore files or settings can change the project structure or add/remove files.
                    project_changed = true;

                    continue;
                }

                if Some(path) == custom_stdlib_versions_path.as_deref() {
                    custom_stdlib_change = true;
                }
            }

            match change {
                ChangeEvent::Changed { path, kind: _ } | ChangeEvent::Opened(path) => {
                    sync_path(self, &path);
                }

                ChangeEvent::Created { kind, path } => {
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

                ChangeEvent::Deleted { kind, path } => {
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

                        if let Some(file) = self.files().try_system(self, &path) {
                            project.remove_file(self, file);
                        }
                    } else {
                        sync_recursively(self, &path);

                        if custom_stdlib_versions_path
                            .as_ref()
                            .is_some_and(|versions_path| versions_path.starts_with(&path))
                        {
                            custom_stdlib_change = true;
                        }

                        // Perform a full-reload in case the deleted directory contained the pyproject.toml.
                        // We may want to make this more clever in the future, to e.g. iterate over the
                        // indexed files and remove the once that start with the same path, unless
                        // the deleted path is the project configuration.
                        project_changed = true;
                    }
                }

                ChangeEvent::CreatedVirtual(path) | ChangeEvent::ChangedVirtual(path) => {
                    File::sync_virtual_path(self, &path);
                }

                ChangeEvent::DeletedVirtual(path) => {
                    if let Some(virtual_file) = self.files().try_virtual_file(&path) {
                        virtual_file.close(self);
                    }
                }

                ChangeEvent::Rescan => {
                    project_changed = true;
                    Files::sync_all(self);
                    break;
                }
            }
        }

        if project_changed {
            match ProjectMetadata::discover(&project_path, self.system()) {
                Ok(mut metadata) => {
                    if let Some(cli_options) = cli_options {
                        metadata.apply_cli_options(cli_options.clone());
                    }

                    let program_settings = metadata.to_program_settings(self.system());

                    let program = Program::get(self);
                    if let Err(error) = program.update_from_settings(self, program_settings) {
                        tracing::error!("Failed to update the program settings, keeping the old program settings: {error}");
                    };

                    if metadata.root() == project.root(self) {
                        tracing::debug!("Reloading project after structural change");
                        project.reload(self, metadata);
                    } else {
                        tracing::debug!("Replace project after structural change");
                        project = Project::from_metadata(self, metadata);
                        self.project = Some(project);
                    }
                }
                Err(error) => {
                    tracing::error!(
                        "Failed to load project, keeping old project configuration: {error}"
                    );
                }
            }

            return;
        } else if custom_stdlib_change {
            let search_paths = project
                .metadata(self)
                .to_program_settings(self.system())
                .search_paths;

            if let Err(error) = program.update_search_paths(self, &search_paths) {
                tracing::error!("Failed to set the new search paths: {error}");
            }
        }

        let mut added_paths = added_paths.into_iter();

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
                let file = system_path_to_file(self, &path);

                if let Ok(file) = file {
                    project.add_file(self, file);
                }
            }
        }
    }
}
