use std::iter::FusedIterator;

use ruff_db::file_system::{FileSystemPath, FileSystemPathBuf};
use ruff_db::vfs::{system_path_to_file, VfsPath};

use crate::db::Db;
use crate::module_name::ModuleName;
use crate::supported_py_version::get_target_py_version;
use crate::typeshed::{parse_typeshed_versions, TypeshedVersions, TypeshedVersionsQueryResult};

/// Enumeration of the different kinds of search paths type checkers are expected to support.
///
/// N.B. Although we don't implement `Ord` for this enum, they are ordered in terms of the
/// priority that we want to give these modules when resolving them,
/// as per [the order given in the typing spec]
///
/// [the order given in the typing spec]: https://typing.readthedocs.io/en/latest/spec/distributing.html#import-resolution-ordering
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ModuleResolutionPathBufInner {
    Extra(FileSystemPathBuf),
    FirstParty(FileSystemPathBuf),
    StandardLibrary(FileSystemPathBuf),
    SitePackages(FileSystemPathBuf),
}

impl ModuleResolutionPathBufInner {
    fn push(&mut self, component: &str) {
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
            Self::Extra(ref mut path) => path,
            Self::FirstParty(ref mut path) => path,
            Self::StandardLibrary(ref mut path) => path,
            Self::SitePackages(ref mut path) => path,
        };
        inner.push(component);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ModuleResolutionPathBuf(ModuleResolutionPathBufInner);

impl ModuleResolutionPathBuf {
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
    pub(crate) fn extra(path: FileSystemPathBuf) -> Option<Self> {
        path.extension()
            .map_or(true, |ext| matches!(ext, "py" | "pyi"))
            .then_some(Self(ModuleResolutionPathBufInner::Extra(path)))
    }

    #[must_use]
    pub(crate) fn first_party(path: FileSystemPathBuf) -> Option<Self> {
        path.extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
            .then_some(Self(ModuleResolutionPathBufInner::FirstParty(path)))
    }

    #[must_use]
    pub(crate) fn standard_library(path: FileSystemPathBuf) -> Option<Self> {
        path.extension()
            .map_or(true, |ext| ext == "pyi")
            .then_some(Self(ModuleResolutionPathBufInner::StandardLibrary(path)))
    }

    #[must_use]
    pub(crate) fn stdlib_from_typeshed_root(typeshed_root: &FileSystemPath) -> Option<Self> {
        Self::standard_library(typeshed_root.join(FileSystemPath::new("stdlib")))
    }

    #[must_use]
    pub(crate) fn site_packages(path: FileSystemPathBuf) -> Option<Self> {
        path.extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
            .then_some(Self(ModuleResolutionPathBufInner::SitePackages(path)))
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
    pub(crate) fn join(&self, component: &str) -> Self {
        Self(ModuleResolutionPathRefInner::from(&self.0).join(component))
    }

    #[must_use]
    pub(crate) fn relativize_path<'a>(
        &'a self,
        absolute_path: &'a FileSystemPath,
    ) -> Option<ModuleResolutionPathRef<'a>> {
        ModuleResolutionPathRef::from(self).relativize_path(absolute_path)
    }
}

impl From<ModuleResolutionPathBuf> for VfsPath {
    fn from(value: ModuleResolutionPathBuf) -> Self {
        VfsPath::FileSystem(match value.0 {
            ModuleResolutionPathBufInner::Extra(path) => path,
            ModuleResolutionPathBufInner::FirstParty(path) => path,
            ModuleResolutionPathBufInner::StandardLibrary(path) => path,
            ModuleResolutionPathBufInner::SitePackages(path) => path,
        })
    }
}

impl AsRef<FileSystemPath> for ModuleResolutionPathBuf {
    #[inline]
    fn as_ref(&self) -> &FileSystemPath {
        ModuleResolutionPathRefInner::from(&self.0).as_file_system_path()
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
enum ModuleResolutionPathRefInner<'a> {
    Extra(&'a FileSystemPath),
    FirstParty(&'a FileSystemPath),
    StandardLibrary(&'a FileSystemPath),
    SitePackages(&'a FileSystemPath),
}

impl<'a> ModuleResolutionPathRefInner<'a> {
    #[must_use]
    fn load_typeshed_versions<'db>(
        db: &'db dyn Db,
        stdlib_root: &FileSystemPath,
    ) -> &'db TypeshedVersions {
        let versions_path = stdlib_root.join("VERSIONS");
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

    #[must_use]
    fn is_directory(&self, db: &dyn Db, search_path: Self) -> bool {
        match (self, search_path) {
            (Self::Extra(path), Self::Extra(_)) => db.file_system().is_directory(path),
            (Self::FirstParty(path), Self::FirstParty(_)) => db.file_system().is_directory(path),
            (Self::SitePackages(path), Self::SitePackages(_)) => db.file_system().is_directory(path),
            (Self::StandardLibrary(path), Self::StandardLibrary(stdlib_root)) => {
                let Some(module_name) = ModuleResolutionPathRef(*self).to_module_name() else {
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
    fn is_regular_package(&self, db: &dyn Db, search_path: Self) -> bool {
        match (self, search_path) {
            (Self::Extra(path), Self::Extra(_))
            | (Self::FirstParty(path), Self::FirstParty(_))
            | (Self::SitePackages(path), Self::SitePackages(_)) => {
                let file_system = db.file_system();
                file_system.exists(&path.join("__init__.py"))
                    || file_system.exists(&path.join("__init__.pyi"))
            }
            // Unlike the other variants:
            // (1) Account for VERSIONS
            // (2) Only test for `__init__.pyi`, not `__init__.py`
            (Self::StandardLibrary(path), Self::StandardLibrary(stdlib_root)) => {
                let Some(module_name) = ModuleResolutionPathRef(*self).to_module_name() else {
                    return false;
                };
                let typeshed_versions = Self::load_typeshed_versions(db, stdlib_root);
                match typeshed_versions.query_module(&module_name, get_target_py_version(db)) {
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        db.file_system().exists(&path.join("__init__.pyi"))
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
    fn parent(&self) -> Option<Self> {
        match self {
            Self::Extra(path) => path.parent().map(Self::Extra),
            Self::FirstParty(path) => path.parent().map(Self::FirstParty),
            Self::StandardLibrary(path) => path.parent().map(Self::StandardLibrary),
            Self::SitePackages(path) => path.parent().map(Self::SitePackages),
        }
    }

    #[must_use]
    fn ends_with_dunder_init(&self) -> bool {
        match self {
            Self::Extra(path) | Self::FirstParty(path) | Self::SitePackages(path) => {
                path.ends_with("__init__.py") || path.ends_with("__init__.pyi")
            }
            Self::StandardLibrary(path) => path.ends_with("__init__.pyi"),
        }
    }

    #[must_use]
    fn with_dunder_init_stripped(self) -> Self {
        if self.ends_with_dunder_init() {
            self.parent().unwrap_or_else(|| match self {
                Self::Extra(_) => Self::Extra(FileSystemPath::new("")),
                Self::FirstParty(_) => Self::FirstParty(FileSystemPath::new("")),
                Self::StandardLibrary(_) => Self::StandardLibrary(FileSystemPath::new("")),
                Self::SitePackages(_) => Self::SitePackages(FileSystemPath::new("")),
            })
        } else {
            self
        }
    }

    #[must_use]
    #[inline]
    fn as_file_system_path(self) -> &'a FileSystemPath {
        match self {
            Self::Extra(path) => path,
            Self::FirstParty(path) => path,
            Self::StandardLibrary(path) => path,
            Self::SitePackages(path) => path,
        }
    }

    #[must_use]
    fn with_pyi_extension(&self) -> ModuleResolutionPathBufInner {
        match self {
            Self::Extra(path) => ModuleResolutionPathBufInner::Extra(path.with_extension("pyi")),
            Self::FirstParty(path) => {
                ModuleResolutionPathBufInner::FirstParty(path.with_extension("pyi"))
            }
            Self::StandardLibrary(path) => {
                ModuleResolutionPathBufInner::StandardLibrary(path.with_extension("pyi"))
            }
            Self::SitePackages(path) => {
                ModuleResolutionPathBufInner::SitePackages(path.with_extension("pyi"))
            }
        }
    }

    #[must_use]
    fn with_py_extension(&self) -> Option<ModuleResolutionPathBufInner> {
        match self {
            Self::Extra(path) => Some(ModuleResolutionPathBufInner::Extra(
                path.with_extension("py"),
            )),
            Self::FirstParty(path) => Some(ModuleResolutionPathBufInner::FirstParty(
                path.with_extension("py"),
            )),
            Self::StandardLibrary(_) => None,
            Self::SitePackages(path) => Some(ModuleResolutionPathBufInner::SitePackages(
                path.with_extension("py"),
            )),
        }
    }

    #[cfg(test)]
    #[must_use]
    fn to_path_buf(self) -> ModuleResolutionPathBufInner {
        match self {
            Self::Extra(path) => ModuleResolutionPathBufInner::Extra(path.to_path_buf()),
            Self::FirstParty(path) => ModuleResolutionPathBufInner::FirstParty(path.to_path_buf()),
            Self::StandardLibrary(path) => {
                ModuleResolutionPathBufInner::StandardLibrary(path.to_path_buf())
            }
            Self::SitePackages(path) => {
                ModuleResolutionPathBufInner::SitePackages(path.to_path_buf())
            }
        }
    }

    #[cfg(test)]
    #[must_use]
    fn join(
        &self,
        component: &'a (impl AsRef<FileSystemPath> + ?Sized),
    ) -> ModuleResolutionPathBufInner {
        let mut result = self.to_path_buf();
        result.push(component.as_ref().as_str());
        result
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModuleResolutionPathRef<'a>(ModuleResolutionPathRefInner<'a>);

impl<'a> ModuleResolutionPathRef<'a> {
    #[must_use]
    pub(crate) fn extra(path: &'a (impl AsRef<FileSystemPath> + ?Sized)) -> Option<Self> {
        let path = path.as_ref();
        path.extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
            .then_some(Self(ModuleResolutionPathRefInner::Extra(path)))
    }

    #[must_use]
    pub(crate) fn first_party(path: &'a (impl AsRef<FileSystemPath> + ?Sized)) -> Option<Self> {
        let path = path.as_ref();
        path.extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
            .then_some(Self(ModuleResolutionPathRefInner::FirstParty(path)))
    }

    #[must_use]
    pub(crate) fn standard_library(
        path: &'a (impl AsRef<FileSystemPath> + ?Sized),
    ) -> Option<Self> {
        let path = path.as_ref();
        // Unlike other variants, only `.pyi` extensions are permitted
        path.extension()
            .map_or(true, |ext| ext == "pyi")
            .then_some(Self(ModuleResolutionPathRefInner::StandardLibrary(path)))
    }

    #[must_use]
    pub(crate) fn site_packages(path: &'a (impl AsRef<FileSystemPath> + ?Sized)) -> Option<Self> {
        let path = path.as_ref();
        path.extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
            .then_some(Self(ModuleResolutionPathRefInner::SitePackages(path)))
    }

    #[must_use]
    pub(crate) fn is_directory(&self, db: &dyn Db, search_path: impl Into<Self>) -> bool {
        self.0.is_directory(db, search_path.into().0)
    }

    #[must_use]
    pub(crate) fn is_regular_package(&self, db: &dyn Db, search_path: impl Into<Self>) -> bool {
        self.0.is_regular_package(db, search_path.into().0)
    }

    #[must_use]
    pub(crate) fn to_module_name(self) -> Option<ModuleName> {
        ModuleName::from_components(ModulePartIterator::from_fs_path(
            self.0.with_dunder_init_stripped().as_file_system_path(),
        ))
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> ModuleResolutionPathBuf {
        ModuleResolutionPathBuf(self.0.with_pyi_extension())
    }

    #[must_use]
    pub(crate) fn with_py_extension(self) -> Option<ModuleResolutionPathBuf> {
        self.0.with_py_extension().map(ModuleResolutionPathBuf)
    }

    #[must_use]
    pub(crate) fn relativize_path(&self, absolute_path: &'a FileSystemPath) -> Option<Self> {
        match self.0 {
            ModuleResolutionPathRefInner::Extra(root) => {
                absolute_path.strip_prefix(root).ok().and_then(Self::extra)
            }
            ModuleResolutionPathRefInner::FirstParty(root) => absolute_path
                .strip_prefix(root)
                .ok()
                .and_then(Self::first_party),
            ModuleResolutionPathRefInner::StandardLibrary(root) => absolute_path
                .strip_prefix(root)
                .ok()
                .and_then(Self::standard_library),
            ModuleResolutionPathRefInner::SitePackages(root) => absolute_path
                .strip_prefix(root)
                .ok()
                .and_then(Self::site_packages),
        }
    }

    #[cfg(test)]
    pub(crate) fn to_path_buf(self) -> ModuleResolutionPathBuf {
        ModuleResolutionPathBuf(self.0.to_path_buf())
    }
}

impl<'a> From<&'a ModuleResolutionPathBufInner> for ModuleResolutionPathRefInner<'a> {
    #[inline]
    fn from(value: &'a ModuleResolutionPathBufInner) -> Self {
        match value {
            ModuleResolutionPathBufInner::Extra(path) => ModuleResolutionPathRefInner::Extra(path),
            ModuleResolutionPathBufInner::FirstParty(path) => {
                ModuleResolutionPathRefInner::FirstParty(path)
            }
            ModuleResolutionPathBufInner::StandardLibrary(path) => {
                ModuleResolutionPathRefInner::StandardLibrary(path)
            }
            ModuleResolutionPathBufInner::SitePackages(path) => {
                ModuleResolutionPathRefInner::SitePackages(path)
            }
        }
    }
}

impl<'a> From<&'a ModuleResolutionPathBuf> for ModuleResolutionPathRef<'a> {
    fn from(value: &'a ModuleResolutionPathBuf) -> Self {
        ModuleResolutionPathRef(ModuleResolutionPathRefInner::from(&value.0))
    }
}

impl<'a> PartialEq<ModuleResolutionPathBuf> for ModuleResolutionPathRef<'a> {
    fn eq(&self, other: &ModuleResolutionPathBuf) -> bool {
        match (self.0, &other.0) {
            (
                ModuleResolutionPathRefInner::Extra(self_path),
                ModuleResolutionPathBufInner::Extra(other_path),
            )
            | (
                ModuleResolutionPathRefInner::FirstParty(self_path),
                ModuleResolutionPathBufInner::FirstParty(other_path),
            )
            | (
                ModuleResolutionPathRefInner::StandardLibrary(self_path),
                ModuleResolutionPathBufInner::StandardLibrary(other_path),
            )
            | (
                ModuleResolutionPathRefInner::SitePackages(self_path),
                ModuleResolutionPathBufInner::SitePackages(other_path),
            ) => *self_path == **other_path,
            _ => false,
        }
    }
}

impl<'a> PartialEq<FileSystemPath> for ModuleResolutionPathRef<'a> {
    fn eq(&self, other: &FileSystemPath) -> bool {
        self.0.as_file_system_path() == other
    }
}

impl<'a> PartialEq<ModuleResolutionPathRef<'a>> for FileSystemPath {
    fn eq(&self, other: &ModuleResolutionPathRef<'a>) -> bool {
        self == other.0.as_file_system_path()
    }
}

/// Iterate over the "module components" of a path
/// (stripping the extension, if there is one.)
pub(crate) struct ModulePartIterator<'a> {
    parent_components: camino::Utf8Components<'a>,
    stem: Option<&'a str>,
}

impl<'a> ModulePartIterator<'a> {
    #[must_use]
    fn from_fs_path(path: &'a FileSystemPath) -> Self {
        let mut parent_components = path.components();
        parent_components.next_back();
        Self {
            parent_components,
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
        parent_components
            .next()
            .map_or_else(|| stem.take(), |component| Some(component.as_str()))
    }
}

impl<'a> FusedIterator for ModulePartIterator<'a> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_part_iterator() {
        fn create_module_parts(path: &str) -> Vec<&str> {
            ModulePartIterator::from_fs_path(FileSystemPath::new(path)).collect()
        }

        assert_eq!(&create_module_parts("foo.pyi"), &["foo"]);
        assert_eq!(&create_module_parts("foo/bar.pyi"), &["foo", "bar"]);
        assert_eq!(
            &create_module_parts("foo/bar/baz.py"),
            &["foo", "bar", "baz"]
        );
        assert_eq!(&create_module_parts("foo"), &["foo"]);
        assert_eq!(&create_module_parts("foo/bar"), &["foo", "bar"]);
        assert_eq!(&create_module_parts("foo/bar/baz"), &["foo", "bar", "baz"]);
    }
}
