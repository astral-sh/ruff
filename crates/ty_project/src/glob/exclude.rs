use std::sync::Arc;

use globset::{Candidate, GlobSet, GlobSetBuilder};
use regex_automata::util::pool::Pool;
use ruff_db::system::SystemPath;

use crate::{
    GlobFilterCheckMode,
    glob::portable::{self, PortableGlobError},
};

///
/// # Equality
///
/// Two filters are only equal if they're constructed from the same patterns (including order).
/// Therefore, two filters that exclude the exact same file might compare unequal.
#[derive(Clone)]
pub(crate) struct ExcludeFilter {
    ignore: Gitignore,
    original_patterns: Box<[String]>,
}

impl ExcludeFilter {
    pub(crate) fn match_directory(&self, path: &SystemPath, mode: GlobFilterCheckMode) -> bool {
        self.matches(path, mode, true)
    }

    pub(crate) fn match_file(&self, path: &SystemPath, mode: GlobFilterCheckMode) -> bool {
        self.matches(path, mode, false)
    }

    fn matches(&self, path: &SystemPath, mode: GlobFilterCheckMode, directory: bool) -> bool {
        match mode {
            GlobFilterCheckMode::TopDown => {
                match self.ignore.matched(path, directory) {
                    // No hit or a negated exclude hit means the file or directory is not excluded.
                    Match::None | Match::Allow => false,
                    Match::Ignore => true,
                }
            }
            GlobFilterCheckMode::Adhoc => {
                for ancestor in path.ancestors() {
                    match self.ignore.matched(ancestor, directory) {
                        // If it's allowlisted or there's no hit, try the parent to ensure we don't return false
                        // for a folder where there's an exclude for a parent.
                        Match::None | Match::Allow => {}
                        Match::Ignore => return true,
                    }
                }

                false
            }
        }
    }
}

impl std::fmt::Debug for ExcludeFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ExcludeFilter")
            .field(&self.original_patterns)
            .finish_non_exhaustive()
    }
}

impl PartialEq for ExcludeFilter {
    fn eq(&self, other: &Self) -> bool {
        self.original_patterns == other.original_patterns
    }
}

impl Eq for ExcludeFilter {}

pub(crate) struct ExcludeFilterBuilder {
    ignore: GitignoreBuilder,
    patterns: Vec<String>,
}

impl ExcludeFilterBuilder {
    pub(crate) fn new() -> Self {
        Self {
            ignore: GitignoreBuilder::new(),
            patterns: Vec::new(),
        }
    }

    pub(crate) fn add(&mut self, pattern: &str) -> Result<&mut Self, PortableGlobError> {
        self.ignore.add(pattern)?;
        self.patterns.push(pattern.to_string());

        Ok(self)
    }

    pub(crate) fn build(self) -> Result<ExcludeFilter, PortableGlobError> {
        Ok(ExcludeFilter {
            ignore: self.ignore.build()?,
            original_patterns: self.patterns.into(),
        })
    }
}

/// Gitignore is a matcher for the globs in one or more gitignore files
/// in the same directory.
///
/// The code here is mainly copied from the `ignore` crate. The main difference
/// is that it doesn't have a `root` path. Instead, it assumes that all paths (and patterns) are absolute.
#[derive(Clone, Debug)]
struct Gitignore {
    set: GlobSet,
    globs: Vec<IgnoreGlob>,
    matches: Option<Arc<Pool<Vec<usize>>>>,
}

impl Gitignore {
    /// Returns whether the given path (file or directory) matched a pattern in
    /// this gitignore matcher.
    ///
    /// `is_dir` should be true if the path refers to a directory and false
    /// otherwise.
    ///
    /// The given path is matched relative to the path given when building
    /// the matcher. Specifically, before matching `path`, its prefix (as
    /// determined by a common suffix of the directory containing this
    /// gitignore) is stripped. If there is no common suffix/prefix overlap,
    /// then `path` is assumed to be relative to this matcher.
    fn matched(&self, path: &SystemPath, is_dir: bool) -> Match {
        debug_assert!(path.is_absolute(), "Expected path `{path}` to be absolute");
        if self.globs.is_empty() {
            return Match::None;
        }

        let mut matches = self.matches.as_ref().unwrap().get();
        let candidate = Candidate::new(path);
        self.set.matches_candidate_into(&candidate, &mut matches);
        for &i in matches.iter().rev() {
            let glob = &self.globs[i];
            if !glob.is_only_dir || is_dir {
                return if glob.is_ignore() {
                    Match::Ignore
                } else {
                    Match::Allow
                };
            }
        }
        Match::None
    }
}

#[derive(Copy, Clone, Debug)]
enum Match {
    /// The path matches no pattern.
    None,

    /// The path matches an ignore pattern (a positive pattern)
    /// It should be ignored.
    Ignore,

    /// The path matches an allow pattern (a negative pattern).
    /// It should not be ignored.
    Allow,
}

#[derive(Debug, Clone)]
struct IgnoreGlob {
    // Useful for debuging.
    #[expect(dead_code)]
    original: String,

    /// This is a pattern allowing a path (it starts with a `!`, possibily undoing a previous ignore)
    is_allow: bool,

    /// Whether this pattern only matches directories.
    is_only_dir: bool,
}

impl IgnoreGlob {
    const fn is_ignore(&self) -> bool {
        !self.is_allow
    }
}

/// Builds a matcher for a single set of globs from a .gitignore file.
#[derive(Clone, Debug)]
struct GitignoreBuilder {
    builder: GlobSetBuilder,
    globs: Vec<IgnoreGlob>,
}

impl GitignoreBuilder {
    /// Create a new builder for a gitignore file.
    fn new() -> GitignoreBuilder {
        GitignoreBuilder {
            builder: GlobSetBuilder::new(),
            globs: vec![],
        }
    }

    /// Builds a new matcher from the globs added so far.
    ///
    /// Once a matcher is built, no new globs can be added to it.
    fn build(&self) -> Result<Gitignore, PortableGlobError> {
        let set = self.builder.build()?;

        Ok(Gitignore {
            set,
            globs: self.globs.clone(),
            matches: Some(Arc::new(Pool::new(std::vec::Vec::new))),
        })
    }

    /// Add a line from a gitignore file to this builder.
    ///
    /// If this line came from a particular `gitignore` file, then its path
    /// should be provided here.
    ///
    /// If the line could not be parsed as a glob, then an error is returned.
    fn add(&mut self, mut pattern: &str) -> Result<&mut GitignoreBuilder, PortableGlobError> {
        let mut glob = IgnoreGlob {
            original: pattern.to_string(),
            is_allow: false,
            is_only_dir: false,
        };

        // File names starting with `!` need to be escaped. Strip the escape character.
        if pattern.starts_with("\\!") {
            pattern = &pattern[1..];
        } else {
            if let Some(after) = pattern.strip_prefix("!") {
                glob.is_allow = true;
                pattern = after;
            }
        }
        // If it ends with a slash, then this should only match directories,
        // but the slash should otherwise not be used while globbing.
        if let Some(before) = pattern.strip_suffix('/') {
            glob.is_only_dir = true;
            pattern = before;
        }

        let mut actual = pattern.to_string();

        // If there is a literal slash, then this is a glob that must match the
        // entire path name. Otherwise, we should let it match anywhere, so use
        // a **/ prefix.
        if !pattern.chars().any(|c| c == '/') {
            // ... but only if we don't already have a **/ prefix.
            if !pattern.starts_with("**/") {
                actual = format!("**/{actual}");
            }
        }
        // If the glob ends with `/**`, then we should only match everything
        // inside a directory, but not the directory itself. Standard globs
        // will match the directory. So we add `/*` to force the issue.
        if actual.ends_with("/**") {
            actual = format!("{actual}/*");
        }

        let parsed = portable::parse(&actual)?;

        self.builder.add(parsed);
        self.globs.push(glob);

        Ok(self)
    }
}
