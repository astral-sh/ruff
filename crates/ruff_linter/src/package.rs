use std::path::Path;

/// The root directory of a Python package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageRoot<'a> {
    /// A normal package root.
    Root { path: &'a Path },
    /// A nested package root. That is, a package root that's a subdirectory (direct or indirect) of
    /// another Python package root.
    ///
    /// For example, `foo/bar/baz` in:
    /// ```text
    /// foo/
    /// ├── __init__.py
    /// └── bar/
    ///     └── baz/
    ///         └── __init__.py
    /// ```
    Nested { path: &'a Path },
}

impl<'a> PackageRoot<'a> {
    /// Create a [`PackageRoot::Root`] variant.
    pub fn root(path: &'a Path) -> Self {
        Self::Root { path }
    }

    /// Create a [`PackageRoot::Nested`] variant.
    pub fn nested(path: &'a Path) -> Self {
        Self::Nested { path }
    }

    /// Return the [`Path`] of the package root.
    pub fn path(self) -> &'a Path {
        match self {
            Self::Root { path } => path,
            Self::Nested { path } => path,
        }
    }
}
