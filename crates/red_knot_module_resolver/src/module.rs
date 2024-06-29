use compact_str::ToCompactString;
use std::fmt::Formatter;
use std::ops::Deref;
use std::sync::Arc;

use ruff_db::vfs::VfsFile;
use ruff_python_stdlib::identifiers::is_identifier;

use crate::path::{
    ExtraPathBuf, FirstPartyPathBuf, ModuleResolutionPathRef, SitePackagesPathBuf,
    StandardLibraryPath, StandardLibraryPathBuf,
};
use crate::Db;

/// A module name, e.g. `foo.bar`.
///
/// Always normalized to the absolute form (never a relative module name, i.e., never `.foo`).
#[derive(Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ModuleName(compact_str::CompactString);

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
        Self::is_valid_name(name).then(|| Self(compact_str::CompactString::from(name)))
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
        // TODO(Micha): Use CompactString::const_new once we upgrade to 0.8 https://github.com/ParkMyCar/compact_str/pull/336
        Self::is_valid_name(name).then(|| Self(compact_str::CompactString::from(name)))
    }

    fn is_valid_name(name: &str) -> bool {
        !name.is_empty() && name.split('.').all(is_identifier)
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
        Some(Self(parent.to_compact_string()))
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

    pub(crate) fn from_relative_path(path: ModuleResolutionPathRef) -> Option<Self> {
        let path = path.sans_dunder_init();
        let mut parts_iter = path.module_name_parts();
        let first_part = parts_iter.next()?;
        if let Some(second_part) = parts_iter.next() {
            let mut name = format!("{first_part}.{second_part}");
            for part in parts_iter {
                name.push('.');
                name.push_str(part);
            }
            Self::new(&name)
        } else {
            Self::new(first_part)
        }
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
        search_path: Arc<ModuleSearchPathEntry>,
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
    pub(crate) fn search_path(&self) -> &ModuleSearchPathEntry {
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
    search_path: Arc<ModuleSearchPathEntry>,
    file: VfsFile,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ModuleKind {
    /// A single-file module (e.g. `foo.py` or `foo.pyi`)
    Module,

    /// A python package (`foo/__init__.py` or `foo/__init__.pyi`)
    Package,
}

/// Enumeration of the different kinds of search paths type checkers are expected to support.
///
/// N.B. Although we don't implement `Ord` for this enum, they are ordered in terms of the
/// priority that we want to give these modules when resolving them,
/// as per [the order given in the typing spec]
///
/// [the order given in the typing spec]: https://typing.readthedocs.io/en/latest/spec/distributing.html#import-resolution-ordering
#[derive(Debug, Eq, PartialEq, Hash)]
pub(crate) enum ModuleSearchPathEntry {
    /// "Extra" paths provided by the user in a config file, env var or CLI flag.
    /// E.g. mypy's `MYPYPATH` env var, or pyright's `stubPath` configuration setting
    Extra(ExtraPathBuf),

    /// Files in the project we're directly being invoked on
    FirstParty(FirstPartyPathBuf),

    /// The `stdlib` directory of typeshed (either vendored or custom)
    StandardLibrary(StandardLibraryPathBuf),

    /// Stubs or runtime modules installed in site-packages
    SitePackagesThirdParty(SitePackagesPathBuf),
    // TODO(Alex): vendor third-party stubs from typeshed as well?
    // VendoredThirdParty(VendoredPathBuf),
}

impl ModuleSearchPathEntry {
    pub(crate) fn stdlib_from_typeshed_root(typeshed: &StandardLibraryPath) -> Self {
        Self::StandardLibrary(StandardLibraryPath::stdlib_from_typeshed_root(typeshed))
    }

    pub(crate) fn path(&self) -> ModuleResolutionPathRef {
        match self {
            Self::Extra(path) => ModuleResolutionPathRef::Extra(path),
            Self::FirstParty(path) => ModuleResolutionPathRef::FirstParty(path),
            Self::StandardLibrary(path) => ModuleResolutionPathRef::StandardLibrary(path),
            Self::SitePackagesThirdParty(path) => ModuleResolutionPathRef::SitePackages(path),
        }
    }
}
