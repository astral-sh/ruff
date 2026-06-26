use crate::db::{Db, ProjectDatabase};
use crate::metadata::options::ProjectOptionsOverrides;
use crate::watch::{ChangeEvent, CreatedKind, DeletedKind};
use crate::{ProjectMetadata, ProjectReloadResult};
use std::collections::BTreeSet;

use super::ignore::IgnoreFiles;
use crate::walk::ProjectFilesWalker;
use ruff_db::Db as _;
use ruff_db::files::{File, Files, system_path_to_file};
use ruff_db::system::{SystemPath, SystemPathBuf};
use rustc_hash::FxHashSet;
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
        // Paths whose project files should be discovered incrementally.
        let mut added_paths = BTreeSet::default();

        // Deduplicate the `sync` calls. Many file watchers emit multiple events for the same path.
        let mut synced_files = FxHashSet::default();
        let mut sync_recursively = BTreeSet::default();
        // A non-file delete may be a deleted directory or an ambiguous LSP delete for a path
        // that no longer exists. Handle it recursively to keep Salsa's file state in sync.
        let mut removed_paths = BTreeSet::default();
        let mut reload_project = false;
        let mut reload_project_files = false;
        let respect_ignore_files = project.settings(self).src().respect_ignore_files;
        let ignore_walk_roots =
            respect_ignore_files.then(|| project.included_paths_or_root(self).to_vec());
        let mut ignore_files = ignore_walk_roots
            .as_deref()
            .map(|walk_roots| IgnoreFiles::new(self.system().dyn_clone(), walk_roots));

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

                if is_ignore_file(path) && project.settings(self).src().respect_ignore_files {
                    File::sync_path(self, path);
                    if let Some(directory) = path.parent() {
                        if project
                            .included_paths_or_root(self)
                            .iter()
                            .any(|included_path| included_path.starts_with(directory))
                        {
                            tracing::debug!(
                                ignore_file = %path,
                                directory = %directory,
                                "Reloading project files for changed ignore file at or above included path"
                            );
                            reload_project_files = true;
                        } else if project.is_directory_included(self, directory)
                            && ignore_files.as_mut().is_none_or(|ignore_files| {
                                ignore_files.is_ignored(directory, true).is_uncertain()
                            })
                        {
                            tracing::debug!(
                                ignore_file = %path,
                                directory = %directory,
                                "Queueing project-file reindex for changed ignore file"
                            );

                            removed_paths.insert(directory.to_path_buf());

                            if self.system().path_exists(directory) {
                                added_paths.insert(directory.to_path_buf());
                            }
                        } else {
                            tracing::debug!(
                                ignore_file = %path,
                                directory = %directory,
                                "Ignoring changed ignore file because it doesn't affect indexed project paths"
                            );
                        }
                    }

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

                    // A created file can be indexed directly unless project indexing needs the
                    // walker to apply ignore-file semantics. The ignore fast path below skips
                    // that walk when it can prove a root ignore file already prunes the path.
                    if !project.file_set(self).is_lazy() {
                        if self.system().is_file(path) {
                            if !project
                                .is_file_included(self, path)
                                .should_index_file(self.system(), path)
                            {
                                continue;
                            }

                            if let Some(ignore_files) = ignore_files.as_mut() {
                                if ignore_files.is_ignored(path, false).is_uncertain() {
                                    added_paths.insert(path.to_path_buf());
                                }
                            } else if let Ok(file) = system_path_to_file(self, path) {
                                project.add_file(self, file);
                            }
                        } else if project.is_directory_included(self, path)
                            && ignore_files.as_mut().is_none_or(|ignore_files| {
                                ignore_files.is_ignored(path, true).is_uncertain()
                            })
                        {
                            // Unlike a new file, a new directory needs walking to discover
                            // project files that exist below it.
                            added_paths.insert(path.clone());
                        }
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

        Files::sync_all_recursive(self, sync_recursively);

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
                        let error = anyhow::Error::new(error);
                        tracing::error!(
                            "Failed to apply configuration files, continuing without applying them: {error:#}"
                        );
                    }

                    if let Some(overrides) = project_options_overrides {
                        metadata.apply_overrides(overrides);
                    }

                    metadata.try_add_project_root(self);

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
                    let error = anyhow::Error::new(error);
                    tracing::error!(
                        "Failed to load project, keeping old project configuration: {error:#}"
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

        project.remove_files_under(self, removed_paths);

        let diagnostics = if !project.file_set(self).is_lazy() {
            // Use directory walking to discover newly added files.
            let walker = ProjectFilesWalker::incremental(added_paths);
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
