//! Checks whether a root ignore file lets incremental indexing skip a project
//! walk.
//!
//! A full project walk decides which files belong to a project. Incremental
//! file watcher updates must make the same decision for newly created paths,
//! including the effect of `.ignore` and `.gitignore` files. Ideally, the
//! `ignore` crate would expose an API that answers whether a given path is
//! ignored. It does not, and reimplementing that decision is involved because
//! it would need to reproduce all of the walker's ignore behavior:
//!
//! - parent `.ignore` files;
//! - parent `.gitignore` files;
//! - `.git/info/exclude`;
//! - the global gitignore;
//! - git and jj repository discovery;
//! - traversal ordering, such as pruning `build/` before reading a nested
//!   `build/.ignore`; and
//! - linked git worktree handling.
//!
//! This module is a compromise. It avoids unnecessary traversal in the common
//! case where a file or directory is already ignored by an ignore file at the
//! project walk root.

use ::ignore::gitignore;
use rustc_hash::FxHashMap;

use crate::system::walk_directory::{IgnoreIncremental, Ignored};
use crate::system::{SystemPath, SystemPathBuf};

pub(super) struct IgnoreFiles {
    walk_roots: Box<[SystemPathBuf]>,
    roots: FxHashMap<SystemPathBuf, RootIgnoreFiles>,
}

impl IgnoreFiles {
    pub(super) fn new(walk_roots: Vec<SystemPathBuf>) -> Self {
        Self {
            walk_roots: walk_roots.into_boxed_slice(),
            roots: FxHashMap::default(),
        }
    }

    /// Answers the question whether the ignore file in the `root` directory
    /// ignores `path`.
    fn root_ignore_prunes_path(
        roots: &mut FxHashMap<SystemPathBuf, RootIgnoreFiles>,
        root: &SystemPath,
        path: &SystemPath,
        is_directory: bool,
    ) -> bool {
        let Ok(relative_path) = path.strip_prefix(root) else {
            return false;
        };
        let mut components = relative_path.components();
        let Some(first_component) = components.next() else {
            return false;
        };

        let first_child = root.join(first_component);
        let first_child_is_target = components.next().is_none();

        let first_child_is_directory = !first_child_is_target || is_directory;

        Self::root_ignore_files(roots, root).is_ignored(&first_child, first_child_is_directory)
    }

    fn root_ignore_files<'a>(
        roots: &'a mut FxHashMap<SystemPathBuf, RootIgnoreFiles>,
        root: &SystemPath,
    ) -> &'a RootIgnoreFiles {
        roots
            .entry(root.to_path_buf())
            .or_insert_with(|| RootIgnoreFiles::read(root))
    }
}

impl IgnoreIncremental for IgnoreFiles {
    /// Returns `Yes` only when the matching walk root can prune `path`
    /// from its own ignore files. In all other cases, return uncertain.
    fn is_ignored(&mut self, path: &SystemPath, is_directory: bool) -> Ignored {
        let Self { walk_roots, roots } = self;

        // A nested explicit walk root gets its own depth-0 walk, so an ancestor
        // root cannot prove that the nested root's descendants are ignored.
        let Some(root) = walk_roots
            .iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.as_str().len())
        else {
            return Ignored::Uncertain;
        };

        if Self::root_ignore_prunes_path(roots, root, path, is_directory) {
            Ignored::Yes
        } else {
            Ignored::Uncertain
        }
    }
}

/// The cached ignore files for a specific root-folder.
struct RootIgnoreFiles {
    ignore: Option<IgnoreFile>,
    gitignore: Option<IgnoreFile>,
}

impl RootIgnoreFiles {
    fn read(root: &SystemPath) -> Self {
        let canonical_root = root.as_utf8_path().canonicalize_utf8().ok().map(|path| {
            SystemPathBuf::from_utf8_path_buf(path)
                .simplified()
                .to_path_buf()
        });

        let gitignore = if let Some(canonical_root) = canonical_root.as_deref()
            && in_git_repository(canonical_root)
            // A parent `.ignore` allowlist takes precedence over a matching
            // `.gitignore` at the walk root. Let the walker resolve that case.
            && !has_parent_ignore_file(canonical_root)
        {
            IgnoreFile::read(root, ".gitignore")
        } else {
            None
        };

        Self {
            ignore: IgnoreFile::read(root, ".ignore"),
            gitignore,
        }
    }

    fn is_ignored(&self, path: &SystemPath, is_directory: bool) -> bool {
        for ignore_file in [&self.ignore, &self.gitignore].into_iter().flatten() {
            match ignore_file.match_path(path, is_directory) {
                Ok(Some(is_ignored)) => return is_ignored,
                Ok(_) => {}
                Err(()) => return false,
            }
        }

        false
    }
}

enum IgnoreFile {
    Matcher(gitignore::Gitignore),
    /// Building the matcher failed.
    Error,
}

impl IgnoreFile {
    fn read(root: &SystemPath, file_name: &str) -> Option<Self> {
        let ignore_path = root.join(file_name);
        let contents = match std::fs::read_to_string(ignore_path.as_std_path()) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(_) => return Some(Self::Error),
        };

        match build_matcher(root, &ignore_path, &contents) {
            Some(matcher) => Some(Self::Matcher(matcher)),
            None => Some(Self::Error),
        }
    }

    fn match_path(&self, path: &SystemPath, is_directory: bool) -> Result<Option<bool>, ()> {
        let matcher = match self {
            Self::Matcher(matcher) => matcher,
            Self::Error => return Err(()),
        };

        Ok(match matcher.matched(path.as_std_path(), is_directory) {
            ::ignore::Match::None => None,
            ::ignore::Match::Ignore(_) => Some(true),
            ::ignore::Match::Whitelist(_) => Some(false),
        })
    }
}

fn build_matcher(
    root: &SystemPath,
    ignore_path: &SystemPath,
    contents: &str,
) -> Option<gitignore::Gitignore> {
    const UTF8_BOM: &str = "\u{feff}";

    let mut builder = gitignore::GitignoreBuilder::new(root.as_std_path());

    let contents = contents.trim_start_matches(UTF8_BOM);

    for line in contents.lines() {
        builder
            .add_line(Some(ignore_path.as_std_path().to_path_buf()), line)
            .ok()?;
    }

    builder.build().ok()
}

fn in_git_repository(canonical_root: &SystemPath) -> bool {
    canonical_root.ancestors().any(|directory| {
        directory.join(".git").as_std_path().exists()
            || directory.join(".jj").as_std_path().exists()
    })
}

fn has_parent_ignore_file(canonical_root: &SystemPath) -> bool {
    canonical_root
        .parent()
        .into_iter()
        .flat_map(SystemPath::ancestors)
        .any(|directory| directory.join(".ignore").as_std_path().exists())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::system::walk_directory::Ignored;
    use crate::system::{OsSystem, System, SystemPath, SystemPathBuf};

    struct TestProject {
        _temp_dir: TempDir,
        system: OsSystem,
        root: SystemPathBuf,
    }

    impl TestProject {
        fn new() -> Self {
            Self::with_root("project")
        }

        fn with_root(root: &str) -> Self {
            let temp_dir = TempDir::new().unwrap();
            let temp_dir_path = SystemPath::from_std_path(temp_dir.path()).unwrap();
            let root = temp_dir_path.join(root);
            std::fs::create_dir_all(root.as_std_path()).unwrap();
            let system = OsSystem::new(&root);

            Self {
                _temp_dir: temp_dir,
                system,
                root,
            }
        }

        fn path(&self, relative_path: &str) -> SystemPathBuf {
            self.root.join(relative_path)
        }

        fn write_files<'a>(&self, files: impl IntoIterator<Item = (SystemPathBuf, &'a str)>) {
            for (path, contents) in files {
                std::fs::create_dir_all(path.parent().unwrap().as_std_path()).unwrap();
                std::fs::write(path.as_std_path(), contents).unwrap();
            }
        }

        fn create_directory(&self, path: impl AsRef<SystemPath>) {
            std::fs::create_dir_all(path.as_ref().as_std_path()).unwrap();
        }

        fn is_ignored(&self, path: &SystemPath) -> Ignored {
            self.is_ignored_from(std::slice::from_ref(&self.root), path)
        }

        fn is_ignored_from(&self, walk_roots: &[SystemPathBuf], path: &SystemPath) -> Ignored {
            let (first, additional) = walk_roots.split_first().unwrap();
            let mut builder = self.system.walk_directory(first);

            for root in additional {
                builder = builder.add(root);
            }

            builder.incremental_matcher().is_ignored(path, false)
        }
    }

    #[test]
    fn root_ignore_file_prunes_top_level_directory() {
        let project = TestProject::new();
        let path = project.path("build/keep.py");
        project.write_files([
            (project.path(".ignore"), "build/\n"),
            (project.path("build/.ignore"), "!keep.py\n"),
        ]);

        assert_eq!(project.is_ignored(&path), Ignored::Yes);
    }

    #[test]
    fn root_gitignore_file_requires_repository() {
        let project = TestProject::new();

        let path = project.path("build/keep.py");
        project.write_files([(project.path(".gitignore"), "build/\n")]);

        assert_eq!(project.is_ignored(&path), Ignored::Uncertain);
    }

    #[test]
    fn bom() {
        let project = TestProject::new();
        let path = project.path("build/keep.py");
        project.write_files([(project.path(".ignore"), "\u{feff}build/\n")]);

        assert_eq!(project.is_ignored(&path), Ignored::Yes);
    }

    #[test]
    fn root_ignore_file_allowlist_overrides_root_gitignore_file() {
        let project = TestProject::new();
        let path = project.path("build/keep.py");
        project.write_files([
            (project.path(".git/HEAD"), "ref: refs/heads/main\n"),
            (project.path(".ignore"), "!build/\n"),
            (project.path(".gitignore"), "build/\n"),
        ]);

        assert_eq!(project.is_ignored(&path), Ignored::Uncertain);
    }

    #[test]
    fn parent_ignore_file_disables_root_gitignore_pruning() {
        let project = TestProject::with_root("workspace/project");
        let path = project.path("build/keep.py");
        project.write_files([
            (project.path(".git/HEAD"), "ref: refs/heads/main\n"),
            (project.path(".gitignore"), "build/\n"),
            (
                project.root.parent().unwrap().join(".ignore"),
                "!project/build/\n",
            ),
        ]);

        assert_eq!(project.is_ignored(&path), Ignored::Uncertain);
    }

    #[test]
    fn root_ignore_file_cannot_prune_deeper_file_match() {
        let project = TestProject::new();
        let path = project.path("pkg/keep.py");
        project.write_files([(project.path(".ignore"), "pkg/keep.py\n")]);

        assert_eq!(project.is_ignored(&path), Ignored::Uncertain);
    }

    #[test]
    fn unreadable_root_ignore_file_cannot_prune_path() {
        let project = TestProject::new();
        let path = project.path("build/ignored.py");
        project.create_directory(project.path(".ignore"));
        project.write_files([
            (project.path(".git/HEAD"), "ref: refs/heads/main\n"),
            (project.path(".gitignore"), "build/\n"),
        ]);

        assert_eq!(project.is_ignored(&path), Ignored::Uncertain);
    }

    #[test]
    fn explicit_file_walk_root_cannot_be_pruned_by_parent_root() {
        let project = TestProject::new();
        let path = project.path("build/keep.py");
        project.write_files([(project.path(".ignore"), "build/\n")]);

        assert_eq!(
            project.is_ignored_from(&[project.root.clone(), path.clone()], &path),
            Ignored::Uncertain
        );
    }

    #[test]
    fn nested_directory_walk_root_uses_its_own_ignore_file() {
        let project = TestProject::new();
        let path = project.path("pkg/build/ignored.py");
        let nested_root = project.path("pkg");
        project.write_files([
            (project.path(".ignore"), "pkg/\n"),
            (project.path("pkg/.ignore"), "build/\n"),
        ]);

        assert_eq!(
            project.is_ignored_from(&[project.root.clone(), nested_root], &path),
            Ignored::Yes
        );
    }
}
