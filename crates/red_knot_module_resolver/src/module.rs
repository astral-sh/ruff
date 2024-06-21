use std::fmt::Formatter;
use std::ops::Deref;
use std::sync::Arc;

use ruff_db::file_system::FileSystemPath;
use ruff_db::vfs::{VfsFile, VfsPath};
use ruff_python_stdlib::identifiers::is_identifier;

use crate::Db;

/// A module name, e.g. `foo.bar`.
///
/// Always normalized to the absolute form (never a relative module name, i.e., never `.foo`).
#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ModuleName(smol_str::SmolStr);

impl ModuleName {
    /// Creates a new module name for `name`. Returns `Some` if `name` is a valid, absolute
    /// module name and `None` otherwise.
    ///
    /// The module name is invalid if:
    ///
    /// * The name is empty
    /// * The name is relative
    /// * The name ends with a `.`
    /// * The name contains a sequence of multiple dots
    /// * A component of a name (the part between two dots) isn't a valid python identifier.
    #[inline]
    pub fn new(name: &str) -> Option<Self> {
        Self::new_from_smol(smol_str::SmolStr::new(name))
    }

    /// Creates a new module name for `name` where `name` is a static string.
    /// Returns `Some` if `name` is a valid, absolute module name and `None` otherwise.
    ///
    /// The module name is invalid if:
    ///
    /// * The name is empty
    /// * The name is relative
    /// * The name ends with a `.`
    /// * The name contains a sequence of multiple dots
    /// * A component of a name (the part between two dots) isn't a valid python identifier.
    ///
    /// ## Examples
    ///
    /// ```
    /// use red_knot_module_resolver::ModuleName;
    ///
    /// assert_eq!(ModuleName::new_static("foo.bar").as_deref(), Some("foo.bar"));
    /// assert_eq!(ModuleName::new_static(""), None);
    /// assert_eq!(ModuleName::new_static("..foo"), None);
    /// assert_eq!(ModuleName::new_static(".foo"), None);
    /// assert_eq!(ModuleName::new_static("foo."), None);
    /// assert_eq!(ModuleName::new_static("foo..bar"), None);
    /// assert_eq!(ModuleName::new_static("2000"), None);
    /// ```
    #[inline]
    pub fn new_static(name: &'static str) -> Option<Self> {
        Self::new_from_smol(smol_str::SmolStr::new_static(name))
    }

    fn new_from_smol(name: smol_str::SmolStr) -> Option<Self> {
        if name.is_empty() {
            return None;
        }

        if name.split('.').all(is_identifier) {
            Some(Self(name))
        } else {
            None
        }
    }

    /// An iterator over the components of the module name:
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_module_resolver::ModuleName;
    ///
    /// assert_eq!(ModuleName::new_static("foo.bar.baz").unwrap().components().collect::<Vec<_>>(), vec!["foo", "bar", "baz"]);
    /// ```
    pub fn components(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.0.split('.')
    }

    /// The name of this module's immediate parent, if it has a parent.
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_module_resolver::ModuleName;
    ///
    /// assert_eq!(ModuleName::new_static("foo.bar").unwrap().parent(), Some(ModuleName::new_static("foo").unwrap()));
    /// assert_eq!(ModuleName::new_static("foo.bar.baz").unwrap().parent(), Some(ModuleName::new_static("foo.bar").unwrap()));
    /// assert_eq!(ModuleName::new_static("root").unwrap().parent(), None);
    /// ```
    pub fn parent(&self) -> Option<ModuleName> {
        let (parent, _) = self.0.rsplit_once('.')?;

        Some(Self(smol_str::SmolStr::new(parent)))
    }

    /// Returns `true` if the name starts with `other`.
    ///
    /// This is equivalent to checking if `self` is a sub-module of `other`.
    ///
    /// # Examples
    ///
    /// ```
    /// use red_knot_module_resolver::ModuleName;
    ///
    /// assert!(ModuleName::new_static("foo.bar").unwrap().starts_with(&ModuleName::new_static("foo").unwrap()));
    ///
    /// assert!(!ModuleName::new_static("foo.bar").unwrap().starts_with(&ModuleName::new_static("bar").unwrap()));
    /// assert!(!ModuleName::new_static("foo_bar").unwrap().starts_with(&ModuleName::new_static("foo").unwrap()));
    /// ```
    pub fn starts_with(&self, other: &ModuleName) -> bool {
        let mut self_components = self.components();
        let other_components = other.components();

        for other_component in other_components {
            if self_components.next() != Some(other_component) {
                return false;
            }
        }

        true
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn from_relative_path(path: &FileSystemPath) -> Option<Self> {
        let path = if path.ends_with("__init__.py") || path.ends_with("__init__.pyi") {
            path.parent()?
        } else {
            path
        };

        let name = if let Some(parent) = path.parent() {
            let mut name = String::with_capacity(path.as_str().len());

            for component in parent.components() {
                name.push_str(component.as_os_str().to_str()?);
                name.push('.');
            }

            // SAFETY: Unwrap is safe here or `parent` would have returned `None`.
            name.push_str(path.file_stem().unwrap());

            smol_str::SmolStr::from(name)
        } else {
            smol_str::SmolStr::new(path.file_stem()?)
        };

        Some(Self(name))
    }
}

impl Deref for ModuleName {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl PartialEq<str> for ModuleName {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<ModuleName> for str {
    fn eq(&self, other: &ModuleName) -> bool {
        self == other.as_str()
    }
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Representation of a Python module.
#[derive(Clone, PartialEq, Eq)]
pub struct Module {
    inner: Arc<ModuleInner>,
}

impl Module {
    pub(crate) fn new(
        name: ModuleName,
        kind: ModuleKind,
        search_path: ModuleSearchPath,
        file: VfsFile,
    ) -> Self {
        Self {
            inner: Arc::new(ModuleInner {
                name,
                kind,
                search_path,
                file,
            }),
        }
    }

    /// The absolute name of the module (e.g. `foo.bar`)
    pub fn name(&self) -> &ModuleName {
        &self.inner.name
    }

    /// The file to the source code that defines this module
    pub fn file(&self) -> VfsFile {
        self.inner.file
    }

    /// The search path from which the module was resolved.
    pub fn search_path(&self) -> &ModuleSearchPath {
        &self.inner.search_path
    }

    /// Determine whether this module is a single-file module or a package
    pub fn kind(&self) -> ModuleKind {
        self.inner.kind
    }
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("name", &self.name())
            .field("kind", &self.kind())
            .field("file", &self.file())
            .field("search_path", &self.search_path())
            .finish()
    }
}

impl salsa::DebugWithDb<dyn Db> for Module {
    fn fmt(&self, f: &mut Formatter<'_>, db: &dyn Db) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("name", &self.name())
            .field("kind", &self.kind())
            .field("file", &self.file().debug(db.upcast()))
            .field("search_path", &self.search_path())
            .finish()
    }
}

#[derive(PartialEq, Eq)]
struct ModuleInner {
    name: ModuleName,
    kind: ModuleKind,
    search_path: ModuleSearchPath,
    file: VfsFile,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ModuleKind {
    /// A single-file module (e.g. `foo.py` or `foo.pyi`)
    Module,

    /// A python package (`foo/__init__.py` or `foo/__init__.pyi`)
    Package,
}

/// A search path in which to search modules.
/// Corresponds to a path in [`sys.path`](https://docs.python.org/3/library/sys_path_init.html) at runtime.
///
/// Cloning a search path is cheap because it's an `Arc`.
#[derive(Clone, PartialEq, Eq)]
pub struct ModuleSearchPath {
    inner: Arc<ModuleSearchPathInner>,
}

impl ModuleSearchPath {
    pub fn new<P>(path: P, kind: ModuleSearchPathKind) -> Self
    where
        P: Into<VfsPath>,
    {
        Self {
            inner: Arc::new(ModuleSearchPathInner {
                path: path.into(),
                kind,
            }),
        }
    }

    /// Determine whether this is a first-party, third-party or standard-library search path
    pub fn kind(&self) -> ModuleSearchPathKind {
        self.inner.kind
    }

    /// Return the location of the search path on the file system
    pub fn path(&self) -> &VfsPath {
        &self.inner.path
    }
}

impl std::fmt::Debug for ModuleSearchPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleSearchPath")
            .field("path", &self.inner.path)
            .field("kind", &self.kind())
            .finish()
    }
}

#[derive(Eq, PartialEq)]
struct ModuleSearchPathInner {
    path: VfsPath,
    kind: ModuleSearchPathKind,
}

/// Enumeration of the different kinds of search paths type checkers are expected to support.
///
/// N.B. Although we don't implement `Ord` for this enum, they are ordered in terms of the
/// priority that we want to give these modules when resolving them.
/// This is roughly [the order given in the typing spec], but typeshed's stubs
/// for the standard library are moved higher up to match Python's semantics at runtime.
///
/// [the order given in the typing spec]: https://typing.readthedocs.io/en/latest/spec/distributing.html#import-resolution-ordering
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ModuleSearchPathKind {
    /// "Extra" paths provided by the user in a config file, env var or CLI flag.
    /// E.g. mypy's `MYPYPATH` env var, or pyright's `stubPath` configuration setting
    Extra,

    /// Files in the project we're directly being invoked on
    FirstParty,

    /// The `stdlib` directory of typeshed (either vendored or custom)
    StandardLibrary,

    /// Stubs or runtime modules installed in site-packages
    SitePackagesThirdParty,

    /// Vendored third-party stubs from typeshed
    VendoredThirdParty,
}
