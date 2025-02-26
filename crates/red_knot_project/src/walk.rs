use crate::Db;
use ruff_db::files::{system_path_to_file, File};
use ruff_db::system::walk_directory::WalkState;
use ruff_db::system::{FileType, System, SystemPath, SystemPathBuf};
use ruff_python_ast::PySourceType;
use rustc_hash::{FxBuildHasher, FxHashSet};

#[derive(Default, Debug)]
pub(crate) struct IncludeFilter<'a> {
    /// Paths that are explicitly included.
    explicitly_included: &'a [SystemPathBuf],
}

impl IncludeFilter<'_> {
    fn is_included(&self, path: &SystemPath) -> bool {
        self.explicitly_included
            .iter()
            .any(|included| path.starts_with(included))
    }
}

pub(crate) struct ProjectFilesWalker<'a> {
    system: &'a dyn System,
    roots: &'a [SystemPathBuf],
    filter: IncludeFilter<'a>,
}

impl<'a> ProjectFilesWalker<'a> {
    pub(crate) fn new(system: &'a dyn System, roots: &'a [SystemPathBuf]) -> Self {
        Self {
            system,
            roots,
            filter: IncludeFilter::default(),
        }
    }

    /// Walks the project paths and collects the paths of all files that
    /// are included in the project.
    pub(crate) fn walk(self) -> Vec<SystemPathBuf> {
        let Some((first, rest)) = self.roots.split_first() else {
            return Vec::new();
        };

        let mut walker = self.system.walk_directory(first);

        for path in rest {
            walker = walker.add(path);
        }

        let paths = std::sync::Mutex::new(Vec::new());
        walker.run(|| {
            Box::new(|entry| {
                match entry {
                    Ok(entry) => {
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

    pub(crate) fn into_files_set(self, db: &dyn Db) -> FxHashSet<File> {
        let paths = self.walk();

        let mut files = FxHashSet::with_capacity_and_hasher(paths.len(), FxBuildHasher);

        for path in paths {
            // If this returns `None`, then the file was deleted between the `walk_directory` call and now.
            // We can ignore this.
            if let Ok(file) = system_path_to_file(db.upcast(), &path) {
                files.insert(file);
            }
        }

        files
    }
}
