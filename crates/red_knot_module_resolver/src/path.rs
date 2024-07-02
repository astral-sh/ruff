use std::iter::FusedIterator;

use ruff_db::file_system::{FileSystemPath, FileSystemPathBuf};
use ruff_db::vfs::VfsPath;

use crate::module_name::ModuleName;
use crate::supported_py_version::get_target_py_version;
use crate::typeshed::TypeshedVersionsQueryResult;
use crate::Db;

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
pub(crate) enum ModuleResolutionPath {
    Extra(ExtraPathBuf),
    FirstParty(FirstPartyPathBuf),
    StandardLibrary(StandardLibraryPathBuf),
    SitePackages(SitePackagesPathBuf),
}

impl ModuleResolutionPath {
    /// Push a new part to the path,
    /// while maintaining the invariant that the path can only have `.py` or `.pyi` extensions.
    /// For the stdlib variant specifically, it may only have a `.pyi` extension.
    ///
    /// ## Panics:
    /// If a component with an invalid extension is passed
    pub(crate) fn push(&mut self, component: &str) {
        debug_assert!(matches!(component.matches('.').count(), 0 | 1));
        if cfg!(debug) {
            if let Some(extension) = std::path::Path::new(component).extension() {
                match self {
                    Self::Extra(_) | Self::FirstParty(_) | Self::SitePackages(_) => assert!(
                        matches!(extension.to_str().unwrap(), "pyi" | "py"),
                        "Extension must be `py` or `pyi`; got {extension:?}"
                    ),
                    Self::StandardLibrary(_) => assert_eq!(
                        extension.to_str().unwrap(),
                        "pyi",
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
            Some(Self::extra_unchecked(path))
        } else {
            None
        }
    }

    #[must_use]
    fn extra_unchecked(path: FileSystemPathBuf) -> Self {
        Self::Extra(ExtraPathBuf(path))
    }

    #[must_use]
    pub(crate) fn first_party(path: FileSystemPathBuf) -> Option<Self> {
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
    fn first_party_unchecked(path: FileSystemPathBuf) -> Self {
        Self::FirstParty(FirstPartyPathBuf(path))
    }

    #[must_use]
    pub(crate) fn standard_library(path: FileSystemPathBuf) -> Option<Self> {
        if path.extension().map_or(true, |ext| ext == "pyi") {
            Some(Self::standard_library_unchecked(path))
        } else {
            None
        }
    }

    #[must_use]
    pub(crate) fn stdlib_from_typeshed_root(typeshed_root: &FileSystemPath) -> Option<Self> {
        Self::standard_library(typeshed_root.join(FileSystemPath::new("stdlib")))
    }

    #[must_use]
    fn standard_library_unchecked(path: FileSystemPathBuf) -> Self {
        Self::StandardLibrary(StandardLibraryPathBuf(path))
    }

    #[must_use]
    pub(crate) fn site_packages(path: FileSystemPathBuf) -> Option<Self> {
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
    fn site_packages_unchecked(path: FileSystemPathBuf) -> Self {
        Self::SitePackages(SitePackagesPathBuf(path))
    }

    #[must_use]
    pub(crate) fn is_regular_package(&self, db: &dyn Db) -> bool {
        ModuleResolutionPathRef::from(self).is_regular_package(db)
    }

    #[must_use]
    pub(crate) fn is_directory(&self, db: &dyn Db) -> bool {
        ModuleResolutionPathRef::from(self).is_directory(db)
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

    #[must_use]
    fn as_file_system_path_buf(&self) -> &FileSystemPathBuf {
        match self {
            Self::Extra(ExtraPathBuf(path)) => path,
            Self::FirstParty(FirstPartyPathBuf(path)) => path,
            Self::StandardLibrary(StandardLibraryPathBuf(path)) => path,
            Self::SitePackages(SitePackagesPathBuf(path)) => path,
        }
    }

    #[must_use]
    #[inline]
    fn into_file_system_path_buf(self) -> FileSystemPathBuf {
        match self {
            Self::Extra(ExtraPathBuf(path)) => path,
            Self::FirstParty(FirstPartyPathBuf(path)) => path,
            Self::StandardLibrary(StandardLibraryPathBuf(path)) => path,
            Self::SitePackages(SitePackagesPathBuf(path)) => path,
        }
    }
}

impl From<ModuleResolutionPath> for VfsPath {
    fn from(value: ModuleResolutionPath) -> Self {
        VfsPath::FileSystem(value.into_file_system_path_buf())
    }
}

impl PartialEq<VfsPath> for ModuleResolutionPath {
    fn eq(&self, other: &VfsPath) -> bool {
        match other {
            VfsPath::FileSystem(path) => self.as_file_system_path_buf() == path,
            VfsPath::Vendored(_) => false,
        }
    }
}

impl PartialEq<ModuleResolutionPath> for VfsPath {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<FileSystemPathBuf> for ModuleResolutionPath {
    fn eq(&self, other: &FileSystemPathBuf) -> bool {
        self.as_file_system_path_buf() == other
    }
}

impl PartialEq<ModuleResolutionPath> for FileSystemPathBuf {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<FileSystemPath> for ModuleResolutionPath {
    fn eq(&self, other: &FileSystemPath) -> bool {
        ModuleResolutionPathRef::from(self) == *other
    }
}

impl PartialEq<ModuleResolutionPath> for FileSystemPath {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        other.eq(self)
    }
}

impl AsRef<FileSystemPathBuf> for ModuleResolutionPath {
    #[inline]
    fn as_ref(&self) -> &FileSystemPathBuf {
        self.as_file_system_path_buf()
    }
}

impl AsRef<FileSystemPath> for ModuleResolutionPath {
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
    pub(crate) fn is_directory(&self, db: &dyn Db) -> bool {
        match self {
            Self::Extra(ExtraPath(path)) => db.file_system().is_directory(path),
            Self::FirstParty(FirstPartyPath(path)) => db.file_system().is_directory(path),
            Self::SitePackages(SitePackagesPath(path)) => db.file_system().is_directory(path),
            Self::StandardLibrary(StandardLibraryPath(path)) => {
                let Some(module_name) = self.as_module_name() else {
                    return false;
                };
                match db
                    .typeshed_versions()
                    .query_module(&module_name, get_target_py_version(db))
                {
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        db.file_system().is_directory(path)
                    }
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                }
            }
        }
    }

    #[must_use]
    pub(crate) fn is_regular_package(&self, db: &dyn Db) -> bool {
        match self {
            Self::Extra(ExtraPath(fs_path))
            | Self::FirstParty(FirstPartyPath(fs_path))
            | Self::SitePackages(SitePackagesPath(fs_path)) => {
                let file_system = db.file_system();
                file_system.exists(&fs_path.join("__init__.py"))
                    || file_system.exists(&fs_path.join("__init__.pyi"))
            }
            // Unlike the other variants:
            // (1) Account for VERSIONS
            // (2) Only test for `__init__.pyi`, not `__init__.py`
            Self::StandardLibrary(StandardLibraryPath(fs_path)) => {
                let Some(module_name) = self.as_module_name() else {
                    return false;
                };
                match db
                    .typeshed_versions()
                    .query_module(&module_name, get_target_py_version(db))
                {
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        db.file_system().exists(&fs_path.join("__init__.pyi"))
                    }
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                }
            }
        }
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
    fn sans_dunder_init(self) -> Self {
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
        let mut parts_iter = match self.sans_dunder_init() {
            Self::Extra(ExtraPath(path)) => ModulePartIterator::from_fs_path(path),
            Self::FirstParty(FirstPartyPath(path)) => ModulePartIterator::from_fs_path(path),
            Self::StandardLibrary(StandardLibraryPath(path)) => {
                ModulePartIterator::from_fs_path(path)
            }
            Self::SitePackages(SitePackagesPath(path)) => ModulePartIterator::from_fs_path(path),
        };
        let first_part = parts_iter.next()?;
        if let Some(second_part) = parts_iter.next() {
            let mut name = format!("{first_part}.{second_part}");
            for part in parts_iter {
                name.push('.');
                name.push_str(part);
            }
            ModuleName::new(&name)
        } else {
            ModuleName::new(first_part)
        }
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> ModuleResolutionPath {
        match self {
            Self::Extra(ExtraPath(path)) => {
                ModuleResolutionPath::extra_unchecked(path.with_extension("pyi"))
            }
            Self::FirstParty(FirstPartyPath(path)) => {
                ModuleResolutionPath::first_party_unchecked(path.with_extension("pyi"))
            }
            Self::StandardLibrary(StandardLibraryPath(path)) => {
                ModuleResolutionPath::standard_library_unchecked(path.with_extension("pyi"))
            }
            Self::SitePackages(SitePackagesPath(path)) => {
                ModuleResolutionPath::site_packages_unchecked(path.with_extension("pyi"))
            }
        }
    }

    #[must_use]
    pub(crate) fn with_py_extension(&self) -> Option<ModuleResolutionPath> {
        match self {
            Self::Extra(ExtraPath(path)) => Some(ModuleResolutionPath::extra_unchecked(
                path.with_extension("py"),
            )),
            Self::FirstParty(FirstPartyPath(path)) => Some(
                ModuleResolutionPath::first_party_unchecked(path.with_extension("py")),
            ),
            Self::StandardLibrary(_) => None,
            Self::SitePackages(SitePackagesPath(path)) => Some(
                ModuleResolutionPath::site_packages_unchecked(path.with_extension("py")),
            ),
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn to_module_resolution_path(self) -> ModuleResolutionPath {
        match self {
            Self::Extra(ExtraPath(path)) => {
                ModuleResolutionPath::extra_unchecked(path.to_path_buf())
            }
            Self::FirstParty(FirstPartyPath(path)) => {
                ModuleResolutionPath::first_party_unchecked(path.to_path_buf())
            }
            Self::StandardLibrary(StandardLibraryPath(path)) => {
                ModuleResolutionPath::standard_library_unchecked(path.to_path_buf())
            }
            Self::SitePackages(SitePackagesPath(path)) => {
                ModuleResolutionPath::site_packages_unchecked(path.to_path_buf())
            }
        }
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn join(
        &self,
        component: &'a (impl AsRef<FileSystemPath> + ?Sized),
    ) -> ModuleResolutionPath {
        let mut result = self.to_module_resolution_path();
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
}

impl<'a> From<&'a ModuleResolutionPath> for ModuleResolutionPathRef<'a> {
    #[inline]
    fn from(value: &'a ModuleResolutionPath) -> Self {
        match value {
            ModuleResolutionPath::Extra(ExtraPathBuf(path)) => {
                ModuleResolutionPathRef::extra_unchecked(path)
            }
            ModuleResolutionPath::FirstParty(FirstPartyPathBuf(path)) => {
                ModuleResolutionPathRef::first_party_unchecked(path)
            }
            ModuleResolutionPath::StandardLibrary(StandardLibraryPathBuf(path)) => {
                ModuleResolutionPathRef::standard_library_unchecked(path)
            }
            ModuleResolutionPath::SitePackages(SitePackagesPathBuf(path)) => {
                ModuleResolutionPathRef::site_packages_unchecked(path)
            }
        }
    }
}

impl<'a> AsRef<FileSystemPath> for ModuleResolutionPathRef<'a> {
    #[inline]
    fn as_ref(&self) -> &FileSystemPath {
        self.as_file_system_path()
    }
}

impl<'a> PartialEq<ModuleResolutionPath> for ModuleResolutionPathRef<'a> {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        match (self, other) {
            (
                ModuleResolutionPathRef::Extra(ExtraPath(self_path)),
                ModuleResolutionPath::Extra(ExtraPathBuf(other_path)),
            )
            | (
                ModuleResolutionPathRef::FirstParty(FirstPartyPath(self_path)),
                ModuleResolutionPath::FirstParty(FirstPartyPathBuf(other_path)),
            )
            | (
                ModuleResolutionPathRef::StandardLibrary(StandardLibraryPath(self_path)),
                ModuleResolutionPath::StandardLibrary(StandardLibraryPathBuf(other_path)),
            )
            | (
                ModuleResolutionPathRef::SitePackages(SitePackagesPath(self_path)),
                ModuleResolutionPath::SitePackages(SitePackagesPathBuf(other_path)),
            ) => *self_path == **other_path,
            _ => false,
        }
    }
}

impl<'a> PartialEq<ModuleResolutionPathRef<'a>> for ModuleResolutionPath {
    fn eq(&self, other: &ModuleResolutionPathRef<'a>) -> bool {
        other.eq(self)
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

impl<'a> PartialEq<VfsPath> for ModuleResolutionPathRef<'a> {
    fn eq(&self, other: &VfsPath) -> bool {
        let VfsPath::FileSystem(other) = other else {
            return false;
        };
        self.as_file_system_path() == &**other
    }
}

impl<'a> PartialEq<ModuleResolutionPathRef<'a>> for VfsPath {
    fn eq(&self, other: &ModuleResolutionPathRef<'a>) -> bool {
        other.eq(self)
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
