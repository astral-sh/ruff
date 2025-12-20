use crate::glob::IncludeExcludeFilter;
use crate::{Db, GlobFilterCheckMode, IOErrorDiagnostic, IOErrorKind, IncludeResult, Project};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::walk_directory::{ErrorKind, WalkDirectoryBuilder, WalkState};
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast::PySourceType;
use rustc_hash::FxHashSet;
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
}

impl<'a> ProjectFilesFilter<'a> {
    pub(crate) fn from_project(db: &'a dyn Db, project: Project) -> Self {
        Self {
            included_paths: project.included_paths_or_root(db),
            src_filter: &project.settings(db).src().files,
        }
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
                            // Exact matches are always included
                            if relative_path.as_str().is_empty() {
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
            Some(CheckPathMatch::Full) => IncludeResult::Included,
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
            Some(CheckPathMatch::Full) => IncludeResult::Included,
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

pub(crate) struct ProjectFilesWalker<'a> {
    walker: WalkDirectoryBuilder,

    filter: ProjectFilesFilter<'a>,

    force_exclude: bool,
}

impl<'a> ProjectFilesWalker<'a> {
    pub(crate) fn new(db: &'a dyn Db) -> Self {
        let project = db.project();

        let filter = ProjectFilesFilter::from_project(db, project);

        Self::from_paths(db, project.included_paths_or_root(db), filter)
            .expect("included_paths_or_root to never return an empty iterator")
    }

    /// Creates a walker for indexing the project files incrementally.
    ///
    /// The main difference to a full project walk is that `paths` may contain paths
    /// that aren't part of the included files.
    pub(crate) fn incremental<P>(db: &'a dyn Db, paths: impl IntoIterator<Item = P>) -> Option<Self>
    where
        P: AsRef<SystemPath>,
    {
        let project = db.project();

        let filter = ProjectFilesFilter::from_project(db, project);

        Self::from_paths(db, paths, filter)
    }

    fn from_paths<P>(
        db: &'a dyn Db,
        paths: impl IntoIterator<Item = P>,
        filter: ProjectFilesFilter<'a>,
    ) -> Option<Self>
    where
        P: AsRef<SystemPath>,
    {
        let mut paths = paths.into_iter();

        let mut walker = db
            .system()
            .walk_directory(paths.next()?.as_ref())
            .standard_filters(db.project().settings(db).src().respect_ignore_files)
            .ignore_hidden(false);

        for path in paths {
            walker = walker.add(path);
        }

        Some(Self {
            walker,
            filter,
            force_exclude: db.project().force_exclude(db),
        })
    }

    /// Walks the project paths and collects the paths of all files that
    /// are included in the project.
    pub(crate) fn collect_vec(self, db: &dyn Db) -> (Vec<File>, Vec<IOErrorDiagnostic>) {
        let files = std::sync::Mutex::new(Vec::new());
        let diagnostics = std::sync::Mutex::new(Vec::new());

        self.walker.run(|| {
            let db = db.dyn_clone();
            let filter = &self.filter;
            let files = &files;
            let diagnostics = &diagnostics;

            Box::new(move |entry| {
                match entry {
                    Ok(entry) => {
                        // Skip excluded directories unless they were explicitly passed to the walker
                        // (which is the case passed to `ty check <paths>`).
                        if entry.file_type().is_directory() {
                            if entry.depth() > 0 || self.force_exclude {
                                let directory_included = filter
                                    .is_directory_included(entry.path(), GlobFilterCheckMode::TopDown);
                                return match directory_included {
                                    IncludeResult::Included => WalkState::Continue,
                                    IncludeResult::Excluded => {
                                        tracing::debug!(
                                            "Skipping directory '{path}' because it is excluded by a default or `src.exclude` pattern",
                                            path=entry.path()
                                        );
                                        WalkState::Skip
                                    },
                                    IncludeResult::NotIncluded => {
                                        tracing::debug!(
                                            "Skipping directory `{path}` because it doesn't match any `src.include` pattern or path specified on the CLI",
                                            path=entry.path()
                                        );
                                        WalkState::Skip
                                    },
                                };
                            }
                        } else {
                            // Ignore any non python files to avoid creating too many entries in `Files`.
                            // Unless the file is explicitly passed, we then always assume it's a python file.
                            let source_type = entry.path().extension().and_then(PySourceType::try_from_extension).or_else(|| {
                                if entry.depth() == 0 {
                                    Some(PySourceType::Python)
                                } else {
                                    db.system().source_type(entry.path())
                                }
                            });

                            if source_type.is_none()
                            {
                                return WalkState::Continue;
                            }

                            // For all files, except the ones that were explicitly passed to the walker (CLI),
                            // check if they're included in the project.
                            if entry.depth() > 0 || self.force_exclude {
                                match filter
                                    .is_file_included(entry.path(), GlobFilterCheckMode::TopDown)
                                {
                                    IncludeResult::Included => {},
                                    IncludeResult::Excluded => {
                                        tracing::debug!(
                                            "Ignoring file `{path}` because it is excluded by a default or `src.exclude` pattern.",
                                            path=entry.path()
                                        );
                                        return WalkState::Continue;
                                    },
                                    IncludeResult::NotIncluded => {
                                        tracing::debug!(
                                            "Ignoring file `{path}` because it doesn't match any `src.include` pattern or path specified on the CLI.",
                                            path=entry.path()
                                        );
                                        return WalkState::Continue;
                                    },
                                }
                            }

                            // If this returns `Err`, then the file was deleted between now and when the walk callback was called.
                            // We can ignore this.
                            if let Ok(file) = system_path_to_file(&*db, entry.path()) {
                                files.lock().unwrap().push(file);
                            }
                        }
                    }
                    Err(error) => match error.kind() {
                        ErrorKind::Loop { .. } => {
                            unreachable!("Loops shouldn't be possible without following symlinks.")
                        }
                        ErrorKind::Io { path, err } => {
                            let mut diagnostics = diagnostics.lock().unwrap();
                            let error = if let Some(path) = path {
                                WalkError::IOPathError {
                                    path: path.clone(),
                                    error: err.to_string(),
                                }
                            } else {
                                WalkError::IOError {
                                    error: err.to_string(),
                                }
                            };

                            diagnostics.push(IOErrorDiagnostic {
                                file: None,
                                error: IOErrorKind::Walk(error),
                            });
                        }
                        ErrorKind::NonUtf8Path { path } => {
                            diagnostics.lock().unwrap().push(IOErrorDiagnostic {
                                file: None,
                                error: IOErrorKind::Walk(WalkError::NonUtf8Path {
                                    path: path.clone(),
                                }),
                            });
                        }
                    },
                }

                WalkState::Continue
            })
        });

        (
            files.into_inner().unwrap(),
            diagnostics.into_inner().unwrap(),
        )
    }

    pub(crate) fn collect_set(self, db: &dyn Db) -> (FxHashSet<File>, Vec<IOErrorDiagnostic>) {
        let (files, diagnostics) = self.collect_vec(db);
        (files.into_iter().collect(), diagnostics)
    }
}

#[derive(Error, Debug, Clone, get_size2::GetSize)]
pub(crate) enum WalkError {
    #[error("`{path}`: {error}")]
    IOPathError { path: SystemPathBuf, error: String },

    #[error("Failed to walk project directory: {error}")]
    IOError { error: String },

    #[error("`{path}` is not a valid UTF-8 path")]
    NonUtf8Path { path: PathBuf },
}
