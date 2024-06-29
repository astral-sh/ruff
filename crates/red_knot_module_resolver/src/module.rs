use std::fmt::Formatter;
use std::sync::Arc;

use ruff_db::vfs::VfsFile;

use crate::module_name::ModuleName;
use crate::path::{
    ExtraPathBuf, FirstPartyPathBuf, ModuleResolutionPathRef, SitePackagesPathBuf,
    StandardLibraryPath, StandardLibraryPathBuf,
};
use crate::Db;

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
