use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ruff_db::system::SystemPath;

use crate::{
    GlobFilterCheckMode,
    glob::portable::{self, InvalidChar, PortableGlobError},
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
                match self.ignore.matched_path_or_any_parents(path, directory) {
                    // No hit or an allow hit means the file or directory is not excluded.
                    ignore::Match::None | ignore::Match::Whitelist(_) => false,
                    ignore::Match::Ignore(_) => true,
                }
            }
            GlobFilterCheckMode::Adhoc => {
                for ancestor in path.ancestors() {
                    match self.ignore.matched_path_or_any_parents(ancestor, directory) {
                        // If it's allowlisted or there's no hit, try the parent to ensure we don't return false
                        // for a folder where there's an exclude for a parent.
                        ignore::Match::None | ignore::Match::Whitelist(_) => {}
                        ignore::Match::Ignore(_) => return true,
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
            ignore: GitignoreBuilder::new(""),
            patterns: Vec::new(),
        }
    }

    pub(crate) fn add(&mut self, pattern: &str) -> Result<&mut Self, ExcludePatternError> {
        // Disallow comment lines.
        if pattern.starts_with('#') {
            return Err(ExcludePatternError::Portable(
                PortableGlobError::InvalidCharacter {
                    glob: pattern.to_string(),
                    pos: 0,
                    invalid: InvalidChar('#'),
                },
            ));
        }

        // Strip the `!` before passing it to `portable_glob::check` because
        // the negation isn't part of the glob itself.
        let pattern_part = pattern.strip_prefix('!').unwrap_or(pattern);
        portable::check(pattern_part)?;

        self.ignore
            .add_line(None, pattern)
            .map_err(|err| match err {
                ignore::Error::Glob { glob, err } => ExcludePatternError::IgnoreGlobError {
                    glob: glob.unwrap_or_else(|| pattern.to_string()),
                    error: err,
                },
                _ => {
                    panic!("Unexpected error during git ignore construction: {err}")
                }
            })?;

        self.patterns.push(pattern.to_string());

        Ok(self)
    }

    pub(crate) fn build(self) -> Result<ExcludeFilter, ignore::Error> {
        Ok(ExcludeFilter {
            ignore: self.ignore.build()?,
            original_patterns: self.patterns.into(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ExcludePatternError {
    #[error(transparent)]
    Portable(#[from] portable::PortableGlobError),

    #[error("invalid glob '{glob}': {error}")]
    IgnoreGlobError { glob: String, error: String },
}
