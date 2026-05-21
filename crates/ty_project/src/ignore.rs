//! Single-path ignore-file matching with project-walk semantics.
//!
//! A normal project file walk delegates ignore handling to `ignore::WalkBuilder`.
//! Some callers need the same answer for one concrete file or directory without
//! walking an entire subtree.
//!
//! `IgnoreFiles` replays just the branch from each matching walk root to that
//! candidate path using `ignore::IgnoreState`, which is the same matcher state
//! carried by the full walker. This preserves directory-pruning semantics such
//! as "a nested allowlist below an already ignored directory is never seen."
//!
//! Accepted directory states are cached per walk root. Repeated checks for
//! nearby paths resume from the deepest cached ancestor instead of rebuilding
//! the same parent and child ignore state from the root each time.

use ignore::IgnoreState;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use rustc_hash::FxHashMap;

/// Cached ignore-file state for single-path project-walk checks.
pub(crate) struct IgnoreFiles<'a> {
    walk_roots: &'a [SystemPathBuf],
    base_state: IgnoreState,
    roots: FxHashMap<SystemPathBuf, RootIgnoreStates>,
}

impl<'a> IgnoreFiles<'a> {
    pub(crate) fn new(system: &dyn System, walk_roots: &'a [SystemPathBuf]) -> Self {
        let cwd = system.current_directory();
        let mut builder = ignore::WalkBuilder::new(cwd.as_std_path());
        builder.current_dir(cwd.as_std_path());
        builder.standard_filters(true);
        builder.hidden(false);

        Self {
            walk_roots,
            base_state: builder.build_ignore_state(),
            roots: FxHashMap::default(),
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
        }

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

        let (mut current_path, mut state) = self.deepest_cached_state(root, path);
        let Ok(relative_path) = path.strip_prefix(&current_path) else {
            return false;
        };
        let mut components = relative_path.components().peekable();

        // Replay the remaining branch one component at a time. A directory's
        // ignore files become visible only after that directory itself has
        // been admitted, matching the walker's descend-before-loading order.
        while let Some(component) = components.next() {
            current_path.push(component);

            let is_last_component = components.peek().is_none();
            let current_path_is_directory = !is_last_component || is_directory;

            if state
                .matched(current_path.as_std_path(), current_path_is_directory)
                .is_ignore()
            {
                return true;
            }

            if !is_last_component {
                state = add_child_state(&state, &current_path);
                self.cache_directory_state(root, current_path.clone(), state.clone());
            }
        }

        false
    }

    fn deepest_cached_state(
        &mut self,
        root: &SystemPath,
        path: &SystemPath,
    ) -> (SystemPathBuf, IgnoreState) {
        let root_states = self.root_states(root);

        for ancestor in path.parent().into_iter().flat_map(SystemPath::ancestors) {
            if !ancestor.starts_with(root) {
                break;
            }

            if let Some(state) = root_states.directories.get(ancestor) {
                return (ancestor.to_path_buf(), state.clone());
            }

            if ancestor == root {
                break;
            }
        }

        let state = root_states
            .directories
            .get(root)
            .expect("root state to be inserted before descendant lookup")
            .clone();
        (root.to_path_buf(), state)
    }

    fn cache_directory_state(
        &mut self,
        root: &SystemPath,
        directory: SystemPathBuf,
        state: IgnoreState,
    ) {
        self.root_states(root).directories.insert(directory, state);
    }

    fn root_states(&mut self, root: &SystemPath) -> &mut RootIgnoreStates {
        if !self.roots.contains_key(root) {
            let root_states = RootIgnoreStates::new(&self.base_state, root);
            self.roots.insert(root.to_path_buf(), root_states);
        }

        self.roots
            .get_mut(root)
            .expect("root state to be inserted before lookup")
    }
}

struct RootIgnoreStates {
    directories: FxHashMap<SystemPathBuf, IgnoreState>,
}

impl RootIgnoreStates {
    fn new(base_state: &IgnoreState, root: &SystemPath) -> Self {
        let root_state = add_parent_state(base_state, root);
        let root_state = add_child_state(&root_state, root);

        let mut directories = FxHashMap::default();
        directories.insert(root.to_path_buf(), root_state);

        Self { directories }
    }
}

fn add_parent_state(state: &IgnoreState, root: &SystemPath) -> IgnoreState {
    let (state, error) = state.add_parents(root.as_std_path());

    if let Some(error) = error {
        tracing::warn!("Failed to read parent ignore files for `{root}`: {error}");
    }

    state
}

fn add_child_state(state: &IgnoreState, directory: &SystemPath) -> IgnoreState {
    let (state, error) = state.add_child(directory.as_std_path());

    if let Some(error) = error {
        tracing::warn!("Failed to read ignore files in `{directory}`: {error}");
    }

    state
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ruff_db::system::{OsSystem, System, SystemPath, SystemPathBuf};

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    use super::IgnoreFiles;

    struct Fixture {
        _temp_dir: tempfile::TempDir,
        root: SystemPathBuf,
        system: OsSystem,
    }

    impl Fixture {
        fn new() -> std::io::Result<Self> {
            let temp_dir = tempfile::tempdir()?;
            let root = SystemPathBuf::from_path_buf(temp_dir.path().to_path_buf())
                .expect("temporary test path to be UTF-8");
            let system = OsSystem::new(&root);

            Ok(Self {
                _temp_dir: temp_dir,
                root,
                system,
            })
        }

        fn path(&self, relative: &str) -> SystemPathBuf {
            self.root.join(relative)
        }

        fn write_file(&self, relative: &str, contents: &str) -> std::io::Result<SystemPathBuf> {
            let path = self.path(relative);

            let system = self
                .system
                .as_writable()
                .expect("OS test system to support writes");

            if let Some(parent) = path.parent() {
                system.create_directory_all(parent)?;
            }

            system.write_file(&path, contents)?;
            Ok(path)
        }

        fn is_ignored(&self, path: &SystemPath, is_directory: bool) -> bool {
            IgnoreFiles::new(&self.system, std::slice::from_ref(&self.root))
                .is_ignored(path, is_directory)
        }
    }

    #[test]
    fn nested_ignore_file_allowlist_overrides_parent_file_rule() -> std::io::Result<()> {
        let fixture = Fixture::new()?;
        fixture.write_file(".ignore", "pkg/keep.py\n")?;
        fixture.write_file("pkg/.ignore", "!keep.py\n")?;
        let path = fixture.write_file("pkg/keep.py", "")?;

        assert!(!fixture.is_ignored(&path, false));
        Ok(())
    }

    #[test]
    fn nested_ignore_file_cannot_allowlist_below_pruned_directory() -> std::io::Result<()> {
        let fixture = Fixture::new()?;
        fixture.write_file(".ignore", "build/\n")?;
        fixture.write_file("build/.ignore", "!keep.py\n")?;
        let path = fixture.write_file("build/keep.py", "")?;

        assert!(fixture.is_ignored(&path, false));
        Ok(())
    }

    #[test]
    fn ignore_file_takes_precedence_over_gitignore_allowlist() -> std::io::Result<()> {
        let fixture = Fixture::new()?;
        fixture.write_file(".git/HEAD", "ref: refs/heads/main\n")?;
        fixture.write_file(".ignore", "ignored.py\n")?;
        fixture.write_file(".gitignore", "!ignored.py\n")?;
        let path = fixture.write_file("ignored.py", "")?;

        assert!(fixture.is_ignored(&path, false));
        Ok(())
    }

    #[test]
    fn ignore_file_allowlist_takes_precedence_over_gitignore_ignore() -> std::io::Result<()> {
        let fixture = Fixture::new()?;
        fixture.write_file(".git/HEAD", "ref: refs/heads/main\n")?;
        fixture.write_file(".ignore", "!included.py\n")?;
        fixture.write_file(".gitignore", "included.py\n")?;
        let path = fixture.write_file("included.py", "")?;

        assert!(!fixture.is_ignored(&path, false));
        Ok(())
    }

    #[test]
    fn ignore_file_strips_utf8_bom() -> std::io::Result<()> {
        let fixture = Fixture::new()?;
        fixture.write_file(".ignore", "\u{feff}ignored.py\n")?;
        let path = fixture.write_file("ignored.py", "")?;

        assert!(fixture.is_ignored(&path, false));
        Ok(())
    }

    #[test]
    fn gitignore_requires_repository() -> std::io::Result<()> {
        let fixture = Fixture::new()?;
        fixture.write_file(".gitignore", "ignored.py\n")?;
        let path = fixture.write_file("ignored.py", "")?;

        assert!(!fixture.is_ignored(&path, false));
        Ok(())
    }

    #[test]
    fn git_exclude_ignores_files() -> std::io::Result<()> {
        let fixture = Fixture::new()?;
        fixture.write_file(".git/HEAD", "ref: refs/heads/main\n")?;
        fixture.write_file(".git/info/exclude", "ignored.py\n")?;
        let path = fixture.write_file("ignored.py", "")?;

        assert!(fixture.is_ignored(&path, false));
        Ok(())
    }

    #[test]
    fn linked_worktree_git_exclude_ignores_files() -> std::io::Result<()> {
        let fixture = Fixture::new()?;
        let common_git_directory = fixture.path(".git-common");
        let worktree_git_directory = common_git_directory.join("worktrees/current");
        let commondir_path = worktree_git_directory.join("commondir");
        let path = fixture.write_file("ignored.py", "")?;

        fixture.write_file(".git", &format!("gitdir: {worktree_git_directory}\n"))?;
        fixture.write_file(".git-common/worktrees/current/commondir", "../..\n")?;
        fixture.write_file(".git-common/info/exclude", "ignored.py\n")?;

        assert!(fixture.is_ignored(&path, false));

        fixture
            .system
            .as_writable()
            .expect("OS test system to support writes")
            .write_file(&commondir_path, common_git_directory.as_str())?;

        assert!(fixture.is_ignored(&path, false));
        Ok(())
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
            IgnoreFiles::new(&system, std::slice::from_ref(&symlink_root))
                .is_ignored(&ignored_path, false)
        );

        Ok(())
    }
}
