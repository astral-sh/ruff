use ruff_db::system::{System, SystemPath, SystemPathBuf};
use rustc_hash::FxHashMap;

/// Cached ignore-file state for one watcher change batch.
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
pub(crate) struct IgnoreFiles {
    walk_roots: Vec<SystemPathBuf>,
    directories: FxHashMap<SystemPathBuf, DirectoryIgnoreFiles>,
    global_gitignore: Option<ignore::gitignore::Gitignore>,
}

impl IgnoreFiles {
    pub(crate) fn new(walk_roots: &[SystemPathBuf]) -> Self {
        Self {
            walk_roots: walk_roots.to_vec(),
            directories: FxHashMap::default(),
            global_gitignore: None,
        }
    }

    /// Returns `true` if every matching project walk root would skip `path`.
    pub(crate) fn is_ignored(
        &mut self,
        system: &dyn System,
        path: &SystemPath,
        is_directory: bool,
    ) -> bool {
        let matching_roots = self
            .walk_roots
            .iter()
            .filter(|root| path.starts_with(root))
            .cloned()
            .collect::<Vec<_>>();

        if matching_roots.is_empty() {
            return false;
        }

        matching_roots
            .iter()
            .all(|root| self.is_ignored_from_root(system, root, path, is_directory))
    }

    fn is_ignored_from_root(
        &mut self,
        system: &dyn System,
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

        let mut active_directories = root
            .parent()
            .into_iter()
            .flat_map(SystemPath::ancestors)
            .map(SystemPath::to_path_buf)
            .collect::<Vec<_>>();
        active_directories.reverse();

        // Once the walker has accepted the root directory, it reads the root's
        // ignore files before deciding whether to visit any child paths.
        active_directories.push(root.to_path_buf());

        let mut current_path = root.to_path_buf();
        let mut components = relative_path.components().peekable();

        while let Some(component) = components.next() {
            current_path.push(component.as_str());

            let is_last_component = components.peek().is_none();
            let current_path_is_directory = !is_last_component || is_directory;

            if self
                .ignored_by_active_ignore_files(
                    system,
                    &active_directories,
                    &current_path,
                    current_path_is_directory,
                )
                .unwrap_or(false)
            {
                return true;
            }

            if !is_last_component {
                active_directories.push(current_path.clone());
            }
        }

        false
    }

    fn ignored_by_active_ignore_files(
        &mut self,
        system: &dyn System,
        active_directories: &[SystemPathBuf],
        path: &SystemPath,
        is_directory: bool,
    ) -> Option<bool> {
        if let Some(is_ignored) =
            self.ignored_by_ignore_files(system, active_directories, path, is_directory)
        {
            return Some(is_ignored);
        }

        if !self.has_git_repository(system, active_directories) {
            return None;
        }

        if let Some(is_ignored) =
            self.ignored_by_gitignore_files(system, active_directories, path, is_directory)
        {
            return Some(is_ignored);
        }

        if let Some(is_ignored) =
            self.ignored_by_git_exclude(system, active_directories, path, is_directory)
        {
            return Some(is_ignored);
        }

        self.ignored_by_global_gitignore(system, path, is_directory)
    }

    fn ignored_by_ignore_files(
        &mut self,
        system: &dyn System,
        active_directories: &[SystemPathBuf],
        path: &SystemPath,
        is_directory: bool,
    ) -> Option<bool> {
        active_directories.iter().rev().find_map(|directory| {
            let matcher = self.directory(system, directory).ignore.as_ref()?;
            ignored_by_match(&matcher.matched(path.as_std_path(), is_directory))
        })
    }

    fn ignored_by_gitignore_files(
        &mut self,
        system: &dyn System,
        active_directories: &[SystemPathBuf],
        path: &SystemPath,
        is_directory: bool,
    ) -> Option<bool> {
        let mut saw_git_repository = false;

        for directory in active_directories.iter().rev() {
            let directory_ignore_files = self.directory(system, directory);

            if !saw_git_repository
                && let Some(matcher) = directory_ignore_files.gitignore.as_ref()
                && let Some(is_ignored) =
                    ignored_by_match(&matcher.matched(path.as_std_path(), is_directory))
            {
                return Some(is_ignored);
            }

            saw_git_repository |= directory_ignore_files.has_git_repository;
        }

        None
    }

    fn ignored_by_git_exclude(
        &mut self,
        system: &dyn System,
        active_directories: &[SystemPathBuf],
        path: &SystemPath,
        is_directory: bool,
    ) -> Option<bool> {
        let mut saw_git_repository = false;

        for directory in active_directories.iter().rev() {
            let directory_ignore_files = self.directory(system, directory);

            if !saw_git_repository
                && let Some(matcher) = directory_ignore_files.git_exclude.as_ref()
                && let Some(is_ignored) =
                    ignored_by_match(&matcher.matched(path.as_std_path(), is_directory))
            {
                return Some(is_ignored);
            }

            saw_git_repository |= directory_ignore_files.has_git_repository;
        }

        None
    }

    fn ignored_by_global_gitignore(
        &mut self,
        system: &dyn System,
        path: &SystemPath,
        is_directory: bool,
    ) -> Option<bool> {
        let matcher = self.global_gitignore.get_or_insert_with(|| {
            let cwd = system.current_directory();
            let (matcher, error) =
                ignore::gitignore::GitignoreBuilder::new(cwd.as_std_path()).build_global();

            if let Some(error) = error {
                tracing::warn!("Failed to read global gitignore: {error}");
            }

            matcher
        });

        ignored_by_match(&matcher.matched(path.as_std_path(), is_directory))
    }

    fn has_git_repository(
        &mut self,
        system: &dyn System,
        active_directories: &[SystemPathBuf],
    ) -> bool {
        active_directories
            .iter()
            .rev()
            .any(|directory| self.directory(system, directory).has_git_repository)
    }

    fn directory(&mut self, system: &dyn System, directory: &SystemPath) -> &DirectoryIgnoreFiles {
        if !self.directories.contains_key(directory) {
            let ignore_files = DirectoryIgnoreFiles::read(system, directory);
            self.directories
                .insert(directory.to_path_buf(), ignore_files);
        }

        self.directories
            .get(directory)
            .expect("directory ignore files to be inserted before lookup")
    }
}

struct DirectoryIgnoreFiles {
    ignore: Option<ignore::gitignore::Gitignore>,
    gitignore: Option<ignore::gitignore::Gitignore>,
    git_exclude: Option<ignore::gitignore::Gitignore>,
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
) -> Option<ignore::gitignore::Gitignore> {
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
        .map(|path| SystemPathBuf::from(path.to_string()))?;
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
        Some(SystemPathBuf::from(common_directory.to_string()))
    }
}

fn ignore_file_matcher(
    system: &dyn System,
    ignore_file: &SystemPath,
    root: &SystemPath,
) -> Option<ignore::gitignore::Gitignore> {
    const UTF8_BOM: &str = "\u{feff}";

    let contents = match system.read_to_string(ignore_file) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            tracing::warn!("Failed to read ignore file `{ignore_file}`: {error}");
            return None;
        }
    };

    let mut builder = ignore::gitignore::GitignoreBuilder::new(root.as_std_path());

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
            ignore::gitignore::Gitignore::empty()
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

    use super::IgnoreFiles;

    fn is_ignored(system: &InMemorySystem, path: &SystemPath, is_directory: bool) -> bool {
        let root = system.current_directory().to_path_buf();
        IgnoreFiles::new(std::slice::from_ref(&root)).is_ignored(system, path, is_directory)
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
}
