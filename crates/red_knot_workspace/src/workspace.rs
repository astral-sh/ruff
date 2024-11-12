use rustc_hash::{FxBuildHasher, FxHashSet};
use salsa::{Durability, Setter as _};
use std::borrow::Cow;
use std::{collections::BTreeMap, sync::Arc};

use crate::db::Db;
use crate::db::RootDatabase;
use crate::workspace::files::{Index, Indexed, IndexedIter, PackageFiles};
pub use metadata::{PackageMetadata, WorkspaceMetadata};
use red_knot_python_semantic::types::check_types;
use red_knot_python_semantic::SearchPathSettings;
use ruff_db::diagnostic::{Diagnostic, ParseDiagnostic, Severity};
use ruff_db::parsed::parsed_module;
use ruff_db::source::{source_text, SourceTextError};
use ruff_db::{
    files::{system_path_to_file, File},
    system::{walk_directory::WalkState, SystemPath, SystemPathBuf},
};
use ruff_python_ast::{name::Name, PySourceType};
use ruff_text_size::TextRange;

mod files;
mod metadata;
mod pyproject;
pub mod settings;

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

    /// The unresolved search path configuration.
    #[return_ref]
    pub search_path_settings: SearchPathSettings,
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
    #[default]
    #[return_ref]
    file_set: PackageFiles,
    // TODO: Add the loaded settings.
}

impl Workspace {
    pub fn from_metadata(db: &dyn Db, metadata: WorkspaceMetadata) -> Self {
        let mut packages = BTreeMap::new();

        for package in metadata.packages {
            packages.insert(package.root.clone(), Package::from_metadata(db, package));
        }

        let program_settings = metadata.settings.program;

        Workspace::builder(metadata.root, packages, program_settings.search_paths)
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

        // if &metadata.settings.program.search_paths != self.search_path_settings(db) {
        //     self.set_search_path_settings(db)
        //         .to(metadata.settings.program.search_paths);
        // }

        self.set_package_tree(db).to(new_packages);
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
    pub fn check(self, db: &RootDatabase) -> Vec<Box<dyn Diagnostic>> {
        let workspace_span = tracing::debug_span!("check_workspace");
        let _span = workspace_span.enter();

        tracing::debug!("Checking workspace");
        let files = WorkspaceFiles::new(db, self);
        let result = Arc::new(std::sync::Mutex::new(Vec::new()));
        let inner_result = Arc::clone(&result);

        let db = db.snapshot();
        let workspace_span = workspace_span.clone();

        rayon::scope(move |scope| {
            for file in &files {
                let result = inner_result.clone();
                let db = db.snapshot();
                let workspace_span = workspace_span.clone();

                scope.spawn(move |_| {
                    let check_file_span = tracing::debug_span!(parent: &workspace_span, "check_file", file=%file.path(&db));
                    let _entered = check_file_span.entered();

                    let file_diagnostics = check_file(&db, file);
                    result.lock().unwrap().extend(file_diagnostics);
                });
            }
        });

        Arc::into_inner(result).unwrap().into_inner().unwrap()
    }

    /// Opens a file in the workspace.
    ///
    /// This changes the behavior of `check` to only check the open files rather than all files in the workspace.
    pub fn open_file(self, db: &mut dyn Db, file: File) {
        tracing::debug!("Opening file `{}`", file.path(db));

        let mut open_files = self.take_open_files(db);
        open_files.insert(file);
        self.set_open_files(db, open_files);
    }

    /// Closes a file in the workspace.
    pub fn close_file(self, db: &mut dyn Db, file: File) -> bool {
        tracing::debug!("Closing file `{}`", file.path(db));

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

    /// Returns `true` if the file is open in the workspace.
    ///
    /// A file is considered open when:
    /// * explicitly set as an open file using [`open_file`](Self::open_file)
    /// * It has a [`SystemPath`] and belongs to a package's `src` files
    /// * It has a [`SystemVirtualPath`](ruff_db::system::SystemVirtualPath)
    pub fn is_file_open(self, db: &dyn Db, file: File) -> bool {
        if let Some(open_files) = self.open_files(db) {
            open_files.contains(&file)
        } else if let Some(system_path) = file.path(db).as_system_path() {
            self.package(db, system_path)
                .map_or(false, |package| package.contains_file(db, file))
        } else {
            file.path(db).is_system_virtual_path()
        }
    }
}

impl Package {
    pub fn root(self, db: &dyn Db) -> &SystemPath {
        self.root_buf(db)
    }

    /// Returns `true` if `file` is a first-party file part of this package.
    pub fn contains_file(self, db: &dyn Db, file: File) -> bool {
        self.files(db).contains(&file)
    }

    #[tracing::instrument(level = "debug", skip(db))]
    pub fn remove_file(self, db: &mut dyn Db, file: File) {
        tracing::debug!(
            "Removing file `{}` from package `{}`",
            file.path(db),
            self.name(db)
        );

        let Some(mut index) = PackageFiles::indexed_mut(db, self) else {
            return;
        };

        index.remove(file);
    }

    pub fn add_file(self, db: &mut dyn Db, file: File) {
        tracing::debug!(
            "Adding file `{}` to package `{}`",
            file.path(db),
            self.name(db)
        );

        let Some(mut index) = PackageFiles::indexed_mut(db, self) else {
            return;
        };

        index.insert(file);
    }

    /// Returns the files belonging to this package.
    pub fn files(self, db: &dyn Db) -> Indexed<'_> {
        let files = self.file_set(db);

        let indexed = match files.get() {
            Index::Lazy(vacant) => {
                let _entered =
                    tracing::debug_span!("index_package_files", package = %self.name(db)).entered();

                let files = discover_package_files(db, self.root(db));
                tracing::info!("Found {} files in package `{}`", files.len(), self.name(db));
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
            self.set_name(db).to(metadata.name);
        }
    }

    pub fn reload_files(self, db: &mut dyn Db) {
        tracing::debug!("Reloading files for package `{}`", self.name(db));

        if !self.file_set(db).is_lazy() {
            // Force a re-index of the files in the next revision.
            self.set_file_set(db).to(PackageFiles::lazy());
        }
    }
}

pub(super) fn check_file(db: &dyn Db, file: File) -> Vec<Box<dyn Diagnostic>> {
    let mut diagnostics: Vec<Box<dyn Diagnostic>> = Vec::new();
    // Abort checking if there are IO errors.
    let source = source_text(db.upcast(), file);

    if let Some(read_error) = source.read_error() {
        diagnostics.push(Box::new(IOErrorDiagnostic {
            file,
            error: read_error.clone(),
        }));
        return diagnostics;
    }

    let parsed = parsed_module(db.upcast(), file);
    diagnostics.extend(parsed.errors().iter().map(|error| {
        let diagnostic: Box<dyn Diagnostic> = Box::new(ParseDiagnostic::new(file, error.clone()));
        diagnostic
    }));

    diagnostics.extend(check_types(db.upcast(), file).iter().map(|diagnostic| {
        let boxed: Box<dyn Diagnostic> = Box::new(diagnostic.clone());
        boxed
    }));

    diagnostics.sort_unstable_by_key(|diagnostic| diagnostic.range().unwrap_or_default().start());

    diagnostics
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

#[derive(Debug)]
enum WorkspaceFiles<'a> {
    OpenFiles(&'a FxHashSet<File>),
    PackageFiles(Vec<Indexed<'a>>),
}

impl<'a> WorkspaceFiles<'a> {
    fn new(db: &'a dyn Db, workspace: Workspace) -> Self {
        if let Some(open_files) = workspace.open_files(db) {
            WorkspaceFiles::OpenFiles(open_files)
        } else {
            WorkspaceFiles::PackageFiles(
                workspace
                    .packages(db)
                    .map(|package| package.files(db))
                    .collect(),
            )
        }
    }
}

impl<'a> IntoIterator for &'a WorkspaceFiles<'a> {
    type Item = File;
    type IntoIter = WorkspaceFilesIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            WorkspaceFiles::OpenFiles(files) => WorkspaceFilesIter::OpenFiles(files.iter()),
            WorkspaceFiles::PackageFiles(package_files) => {
                let mut package_files = package_files.iter();
                WorkspaceFilesIter::PackageFiles {
                    current: package_files.next().map(IntoIterator::into_iter),
                    package_files,
                }
            }
        }
    }
}

enum WorkspaceFilesIter<'db> {
    OpenFiles(std::collections::hash_set::Iter<'db, File>),
    PackageFiles {
        package_files: std::slice::Iter<'db, Indexed<'db>>,
        current: Option<IndexedIter<'db>>,
    },
}

impl Iterator for WorkspaceFilesIter<'_> {
    type Item = File;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            WorkspaceFilesIter::OpenFiles(files) => files.next().copied(),
            WorkspaceFilesIter::PackageFiles {
                package_files,
                current,
            } => loop {
                if let Some(file) = current.as_mut().and_then(Iterator::next) {
                    return Some(file);
                }

                *current = Some(package_files.next()?.into_iter());
            },
        }
    }
}

#[derive(Debug)]
pub struct IOErrorDiagnostic {
    file: File,
    error: SourceTextError,
}

impl Diagnostic for IOErrorDiagnostic {
    fn rule(&self) -> &str {
        "io"
    }

    fn message(&self) -> Cow<str> {
        self.error.to_string().into()
    }

    fn file(&self) -> File {
        self.file
    }

    fn range(&self) -> Option<TextRange> {
        None
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }
}

#[cfg(test)]
mod tests {
    use crate::db::tests::TestDb;
    use crate::workspace::check_file;
    use red_knot_python_semantic::types::check_types;
    use ruff_db::diagnostic::Diagnostic;
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithTestSystem, SystemPath};
    use ruff_db::testing::assert_function_query_was_not_run;

    #[test]
    fn check_file_skips_type_checking_when_file_cant_be_read() -> ruff_db::system::Result<()> {
        let mut db = TestDb::new();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10")?;
        let file = system_path_to_file(&db, path).unwrap();

        // Now the file gets deleted before we had a chance to read its source text.
        db.memory_file_system().remove_file(path)?;
        file.sync(&mut db);

        assert_eq!(source_text(&db, file).as_str(), "");
        assert_eq!(
            check_file(&db, file)
                .into_iter()
                .map(|diagnostic| diagnostic.message().into_owned())
                .collect::<Vec<_>>(),
            vec!["Failed to read file: No such file or directory".to_string()]
        );

        let events = db.take_salsa_events();
        assert_function_query_was_not_run(&db, check_types, file, &events);

        // The user now creates a new file with an empty text. The source text
        // content returned by `source_text` remains unchanged, but the diagnostics should get updated.
        db.write_file(path, "").unwrap();

        assert_eq!(source_text(&db, file).as_str(), "");
        assert_eq!(
            check_file(&db, file)
                .into_iter()
                .map(|diagnostic| diagnostic.message().into_owned())
                .collect::<Vec<_>>(),
            vec![] as Vec<String>
        );

        Ok(())
    }
}
