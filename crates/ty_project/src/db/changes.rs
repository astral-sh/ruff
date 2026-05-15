use crate::db::{Db, ProjectDatabase};
use crate::metadata::options::ProjectOptionsOverrides;
use crate::watch::{ChangeEvent, CreatedKind, DeletedKind};
use crate::{ProjectMetadata, ProjectReloadResult};
use std::collections::BTreeSet;

use crate::walk::ProjectFilesWalker;
use ruff_db::Db as _;
use ruff_db::file_revision::FileRevision;
use ruff_db::files::{File, FileRootKind, Files};
use ruff_db::system::{SystemPath, SystemPathBuf, deduplicate_nested_paths};
use rustc_hash::FxHashSet;
use salsa::Setter;
use ty_python_core::program::{FallibleStrategy, Program};

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
        changes: &[ChangeEvent],
        project_options_overrides: Option<&ProjectOptionsOverrides>,
    ) -> ChangeResult {
        let project = self.project();
        let project_root = project.root(self).to_path_buf();
        let config_file_override =
            project_options_overrides.and_then(|options| options.config_file_override.clone());
        let extra_configuration_paths = project.metadata(self).extra_configuration_paths().to_vec();
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
        // A non-file delete may be a deleted directory or an ambiguous LSP delete for a path
        // that no longer exists. Handle it recursively to keep Salsa's file state in sync.
        let mut removed_paths = BTreeSet::default();
        let mut reload_project = false;
        let mut reload_project_files = false;

        for change in changes {
            tracing::debug!("Handling file watcher change event: {:?}", change);

            if let Some(path) = change.system_path() {
                if is_project_configuration_path(
                    path,
                    &project_root,
                    config_file_override.as_ref(),
                    &extra_configuration_paths,
                ) {
                    File::sync_path(self, path);
                    reload_project = true;

                    continue;
                }

                if is_ignore_file(path) {
                    File::sync_path(self, path);
                    reload_project_files = true;

                    continue;
                }

                if Some(path) == custom_stdlib_versions_path.as_deref() {
                    result.custom_stdlib_changed = true;
                }
            }

            match change {
                ChangeEvent::Changed { path, kind: _ } | ChangeEvent::Opened(path) => {
                    if synced_files.insert(path.to_path_buf()) {
                        File::sync_path_only(self, path);
                        if let Some(root) = self.files().root(self, path) {
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
                                File::sync_path(self, path);
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
                    if self.system().is_file(path) {
                        if project.is_file_included(self, path) {
                            // Add the parent directory because `walkdir`
                            // always visits explicitly passed files even if
                            // they match an exclude filter.
                            added_paths.insert(path.parent().unwrap().to_path_buf());
                        }
                    } else if project.is_directory_included(self, path) {
                        added_paths.insert(path.clone());
                    }
                }

                ChangeEvent::Deleted { kind, path } => {
                    let is_file = match kind {
                        DeletedKind::File => true,
                        DeletedKind::Directory => false,
                        DeletedKind::Any => self
                            .files
                            .try_system(self, path)
                            .is_some_and(|file| file.exists(self)),
                    };

                    if is_file {
                        if synced_files.insert(path.to_path_buf()) {
                            File::sync_path(self, path);
                        }

                        if let Some(file) = self.files().try_system(self, path) {
                            project.remove_file(self, file);
                        }
                    } else {
                        sync_recursively.insert(path.clone());
                        removed_paths.insert(path.clone());

                        if custom_stdlib_versions_path
                            .as_ref()
                            .is_some_and(|versions_path| versions_path.starts_with(path))
                        {
                            result.custom_stdlib_changed = true;
                        }

                        if directory_may_contain_project_configuration(
                            path,
                            &project_root,
                            config_file_override.as_ref(),
                            &extra_configuration_paths,
                        ) {
                            tracing::debug!(
                                "Reload project because a configuration file may have been deleted."
                            );
                            reload_project = true;
                        }
                    }
                }

                ChangeEvent::CreatedVirtual(path) | ChangeEvent::ChangedVirtual(path) => {
                    File::sync_virtual_path(self, path);
                }

                ChangeEvent::DeletedVirtual(path) => {
                    if let Some(virtual_file) = self.files().try_virtual_file(path) {
                        virtual_file.close(self);
                    }
                }

                ChangeEvent::Rescan => {
                    reload_project = true;
                    reload_project_files = true;
                    Files::sync_all(self);
                    sync_recursively.clear();
                    removed_paths.clear();
                    break;
                }
            }
        }

        Files::sync_all_recursive(self, deduplicate_nested_paths(sync_recursively));

        if reload_project {
            // The active project root may have been deleted. Start rediscovery from
            // the closest existing ancestor so ty can fall back to an enclosing project.
            let rediscovery_path = project_root
                .ancestors()
                .find(|path| self.system().is_directory(path))
                .unwrap_or(&project_root);
            let new_project_metadata = match config_file_override {
                Some(config_file) => {
                    ProjectMetadata::from_config_file(config_file, &project_root, self.system())
                }
                None => ProjectMetadata::discover(rediscovery_path, self.system()),
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

                    let program_settings_diagnostics = match metadata.to_program_settings(
                        self.system(),
                        self.vendored(),
                        &FallibleStrategy,
                    ) {
                        Ok((program_settings, diagnostics)) => {
                            let program = Program::get(self);
                            program.update_from_settings(self, program_settings);
                            diagnostics
                        }
                        Err(error) => {
                            tracing::error!(
                                "Failed to convert metadata to program settings, continuing without applying them: {error}"
                            );
                            Vec::new()
                        }
                    };

                    let (settings, settings_diagnostics) = match metadata.options().to_settings(
                        self,
                        metadata.root(),
                        &FallibleStrategy,
                    ) {
                        Ok((settings, diagnostics)) => (Some(settings), diagnostics),
                        Err(error) => {
                            tracing::warn!(
                                "Keeping old project configuration because loading the new settings failed with: {error}"
                            );
                            (None, vec![error.into_diagnostic()])
                        }
                    };

                    tracing::debug!("Reloading project after structural change");
                    match project.reload(
                        self,
                        metadata,
                        settings,
                        settings_diagnostics,
                        program_settings_diagnostics,
                    ) {
                        ProjectReloadResult::Unchanged => {}
                        ProjectReloadResult::Changed { files_changed } => {
                            result.project_changed = true;
                            if files_changed {
                                // The project file set has already been rebuilt; continuing would
                                // run incremental discovery from paths collected before the reload.
                                return result;
                            }
                        }
                    }
                }
                Err(error) => {
                    tracing::error!(
                        "Failed to load project, keeping old project configuration: {error}"
                    );
                    if reload_project_files {
                        project.reload_files(self);
                        return result;
                    }
                }
            }
        }

        if reload_project_files {
            project.reload_files(self);
            // A full project-file reload supersedes incremental project-file updates.
            added_paths.clear();
            removed_paths.clear();
        }

        if result.custom_stdlib_changed {
            match project.metadata(self).to_program_settings(
                self.system(),
                self.vendored(),
                &FallibleStrategy,
            ) {
                Ok((program_settings, program_settings_diagnostics)) => {
                    program.update_from_settings(self, program_settings);
                    let settings_diagnostics = match project.metadata(self).options().to_settings(
                        self,
                        project.metadata(self).root(),
                        &FallibleStrategy,
                    ) {
                        Ok((_, diagnostics)) => diagnostics,
                        Err(error) => vec![error.into_diagnostic()],
                    };
                    project.update_settings_diagnostics(
                        self,
                        settings_diagnostics,
                        program_settings_diagnostics,
                    );
                }
                Err(error) => {
                    tracing::error!("Failed to resolve program settings: {error}");
                }
            }
        }

        for path in deduplicate_nested_paths(removed_paths) {
            project.remove_files_under(self, &path);
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

fn is_project_configuration_path(
    path: &SystemPath,
    project_root: &SystemPath,
    config_file_override: Option<&SystemPathBuf>,
    extra_configuration_paths: &[SystemPathBuf],
) -> bool {
    if extra_configuration_paths
        .iter()
        .any(|config_path| config_path.as_path() == path)
    {
        return true;
    }

    if let Some(config_path) = config_file_override {
        config_path.as_path() == path
    } else {
        path.parent()
            .is_some_and(|parent| project_root.starts_with(parent))
            && is_project_config_file(path)
    }
}

fn directory_may_contain_project_configuration(
    directory: &SystemPath,
    project_root: &SystemPath,
    config_file_override: Option<&SystemPathBuf>,
    extra_configuration_paths: &[SystemPathBuf],
) -> bool {
    if extra_configuration_paths
        .iter()
        .any(|config_path| config_path.starts_with(directory))
    {
        return true;
    }

    if let Some(config_path) = config_file_override {
        config_path.starts_with(directory)
    } else {
        // Deleting the project root or one of its ancestors can change rediscovery:
        // ty may need to fall back to an enclosing configuration.
        project_root.starts_with(directory)
    }
}

fn is_ignore_file(path: &SystemPath) -> bool {
    matches!(path.file_name(), Some(".gitignore" | ".ignore"))
}

fn is_project_config_file(path: &SystemPath) -> bool {
    matches!(path.file_name(), Some("ty.toml" | "pyproject.toml"))
}
