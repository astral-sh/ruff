use std::{collections::BTreeMap, sync::Arc};

use rustc_hash::{FxBuildHasher, FxHashSet};
use salsa::{Durability, Setter as _};

pub use metadata::{PackageMetadata, WorkspaceMetadata};
use ruff_db::source::{source_text, SourceDiagnostic};
use ruff_db::{
    files::{system_path_to_file, File},
    system::{walk_directory::WalkState, SystemPath, SystemPathBuf},
};
use ruff_python_ast::{name::Name, PySourceType};

use crate::workspace::files::{Index, IndexedFiles, PackageFiles};
use crate::{
    db::Db,
    lint::{lint_semantic, lint_syntax, Diagnostics},
};

mod files;
mod metadata;

/// The project workspace as a Salsa ingredient.
///
/// A workspace consists of one or multiple packages. Packages can be nested. A file in a workspace
/// belongs to no or exactly one package (files can't belong to multiple packages).
///
/// How workspaces and packages are discovered is TBD. For now, a workspace can be any directory,
/// and it always contains a single package which has the same root as the workspace.
///
/// ## Examples
///
/// ```text
/// app-1/
///     pyproject.toml
///     src/
///         ... python files
///
/// app-2/
///     pyproject.toml
///     src/
///         ... python files
///
/// shared/
///     pyproject.toml
///     src/
///         ... python files
///
/// pyproject.toml
/// ```
///
/// The above project structure has three packages: `app-1`, `app-2`, and `shared`.
/// Each of the packages can define their own settings in their `pyproject.toml` file, but
/// they must be compatible. For example, each package can define a different `requires-python` range,
/// but the ranges must overlap.
///
/// ## How is a workspace different from a program?
/// There are two (related) motivations:
///
/// 1. Program is defined in `ruff_db` and it can't reference the settings types for the linter and formatter
///    without introducing a cyclic dependency. The workspace is defined in a higher level crate
///    where it can reference these setting types.
/// 2. Running `ruff check` with different target versions results in different programs (settings) but
///    it remains the same workspace. That's why program is a narrowed view of the workspace only
///    holding on to the most fundamental settings required for checking.
#[salsa::input]
pub struct Workspace {
    #[return_ref]
    root_buf: SystemPathBuf,

    /// The files that are open in the workspace.
    ///
    /// Setting the open files to a non-`None` value changes `check` to only check the
    /// open files rather than all files in the workspace.
    #[return_ref]
    #[default]
    open_fileset: Option<Arc<FxHashSet<File>>>,

    /// The (first-party) packages in this workspace.
    #[return_ref]
    package_tree: BTreeMap<SystemPathBuf, Package>,
}

/// A first-party package in a workspace.
#[salsa::input]
pub struct Package {
    #[return_ref]
    pub name: Name,

    /// The path to the root directory of the package.
    #[return_ref]
    root_buf: SystemPathBuf,

    /// The files that are part of this package.
    #[return_ref]
    #[default]
    file_set: PackageFiles,
    // TODO: Add the loaded settings.
}

impl Workspace {
    /// Discovers the closest workspace at `path` and returns its metadata.
    pub fn from_metadata(db: &dyn Db, metadata: WorkspaceMetadata) -> Self {
        let mut packages = BTreeMap::new();

        for package in metadata.packages {
            packages.insert(package.root.clone(), Package::from_metadata(db, package));
        }

        Workspace::builder(metadata.root, packages)
            .durability(Durability::MEDIUM)
            .open_fileset_durability(Durability::LOW)
            .new(db)
    }

    pub fn root(self, db: &dyn Db) -> &SystemPath {
        self.root_buf(db)
    }

    pub fn packages(self, db: &dyn Db) -> impl Iterator<Item = Package> + '_ {
        self.package_tree(db).values().copied()
    }

    pub fn reload(self, db: &mut dyn Db, metadata: WorkspaceMetadata) {
        tracing::debug!("Reloading workspace");
        assert_eq!(self.root(db), metadata.root());

        let mut old_packages = self.package_tree(db).clone();
        let mut new_packages = BTreeMap::new();

        for package_metadata in metadata.packages {
            let path = package_metadata.root().to_path_buf();

            let package = if let Some(old_package) = old_packages.remove(&path) {
                old_package.update(db, package_metadata);
                old_package
            } else {
                Package::from_metadata(db, package_metadata)
            };

            new_packages.insert(path, package);
        }

        self.set_package_tree(db)
            .with_durability(Durability::MEDIUM)
            .to(new_packages);
    }

    pub fn update_package(self, db: &mut dyn Db, metadata: PackageMetadata) -> anyhow::Result<()> {
        let path = metadata.root().to_path_buf();

        if let Some(package) = self.package_tree(db).get(&path).copied() {
            package.update(db, metadata);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Package {path} not found"))
        }
    }

    /// Returns the closest package to which the first-party `path` belongs.
    ///
    /// Returns `None` if the `path` is outside of any package or if `file` isn't a first-party file
    /// (e.g. third-party dependencies or `excluded`).
    pub fn package(self, db: &dyn Db, path: &SystemPath) -> Option<Package> {
        let packages = self.package_tree(db);

        let (package_path, package) = packages.range(..=path.to_path_buf()).next_back()?;

        if path.starts_with(package_path) {
            Some(*package)
        } else {
            None
        }
    }

    /// Checks all open files in the workspace and its dependencies.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn check(self, db: &dyn Db) -> Vec<String> {
        tracing::debug!("Checking workspace");

        let mut result = Vec::new();

        if let Some(open_files) = self.open_files(db) {
            for file in open_files {
                result.extend_from_slice(&check_file(db, *file));
            }
        } else {
            for package in self.packages(db) {
                result.extend(package.check(db));
            }
        }

        result
    }

    /// Opens a file in the workspace.
    ///
    /// This changes the behavior of `check` to only check the open files rather than all files in the workspace.
    pub fn open_file(self, db: &mut dyn Db, file: File) {
        tracing::debug!("Opening file {}", file.path(db));

        let mut open_files = self.take_open_files(db);
        open_files.insert(file);
        self.set_open_files(db, open_files);
    }

    /// Closes a file in the workspace.
    pub fn close_file(self, db: &mut dyn Db, file: File) -> bool {
        tracing::debug!("Closing file {}", file.path(db));

        let mut open_files = self.take_open_files(db);
        let removed = open_files.remove(&file);

        if removed {
            self.set_open_files(db, open_files);
        }

        removed
    }

    /// Returns the open files in the workspace or `None` if the entire workspace should be checked.
    pub fn open_files(self, db: &dyn Db) -> Option<&FxHashSet<File>> {
        self.open_fileset(db).as_deref()
    }

    /// Sets the open files in the workspace.
    ///
    /// This changes the behavior of `check` to only check the open files rather than all files in the workspace.
    #[tracing::instrument(level = "debug", skip(self, db))]
    pub fn set_open_files(self, db: &mut dyn Db, open_files: FxHashSet<File>) {
        tracing::debug!("Set open workspace files (count: {})", open_files.len());

        self.set_open_fileset(db).to(Some(Arc::new(open_files)));
    }

    /// This takes the open files from the workspace and returns them.
    ///
    /// This changes the behavior of `check` to check all files in the workspace instead of just the open files.
    pub fn take_open_files(self, db: &mut dyn Db) -> FxHashSet<File> {
        tracing::debug!("Take open workspace files");

        // Salsa will cancel any pending queries and remove its own reference to `open_files`
        // so that the reference counter to `open_files` now drops to 1.
        let open_files = self.set_open_fileset(db).to(None);

        if let Some(open_files) = open_files {
            Arc::try_unwrap(open_files).unwrap()
        } else {
            FxHashSet::default()
        }
    }
}

#[salsa::tracked]
impl Package {
    pub fn root(self, db: &dyn Db) -> &SystemPath {
        self.root_buf(db)
    }

    /// Returns `true` if `file` is a first-party file part of this package.
    pub fn contains_file(self, db: &dyn Db, file: File) -> bool {
        self.files(db).read().contains(&file)
    }

    #[tracing::instrument(level = "debug", skip(db))]
    pub fn remove_file(self, db: &mut dyn Db, file: File) {
        tracing::debug!(
            "Remove file {} from package {}",
            file.path(db),
            self.name(db)
        );

        let Some(mut index) = PackageFiles::indexed_mut(db, self) else {
            return;
        };

        index.remove(file);
    }

    pub fn add_file(self, db: &mut dyn Db, file: File) {
        tracing::debug!("Add file {} to package {}", file.path(db), self.name(db));

        let Some(mut index) = PackageFiles::indexed_mut(db, self) else {
            return;
        };

        index.insert(file);
    }

    #[tracing::instrument(level = "debug", skip(db))]
    pub(crate) fn check(self, db: &dyn Db) -> Vec<String> {
        tracing::debug!("Checking package {}", self.root(db));

        let mut result = Vec::new();
        for file in &self.files(db).read() {
            let diagnostics = check_file(db, file);
            result.extend_from_slice(&diagnostics);
        }

        result
    }

    /// Returns the files belonging to this package.
    #[salsa::tracked]
    pub fn files(self, db: &dyn Db) -> IndexedFiles {
        let _entered = tracing::debug_span!("files").entered();
        let files = self.file_set(db);

        let indexed = match files.get() {
            Index::Lazy(vacant) => {
                tracing::debug!("Indexing files for package {}", self.name(db));
                let files = discover_package_files(db, self.root(db));
                vacant.set(files)
            }
            Index::Indexed(indexed) => indexed,
        };

        indexed
    }

    fn from_metadata(db: &dyn Db, metadata: PackageMetadata) -> Self {
        Self::builder(metadata.name, metadata.root)
            .durability(Durability::MEDIUM)
            .file_set_durability(Durability::LOW)
            .new(db)
    }

    fn update(self, db: &mut dyn Db, metadata: PackageMetadata) {
        let root = self.root(db);
        assert_eq!(root, metadata.root());

        if self.name(db) != metadata.name() {
            self.set_name(db)
                .with_durability(Durability::MEDIUM)
                .to(metadata.name);
        }
    }

    pub fn reload_files(self, db: &mut dyn Db) {
        tracing::debug!("Reload files for package {}", self.name(db));

        if !self.file_set(db).is_lazy() {
            // Force a re-index of the files in the next revision.
            self.set_file_set(db).to(PackageFiles::lazy());
        }
    }
}

#[salsa::tracked]
pub(super) fn check_file(db: &dyn Db, file: File) -> Diagnostics {
    let path = file.path(db);
    let _span = tracing::debug_span!("check_file", file=%path).entered();
    tracing::debug!("Checking file {path}");

    let mut diagnostics = Vec::new();

    let source_diagnostics = source_text::accumulated::<SourceDiagnostic>(db.upcast(), file);
    // TODO(micha): Consider using a single accumulator for all diagnostics
    diagnostics.extend(
        source_diagnostics
            .iter()
            .map(std::string::ToString::to_string),
    );

    // Abort checking if there are IO errors.
    if source_text(db.upcast(), file).has_read_error() {
        return Diagnostics::from(diagnostics);
    }

    diagnostics.extend_from_slice(lint_syntax(db, file));
    diagnostics.extend_from_slice(lint_semantic(db, file));
    Diagnostics::from(diagnostics)
}

fn discover_package_files(db: &dyn Db, path: &SystemPath) -> FxHashSet<File> {
    let paths = std::sync::Mutex::new(Vec::new());

    db.system().walk_directory(path).run(|| {
        Box::new(|entry| {
            match entry {
                Ok(entry) => {
                    // Skip over any non python files to avoid creating too many entries in `Files`.
                    if entry.file_type().is_file()
                        && entry
                            .path()
                            .extension()
                            .and_then(PySourceType::try_from_extension)
                            .is_some()
                    {
                        let mut paths = paths.lock().unwrap();
                        paths.push(entry.into_path());
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

    let paths = paths.into_inner().unwrap();
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

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithTestSystem, SystemPath};
    use ruff_db::testing::assert_function_query_was_not_run;

    use crate::db::tests::TestDb;
    use crate::lint::{lint_syntax, Diagnostics};
    use crate::workspace::check_file;

    #[test]
    fn check_file_skips_linting_when_file_cant_be_read() -> ruff_db::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10")?;
        let file = system_path_to_file(&db, path).unwrap();

        // Now the file gets deleted before we had a chance to read its source text.
        db.memory_file_system().remove_file(path)?;
        file.sync(&mut db);

        assert_eq!(source_text(&db, file).as_str(), "");
        assert_eq!(
            check_file(&db, file),
            Diagnostics::List(vec![
                "Failed to read file: No such file or directory".to_string()
            ])
        );

        let events = db.take_salsa_events();
        assert_function_query_was_not_run(&db, lint_syntax, file, &events);

        // The user now creates a new file with an empty text. The source text
        // content returned by `source_text` remains unchanged, but the diagnostics should get updated.
        db.write_file(path, "").unwrap();

        assert_eq!(source_text(&db, file).as_str(), "");
        assert_eq!(check_file(&db, file), Diagnostics::Empty);

        Ok(())
    }
}
