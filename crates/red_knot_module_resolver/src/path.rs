use std::iter::FusedIterator;

use ruff_db::file_system::{FileSystemPath, FileSystemPathBuf};
use ruff_db::vfs::{system_path_to_file, VfsPath};

use crate::db::Db;
use crate::module_name::ModuleName;
use crate::supported_py_version::get_target_py_version;
use crate::typeshed::{parse_typeshed_versions, TypeshedVersions, TypeshedVersionsQueryResult};

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct ExtraPath(FileSystemPath);

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub(crate) struct ExtraPathBuf(FileSystemPathBuf);

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct FirstPartyPath(FileSystemPath);

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub(crate) struct FirstPartyPathBuf(FileSystemPathBuf);

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct StandardLibraryPath(FileSystemPath);

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub(crate) struct StandardLibraryPathBuf(FileSystemPathBuf);

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub(crate) struct SitePackagesPath(FileSystemPath);

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub(crate) struct SitePackagesPathBuf(FileSystemPathBuf);

/// Enumeration of the different kinds of search paths type checkers are expected to support.
///
/// N.B. Although we don't implement `Ord` for this enum, they are ordered in terms of the
/// priority that we want to give these modules when resolving them,
/// as per [the order given in the typing spec]
///
/// [the order given in the typing spec]: https://typing.readthedocs.io/en/latest/spec/distributing.html#import-resolution-ordering
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ModuleResolutionPathBuf {
    Extra(ExtraPathBuf),
    FirstParty(FirstPartyPathBuf),
    StandardLibrary(StandardLibraryPathBuf),
    SitePackages(SitePackagesPathBuf),
}

impl ModuleResolutionPathBuf {
    /// Push a new part to the path,
    /// while maintaining the invariant that the path can only have `.py` or `.pyi` extensions.
    /// For the stdlib variant specifically, it may only have a `.pyi` extension.
    ///
    /// ## Panics:
    /// If a component with an invalid extension is passed
    pub(crate) fn push(&mut self, component: &str) {
        debug_assert!(matches!(component.matches('.').count(), 0 | 1));
        if cfg!(debug) {
            if let Some(extension) = camino::Utf8Path::new(component).extension() {
                match self {
                    Self::Extra(_) | Self::FirstParty(_) | Self::SitePackages(_) => assert!(
                        matches!(extension, "pyi" | "py"),
                        "Extension must be `py` or `pyi`; got {extension:?}"
                    ),
                    Self::StandardLibrary(_) => assert_eq!(
                        extension, "pyi",
                        "Extension must be `py` or `pyi`; got {extension:?}"
                    ),
                };
            }
        }
        let inner = match self {
            Self::Extra(ExtraPathBuf(ref mut path)) => path,
            Self::FirstParty(FirstPartyPathBuf(ref mut path)) => path,
            Self::StandardLibrary(StandardLibraryPathBuf(ref mut path)) => path,
            Self::SitePackages(SitePackagesPathBuf(ref mut path)) => path,
        };
        inner.push(component);
    }

    #[must_use]
    pub(crate) fn extra(path: FileSystemPathBuf) -> Option<Self> {
        if path
            .extension()
            .map_or(true, |ext| matches!(ext, "py" | "pyi"))
        {
            Some(Self::Extra(ExtraPathBuf(path)))
        } else {
            None
        }
    }

    #[must_use]
    pub(crate) fn first_party(path: FileSystemPathBuf) -> Option<Self> {
        if path
            .extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
        {
            Some(Self::FirstParty(FirstPartyPathBuf(path)))
        } else {
            None
        }
    }

    #[must_use]
    pub(crate) fn standard_library(path: FileSystemPathBuf) -> Option<Self> {
        if path.extension().map_or(true, |ext| ext == "pyi") {
            Some(Self::StandardLibrary(StandardLibraryPathBuf(path)))
        } else {
            None
        }
    }

    #[must_use]
    pub(crate) fn stdlib_from_typeshed_root(typeshed_root: &FileSystemPath) -> Option<Self> {
        Self::standard_library(typeshed_root.join(FileSystemPath::new("stdlib")))
    }

    #[must_use]
    pub(crate) fn site_packages(path: FileSystemPathBuf) -> Option<Self> {
        if path
            .extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
        {
            Some(Self::SitePackages(SitePackagesPathBuf(path)))
        } else {
            None
        }
    }

    #[must_use]
    pub(crate) fn is_regular_package(&self, db: &dyn Db, search_path: &Self) -> bool {
        ModuleResolutionPathRef::from(self).is_regular_package(db, search_path)
    }

    #[must_use]
    pub(crate) fn is_directory(&self, db: &dyn Db, search_path: &Self) -> bool {
        ModuleResolutionPathRef::from(self).is_directory(db, search_path)
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> Self {
        ModuleResolutionPathRef::from(self).with_pyi_extension()
    }

    #[must_use]
    pub(crate) fn with_py_extension(&self) -> Option<Self> {
        ModuleResolutionPathRef::from(self).with_py_extension()
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn join(&self, component: &(impl AsRef<FileSystemPath> + ?Sized)) -> Self {
        ModuleResolutionPathRef::from(self).join(component)
    }

    pub(crate) fn relativize_path<'a>(
        &'a self,
        absolute_path: &'a FileSystemPath,
    ) -> Option<ModuleResolutionPathRef<'a>> {
        ModuleResolutionPathRef::from(self).relativize_path(absolute_path)
    }
}

impl From<ModuleResolutionPathBuf> for VfsPath {
    fn from(value: ModuleResolutionPathBuf) -> Self {
        VfsPath::FileSystem(match value {
            ModuleResolutionPathBuf::Extra(ExtraPathBuf(path)) => path,
            ModuleResolutionPathBuf::FirstParty(FirstPartyPathBuf(path)) => path,
            ModuleResolutionPathBuf::StandardLibrary(StandardLibraryPathBuf(path)) => path,
            ModuleResolutionPathBuf::SitePackages(SitePackagesPathBuf(path)) => path,
        })
    }
}

impl AsRef<FileSystemPath> for ModuleResolutionPathBuf {
    #[inline]
    fn as_ref(&self) -> &FileSystemPath {
        ModuleResolutionPathRef::from(self).as_file_system_path()
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub(crate) enum ModuleResolutionPathRef<'a> {
    Extra(&'a ExtraPath),
    FirstParty(&'a FirstPartyPath),
    StandardLibrary(&'a StandardLibraryPath),
    SitePackages(&'a SitePackagesPath),
}

impl<'a> ModuleResolutionPathRef<'a> {
    #[must_use]
    pub(crate) fn extra(path: &'a (impl AsRef<FileSystemPath> + ?Sized)) -> Option<Self> {
        let path = path.as_ref();
        if path
            .extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
        {
            Some(Self::extra_unchecked(path))
        } else {
            None
        }
    }

    #[must_use]
    #[allow(unsafe_code)]
    fn extra_unchecked(path: &'a (impl AsRef<FileSystemPath> + ?Sized)) -> Self {
        // SAFETY: ExtraPath is marked as #[repr(transparent)] so the conversion from a
        // *const FileSystemPath to a *const ExtraPath is valid.
        Self::Extra(unsafe { &*(path.as_ref() as *const FileSystemPath as *const ExtraPath) })
    }

    #[must_use]
    pub(crate) fn first_party(path: &'a (impl AsRef<FileSystemPath> + ?Sized)) -> Option<Self> {
        let path = path.as_ref();
        if path
            .extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
        {
            Some(Self::first_party_unchecked(path))
        } else {
            None
        }
    }

    #[must_use]
    #[allow(unsafe_code)]
    fn first_party_unchecked(path: &'a (impl AsRef<FileSystemPath> + ?Sized)) -> Self {
        // SAFETY: FirstPartyPath is marked as #[repr(transparent)] so the conversion from a
        // *const FileSystemPath to a *const FirstPartyPath is valid.
        Self::FirstParty(unsafe {
            &*(path.as_ref() as *const FileSystemPath as *const FirstPartyPath)
        })
    }

    #[must_use]
    pub(crate) fn standard_library(
        path: &'a (impl AsRef<FileSystemPath> + ?Sized),
    ) -> Option<Self> {
        let path = path.as_ref();
        // Unlike other variants, only `.pyi` extensions are permitted
        if path.extension().map_or(true, |ext| ext == "pyi") {
            Some(Self::standard_library_unchecked(path))
        } else {
            None
        }
    }

    #[must_use]
    #[allow(unsafe_code)]
    fn standard_library_unchecked(path: &'a (impl AsRef<FileSystemPath> + ?Sized)) -> Self {
        // SAFETY: StandardLibraryPath is marked as #[repr(transparent)] so the conversion from a
        // *const FileSystemPath to a *const StandardLibraryPath is valid.
        Self::StandardLibrary(unsafe {
            &*(path.as_ref() as *const FileSystemPath as *const StandardLibraryPath)
        })
    }

    #[must_use]
    pub(crate) fn site_packages(path: &'a (impl AsRef<FileSystemPath> + ?Sized)) -> Option<Self> {
        let path = path.as_ref();
        if path
            .extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
        {
            Some(Self::site_packages_unchecked(path))
        } else {
            None
        }
    }

    #[must_use]
    #[allow(unsafe_code)]
    fn site_packages_unchecked(path: &'a (impl AsRef<FileSystemPath> + ?Sized)) -> Self {
        // SAFETY: SitePackagesPath is marked as #[repr(transparent)] so the conversion from a
        // *const FileSystemPath to a *const SitePackagesPath is valid.
        Self::SitePackages(unsafe {
            &*(path.as_ref() as *const FileSystemPath as *const SitePackagesPath)
        })
    }

    #[must_use]
    fn load_typeshed_versions<'db>(
        db: &'db dyn Db,
        stdlib_root: &StandardLibraryPath,
    ) -> &'db TypeshedVersions {
        let StandardLibraryPath(stdlib_fs_path) = stdlib_root;
        let versions_path = stdlib_fs_path.join("VERSIONS");
        let Some(versions_file) = system_path_to_file(db.upcast(), &versions_path) else {
            todo!(
                "Still need to figure out how to handle VERSIONS files being deleted \
                from custom typeshed directories! Expected a file to exist at {versions_path}"
            )
        };
        // TODO(Alex/Micha): If VERSIONS is invalid,
        // this should invalidate not just the specific module resolution we're currently attempting,
        // but all type inference that depends on any standard-library types.
        // Unwrapping here is not correct...
        parse_typeshed_versions(db, versions_file).as_ref().unwrap()
    }

    // Private helper function with concrete inputs,
    // to avoid monomorphization
    #[must_use]
    fn is_directory_impl(&self, db: &dyn Db, search_path: Self) -> bool {
        match (self, search_path) {
            (Self::Extra(ExtraPath(path)), Self::Extra(_)) => db.file_system().is_directory(path),
            (Self::FirstParty(FirstPartyPath(path)), Self::FirstParty(_)) => db.file_system().is_directory(path),
            (Self::SitePackages(SitePackagesPath(path)), Self::SitePackages(_)) => db.file_system().is_directory(path),
            (Self::StandardLibrary(StandardLibraryPath(path)), Self::StandardLibrary(stdlib_root)) => {
                let Some(module_name) = self.as_module_name() else {
                    return false;
                };
                let typeshed_versions = Self::load_typeshed_versions(db, stdlib_root);
                match typeshed_versions.query_module(&module_name, get_target_py_version(db)) {
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        db.file_system().is_directory(path)
                    }
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                }
            }
            (path, root) => unreachable!(
                "The search path should always be the same variant as `self` (got: {path:?}, {root:?})"
            )
        }
    }

    #[must_use]
    pub(crate) fn is_directory(&self, db: &dyn Db, search_path: impl Into<Self>) -> bool {
        self.is_directory_impl(db, search_path.into())
    }

    // Private helper function with concrete inputs,
    // to avoid monomorphization
    #[must_use]
    fn is_regular_package_impl(&self, db: &dyn Db, search_path: Self) -> bool {
        match (self, search_path) {
            (Self::Extra(ExtraPath(fs_path)), Self::Extra(_))
            | (Self::FirstParty(FirstPartyPath(fs_path)), Self::FirstParty(_))
            | (Self::SitePackages(SitePackagesPath(fs_path)), Self::SitePackages(_)) => {
                let file_system = db.file_system();
                file_system.exists(&fs_path.join("__init__.py"))
                    || file_system.exists(&fs_path.join("__init__.pyi"))
            }
            // Unlike the other variants:
            // (1) Account for VERSIONS
            // (2) Only test for `__init__.pyi`, not `__init__.py`
            (Self::StandardLibrary(StandardLibraryPath(fs_path)), Self::StandardLibrary(stdlib_root)) => {
                let Some(module_name) = self.as_module_name() else {
                    return false;
                };
                let typeshed_versions = Self::load_typeshed_versions(db, stdlib_root);
                match typeshed_versions.query_module(&module_name, get_target_py_version(db)) {
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        db.file_system().exists(&fs_path.join("__init__.pyi"))
                    }
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                }
            }
            (path, root) => unreachable!(
                "The search path should always be the same variant as `self` (got: {path:?}, {root:?})"
            )
        }
    }

    #[must_use]
    pub(crate) fn is_regular_package(&self, db: &dyn Db, search_path: impl Into<Self>) -> bool {
        self.is_regular_package_impl(db, search_path.into())
    }

    #[must_use]
    pub(crate) fn parent(&self) -> Option<Self> {
        Some(match self {
            Self::Extra(ExtraPath(path)) => Self::extra_unchecked(path.parent()?),
            Self::FirstParty(FirstPartyPath(path)) => Self::first_party_unchecked(path.parent()?),
            Self::StandardLibrary(StandardLibraryPath(path)) => {
                Self::standard_library_unchecked(path.parent()?)
            }
            Self::SitePackages(SitePackagesPath(path)) => {
                Self::site_packages_unchecked(path.parent()?)
            }
        })
    }

    #[must_use]
    fn ends_with_dunder_init(&self) -> bool {
        match self {
            Self::Extra(ExtraPath(path))
            | Self::FirstParty(FirstPartyPath(path))
            | Self::SitePackages(SitePackagesPath(path)) => {
                path.ends_with("__init__.py") || path.ends_with("__init__.pyi")
            }
            Self::StandardLibrary(StandardLibraryPath(path)) => path.ends_with("__init__.pyi"),
        }
    }

    #[must_use]
    fn with_dunder_init_stripped(self) -> Self {
        if self.ends_with_dunder_init() {
            self.parent().unwrap_or_else(|| match self {
                Self::Extra(_) => Self::extra_unchecked(""),
                Self::FirstParty(_) => Self::first_party_unchecked(""),
                Self::StandardLibrary(_) => Self::standard_library_unchecked(""),
                Self::SitePackages(_) => Self::site_packages_unchecked(""),
            })
        } else {
            self
        }
    }

    #[must_use]
    pub(crate) fn as_module_name(&self) -> Option<ModuleName> {
        ModuleName::from_components(match self.with_dunder_init_stripped() {
            Self::Extra(ExtraPath(path)) => ModulePartIterator::from_fs_path(path),
            Self::FirstParty(FirstPartyPath(path)) => ModulePartIterator::from_fs_path(path),
            Self::StandardLibrary(StandardLibraryPath(path)) => {
                ModulePartIterator::from_fs_path(path)
            }
            Self::SitePackages(SitePackagesPath(path)) => ModulePartIterator::from_fs_path(path),
        })
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> ModuleResolutionPathBuf {
        match self {
            Self::Extra(ExtraPath(path)) => {
                ModuleResolutionPathBuf::Extra(ExtraPathBuf(path.with_extension("pyi")))
            }
            Self::FirstParty(FirstPartyPath(path)) => {
                ModuleResolutionPathBuf::FirstParty(FirstPartyPathBuf(path.with_extension("pyi")))
            }
            Self::StandardLibrary(StandardLibraryPath(path)) => {
                ModuleResolutionPathBuf::StandardLibrary(StandardLibraryPathBuf(
                    path.with_extension("pyi"),
                ))
            }
            Self::SitePackages(SitePackagesPath(path)) => ModuleResolutionPathBuf::SitePackages(
                SitePackagesPathBuf(path.with_extension("pyi")),
            ),
        }
    }

    #[must_use]
    pub(crate) fn with_py_extension(&self) -> Option<ModuleResolutionPathBuf> {
        match self {
            Self::Extra(ExtraPath(path)) => Some(ModuleResolutionPathBuf::Extra(ExtraPathBuf(
                path.with_extension("py"),
            ))),
            Self::FirstParty(FirstPartyPath(path)) => Some(ModuleResolutionPathBuf::FirstParty(
                FirstPartyPathBuf(path.with_extension("py")),
            )),
            Self::StandardLibrary(_) => None,
            Self::SitePackages(SitePackagesPath(path)) => {
                Some(ModuleResolutionPathBuf::SitePackages(SitePackagesPathBuf(
                    path.with_extension("py"),
                )))
            }
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn to_path_buf(self) -> ModuleResolutionPathBuf {
        match self {
            Self::Extra(ExtraPath(path)) => {
                ModuleResolutionPathBuf::Extra(ExtraPathBuf(path.to_path_buf()))
            }
            Self::FirstParty(FirstPartyPath(path)) => {
                ModuleResolutionPathBuf::FirstParty(FirstPartyPathBuf(path.to_path_buf()))
            }
            Self::StandardLibrary(StandardLibraryPath(path)) => {
                ModuleResolutionPathBuf::StandardLibrary(StandardLibraryPathBuf(path.to_path_buf()))
            }
            Self::SitePackages(SitePackagesPath(path)) => {
                ModuleResolutionPathBuf::SitePackages(SitePackagesPathBuf(path.to_path_buf()))
            }
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn join(
        &self,
        component: &'a (impl AsRef<FileSystemPath> + ?Sized),
    ) -> ModuleResolutionPathBuf {
        let mut result = self.to_path_buf();
        result.push(component.as_ref().as_str());
        result
    }

    #[must_use]
    #[inline]
    fn as_file_system_path(self) -> &'a FileSystemPath {
        match self {
            Self::Extra(ExtraPath(path)) => path,
            Self::FirstParty(FirstPartyPath(path)) => path,
            Self::StandardLibrary(StandardLibraryPath(path)) => path,
            Self::SitePackages(SitePackagesPath(path)) => path,
        }
    }

    #[must_use]
    pub(crate) fn relativize_path(&self, absolute_path: &'a FileSystemPath) -> Option<Self> {
        match self {
            Self::Extra(ExtraPath(root)) => {
                absolute_path.strip_prefix(root).ok().and_then(Self::extra)
            }
            Self::FirstParty(FirstPartyPath(root)) => absolute_path
                .strip_prefix(root)
                .ok()
                .and_then(Self::first_party),
            Self::StandardLibrary(StandardLibraryPath(root)) => absolute_path
                .strip_prefix(root)
                .ok()
                .and_then(Self::standard_library),
            Self::SitePackages(SitePackagesPath(root)) => absolute_path
                .strip_prefix(root)
                .ok()
                .and_then(Self::site_packages),
        }
    }
}

impl<'a> From<&'a ModuleResolutionPathBuf> for ModuleResolutionPathRef<'a> {
    #[inline]
    fn from(value: &'a ModuleResolutionPathBuf) -> Self {
        match value {
            ModuleResolutionPathBuf::Extra(ExtraPathBuf(path)) => {
                ModuleResolutionPathRef::extra_unchecked(path)
            }
            ModuleResolutionPathBuf::FirstParty(FirstPartyPathBuf(path)) => {
                ModuleResolutionPathRef::first_party_unchecked(path)
            }
            ModuleResolutionPathBuf::StandardLibrary(StandardLibraryPathBuf(path)) => {
                ModuleResolutionPathRef::standard_library_unchecked(path)
            }
            ModuleResolutionPathBuf::SitePackages(SitePackagesPathBuf(path)) => {
                ModuleResolutionPathRef::site_packages_unchecked(path)
            }
        }
    }
}

impl<'a> PartialEq<ModuleResolutionPathBuf> for ModuleResolutionPathRef<'a> {
    fn eq(&self, other: &ModuleResolutionPathBuf) -> bool {
        match (self, other) {
            (
                ModuleResolutionPathRef::Extra(ExtraPath(self_path)),
                ModuleResolutionPathBuf::Extra(ExtraPathBuf(other_path)),
            )
            | (
                ModuleResolutionPathRef::FirstParty(FirstPartyPath(self_path)),
                ModuleResolutionPathBuf::FirstParty(FirstPartyPathBuf(other_path)),
            )
            | (
                ModuleResolutionPathRef::StandardLibrary(StandardLibraryPath(self_path)),
                ModuleResolutionPathBuf::StandardLibrary(StandardLibraryPathBuf(other_path)),
            )
            | (
                ModuleResolutionPathRef::SitePackages(SitePackagesPath(self_path)),
                ModuleResolutionPathBuf::SitePackages(SitePackagesPathBuf(other_path)),
            ) => *self_path == **other_path,
            _ => false,
        }
    }
}

impl<'a> PartialEq<FileSystemPath> for ModuleResolutionPathRef<'a> {
    fn eq(&self, other: &FileSystemPath) -> bool {
        self.as_file_system_path() == other
    }
}

impl<'a> PartialEq<ModuleResolutionPathRef<'a>> for FileSystemPath {
    fn eq(&self, other: &ModuleResolutionPathRef<'a>) -> bool {
        self == other.as_file_system_path()
    }
}

pub(crate) struct ModulePartIterator<'a> {
    parent_components: Option<camino::Utf8Components<'a>>,
    stem: Option<&'a str>,
}

impl<'a> ModulePartIterator<'a> {
    #[must_use]
    fn from_fs_path(path: &'a FileSystemPath) -> Self {
        Self {
            parent_components: path.parent().map(|path| path.components()),
            stem: path.file_stem(),
        }
    }
}

impl<'a> Iterator for ModulePartIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        let ModulePartIterator {
            parent_components,
            stem,
        } = self;

        if let Some(ref mut components) = parent_components {
            components
                .next()
                .map(|component| component.as_str())
                .or_else(|| stem.take())
        } else {
            stem.take()
        }
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl<'a> DoubleEndedIterator for ModulePartIterator<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let ModulePartIterator {
            parent_components,
            stem,
        } = self;

        if let Some(part) = stem.take() {
            Some(part)
        } else if let Some(components) = parent_components {
            components.next_back().map(|component| component.as_str())
        } else {
            None
        }
    }
}

impl<'a> FusedIterator for ModulePartIterator<'a> {}
