#![warn(
    clippy::disallowed_methods,
    reason = "Prefer System trait methods over std methods in ty crates"
)]
use crate::glob::{GlobFilterCheckMode, IncludeResult};
use crate::metadata::options::{OptionDiagnostic, ProgramSettingsDiagnostic};
use crate::parallel::ParallelIteratorExt;
use crate::walk::{ProjectFilesFilter, ProjectFilesWalker};
#[cfg(feature = "testing")]
pub use db::testing::TestDb;
pub use db::{ChangeResult, CheckMode, Db, ProjectDatabase, SalsaMemoryDump};
use files::{Index, Indexed, IndexedFiles};

use metadata::settings::Settings;
pub use metadata::{ProjectMetadata, ProjectMetadataError};
use rayon::prelude::*;
use ruff_db::PythonFile;
use ruff_db::diagnostic::{
    Diagnostic, DiagnosticId, Severity, SubDiagnostic, SubDiagnosticSeverity,
};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::system::{SystemPath, SystemPathBuf, deduplicate_nested_paths};
use rustc_hash::FxHashSet;
use salsa::{Database, Durability, Setter};
use std::backtrace::BacktraceStatus;
use std::collections::{BTreeSet, hash_set};
use std::iter::FusedIterator;
use std::panic::{AssertUnwindSafe, UnwindSafe};
use std::sync::Arc;
use ty_module_resolver::Db as _;
use ty_python_semantic::lint::RuleSelection;

mod db;
mod files;
pub mod glob;
pub mod metadata;
pub mod parallel;
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
    /// The files that are open in the project.
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
    #[returns(copy)]
    check_mode: CheckMode,

    #[default]
    #[returns(copy)]
    verbose_flag: bool,

    /// Whether to enforce exclusion rules even to files explicitly passed to ty on the command line.
    #[default]
    #[returns(copy)]
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
    /// Create a project from resolved metadata and settings.
    ///
    /// Program-settings diagnostics are accepted separately so callers do not need to know how to
    /// convert and merge them into the stored project settings diagnostics.
    pub(crate) fn from_metadata(
        db: &dyn Db,
        metadata: ProjectMetadata,
        settings: Settings,
        settings_diagnostics: Vec<OptionDiagnostic>,
        program_settings_diagnostics: Vec<ProgramSettingsDiagnostic>,
    ) -> Self {
        let diagnostics = Self::settings_diagnostics_with_program_diagnostics(
            db,
            settings_diagnostics,
            program_settings_diagnostics,
        );

        Project::builder(Box::new(metadata), Box::new(settings), diagnostics)
            .durability(Durability::MEDIUM)
            .open_fileset_durability(Durability::LOW)
            .file_set_durability(Durability::LOW)
            .new(db)
    }

    /// Permanently freezes the most heavily read immutable project inputs.
    ///
    /// This is intentionally not exhaustive.
    pub(crate) fn freeze(self, db: &mut dyn Db) {
        let durability = Durability::NEVER_CHANGE;
        let metadata = Box::new(self.metadata(db).clone());
        let settings = Box::new(self.settings(db).clone());
        let included_paths = self.included_paths_list(db).to_vec();
        let check_mode = self.check_mode(db);
        let verbose = self.verbose_flag(db);
        let force_exclude = self.force_exclude_flag(db);

        self.set_metadata(db)
            .with_durability(durability)
            .to(metadata);
        self.set_settings(db)
            .with_durability(durability)
            .to(settings);
        self.set_included_paths_list(db)
            .with_durability(durability)
            .to(included_paths);
        self.set_check_mode(db)
            .with_durability(durability)
            .to(check_mode);
        self.set_verbose_flag(db)
            .with_durability(durability)
            .to(verbose);
        self.set_force_exclude_flag(db)
            .with_durability(durability)
            .to(force_exclude);

        IndexedFiles::freeze(db, self);
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

    /// Returns whether `path` is part of the project and included (see `included_paths_list`).
    ///
    /// Unlike [Self::files], this method does not respect `.gitignore` files. It only checks
    /// the project's include and exclude settings as well as the paths that were passed to `ty check <paths>`.
    /// This means, that this method is an over-approximation of `Self::files` and may return `true` for paths
    /// that won't be included when checking the project because they're ignored in a `.gitignore` file.
    pub fn is_file_included(self, db: &dyn Db, path: &SystemPath) -> IncludeResult {
        ProjectFilesFilter::from_project(db, self)
            .is_file_included(path, GlobFilterCheckMode::Adhoc)
    }

    pub fn is_directory_included(self, db: &dyn Db, path: &SystemPath) -> bool {
        matches!(
            ProjectFilesFilter::from_project(db, self)
                .is_directory_included(path, GlobFilterCheckMode::Adhoc),
            IncludeResult::Included { .. }
        )
    }

    /// Reload the project after its metadata or settings have changed.
    ///
    /// Program-settings diagnostics are converted and merged here to keep reload behavior
    /// consistent with initial project creation.
    pub fn reload(
        self,
        db: &mut dyn Db,
        metadata: ProjectMetadata,
        settings: Option<Settings>,
        settings_diagnostics: Vec<OptionDiagnostic>,
        program_settings_diagnostics: Vec<ProgramSettingsDiagnostic>,
    ) -> ProjectReloadResult {
        tracing::debug!("Reloading project");
        let metadata_changed = &metadata != self.metadata(db);
        let settings_diagnostics = Self::settings_diagnostics_with_program_diagnostics(
            db,
            settings_diagnostics,
            program_settings_diagnostics,
        );

        let root_changed = metadata.root() != self.root(db);
        let (settings_changed, files_changed) = if let Some(settings) = settings
            && self.settings(db) != &settings
        {
            let files_changed = root_changed || settings.src() != self.settings(db).src();
            self.set_settings(db).to(Box::new(settings));
            (true, files_changed)
        } else {
            (false, root_changed)
        };

        if self.settings_diagnostics(db) != settings_diagnostics {
            self.set_settings_diagnostics(db).to(settings_diagnostics);
        }

        if files_changed {
            // The project file set only depends on the project root, explicit check paths,
            // force-exclude, and `src` settings. Check paths and force-exclude are updated
            // through their own setters, so a config reload only needs to reindex when the
            // root or resolved `src` settings changed.
            self.reload_files(db);
        }

        if metadata_changed {
            self.set_metadata(db).to(Box::new(metadata));
        }

        if metadata_changed || settings_changed {
            ProjectReloadResult::Changed { files_changed }
        } else {
            ProjectReloadResult::Unchanged
        }
    }

    /// Replace stored settings diagnostics after recomputing program settings.
    ///
    /// This is used when a change affects [`ty_python_core::program::ProgramSettings`] without
    /// reloading the full project.
    pub(crate) fn update_settings_diagnostics(
        self,
        db: &mut dyn Db,
        settings_diagnostics: Vec<OptionDiagnostic>,
        program_settings_diagnostics: Vec<ProgramSettingsDiagnostic>,
    ) {
        let settings_diagnostics = Self::settings_diagnostics_with_program_diagnostics(
            db,
            settings_diagnostics,
            program_settings_diagnostics,
        );

        if self.settings_diagnostics(db) != settings_diagnostics {
            self.set_settings_diagnostics(db).to(settings_diagnostics);
        }
    }

    fn settings_diagnostics_with_program_diagnostics(
        db: &dyn Db,
        mut settings_diagnostics: Vec<OptionDiagnostic>,
        program_settings_diagnostics: Vec<ProgramSettingsDiagnostic>,
    ) -> Vec<OptionDiagnostic> {
        settings_diagnostics.extend(
            program_settings_diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.into_diagnostic(db)),
        );
        settings_diagnostics
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

        diagnostics.extend_from_slice(files.diagnostics());

        reporter.report_diagnostics(db, diagnostics);

        let open_files = self.open_files(db);
        let check_start = ruff_db::Instant::now();

        let files: Vec<_> = (&files).into_iter().collect();

        files
            .into_par_iter()
            .for_each_with_project_db(db, |db, file| {
                db.unwind_if_revision_cancelled();

                let check_file_span =
                    tracing::debug_span!(parent: &project_span, "check_file", ?file);
                let _entered = check_file_span.entered();
                let python_file = PythonFile::new(db, file, db.python_version());

                match check_file_impl(db, python_file) {
                    Ok(diagnostics) => {
                        reporter.report_checked_file(db, file, diagnostics);

                        // This is outside `check_file_impl` to avoid that opening or closing
                        // a file invalidates the `check_file_impl` query of every file!
                        if !open_files.contains(&file) {
                            // The module has already been parsed by `check_file_impl`.
                            // We only retrieve it here so that we can call `clear` on it.
                            let parsed = parsed_module(db, python_file);

                            // Drop the AST now that we are done checking this file. It is not currently open,
                            // so it is unlikely to be accessed again soon. If any queries need to access the AST
                            // from across files, it will be re-parsed.
                            parsed.clear();
                        }
                    }
                    Err(io_error) => {
                        reporter.report_checked_file(db, file, std::slice::from_ref(io_error));
                    }
                }
            });

        tracing::debug!(
            "Checking all files took {:.3}s",
            check_start.elapsed().as_secs_f64(),
        );
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
        self.set_open_files(db, open_files);

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

    /// Returns the open files in the project.
    pub fn open_files(self, db: &dyn Db) -> &FxHashSet<File> {
        self.open_fileset(db)
    }

    /// Sets the open files in the project.
    #[tracing::instrument(level = "debug", skip(self, db))]
    pub fn set_open_files(self, db: &mut dyn Db, open_files: FxHashSet<File>) {
        tracing::debug!("Set open project files (count: {})", open_files.len());

        self.set_open_fileset(db).to(open_files);
    }

    /// Permanently marks the project as never having open files, so reads of the
    /// open-file state record no salsa dependency. Any later write panics.
    pub fn freeze_open_files(self, db: &mut dyn Db) {
        self.set_open_fileset(db)
            .with_durability(Durability::NEVER_CHANGE)
            .to(FxHashSet::default());
    }

    /// This takes the open files from the project and returns them.
    fn take_open_files(self, db: &mut dyn Db) -> FxHashSet<File> {
        tracing::debug!("Take open project files");

        // Salsa will cancel any pending queries and remove its own reference to `open_files`
        // so that the reference counter to `open_files` now drops to 1.
        self.set_open_fileset(db).to(FxHashSet::default())
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

    /// Removes all indexed project files under `paths`.
    ///
    /// This is a no-op if the project files are still lazily indexed.
    #[tracing::instrument(level = "debug", skip(self, db, paths))]
    pub(crate) fn remove_files_under<P, I>(self, db: &mut dyn Db, paths: I)
    where
        I: IntoIterator<Item = P>,
        P: AsRef<SystemPath>,
    {
        let paths = deduplicate_nested_paths(
            paths
                .into_iter()
                .map(|path| SystemPath::absolute(path, db.system().current_directory())),
        )
        .collect::<BTreeSet<_>>();

        if paths.is_empty() {
            return;
        }

        if self.file_set(db).is_lazy() {
            return;
        }

        let files_to_remove = {
            let files = self.files(db);
            files
                .iter()
                .copied()
                .filter(|file| {
                    file.path(db).as_system_path().is_some_and(|file_path| {
                        paths
                            .range(..=file_path.to_path_buf())
                            .next_back()
                            .is_some_and(|path| file_path.starts_with(path))
                    })
                })
                .collect::<Vec<_>>()
        };

        if files_to_remove.is_empty() {
            return;
        }

        let Some(mut index) = IndexedFiles::indexed_mut(db, self) else {
            return;
        };

        for file in files_to_remove {
            index.remove(file);
        }
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
    pub fn replace_index_diagnostics(self, db: &mut dyn Db, diagnostics: Vec<Diagnostic>) {
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

                let walker = ProjectFilesWalker::full();
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

pub(crate) fn check_file(db: &dyn Db, file: File) -> Vec<Diagnostic> {
    if !db.should_check_file(file) {
        return Vec::new();
    }

    check_file_impl(db, PythonFile::new(db, file, db.python_version()))
        .map(<[Diagnostic]>::to_vec)
        .unwrap_or_else(|diagnostic| vec![diagnostic.clone()])
}

/// Returns `true` if the file should be checked.
///
/// This depends on the project's check mode:
/// * For [`CheckMode::OpenFiles`], it checks if the file is explicitly in the open file set.
/// * For [`CheckMode::AllFiles`], it checks if the file is virtual, indexed in the project, or in
///   the open file set.
///
/// This query provides a per-file backdating boundary around the project-wide file sets. Updating
/// either set still revalidates this query, but unchanged results are backdated before invalidation
/// reaches semantic-index and type-inference queries.
#[salsa::tracked(returns(copy))]
pub(crate) fn should_check_file(db: &dyn Db, file: File) -> bool {
    let project = db.project();
    let path = file.path(db);

    // NOTE: The tracing messages below were added because whether a file should be checked or not
    // can sometimes be at the root of confusing UX like "diagnostics all of a sudden stopped
    // working." Having a trace message indicating why a particular file isn't being checked can
    // be quite helpful for narrowing down the issue. The messages are at TRACE because they are
    // extremely noisy.

    if path.is_vendored_path() {
        tracing::trace!("Not checking {path} because it is a vendored path");
        return false;
    }

    match project.check_mode(db) {
        CheckMode::OpenFiles => {
            let should_check = project.open_files(db).contains(&file);
            if !should_check {
                tracing::trace!(
                    "Not checking {path} because check mode is `OpenFiles` \
                     and it is not in the open file set"
                );
            }
            should_check
        }
        CheckMode::AllFiles => {
            // Virtual files are always checked.
            //
            // We also check the open file set. In theory, we shouldn't need to do this since it is
            // accounted for by the virtual file check (for the case when a file wants to be checked
            // but isn't saved to disk yet). However, not all clients follow the LSP convention that
            // URIs for documents not on disk yet use the `untitled://...` scheme. That is, we assume
            // that a `file://...` scheme corresponds to a saved file on disk, and anything else is
            // "virtual." For example, neovim uses `file://...` even for an open buffer that does not
            // correspond to a file saved to disk yet.
            if path.is_system_virtual_path() {
                return true;
            }

            let should_check =
                project.files(db).contains(&file) || project.open_files(db).contains(&file);
            if !should_check {
                tracing::trace!(
                    "Not checking {path} because check mode is `AllFiles` \
                     and it is not a virtual path, in the project files \
                     or in the open file set"
                );
            }
            should_check
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectReloadResult {
    /// Neither project metadata nor settings changed.
    Unchanged,
    /// Project metadata or settings changed.
    Changed {
        /// Whether the indexed project files changed.
        files_changed: bool,
    },
}

#[salsa::tracked(returns(as_deref), heap_size=ruff_memory_usage::heap_size)]
pub(crate) fn check_file_impl(
    db: &dyn Db,
    file: PythonFile<'_>,
) -> Result<Box<[Diagnostic]>, Diagnostic> {
    let source_file = file.file(db);
    {
        let db = AssertUnwindSafe(db);
        match catch(&**db, source_file, || {
            ty_python_semantic::check_file(*db, file)
        }) {
            Ok(result) => result,
            Err(diagnostic) => Ok(Box::new([diagnostic])),
        }
    }
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

    fn diagnostics(&self) -> &[Diagnostic] {
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

fn catch<F, R>(db: &dyn Db, file: File, f: F) -> Result<R, Diagnostic>
where
    F: FnOnce() -> R + UnwindSafe,
{
    match ruff_db::panic::catch_unwind(f) {
        Ok(result) => Ok(result),
        Err(error) => {
            match error.payload.downcast_ref::<salsa::Cancelled>() {
                None => {
                    // Add a diagnostic (by not early returning) for
                    // any non Salsa panic (a bug in ty)
                }
                Some(salsa::Cancelled::PropagatedPanic) => {
                    // Add a diagnostic for propagated Salsa panics. That is, query `A`
                    // running on thread `a` depends on query `B` running on thread `b`
                    // and query `B` panics. However, avoid adding such a diagnostic
                    // if query `B` panicked because of a cancellation by calling
                    // `unwind_if_revision_cancelled`.
                    //
                    // The propagated Salsa panic isn't very actionable for users,
                    // but it can be useful to know that file A failed to type check
                    // because file B panicked (both files will have a panic-diagnostic).
                    db.unwind_if_revision_cancelled();
                }

                // For any pending write or local cancellation, resume the panic to abort the outer query.
                Some(_) => {
                    error.resume_unwind();
                }
            }

            let message = error.to_diagnostic_message(Some(file.path(db)));
            let mut diagnostic = Diagnostic::new(DiagnosticId::Panic, Severity::Fatal, message);
            diagnostic.add_bug_sub_diagnostics("%5Bpanic%5D");

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

            // Report an untracked read because Salsa didn't carry over
            // the dependencies of any query called by `f` because it panicked.
            db.report_untracked_read();

            Err(diagnostic)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check_file_impl;
    use crate::db::Db as _;
    use crate::db::testing::TestDb;
    use crate::{IncludeResult, ProjectMetadata};
    use ruff_db::PythonFile;
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::source_text;
    use ruff_db::system::{DbWithTestSystem, DbWithWritableSystem as _, SystemPath, SystemPathBuf};
    use ruff_db::testing::assert_function_query_was_not_run;
    use ty_module_resolver::Db as _;
    use ty_python_semantic::types::check_types;

    #[test]
    fn check_file_skips_type_checking_when_file_cant_be_read() -> ruff_db::system::Result<()> {
        let project = ProjectMetadata::new("test", SystemPathBuf::from("/"));
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
            check_file_impl(&db, PythonFile::new(&db, file, db.python_version()))
                .as_ref()
                .unwrap_err()
                .primary_message()
                .to_string(),
            "Failed to read file: No such file or directory".to_string()
        );

        let events = db.take_salsa_events();
        assert_function_query_was_not_run(
            &db,
            check_types,
            PythonFile::new(&db, file, db.python_version()),
            &events,
        );

        // The user now creates a new file with an empty text. The source text
        // content returned by `source_text` remains unchanged, but the diagnostics should get updated.
        db.write_file(path, "").unwrap();

        assert_eq!(source_text(&db, file).as_str(), "");
        assert_eq!(
            check_file_impl(&db, PythonFile::new(&db, file, db.python_version()))
                .as_ref()
                .unwrap()
                .iter()
                .map(|diagnostic| diagnostic.primary_message().to_string())
                .collect::<Vec<_>>(),
            vec![] as Vec<String>
        );

        Ok(())
    }

    #[test]
    fn explicit_nested_included_file_is_a_literal_match() {
        let root = SystemPathBuf::from("/project");
        let explicit_file = root.join("build/keep.txt");
        let project = ProjectMetadata::new("test", root.clone());
        let mut db = TestDb::new(project);
        let project = db.project();

        project.set_included_paths(&mut db, vec![root, explicit_file.clone()]);

        assert_eq!(
            project.is_file_included(&db, &explicit_file),
            IncludeResult::Included {
                literal_match: Some(true)
            }
        );
    }
}
