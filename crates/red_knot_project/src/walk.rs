use crate::{Db, Project};
use ruff_db::files::{system_path_to_file, File};
use ruff_db::system::walk_directory::WalkState;
use ruff_db::system::{FileType, System, SystemPath, SystemPathBuf};
use ruff_python_ast::PySourceType;
use rustc_hash::{FxBuildHasher, FxHashSet};

/// Filter that decides which files are included in the project.
///
/// In the future, this will hold a reference to the `include` and `exclude` pattern.
#[derive(Default, Debug)]
pub(crate) struct ProjectFilesFilter<'a> {
    /// A copy of [`Project::check_paths_or_root`] because we can't access the db from the walker.
    check_paths: &'a [SystemPathBuf],
}

impl<'a> ProjectFilesFilter<'a> {
    pub(crate) fn from_project(db: &'a dyn Db, project: Project) -> Self {
        Self {
            check_paths: project.check_paths_or_root(db),
        }
    }

    /// Returns `true` if a file is included according to the project's `include` and `exclude` settings
    /// and the paths specified on the CLI.
    ///
    /// A file is considered being part of the project if it is a sub path of the project's root
    /// (when no CLI path arguments are specified) or if it is a sub path of one of the CLI paths AND:
    ///
    /// * It is considered included after applying all `include` patterns
    /// * It isn't excluded by any `exclude` pattern
    ///
    /// ## Note
    ///
    /// This method may return `true` for files that don't end up being included when walking the
    /// project tree because it doesn't consider `.gitignore` and other ignore files.
    pub(crate) fn is_included(&self, path: &SystemPath) -> bool {
        #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum CheckPathMatch {
            /// The path is a partial match of the checked path (it's a sub path)
            Partial,

            /// The path matches a check path exactly.
            Full,
        }

        let m = self
            .check_paths
            .iter()
            .filter_map(|check_path| {
                if let Ok(relative_path) = check_path.strip_prefix(path) {
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
            .max();

        match m {
            None => false,
            Some(CheckPathMatch::Partial) => {
                // TODO: Respect `exclude` and `include` settings
                true
            }
            Some(CheckPathMatch::Full) => true,
        }
    }
}

pub(crate) struct ProjectFilesWalker<'a> {
    system: &'a dyn System,

    /// The paths that should be searched for new project files.
    ///
    /// The walker traverses the files recursively but doesn't follow symlinks.
    /// This is consistent with Ruff's behavior. Lifting the symlink restriction
    /// would be somewhat involved because we then need a different way to identify
    /// if a path is part of a project because simply testing if the path
    /// starts with the project root would not work anymore (for files in a symlinked directory).
    paths: &'a [SystemPathBuf],

    filter: ProjectFilesFilter<'a>,
}

impl<'a> ProjectFilesWalker<'a> {
    pub(crate) fn new(db: &'a dyn Db) -> Self {
        let project = db.project();

        Self::new_with_paths(db, project.check_paths_or_root(db))
    }

    pub(crate) fn new_with_paths(db: &'a dyn Db, paths: &'a [SystemPathBuf]) -> Self {
        let project = db.project();

        let filter = ProjectFilesFilter::from_project(db, project);

        Self {
            system: db.system(),
            paths,
            filter,
        }
    }

    /// Walks the project paths and collects the paths of all files that
    /// are included in the project.
    pub(crate) fn walk_paths(self) -> Vec<SystemPathBuf> {
        let Some((first, rest)) = self.paths.split_first() else {
            return Vec::new();
        };

        // TODO: Filter out paths that are not included in the project or don't exist?
        let mut walker = self.system.walk_directory(first);

        for path in rest {
            walker = walker.add(path);
        }

        let paths = std::sync::Mutex::new(Vec::new());
        walker.run(|| {
            Box::new(|entry| {
                match entry {
                    Ok(entry) => {
                        if !self.filter.is_included(entry.path()) {
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
                    Err(error) => {
                        // TODO Handle error
                        tracing::error!("Failed to walk path: {error}");
                    }
                }

                WalkState::Continue
            })
        });

        paths.into_inner().unwrap()
    }

    pub(crate) fn into_files_iter<'db>(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = File> + use<'db> {
        let paths = self.walk_paths();

        paths.into_iter().filter_map(move |path| {
            // If this returns `None`, then the file was deleted between the `walk_directory` call and now.
            // We can ignore this.
            system_path_to_file(db.upcast(), &path).ok()
        })
    }

    pub(crate) fn into_files_set(self, db: &dyn Db) -> FxHashSet<File> {
        let paths = self.walk_paths();

        let mut files = FxHashSet::with_capacity_and_hasher(paths.len(), FxBuildHasher);

        for path in paths {
            if let Ok(file) = system_path_to_file(db.upcast(), &path) {
                files.insert(file);
            }
        }

        files
    }
}
