use crate::{Db, IOErrorDiagnostic, IOErrorKind, Project};
use ruff_db::files::{system_path_to_file, File};
use ruff_db::system::walk_directory::{ErrorKind, WalkDirectoryBuilder, WalkState};
use ruff_db::system::{FileType, SystemPath, SystemPathBuf};
use ruff_python_ast::PySourceType;
use rustc_hash::{FxBuildHasher, FxHashSet};
use std::path::PathBuf;
use thiserror::Error;

/// Filter that decides which files are included in the project.
///
/// In the future, this will hold a reference to the `include` and `exclude` pattern.
///
/// This struct mainly exists because `dyn Db` isn't `Send` or `Sync`, making it impossible
/// to access fields from within the walker.
#[derive(Default, Debug)]
pub(crate) struct ProjectFilesFilter<'a> {
    /// The same as [`Project::included_paths_or_root`].
    included_paths: &'a [SystemPathBuf],

    /// The filter skips checking if the path is in `included_paths` if set to `true`.
    ///
    /// Skipping this check is useful when the walker only walks over `included_paths`.
    skip_included_paths: bool,
}

impl<'a> ProjectFilesFilter<'a> {
    pub(crate) fn from_project(db: &'a dyn Db, project: Project) -> Self {
        Self {
            included_paths: project.included_paths_or_root(db),
            skip_included_paths: false,
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
    pub(crate) fn is_included(&self, path: &SystemPath) -> bool {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum CheckPathMatch {
            /// The path is a partial match of the checked path (it's a sub path)
            Partial,

            /// The path matches a check path exactly.
            Full,
        }

        let m = if self.skip_included_paths {
            Some(CheckPathMatch::Partial)
        } else {
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
        };

        match m {
            None => false,
            Some(CheckPathMatch::Partial) => {
                // TODO: For partial matches, only include the file if it is included by the project's include/exclude settings.
                true
            }
            Some(CheckPathMatch::Full) => true,
        }
    }
}

pub(crate) struct ProjectFilesWalker<'a> {
    walker: WalkDirectoryBuilder,

    filter: ProjectFilesFilter<'a>,
}

impl<'a> ProjectFilesWalker<'a> {
    pub(crate) fn new(db: &'a dyn Db) -> Self {
        let project = db.project();

        let mut filter = ProjectFilesFilter::from_project(db, project);
        // It's unnecessary to filter on included paths because it only iterates over those to start with.
        filter.skip_included_paths = true;

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
            .standard_filters(db.project().settings(db).respect_ignore_files())
            .ignore_hidden(false);

        for path in paths {
            walker = walker.add(path);
        }

        Some(Self { walker, filter })
    }

    /// Walks the project paths and collects the paths of all files that
    /// are included in the project.
    pub(crate) fn walk_paths(self) -> (Vec<SystemPathBuf>, Vec<IOErrorDiagnostic>) {
        let paths = std::sync::Mutex::new(Vec::new());
        let diagnostics = std::sync::Mutex::new(Vec::new());

        self.walker.run(|| {
            Box::new(|entry| {
                match entry {
                    Ok(entry) => {
                        if !self.filter.is_included(entry.path()) {
                            tracing::debug!("Ignoring not-included path: {}", entry.path());
                            return WalkState::Skip;
                        }

                        // Skip over any non python files to avoid creating too many entries in `Files`.
                        match entry.file_type() {
                            FileType::File => {
                                if entry
                                    .path()
                                    .extension()
                                    .and_then(PySourceType::try_from_extension)
                                    .is_some()
                                {
                                    let mut paths = paths.lock().unwrap();
                                    paths.push(entry.into_path());
                                }
                            }
                            FileType::Directory | FileType::Symlink => {}
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
            paths.into_inner().unwrap(),
            diagnostics.into_inner().unwrap(),
        )
    }

    pub(crate) fn collect_vec(self, db: &dyn Db) -> (Vec<File>, Vec<IOErrorDiagnostic>) {
        let (paths, diagnostics) = self.walk_paths();

        (
            paths
                .into_iter()
                .filter_map(move |path| {
                    // If this returns `None`, then the file was deleted between the `walk_directory` call and now.
                    // We can ignore this.
                    system_path_to_file(db.upcast(), &path).ok()
                })
                .collect(),
            diagnostics,
        )
    }

    pub(crate) fn collect_set(self, db: &dyn Db) -> (FxHashSet<File>, Vec<IOErrorDiagnostic>) {
        let (paths, diagnostics) = self.walk_paths();

        let mut files = FxHashSet::with_capacity_and_hasher(paths.len(), FxBuildHasher);

        for path in paths {
            if let Ok(file) = system_path_to_file(db.upcast(), &path) {
                files.insert(file);
            }
        }

        (files, diagnostics)
    }
}

#[derive(Error, Debug, Clone)]
pub(crate) enum WalkError {
    #[error("`{path}`: {error}")]
    IOPathError { path: SystemPathBuf, error: String },

    #[error("Failed to walk project directory: {error}")]
    IOError { error: String },

    #[error("`{path}` is not a valid UTF-8 path")]
    NonUtf8Path { path: PathBuf },
}
