#![warn(
    clippy::disallowed_methods,
    reason = "Prefer System trait methods over std methods in ty crates"
)]
use crate::glob::{GlobFilterCheckMode, IncludeResult};
use crate::metadata::options::{OptionDiagnostic, ToSettingsError};
use crate::walk::{ProjectFilesFilter, ProjectFilesWalker};
#[cfg(feature = "testing")]
pub use db::tests::TestDb;
pub use db::{ChangeResult, CheckMode, Db, ProjectDatabase, SalsaMemoryDump};
use files::{Index, Indexed, IndexedFiles};
use metadata::settings::Settings;
pub use metadata::{ProjectMetadata, ProjectMetadataError};
use ruff_db::diagnostic::{
    Annotation, Diagnostic, DiagnosticId, Severity, Span, SubDiagnostic, SubDiagnosticSeverity,
};
use ruff_db::files::{File, FileRootKind};
use ruff_db::parsed::parsed_module;
use ruff_db::source::{SourceTextError, source_text};
use ruff_db::system::{SystemPath, SystemPathBuf};
use rustc_hash::FxHashSet;
use salsa::Durability;
use salsa::Setter;
use std::backtrace::BacktraceStatus;
use std::collections::hash_set;
use std::iter::FusedIterator;
use std::panic::{AssertUnwindSafe, UnwindSafe};
use std::sync::Arc;
use thiserror::Error;
use ty_python_semantic::add_inferred_python_version_hint_to_diagnostic;
use ty_python_semantic::lint::RuleSelection;
use ty_python_semantic::types::check_types;

mod db;
mod files;
mod glob;
pub mod metadata;
mod walk;
pub mod watch;

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
#[salsa::input(heap_size=ruff_memory_usage::heap_size)]
#[derive(Debug)]
pub struct Project {
    /// The files that are open in the project, [`None`] if there are no open files.
    #[returns(ref)]
    #[default]
    open_fileset: FxHashSet<File>,

    /// The first-party files of this project.
    #[default]
    #[returns(ref)]
    file_set: IndexedFiles,

    /// The metadata describing the project, including the unresolved options.
    ///
    /// We box the metadata here because it's a fairly large type and
    /// reducing the size of `Project` helps reduce the size of the
    /// salsa allocated table for `Project`.
    #[returns(deref)]
    pub metadata: Box<ProjectMetadata>,

    /// The resolved project settings.
    ///
    /// We box the metadata here because it's a fairly large type and
    /// reducing the size of `Project` helps reduce the size of the
    /// salsa allocated table for `Project`.
    #[returns(deref)]
    pub settings: Box<Settings>,

    /// The paths that should be included when checking this project.
    ///
    /// The default (when this list is empty) is to include all files in the project root
    /// (that satisfy the configured include and exclude patterns).
    /// However, it's sometimes desired to only check a subset of the project, e.g. to see
    /// the diagnostics for a single file or a folder.
    ///
    /// This list gets initialized by the paths passed to `ty check <paths>`
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
    #[returns(deref)]
    included_paths_list: Vec<SystemPathBuf>,

    /// Diagnostics that were generated when resolving the project settings.
    #[returns(deref)]
    settings_diagnostics: Vec<OptionDiagnostic>,

    /// The mode in which the project should be checked.
    ///
    /// This changes the behavior of `check` to either check only the open files or all files in
    /// the project including the virtual files that might exists in the editor.
    #[default]
    check_mode: CheckMode,

    #[default]
    verbose_flag: bool,

    /// Whether to enforce exclusion rules even to files explicitly passed to ty on the command line.
    #[default]
    force_exclude_flag: bool,
}

/// A progress reporter.
pub trait ProgressReporter: Send + Sync {
    /// Initialize the reporter with the number of files.
    fn set_files(&mut self, files: usize);

    /// Report the completion of checking a given file along with its diagnostics.
    fn report_checked_file(&self, db: &ProjectDatabase, file: File, diagnostics: &[Diagnostic]);

    /// Reports settings or IO related diagnostics. The diagnostics
    /// can belong to different files or no file at all.
    /// But it's never a file for which [`Self::report_checked_file`] gets called.
    fn report_diagnostics(&mut self, db: &ProjectDatabase, diagnostics: Vec<Diagnostic>);
}

/// Reporter that collects all diagnostics into a `Vec`.
#[derive(Default)]
pub struct CollectReporter(std::sync::Mutex<Vec<Diagnostic>>);

impl CollectReporter {
    pub fn into_sorted(self, db: &dyn Db) -> Vec<Diagnostic> {
        let mut diagnostics = self.0.into_inner().unwrap();
        diagnostics.sort_by(|left, right| {
            left.rendering_sort_key(db)
                .cmp(&right.rendering_sort_key(db))
        });
        diagnostics
    }
}

impl ProgressReporter for CollectReporter {
    fn set_files(&mut self, _files: usize) {}
    fn report_checked_file(&self, _db: &ProjectDatabase, _file: File, diagnostics: &[Diagnostic]) {
        if diagnostics.is_empty() {
            return;
        }

        self.0
            .lock()
            .unwrap()
            .extend(diagnostics.iter().map(Clone::clone));
    }

    fn report_diagnostics(&mut self, _db: &ProjectDatabase, diagnostics: Vec<Diagnostic>) {
        self.0.get_mut().unwrap().extend(diagnostics);
    }
}

#[salsa::tracked]
impl Project {
    pub fn from_metadata(db: &dyn Db, metadata: ProjectMetadata) -> Result<Self, ToSettingsError> {
        let (settings, diagnostics) = metadata.options().to_settings(db, metadata.root())?;

        // This adds a file root for the project itself. This enables
        // tracking of when changes are made to the files in a project
        // at the directory level. At time of writing (2025-07-17),
        // this is used for caching completions for submodules.
        db.files()
            .try_add_root(db, metadata.root(), FileRootKind::Project);

        let project = Project::builder(Box::new(metadata), Box::new(settings), diagnostics)
            .durability(Durability::MEDIUM)
            .open_fileset_durability(Durability::LOW)
            .file_set_durability(Durability::LOW)
            .new(db);

        Ok(project)
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
    #[salsa::tracked(returns(deref), heap_size=ruff_memory_usage::heap_size)]
    pub fn rules(self, db: &dyn Db) -> Arc<RuleSelection> {
        self.settings(db).to_rules()
    }

    /// Returns `true` if `path` is both part of the project and included (see `included_paths_list`).
    ///
    /// Unlike [Self::files], this method does not respect `.gitignore` files. It only checks
    /// the project's include and exclude settings as well as the paths that were passed to `ty check <paths>`.
    /// This means, that this method is an over-approximation of `Self::files` and may return `true` for paths
    /// that won't be included when checking the project because they're ignored in a `.gitignore` file.
    pub fn is_file_included(self, db: &dyn Db, path: &SystemPath) -> bool {
        ProjectFilesFilter::from_project(db, self)
            .is_file_included(path, GlobFilterCheckMode::Adhoc)
            == IncludeResult::Included
    }

    pub fn is_directory_included(self, db: &dyn Db, path: &SystemPath) -> bool {
        ProjectFilesFilter::from_project(db, self)
            .is_directory_included(path, GlobFilterCheckMode::Adhoc)
            == IncludeResult::Included
    }

    pub fn reload(self, db: &mut dyn Db, metadata: ProjectMetadata) {
        tracing::debug!("Reloading project");
        assert_eq!(self.root(db), metadata.root());

        if &metadata != self.metadata(db) {
            match metadata.options().to_settings(db, metadata.root()) {
                Ok((settings, settings_diagnostics)) => {
                    if self.settings(db) != &settings {
                        self.set_settings(db).to(Box::new(settings));
                    }

                    if self.settings_diagnostics(db) != settings_diagnostics {
                        self.set_settings_diagnostics(db).to(settings_diagnostics);
                    }
                }
                Err(error) => {
                    self.set_settings_diagnostics(db)
                        .to(vec![error.into_diagnostic()]);
                }
            }

            self.set_metadata(db).to(Box::new(metadata));
        }

        self.reload_files(db);
    }

    /// Checks the project and its dependencies according to the project's check mode.
    pub(crate) fn check(self, db: &ProjectDatabase, reporter: &mut dyn ProgressReporter) {
        let project_span = tracing::debug_span!("Project::check");
        let _span = project_span.enter();

        tracing::debug!(
            "Checking {} in project '{name}'",
            self.check_mode(db),
            name = self.name(db)
        );

        let mut diagnostics: Vec<Diagnostic> = self
            .settings_diagnostics(db)
            .iter()
            .map(OptionDiagnostic::to_diagnostic)
            .collect();

        let files = ProjectFiles::new(db, self);
        reporter.set_files(files.len());

        diagnostics.extend(
            files
                .diagnostics()
                .iter()
                .map(IOErrorDiagnostic::to_diagnostic),
        );

        reporter.report_diagnostics(db, diagnostics);

        let open_files = self.open_files(db);
        let check_start = ruff_db::Instant::now();

        {
            let db = db.clone();
            let project_span = &project_span;

            rayon::scope(move |scope| {
                for file in &files {
                    let db = db.clone();
                    let reporter = &*reporter;
                    scope.spawn(move |_| {
                        let check_file_span =
                            tracing::debug_span!(parent: project_span, "check_file", ?file);
                        let _entered = check_file_span.entered();

                        match check_file_impl(&db, file) {
                            Ok(diagnostics) => {
                                reporter.report_checked_file(&db, file, diagnostics);

                                // This is outside `check_file_impl` to avoid that opening or closing
                                // a file invalidates the `check_file_impl` query of every file!
                                if !open_files.contains(&file) {
                                    // The module has already been parsed by `check_file_impl`.
                                    // We only retrieve it here so that we can call `clear` on it.
                                    let parsed = parsed_module(&db, file);

                                    // Drop the AST now that we are done checking this file. It is not currently open,
                                    // so it is unlikely to be accessed again soon. If any queries need to access the AST
                                    // from across files, it will be re-parsed.
                                    parsed.clear();
                                }
                            }
                            Err(io_error) => {
                                reporter.report_checked_file(
                                    &db,
                                    file,
                                    std::slice::from_ref(io_error),
                                );
                            }
                        }
                    });
                }
            });
        };

        tracing::debug!(
            "Checking all files took {:.3}s",
            check_start.elapsed().as_secs_f64(),
        );
    }

    pub(crate) fn check_file(self, db: &dyn Db, file: File) -> Vec<Diagnostic> {
        if !self.should_check_file(db, file) {
            return Vec::new();
        }

        match check_file_impl(db, file) {
            Ok(diagnostics) => diagnostics.to_vec(),
            Err(diagnostic) => vec![diagnostic.clone()],
        }
    }

    /// Opens a file in the project.
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

    pub fn set_verbose(self, db: &mut dyn Db, verbose: bool) {
        if self.verbose_flag(db) != verbose {
            self.set_verbose_flag(db).to(verbose);
        }
    }

    pub fn verbose(self, db: &dyn Db) -> bool {
        self.verbose_flag(db)
    }

    pub fn set_force_exclude(self, db: &mut dyn Db, force: bool) {
        if self.force_exclude_flag(db) != force {
            self.set_force_exclude_flag(db).to(force);
        }
    }

    pub fn force_exclude(self, db: &dyn Db) -> bool {
        self.force_exclude_flag(db)
    }

    /// Returns the paths that should be checked.
    ///
    /// The default is to check the entire project in which case this method returns
    /// the project root. However, users can specify to only check specific sub-folders or
    /// even files of a project by using `ty check <paths>`. In that case, this method
    /// returns the provided absolute paths.
    ///
    /// Note: The CLI doesn't prohibit users from specifying paths outside the project root.
    /// This can be useful to check arbitrary files, but it isn't something we recommend.
    /// We should try to support this use case but it's okay if there are some limitations around it.
    fn included_paths_or_root(self, db: &dyn Db) -> &[SystemPathBuf] {
        match self.included_paths_list(db) {
            [] => std::slice::from_ref(&self.metadata(db).root),
            paths => paths,
        }
    }

    /// Returns the open files in the project or `None` if there are no open files.
    pub fn open_files(self, db: &dyn Db) -> &FxHashSet<File> {
        self.open_fileset(db)
    }

    /// Sets the open files in the project.
    #[tracing::instrument(level = "debug", skip(self, db))]
    pub fn set_open_files(self, db: &mut dyn Db, open_files: FxHashSet<File>) {
        tracing::debug!("Set open project files (count: {})", open_files.len());

        self.set_open_fileset(db).to(open_files);
    }

    /// This takes the open files from the project and returns them.
    fn take_open_files(self, db: &mut dyn Db) -> FxHashSet<File> {
        tracing::debug!("Take open project files");

        // Salsa will cancel any pending queries and remove its own reference to `open_files`
        // so that the reference counter to `open_files` now drops to 1.
        self.set_open_fileset(db).to(FxHashSet::default())
    }

    /// Returns `true` if the file should be checked.
    ///
    /// This depends on the project's check mode:
    /// * For [`OpenFiles`], it checks if the file is either explicitly set as an open file using
    ///   [`open_file`] or a system virtual path
    /// * For [`AllFiles`], it checks if the file is either a system virtual path or a part of the
    ///   indexed files in the project
    ///
    /// [`open_file`]: Self::open_file
    /// [`OpenFiles`]: CheckMode::OpenFiles
    /// [`AllFiles`]: CheckMode::AllFiles
    pub fn should_check_file(self, db: &dyn Db, file: File) -> bool {
        let path = file.path(db);

        // Try to return early to avoid adding a dependency on `open_files` or `file_set` which
        // both have a durability of `LOW`.
        if path.is_vendored_path() {
            return false;
        }

        match self.check_mode(db) {
            CheckMode::OpenFiles => self.open_files(db).contains(&file),
            CheckMode::AllFiles => {
                // Virtual files are always checked.
                path.is_system_virtual_path() || self.files(db).contains(&file)
            }
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

        match files.get() {
            Index::Lazy(vacant) => {
                let _entered =
                    tracing::debug_span!("Project::index_files", project = %self.name(db))
                        .entered();
                let start = ruff_db::Instant::now();

                let walker = ProjectFilesWalker::new(db);
                let (files, diagnostics) = walker.collect_set(db);

                tracing::info!(
                    "Indexed {} file(s) in {:.3}s",
                    files.len(),
                    start.elapsed().as_secs_f64()
                );
                vacant.set(files, diagnostics)
            }
            Index::Indexed(indexed) => indexed,
        }
    }

    pub fn reload_files(self, db: &mut dyn Db) {
        tracing::debug!("Reloading files for project `{}`", self.name(db));

        if !self.file_set(db).is_lazy() {
            // Force a re-index of the files in the next revision.
            self.set_file_set(db).to(IndexedFiles::lazy());
        }
    }

    /// Check if the project's settings have any issues
    pub fn check_settings(&self, db: &dyn Db) -> Vec<Diagnostic> {
        self.settings_diagnostics(db)
            .iter()
            .map(OptionDiagnostic::to_diagnostic)
            .collect()
    }
}

#[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn check_file_impl(db: &dyn Db, file: File) -> Result<Box<[Diagnostic]>, Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // Abort checking if there are IO errors.
    let source = source_text(db, file);

    if let Some(read_error) = source.read_error() {
        return Err(IOErrorDiagnostic {
            file: Some(file),
            error: read_error.clone().into(),
        }
        .to_diagnostic());
    }

    let parsed = parsed_module(db, file);

    let parsed_ref = parsed.load(db);
    diagnostics.extend(
        parsed_ref
            .errors()
            .iter()
            .map(|error| Diagnostic::invalid_syntax(file, &error.error, error)),
    );

    diagnostics.extend(parsed_ref.unsupported_syntax_errors().iter().map(|error| {
        let mut error = Diagnostic::invalid_syntax(file, error, error);
        add_inferred_python_version_hint_to_diagnostic(db, &mut error, "parsing syntax");
        error
    }));

    {
        let db = AssertUnwindSafe(db);
        match catch(&**db, file, || check_types(*db, file)) {
            Ok(Some(type_check_diagnostics)) => {
                diagnostics.extend(type_check_diagnostics);
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

    Ok(diagnostics.into_boxed_slice())
}

#[derive(Debug)]
enum ProjectFiles<'a> {
    OpenFiles(&'a FxHashSet<File>),
    Indexed(files::Indexed<'a>),
}

impl<'a> ProjectFiles<'a> {
    fn new(db: &'a dyn Db, project: Project) -> Self {
        match project.check_mode(db) {
            CheckMode::OpenFiles => ProjectFiles::OpenFiles(project.open_files(db)),
            CheckMode::AllFiles => ProjectFiles::Indexed(project.files(db)),
        }
    }

    fn diagnostics(&self) -> &[IOErrorDiagnostic] {
        match self {
            ProjectFiles::OpenFiles(_) => &[],
            ProjectFiles::Indexed(files) => files.diagnostics(),
        }
    }

    fn len(&self) -> usize {
        match self {
            ProjectFiles::OpenFiles(open_files) => open_files.len(),
            ProjectFiles::Indexed(files) => files.len(),
        }
    }
}

impl<'a> IntoIterator for &'a ProjectFiles<'a> {
    type Item = File;
    type IntoIter = ProjectFilesIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            ProjectFiles::OpenFiles(files) => ProjectFilesIter::OpenFiles(files.iter()),
            ProjectFiles::Indexed(files) => ProjectFilesIter::Indexed(files.into_iter()),
        }
    }
}

enum ProjectFilesIter<'db> {
    OpenFiles(hash_set::Iter<'db, File>),
    Indexed(files::IndexedIter<'db>),
}

impl Iterator for ProjectFilesIter<'_> {
    type Item = File;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            ProjectFilesIter::OpenFiles(files) => files.next().copied(),
            ProjectFilesIter::Indexed(files) => files.next(),
        }
    }
}

impl FusedIterator for ProjectFilesIter<'_> {}

#[derive(Debug, Clone, get_size2::GetSize)]
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

#[derive(Error, Debug, Clone, get_size2::GetSize)]
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
            let message = error.to_diagnostic_message(Some(file.path(db)));
            let mut diagnostic = Diagnostic::new(DiagnosticId::Panic, Severity::Fatal, message);
            diagnostic.sub(SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                "This indicates a bug in ty.",
            ));

            let report_message = "If you could open an issue at https://github.com/astral-sh/ty/issues/new?title=%5Bpanic%5D, we'd be very appreciative!";
            diagnostic.sub(SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                report_message,
            ));
            diagnostic.sub(SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                format!(
                    "Platform: {os} {arch}",
                    os = std::env::consts::OS,
                    arch = std::env::consts::ARCH
                ),
            ));
            if let Some(version) = ruff_db::program_version() {
                diagnostic.sub(SubDiagnostic::new(
                    SubDiagnosticSeverity::Info,
                    format!("Version: {version}"),
                ));
            }

            diagnostic.sub(SubDiagnostic::new(
                SubDiagnosticSeverity::Info,
                format!(
                    "Args: {args:?}",
                    args = std::env::args().collect::<Vec<_>>()
                ),
            ));

            if let Some(backtrace) = error.backtrace {
                match backtrace.status() {
                    BacktraceStatus::Disabled => {
                        diagnostic.sub(SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            "run with `RUST_BACKTRACE=1` environment variable to show the full backtrace information",
                        ));
                    }
                    BacktraceStatus::Captured => {
                        diagnostic.sub(SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            format!("Backtrace:\n{backtrace}"),
                        ));
                    }
                    _ => {}
                }
            }

            if let Some(backtrace) = error.salsa_backtrace {
                salsa::attach(db, || {
                    diagnostic.sub(SubDiagnostic::new(
                        SubDiagnosticSeverity::Info,
                        backtrace.to_string(),
                    ));
                });
            }

            Err(diagnostic)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ProjectMetadata;
    use crate::check_file_impl;
    use crate::db::tests::TestDb;
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem as _, SystemPath, SystemPathBuf};
    use ruff_db::testing::assert_function_query_was_not_run;
    use ruff_python_ast::name::Name;
    use ty_python_semantic::types::check_types;

    #[test]
    fn check_file_skips_type_checking_when_file_cant_be_read() -> ruff_db::system::Result<()> {
        let project = ProjectMetadata::new(Name::new_static("test"), SystemPathBuf::from("/"));
        let mut db = TestDb::new(project);
        db.init_program().unwrap();
        let path = SystemPath::new("test.py");

        db.write_file(path, "x = 10")?;
        let file = system_path_to_file(&db, path).unwrap();

        // Now the file gets deleted before we had a chance to read its source text.
        db.memory_file_system().remove_file(path)?;
        file.sync(&mut db);

        assert_eq!(source_text(&db, file).as_str(), "");
        assert_eq!(
            check_file_impl(&db, file)
                .as_ref()
                .unwrap_err()
                .primary_message()
                .to_string(),
            "Failed to read file: No such file or directory".to_string()
        );

        let events = db.take_salsa_events();
        assert_function_query_was_not_run(&db, check_types, file, &events);

        // The user now creates a new file with an empty text. The source text
        // content returned by `source_text` remains unchanged, but the diagnostics should get updated.
        db.write_file(path, "").unwrap();

        assert_eq!(source_text(&db, file).as_str(), "");
        assert_eq!(
            check_file_impl(&db, file)
                .as_ref()
                .unwrap()
                .iter()
                .map(|diagnostic| diagnostic.primary_message().to_string())
                .collect::<Vec<_>>(),
            vec![] as Vec<String>
        );

        Ok(())
    }
}
