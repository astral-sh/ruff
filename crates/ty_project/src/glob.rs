use ruff_db::system::SystemPath;

pub(crate) use exclude::{ExcludeFilter, ExcludeFilterBuilder};
pub(crate) use include::{IncludeFilter, IncludeFilterBuilder};
pub(crate) use portable::{
    AbsolutePortableGlobPattern, PortableGlobError, PortableGlobKind, PortableGlobPattern,
};

mod exclude;
mod include;
mod portable;

/// Path filtering based on an exclude and include glob pattern set.
///
/// Exclude patterns take precedence over includes.
#[derive(Clone, Debug, Eq, PartialEq, get_size2::GetSize)]
pub struct IncludeExcludeFilter {
    include: IncludeFilter,
    exclude: ExcludeFilter,
}

impl IncludeExcludeFilter {
    pub(crate) fn new(include: IncludeFilter, exclude: ExcludeFilter) -> Self {
        Self { include, exclude }
    }

    /// Returns whether this directory is included in this filter.
    ///
    /// Note, this function never returns [`IncludeResult::Included`] for a path that is not included or excluded.
    /// However, it may return [`IncludeResult::Included`] for directories that are not excluded, but where
    /// it requires traversal to decide if any of its subdirectories or files are included. This, for example,
    /// is the case when using wildcard include-patterns like `**/test`. Prefix wildcards require to traverse `src`
    /// because it can't be known ahead of time whether it contains a `test` directory or file.
    pub(crate) fn is_directory_maybe_included(
        &self,
        path: &SystemPath,
        mode: GlobFilterCheckMode,
    ) -> IncludeResult {
        if self.exclude.match_directory(path, mode) {
            IncludeResult::Excluded
        } else if self.include.match_directory(path) {
            IncludeResult::Included
        } else {
            IncludeResult::NotIncluded
        }
    }

    pub(crate) fn is_file_included(
        &self,
        path: &SystemPath,
        mode: GlobFilterCheckMode,
    ) -> IncludeResult {
        if self.exclude.match_file(path, mode) {
            IncludeResult::Excluded
        } else if self.include.match_file(path) {
            IncludeResult::Included
        } else {
            IncludeResult::NotIncluded
        }
    }
}

impl std::fmt::Display for IncludeExcludeFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "include={}, exclude={}", &self.include, &self.exclude)
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum GlobFilterCheckMode {
    /// The paths are checked top-to-bottom and inclusion is determined
    /// for each path during the traversal.
    TopDown,

    /// An adhoc test if a single file or directory is included.
    ///
    /// This is more expensive than a [`Self::TopDown`] check
    /// because it may require testing every ancestor path in addition to the
    /// path itself to ensure no ancestor path matches an exclude rule.
    Adhoc,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum IncludeResult {
    /// The path matches or at least is a prefix of an include pattern.
    ///
    /// For directories: This isn't a guarantee that any file in this directory gets included
    /// but we need to traverse it to make this decision.
    Included,

    /// The path matches an exclude pattern.
    Excluded,

    /// The path matches neither an include nor an exclude pattern and, therefore,
    /// isn't included.
    NotIncluded,
}
