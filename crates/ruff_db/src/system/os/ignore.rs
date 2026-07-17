//! Checks whether paths are ignored during incremental project indexing.

use crate::system::SystemPath;
use crate::system::walk_directory::IgnoreIncremental;

pub(super) struct IgnoreFiles {
    pub(super) root_matchers: Vec<ignore::IncrementalIgnore>,
}

impl IgnoreIncremental for IgnoreFiles {
    fn is_ignored(&mut self, path: &SystemPath, is_directory: bool) -> bool {
        let Some(root) = self
            .root_matchers
            .iter_mut()
            .filter(|root| path.as_std_path().starts_with(root.root()))
            .max_by_key(|root| root.root().as_os_str().len())
        else {
            return false;
        };
        let Some(norm) = root.normalize(path.as_std_path()) else {
            return false;
        };
        root.matched(norm, is_directory).is_ignore()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

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

        fn is_ignored(&self, path: &SystemPath) -> bool {
            self.is_ignored_from(std::slice::from_ref(&self.root), path)
        }

        fn is_ignored_from(&self, walk_roots: &[SystemPathBuf], path: &SystemPath) -> bool {
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

        assert!(project.is_ignored(&path));
    }

    #[test]
    fn root_gitignore_file_requires_repository() {
        let project = TestProject::new();

        let path = project.path("build/keep.py");
        project.write_files([(project.path(".gitignore"), "build/\n")]);

        assert!(!project.is_ignored(&path));
    }

    #[test]
    fn bom() {
        let project = TestProject::new();
        let path = project.path("build/keep.py");
        project.write_files([(project.path(".ignore"), "\u{feff}build/\n")]);

        assert!(project.is_ignored(&path));
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

        assert!(!project.is_ignored(&path));
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

        assert!(!project.is_ignored(&path));
    }

    #[test]
    fn root_ignore_file_cannot_prune_deeper_file_match() {
        let project = TestProject::new();
        let path = project.path("pkg/keep.py");
        project.write_files([(project.path(".ignore"), "pkg/keep.py\n")]);

        assert!(project.is_ignored(&path));
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

        assert!(project.is_ignored(&path));
    }

    #[test]
    fn explicit_file_walk_root_cannot_be_pruned_by_parent_root() {
        let project = TestProject::new();
        let path = project.path("build/keep.py");
        project.write_files([(project.path(".ignore"), "build/\n")]);

        assert!(!project.is_ignored_from(&[project.root.clone(), path.clone()], &path));
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

        assert!(project.is_ignored_from(&[project.root.clone(), nested_root], &path));
    }
}
