//! Internal abstractions for differentiating between different kinds of search paths.
//!
//! TODO(Alex): Should we use different types for absolute vs relative paths?
//! <https://github.com/astral-sh/ruff/pull/12141#discussion_r1667010245>

use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

use ruff_db::files::{system_path_to_file, vendored_path_to_file, File, FilePath};
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_db::vendored::{VendoredPath, VendoredPathBuf};

use crate::db::Db;
use crate::module_name::ModuleName;
use crate::state::ResolverState;
use crate::typeshed::{TypeshedVersionsParseError, TypeshedVersionsQueryResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FilePathRef<'a> {
    System(&'a SystemPath),
    Vendored(&'a VendoredPath),
}

impl<'a> FilePathRef<'a> {
    fn parent(&self) -> Option<Self> {
        match self {
            Self::System(path) => path.parent().map(Self::System),
            Self::Vendored(path) => path.parent().map(Self::Vendored),
        }
    }

    fn components(&self) -> camino::Utf8Components {
        match self {
            Self::System(path) => path.components(),
            Self::Vendored(path) => path.components(),
        }
    }

    fn file_stem(&self) -> Option<&str> {
        match self {
            Self::System(path) => path.file_stem(),
            Self::Vendored(path) => path.file_stem(),
        }
    }

    #[inline]
    fn to_file(self, db: &dyn Db) -> Option<File> {
        match self {
            Self::System(path) => system_path_to_file(db.upcast(), path),
            Self::Vendored(path) => vendored_path_to_file(db.upcast(), path),
        }
    }
}

impl<'a> From<&'a FilePath> for FilePathRef<'a> {
    fn from(value: &'a FilePath) -> Self {
        match value {
            FilePath::System(path) => FilePathRef::System(path),
            FilePath::Vendored(path) => FilePathRef::Vendored(path),
        }
    }
}

/// Enumeration of the different kinds of search paths type checkers are expected to support.
///
/// N.B. Although we don't implement `Ord` for this enum, they are ordered in terms of the
/// priority that we want to give these modules when resolving them,
/// as per [the order given in the typing spec]
///
/// [the order given in the typing spec]: https://typing.readthedocs.io/en/latest/spec/distributing.html#import-resolution-ordering
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ModulePathBufInner {
    Extra(SystemPathBuf),
    FirstParty(SystemPathBuf),
    StandardLibrary(FilePath),
    SitePackages(SystemPathBuf),
    EditableInstall(SystemPathBuf),
}

impl ModulePathBufInner {
    fn push(&mut self, component: &str) {
        let extension = camino::Utf8Path::new(component).extension();
        match self {
            Self::Extra(ref mut path) => {
                if let Some(extension) = extension {
                    assert!(
                        matches!(extension, "pyi" | "py"),
                        "Extension must be `py` or `pyi`; got `{extension}`"
                    );
                }
                assert!(
                    path.extension().is_none(),
                    "Cannot push part {component} to {path}, which already has an extension"
                );
                path.push(component);
            }
            Self::FirstParty(ref mut path) => {
                if let Some(extension) = extension {
                    assert!(
                        matches!(extension, "pyi" | "py"),
                        "Extension must be `py` or `pyi`; got `{extension}`"
                    );
                }
                assert!(
                    path.extension().is_none(),
                    "Cannot push part {component} to {path}, which already has an extension"
                );
                path.push(component);
            }
            Self::StandardLibrary(ref mut path) => {
                if let Some(extension) = extension {
                    assert_eq!(
                        extension, "pyi",
                        "Extension must be `pyi`; got `{extension}`"
                    );
                }
                assert!(
                    path.extension().is_none(),
                    "Cannot push part {component} to {path:?}, which already has an extension"
                );
                match path {
                    FilePath::System(path) => path.push(component),
                    FilePath::Vendored(path) => path.push(component),
                }
            }
            Self::SitePackages(ref mut path) => {
                if let Some(extension) = extension {
                    assert!(
                        matches!(extension, "pyi" | "py"),
                        "Extension must be `py` or `pyi`; got `{extension}`"
                    );
                }
                assert!(
                    path.extension().is_none(),
                    "Cannot push part {component} to {path}, which already has an extension"
                );
                path.push(component);
            }
            Self::EditableInstall(ref mut path) => {
                if let Some(extension) = extension {
                    assert!(
                        matches!(extension, "pyi" | "py"),
                        "Extension must be `py` or `pyi`; got `{extension}`"
                    );
                }
                assert!(
                    path.extension().is_none(),
                    "Cannot push part {component} to {path}, which already has an extension"
                );
                path.push(component);
            }
        }
    }
}

/// An owned path that points to the source file for a Python module
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct ModulePathBuf(ModulePathBufInner);

impl ModulePathBuf {
    /// Push a new part to the path,
    /// while maintaining the invariant that the path can only have `.py` or `.pyi` extensions.
    /// For the stdlib variant specifically, it may only have a `.pyi` extension.
    ///
    /// ## Panics:
    /// If a component with an invalid extension is passed
    pub(crate) fn push(&mut self, component: &str) {
        self.0.push(component);
    }

    #[must_use]
    pub(crate) fn is_regular_package(&self, search_path: &Self, resolver: &ResolverState) -> bool {
        ModulePathRef::from(self).is_regular_package(search_path, resolver)
    }

    #[must_use]
    pub(crate) fn is_directory(&self, search_path: &Self, resolver: &ResolverState) -> bool {
        ModulePathRef::from(self).is_directory(search_path, resolver)
    }

    #[must_use]
    pub(crate) const fn is_site_packages(&self) -> bool {
        matches!(self.0, ModulePathBufInner::SitePackages(_))
    }

    #[must_use]
    pub(crate) const fn is_standard_library(&self) -> bool {
        matches!(self.0, ModulePathBufInner::StandardLibrary(_))
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> Self {
        ModulePathRef::from(self).with_pyi_extension()
    }

    #[must_use]
    pub(crate) fn with_py_extension(&self) -> Option<Self> {
        ModulePathRef::from(self).with_py_extension()
    }

    #[must_use]
    pub(crate) fn relativize_path<'a>(
        &'a self,
        absolute_path: &'a FilePath,
    ) -> Option<ModulePathRef<'a>> {
        ModulePathRef::from(self).relativize_path(&FilePathRef::from(absolute_path))
    }

    /// Returns `None` if the path doesn't exist, isn't accessible, or if the path points to a directory.
    pub(crate) fn to_file(&self, search_path: &Self, resolver: &ResolverState) -> Option<File> {
        ModulePathRef::from(self).to_file(search_path, resolver)
    }

    pub(crate) fn as_system_path(&self) -> Option<&SystemPathBuf> {
        match &self.0 {
            ModulePathBufInner::Extra(path) => Some(path),
            ModulePathBufInner::FirstParty(path) => Some(path),
            ModulePathBufInner::StandardLibrary(_) => None,
            ModulePathBufInner::SitePackages(path) => Some(path),
            ModulePathBufInner::EditableInstall(path) => Some(path),
        }
    }
}

impl fmt::Debug for ModulePathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ModulePathBufInner::Extra(path) => {
                f.debug_tuple("ModulePathBuf::Extra").field(path).finish()
            }
            ModulePathBufInner::FirstParty(path) => f
                .debug_tuple("ModulePathBuf::FirstParty")
                .field(path)
                .finish(),
            ModulePathBufInner::SitePackages(path) => f
                .debug_tuple("ModulePathBuf::SitePackages")
                .field(path)
                .finish(),
            ModulePathBufInner::StandardLibrary(path) => f
                .debug_tuple("ModulePathBuf::StandardLibrary")
                .field(path)
                .finish(),
            ModulePathBufInner::EditableInstall(path) => f
                .debug_tuple("ModulePathBuf::EditableInstall")
                .field(path)
                .finish(),
        }
    }
}

impl PartialEq<SystemPathBuf> for ModulePathBuf {
    fn eq(&self, other: &SystemPathBuf) -> bool {
        ModulePathRef::from(self) == **other
    }
}

impl PartialEq<ModulePathBuf> for SystemPathBuf {
    fn eq(&self, other: &ModulePathBuf) -> bool {
        other.eq(self)
    }
}

impl PartialEq<VendoredPathBuf> for ModulePathBuf {
    fn eq(&self, other: &VendoredPathBuf) -> bool {
        ModulePathRef::from(self) == **other
    }
}

impl PartialEq<ModulePathBuf> for VendoredPathBuf {
    fn eq(&self, other: &ModulePathBuf) -> bool {
        other.eq(self)
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
enum ModulePathRefInner<'a> {
    Extra(&'a SystemPath),
    FirstParty(&'a SystemPath),
    StandardLibrary(FilePathRef<'a>),
    SitePackages(&'a SystemPath),
    EditableInstall(&'a SystemPath),
}

impl<'a> ModulePathRefInner<'a> {
    #[must_use]
    fn query_stdlib_version<'db>(
        module_path: &FilePathRef<'a>,
        stdlib_search_path: Self,
        stdlib_root: &FilePathRef<'a>,
        resolver_state: &ResolverState<'db>,
    ) -> TypeshedVersionsQueryResult {
        let Some(module_name) = stdlib_search_path
            .relativize_path(module_path)
            .and_then(Self::to_module_name)
        else {
            return TypeshedVersionsQueryResult::DoesNotExist;
        };
        let ResolverState {
            db,
            typeshed_versions,
            target_version,
        } = resolver_state;
        let root_to_pass = match stdlib_root {
            FilePathRef::System(root) => Some(*root),
            FilePathRef::Vendored(_) => None,
        };
        typeshed_versions.query_module(*db, &module_name, root_to_pass, *target_version)
    }

    #[must_use]
    fn is_directory(&self, search_path: Self, resolver: &ResolverState) -> bool {
        match (self, search_path) {
            (Self::Extra(path), Self::Extra(_)) => resolver.system().is_directory(path),
            (Self::FirstParty(path), Self::FirstParty(_)) => resolver.system().is_directory(path),
            (Self::SitePackages(path), Self::SitePackages(_)) => resolver.system().is_directory(path),
            (Self::EditableInstall(path), Self::EditableInstall(_)) => resolver.system().is_directory(path),
            (Self::StandardLibrary(path), Self::StandardLibrary(stdlib_root)) => {
                match Self::query_stdlib_version(path, search_path, &stdlib_root, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists | TypeshedVersionsQueryResult::MaybeExists => match path {
                        FilePathRef::System(path) => resolver.system().is_directory(path),
                        FilePathRef::Vendored(path) => resolver.vendored().is_directory(path)
                    }
                }
            }
            (path, root) => unreachable!(
                "The search path should always be the same variant as `self` (got: {path:?}, {root:?})"
            )
        }
    }

    #[must_use]
    fn is_regular_package(&self, search_path: Self, resolver: &ResolverState) -> bool {
        fn is_non_stdlib_pkg(resolver: &ResolverState, path: &SystemPath) -> bool {
            system_path_to_file(resolver.db.upcast(), path.join("__init__.py")).is_some()
                || system_path_to_file(resolver.db.upcast(), path.join("__init__.py")).is_some()
        }

        match (self, search_path) {
            (Self::Extra(path), Self::Extra(_)) => is_non_stdlib_pkg(resolver, path),
            (Self::FirstParty(path), Self::FirstParty(_)) => is_non_stdlib_pkg(resolver, path),
            (Self::SitePackages(path), Self::SitePackages(_)) => is_non_stdlib_pkg(resolver, path),
            (Self::EditableInstall(path), Self::EditableInstall(_)) => is_non_stdlib_pkg(resolver, path),
            // Unlike the other variants:
            // (1) Account for VERSIONS
            // (2) Only test for `__init__.pyi`, not `__init__.py`
            (Self::StandardLibrary(path), Self::StandardLibrary(stdlib_root)) => {
                match Self::query_stdlib_version( path, search_path, &stdlib_root, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists | TypeshedVersionsQueryResult::MaybeExists => match path {
                        FilePathRef::System(path) => system_path_to_file(resolver.db.upcast(),path.join("__init__.pyi")).is_some(),
                        // No need to use `vendored_path_to_file` here:
                        // (1) The vendored filesystem is immutable, so we don't need to worry about Salsa invalidation
                        // (2) The caching Salsa provides probably won't speed us up that much
                        //     (TODO: check that assumption when we're able to run red-knot on larger code bases)
                        // (3) We don't need the `File` object that `vendored_path_to_file` would return; we just need to know if the file exists
                        FilePathRef::Vendored(path) => resolver.db.vendored().exists(path.join("__init__.pyi"))
                    },
                }
            }
            (path, root) => unreachable!(
                "The search path should always be the same variant as `self` (got: {path:?}, {root:?})"
            )
        }
    }

    fn to_file(self, search_path: Self, resolver: &ResolverState) -> Option<File> {
        match (self, search_path) {
            (Self::Extra(path), Self::Extra(_)) => system_path_to_file(resolver.db.upcast(), path),
            (Self::FirstParty(path), Self::FirstParty(_)) => system_path_to_file(resolver.db.upcast(), path),
            (Self::SitePackages(path), Self::SitePackages(_)) => {
                system_path_to_file(resolver.db.upcast(), path)
            }
            (Self::EditableInstall(path), Self::EditableInstall(_)) => system_path_to_file(resolver.db.upcast(), path),
            (Self::StandardLibrary(path), Self::StandardLibrary(stdlib_root)) => {
                match Self::query_stdlib_version(&path, search_path, &stdlib_root, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => None,
                    TypeshedVersionsQueryResult::Exists => path.to_file(resolver.db),
                    TypeshedVersionsQueryResult::MaybeExists => path.to_file(resolver.db),
                }
            }
            (path, root) => unreachable!(
                "The search path should always be the same variant as `self` (got: {path:?}, {root:?})"
            )
        }
    }

    #[must_use]
    fn to_module_name(self) -> Option<ModuleName> {
        match self {
            Self::Extra(path)
            | Self::FirstParty(path)
            | Self::SitePackages(path)
            | Self::EditableInstall(path) => {
                let parent = path.parent()?;
                let parent_components = parent.components().map(|component| component.as_str());
                let skip_final_part =
                    path.ends_with("__init__.py") || path.ends_with("__init__.pyi");
                if skip_final_part {
                    ModuleName::from_components(parent_components)
                } else {
                    ModuleName::from_components(parent_components.chain(path.file_stem()))
                }
            }
            Self::StandardLibrary(path) => {
                let parent = path.parent()?;
                let parent_components = parent.components().map(|component| component.as_str());
                let skip_final_part = match path {
                    FilePathRef::System(path) => path.ends_with("__init__.pyi"),
                    FilePathRef::Vendored(path) => path.ends_with("__init__.pyi"),
                };
                if skip_final_part {
                    ModuleName::from_components(parent_components)
                } else {
                    ModuleName::from_components(parent_components.chain(path.file_stem()))
                }
            }
        }
    }

    #[must_use]
    fn with_pyi_extension(&self) -> ModulePathBufInner {
        match self {
            Self::Extra(path) => ModulePathBufInner::Extra(path.with_extension("pyi")),
            Self::FirstParty(path) => ModulePathBufInner::FirstParty(path.with_extension("pyi")),
            Self::StandardLibrary(FilePathRef::System(path)) => {
                ModulePathBufInner::StandardLibrary(FilePath::System(path.with_extension("pyi")))
            }
            Self::StandardLibrary(FilePathRef::Vendored(path)) => {
                ModulePathBufInner::StandardLibrary(FilePath::Vendored(path.with_pyi_extension()))
            }
            Self::SitePackages(path) => {
                ModulePathBufInner::SitePackages(path.with_extension("pyi"))
            }
            Self::EditableInstall(path) => {
                ModulePathBufInner::EditableInstall(path.with_extension("pyi"))
            }
        }
    }

    #[must_use]
    fn with_py_extension(&self) -> Option<ModulePathBufInner> {
        match self {
            Self::Extra(path) => Some(ModulePathBufInner::Extra(path.with_extension("py"))),
            Self::FirstParty(path) => {
                Some(ModulePathBufInner::FirstParty(path.with_extension("py")))
            }
            Self::StandardLibrary(_) => None,
            Self::SitePackages(path) => {
                Some(ModulePathBufInner::SitePackages(path.with_extension("py")))
            }
            Self::EditableInstall(path) => Some(ModulePathBufInner::EditableInstall(
                path.with_extension("py"),
            )),
        }
    }

    #[must_use]
    fn relativize_path(&self, absolute_path: &FilePathRef<'a>) -> Option<Self> {
        match (self, absolute_path) {
            (Self::Extra(root), FilePathRef::System(absolute_path)) => {
                absolute_path.strip_prefix(root).ok().and_then(|path| {
                    path.extension()
                        .map_or(true, |ext| matches!(ext, "py" | "pyi"))
                        .then_some(Self::Extra(path))
                })
            }
            (Self::FirstParty(root), FilePathRef::System(absolute_path)) => {
                absolute_path.strip_prefix(root).ok().and_then(|path| {
                    path.extension()
                        .map_or(true, |ext| matches!(ext, "pyi" | "py"))
                        .then_some(Self::FirstParty(path))
                })
            }
            (Self::StandardLibrary(root), FilePathRef::System(absolute_path)) => match root {
                FilePathRef::System(root) => {
                    absolute_path.strip_prefix(root).ok().and_then(|path| {
                        path.extension()
                            .map_or(true, |ext| ext == "pyi")
                            .then_some(Self::StandardLibrary(FilePathRef::System(path)))
                    })
                }
                FilePathRef::Vendored(_) => None,
            },
            (Self::SitePackages(root), FilePathRef::System(absolute_path)) => {
                absolute_path.strip_prefix(root).ok().and_then(|path| {
                    path.extension()
                        .map_or(true, |ext| matches!(ext, "pyi" | "py"))
                        .then_some(Self::SitePackages(path))
                })
            }
            (Self::EditableInstall(root), FilePathRef::System(absolute_path)) => {
                absolute_path.strip_prefix(root).ok().and_then(|path| {
                    path.extension()
                        .map_or(true, |ext| matches!(ext, "pyi" | "py"))
                        .then_some(Self::EditableInstall(path))
                })
            }
            (Self::Extra(_), FilePathRef::Vendored(_)) => None,
            (Self::FirstParty(_), FilePathRef::Vendored(_)) => None,
            (Self::StandardLibrary(root), FilePathRef::Vendored(absolute_path)) => match root {
                FilePathRef::System(_) => None,
                FilePathRef::Vendored(root) => {
                    absolute_path.strip_prefix(root).ok().and_then(|path| {
                        path.extension()
                            .map_or(true, |ext| ext == "pyi")
                            .then_some(Self::StandardLibrary(FilePathRef::Vendored(path)))
                    })
                }
            },
            (Self::SitePackages(_), FilePathRef::Vendored(_)) => None,
            (Self::EditableInstall(_), FilePathRef::Vendored(_)) => None,
        }
    }
}

/// An borrowed path that points to the source file for a Python module
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModulePathRef<'a>(ModulePathRefInner<'a>);

impl<'a> ModulePathRef<'a> {
    #[must_use]
    pub(crate) fn is_directory(
        &self,
        search_path: impl Into<Self>,
        resolver: &ResolverState,
    ) -> bool {
        self.0.is_directory(search_path.into().0, resolver)
    }

    #[must_use]
    pub(crate) fn is_regular_package(
        &self,
        search_path: impl Into<Self>,
        resolver: &ResolverState,
    ) -> bool {
        self.0.is_regular_package(search_path.into().0, resolver)
    }

    #[must_use]
    pub(crate) fn to_file(
        self,
        search_path: impl Into<Self>,
        resolver: &ResolverState,
    ) -> Option<File> {
        self.0.to_file(search_path.into().0, resolver)
    }

    #[must_use]
    pub(crate) fn to_module_name(self) -> Option<ModuleName> {
        self.0.to_module_name()
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> ModulePathBuf {
        ModulePathBuf(self.0.with_pyi_extension())
    }

    #[must_use]
    pub(crate) fn with_py_extension(self) -> Option<ModulePathBuf> {
        self.0.with_py_extension().map(ModulePathBuf)
    }

    #[must_use]
    fn relativize_path(&self, absolute_path: &FilePathRef<'a>) -> Option<Self> {
        self.0.relativize_path(absolute_path).map(Self)
    }
}

impl fmt::Debug for ModulePathRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ModulePathRefInner::Extra(path) => {
                f.debug_tuple("ModulePathRef::Extra").field(path).finish()
            }
            ModulePathRefInner::FirstParty(path) => f
                .debug_tuple("ModulePathRef::FirstParty")
                .field(path)
                .finish(),
            ModulePathRefInner::SitePackages(path) => f
                .debug_tuple("ModulePathRef::SitePackages")
                .field(path)
                .finish(),
            ModulePathRefInner::StandardLibrary(path) => f
                .debug_tuple("ModulePathRef::StandardLibrary")
                .field(path)
                .finish(),
            ModulePathRefInner::EditableInstall(path) => f
                .debug_tuple("ModulePathRef::EditableInstall")
                .field(path)
                .finish(),
        }
    }
}

impl<'a> From<&'a ModulePathBuf> for ModulePathRef<'a> {
    fn from(value: &'a ModulePathBuf) -> Self {
        let inner = match &value.0 {
            ModulePathBufInner::Extra(path) => ModulePathRefInner::Extra(path),
            ModulePathBufInner::FirstParty(path) => ModulePathRefInner::FirstParty(path),
            ModulePathBufInner::StandardLibrary(FilePath::System(path)) => {
                ModulePathRefInner::StandardLibrary(FilePathRef::System(path))
            }
            ModulePathBufInner::StandardLibrary(FilePath::Vendored(path)) => {
                ModulePathRefInner::StandardLibrary(FilePathRef::Vendored(path))
            }
            ModulePathBufInner::SitePackages(path) => ModulePathRefInner::SitePackages(path),
            ModulePathBufInner::EditableInstall(path) => ModulePathRefInner::EditableInstall(path),
        };
        ModulePathRef(inner)
    }
}

impl PartialEq<SystemPath> for ModulePathRef<'_> {
    fn eq(&self, other: &SystemPath) -> bool {
        match self.0 {
            ModulePathRefInner::Extra(path) => path == other,
            ModulePathRefInner::FirstParty(path) => path == other,
            ModulePathRefInner::SitePackages(path) => path == other,
            ModulePathRefInner::EditableInstall(path) => path == other,
            ModulePathRefInner::StandardLibrary(FilePathRef::System(path)) => path == other,
            ModulePathRefInner::StandardLibrary(FilePathRef::Vendored(_)) => false,
        }
    }
}

impl PartialEq<ModulePathRef<'_>> for SystemPath {
    fn eq(&self, other: &ModulePathRef) -> bool {
        other == self
    }
}

impl PartialEq<SystemPathBuf> for ModulePathRef<'_> {
    fn eq(&self, other: &SystemPathBuf) -> bool {
        self == &**other
    }
}

impl PartialEq<ModulePathRef<'_>> for SystemPathBuf {
    fn eq(&self, other: &ModulePathRef<'_>) -> bool {
        &**self == other
    }
}

impl PartialEq<VendoredPath> for ModulePathRef<'_> {
    fn eq(&self, other: &VendoredPath) -> bool {
        match self.0 {
            ModulePathRefInner::Extra(_) => false,
            ModulePathRefInner::FirstParty(_) => false,
            ModulePathRefInner::SitePackages(_) => false,
            ModulePathRefInner::EditableInstall(_) => false,
            ModulePathRefInner::StandardLibrary(FilePathRef::System(_)) => false,
            ModulePathRefInner::StandardLibrary(FilePathRef::Vendored(path)) => path == other,
        }
    }
}

impl PartialEq<ModulePathRef<'_>> for VendoredPath {
    fn eq(&self, other: &ModulePathRef) -> bool {
        other == self
    }
}

impl PartialEq<VendoredPathBuf> for ModulePathRef<'_> {
    fn eq(&self, other: &VendoredPathBuf) -> bool {
        self == &**other
    }
}

impl PartialEq<ModulePathRef<'_>> for VendoredPathBuf {
    fn eq(&self, other: &ModulePathRef<'_>) -> bool {
        &**self == other
    }
}

/// Enumeration describing the various ways in which validation of a search path might fail.
///
/// If validation fails for a search path derived from the user settings,
/// a message must be displayed to the user,
/// as type checking cannot be done reliably in these circumstances.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SearchPathValidationError {
    /// The path provided by the user was not a directory
    NotADirectory(SystemPathBuf),

    /// The path provided by the user is a directory,
    /// but no `stdlib/` subdirectory exists.
    /// (This is only relevant for stdlib search paths.)
    NoStdlibSubdirectory(SystemPathBuf),

    /// The path provided by the user is a directory,
    /// but no `stdlib/VERSIONS` file exists.
    /// (This is only relevant for stdlib search paths.)
    NoVersionsFile(SystemPathBuf),

    /// The path provided by the user is a directory,
    /// and a `stdlib/VERSIONS` file exists, but it fails to parse.
    /// (This is only relevant for stdlib search paths.)
    VersionsParseError(TypeshedVersionsParseError),
}

impl fmt::Display for SearchPathValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotADirectory(path) => write!(f, "{path} does not point to a directory"),
            Self::NoStdlibSubdirectory(path) => {
                write!(f, "The directory at {path} has no `stdlib/` subdirectory")
            }
            Self::NoVersionsFile(path) => write!(f, "Expected a file at {path}/stldib/VERSIONS"),
            Self::VersionsParseError(underlying_error) => underlying_error.fmt(f),
        }
    }
}

impl std::error::Error for SearchPathValidationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::VersionsParseError(underlying_error) = self {
            Some(underlying_error)
        } else {
            None
        }
    }
}

type SearchPathResult = Result<ModuleSearchPath, SearchPathValidationError>;

/// A module-resolution search path, from which [`ModulePath`]s can be derived.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ModuleSearchPath(Arc<ModulePathBuf>);

impl ModuleSearchPath {
    pub(crate) fn extra(system: &dyn System, root: impl Into<SystemPathBuf>) -> SearchPathResult {
        let root = root.into();
        if system.is_directory(&root) {
            Ok(Self(Arc::new(ModulePathBuf(ModulePathBufInner::Extra(
                SystemPath::absolute(root, system.current_directory()),
            )))))
        } else {
            Err(SearchPathValidationError::NotADirectory(root))
        }
    }

    pub(crate) fn first_party(
        system: &dyn System,
        root: impl Into<SystemPathBuf>,
    ) -> SearchPathResult {
        let root = root.into();
        if system.is_directory(&root) {
            Ok(Self(Arc::new(ModulePathBuf(
                ModulePathBufInner::FirstParty(SystemPath::absolute(
                    root,
                    system.current_directory(),
                )),
            ))))
        } else {
            Err(SearchPathValidationError::NotADirectory(root))
        }
    }
    pub(crate) fn custom_stdlib(
        db: &dyn Db,
        typeshed: impl Into<SystemPathBuf>,
    ) -> SearchPathResult {
        let typeshed = typeshed.into();
        let system = db.system();
        if !system.is_directory(&typeshed) {
            return Err(SearchPathValidationError::NotADirectory(typeshed));
        }
        let stdlib = typeshed.join("stdlib");
        if !system.is_directory(&stdlib) {
            return Err(SearchPathValidationError::NoStdlibSubdirectory(typeshed));
        }
        let Some(typeshed_versions) = system_path_to_file(db.upcast(), stdlib.join("VERSIONS"))
        else {
            return Err(SearchPathValidationError::NoVersionsFile(typeshed));
        };
        crate::typeshed::parse_typeshed_versions(db, typeshed_versions)
            .as_ref()
            .map_err(|validation_error| {
                SearchPathValidationError::VersionsParseError(validation_error.clone())
            })?;
        Ok(Self(Arc::new(ModulePathBuf(
            ModulePathBufInner::StandardLibrary(FilePath::System(SystemPath::absolute(
                stdlib,
                system.current_directory(),
            ))),
        ))))
    }

    pub(crate) fn vendored_stdlib() -> Self {
        Self(Arc::new(ModulePathBuf(
            ModulePathBufInner::StandardLibrary(FilePath::Vendored(VendoredPathBuf::from(
                "stdlib",
            ))),
        )))
    }

    pub(crate) fn site_packages(
        system: &dyn System,
        root: impl Into<SystemPathBuf>,
    ) -> SearchPathResult {
        let root = root.into();
        if system.is_directory(&root) {
            Ok(Self(Arc::new(ModulePathBuf(
                ModulePathBufInner::SitePackages(SystemPath::absolute(
                    root,
                    system.current_directory(),
                )),
            ))))
        } else {
            Err(SearchPathValidationError::NotADirectory(root))
        }
    }

    pub(crate) fn editable(
        system: &dyn System,
        root: impl Into<SystemPathBuf>,
    ) -> SearchPathResult {
        let root = root.into();
        if system.is_directory(&root) {
            Ok(Self(Arc::new(ModulePathBuf(
                ModulePathBufInner::EditableInstall(SystemPath::absolute(
                    root,
                    system.current_directory(),
                )),
            ))))
        } else {
            Err(SearchPathValidationError::NotADirectory(root))
        }
    }

    pub(crate) fn as_module_path(&self) -> &ModulePathBuf {
        &self.0
    }
}

impl PartialEq<SystemPathBuf> for ModuleSearchPath {
    fn eq(&self, other: &SystemPathBuf) -> bool {
        &*self.0 == other
    }
}

impl PartialEq<ModuleSearchPath> for SystemPathBuf {
    fn eq(&self, other: &ModuleSearchPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<VendoredPathBuf> for ModuleSearchPath {
    fn eq(&self, other: &VendoredPathBuf) -> bool {
        &*self.0 == other
    }
}

impl PartialEq<ModuleSearchPath> for VendoredPathBuf {
    fn eq(&self, other: &ModuleSearchPath) -> bool {
        other.eq(self)
    }
}

// TODO: this is unprincipled.
// We should instead just implement the methods we need on ModuleSearchPath,
// and adjust the signatures/implementations of methods that receive ModuleSearchPaths.
impl Deref for ModuleSearchPath {
    type Target = ModulePathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;
    use ruff_db::program::TargetVersion;

    use crate::db::tests::TestDb;
    use crate::testing::{FileSpec, MockedTypeshed, TestCase, TestCaseBuilder};

    use super::*;

    impl<'a> FilePathRef<'a> {
        fn system(path: &'a (impl AsRef<SystemPath> + ?Sized)) -> Self {
            Self::System(path.as_ref())
        }
    }

    impl ModulePathBuf {
        #[must_use]
        pub(crate) fn extra(path: impl Into<SystemPathBuf>) -> Self {
            Self(ModulePathBufInner::Extra(path.into()))
        }

        #[must_use]
        pub(crate) fn first_party(path: impl Into<SystemPathBuf>) -> Self {
            Self(ModulePathBufInner::FirstParty(path.into()))
        }

        #[must_use]
        pub(crate) fn standard_library(path: FilePath) -> Self {
            Self(ModulePathBufInner::StandardLibrary(path))
        }

        #[must_use]
        pub(crate) fn site_packages(path: impl Into<SystemPathBuf>) -> Self {
            Self(ModulePathBufInner::SitePackages(path.into()))
        }

        #[must_use]
        pub(crate) fn join(&self, component: &str) -> Self {
            ModulePathRef::from(self).join(component)
        }
    }

    impl<'a> ModulePathRef<'a> {
        #[must_use]
        fn join(&self, component: &'a (impl AsRef<SystemPath> + ?Sized)) -> ModulePathBuf {
            let mut result = self.to_path_buf();
            result.push(component.as_ref().as_str());
            result
        }

        #[must_use]
        pub(crate) fn to_path_buf(self) -> ModulePathBuf {
            let inner = match self.0 {
                ModulePathRefInner::Extra(path) => ModulePathBufInner::Extra(path.to_path_buf()),
                ModulePathRefInner::FirstParty(path) => {
                    ModulePathBufInner::FirstParty(path.to_path_buf())
                }
                ModulePathRefInner::StandardLibrary(FilePathRef::System(path)) => {
                    ModulePathBufInner::StandardLibrary(FilePath::System(path.to_path_buf()))
                }
                ModulePathRefInner::StandardLibrary(FilePathRef::Vendored(path)) => {
                    ModulePathBufInner::StandardLibrary(FilePath::Vendored(path.to_path_buf()))
                }
                ModulePathRefInner::SitePackages(path) => {
                    ModulePathBufInner::SitePackages(path.to_path_buf())
                }
                ModulePathRefInner::EditableInstall(path) => {
                    ModulePathBufInner::EditableInstall(path.to_path_buf())
                }
            };
            ModulePathBuf(inner)
        }
    }

    impl ModuleSearchPath {
        #[must_use]
        pub(crate) fn is_stdlib_search_path(&self) -> bool {
            matches!(&self.0 .0, ModulePathBufInner::StandardLibrary(_))
        }
    }

    #[test]
    fn path_buf_debug_impl() {
        assert_debug_snapshot!(
            ModulePathBuf::standard_library(FilePath::system("foo/bar.pyi")),
            @r###"
        ModulePathBuf::StandardLibrary(
            System(
                "foo/bar.pyi",
            ),
        )
        "###
        );
    }

    #[test]
    fn path_ref_debug_impl() {
        assert_debug_snapshot!(
            ModulePathRef(ModulePathRefInner::Extra(SystemPath::new("foo/bar.py"))),
            @r###"
        ModulePathRef::Extra(
            "foo/bar.py",
        )
        "###
        );
    }

    #[test]
    fn with_extension_methods() {
        assert_eq!(
            ModulePathBuf::standard_library(FilePath::system("foo")).with_py_extension(),
            None
        );

        assert_eq!(
            ModulePathBuf::standard_library(FilePath::system("foo")).with_pyi_extension(),
            ModulePathBuf(ModulePathBufInner::StandardLibrary(FilePath::System(
                SystemPathBuf::from("foo.pyi")
            )))
        );

        assert_eq!(
            ModulePathBuf::first_party("foo/bar")
                .with_py_extension()
                .unwrap(),
            ModulePathBuf(ModulePathBufInner::FirstParty(SystemPathBuf::from(
                "foo/bar.py"
            )))
        );
    }

    #[test]
    fn module_name_1_part() {
        assert_eq!(
            ModulePathRef(ModulePathRefInner::Extra(SystemPath::new("foo"))).to_module_name(),
            ModuleName::new_static("foo")
        );

        assert_eq!(
            ModulePathRef(ModulePathRefInner::StandardLibrary(FilePathRef::system(
                "foo.pyi"
            )))
            .to_module_name(),
            ModuleName::new_static("foo")
        );

        assert_eq!(
            ModulePathRef(ModulePathRefInner::FirstParty(SystemPath::new(
                "foo/__init__.py"
            )))
            .to_module_name(),
            ModuleName::new_static("foo")
        );
    }

    #[test]
    fn module_name_2_parts() {
        assert_eq!(
            ModulePathRef(ModulePathRefInner::StandardLibrary(FilePathRef::system(
                "foo/bar"
            )))
            .to_module_name(),
            ModuleName::new_static("foo.bar")
        );

        assert_eq!(
            ModulePathRef(ModulePathRefInner::Extra(SystemPath::new("foo/bar.pyi")))
                .to_module_name(),
            ModuleName::new_static("foo.bar")
        );

        assert_eq!(
            ModulePathRef(ModulePathRefInner::SitePackages(SystemPath::new(
                "foo/bar/__init__.pyi"
            )))
            .to_module_name(),
            ModuleName::new_static("foo.bar")
        );
    }

    #[test]
    fn module_name_3_parts() {
        assert_eq!(
            ModulePathRef(ModulePathRefInner::SitePackages(SystemPath::new(
                "foo/bar/__init__.pyi"
            )))
            .to_module_name(),
            ModuleName::new_static("foo.bar")
        );

        assert_eq!(
            ModulePathRef(ModulePathRefInner::SitePackages(SystemPath::new(
                "foo/bar/baz"
            )))
            .to_module_name(),
            ModuleName::new_static("foo.bar.baz")
        );
    }

    #[test]
    fn join() {
        assert_eq!(
            ModulePathBuf::standard_library(FilePath::system("foo")).join("bar"),
            ModulePathBuf(ModulePathBufInner::StandardLibrary(FilePath::system(
                "foo/bar"
            )))
        );
        assert_eq!(
            ModulePathBuf::standard_library(FilePath::system("foo")).join("bar.pyi"),
            ModulePathBuf(ModulePathBufInner::StandardLibrary(FilePath::system(
                "foo/bar.pyi"
            )))
        );
        assert_eq!(
            ModulePathBuf::extra("foo").join("bar.py"),
            ModulePathBuf(ModulePathBufInner::Extra(SystemPathBuf::from("foo/bar.py")))
        );
    }

    #[test]
    #[should_panic(expected = "Extension must be `pyi`; got `py`")]
    fn stdlib_path_invalid_join_py() {
        ModulePathBuf::standard_library(FilePath::system("foo")).push("bar.py");
    }

    #[test]
    #[should_panic(expected = "Extension must be `pyi`; got `rs`")]
    fn stdlib_path_invalid_join_rs() {
        ModulePathBuf::standard_library(FilePath::system("foo")).push("bar.rs");
    }

    #[test]
    #[should_panic(expected = "Extension must be `py` or `pyi`; got `rs`")]
    fn non_stdlib_path_invalid_join_rs() {
        ModulePathBuf::site_packages("foo").push("bar.rs");
    }

    #[test]
    #[should_panic(expected = "already has an extension")]
    fn invalid_stdlib_join_too_many_extensions() {
        ModulePathBuf::standard_library(FilePath::system("foo.pyi")).push("bar.pyi");
    }

    #[test]
    fn relativize_stdlib_path_errors() {
        let root = ModulePathBuf::standard_library(FilePath::system("foo/stdlib"));

        // Must have a `.pyi` extension or no extension:
        let bad_absolute_path = FilePath::system("foo/stdlib/x.py");
        assert_eq!(root.relativize_path(&bad_absolute_path), None);
        let second_bad_absolute_path = FilePath::system("foo/stdlib/x.rs");
        assert_eq!(root.relativize_path(&second_bad_absolute_path), None);

        // Must be a path that is a child of `root`:
        let third_bad_absolute_path = FilePath::system("bar/stdlib/x.pyi");
        assert_eq!(root.relativize_path(&third_bad_absolute_path), None);
    }

    #[test]
    fn relativize_non_stdlib_path_errors() {
        let root = ModulePathBuf::extra("foo/stdlib");
        // Must have a `.py` extension, a `.pyi` extension, or no extension:
        let bad_absolute_path = FilePath::system("foo/stdlib/x.rs");
        assert_eq!(root.relativize_path(&bad_absolute_path), None);
        // Must be a path that is a child of `root`:
        let second_bad_absolute_path = FilePath::system("bar/stdlib/x.pyi");
        assert_eq!(root.relativize_path(&second_bad_absolute_path), None);
    }

    #[test]
    fn relativize_path() {
        assert_eq!(
            ModulePathBuf::standard_library(FilePath::system("foo/baz"))
                .relativize_path(&FilePath::system("foo/baz/eggs/__init__.pyi"))
                .unwrap(),
            ModulePathRef(ModulePathRefInner::StandardLibrary(FilePathRef::system(
                "eggs/__init__.pyi"
            )))
        );
    }

    fn typeshed_test_case(
        typeshed: MockedTypeshed,
        target_version: TargetVersion,
    ) -> (TestDb, ModulePathBuf) {
        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_custom_typeshed(typeshed)
            .with_target_version(target_version)
            .build();
        let stdlib = ModulePathBuf::standard_library(FilePath::System(stdlib));
        (db, stdlib)
    }

    fn py38_typeshed_test_case(typeshed: MockedTypeshed) -> (TestDb, ModulePathBuf) {
        typeshed_test_case(typeshed, TargetVersion::Py38)
    }

    fn py39_typeshed_test_case(typeshed: MockedTypeshed) -> (TestDb, ModulePathBuf) {
        typeshed_test_case(typeshed, TargetVersion::Py39)
    }

    #[test]
    fn mocked_typeshed_existing_regular_stdlib_pkg_py38() {
        const VERSIONS: &str = "\
            asyncio: 3.8-
            asyncio.tasks: 3.9-3.11
        ";

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: VERSIONS,
            stdlib_files: &[("asyncio/__init__.pyi", ""), ("asyncio/tasks.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver = ResolverState::new(&db, TargetVersion::Py38);

        let asyncio_regular_package = stdlib_path.join("asyncio");
        assert!(asyncio_regular_package.is_directory(&stdlib_path, &resolver));
        assert!(asyncio_regular_package.is_regular_package(&stdlib_path, &resolver));
        // Paths to directories don't resolve to VfsFiles
        assert_eq!(
            asyncio_regular_package.to_file(&stdlib_path, &resolver),
            None
        );
        assert!(asyncio_regular_package
            .join("__init__.pyi")
            .to_file(&stdlib_path, &resolver)
            .is_some());

        // The `asyncio` package exists on Python 3.8, but the `asyncio.tasks` submodule does not,
        // according to the `VERSIONS` file in our typeshed mock:
        let asyncio_tasks_module = stdlib_path.join("asyncio/tasks.pyi");
        assert_eq!(asyncio_tasks_module.to_file(&stdlib_path, &resolver), None);
        assert!(!asyncio_tasks_module.is_directory(&stdlib_path, &resolver));
        assert!(!asyncio_tasks_module.is_regular_package(&stdlib_path, &resolver));
    }

    #[test]
    fn mocked_typeshed_existing_namespace_stdlib_pkg_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "xml: 3.8-3.8",
            stdlib_files: &[("xml/etree.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver = ResolverState::new(&db, TargetVersion::Py38);

        let xml_namespace_package = stdlib_path.join("xml");
        assert!(xml_namespace_package.is_directory(&stdlib_path, &resolver));
        // Paths to directories don't resolve to VfsFiles
        assert_eq!(xml_namespace_package.to_file(&stdlib_path, &resolver), None);
        assert!(!xml_namespace_package.is_regular_package(&stdlib_path, &resolver));

        let xml_etree = stdlib_path.join("xml/etree.pyi");
        assert!(!xml_etree.is_directory(&stdlib_path, &resolver));
        assert!(xml_etree.to_file(&stdlib_path, &resolver).is_some());
        assert!(!xml_etree.is_regular_package(&stdlib_path, &resolver));
    }

    #[test]
    fn mocked_typeshed_single_file_stdlib_module_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver = ResolverState::new(&db, TargetVersion::Py38);

        let functools_module = stdlib_path.join("functools.pyi");
        assert!(functools_module.to_file(&stdlib_path, &resolver).is_some());
        assert!(!functools_module.is_directory(&stdlib_path, &resolver));
        assert!(!functools_module.is_regular_package(&stdlib_path, &resolver));
    }

    #[test]
    fn mocked_typeshed_nonexistent_regular_stdlib_pkg_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "collections: 3.9-",
            stdlib_files: &[("collections/__init__.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver = ResolverState::new(&db, TargetVersion::Py38);

        let collections_regular_package = stdlib_path.join("collections");
        assert_eq!(
            collections_regular_package.to_file(&stdlib_path, &resolver),
            None
        );
        assert!(!collections_regular_package.is_directory(&stdlib_path, &resolver));
        assert!(!collections_regular_package.is_regular_package(&stdlib_path, &resolver));
    }

    #[test]
    fn mocked_typeshed_nonexistent_namespace_stdlib_pkg_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "importlib: 3.9-",
            stdlib_files: &[("importlib/abc.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver = ResolverState::new(&db, TargetVersion::Py38);

        let importlib_namespace_package = stdlib_path.join("importlib");
        assert_eq!(
            importlib_namespace_package.to_file(&stdlib_path, &resolver),
            None
        );
        assert!(!importlib_namespace_package.is_directory(&stdlib_path, &resolver));
        assert!(!importlib_namespace_package.is_regular_package(&stdlib_path, &resolver));

        let importlib_abc = stdlib_path.join("importlib/abc.pyi");
        assert_eq!(importlib_abc.to_file(&stdlib_path, &resolver), None);
        assert!(!importlib_abc.is_directory(&stdlib_path, &resolver));
        assert!(!importlib_abc.is_regular_package(&stdlib_path, &resolver));
    }

    #[test]
    fn mocked_typeshed_nonexistent_single_file_module_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "foo: 2.6-",
            stdlib_files: &[("foo.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver = ResolverState::new(&db, TargetVersion::Py38);

        let non_existent = stdlib_path.join("doesnt_even_exist");
        assert_eq!(non_existent.to_file(&stdlib_path, &resolver), None);
        assert!(!non_existent.is_directory(&stdlib_path, &resolver));
        assert!(!non_existent.is_regular_package(&stdlib_path, &resolver));
    }

    #[test]
    fn mocked_typeshed_existing_regular_stdlib_pkgs_py39() {
        const VERSIONS: &str = "\
            asyncio: 3.8-
            asyncio.tasks: 3.9-3.11
            collections: 3.9-
        ";

        const STDLIB: &[FileSpec] = &[
            ("asyncio/__init__.pyi", ""),
            ("asyncio/tasks.pyi", ""),
            ("collections/__init__.pyi", ""),
        ];

        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: VERSIONS,
            stdlib_files: STDLIB,
        };

        let (db, stdlib_path) = py39_typeshed_test_case(TYPESHED);
        let resolver = ResolverState::new(&db, TargetVersion::Py39);

        // Since we've set the target version to Py39,
        // `collections` should now exist as a directory, according to VERSIONS...
        let collections_regular_package = stdlib_path.join("collections");
        assert!(collections_regular_package.is_directory(&stdlib_path, &resolver));
        assert!(collections_regular_package.is_regular_package(&stdlib_path, &resolver));
        // (This is still `None`, as directories don't resolve to `Vfs` files)
        assert_eq!(
            collections_regular_package.to_file(&stdlib_path, &resolver),
            None
        );
        assert!(collections_regular_package
            .join("__init__.pyi")
            .to_file(&stdlib_path, &resolver)
            .is_some());

        // ...and so should the `asyncio.tasks` submodule (though it's still not a directory):
        let asyncio_tasks_module = stdlib_path.join("asyncio/tasks.pyi");
        assert!(asyncio_tasks_module
            .to_file(&stdlib_path, &resolver)
            .is_some());
        assert!(!asyncio_tasks_module.is_directory(&stdlib_path, &resolver));
        assert!(!asyncio_tasks_module.is_regular_package(&stdlib_path, &resolver));
    }

    #[test]
    fn mocked_typeshed_existing_namespace_stdlib_pkg_py39() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "importlib: 3.9-",
            stdlib_files: &[("importlib/abc.pyi", "")],
        };

        let (db, stdlib_path) = py39_typeshed_test_case(TYPESHED);
        let resolver = ResolverState::new(&db, TargetVersion::Py39);

        // The `importlib` directory now also exists
        let importlib_namespace_package = stdlib_path.join("importlib");
        assert!(importlib_namespace_package.is_directory(&stdlib_path, &resolver));
        assert!(!importlib_namespace_package.is_regular_package(&stdlib_path, &resolver));
        // (This is still `None`, as directories don't resolve to `Vfs` files)
        assert_eq!(
            importlib_namespace_package.to_file(&stdlib_path, &resolver),
            None
        );

        // Submodules in the `importlib` namespace package also now exist:
        let importlib_abc = importlib_namespace_package.join("abc.pyi");
        assert!(!importlib_abc.is_directory(&stdlib_path, &resolver));
        assert!(!importlib_abc.is_regular_package(&stdlib_path, &resolver));
        assert!(importlib_abc.to_file(&stdlib_path, &resolver).is_some());
    }

    #[test]
    fn mocked_typeshed_nonexistent_namespace_stdlib_pkg_py39() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "xml: 3.8-3.8",
            stdlib_files: &[("xml/etree.pyi", "")],
        };

        let (db, stdlib_path) = py39_typeshed_test_case(TYPESHED);
        let resolver = ResolverState::new(&db, TargetVersion::Py39);

        // The `xml` package no longer exists on py39:
        let xml_namespace_package = stdlib_path.join("xml");
        assert_eq!(xml_namespace_package.to_file(&stdlib_path, &resolver), None);
        assert!(!xml_namespace_package.is_directory(&stdlib_path, &resolver));
        assert!(!xml_namespace_package.is_regular_package(&stdlib_path, &resolver));

        let xml_etree = xml_namespace_package.join("etree.pyi");
        assert_eq!(xml_etree.to_file(&stdlib_path, &resolver), None);
        assert!(!xml_etree.is_directory(&stdlib_path, &resolver));
        assert!(!xml_etree.is_regular_package(&stdlib_path, &resolver));
    }
}
