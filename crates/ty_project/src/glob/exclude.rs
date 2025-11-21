//! Exclude filter supporting gitignore-like globs.
//!
//! * `src` excludes a file or directory named `src` anywhere in the path.
//! * `/src/` excludes a directory named `src` at the root of the path.
//! * `/src` excludes a directory or file named `src` at the root of the path.
//! * `/src/**` excludes all files and directories inside a directory named `src` but not `src` itself.
//! * `!src` allows a file or directory named `src` anywhere in the path

use std::fmt::Formatter;
use std::sync::Arc;

use globset::{Candidate, GlobBuilder, GlobSet, GlobSetBuilder};
use regex_automata::util::pool::Pool;
use ruff_db::system::SystemPath;

use crate::GlobFilterCheckMode;
use crate::glob::portable::AbsolutePortableGlobPattern;

/// A filter for gitignore-like globs that excludes files and directories.
///
/// # Equality
///
/// Two filters are equal if they're constructed from the same patterns (including order).
/// Two filters that exclude the exact same files but were constructed from different patterns aren't considered
/// equal.
#[derive(Clone, Debug, PartialEq, Eq, get_size2::GetSize)]
pub(crate) struct ExcludeFilter {
    ignore: Gitignore,
}

impl ExcludeFilter {
    /// Returns `true` if the path to a directory is definitely excluded and `false` otherwise.
    pub(crate) fn match_directory(&self, path: &SystemPath, mode: GlobFilterCheckMode) -> bool {
        self.matches(path, mode, true)
    }

    /// Returns `true` if the path to a file is definitely excluded and `false` otherwise.
    pub(crate) fn match_file(&self, path: &SystemPath, mode: GlobFilterCheckMode) -> bool {
        self.matches(path, mode, false)
    }

    fn matches(&self, path: &SystemPath, mode: GlobFilterCheckMode, directory: bool) -> bool {
        match mode {
            GlobFilterCheckMode::TopDown => {
                match self.ignore.matched(path, directory) {
                    // No hit or an allow hit means the file or directory is not excluded.
                    Match::None | Match::Allow => false,
                    Match::Ignore => true,
                }
            }
            GlobFilterCheckMode::Adhoc => {
                for ancestor in path.ancestors() {
                    match self.ignore.matched(ancestor, directory) {
                        // If the path is allowlisted or there's no hit, try the parent to ensure we don't return false
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

impl std::fmt::Display for ExcludeFilter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.ignore.globs).finish()
    }
}

pub(crate) struct ExcludeFilterBuilder {
    ignore: GitignoreBuilder,
}

impl ExcludeFilterBuilder {
    pub(crate) fn new() -> Self {
        Self {
            ignore: GitignoreBuilder::new(),
        }
    }

    pub(crate) fn add(
        &mut self,
        pattern: &AbsolutePortableGlobPattern,
    ) -> Result<&mut Self, globset::Error> {
        self.ignore.add(pattern)?;

        Ok(self)
    }

    pub(crate) fn build(self) -> Result<ExcludeFilter, globset::Error> {
        Ok(ExcludeFilter {
            ignore: self.ignore.build()?,
        })
    }
}

/// Matcher for gitignore like globs.
///
/// This code is our own vendored copy of the ignore's crate `Gitignore` type.
///
/// The differences with the ignore's crate version are:
///
/// * All globs are anchored. `src` matches `./src` only and not `**/src` to be consistent with `include`.
/// * It makes use of the fact that all our globs are absolute. This simplifies the implementation a fair bit.
///   Making globs absolute is also motivated by the fact that the globs can come from both the CLI and configuration files,
///   where the paths are anchored relative to the current working directory or the project root respectively.
/// * It uses [`globset::Error`] over the ignore's crate `Error` type.
/// * Removes supported for commented lines, because the patterns aren't read
///   from a `.gitignore` file. This removes the need to escape `#` for file names starting with `#`,
///
/// You can find the original source on [GitHub](https://github.com/BurntSushi/ripgrep/blob/cbc598f245f3c157a872b69102653e2e349b6d92/crates/ignore/src/gitignore.rs#L81).
///
/// # Equality
///
/// Two ignore matches are only equal if they're constructed from the same patterns (including order).
/// Two matchers that were constructed from different patterns but result in
/// including the same files don't compare equal.
#[derive(Clone, get_size2::GetSize)]
struct Gitignore {
    #[get_size(ignore)]
    set: GlobSet,
    globs: Vec<IgnoreGlob>,
    #[get_size(ignore)]
    matches: Option<Arc<Pool<Vec<usize>>>>,
}

impl Gitignore {
    /// Returns whether the given path (file or directory) matched a pattern in
    /// this gitignore matcher.
    ///
    /// `is_dir` should be true if the path refers to a directory and false
    /// otherwise.
    ///
    /// The path must be absolute or it will only match prefix-wildcard patterns.
    fn matched(&self, path: &SystemPath, is_dir: bool) -> Match {
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

impl std::fmt::Debug for Gitignore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Gitignore")
            .field(&self.globs)
            .finish_non_exhaustive()
    }
}

impl PartialEq for Gitignore {
    fn eq(&self, other: &Self) -> bool {
        self.globs == other.globs
    }
}

impl Eq for Gitignore {}

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

#[derive(Debug, Clone, PartialEq, Eq, get_size2::GetSize)]
struct IgnoreGlob {
    /// The pattern that was originally parsed.
    original: String,

    /// This is a pattern allowing a path (it starts with a `!`, possibly undoing a previous ignore)
    is_allow: bool,

    /// Whether this pattern only matches directories.
    is_only_dir: bool,
}

impl IgnoreGlob {
    const fn is_ignore(&self) -> bool {
        !self.is_allow
    }
}

/// Builds a matcher for git-ignore like globs.
///
/// All globs need to use absolute paths, unless they're unanchored (contain no `/`).
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
    fn build(&self) -> Result<Gitignore, globset::Error> {
        let set = self.builder.build()?;

        Ok(Gitignore {
            set,
            globs: self.globs.clone(),
            matches: Some(Arc::new(Pool::new(Vec::new))),
        })
    }

    /// Adds a gitignore like glob pattern to this builder.
    ///
    /// If the pattern could not be parsed as a glob, then an error is returned.
    fn add(
        &mut self,
        pattern: &AbsolutePortableGlobPattern,
    ) -> Result<&mut GitignoreBuilder, globset::Error> {
        let mut glob = IgnoreGlob {
            original: pattern.relative().to_string(),
            is_allow: false,
            is_only_dir: false,
        };

        let mut pattern = pattern.absolute();

        // File names starting with `!` are escaped with a backslash. Strip the backslash.
        // This is not a negated pattern!
        if pattern.starts_with("\\!") {
            pattern = &pattern[1..];
        } else if let Some(after) = pattern.strip_prefix("!") {
            glob.is_allow = true;
            pattern = after;
        }

        // If it ends with a slash, then this should only match directories,
        // but the slash should otherwise not be used while globbing.
        if let Some(before) = pattern.strip_suffix('/') {
            glob.is_only_dir = true;
            pattern = before;
        }

        let mut actual = pattern.to_string();

        // If the glob ends with `/**`, then we should only match everything
        // inside a directory, but not the directory itself. Standard globs
        // will match the directory. So we add `/*` to force the issue.
        if actual.ends_with("/**") {
            actual = format!("{actual}/*");
        }

        let parsed = GlobBuilder::new(&actual)
            .literal_separator(true)
            // No need to support Windows-style paths, so the backslash can be used an escape.
            .backslash_escape(true)
            .build()?;

        self.builder.add(parsed);
        self.globs.push(glob);

        Ok(self)
    }
}
