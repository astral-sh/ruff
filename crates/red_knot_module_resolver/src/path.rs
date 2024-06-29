#![allow(unsafe_code)]
use std::iter::FusedIterator;
use std::ops::Deref;
use std::path;

use ruff_db::file_system::{FileSystem, FileSystemPath, FileSystemPathBuf};
use ruff_db::vfs::VfsPath;

use crate::Db;

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ExtraPath(FileSystemPath);

impl ExtraPath {
    #[must_use]
    pub fn new(path: &(impl AsRef<FileSystemPath> + ?Sized)) -> Option<&Self> {
        let path = path.as_ref();
        if path
            .extension()
            .is_some_and(|extension| !matches!(extension, "pyi" | "py"))
        {
            return None;
        }
        Some(Self::new_unchecked(path))
    }

    #[must_use]
    fn new_unchecked(path: &FileSystemPath) -> &Self {
        // SAFETY: ExtraPath is marked as #[repr(transparent)] so the conversion from a
        // *const FileSystemPath to a *const ExtraPath is valid.
        unsafe { &*(path as *const FileSystemPath as *const ExtraPath) }
    }

    #[must_use]
    pub(crate) fn parent(&self) -> Option<&Self> {
        Some(Self::new_unchecked(self.0.parent()?))
    }

    #[must_use]
    pub(crate) fn sans_dunder_init(&self) -> &Self {
        if self.0.ends_with("__init__.py") || self.0.ends_with("__init__.pyi") {
            self.parent()
                .unwrap_or_else(|| Self::new_unchecked(FileSystemPath::new("")))
        } else {
            self
        }
    }

    #[must_use]
    pub(crate) fn module_name_parts(&self) -> ModulePartIterator {
        ModulePartIterator::from_fs_path(&self.0)
    }

    #[must_use]
    pub(crate) fn relative_to_search_path(&self, search_path: &ExtraPath) -> Option<&Self> {
        self.0
            .strip_prefix(search_path)
            .map(Self::new_unchecked)
            .ok()
    }

    #[must_use]
    pub fn to_path_buf(&self) -> ExtraPathBuf {
        ExtraPathBuf(self.0.to_path_buf())
    }

    #[must_use]
    fn is_regular_package(&self, file_system: &dyn FileSystem) -> bool {
        file_system.exists(&self.0.join("__init__.py"))
            || file_system.exists(&self.0.join("__init__.pyi"))
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> ExtraPathBuf {
        ExtraPathBuf(self.0.with_extension("pyi"))
    }

    #[must_use]
    pub(crate) fn with_py_extension(&self) -> ExtraPathBuf {
        ExtraPathBuf(self.0.with_extension("py"))
    }

    #[must_use]
    #[inline]
    pub(crate) fn as_file_system_path(&self) -> &FileSystemPath {
        &self.0
    }
}

impl PartialEq<FileSystemPath> for ExtraPath {
    fn eq(&self, other: &FileSystemPath) -> bool {
        self.0 == *other
    }
}

impl PartialEq<VfsPath> for ExtraPath {
    fn eq(&self, other: &VfsPath) -> bool {
        match other {
            VfsPath::FileSystem(path) => **path == self.0,
            VfsPath::Vendored(_) => false,
        }
    }
}

impl AsRef<ExtraPath> for ExtraPath {
    #[inline]
    fn as_ref(&self) -> &ExtraPath {
        self
    }
}

impl AsRef<FileSystemPath> for ExtraPath {
    #[inline]
    fn as_ref(&self) -> &FileSystemPath {
        self.as_file_system_path()
    }
}

impl AsRef<path::Path> for ExtraPath {
    #[inline]
    fn as_ref(&self) -> &path::Path {
        self.0.as_ref()
    }
}

impl AsRef<str> for ExtraPath {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct ExtraPathBuf(FileSystemPathBuf);

impl ExtraPathBuf {
    #[must_use]
    #[inline]
    fn as_path(&self) -> &ExtraPath {
        ExtraPath::new(&self.0).unwrap()
    }

    /// Push a new part to the path,
    /// while maintaining the invariant that the path can only have `.py` or `.pyi` extensions
    ///
    /// ## Panics:
    /// If a component with an invalid extension is passed
    fn push(&mut self, component: &str) {
        debug_assert!(matches!(component.matches('.').count(), 0 | 1));
        if cfg!(debug) {
            if let Some(extension) = std::path::Path::new(component).extension() {
                assert!(
                    matches!(extension.to_str().unwrap(), "pyi" | "py"),
                    "Extension must be `py` or `pyi`; got {extension:?}"
                );
            }
        }
        self.0.push(component);
    }

    #[inline]
    pub(crate) fn as_file_system_path_buf(&self) -> &FileSystemPathBuf {
        &self.0
    }
}

impl AsRef<FileSystemPathBuf> for ExtraPathBuf {
    #[inline]
    fn as_ref(&self) -> &FileSystemPathBuf {
        self.as_file_system_path_buf()
    }
}

impl AsRef<ExtraPath> for ExtraPathBuf {
    #[inline]
    fn as_ref(&self) -> &ExtraPath {
        self.as_path()
    }
}

impl Deref for ExtraPathBuf {
    type Target = ExtraPath;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_path()
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct FirstPartyPath(FileSystemPath);

impl FirstPartyPath {
    #[must_use]
    pub fn new(path: &(impl AsRef<FileSystemPath> + ?Sized)) -> Option<&Self> {
        let path = path.as_ref();
        if path
            .extension()
            .is_some_and(|extension| !matches!(extension, "pyi" | "py"))
        {
            return None;
        }
        Some(Self::new_unchecked(path))
    }

    #[must_use]
    fn new_unchecked(path: &FileSystemPath) -> &Self {
        // SAFETY: FirstPartyPath is marked as #[repr(transparent)] so the conversion from a
        // *const FileSystemPath to a *const FirstPartyPath is valid.
        unsafe { &*(path as *const FileSystemPath as *const FirstPartyPath) }
    }

    #[must_use]
    pub(crate) fn parent(&self) -> Option<&Self> {
        Some(Self::new_unchecked(self.0.parent()?))
    }

    #[must_use]
    pub(crate) fn sans_dunder_init(&self) -> &Self {
        if self.0.ends_with("__init__.py") || self.0.ends_with("__init__.pyi") {
            self.parent()
                .unwrap_or_else(|| Self::new_unchecked(FileSystemPath::new("")))
        } else {
            self
        }
    }

    #[must_use]
    pub(crate) fn module_name_parts(&self) -> ModulePartIterator {
        ModulePartIterator::from_fs_path(&self.0)
    }

    #[must_use]
    pub(crate) fn relative_to_search_path(&self, search_path: &FirstPartyPath) -> Option<&Self> {
        self.0
            .strip_prefix(search_path)
            .map(Self::new_unchecked)
            .ok()
    }

    #[must_use]
    pub fn to_path_buf(&self) -> FirstPartyPathBuf {
        FirstPartyPathBuf(self.0.to_path_buf())
    }

    #[must_use]
    fn is_regular_package(&self, file_system: &dyn FileSystem) -> bool {
        file_system.exists(&self.0.join("__init__.py"))
            || file_system.exists(&self.0.join("__init__.pyi"))
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> FirstPartyPathBuf {
        FirstPartyPathBuf(self.0.with_extension("pyi"))
    }

    #[must_use]
    pub(crate) fn with_py_extension(&self) -> FirstPartyPathBuf {
        FirstPartyPathBuf(self.0.with_extension("py"))
    }

    #[must_use]
    #[inline]
    pub(crate) fn as_file_system_path(&self) -> &FileSystemPath {
        &self.0
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn join(&self, path: &str) -> FirstPartyPathBuf {
        let mut result = self.to_path_buf();
        result.push(path);
        result
    }
}

impl PartialEq<FileSystemPath> for FirstPartyPath {
    fn eq(&self, other: &FileSystemPath) -> bool {
        self.0 == *other
    }
}

impl PartialEq<VfsPath> for FirstPartyPath {
    fn eq(&self, other: &VfsPath) -> bool {
        match other {
            VfsPath::FileSystem(path) => **path == self.0,
            VfsPath::Vendored(_) => false,
        }
    }
}

impl AsRef<FirstPartyPath> for FirstPartyPath {
    #[inline]
    fn as_ref(&self) -> &FirstPartyPath {
        self
    }
}

impl AsRef<FileSystemPath> for FirstPartyPath {
    #[inline]
    fn as_ref(&self) -> &FileSystemPath {
        self.as_file_system_path()
    }
}

impl AsRef<path::Path> for FirstPartyPath {
    #[inline]
    fn as_ref(&self) -> &path::Path {
        self.0.as_ref()
    }
}

impl AsRef<str> for FirstPartyPath {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct FirstPartyPathBuf(FileSystemPathBuf);

impl FirstPartyPathBuf {
    #[must_use]
    #[inline]
    fn as_path(&self) -> &FirstPartyPath {
        FirstPartyPath::new(&self.0).unwrap()
    }

    /// Push a new part to the path,
    /// while maintaining the invariant that the path can only have `.py` or `.pyi` extensions
    ///
    /// ## Panics:
    /// If a component with an invalid extension is passed
    fn push(&mut self, component: &str) {
        debug_assert!(matches!(component.matches('.').count(), 0 | 1));
        if cfg!(debug) {
            if let Some(extension) = std::path::Path::new(component).extension() {
                assert!(
                    matches!(extension.to_str().unwrap(), "pyi" | "py"),
                    "Extension must be `py` or `pyi`; got {extension:?}"
                );
            }
        }
        self.0.push(component);
    }

    #[cfg(test)]
    pub(crate) fn into_vfs_path(self) -> VfsPath {
        VfsPath::FileSystem(self.0)
    }

    #[inline]
    pub(crate) fn as_file_system_path_buf(&self) -> &FileSystemPathBuf {
        &self.0
    }
}

impl AsRef<FileSystemPathBuf> for FirstPartyPathBuf {
    #[inline]
    fn as_ref(&self) -> &FileSystemPathBuf {
        self.as_file_system_path_buf()
    }
}

impl AsRef<FirstPartyPath> for FirstPartyPathBuf {
    #[inline]
    fn as_ref(&self) -> &FirstPartyPath {
        self.as_path()
    }
}

impl Deref for FirstPartyPathBuf {
    type Target = FirstPartyPath;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_path()
    }
}

// TODO(Alex): Standard-library paths could be vendored paths
#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct StandardLibraryPath(FileSystemPath);

impl StandardLibraryPath {
    #[must_use]
    pub fn new(path: &(impl AsRef<FileSystemPath> + ?Sized)) -> Option<&Self> {
        let path = path.as_ref();
        // Only allow pyi extensions, unlike other paths
        if path.extension().is_some_and(|extension| extension != "pyi") {
            return None;
        }
        Some(Self::new_unchecked(path))
    }

    #[must_use]
    fn new_unchecked(path: &(impl AsRef<FileSystemPath> + ?Sized)) -> &Self {
        // SAFETY: FirstPartyPath is marked as #[repr(transparent)] so the conversion from a
        // *const FileSystemPath to a *const FirstPartyPath is valid.
        let path = path.as_ref();
        unsafe { &*(path as *const FileSystemPath as *const StandardLibraryPath) }
    }

    #[must_use]
    #[inline]
    pub(crate) fn stdlib_dir() -> &'static Self {
        Self::new_unchecked("stdlib")
    }

    pub(crate) fn stdlib_from_typeshed_root(
        typeshed: &StandardLibraryPath,
    ) -> StandardLibraryPathBuf {
        StandardLibraryPathBuf(typeshed.0.join(Self::stdlib_dir()))
    }

    #[must_use]
    pub(crate) fn relative_to_search_path(
        &self,
        search_path: &StandardLibraryPath,
    ) -> Option<&Self> {
        self.0
            .strip_prefix(search_path)
            .map(Self::new_unchecked)
            .ok()
    }

    #[must_use]
    pub(crate) fn parent(&self) -> Option<&Self> {
        Some(Self::new_unchecked(self.0.parent()?))
    }

    #[must_use]
    pub(crate) fn sans_dunder_init(&self) -> &Self {
        // Only try to strip `__init__.pyi` from the end, unlike other paths
        if self.0.ends_with("__init__.pyi") {
            self.parent()
                .unwrap_or_else(|| Self::new_unchecked(FileSystemPath::new("")))
        } else {
            self
        }
    }

    #[must_use]
    pub(crate) fn module_name_parts(&self) -> ModulePartIterator {
        ModulePartIterator::from_fs_path(&self.0)
    }

    #[must_use]
    pub fn to_path_buf(&self) -> StandardLibraryPathBuf {
        StandardLibraryPathBuf(self.0.to_path_buf())
    }

    #[must_use]
    fn is_regular_package(&self, db: &dyn Db) -> bool {
        todo!()
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> StandardLibraryPathBuf {
        StandardLibraryPathBuf(self.0.with_extension("pyi"))
    }

    #[must_use]
    #[inline]
    pub(crate) fn as_file_system_path(&self) -> &FileSystemPath {
        &self.0
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn join(&self, path: &str) -> StandardLibraryPathBuf {
        let mut result = self.to_path_buf();
        result.push(path);
        result
    }
}

impl PartialEq<FileSystemPath> for StandardLibraryPath {
    fn eq(&self, other: &FileSystemPath) -> bool {
        self.0 == *other
    }
}

impl PartialEq<VfsPath> for StandardLibraryPath {
    fn eq(&self, other: &VfsPath) -> bool {
        match other {
            VfsPath::FileSystem(path) => **path == self.0,
            VfsPath::Vendored(_) => false,
        }
    }
}

impl AsRef<StandardLibraryPath> for StandardLibraryPath {
    fn as_ref(&self) -> &StandardLibraryPath {
        self
    }
}

impl AsRef<FileSystemPath> for StandardLibraryPath {
    #[inline]
    fn as_ref(&self) -> &FileSystemPath {
        self.as_file_system_path()
    }
}

impl AsRef<path::Path> for StandardLibraryPath {
    #[inline]
    fn as_ref(&self) -> &path::Path {
        self.0.as_ref()
    }
}

impl AsRef<str> for StandardLibraryPath {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

// TODO(Alex): Standard-library paths could also be vendored paths
#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct StandardLibraryPathBuf(FileSystemPathBuf);

impl StandardLibraryPathBuf {
    #[must_use]
    #[inline]
    fn as_path(&self) -> &StandardLibraryPath {
        StandardLibraryPath::new(&self.0).unwrap()
    }

    /// Push a new part to the path,
    /// while maintaining the invariant that the path can only have `.pyi` extensions
    ///
    /// ## Panics:
    /// If a component with an invalid extension is passed
    fn push(&mut self, component: &str) {
        debug_assert!(matches!(component.matches('.').count(), 0 | 1));
        if cfg!(debug) {
            if let Some(extension) = std::path::Path::new(component).extension() {
                assert_eq!(
                    extension.to_str().unwrap(),
                    "pyi",
                    "Extension must be `pyi`; got {extension:?}"
                );
            }
        }
        self.0.push(component);
    }

    #[cfg(test)]
    pub(crate) fn into_vfs_path(self) -> VfsPath {
        VfsPath::FileSystem(self.0)
    }

    #[inline]
    pub(crate) fn as_file_system_path_buf(&self) -> &FileSystemPathBuf {
        &self.0
    }
}

impl AsRef<StandardLibraryPath> for StandardLibraryPathBuf {
    #[inline]
    fn as_ref(&self) -> &StandardLibraryPath {
        self.as_path()
    }
}

impl AsRef<FileSystemPathBuf> for StandardLibraryPathBuf {
    #[inline]
    fn as_ref(&self) -> &FileSystemPathBuf {
        self.as_file_system_path_buf()
    }
}

impl Deref for StandardLibraryPathBuf {
    type Target = StandardLibraryPath;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_path()
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct SitePackagesPath(FileSystemPath);

impl SitePackagesPath {
    #[must_use]
    pub fn new(path: &(impl AsRef<FileSystemPath> + ?Sized)) -> Option<&Self> {
        let path = path.as_ref();
        if path
            .extension()
            .is_some_and(|extension| !matches!(extension, "pyi" | "py"))
        {
            return None;
        }
        Some(Self::new_unchecked(path))
    }

    #[must_use]
    fn new_unchecked(path: &FileSystemPath) -> &Self {
        // SAFETY: SitePackagesPath is marked as #[repr(transparent)] so the conversion from a
        // *const FileSystemPath to a *const SitePackagesPath is valid.
        unsafe { &*(path as *const FileSystemPath as *const SitePackagesPath) }
    }

    #[must_use]
    pub(crate) fn parent(&self) -> Option<&Self> {
        Some(Self::new_unchecked(self.0.parent()?))
    }

    #[must_use]
    pub(crate) fn sans_dunder_init(&self) -> &Self {
        // Only try to strip `__init__.pyi` from the end, unlike other paths
        if self.0.ends_with("__init__.pyi") || self.0.ends_with("__init__.py") {
            self.parent()
                .unwrap_or_else(|| Self::new_unchecked(FileSystemPath::new("")))
        } else {
            self
        }
    }

    #[must_use]
    pub(crate) fn module_name_parts(&self) -> ModulePartIterator {
        ModulePartIterator::from_fs_path(&self.0)
    }

    #[must_use]
    pub(crate) fn relative_to_search_path(&self, search_path: &SitePackagesPath) -> Option<&Self> {
        self.0
            .strip_prefix(search_path)
            .map(Self::new_unchecked)
            .ok()
    }

    #[must_use]
    pub fn to_path_buf(&self) -> SitePackagesPathBuf {
        SitePackagesPathBuf(self.0.to_path_buf())
    }

    #[must_use]
    fn is_regular_package(&self, file_system: &dyn FileSystem) -> bool {
        file_system.exists(&self.0.join("__init__.py"))
            || file_system.exists(&self.0.join("__init__.pyi"))
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> SitePackagesPathBuf {
        SitePackagesPathBuf(self.0.with_extension("pyi"))
    }

    #[must_use]
    pub(crate) fn with_py_extension(&self) -> SitePackagesPathBuf {
        SitePackagesPathBuf(self.0.with_extension("py"))
    }

    #[must_use]
    #[inline]
    pub(crate) fn as_file_system_path(&self) -> &FileSystemPath {
        &self.0
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) fn join(&self, path: &str) -> SitePackagesPathBuf {
        let mut result = self.to_path_buf();
        result.push(path);
        result
    }
}

impl PartialEq<FileSystemPath> for SitePackagesPath {
    fn eq(&self, other: &FileSystemPath) -> bool {
        self.0 == *other
    }
}

impl PartialEq<VfsPath> for SitePackagesPath {
    fn eq(&self, other: &VfsPath) -> bool {
        match other {
            VfsPath::FileSystem(path) => **path == self.0,
            VfsPath::Vendored(_) => false,
        }
    }
}

impl AsRef<SitePackagesPath> for SitePackagesPath {
    fn as_ref(&self) -> &SitePackagesPath {
        self
    }
}

impl AsRef<FileSystemPath> for SitePackagesPath {
    #[inline]
    fn as_ref(&self) -> &FileSystemPath {
        self.as_file_system_path()
    }
}

impl AsRef<path::Path> for SitePackagesPath {
    #[inline]
    fn as_ref(&self) -> &path::Path {
        self.0.as_ref()
    }
}

impl AsRef<str> for SitePackagesPath {
    #[inline]
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct SitePackagesPathBuf(FileSystemPathBuf);

impl SitePackagesPathBuf {
    #[must_use]
    #[inline]
    fn as_path(&self) -> &SitePackagesPath {
        SitePackagesPath::new(&self.0).unwrap()
    }

    /// Push a new part to the path,
    /// while maintaining the invariant that the path can only have `.py` or `.pyi` extensions
    ///
    /// ## Panics:
    /// If a component with an invalid extension is passed
    fn push(&mut self, component: &str) {
        debug_assert!(matches!(component.matches('.').count(), 0 | 1));
        if cfg!(debug) {
            if let Some(extension) = std::path::Path::new(component).extension() {
                assert!(
                    matches!(extension.to_str().unwrap(), "pyi" | "py"),
                    "Extension must be `py` or `pyi`; got {extension:?}"
                );
            }
        }
        self.0.push(component);
    }

    #[cfg(test)]
    pub(crate) fn into_vfs_path(self) -> VfsPath {
        VfsPath::FileSystem(self.0)
    }

    #[inline]
    pub(crate) fn as_file_system_path_buf(&self) -> &FileSystemPathBuf {
        &self.0
    }
}

impl AsRef<FileSystemPathBuf> for SitePackagesPathBuf {
    #[inline]
    fn as_ref(&self) -> &FileSystemPathBuf {
        self.as_file_system_path_buf()
    }
}

impl AsRef<SitePackagesPath> for SitePackagesPathBuf {
    #[inline]
    fn as_ref(&self) -> &SitePackagesPath {
        self.as_path()
    }
}

impl Deref for SitePackagesPathBuf {
    type Target = SitePackagesPath;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_path()
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub(crate) enum ModuleResolutionPath {
    Extra(ExtraPathBuf),
    FirstParty(FirstPartyPathBuf),
    StandardLibrary(StandardLibraryPathBuf),
    SitePackages(SitePackagesPathBuf),
}

impl ModuleResolutionPath {
    pub(crate) fn push(&mut self, component: &str) {
        match self {
            Self::Extra(ref mut path) => path.push(component),
            Self::FirstParty(ref mut path) => path.push(component),
            Self::StandardLibrary(ref mut path) => path.push(component),
            Self::SitePackages(ref mut path) => path.push(component),
        }
    }

    pub(crate) fn is_regular_package(&self, db: &dyn Db) -> bool {
        ModuleResolutionPathRef::from(self).is_regular_package(db)
    }

    pub(crate) fn is_directory(&self, db: &dyn Db) -> bool {
        ModuleResolutionPathRef::from(self).is_regular_package(db)
    }

    pub(crate) fn with_pyi_extension(&self) -> Self {
        ModuleResolutionPathRef::from(self).with_pyi_extension()
    }

    pub(crate) fn with_py_extension(&self) -> Option<Self> {
        ModuleResolutionPathRef::from(self).with_py_extension()
    }
}

impl AsRef<FileSystemPath> for ModuleResolutionPath {
    fn as_ref(&self) -> &FileSystemPath {
        match self {
            Self::Extra(path) => path.as_file_system_path(),
            Self::FirstParty(path) => path.as_file_system_path(),
            Self::StandardLibrary(path) => path.as_file_system_path(),
            Self::SitePackages(path) => path.as_file_system_path(),
        }
    }
}

impl AsRef<FileSystemPathBuf> for ModuleResolutionPath {
    fn as_ref(&self) -> &FileSystemPathBuf {
        match self {
            Self::Extra(path) => path.as_file_system_path_buf(),
            Self::FirstParty(path) => path.as_file_system_path_buf(),
            Self::StandardLibrary(path) => path.as_file_system_path_buf(),
            Self::SitePackages(path) => path.as_file_system_path_buf(),
        }
    }
}

impl PartialEq<ExtraPath> for ModuleResolutionPath {
    fn eq(&self, other: &ExtraPath) -> bool {
        if let ModuleResolutionPath::Extra(path) = self {
            **path == *other
        } else {
            false
        }
    }
}

impl PartialEq<ModuleResolutionPath> for ExtraPath {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<ExtraPathBuf> for ModuleResolutionPath {
    fn eq(&self, other: &ExtraPathBuf) -> bool {
        self.eq(&**other)
    }
}

impl PartialEq<ModuleResolutionPath> for ExtraPathBuf {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<FirstPartyPath> for ModuleResolutionPath {
    fn eq(&self, other: &FirstPartyPath) -> bool {
        if let ModuleResolutionPath::FirstParty(path) = self {
            **path == *other
        } else {
            false
        }
    }
}

impl PartialEq<ModuleResolutionPath> for FirstPartyPath {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<FirstPartyPathBuf> for ModuleResolutionPath {
    fn eq(&self, other: &FirstPartyPathBuf) -> bool {
        self.eq(&**other)
    }
}

impl PartialEq<ModuleResolutionPath> for FirstPartyPathBuf {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<StandardLibraryPath> for ModuleResolutionPath {
    fn eq(&self, other: &StandardLibraryPath) -> bool {
        if let ModuleResolutionPath::StandardLibrary(path) = self {
            **path == *other
        } else {
            false
        }
    }
}

impl PartialEq<ModuleResolutionPath> for StandardLibraryPath {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<StandardLibraryPathBuf> for ModuleResolutionPath {
    fn eq(&self, other: &StandardLibraryPathBuf) -> bool {
        self.eq(&**other)
    }
}

impl PartialEq<ModuleResolutionPath> for StandardLibraryPathBuf {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<SitePackagesPath> for ModuleResolutionPath {
    fn eq(&self, other: &SitePackagesPath) -> bool {
        if let ModuleResolutionPath::SitePackages(path) = self {
            **path == *other
        } else {
            false
        }
    }
}

impl PartialEq<ModuleResolutionPath> for SitePackagesPath {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<SitePackagesPathBuf> for ModuleResolutionPath {
    fn eq(&self, other: &SitePackagesPathBuf) -> bool {
        self.eq(&**other)
    }
}

impl PartialEq<ModuleResolutionPath> for SitePackagesPathBuf {
    fn eq(&self, other: &ModuleResolutionPath) -> bool {
        other.eq(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ModuleResolutionPathRef<'a> {
    Extra(&'a ExtraPath),
    FirstParty(&'a FirstPartyPath),
    StandardLibrary(&'a StandardLibraryPath),
    SitePackages(&'a SitePackagesPath),
}

impl<'a> ModuleResolutionPathRef<'a> {
    #[must_use]
    pub(crate) fn sans_dunder_init(self) -> Self {
        match self {
            Self::Extra(path) => Self::Extra(path.sans_dunder_init()),
            Self::FirstParty(path) => Self::FirstParty(path.sans_dunder_init()),
            Self::StandardLibrary(path) => Self::StandardLibrary(path.sans_dunder_init()),
            Self::SitePackages(path) => Self::SitePackages(path.sans_dunder_init()),
        }
    }

    #[must_use]
    pub(crate) fn module_name_parts(self) -> ModulePartIterator<'a> {
        match self {
            Self::Extra(path) => path.module_name_parts(),
            Self::FirstParty(path) => path.module_name_parts(),
            Self::StandardLibrary(path) => path.module_name_parts(),
            Self::SitePackages(path) => path.module_name_parts(),
        }
    }

    #[must_use]
    pub(crate) fn to_owned(self) -> ModuleResolutionPath {
        match self {
            Self::Extra(path) => ModuleResolutionPath::Extra(path.to_path_buf()),
            Self::FirstParty(path) => ModuleResolutionPath::FirstParty(path.to_path_buf()),
            Self::StandardLibrary(path) => {
                ModuleResolutionPath::StandardLibrary(path.to_path_buf())
            }
            Self::SitePackages(path) => ModuleResolutionPath::SitePackages(path.to_path_buf()),
        }
    }

    #[must_use]
    pub(crate) fn is_regular_package(self, db: &dyn Db) -> bool {
        match self {
            Self::Extra(path) => path.is_regular_package(db.file_system()),
            Self::FirstParty(path) => path.is_regular_package(db.file_system()),
            Self::StandardLibrary(path) => path.is_regular_package(db),
            Self::SitePackages(path) => path.is_regular_package(db.file_system()),
        }
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(self) -> ModuleResolutionPath {
        match self {
            Self::Extra(path) => ModuleResolutionPath::Extra(path.with_pyi_extension()),
            Self::FirstParty(path) => ModuleResolutionPath::FirstParty(path.with_pyi_extension()),
            Self::StandardLibrary(path) => {
                ModuleResolutionPath::StandardLibrary(path.with_pyi_extension())
            }
            Self::SitePackages(path) => {
                ModuleResolutionPath::SitePackages(path.with_pyi_extension())
            }
        }
    }

    #[must_use]
    pub(crate) fn with_py_extension(self) -> Option<ModuleResolutionPath> {
        match self {
            Self::Extra(path) => Some(ModuleResolutionPath::Extra(path.with_py_extension())),
            Self::FirstParty(path) => {
                Some(ModuleResolutionPath::FirstParty(path.with_py_extension()))
            }
            Self::StandardLibrary(_) => None,
            Self::SitePackages(path) => {
                Some(ModuleResolutionPath::SitePackages(path.with_py_extension()))
            }
        }
    }
}

impl<'a> From<&'a ModuleResolutionPath> for ModuleResolutionPathRef<'a> {
    #[inline]
    fn from(value: &'a ModuleResolutionPath) -> Self {
        match value {
            ModuleResolutionPath::Extra(path) => ModuleResolutionPathRef::Extra(path),
            ModuleResolutionPath::FirstParty(path) => ModuleResolutionPathRef::FirstParty(path),
            ModuleResolutionPath::StandardLibrary(path) => {
                ModuleResolutionPathRef::StandardLibrary(path)
            }
            ModuleResolutionPath::SitePackages(path) => ModuleResolutionPathRef::SitePackages(path),
        }
    }
}

impl<'a> AsRef<FileSystemPath> for ModuleResolutionPathRef<'a> {
    fn as_ref(&self) -> &FileSystemPath {
        match self {
            Self::Extra(path) => path.as_file_system_path(),
            Self::FirstParty(path) => path.as_file_system_path(),
            Self::StandardLibrary(path) => path.as_file_system_path(),
            Self::SitePackages(path) => path.as_file_system_path(),
        }
    }
}

impl<'a> PartialEq<ExtraPath> for ModuleResolutionPathRef<'a> {
    fn eq(&self, other: &ExtraPath) -> bool {
        if let ModuleResolutionPathRef::Extra(path) = self {
            *path == other
        } else {
            false
        }
    }
}

impl<'a> PartialEq<ModuleResolutionPathRef<'a>> for ExtraPath {
    fn eq(&self, other: &ModuleResolutionPathRef) -> bool {
        other.eq(self)
    }
}

impl<'a> PartialEq<FirstPartyPath> for ModuleResolutionPathRef<'a> {
    fn eq(&self, other: &FirstPartyPath) -> bool {
        if let ModuleResolutionPathRef::FirstParty(path) = self {
            *path == other
        } else {
            false
        }
    }
}

impl<'a> PartialEq<ModuleResolutionPathRef<'a>> for FirstPartyPath {
    fn eq(&self, other: &ModuleResolutionPathRef) -> bool {
        other.eq(self)
    }
}

impl<'a> PartialEq<StandardLibraryPath> for ModuleResolutionPathRef<'a> {
    fn eq(&self, other: &StandardLibraryPath) -> bool {
        if let ModuleResolutionPathRef::StandardLibrary(path) = self {
            *path == other
        } else {
            false
        }
    }
}

impl<'a> PartialEq<ModuleResolutionPathRef<'a>> for StandardLibraryPath {
    fn eq(&self, other: &ModuleResolutionPathRef) -> bool {
        other.eq(self)
    }
}

impl<'a> PartialEq<SitePackagesPath> for ModuleResolutionPathRef<'a> {
    fn eq(&self, other: &SitePackagesPath) -> bool {
        if let ModuleResolutionPathRef::SitePackages(path) = self {
            *path == other
        } else {
            false
        }
    }
}

impl<'a> PartialEq<ModuleResolutionPathRef<'a>> for SitePackagesPath {
    fn eq(&self, other: &ModuleResolutionPathRef) -> bool {
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
