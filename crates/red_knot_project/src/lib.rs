#![allow(clippy::ref_option)]

use crate::metadata::options::OptionDiagnostic;
pub use db::{Db, ProjectDatabase};
use files::{Index, Indexed, IndexedFiles};
use metadata::settings::Settings;
pub use metadata::{ProjectDiscoveryError, ProjectMetadata};
use red_knot_python_semantic::lint::{LintRegistry, LintRegistryBuilder, RuleSelection};
use red_knot_python_semantic::syntax::SyntaxDiagnostic;
use red_knot_python_semantic::types::check_types;
use red_knot_python_semantic::{Program, register_lints};
use ruff_db::diagnostic::{Diagnostic, DiagnosticId, ParseDiagnostic, Severity, Span};
use ruff_db::files::{system_path_to_file, File};
use ruff_db::parsed::parsed_module;
use ruff_db::source::{source_text, SourceTextError};
use ruff_db::system::walk_directory::WalkState;
use ruff_db::system::{FileType, SystemPath};
use ruff_python_ast::PySourceType;
use rustc_hash::{FxBuildHasher, FxHashSet};
use salsa::Durability;
use salsa::Setter;
use std::borrow::Cow;
use std::sync::Arc;

pub mod combine;

mod db;
mod files;
pub mod metadata;
pub mod watch;

pub static DEFAULT_LINT_REGISTRY: std::sync::LazyLock<LintRegistry> =
    std::sync::LazyLock::new(default_lints_registry);

pub fn default_lints_registry() -> LintRegistry {
    let mut builder = LintRegistryBuilder::default();
    register_lints(&mut builder);
    builder.build()
}

/// The project as a Salsa ingredient.
///
/// ## How is a project different from a program?
/// There are two (related) motivations:
///
/// 1. Program is defined in `ruff_db` and it can't reference the settings types for the linter and formatter
///    without introducing a cyclic dependency. The project is defined in a higher level crate
///    where it can reference these setting types.
/// 2. Running `ruff check` with different target versions results in different programs (settings) but
///    it remains the same project. That's why program is a narrowed view of the project only
///    holding on to the most fundamental settings required for checking.
#[salsa::input]
pub struct Project {
    /// The files that are open in the project.
    ///
    /// Setting the open files to a non-`None` value changes `check` to only check the
    /// open files rather than all files in the project.
    #[return_ref]
    #[default]
    open_fileset: Option<Arc<FxHashSet<File>>>,

    /// The first-party files of this project.
    #[default]
    #[return_ref]
    file_set: IndexedFiles,

    /// The metadata describing the project, including the unresolved options.
    #[return_ref]
    pub metadata: ProjectMetadata,

    /// The resolved project settings.
    #[return_ref]
    pub settings: Settings,

    /// Diagnostics that were generated when resolving the project settings.
    #[return_ref]
    settings_diagnostics: Vec<OptionDiagnostic>,
}

#[salsa::tracked]
impl Project {
    pub fn from_metadata(db: &dyn Db, metadata: ProjectMetadata) -> Self {
        let (settings, settings_diagnostics) = metadata.options().to_settings(db);

        Project::builder(metadata, settings, settings_diagnostics)
            .durability(Durability::MEDIUM)
            .open_fileset_durability(Durability::LOW)
            .file_set_durability(Durability::LOW)
            .new(db)
    }

    pub fn root(self, db: &dyn Db) -> &SystemPath {
        self.metadata(db).root()
    }

    pub fn name(self, db: &dyn Db) -> &str {
        self.metadata(db).name()
    }

    /// Returns the resolved linter rules for the project.
    ///
    /// This is a salsa query to prevent re-computing queries if other, unrelated
    /// settings change. For example, we don't want that changing the terminal settings
    /// invalidates any type checking queries.
    #[salsa::tracked]
    pub fn rules(self, db: &dyn Db) -> Arc<RuleSelection> {
        self.settings(db).to_rules()
    }

    pub fn reload(self, db: &mut dyn Db, metadata: ProjectMetadata) {
        tracing::debug!("Reloading project");
        assert_eq!(self.root(db), metadata.root());

        if &metadata != self.metadata(db) {
            let (settings, settings_diagnostics) = metadata.options().to_settings(db);

            if self.settings(db) != &settings {
                self.set_settings(db).to(settings);
            }

            if self.settings_diagnostics(db) != &settings_diagnostics {
                self.set_settings_diagnostics(db).to(settings_diagnostics);
            }

            self.set_metadata(db).to(metadata);
        }

        self.reload_files(db);
    }

    /// Checks all open files in the project and its dependencies.
    pub(crate) fn check(self, db: &ProjectDatabase) -> Vec<Box<dyn Diagnostic>> {
        let project_span = tracing::debug_span!("Project::check");
        let _span = project_span.enter();

        tracing::debug!("Checking project '{name}'", name = self.name(db));

        let mut diagnostics: Vec<Box<dyn Diagnostic>> = Vec::new();
        diagnostics.extend(self.settings_diagnostics(db).iter().map(|diagnostic| {
            let diagnostic: Box<dyn Diagnostic> = Box::new(diagnostic.clone());
            diagnostic
        }));

        let result = Arc::new(std::sync::Mutex::new(diagnostics));
        let inner_result = Arc::clone(&result);

        let db = db.clone();
        let project_span = project_span.clone();

        rayon::scope(move |scope| {
            let files = ProjectFiles::new(&db, self);
            for file in &files {
                let result = inner_result.clone();
                let db = db.clone();
                let project_span = project_span.clone();

                scope.spawn(move |_| {
                    let check_file_span = tracing::debug_span!(parent: &project_span, "check_file", file=%file.path(&db));
                    let _entered = check_file_span.entered();

                    let file_diagnostics = check_file_impl(&db, file);
                    result.lock().unwrap().extend(file_diagnostics);
                });
            }
        });

        Arc::into_inner(result).unwrap().into_inner().unwrap()
    }

    pub(crate) fn check_file(self, db: &dyn Db, file: File) -> Vec<Box<dyn Diagnostic>> {
        let mut file_diagnostics: Vec<_> = self
            .settings_diagnostics(db)
            .iter()
            .map(|diagnostic| {
                let diagnostic: Box<dyn Diagnostic> = Box::new(diagnostic.clone());
                diagnostic
            })
            .collect();

        let check_diagnostics = check_file_impl(db, file);
        file_diagnostics.extend(check_diagnostics);

        file_diagnostics
    }

    /// Opens a file in the project.
    ///
    /// This changes the behavior of `check` to only check the open files rather than all files in the project.
    pub fn open_file(self, db: &mut dyn Db, file: File) {
        tracing::debug!("Opening file `{}`", file.path(db));

        let mut open_files = self.take_open_files(db);
        open_files.insert(file);
        self.set_open_files(db, open_files);
    }

    /// Closes a file in the project.
    pub fn close_file(self, db: &mut dyn Db, file: File) -> bool {
        tracing::debug!("Closing file `{}`", file.path(db));

        let mut open_files = self.take_open_files(db);
        let removed = open_files.remove(&file);

        if removed {
            self.set_open_files(db, open_files);
        }

        removed
    }

    /// Returns the open files in the project or `None` if the entire project should be checked.
    pub fn open_files(self, db: &dyn Db) -> Option<&FxHashSet<File>> {
        self.open_fileset(db).as_deref()
    }

    /// Sets the open files in the project.
    ///
    /// This changes the behavior of `check` to only check the open files rather than all files in the project.
    #[tracing::instrument(level = "debug", skip(self, db))]
    pub fn set_open_files(self, db: &mut dyn Db, open_files: FxHashSet<File>) {
        tracing::debug!("Set open project files (count: {})", open_files.len());

        self.set_open_fileset(db).to(Some(Arc::new(open_files)));
    }

    /// This takes the open files from the project and returns them.
    ///
    /// This changes the behavior of `check` to check all files in the project instead of just the open files.
    fn take_open_files(self, db: &mut dyn Db) -> FxHashSet<File> {
        tracing::debug!("Take open project files");

        // Salsa will cancel any pending queries and remove its own reference to `open_files`
        // so that the reference counter to `open_files` now drops to 1.
        let open_files = self.set_open_fileset(db).to(None);

        if let Some(open_files) = open_files {
            Arc::try_unwrap(open_files).unwrap()
        } else {
            FxHashSet::default()
        }
    }

    /// Returns `true` if the file is open in the project.
    ///
    /// A file is considered open when:
    /// * explicitly set as an open file using [`open_file`](Self::open_file)
    /// * It has a [`SystemPath`] and belongs to a package's `src` files
    /// * It has a [`SystemVirtualPath`](ruff_db::system::SystemVirtualPath)
    pub fn is_file_open(self, db: &dyn Db, file: File) -> bool {
        if let Some(open_files) = self.open_files(db) {
            open_files.contains(&file)
        } else if file.path(db).is_system_path() {
            self.contains_file(db, file)
        } else {
            file.path(db).is_system_virtual_path()
        }
    }

    /// Returns `true` if `file` is a first-party file part of this package.
    pub fn contains_file(self, db: &dyn Db, file: File) -> bool {
        self.files(db).contains(&file)
    }

    #[tracing::instrument(level = "debug", skip(db))]
    pub fn remove_file(self, db: &mut dyn Db, file: File) {
        tracing::debug!(
            "Removing file `{}` from project `{}`",
            file.path(db),
            self.name(db)
        );

        let Some(mut index) = IndexedFiles::indexed_mut(db, self) else {
            return;
        };

        index.remove(file);
    }

    pub fn add_file(self, db: &mut dyn Db, file: File) {
        tracing::debug!(
            "Adding file `{}` to project `{}`",
            file.path(db),
            self.name(db)
        );

        let Some(mut index) = IndexedFiles::indexed_mut(db, self) else {
            return;
        };

        index.insert(file);
    }

    /// Returns the files belonging to this project.
    pub fn files(self, db: &dyn Db) -> Indexed<'_> {
        let files = self.file_set(db);

        let indexed = match files.get() {
            Index::Lazy(vacant) => {
                let _entered =
                    tracing::debug_span!("Project::index_files", package = %self.name(db))
                        .entered();

                let files = discover_project_files(db, self);
                tracing::info!("Found {} files in project `{}`", files.len(), self.name(db));
                vacant.set(files)
            }
            Index::Indexed(indexed) => indexed,
        };

        indexed
    }

    pub fn reload_files(self, db: &mut dyn Db) {
        tracing::debug!("Reloading files for project `{}`", self.name(db));

        if !self.file_set(db).is_lazy() {
            // Force a re-index of the files in the next revision.
            self.set_file_set(db).to(IndexedFiles::lazy());
        }
    }
}

fn check_file_impl(db: &dyn Db, file: File) -> Vec<Box<dyn Diagnostic>> {
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

    let parsed = parsed_module(db.upcast(), file, Program::get(db).python_version(db));
    diagnostics.extend(parsed.errors().iter().map(|error| {
        let diagnostic: Box<dyn Diagnostic> = Box::new(ParseDiagnostic::new(file, error.clone()));
        diagnostic
    }));

    if parsed.is_valid() {
        diagnostics.extend(parsed.syntax_errors().iter().map(|error| {
            let diagnostic: Box<dyn Diagnostic> =
                Box::new(SyntaxDiagnostic::from_syntax_error(error, file));
            diagnostic
        }));
    }

    diagnostics.extend(check_types(db.upcast(), file).iter().map(|diagnostic| {
        let boxed: Box<dyn Diagnostic> = Box::new(diagnostic.clone());
        boxed
    }));

    diagnostics.sort_unstable_by_key(|diagnostic| {
        diagnostic
            .span()
            .and_then(|span| span.range())
            .unwrap_or_default()
            .start()
    });

    diagnostics
}

fn discover_project_files(db: &dyn Db, project: Project) -> FxHashSet<File> {
    let paths = std::sync::Mutex::new(Vec::new());

    db.system().walk_directory(project.root(db)).run(|| {
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
enum ProjectFiles<'a> {
    OpenFiles(&'a FxHashSet<File>),
    Indexed(files::Indexed<'a>),
}

impl<'a> ProjectFiles<'a> {
    fn new(db: &'a dyn Db, project: Project) -> Self {
        if let Some(open_files) = project.open_files(db) {
            ProjectFiles::OpenFiles(open_files)
        } else {
            ProjectFiles::Indexed(project.files(db))
        }
    }
}

impl<'a> IntoIterator for &'a ProjectFiles<'a> {
    type Item = File;
    type IntoIter = ProjectFilesIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            ProjectFiles::OpenFiles(files) => ProjectFilesIter::OpenFiles(files.iter()),
            ProjectFiles::Indexed(indexed) => ProjectFilesIter::Indexed {
                files: indexed.into_iter(),
            },
        }
    }
}

enum ProjectFilesIter<'db> {
    OpenFiles(std::collections::hash_set::Iter<'db, File>),
    Indexed { files: files::IndexedIter<'db> },
}

impl Iterator for ProjectFilesIter<'_> {
    type Item = File;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ProjectFilesIter::OpenFiles(files) => files.next().copied(),
            ProjectFilesIter::Indexed { files } => files.next(),
        }
    }
}

#[derive(Debug)]
pub struct IOErrorDiagnostic {
    file: File,
    error: SourceTextError,
}

impl Diagnostic for IOErrorDiagnostic {
    fn id(&self) -> DiagnosticId {
        DiagnosticId::Io
    }

    fn message(&self) -> Cow<str> {
        self.error.to_string().into()
    }

    fn span(&self) -> Option<Span> {
        Some(Span::from(self.file))
    }

    fn severity(&self) -> Severity {
        Severity::Error
    }
}

#[cfg(test)]
mod tests {
    use crate::db::tests::TestDb;
    use crate::{check_file_impl, ProjectMetadata};
    use red_knot_python_semantic::types::check_types;
    use ruff_db::diagnostic::Diagnostic;
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithTestSystem, SystemPath, SystemPathBuf};
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast::name::Name;

    #[test]
    fn check_file_skips_type_checking_when_file_cant_be_read() -> ruff_db::system::Result<()> {
        let project = ProjectMetadata::new(Name::new_static("test"), SystemPathBuf::from("/"));
        let mut db = TestDb::new(project);
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10")?;
        let file = system_path_to_file(&db, path).unwrap();

        // Now the file gets deleted before we had a chance to read its source text.
        db.memory_file_system().remove_file(path)?;
        file.sync(&mut db);

        assert_eq!(source_text(&db, file).as_str(), "");
        assert_eq!(
            check_file_impl(&db, file)
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
            check_file_impl(&db, file)
                .into_iter()
                .map(|diagnostic| diagnostic.message().into_owned())
                .collect::<Vec<_>>(),
            vec![] as Vec<String>
        );

        Ok(())
    }
}
