#![allow(clippy::ref_option)]

use crate::metadata::options::OptionDiagnostic;
use crate::walk::{ProjectFilesFilter, ProjectFilesWalker};
pub use db::{Db, ProjectDatabase};
use files::{Index, Indexed, IndexedFiles};
use metadata::settings::Settings;
pub use metadata::{ProjectDiscoveryError, ProjectMetadata};
use red_knot_python_semantic::lint::{LintRegistry, LintRegistryBuilder, RuleSelection};
use red_knot_python_semantic::register_lints;
use red_knot_python_semantic::types::check_types;
use ruff_db::diagnostic::{
    create_parse_diagnostic, create_unsupported_syntax_diagnostic, Annotation, Diagnostic,
    DiagnosticId, Severity, Span, SubDiagnostic,
};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::{source_text, SourceTextError};
use ruff_db::system::{SystemPath, SystemPathBuf};
use rustc_hash::FxHashSet;
use salsa::Durability;
use salsa::Setter;
use std::backtrace::BacktraceStatus;
use std::panic::{AssertUnwindSafe, UnwindSafe};
use std::sync::Arc;
use thiserror::Error;
use tracing::error;

pub mod combine;

mod db;
mod files;
pub mod metadata;
mod walk;
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

    /// The paths that should be included when checking this project.
    ///
    /// The default (when this list is empty) is to include all files in the project root
    /// (that satisfy the configured include and exclude patterns).
    /// However, it's sometimes desired to only check a subset of the project, e.g. to see
    /// the diagnostics for a single file or a folder.
    ///
    /// This list gets initialized by the paths passed to `knot check <paths>`
    ///
    /// ## How is this different from `open_files`?
    ///
    /// The `included_paths` is closely related to `open_files`. The only difference is that
    /// `open_files` is already a resolved set of files whereas `included_paths` is only a list of paths
    /// that are resolved to files by indexing them. The other difference is that
    /// new files added to any directory in `included_paths` will be indexed and added to the project
    /// whereas `open_files` needs to be updated manually (e.g. by the IDE).
    ///
    /// In short, `open_files` is cheaper in contexts where the set of files is known, like
    /// in an IDE when the user only wants to check the open tabs. This could be modeled
    /// with `included_paths` too but it would require an explicit walk dir step that's simply unnecessary.
    #[default]
    #[return_ref]
    included_paths_list: Vec<SystemPathBuf>,

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

    /// Returns `true` if `path` is both part of the project and included (see `included_paths_list`).
    ///
    /// Unlike [Self::files], this method does not respect `.gitignore` files. It only checks
    /// the project's include and exclude settings as well as the paths that were passed to `knot check <paths>`.
    /// This means, that this method is an over-approximation of `Self::files` and may return `true` for paths
    /// that won't be included when checking the project because they're ignored in a `.gitignore` file.
    pub fn is_path_included(self, db: &dyn Db, path: &SystemPath) -> bool {
        ProjectFilesFilter::from_project(db, self).is_included(path)
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
    pub(crate) fn check(self, db: &ProjectDatabase) -> Vec<Diagnostic> {
        let project_span = tracing::debug_span!("Project::check");
        let _span = project_span.enter();

        tracing::debug!("Checking project '{name}'", name = self.name(db));

        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        diagnostics.extend(
            self.settings_diagnostics(db)
                .iter()
                .map(OptionDiagnostic::to_diagnostic),
        );

        let files = ProjectFiles::new(db, self);

        diagnostics.extend(
            files
                .diagnostics()
                .iter()
                .map(IOErrorDiagnostic::to_diagnostic),
        );

        let file_diagnostics = Arc::new(std::sync::Mutex::new(vec![]));

        {
            let file_diagnostics = Arc::clone(&file_diagnostics);
            let db = db.clone();
            let project_span = project_span.clone();

            rayon::scope(move |scope| {
                for file in &files {
                    let result = Arc::clone(&file_diagnostics);
                    let db = db.clone();
                    let project_span = project_span.clone();

                    scope.spawn(move |_| {
                        let check_file_span =
                            tracing::debug_span!(parent: &project_span, "check_file", ?file);
                        let _entered = check_file_span.entered();

                        let file_diagnostics = check_file_impl(&db, file);
                        result.lock().unwrap().extend(file_diagnostics);
                    });
                }
            });
        }

        let mut file_diagnostics = Arc::into_inner(file_diagnostics)
            .unwrap()
            .into_inner()
            .unwrap();
        // We sort diagnostics in a way that keeps them in source order
        // and grouped by file. After that, we fall back to severity
        // (with fatal messages sorting before info messages) and then
        // finally the diagnostic ID.
        file_diagnostics.sort_by(|d1, d2| {
            if let (Some(span1), Some(span2)) = (d1.primary_span(), d2.primary_span()) {
                let order = span1
                    .file()
                    .path(db)
                    .as_str()
                    .cmp(span2.file().path(db).as_str());
                if order.is_ne() {
                    return order;
                }

                if let (Some(range1), Some(range2)) = (span1.range(), span2.range()) {
                    let order = range1.start().cmp(&range2.start());
                    if order.is_ne() {
                        return order;
                    }
                }
            }
            // Reverse so that, e.g., Fatal sorts before Info.
            let order = d1.severity().cmp(&d2.severity()).reverse();
            if order.is_ne() {
                return order;
            }
            d1.id().cmp(&d2.id())
        });
        diagnostics.extend(file_diagnostics);
        diagnostics
    }

    pub(crate) fn check_file(self, db: &dyn Db, file: File) -> Vec<Diagnostic> {
        let mut file_diagnostics: Vec<_> = self
            .settings_diagnostics(db)
            .iter()
            .map(OptionDiagnostic::to_diagnostic)
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

    pub fn set_included_paths(self, db: &mut dyn Db, paths: Vec<SystemPathBuf>) {
        tracing::debug!("Setting included paths: {paths}", paths = paths.len());

        self.set_included_paths_list(db).to(paths);
        self.reload_files(db);
    }

    /// Returns the paths that should be checked.
    ///
    /// The default is to check the entire project in which case this method returns
    /// the project root. However, users can specify to only check specific sub-folders or
    /// even files of a project by using `knot check <paths>`. In that case, this method
    /// returns the provided absolute paths.
    ///
    /// Note: The CLI doesn't prohibit users from specifying paths outside the project root.
    /// This can be useful to check arbitrary files, but it isn't something we recommend.
    /// We should try to support this use case but it's okay if there are some limitations around it.
    fn included_paths_or_root(self, db: &dyn Db) -> &[SystemPathBuf] {
        match &**self.included_paths_list(db) {
            [] => std::slice::from_ref(&self.metadata(db).root),
            paths => paths,
        }
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
        let path = file.path(db);

        // Try to return early to avoid adding a dependency on `open_files` or `file_set` which
        // both have a durability of `LOW`.
        if path.is_vendored_path() {
            return false;
        }

        if let Some(open_files) = self.open_files(db) {
            open_files.contains(&file)
        } else if file.path(db).is_system_path() {
            self.files(db).contains(&file)
        } else {
            file.path(db).is_system_virtual_path()
        }
    }

    #[tracing::instrument(level = "debug", skip(self, db))]
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

    /// Replaces the diagnostics from indexing the project files with `diagnostics`.
    ///
    /// This is a no-op if the project files haven't been indexed yet.
    pub fn replace_index_diagnostics(self, db: &mut dyn Db, diagnostics: Vec<IOErrorDiagnostic>) {
        let Some(mut index) = IndexedFiles::indexed_mut(db, self) else {
            return;
        };

        index.set_diagnostics(diagnostics);
    }

    /// Returns the files belonging to this project.
    pub fn files(self, db: &dyn Db) -> Indexed<'_> {
        let files = self.file_set(db);

        let indexed = match files.get() {
            Index::Lazy(vacant) => {
                let _entered =
                    tracing::debug_span!("Project::index_files", project = %self.name(db))
                        .entered();

                let walker = ProjectFilesWalker::new(db);
                let (files, diagnostics) = walker.collect_set(db);

                tracing::info!("Indexed {} file(s)", files.len());
                vacant.set(files, diagnostics)
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

fn check_file_impl(db: &dyn Db, file: File) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // Abort checking if there are IO errors.
    let source = source_text(db.upcast(), file);

    if let Some(read_error) = source.read_error() {
        diagnostics.push(
            IOErrorDiagnostic {
                file: Some(file),
                error: read_error.clone().into(),
            }
            .to_diagnostic(),
        );
        return diagnostics;
    }

    let parsed = parsed_module(db.upcast(), file);
    diagnostics.extend(
        parsed
            .errors()
            .iter()
            .map(|error| create_parse_diagnostic(file, error)),
    );

    diagnostics.extend(
        parsed
            .unsupported_syntax_errors()
            .iter()
            .map(|error| create_unsupported_syntax_diagnostic(file, error)),
    );

    {
        let db = AssertUnwindSafe(db);
        match catch(&**db, file, || check_types(db.upcast(), file)) {
            Ok(Some(type_check_diagnostics)) => {
                diagnostics.extend(type_check_diagnostics.into_iter().cloned());
            }
            Ok(None) => {}
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }

    diagnostics.sort_unstable_by_key(|diagnostic| {
        diagnostic
            .primary_span()
            .and_then(|span| span.range())
            .unwrap_or_default()
            .start()
    });

    diagnostics
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

    fn diagnostics(&self) -> &[IOErrorDiagnostic] {
        match self {
            ProjectFiles::OpenFiles(_) => &[],
            ProjectFiles::Indexed(indexed) => indexed.diagnostics(),
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

#[derive(Debug, Clone)]
pub struct IOErrorDiagnostic {
    file: Option<File>,
    error: IOErrorKind,
}

impl IOErrorDiagnostic {
    fn to_diagnostic(&self) -> Diagnostic {
        let mut diag = Diagnostic::new(DiagnosticId::Io, Severity::Error, &self.error);
        if let Some(file) = self.file {
            diag.annotate(Annotation::primary(Span::from(file)));
        }
        diag
    }
}

#[derive(Error, Debug, Clone)]
enum IOErrorKind {
    #[error(transparent)]
    Walk(#[from] walk::WalkError),

    #[error(transparent)]
    SourceText(#[from] SourceTextError),
}

fn catch<F, R>(db: &dyn Db, file: File, f: F) -> Result<Option<R>, Diagnostic>
where
    F: FnOnce() -> R + UnwindSafe,
{
    match ruff_db::panic::catch_unwind(|| {
        // Ignore salsa errors
        salsa::Cancelled::catch(f).ok()
    }) {
        Ok(result) => Ok(result),
        Err(error) => {
            use std::fmt::Write;
            let mut message = String::new();
            message.push_str("Panicked");

            if let Some(location) = error.location {
                let _ = write!(&mut message, " at {location}");
            }

            let _ = write!(
                &mut message,
                " when checking `{file}`",
                file = file.path(db)
            );

            if let Some(payload) = error.payload.as_str() {
                let _ = write!(&mut message, ": `{payload}`");
            }

            let mut diagnostic = Diagnostic::new(DiagnosticId::Panic, Severity::Fatal, message);
            diagnostic.sub(SubDiagnostic::new(
                Severity::Info,
                "This indicates a bug in Red Knot.",
            ));

            let report_message = "If you could open an issue at https://github.com/astral-sh/ruff/issues/new?title=%5Bred-knot%5D:%20panic we'd be very appreciative!";
            diagnostic.sub(SubDiagnostic::new(Severity::Info, report_message));
            diagnostic.sub(SubDiagnostic::new(
                Severity::Info,
                format!(
                    "Platform: {os} {arch}",
                    os = std::env::consts::OS,
                    arch = std::env::consts::ARCH
                ),
            ));
            diagnostic.sub(SubDiagnostic::new(
                Severity::Info,
                format!(
                    "Args: {args:?}",
                    args = std::env::args().collect::<Vec<_>>()
                ),
            ));

            if let Some(backtrace) = error.backtrace {
                match backtrace.status() {
                    BacktraceStatus::Disabled => {
                        diagnostic.sub(SubDiagnostic::new(
                            Severity::Info,
                            "run with `RUST_BACKTRACE=1` environment variable to show the full backtrace information",
                        ));
                    }
                    BacktraceStatus::Captured => {
                        diagnostic.sub(SubDiagnostic::new(
                            Severity::Info,
                            format!("Backtrace:\n{backtrace}"),
                        ));
                    }
                    _ => {}
                }
            }

            if let Some(backtrace) = error.salsa_backtrace {
                salsa::attach(db, || {
                    diagnostic.sub(SubDiagnostic::new(Severity::Info, backtrace.to_string()));
                });
            }

            Err(diagnostic)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::db::tests::TestDb;
    use crate::{check_file_impl, ProjectMetadata};
    use red_knot_python_semantic::types::check_types;
    use red_knot_python_semantic::{Program, ProgramSettings, PythonPlatform, SearchPathSettings};
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem as _, SystemPath, SystemPathBuf};
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast::name::Name;
    use ruff_python_ast::PythonVersion;

    #[test]
    fn check_file_skips_type_checking_when_file_cant_be_read() -> ruff_db::system::Result<()> {
        let project = ProjectMetadata::new(Name::new_static("test"), SystemPathBuf::from("/"));
        let mut db = TestDb::new(project);
        let path = SystemPath::new("test.py");

        Program::from_settings(
            &db,
            ProgramSettings {
                python_version: PythonVersion::default(),
                python_platform: PythonPlatform::default(),
                search_paths: SearchPathSettings::new(vec![SystemPathBuf::from(".")]),
            },
        )
        .expect("Failed to configure program settings");

        db.write_file(path, "x = 10")?;
        let file = system_path_to_file(&db, path).unwrap();

        // Now the file gets deleted before we had a chance to read its source text.
        db.memory_file_system().remove_file(path)?;
        file.sync(&mut db);

        assert_eq!(source_text(&db, file).as_str(), "");
        assert_eq!(
            check_file_impl(&db, file)
                .into_iter()
                .map(|diagnostic| diagnostic.primary_message().to_string())
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
                .map(|diagnostic| diagnostic.primary_message().to_string())
                .collect::<Vec<_>>(),
            vec![] as Vec<String>
        );

        Ok(())
    }
}
