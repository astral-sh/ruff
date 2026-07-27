use crate::glob::IncludeExcludeFilter;
use crate::metadata::script::script_metadata;
use crate::{Db, GlobFilterCheckMode, IncludeResult, Project};
use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::walk_directory::{ErrorKind, WalkDirectoryBuilder, WalkState};
use ruff_db::system::{SystemPath, SystemPathBuf, deduplicate_nested_paths};
use rustc_hash::FxHashSet;
use std::collections::BTreeSet;
use std::path::PathBuf;
use thiserror::Error;

/// Filter that decides which files are included in the project.
///
/// In the future, this will hold a reference to the `include` and `exclude` pattern.
///
/// This struct mainly exists because `dyn Db` isn't `Send` or `Sync`, making it impossible
/// to access fields from within the walker.
#[derive(Debug)]
pub(crate) struct ProjectFilesFilter<'a> {
    /// The same as [`Project::included_paths_or_root`].
    included_paths: &'a [SystemPathBuf],

    /// The resolved `src.include` and `src.exclude` filter.
    src_filter: &'a IncludeExcludeFilter,

    force_exclude: bool,
}

impl<'a> ProjectFilesFilter<'a> {
    pub(crate) fn from_project(db: &'a dyn Db, project: Project) -> Self {
        Self {
            included_paths: project.included_paths_or_root(db),
            src_filter: &project.settings(db).src().files,
            force_exclude: project.force_exclude(db),
        }
    }

    pub(crate) fn force_exclude(&self) -> bool {
        self.force_exclude
    }

    fn match_included_paths(
        &self,
        path: &SystemPath,
        mode: GlobFilterCheckMode,
    ) -> Option<CheckPathMatch> {
        match mode {
            GlobFilterCheckMode::TopDown => Some(CheckPathMatch::Partial),
            GlobFilterCheckMode::Adhoc => {
                self.included_paths
                    .iter()
                    .filter_map(|included_path| {
                        if let Ok(relative_path) = path.strip_prefix(included_path) {
                            // Exact matches are always included, unless forced to exclude
                            if relative_path.as_str().is_empty() && !self.force_exclude {
                                Some(CheckPathMatch::Full)
                            } else {
                                Some(CheckPathMatch::Partial)
                            }
                        } else {
                            None
                        }
                    })
                    .max()
            }
        }
    }

    /// Returns `true` if a file is part of the project and included in the paths to check.
    ///
    /// A file is included in the checked files if it is a sub path of the project's root
    /// (when no CLI path arguments are specified) or if it is a sub path of any path provided on the CLI (`ty check <paths>`) AND:
    ///
    /// * It matches a positive `include` pattern and isn't excluded by a later negative `include` pattern.
    /// * It doesn't match a positive `exclude` pattern or is re-included by a later negative `exclude` pattern.
    ///
    /// ## Note
    ///
    /// This method may return `true` for files that don't end up being included when walking the
    /// project tree because it doesn't consider `.gitignore` and other ignore files when deciding
    /// if a file's included.
    pub(crate) fn is_file_included(
        &self,
        path: &SystemPath,
        mode: GlobFilterCheckMode,
    ) -> IncludeResult {
        match self.match_included_paths(path, mode) {
            None => IncludeResult::NotIncluded,
            Some(CheckPathMatch::Partial) => self.src_filter.is_file_included(path, mode),
            Some(CheckPathMatch::Full) => IncludeResult::Included {
                literal_match: Some(true),
            },
        }
    }

    pub(crate) fn is_directory_included(
        &self,
        path: &SystemPath,
        mode: GlobFilterCheckMode,
    ) -> IncludeResult {
        match self.match_included_paths(path, mode) {
            None => IncludeResult::NotIncluded,
            Some(CheckPathMatch::Partial) => {
                self.src_filter.is_directory_maybe_included(path, mode)
            }
            Some(CheckPathMatch::Full) => IncludeResult::Included {
                literal_match: Some(true),
            },
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CheckPathMatch {
    /// The path is a partial match of the checked path (it's a sub path)
    Partial,

    /// The path matches a check path exactly.
    Full,
}

pub(crate) struct ProjectFilesWalker {
    /// If set, the walker only visits paths that lead to one of these paths.
    ///
    /// Otherwise, the visitor walks all paths recursively, except paths that are excluded or
    /// not included by the project.
    incremental_paths: Option<BTreeSet<SystemPathBuf>>,
}

impl ProjectFilesWalker {
    pub(crate) fn full() -> Self {
        Self {
            incremental_paths: None,
        }
    }

    /// Creates a walker for indexing newly added project files incrementally.
    pub(crate) fn incremental(paths: impl IntoIterator<Item = SystemPathBuf>) -> Self {
        Self {
            incremental_paths: Some(deduplicate_nested_paths(paths).collect()),
        }
    }

    /// Walks the project paths and collects the paths of all files that
    /// are included in the project.
    pub(crate) fn collect_vec(self, db: &dyn Db) -> (Vec<File>, Vec<Diagnostic>) {
        let project = db.project();
        let root_paths = project.included_paths_or_root(db);

        let walker = if let Some(incremental_paths) = &self.incremental_paths {
            if incremental_paths.is_empty() {
                return (Vec::new(), Vec::new());
            }

            create_walker_builder(
                db,
                root_paths.iter().filter(|root| {
                    should_visit_incremental_path(root.as_path(), incremental_paths)
                }),
            )
        } else {
            create_walker_builder(db, root_paths)
        };

        let Some(walker) = walker else {
            return (Vec::new(), Vec::new());
        };

        let filter = ProjectFilesFilter::from_project(db, project);
        let exclude_scripts = project.settings(db).src().exclude_scripts;
        let files = std::sync::Mutex::new(Vec::new());
        let diagnostics = std::sync::Mutex::new(Vec::new());

        walker.run(|| {
            let db = Db::dyn_clone(db);
            let filter = &filter;
            let incremental_paths = &self.incremental_paths;
            let files = &files;
            let diagnostics = &diagnostics;
            let force_exclude = filter.force_exclude();

            Box::new(move |entry| {
                db.unwind_if_revision_cancelled();

                match entry {
                    Ok(entry) => {
                        if incremental_paths.as_ref().is_some_and(|incremental_paths| {
                            !should_visit_incremental_path(entry.path(), incremental_paths)
                        }) {
                            return WalkState::Skip;
                        }

                        // Skip excluded directories unless they were explicitly passed to the walker
                        // (which is the case passed to `ty check <paths>`).
                        if entry.file_type().is_directory() {
                            if entry.depth() > 0 || force_exclude {
                                let directory_included = filter.is_directory_included(
                                    entry.path(),
                                    GlobFilterCheckMode::TopDown,
                                );
                                return match directory_included {
                                    IncludeResult::Included { .. } => WalkState::Continue,
                                    IncludeResult::Excluded => {
                                        tracing::debug!(
                                            "Skipping directory '{path}' because it is excluded by \
                                            a default or `src.exclude` pattern",
                                            path = entry.path()
                                        );
                                        WalkState::Skip
                                    }
                                    IncludeResult::NotIncluded => {
                                        tracing::debug!(
                                            "Skipping directory `{path}` because it doesn't match \
                                            any `src.include` pattern or path specified on the CLI",
                                            path = entry.path()
                                        );
                                        WalkState::Skip
                                    }
                                };
                            }
                        } else {
                            // For all files, except the ones that were explicitly passed to the walker (CLI),
                            // check if they're included in the project.
                            if entry.depth() > 0 || force_exclude {
                                let match_mode = if entry.depth() == 0 && force_exclude {
                                    GlobFilterCheckMode::Adhoc
                                } else {
                                    GlobFilterCheckMode::TopDown
                                };
                                match filter.is_file_included(entry.path(), match_mode) {
                                    include_result @ IncludeResult::Included { .. } => {
                                        // Ignore any non python files to avoid creating too many entries in `Files`.
                                        // Unless the file is explicitly passed on the CLI or a literal match in the `include`, we then always assume it's a file ty can analyze
                                        if entry.depth() > 0
                                            && !include_result
                                                .should_index_file(db.system(), entry.path())
                                        {
                                            return WalkState::Skip;
                                        }
                                    }
                                    IncludeResult::Excluded => {
                                        tracing::debug!(
                                            "Ignoring file `{path}` because it is excluded by \
                                            a default or `src.exclude` pattern.",
                                            path = entry.path()
                                        );
                                        return WalkState::Skip;
                                    }
                                    IncludeResult::NotIncluded => {
                                        tracing::debug!(
                                            "Ignoring file `{path}` because it doesn't match any \
                                            `src.include` pattern or path specified on the CLI.",
                                            path = entry.path()
                                        );
                                        return WalkState::Skip;
                                    }
                                }
                            }

                            // If this returns `Err`, then the file was deleted between now and when the walk callback was called.
                            // We can ignore this.
                            if let Ok(file) = system_path_to_file(&*db, entry.path()) {
                                if entry.depth() > 0
                                    && exclude_scripts
                                    && script_metadata(&*db, file).is_some()
                                {
                                    tracing::debug!(
                                        "Ignoring implicitly discovered PEP 723 script `{path}` \
                                        because `exclude-scripts` is enabled.",
                                        path = entry.path()
                                    );
                                    return WalkState::Skip;
                                }

                                files.lock().unwrap().push(file);
                            }
                        }
                    }
                    Err(error) => {
                        let error = match error.kind() {
                            ErrorKind::Loop { .. } => {
                                unreachable!(
                                    "Loops shouldn't be possible without following symlinks."
                                )
                            }
                            ErrorKind::Io { path, err } => {
                                if let Some(path) = path {
                                    WalkError::IOPathError {
                                        path: path.clone(),
                                        error: err.to_string(),
                                    }
                                } else {
                                    WalkError::IOError {
                                        error: err.to_string(),
                                    }
                                }
                            }
                            ErrorKind::NonUtf8Path { path } => {
                                WalkError::NonUtf8Path { path: path.clone() }
                            }
                        };

                        diagnostics.lock().unwrap().push(error.to_diagnostic());
                    }
                }

                WalkState::Continue
            })
        });

        (
            files.into_inner().unwrap(),
            diagnostics.into_inner().unwrap(),
        )
    }

    pub(crate) fn collect_set(self, db: &dyn Db) -> (FxHashSet<File>, Vec<Diagnostic>) {
        let (files, diagnostics) = self.collect_vec(db);
        (files.into_iter().collect(), diagnostics)
    }
}

pub(crate) fn create_walker_builder<I, T>(db: &dyn Db, paths: I) -> Option<WalkDirectoryBuilder>
where
    I: IntoIterator<Item = T>,
    T: AsRef<SystemPath>,
{
    let mut paths = paths.into_iter();

    let mut builder = db
        .system()
        .walk_directory(paths.next()?.as_ref())
        .standard_filters(db.project().settings(db).src().respect_ignore_files)
        .ignore_hidden(false);

    for path in paths {
        builder = builder.add(path);
    }

    Some(builder)
}

fn should_visit_incremental_path(
    path: &SystemPath,
    incremental_paths: &BTreeSet<SystemPathBuf>,
) -> bool {
    let incremental_path_is_ancestor = incremental_paths
        .range(..=path.to_path_buf())
        .next_back()
        .is_some_and(|incremental_path| path.starts_with(incremental_path));
    let incremental_path_is_descendant = incremental_paths
        .range(path.to_path_buf()..)
        .next()
        .is_some_and(|incremental_path| incremental_path.starts_with(path));

    incremental_path_is_ancestor || incremental_path_is_descendant
}

#[derive(Error, Debug, Clone, get_size2::GetSize)]
enum WalkError {
    #[error("`{path}`: {error}")]
    IOPathError { path: SystemPathBuf, error: String },

    #[error("Failed to walk project directory: {error}")]
    IOError { error: String },

    #[error("`{path}` is not a valid UTF-8 path")]
    NonUtf8Path { path: PathBuf },
}
impl WalkError {
    fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic::new(DiagnosticId::Io, Severity::Error, self)
    }
}
