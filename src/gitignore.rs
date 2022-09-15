/// Like gitignore::File, but with a public `is_excluded` and support for comments in .gitignore.
use std::fs;
use std::path::{Path, PathBuf};

use gitignore::Pattern;

use crate::fs::read_file;
use anyhow::Result;

#[derive(Debug)]
pub struct File<'a> {
    patterns: Vec<Pattern<'a>>,
    root: &'a Path,
}

impl<'b> File<'b> {
    /// Parse the given `.gitignore` file for patterns, allowing any arbitrary path to be checked
    /// against the set of rules to test for exclusion.
    ///
    /// The value of `gitignore_path` must be an absolute path.
    pub fn new(gitignore_path: &'b Path) -> Result<File<'b>> {
        let root = gitignore_path.parent().unwrap();
        let patterns = File::patterns(gitignore_path, root)?;

        Ok(File { patterns, root })
    }

    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Returns true if, after checking against all the patterns found in the `.gitignore` file,
    /// the given path is matched any of the globs (applying negated patterns as expected).
    ///
    /// If the value for `path` is not absolute, it will assumed to be relative to the current
    /// working directory.
    ///
    /// Note very importantly that this method _does not_ check if the parent directories are
    /// excluded. This is only for determining if the file itself matched any rules.
    pub fn is_excluded(&self, path: &'b Path) -> Result<bool> {
        let abs_path = self.abs_path(path);
        let directory = fs::metadata(&abs_path)?.is_dir();
        for pattern in self.patterns.iter() {
            let matches = pattern.is_excluded(&abs_path, directory);
            if matches {
                return Ok(!pattern.negation);
            }
        }

        Ok(false)
    }

    /// Given the path to the `.gitignore` file and the root folder within which it resides,
    /// parse out all the patterns and collect them up into a vector of patterns.
    fn patterns(path: &'b Path, root: &'b Path) -> Result<Vec<Pattern<'b>>> {
        let contents = read_file(path)?;
        Ok(contents
            .lines()
            .filter_map(|line| {
                if !line.trim().is_empty() && !line.starts_with('#') {
                    Pattern::new(line, root).ok()
                } else {
                    None
                }
            })
            .collect())
    }

    /// Given a path, make it absolute if relative by joining it to a given root, otherwise leave
    /// absolute as originally given.
    fn abs_path(&self, path: &'b Path) -> PathBuf {
        if path.is_absolute() {
            path.to_owned()
        } else {
            self.root.join(path)
        }
    }
}
