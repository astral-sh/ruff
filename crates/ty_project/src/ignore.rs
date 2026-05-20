//! Single-path ignore-file matching with project-walk semantics.
//!
//! A normal project file walk delegates ignore handling to `ignore::WalkBuilder`.
//! Some callers need the same decision for one concrete file or directory
//! without walking an entire subtree.
//!
//! `IgnoreFiles` answers that question by replaying the relevant walk branch
//! from each project walk root to the changed path. At each branch component it
//! asks whether the active ignore files would prune that component. If an
//! intermediate directory is ignored, nested ignore files below it are never
//! considered, matching the full walker's pruning behavior.
//!
//! The active directory list represents ignore files that the walker would
//! already have discovered at the current branch position. It contains:
//!
//! - canonical parent directories above the explicit walk root, matching
//!   `ignore::Ignore::add_parents`;
//! - the walk root itself once depth-0 admission has happened; and
//! - descendant directories accepted while replaying the branch.
//!
//! Matching needs both the lexical walked path and, when available, the
//! canonicalized candidate path. Parent ignore files discovered above a
//! symlinked root match against the canonical path, while ignore files loaded
//! at or below the explicit root match against the walked path. `ActiveDirectory`
//! and `CandidatePaths` keep that distinction local.
//!
//! Parsed ignore files are cached per directory within one `IgnoreFiles`
//! instance so repeated checks do not keep reparsing the same ignore files.

use ignore::gitignore;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use rustc_hash::FxHashMap;

/// Cached ignore-file state for single-path project-walk checks.
///
/// `ignore::WalkBuilder` decides whether to descend into a directory before it
/// reads ignore files inside that directory. This checker mirrors that behavior
/// along the single branch from an active project walk root to a changed path.
/// A nested ignore file therefore cannot re-include a path below a directory
/// that the normal project walk would have pruned already.
///
/// Directory entries in `directories` are populated lazily. The presence of an
/// entry means we already checked that directory for ignore files, and any
/// parsed matchers are reused by later watcher events in the same batch.
pub(crate) struct IgnoreFiles<'a> {
    walk_roots: &'a [SystemPathBuf],
    system: Box<dyn System>,
    directories: FxHashMap<SystemPathBuf, DirectoryIgnoreFiles>,
    global_gitignore: Option<gitignore::Gitignore>,
}

impl<'a> IgnoreFiles<'a> {
    pub(crate) fn new(system: Box<dyn System>, walk_roots: &'a [SystemPathBuf]) -> Self {
        Self {
            walk_roots,
            system,
            directories: FxHashMap::default(),
            global_gitignore: None,
        }
    }

    /// Returns `true` if every matching project walk root would skip `path`.
    pub(crate) fn is_ignored(&mut self, path: &SystemPath, is_directory: bool) -> bool {
        let walk_roots = self.walk_roots;
        let mut matching_roots = walk_roots
            .iter()
            .filter(|root| path.starts_with(root))
            .peekable();

        if matching_roots.peek().is_none() {
            return false;
        };

        matching_roots.all(|root| self.is_ignored_from_root(root, path, is_directory))
    }

    fn is_ignored_from_root(
        &mut self,
        root: &SystemPath,
        path: &SystemPath,
        is_directory: bool,
    ) -> bool {
        if path == root {
            // Walk roots are yielded at depth 0 before ignore filtering runs.
            return false;
        }

        let Ok(relative_path) = path.strip_prefix(root) else {
            return false;
        };

        // `ignore::Ignore::add_parents` skips parent matcher setup when the
        // walk root cannot be canonicalized. Keep walking from the requested
        // root in that case, but omit canonical parent ignore files.
        let canonical_root = self.system.canonicalize_path(root).ok();
        let mut active_directories = canonical_root
            .as_deref()
            .into_iter()
            .flat_map(SystemPath::ancestors)
            .skip(1)
            .map(|directory| ActiveDirectory::canonical_parent(directory.to_path_buf()))
            .collect::<Vec<_>>();
        active_directories.reverse();

        // Once the walker has accepted the root directory, it reads the root's
        // ignore files before deciding whether to visit any child paths.
        active_directories.push(ActiveDirectory::walked(root.to_path_buf()));

        let mut current_paths = CandidatePaths::new(root.to_path_buf(), canonical_root);
        let mut components = relative_path.components().peekable();

        // Replay the walk one branch component at a time. A directory must be
        // admitted before ignore files inside it can affect deeper descendants.
        while let Some(component) = components.next() {
            current_paths.push(component);

            let is_last_component = components.peek().is_none();
            let current_path_is_directory = !is_last_component || is_directory;

            if self.ignored_by_active_ignore_files(
                &active_directories,
                &current_paths,
                current_path_is_directory,
            ) == Some(true)
            {
                return true;
            }

            if !is_last_component {
                active_directories.push(ActiveDirectory::walked(
                    current_paths.walked().to_path_buf(),
                ));
            }
        }

        false
    }

    /// Returns whether ignore files already visible at the current branch
    /// position would prune `paths`.
    ///
    /// The active directories intentionally exclude ignore files below the
    /// candidate path. This keeps nested allowlists from reviving paths in a
    /// directory the real walker would already have skipped.
    fn ignored_by_active_ignore_files(
        &mut self,
        active_directories: &[ActiveDirectory],
        paths: &CandidatePaths,
        is_directory: bool,
    ) -> Option<bool> {
        if let Some(is_ignored) =
            self.ignored_by_ignore_files(active_directories, paths, is_directory)
        {
            return Some(is_ignored);
        }

        if !self.has_git_repository(active_directories) {
            return None;
        }

        if let Some(is_ignored) =
            self.ignored_by_gitignore_files(active_directories, paths, is_directory)
        {
            return Some(is_ignored);
        }

        if let Some(is_ignored) =
            self.ignored_by_git_exclude(active_directories, paths, is_directory)
        {
            return Some(is_ignored);
        }

        self.ignored_by_global_gitignore(paths.walked(), is_directory)
    }

    fn ignored_by_ignore_files(
        &mut self,
        active_directories: &[ActiveDirectory],
        paths: &CandidatePaths,
        is_directory: bool,
    ) -> Option<bool> {
        active_directories.iter().rev().find_map(|directory| {
            let matcher = self.ignore_files(directory.path()).ignore.as_ref()?;
            let path = directory.candidate_path(paths)?;
            ignored_by_match(&matcher.matched(path.as_std_path(), is_directory))
        })
    }

    fn ignored_by_gitignore_files(
        &mut self,
        active_directories: &[ActiveDirectory],
        paths: &CandidatePaths,
        is_directory: bool,
    ) -> Option<bool> {
        let mut saw_git_repository = false;

        for directory in active_directories.iter().rev() {
            let ignore_files = self.ignore_files(directory.path());

            if !saw_git_repository
                && let Some(matcher) = ignore_files.gitignore.as_ref()
                && let Some(path) = directory.candidate_path(paths)
                && let Some(is_ignored) =
                    ignored_by_match(&matcher.matched(path.as_std_path(), is_directory))
            {
                return Some(is_ignored);
            }

            saw_git_repository |= ignore_files.has_git_repository;
        }

        None
    }

    fn ignored_by_git_exclude(
        &mut self,
        active_directories: &[ActiveDirectory],
        paths: &CandidatePaths,
        is_directory: bool,
    ) -> Option<bool> {
        let mut saw_git_repository = false;

        for directory in active_directories.iter().rev() {
            let ignore_files = self.ignore_files(directory.path());

            if !saw_git_repository
                && let Some(matcher) = ignore_files.git_exclude.as_ref()
                && let Some(path) = directory.candidate_path(paths)
                && let Some(is_ignored) =
                    ignored_by_match(&matcher.matched(path.as_std_path(), is_directory))
            {
                return Some(is_ignored);
            }

            saw_git_repository |= ignore_files.has_git_repository;
        }

        None
    }

    fn ignored_by_global_gitignore(
        &mut self,
        path: &SystemPath,
        is_directory: bool,
    ) -> Option<bool> {
        ignored_by_match(
            &self
                .global_gitignore()
                .matched(path.as_std_path(), is_directory),
        )
    }

    fn global_gitignore(&mut self) -> &gitignore::Gitignore {
        self.global_gitignore.get_or_insert_with(|| {
            let cwd = self.system.current_directory();
            let (matcher, error) =
                gitignore::GitignoreBuilder::new(cwd.as_std_path()).build_global();

            if let Some(error) = error {
                tracing::warn!("Failed to read global gitignore: {error}");
            }

            matcher
        })
    }

    fn has_git_repository(&mut self, active_directories: &[ActiveDirectory]) -> bool {
        active_directories
            .iter()
            .rev()
            .any(|directory| self.ignore_files(directory.path()).has_git_repository)
    }

    fn ignore_files(&mut self, directory: &SystemPath) -> &DirectoryIgnoreFiles {
        self.directories
            .entry(directory.to_path_buf())
            .or_insert_with(|| DirectoryIgnoreFiles::read(self.system.as_ref(), directory))
    }
}

/// Candidate paths for the branch component currently being checked.
///
/// `walked` follows the lexical path requested by the project walk. `canonical`
/// tracks the same branch from the canonicalized walk root when that root could
/// be resolved. Parent ignore files use the canonical path, while ignore files
/// loaded at or below the explicit walk root use the walked path.
struct CandidatePaths {
    walked: SystemPathBuf,
    canonical: Option<SystemPathBuf>,
}

impl CandidatePaths {
    fn new(walked: SystemPathBuf, canonical: Option<SystemPathBuf>) -> Self {
        Self { walked, canonical }
    }

    fn push(&mut self, component: impl AsRef<SystemPath>) {
        let component = component.as_ref();
        self.walked.push(component);

        if let Some(canonical) = self.canonical.as_mut() {
            canonical.push(component);
        }
    }

    fn walked(&self) -> &SystemPath {
        &self.walked
    }

    fn canonical(&self) -> Option<&SystemPath> {
        self.canonical.as_deref()
    }
}

struct ActiveDirectory {
    path: SystemPathBuf,
    match_path: MatchPath,
}

impl ActiveDirectory {
    fn canonical_parent(path: SystemPathBuf) -> Self {
        Self {
            path,
            match_path: MatchPath::Canonical,
        }
    }

    fn walked(path: SystemPathBuf) -> Self {
        Self {
            path,
            match_path: MatchPath::Walked,
        }
    }

    fn path(&self) -> &SystemPath {
        &self.path
    }

    fn candidate_path<'a>(&self, paths: &'a CandidatePaths) -> Option<&'a SystemPath> {
        match self.match_path {
            MatchPath::Canonical => paths.canonical(),
            MatchPath::Walked => Some(paths.walked()),
        }
    }
}

enum MatchPath {
    /// Match against the canonicalized candidate path.
    ///
    /// To mirror `ignore::Ignore::add_parents`, parent ignore files above the
    /// configured walk root are discovered by first resolving that root through
    /// the filesystem. If the walk root is a symlink, those parent files belong
    /// to the symlink target's ancestors, so they must match against the
    /// correspondingly resolved candidate path.
    Canonical,

    /// Match against the lexical path observed during the walk.
    ///
    /// Ignore files at the explicit walk root or below it are loaded while
    /// descending the requested path branch, so they keep the walked path.
    Walked,
}

struct DirectoryIgnoreFiles {
    ignore: Option<gitignore::Gitignore>,
    gitignore: Option<gitignore::Gitignore>,
    git_exclude: Option<gitignore::Gitignore>,
    has_git_repository: bool,
}

impl DirectoryIgnoreFiles {
    fn read(system: &dyn System, directory: &SystemPath) -> Self {
        let git_directory = directory.join(".git");
        let has_git_repository =
            system.path_exists(&git_directory) || system.path_exists(&directory.join(".jj"));
        let git_exclude = git_exclude_matcher(system, directory, &git_directory);

        Self {
            ignore: ignore_file_matcher(system, &directory.join(".ignore"), directory),
            gitignore: ignore_file_matcher(system, &directory.join(".gitignore"), directory),
            git_exclude,
            has_git_repository,
        }
    }
}

fn git_exclude_matcher(
    system: &dyn System,
    directory: &SystemPath,
    git_directory: &SystemPath,
) -> Option<gitignore::Gitignore> {
    let git_common_directory = resolve_git_common_directory(system, git_directory)?;
    ignore_file_matcher(
        system,
        &git_common_directory.join("info/exclude"),
        directory,
    )
}

/// Mirrors the linked-worktree `commondir` lookup in
/// [`resolve_git_commondir`](https://github.com/BurntSushi/ripgrep/blob/57c190d56eedac90c061a238b63dbfed434fee50/crates/ignore/src/dir.rs#L884-L936).
fn resolve_git_common_directory(
    system: &dyn System,
    git_directory: &SystemPath,
) -> Option<SystemPathBuf> {
    if system.is_directory(git_directory) {
        return Some(git_directory.to_path_buf());
    }

    if !system.is_file(git_directory) {
        return None;
    }

    let git_file = match system.read_to_string(git_directory) {
        Ok(git_file) => git_file,
        Err(error) => {
            tracing::warn!("Failed to read linked-worktree git dir `{git_directory}`: {error}");
            return None;
        }
    };

    let real_git_directory = git_file
        .lines()
        .next()?
        .strip_prefix("gitdir: ")
        .map(SystemPathBuf::from)?;
    let common_directory_file = real_git_directory.join("commondir");
    let common_directory = match system.read_to_string(&common_directory_file) {
        Ok(common_directory) => common_directory,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            tracing::warn!(
                "Failed to read linked-worktree common dir `{common_directory_file}`: {error}"
            );
            return None;
        }
    };
    let common_directory = common_directory.lines().next()?;

    if common_directory.starts_with('.') {
        Some(real_git_directory.join(common_directory))
    } else {
        Some(SystemPathBuf::from(common_directory))
    }
}

fn ignore_file_matcher(
    system: &dyn System,
    ignore_file: &SystemPath,
    root: &SystemPath,
) -> Option<gitignore::Gitignore> {
    const UTF8_BOM: &str = "\u{feff}";

    let contents = match system.read_to_string(ignore_file) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            tracing::warn!("Failed to read ignore file `{ignore_file}`: {error}");
            return None;
        }
    };

    let mut builder = gitignore::GitignoreBuilder::new(root.as_std_path());

    let contents = contents.trim_start_matches(UTF8_BOM);

    for (line_number, line) in contents.lines().enumerate() {
        if let Err(error) = builder.add_line(Some(ignore_file.as_std_path().to_path_buf()), line) {
            tracing::warn!(
                "Failed to parse ignore file `{ignore_file}` at line {}: {error}",
                line_number + 1
            );
        }
    }

    Some(match builder.build() {
        Ok(matcher) => matcher,
        Err(error) => {
            tracing::warn!("Failed to build ignore matcher for `{ignore_file}`: {error}");
            gitignore::Gitignore::empty()
        }
    })
}

fn ignored_by_match<T>(match_result: &ignore::Match<T>) -> Option<bool> {
    match match_result {
        ignore::Match::None => None,
        ignore::Match::Ignore(_) => Some(true),
        ignore::Match::Whitelist(_) => Some(false),
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::system::{InMemorySystem, System, SystemPath};
    #[cfg(unix)]
    use ruff_db::system::{OsSystem, SystemPathBuf};
    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use super::IgnoreFiles;

    fn is_ignored(system: &InMemorySystem, path: &SystemPath, is_directory: bool) -> bool {
        let root = system.current_directory().to_path_buf();
        IgnoreFiles::new(system.dyn_clone(), std::slice::from_ref(&root))
            .is_ignored(path, is_directory)
    }

    #[test]
    fn nested_ignore_file_allowlist_overrides_parent_file_rule() {
        let system = InMemorySystem::new("/project".into());
        let root = system.current_directory().to_path_buf();
        let path = root.join("pkg/keep.py");

        system
            .fs()
            .write_files_all([
                (root.join(".ignore"), "pkg/keep.py\n"),
                (root.join("pkg/.ignore"), "!keep.py\n"),
                (path.clone(), ""),
            ])
            .unwrap();

        assert!(!is_ignored(&system, &path, false));
    }

    #[test]
    fn nested_ignore_file_cannot_allowlist_below_pruned_directory() {
        let system = InMemorySystem::new("/project".into());
        let root = system.current_directory().to_path_buf();
        let path = root.join("build/keep.py");

        system
            .fs()
            .write_files_all([
                (root.join(".ignore"), "build/\n"),
                (root.join("build/.ignore"), "!keep.py\n"),
                (path.clone(), ""),
            ])
            .unwrap();

        assert!(is_ignored(&system, &path, false));
    }

    #[test]
    fn ignore_file_takes_precedence_over_gitignore_allowlist() {
        let system = InMemorySystem::new("/project".into());
        let root = system.current_directory().to_path_buf();
        let path = root.join("ignored.py");

        system
            .fs()
            .write_files_all([
                (root.join(".git/HEAD"), "ref: refs/heads/main\n"),
                (root.join(".ignore"), "ignored.py\n"),
                (root.join(".gitignore"), "!ignored.py\n"),
                (path.clone(), ""),
            ])
            .unwrap();

        assert!(is_ignored(&system, &path, false));
    }

    #[test]
    fn ignore_file_allowlist_takes_precedence_over_gitignore_ignore() {
        let system = InMemorySystem::new("/project".into());
        let root = system.current_directory().to_path_buf();
        let path = root.join("included.py");

        system
            .fs()
            .write_files_all([
                (root.join(".git/HEAD"), "ref: refs/heads/main\n"),
                (root.join(".ignore"), "!included.py\n"),
                (root.join(".gitignore"), "included.py\n"),
                (path.clone(), ""),
            ])
            .unwrap();

        assert!(!is_ignored(&system, &path, false));
    }

    #[test]
    fn ignore_file_strips_utf8_bom() {
        let system = InMemorySystem::new("/project".into());
        let root = system.current_directory().to_path_buf();
        let path = root.join("ignored.py");

        system
            .fs()
            .write_files_all([
                (root.join(".ignore"), "\u{feff}ignored.py\n"),
                (path.clone(), ""),
            ])
            .unwrap();

        assert!(is_ignored(&system, &path, false));
    }

    #[test]
    fn gitignore_requires_repository() {
        let system = InMemorySystem::new("/project".into());
        let root = system.current_directory().to_path_buf();
        let path = root.join("ignored.py");

        system
            .fs()
            .write_files_all([
                (root.join(".gitignore"), "ignored.py\n"),
                (path.clone(), ""),
            ])
            .unwrap();

        assert!(!is_ignored(&system, &path, false));
    }

    #[test]
    fn git_exclude_ignores_files() {
        let system = InMemorySystem::new("/project".into());
        let root = system.current_directory().to_path_buf();
        let path = root.join("ignored.py");

        system
            .fs()
            .write_files_all([
                (root.join(".git/HEAD"), "ref: refs/heads/main\n"),
                (root.join(".git/info/exclude"), "ignored.py\n"),
                (path.clone(), ""),
            ])
            .unwrap();

        assert!(is_ignored(&system, &path, false));
    }

    #[test]
    fn linked_worktree_git_exclude_ignores_files() {
        let system = InMemorySystem::new("/project".into());
        let root = system.current_directory().to_path_buf();
        let common_git_directory = root.join(".git-common");
        let worktree_git_directory = common_git_directory.join("worktrees/current");
        let commondir_path = worktree_git_directory.join("commondir");
        let path = root.join("ignored.py");

        system
            .fs()
            .write_files_all([
                (
                    root.join(".git"),
                    format!("gitdir: {worktree_git_directory}\n"),
                ),
                (commondir_path.clone(), "../..\n".to_string()),
                (
                    common_git_directory.join("info/exclude"),
                    "ignored.py\n".to_string(),
                ),
                (path.clone(), String::new()),
            ])
            .unwrap();

        assert!(is_ignored(&system, &path, false));

        system
            .fs()
            .write_file_all(&commondir_path, common_git_directory.as_str())
            .unwrap();

        assert!(is_ignored(&system, &path, false));
    }

    #[cfg(unix)]
    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "Symlinked-root canonicalization requires real filesystem fixtures."
    )]
    fn symlinked_walk_root_uses_canonical_parent_ignore_files() -> std::io::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let temp_root = SystemPathBuf::from_path_buf(temp_dir.path().to_path_buf())
            .expect("temporary test path to be UTF-8");
        let real_parent = temp_root.join("real");
        let real_root = real_parent.join("project");
        let symlink_root = temp_root.join("project-link");
        let ignored_path = symlink_root.join("ignored.py");

        fs::create_dir_all(real_root.as_std_path())?;
        fs::write(
            real_parent.join(".ignore").as_std_path(),
            "project/ignored.py\n",
        )?;
        fs::write(real_root.join("ignored.py").as_std_path(), "")?;
        symlink(real_root.as_std_path(), symlink_root.as_std_path())?;

        let system = OsSystem::new(&temp_root);
        assert!(
            IgnoreFiles::new(system.dyn_clone(), std::slice::from_ref(&symlink_root))
                .is_ignored(&ignored_path, false)
        );

        Ok(())
    }
}
