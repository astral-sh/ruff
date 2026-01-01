use crate::db::{Db, ProjectDatabase};
use crate::metadata::options::ProjectOptionsOverrides;
use crate::watch::{ChangeEvent, CreatedKind, DeletedKind};
use crate::{Project, ProjectMetadata};
use std::collections::BTreeSet;

use crate::walk::ProjectFilesWalker;
use ruff_db::Db as _;
use ruff_db::file_revision::FileRevision;
use ruff_db::files::{File, FileRootKind, Files};
use ruff_db::system::SystemPath;
use rustc_hash::FxHashSet;
use salsa::Setter;
use ty_python_semantic::Program;

/// Represents the result of applying changes to the project database.
pub struct ChangeResult {
    project_changed: bool,
    custom_stdlib_changed: bool,
}

impl ChangeResult {
    /// Returns `true` if the project structure has changed.
    pub fn project_changed(&self) -> bool {
        self.project_changed
    }

    /// Returns `true` if the custom stdlib's VERSIONS file has changed.
    pub fn custom_stdlib_changed(&self) -> bool {
        self.custom_stdlib_changed
    }
}

impl ProjectDatabase {
    #[tracing::instrument(level = "debug", skip(self, changes, project_options_overrides))]
    pub fn apply_changes(
        &mut self,
        changes: Vec<ChangeEvent>,
        project_options_overrides: Option<&ProjectOptionsOverrides>,
    ) -> ChangeResult {
        let mut project = self.project();
        let project_root = project.root(self).to_path_buf();
        let config_file_override =
            project_options_overrides.and_then(|options| options.config_file_override.clone());
        let program = Program::get(self);
        let custom_stdlib_versions_path = program
            .custom_stdlib_search_path(self)
            .map(|path| path.join("VERSIONS"));

        let mut result = ChangeResult {
            project_changed: false,
            custom_stdlib_changed: false,
        };
        // Paths that were added
        let mut added_paths = FxHashSet::default();

        // Deduplicate the `sync` calls. Many file watchers emit multiple events for the same path.
        let mut synced_files = FxHashSet::default();
        let mut sync_recursively = BTreeSet::default();

        for change in changes {
            tracing::trace!("Handle change: {:?}", change);

            if let Some(path) = change.system_path() {
                if let Some(config_file) = &config_file_override {
                    if config_file.as_path() == path {
                        result.project_changed = true;

                        continue;
                    }
                }

                if matches!(
                    path.file_name(),
                    Some(".gitignore" | ".ignore" | "ty.toml" | "pyproject.toml")
                ) {
                    // Changes to ignore files or settings can change the project structure or add/remove files.
                    result.project_changed = true;

                    continue;
                }

                if Some(path) == custom_stdlib_versions_path.as_deref() {
                    result.custom_stdlib_changed = true;
                }
            }

            match change {
                ChangeEvent::Changed { path, kind: _ } | ChangeEvent::Opened(path) => {
                    if synced_files.insert(path.to_path_buf()) {
                        let absolute =
                            SystemPath::absolute(&path, self.system().current_directory());
                        File::sync_path_only(self, &absolute);
                        if let Some(root) = self.files().root(self, &absolute) {
                            match root.kind_at_time_of_creation(self) {
                                // When a file inside the root of
                                // the project is changed, we don't
                                // want to mark the entire root as
                                // having changed too. In theory it
                                // might make sense to, but at time
                                // of writing, the file root revision
                                // on a project is used to invalidate
                                // the submodule files found within a
                                // directory. If we bumped the revision
                                // on every change within a project,
                                // then this caching technique would be
                                // effectively useless.
                                //
                                // It's plausible we should explore
                                // a more robust cache invalidation
                                // strategy that models more directly
                                // what we care about. For example, by
                                // keeping track of directories and
                                // their direct children explicitly,
                                // and then keying the submodule cache
                                // off of that instead. ---AG
                                FileRootKind::Project => {}
                                FileRootKind::LibrarySearchPath => {
                                    root.set_revision(self).to(FileRevision::now());
                                }
                            }
                        }
                    }
                }

                ChangeEvent::Created { kind, path } => {
                    match kind {
                        CreatedKind::File => {
                            if synced_files.insert(path.to_path_buf()) {
                                File::sync_path(self, &path);
                            }
                        }
                        CreatedKind::Directory | CreatedKind::Any => {
                            sync_recursively.insert(path.clone());
                        }
                    }

                    // Unlike other files, it's not only important to update the status of existing
                    // and known `File`s (`sync_recursively`), it's also important to discover new files
                    // that were added in the project's root (or any of the paths included for checking).
                    //
                    // This is important because `Project::check` iterates over all included files.
                    // The code below walks the `added_paths` and adds all files that
                    // should be included in the project. We can skip this check for
                    // paths that aren't part of the project or shouldn't be included
                    // when checking the project.

                    if self.system().is_file(&path) {
                        if project.is_file_included(self, &path) {
                            // Add the parent directory because `walkdir` always visits explicitly passed files
                            // even if they match an exclude filter.
                            added_paths.insert(path.parent().unwrap().to_path_buf());
                        }
                    } else if project.is_directory_included(self, &path) {
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
                        if synced_files.insert(path.to_path_buf()) {
                            File::sync_path(self, &path);
                        }

                        if let Some(file) = self.files().try_system(self, &path) {
                            project.remove_file(self, file);
                        }
                    } else {
                        sync_recursively.insert(path.clone());

                        if custom_stdlib_versions_path
                            .as_ref()
                            .is_some_and(|versions_path| versions_path.starts_with(&path))
                        {
                            result.custom_stdlib_changed = true;
                        }

                        let directory_included = project.is_directory_included(self, &path);

                        if directory_included || path == project_root {
                            // TODO: Shouldn't it be enough to simply traverse the project files and remove all
                            // that start with the given path?
                            tracing::debug!(
                                "Reload project because of a path that could have been a directory."
                            );

                            // Perform a full-reload in case the deleted directory contained the pyproject.toml.
                            // We may want to make this more clever in the future, to e.g. iterate over the
                            // indexed files and remove the once that start with the same path, unless
                            // the deleted path is the project configuration.
                            result.project_changed = true;
                        } else if !directory_included {
                            tracing::debug!(
                                "Skipping reload because directory '{path}' isn't included in the project"
                            );
                        }
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
                    result.project_changed = true;
                    Files::sync_all(self);
                    sync_recursively.clear();
                    break;
                }
            }
        }

        let sync_recursively = sync_recursively.into_iter();
        let mut last = None;

        for path in sync_recursively {
            // Avoid re-syncing paths that are sub-paths of each other.
            if let Some(last) = &last {
                if path.starts_with(last) {
                    continue;
                }
            }

            Files::sync_recursively(self, &path);
            last = Some(path);
        }

        if result.project_changed {
            let new_project_metadata = match config_file_override {
                Some(config_file) => {
                    ProjectMetadata::from_config_file(config_file, &project_root, self.system())
                }
                None => ProjectMetadata::discover(&project_root, self.system()),
            };
            match new_project_metadata {
                Ok(mut metadata) => {
                    if let Err(error) = metadata.apply_configuration_files(self.system()) {
                        tracing::error!(
                            "Failed to apply configuration files, continuing without applying them: {error}"
                        );
                    }

                    if let Some(overrides) = project_options_overrides {
                        metadata.apply_overrides(overrides);
                    }

                    match metadata.to_program_settings(self.system(), self.vendored()) {
                        Ok(program_settings) => {
                            let program = Program::get(self);
                            program.update_from_settings(self, program_settings);
                        }
                        Err(error) => {
                            tracing::error!(
                                "Failed to convert metadata to program settings, continuing without applying them: {error}"
                            );
                        }
                    }

                    if metadata.root() == project.root(self) {
                        tracing::debug!("Reloading project after structural change");
                        project.reload(self, metadata);
                    } else {
                        match Project::from_metadata(self, metadata) {
                            Ok(new_project) => {
                                tracing::debug!("Replace project after structural change");
                                project = new_project;
                            }
                            Err(error) => {
                                tracing::error!(
                                    "Keeping old project configuration because loading the new settings failed with: {error}"
                                );

                                project
                                    .set_settings_diagnostics(self)
                                    .to(vec![error.into_diagnostic()]);
                            }
                        }

                        self.project = Some(project);
                    }
                }
                Err(error) => {
                    tracing::error!(
                        "Failed to load project, keeping old project configuration: {error}"
                    );
                }
            }

            return result;
        } else if result.custom_stdlib_changed {
            match project
                .metadata(self)
                .to_program_settings(self.system(), self.vendored())
            {
                Ok(program_settings) => {
                    program.update_from_settings(self, program_settings);
                }
                Err(error) => {
                    tracing::error!("Failed to resolve program settings: {error}");
                }
            }
        }

        let diagnostics = if let Some(walker) = ProjectFilesWalker::incremental(self, added_paths) {
            // Use directory walking to discover newly added files.
            let (files, diagnostics) = walker.collect_vec(self);

            for file in files {
                project.add_file(self, file);
            }

            diagnostics
        } else {
            Vec::new()
        };

        // Note: We simply replace all IO related diagnostics here. This isn't ideal, because
        // it removes IO errors that may still be relevant. However, tracking IO errors correctly
        // across revisions doesn't feel essential, considering that they're rare. However, we could
        // implement a `BTreeMap` or similar and only prune the diagnostics from paths that we've
        // re-scanned (or that were removed etc).
        project.replace_index_diagnostics(self, diagnostics);

        result
    }
}
