use crate::db::{Db, ProjectDatabase};
use crate::watch::{ChangeEvent, CreatedKind, DeletedKind};
use crate::{ProjectMetadata, ProjectReloadResult};
use std::collections::BTreeSet;

use crate::walk::{ProjectFilesWalker, create_walker_builder};
use ruff_db::Db as _;
use ruff_db::files::{File, Files, system_path_to_file};
use ruff_db::system::{SystemPath, SystemPathBuf};
use rustc_hash::FxHashSet;
use ty_python_core::program::FallibleStrategy;

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
    #[tracing::instrument(level = "debug", skip(self, changes))]
    pub fn apply_changes(&mut self, changes: &[ChangeEvent]) -> ChangeResult {
        let project = self.project();
        let project_root = project.root(self).to_path_buf();
        let configuration_paths = ConfigurationPaths::from_metadata(project.metadata(self));
        let program = self.project().program(self);
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
        // TODO: This should be removed once the incremental checker is ported
        // over to the `ignore` crate, since the `ignore` crate will respect
        // the settings provided in `create_walker`. ---AG
        let respect_ignore_files = project.settings(self).src().respect_ignore_files;
        let ignore_walk_roots =
            respect_ignore_files.then(|| project.included_paths_or_root(self).to_vec());
        let mut ignore_files = ignore_walk_roots.as_deref().and_then(|walk_roots| {
            Some(create_walker_builder(self, walk_roots)?.incremental_matcher())
        });

        for change in changes {
            tracing::debug!("Handling file watcher change event: {:?}", change);

            if let Some(path) = change.system_path() {
                if configuration_paths.is_configuration(path, &project_root) {
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
                                !ignore_files.is_ignored(directory, true)
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
                    // walker to apply ignore-file semantics. The ignore check below skips that
                    // walk when the path is ignored.
                    if !project.file_set(self).is_lazy() {
                        if self.system().is_file(path) {
                            if !project
                                .is_file_included(self, path)
                                .should_index_file(self.system(), path)
                            {
                                continue;
                            }

                            if ignore_files
                                .as_mut()
                                .is_none_or(|ignore_files| !ignore_files.is_ignored(path, false))
                                && let Ok(file) = system_path_to_file(self, path)
                            {
                                project.add_file(self, file);
                            }
                        } else if project.is_directory_included(self, path)
                            && ignore_files
                                .as_mut()
                                .is_none_or(|ignore_files| !ignore_files.is_ignored(path, true))
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

                        if configuration_paths.may_contain_configuration(path, &project_root) {
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
            let new_project_metadata = project.metadata(self).rediscover(self.system());
            match new_project_metadata {
                Ok(mut metadata) => {
                    if let Err(error) = metadata.apply_configuration_files(self.system()) {
                        let error = anyhow::Error::new(error);
                        tracing::error!(
                            "Failed to apply configuration files, continuing without applying them: {error:#}"
                        );
                    }

                    metadata.try_add_project_root(self);
                    let merged_options = metadata.to_merged_options();

                    let program_settings_diagnostics = match merged_options.to_program_settings(
                        self.system(),
                        self.vendored(),
                        &FallibleStrategy,
                    ) {
                        Ok((program_settings, diagnostics)) => {
                            let program = self.project().program(self);
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

                    let (settings, mut settings_diagnostics) = match merged_options
                        .to_settings(self, &FallibleStrategy)
                    {
                        Ok((settings, diagnostics)) => (Some(settings), diagnostics),
                        Err(error) => {
                            tracing::warn!(
                                "Keeping old project configuration because loading the new settings failed with: {error}"
                            );
                            (None, vec![error.into_diagnostic()])
                        }
                    };
                    settings_diagnostics.extend(
                        program_settings_diagnostics
                            .into_iter()
                            .map(|diagnostic| diagnostic.into_diagnostic(self)),
                    );

                    tracing::debug!("Reloading project after structural change");
                    match project.reload(self, metadata, settings, settings_diagnostics) {
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
            let metadata = project.metadata(self);
            let merged_options = metadata.to_merged_options();
            match merged_options.to_program_settings(
                self.system(),
                self.vendored(),
                &FallibleStrategy,
            ) {
                Ok((program_settings, program_settings_diagnostics)) => {
                    let mut settings_diagnostics =
                        match merged_options.to_settings(self, &FallibleStrategy) {
                            Ok((_, diagnostics)) => diagnostics,
                            Err(error) => vec![error.into_diagnostic()],
                        };
                    program.update_from_settings(self, program_settings);
                    settings_diagnostics.extend(
                        program_settings_diagnostics
                            .into_iter()
                            .map(|diagnostic| diagnostic.into_diagnostic(self)),
                    );
                    project.update_settings_diagnostics(self, settings_diagnostics);
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

struct ConfigurationPaths {
    normal_discovery: bool,
    extra: Box<[SystemPathBuf]>,
}

impl ConfigurationPaths {
    fn from_metadata(metadata: &ProjectMetadata) -> Self {
        Self {
            normal_discovery: metadata.config_file_override().is_none(),
            extra: metadata
                .extra_configuration_paths()
                .map(SystemPath::to_path_buf)
                .collect(),
        }
    }

    fn is_configuration(&self, path: &SystemPath, project_root: &SystemPath) -> bool {
        if self
            .extra
            .iter()
            .any(|config_path| config_path.as_path() == path)
        {
            return true;
        }

        self.normal_discovery
            && path
                .parent()
                .is_some_and(|parent| project_root.starts_with(parent))
            && matches!(path.file_name(), Some("ty.toml" | "pyproject.toml"))
    }

    fn may_contain_configuration(&self, directory: &SystemPath, project_root: &SystemPath) -> bool {
        if self
            .extra
            .iter()
            .any(|config_path| config_path.starts_with(directory))
        {
            return true;
        }

        // Deleting the project root or one of its ancestors can change rediscovery:
        // ty may need to fall back to an enclosing configuration.
        self.normal_discovery && project_root.starts_with(directory)
    }
}

fn is_ignore_file(path: &SystemPath) -> bool {
    matches!(path.file_name(), Some(".gitignore" | ".ignore"))
}
