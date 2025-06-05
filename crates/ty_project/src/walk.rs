use crate::{Db, IOErrorDiagnostic, IOErrorKind, Project};
use globset::{Candidate, GlobBuilder, GlobSet, GlobSetBuilder};
use regex_automata::util::pool::Pool;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::walk_directory::{ErrorKind, WalkDirectoryBuilder, WalkState};
use ruff_db::system::{FileType, SystemPath, SystemPathBuf};
use ruff_python_ast::PySourceType;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
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

    files_patterns: &'a FilePatterns,

    project_root: &'a SystemPath,

    /// The filter skips checking if the path is in `included_paths` if set to `true`.
    ///
    /// Skipping this check is useful when the walker only walks over `included_paths`.
    skip_included_paths: bool,
}

impl<'a> ProjectFilesFilter<'a> {
    pub(crate) fn from_project(db: &'a dyn Db, project: Project) -> Self {
        Self {
            included_paths: project.included_paths_or_root(db),
            project_root: project.root(db),
            files_patterns: &project.settings(db).src().files,
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
    pub(crate) fn is_included(&self, path: &SystemPath, is_directory: bool) -> bool {
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
                if path == self.project_root {
                    return true;
                }

                // TODO: Do we need to use `matched_path_or_any_parents` when not walking?

                let matched = self.files_patterns.matches_path(path, is_directory);
                tracing::debug!("path `{path} matches {matched:?}");
                // TODO: For partial matches, only include the file if it is included by the project's include/exclude settings.
                match matched {
                    // We need to traverse directories that don't match because `a` doesn't match the pattern `a/b/c/d.py`
                    // but we need to traverse the directory to successfully match `a/b/c/d.py`.
                    // This is very unfortunate because it means ty traverses all directories when e.g. using `files = ["src"]`.
                    // TODO(micha): 04.06.2025: It would be nice if we could avoid traversing directories
                    // that are known can never match because they don't share a common prefix with any of the globs.
                    // But we'd need to be careful in the precense of `**/test` patterns because they can match any path.
                    PatternMatch::None => true,
                    PatternMatch::Exclude(_) => false,
                    PatternMatch::Include => true,
                }
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
            .standard_filters(db.project().settings(db).src().respect_ignore_files)
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
                        if !self
                            .filter
                            .is_included(entry.path(), entry.file_type().is_directory())
                        {
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

#[derive(Clone)]
pub struct FilePatterns {
    set: GlobSet,
    patterns: Box<[FilePattern]>,
    matches: Option<Arc<Pool<Vec<usize>>>>,
    static_prefixes: IncludedPrefixes,
    num_positive: usize,
}

impl FilePatterns {
    pub(crate) fn empty() -> Self {
        Self {
            set: GlobSet::empty(),
            patterns: Box::default(),
            matches: None,
            static_prefixes: IncludedPrefixes::new(),
            num_positive: 0,
        }
    }

    pub(crate) fn matches(&self, path: impl AsRef<SystemPath>) -> PatternMatch {
        self.matches_path(path.as_ref(), false)
    }

    pub(crate) fn matches_directory(&self, path: impl AsRef<SystemPath>) -> PatternMatch {
        self.matches_path(path.as_ref(), true)
    }

    pub(crate) fn matches_path(&self, path: &SystemPath, is_directory: bool) -> PatternMatch {
        debug_assert!(path.is_absolute(), "Path {path} isn't absolute");
        if self.patterns.is_empty() {
            return PatternMatch::None;
        }

        let candidate = Candidate::new(path);
        let mut matches = self.matches.as_ref().unwrap().get();
        self.set.matches_candidate_into(&candidate, &mut *matches);

        for &i in matches.iter().rev() {
            let pattern = &self.patterns[i];

            if pattern.is_only_directory && !is_directory {
                continue;
            }

            return if pattern.negated {
                PatternMatch::Exclude(ExcludeReason::Match)
            } else {
                PatternMatch::Include
            };
        }

        if self.num_positive > 0 {
            if is_directory {
                // Skip directories for which we know that no glob has a shared prefix with.
                // E.g. if `files = ["src"], skip `tests`
                if dbg!(self.static_prefixes.is_statically_excluded(path)) {
                    return PatternMatch::Exclude(ExcludeReason::NoIncludePattern);
                }
            } else {
                // If this is a file and there's at least one include pattern but the file doesn't match it,
                // then the file is excluded. If there are only exclude patterns, than the file should be included.
                return PatternMatch::Exclude(ExcludeReason::NoIncludePattern);
            }
        }

        PatternMatch::None
    }

    pub(crate) fn match_once(&self, path: &SystemPath) -> PatternMatch {
        for parent in path.ancestors().skip(1) {
            match self.matches_directory(parent) {
                PatternMatch::Include | PatternMatch::None => {
                    continue;
                }
                PatternMatch::Exclude(exclude_reason) => {
                    return PatternMatch::Exclude(exclude_reason);
                }
            }
        }

        // At this point it is known that no parent path is excluded.
        // TODO: This could be adirectory too
        self.matches(path)
    }
}

impl PartialEq for FilePatterns {
    fn eq(&self, other: &Self) -> bool {
        self.patterns == other.patterns
    }
}

impl Eq for FilePatterns {}

impl std::fmt::Debug for FilePatterns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilePatterns")
            .field("patterns", &self.patterns)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct FilePatternsBuilder {
    set: GlobSetBuilder,
    patterns: Vec<FilePattern>,
    static_prefixes: IncludedPrefixes,
    num_positive: usize,
}

impl FilePatternsBuilder {
    pub(crate) fn new() -> Self {
        Self {
            set: GlobSetBuilder::new(),
            patterns: Vec::new(),
            static_prefixes: IncludedPrefixes::new(),
            num_positive: 0,
        }
    }

    pub(crate) fn add(&mut self, input: &str) -> Result<&mut Self, globset::Error> {
        let mut pattern = FilePattern {
            negated: false,
            is_only_directory: false,
            original: input.to_string(),
        };

        let mut glob = input;

        if let Some(after) = glob.strip_prefix('!') {
            pattern.negated = true;
            glob = after;
        }

        debug_assert!(
            SystemPath::new(glob).is_absolute(),
            "The glob {input} isn't anchored"
        );

        // A pattern ending with a `/` should only match directories. E.g. `src/` only matches directories
        // whereas `src` matches both files and directories.
        // We need to remove the `/` to ensure that a path missing the trailing `/` matches.
        if let Some(before) = glob.strip_suffix('/') {
            pattern.is_only_directory = true;
            glob = before;

            // If the slash was escaped, then remove the escape.
            // See: https://github.com/BurntSushi/ripgrep/issues/2236
            let trailing_backslashes = glob.chars().rev().filter(|c| *c == '\\').count();
            if trailing_backslashes % 2 == 1 {
                glob = &glob[..glob.len() - trailing_backslashes]
            }
        }

        // If the last component contains no wildcards or extension, consider it an implicit glob
        // This turns `src` into `src/**/*`
        // TODO: Should we also enable this behavior for `is_only_directory` patterns?
        if !glob.ends_with("**") && !pattern.negated {
            let parsed = GlobBuilder::new(&format!("{glob}/**"))
                .literal_separator(true)
                .backslash_escape(true)
                // TODO: Map the error to the pattern the user provided.
                .build()?;

            self.set.add(parsed);
            self.patterns.push(FilePattern {
                is_only_directory: false,
                ..pattern.clone()
            });
        }

        let mut actual = Cow::Borrowed(glob);

        // If the glob ends with `/**`, then we should only match everything
        // inside a directory, but not the directory itself. Standard globs
        // will match the directory. So we add `/*` to force the issue.
        if actual.ends_with("/**") {
            actual = Cow::Owned(format!("{}/*", actual));
        }

        // Unlike gitignore, anchor paths (don't insert a `**` prefix).
        let parsed = GlobBuilder::new(&*actual)
            .literal_separator(true)
            .backslash_escape(true)
            // TODO: Map the error to the pattern the user provided.
            .build()?;

        if !pattern.negated {
            self.num_positive += 1;

            let mut parent = self.static_prefixes.root_mut();

            // Do a best effort at extracting a static prefix from a positive include match.
            // This allows short-circuting traversal of folders that are known to not overlap with any positive
            // match. However, we have to be careful. Any path starting with a `**` requires visiting all folders.
            for component in SystemPath::new(glob).components() {
                if glob::Pattern::escape(component.as_str()) != component.as_str() {
                    *parent = IncludedPrefix::Dynamic;
                    break;
                }

                let static_parent = match parent {
                    IncludedPrefix::Dynamic => {
                        break;
                    }
                    IncludedPrefix::Static(static_prefix) => static_prefix,
                };

                parent = static_parent
                    .0
                    .entry(component.to_string())
                    .or_insert_with(|| IncludedPrefix::Static(StaticPrefix::default()));
            }
        }

        self.set.add(parsed);
        self.patterns.push(pattern);

        Ok(self)
    }

    pub(crate) fn build(self) -> Result<FilePatterns, globset::Error> {
        Ok(FilePatterns {
            set: self.set.build()?,
            patterns: self.patterns.into(),
            matches: Some(Arc::new(Pool::new(|| vec![]))),
            static_prefixes: self.static_prefixes,
            num_positive: self.num_positive,
        })
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum PatternMatch {
    /// The highest precedence pattern is an include pattern.
    Include,

    /// The highest precedence pattern is a negated pattern (the file should not be included).
    Exclude(ExcludeReason),

    /// No pattern matched the path.
    None,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ExcludeReason {
    /// The path is excluded because it matches a negative pattern.
    Match,

    /// It's a path that doesn't match any include pattern.
    NoIncludePattern,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FilePattern {
    /// The pattern as specified by the user.
    original: String,

    /// Whether the glob should only match directories (`src/` matches only directories).
    is_only_directory: bool,

    /// Whether this pattern was negated.
    negated: bool,
}

fn is_implicit_glob(pattern: &str) -> bool {
    let as_path = SystemPath::new(pattern);

    as_path
        .components()
        .last()
        .is_some_and(|last| !last.as_str().contains(['.', '*', '?']))
}

#[derive(Clone, Debug)]
struct IncludedPrefixes {
    root: IncludedPrefix,
}

impl IncludedPrefixes {
    fn new() -> Self {
        Self {
            root: IncludedPrefix::Static(StaticPrefix(FxHashMap::default())),
        }
    }

    fn root_mut(&mut self) -> &mut IncludedPrefix {
        &mut self.root
    }

    fn is_statically_excluded(&self, path: &SystemPath) -> bool {
        let mut current = &self.root;

        for component in path.components() {
            match current {
                IncludedPrefix::Dynamic => return false,
                IncludedPrefix::Static(static_prefix) => {
                    match static_prefix.0.get(component.as_str()) {
                        Some(parent) => {
                            current = parent;
                        }
                        None => {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }
}

#[derive(Clone, Debug)]
enum IncludedPrefix {
    /// The path contains at least one dynamic child pattern. E.g. if `a/*/b` and `a/c`, then `a` is dynamic because of the first pattern.
    Dynamic,
    /// All it's children are fixed.
    Static(StaticPrefix),
}

#[derive(Clone, Debug, Default)]
struct StaticPrefix(FxHashMap<String, IncludedPrefix>);

#[cfg(test)]
mod tests {
    use crate::walk::{ExcludeReason, FilePatterns, FilePatternsBuilder, PatternMatch};

    fn create_patterns(patterns: impl IntoIterator<Item = &'static str>) -> FilePatterns {
        let mut builder = FilePatternsBuilder::new();

        for pattern in patterns {
            builder.add(pattern).unwrap_or_else(|err| {
                panic!("Invalid pattern '{pattern}`: {err}");
            });
        }

        builder.build().unwrap()
    }

    /// The pattern set matching `**` always returns `Include`
    #[test]
    fn all() {
        let patterns = create_patterns(["/**"]);

        assert_eq!(patterns.matches_directory("/src"), PatternMatch::Include);
        assert_eq!(patterns.matches_directory("/src/"), PatternMatch::Include);

        assert_eq!(patterns.matches_directory("/"), PatternMatch::Include);
        assert_eq!(patterns.matches("/test.py"), PatternMatch::Include);
    }

    /// The empty pattern set always returns `None`.
    #[test]
    fn empty() {
        let patterns = create_patterns([]);

        assert_eq!(patterns.matches("/a.foo"), PatternMatch::None);
        assert_eq!(patterns.matches("/a"), PatternMatch::None);
        assert_eq!(patterns.matches("/"), PatternMatch::None);
    }

    #[test]
    fn simple() {
        let patterns = create_patterns(["/*.foo", "!/*.bar"]);
        assert_eq!(patterns.matches("/a.foo"), PatternMatch::Include);
        assert_eq!(patterns.matches_directory("/a.foo"), PatternMatch::Include);
        assert_eq!(
            patterns.matches("/a.rs"),
            PatternMatch::Exclude(ExcludeReason::NoIncludePattern)
        );
        assert_eq!(patterns.matches_directory("/a.rs"), PatternMatch::None);
        assert_eq!(
            patterns.matches("/a.bar"),
            PatternMatch::Exclude(ExcludeReason::Match)
        );
        assert_eq!(
            patterns.matches_directory("/a.bar"),
            PatternMatch::Exclude(ExcludeReason::Match)
        );
    }

    #[test]
    fn only_excludes() {
        let patterns = create_patterns(["!/*.bar"]);
        assert_eq!(patterns.matches("/a.rs"), PatternMatch::None);
        assert_eq!(patterns.matches_directory("/a.rs"), PatternMatch::None);
        assert_eq!(
            patterns.matches("/a.bar"),
            PatternMatch::Exclude(ExcludeReason::Match)
        );
        assert_eq!(
            patterns.matches_directory("/a.bar"),
            PatternMatch::Exclude(ExcludeReason::Match)
        );
    }

    #[test]
    fn precedence() {
        let patterns = create_patterns(["/*.foo", "!/*.bar.foo"]);
        assert_eq!(patterns.matches("/a.foo"), PatternMatch::Include);
        assert_eq!(
            patterns.matches("/a.baz"),
            PatternMatch::Exclude(ExcludeReason::NoIncludePattern)
        );
        assert_eq!(
            patterns.matches("/a.bar.foo"),
            PatternMatch::Exclude(ExcludeReason::Match)
        );
    }

    /// `directory/` matches the directory `directory` and its contents. It doesn't match files.
    #[test]
    fn implicit_directory_pattern() {
        let patterns = create_patterns(["/src/"]);

        assert_eq!(patterns.matches_directory("/src"), PatternMatch::Include);
        assert_eq!(patterns.matches_directory("/src/"), PatternMatch::Include);

        // Don't include files, because the pattern ends with `/`
        assert_eq!(
            patterns.matches("/src"),
            PatternMatch::Exclude(ExcludeReason::NoIncludePattern)
        );

        // But include the content of src
        assert_eq!(patterns.matches("/src/test.py"), PatternMatch::Include);

        // Deep nesting
        assert_eq!(
            patterns.matches("/src/glob/builder.py"),
            PatternMatch::Include
        );

        // Or a file with the same name
        assert_eq!(patterns.matches("/src/src"), PatternMatch::Include);

        // Or a directory with the same name
        assert_eq!(
            patterns.matches_directory("/src/src"),
            PatternMatch::Include
        );
    }

    #[test]
    fn implicit_pattern() {
        // Patterns ending without a slash include both files and directories.
        // It includes all files in said directory
        let patterns = create_patterns(["/src"]);

        assert_eq!(patterns.matches_directory("/src"), PatternMatch::Include);
        assert_eq!(patterns.matches("/src/"), PatternMatch::Include);

        // Also include files
        assert_eq!(patterns.matches("/src"), PatternMatch::Include);

        assert_eq!(patterns.matches("/src/test.py"), PatternMatch::Include);

        // Deep nesting
        assert_eq!(
            patterns.matches("/src/glob/builder.py"),
            PatternMatch::Include
        );

        // Or a file with the same name
        assert_eq!(patterns.matches("/src/src"), PatternMatch::Include);

        // Or a directory with the same name
        assert_eq!(
            patterns.matches_directory("/src/src"),
            PatternMatch::Include
        );
    }

    /// Patterns where the last part has an extension match files or directories (without their content).
    #[test]
    fn pattern_with_extension() {
        let patterns = create_patterns(["/test.py"]);

        assert_eq!(
            patterns.matches_directory("/test.py"),
            PatternMatch::Include
        );
        assert_eq!(
            patterns.matches_directory("/test.py"),
            PatternMatch::Include
        );

        assert_eq!(
            patterns.matches("/test.py/abcd"),
            PatternMatch::Exclude(ExcludeReason::NoIncludePattern)
        );

        assert_eq!(
            patterns.matches_directory("/test.py/abcd"),
            PatternMatch::Exclude(ExcludeReason::NoIncludePattern)
        );
    }

    /// Tests that `matches` returns `Exclude` if:
    ///
    /// * There's at least one include
    /// * The parent component of `paths` are known to not overlap with any include pattern
    ///
    /// This allows to avoid traversing directories for which it is known that no file will match
    /// any include pattern. For example, we want to avoid traversing `tests` if the pattern is `["src"]`.
    #[test]
    fn directory_pruning() {
        let patterns = create_patterns(["/a/b/test-*/d", "/a/b/c/e", "/b/c"]);

        // Paths that can be statically pruned because they match no known prefix
        assert_eq!(
            patterns.matches_directory("/a/x"),
            PatternMatch::Exclude(ExcludeReason::NoIncludePattern),
        );

        assert_eq!(
            patterns.matches_directory("/x"),
            PatternMatch::Exclude(ExcludeReason::NoIncludePattern),
        );

        // Paths that are known to be included
        assert_eq!(patterns.matches_directory("/b/c"), PatternMatch::Include);
        assert_eq!(
            patterns.matches_directory("/a/b/test-x/d"),
            PatternMatch::Include
        );
        assert_eq!(
            patterns.matches_directory("/a/b/c/e"),
            PatternMatch::Include
        );

        // Path's that can't be pruned because they could match the `test-*` wildcard pattern
        assert_eq!(patterns.matches_directory("/a/b/b/d"), PatternMatch::None);

        // Path's that can't be pruned because they match a known prefix (in this case `/b/c`) but they don't
        // match a pattern themselves
        assert_eq!(patterns.matches_directory("/b"), PatternMatch::None)
    }

    #[test]
    fn prefix_wildcard_include() {
        let patterns = create_patterns(["/**/test/**", "/a/b/c/e", "/b/c"]);

        assert_eq!(
            patterns.matches_directory("/src/test/"),
            PatternMatch::Include
        );
        assert_eq!(
            patterns.matches_directory("/a/b/c/e"),
            PatternMatch::Include
        );
        assert_eq!(patterns.matches_directory("/b/c"), PatternMatch::Include);

        // We can't skip over the following directories because of the `**` wildcard
        assert_eq!(
            patterns.matches_directory("/not_included/a/b"),
            PatternMatch::None
        );
    }

    #[test]
    fn nested_prefix_wildcard_include() {
        let patterns = create_patterns(["/src/**/test", "/a/b", "/src/abcd/main.py"]);

        assert_eq!(patterns.matches_directory("/a/b"), PatternMatch::Include);
        assert_eq!(
            patterns.matches_directory("/src/test"),
            PatternMatch::Include
        );

        // We can't skip over the following directories because of the `**` wildcard
        assert_eq!(
            patterns.matches_directory("/src/not_included/a/b"),
            PatternMatch::None
        );
    }
}
