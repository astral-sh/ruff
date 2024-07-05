use std::fmt;

use ruff_db::file_system::{FileSystemPath, FileSystemPathBuf};
use ruff_db::vfs::{system_path_to_file, VfsFile, VfsPath};

use crate::db::Db;
use crate::module_name::ModuleName;
use crate::typeshed::{LazyTypeshedVersions, TypeshedVersionsQueryResult};

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
        if cfg!(debug_assertions) {
            if let Some(extension) = camino::Utf8Path::new(component).extension() {
                match self {
                    Self::Extra(_) | Self::FirstParty(_) | Self::SitePackages(_) => assert!(
                        matches!(extension, "pyi" | "py"),
                        "Extension must be `py` or `pyi`; got `{extension}`"
                    ),
                    Self::StandardLibrary(_) => {
                        assert!(
                            matches!(component.matches('.').count(), 0 | 1),
                            "Component can have at most one '.'; got {component}"
                        );
                        assert_eq!(
                            extension, "pyi",
                            "Extension must be `pyi`; got `{extension}`"
                        );
                    }
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

#[derive(Clone, PartialEq, Eq, Hash)]
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
    pub(crate) fn extra(path: impl Into<FileSystemPathBuf>) -> Option<Self> {
        let path = path.into();
        path.extension()
            .map_or(true, |ext| matches!(ext, "py" | "pyi"))
            .then_some(Self(ModuleResolutionPathBufInner::Extra(path)))
    }

    #[must_use]
    pub(crate) fn first_party(path: impl Into<FileSystemPathBuf>) -> Option<Self> {
        let path = path.into();
        path.extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
            .then_some(Self(ModuleResolutionPathBufInner::FirstParty(path)))
    }

    #[must_use]
    pub(crate) fn standard_library(path: impl Into<FileSystemPathBuf>) -> Option<Self> {
        let path = path.into();
        if path.file_stem().is_some_and(|stem| stem.contains('.')) {
            return None;
        }
        path.extension()
            .map_or(true, |ext| ext == "pyi")
            .then_some(Self(ModuleResolutionPathBufInner::StandardLibrary(path)))
    }

    #[must_use]
    pub(crate) fn stdlib_from_typeshed_root(typeshed_root: &FileSystemPath) -> Option<Self> {
        Self::standard_library(typeshed_root.join(FileSystemPath::new("stdlib")))
    }

    #[must_use]
    pub(crate) fn site_packages(path: impl Into<FileSystemPathBuf>) -> Option<Self> {
        let path = path.into();
        path.extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
            .then_some(Self(ModuleResolutionPathBufInner::SitePackages(path)))
    }

    #[must_use]
    pub(crate) fn is_regular_package(
        &self,
        db: &dyn Db,
        search_path: &Self,
        typeshed_versions: &LazyTypeshedVersions,
    ) -> bool {
        ModuleResolutionPathRef::from(self).is_regular_package(db, search_path, typeshed_versions)
    }

    #[must_use]
    pub(crate) fn is_directory(
        &self,
        db: &dyn Db,
        search_path: &Self,
        typeshed_versions: &LazyTypeshedVersions,
    ) -> bool {
        let as_ref = ModuleResolutionPathRef::from(self);
        as_ref.is_directory(db, search_path, typeshed_versions)
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> Self {
        ModuleResolutionPathRef::from(self).with_pyi_extension()
    }

    #[must_use]
    pub(crate) fn with_py_extension(&self) -> Option<Self> {
        ModuleResolutionPathRef::from(self).with_py_extension()
    }

    #[must_use]
    pub(crate) fn relativize_path<'a>(
        &'a self,
        absolute_path: &'a (impl AsRef<FileSystemPath> + ?Sized),
    ) -> Option<ModuleResolutionPathRef<'a>> {
        ModuleResolutionPathRef::from(self).relativize_path(absolute_path.as_ref())
    }

    /// Returns `None` if the path doesn't exist, isn't accessible, or if the path points to a directory.
    pub(crate) fn to_vfs_file(
        &self,
        db: &dyn Db,
        search_path: &Self,
        typeshed_versions: &LazyTypeshedVersions,
    ) -> Option<VfsFile> {
        ModuleResolutionPathRef::from(self).to_vfs_file(db, search_path, typeshed_versions)
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

impl fmt::Debug for ModuleResolutionPathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (name, path) = match &self.0 {
            ModuleResolutionPathBufInner::Extra(path) => ("Extra", path),
            ModuleResolutionPathBufInner::FirstParty(path) => ("FirstParty", path),
            ModuleResolutionPathBufInner::SitePackages(path) => ("SitePackages", path),
            ModuleResolutionPathBufInner::StandardLibrary(path) => ("StandardLibrary", path),
        };
        f.debug_tuple(&format!("ModuleResolutionPath::{name}"))
            .field(path)
            .finish()
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
    #[inline]
    fn absolute_path_to_module_name(
        absolute_path: &FileSystemPath,
        stdlib_root: ModuleResolutionPathRef,
    ) -> Option<ModuleName> {
        stdlib_root
            .relativize_path(absolute_path)
            .and_then(ModuleResolutionPathRef::to_module_name)
    }

    #[must_use]
    fn is_directory(
        &self,
        db: &dyn Db,
        search_path: ModuleResolutionPathRef<'a>,
        typeshed_versions: &LazyTypeshedVersions,
    ) -> bool {
        match (self, search_path.0) {
            (Self::Extra(path), Self::Extra(_)) => db.file_system().is_directory(path),
            (Self::FirstParty(path), Self::FirstParty(_)) => db.file_system().is_directory(path),
            (Self::SitePackages(path), Self::SitePackages(_)) => db.file_system().is_directory(path),
            (Self::StandardLibrary(path), Self::StandardLibrary(stdlib_root)) => {
                let Some(module_name) = Self::absolute_path_to_module_name(path, search_path) else {
                    return false;
                };
                match typeshed_versions.query_module(&module_name, db, stdlib_root) {
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
    fn is_regular_package(
        &self,
        db: &dyn Db,
        search_path: ModuleResolutionPathRef<'a>,
        typeshed_versions: &LazyTypeshedVersions,
    ) -> bool {
        match (self, search_path.0) {
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
                let Some(module_name) = Self::absolute_path_to_module_name(path, search_path) else {
                    return false;
                };
                match typeshed_versions.query_module(&module_name, db, stdlib_root) {
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

    fn to_vfs_file(
        self,
        db: &dyn Db,
        search_path: ModuleResolutionPathRef<'a>,
        typeshed_versions: &LazyTypeshedVersions,
    ) -> Option<VfsFile> {
        match (self, search_path.0) {
            (Self::Extra(path), Self::Extra(_)) => system_path_to_file(db.upcast(), path),
            (Self::FirstParty(path), Self::FirstParty(_)) => system_path_to_file(db.upcast(), path),
            (Self::SitePackages(path), Self::SitePackages(_)) => {
                system_path_to_file(db.upcast(), path)
            }
            (Self::StandardLibrary(path), Self::StandardLibrary(stdlib_root)) => {
                let module_name = Self::absolute_path_to_module_name(path, search_path)?;
                match typeshed_versions.query_module(&module_name, db, stdlib_root) {
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        system_path_to_file(db.upcast(), path)
                    }
                    TypeshedVersionsQueryResult::DoesNotExist => None,
                }
            }
            (path, root) => unreachable!(
                "The search path should always be the same variant as `self` (got: {path:?}, {root:?})"
            )
        }
    }

    #[must_use]
    pub(crate) fn to_module_name(self) -> Option<ModuleName> {
        let (fs_path, skip_final_part) = match self {
            Self::Extra(path) | Self::FirstParty(path) | Self::SitePackages(path) => (
                path,
                path.ends_with("__init__.py") || path.ends_with("__init__.pyi"),
            ),
            Self::StandardLibrary(path) => (path, path.ends_with("__init__.pyi")),
        };

        let parent_components = fs_path
            .parent()?
            .components()
            .map(|component| component.as_str());

        if skip_final_part {
            ModuleName::from_components(parent_components)
        } else {
            ModuleName::from_components(parent_components.chain(fs_path.file_stem()))
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
    pub(crate) fn is_directory(
        &self,
        db: &dyn Db,
        search_path: impl Into<Self>,
        typeshed_versions: &LazyTypeshedVersions,
    ) -> bool {
        self.0
            .is_directory(db, search_path.into(), typeshed_versions)
    }

    #[must_use]
    pub(crate) fn is_regular_package(
        &self,
        db: &dyn Db,
        search_path: impl Into<Self>,
        typeshed_versions: &LazyTypeshedVersions,
    ) -> bool {
        self.0
            .is_regular_package(db, search_path.into(), typeshed_versions)
    }

    #[must_use]
    pub(crate) fn to_vfs_file(
        self,
        db: &dyn Db,
        search_path: impl Into<Self>,
        typeshed_versions: &LazyTypeshedVersions,
    ) -> Option<VfsFile> {
        self.0
            .to_vfs_file(db, search_path.into(), typeshed_versions)
    }

    #[must_use]
    pub(crate) fn to_module_name(self) -> Option<ModuleName> {
        self.0.to_module_name()
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

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use crate::db::tests::{create_resolver_builder, TestCase, TestDb};
    use crate::supported_py_version::SupportedPyVersion;

    use super::*;

    // Replace windows paths
    static WINDOWS_PATH_FILTER: [(&str, &str); 1] = [(r"\\\\", "/")];

    impl ModuleResolutionPathBuf {
        #[must_use]
        pub(crate) fn join(&self, component: &str) -> Self {
            ModuleResolutionPathRef::from(self).join(component)
        }
    }

    impl<'a> ModuleResolutionPathRef<'a> {
        #[must_use]
        fn join(
            &self,
            component: &'a (impl AsRef<FileSystemPath> + ?Sized),
        ) -> ModuleResolutionPathBuf {
            let mut result = self.to_path_buf();
            result.push(component.as_ref().as_str());
            result
        }

        #[must_use]
        pub(crate) fn to_path_buf(self) -> ModuleResolutionPathBuf {
            let inner = match self.0 {
                ModuleResolutionPathRefInner::Extra(path) => {
                    ModuleResolutionPathBufInner::Extra(path.to_path_buf())
                }
                ModuleResolutionPathRefInner::FirstParty(path) => {
                    ModuleResolutionPathBufInner::FirstParty(path.to_path_buf())
                }
                ModuleResolutionPathRefInner::StandardLibrary(path) => {
                    ModuleResolutionPathBufInner::StandardLibrary(path.to_path_buf())
                }
                ModuleResolutionPathRefInner::SitePackages(path) => {
                    ModuleResolutionPathBufInner::SitePackages(path.to_path_buf())
                }
            };
            ModuleResolutionPathBuf(inner)
        }

        pub(crate) const fn is_stdlib_search_path(&self) -> bool {
            matches!(&self.0, ModuleResolutionPathRefInner::StandardLibrary(_))
        }
    }

    #[test]
    fn constructor_rejects_non_pyi_stdlib_paths() {
        assert_eq!(ModuleResolutionPathBuf::standard_library("foo.py"), None);
        assert_eq!(
            ModuleResolutionPathBuf::standard_library("foo/__init__.py"),
            None
        );
        assert_eq!(
            ModuleResolutionPathBuf::standard_library("foo.py.pyi"),
            None
        );
    }

    fn stdlib_path_test_case(path: &str) -> ModuleResolutionPathBuf {
        ModuleResolutionPathBuf::standard_library(path).unwrap()
    }

    #[test]
    fn stdlib_path_no_extension() {
        assert_debug_snapshot!(stdlib_path_test_case("foo"), @r###"
        ModuleResolutionPath::StandardLibrary(
            "foo",
        )
        "###);
    }

    #[test]
    fn stdlib_path_pyi_extension() {
        assert_debug_snapshot!(stdlib_path_test_case("foo.pyi"), @r###"
        ModuleResolutionPath::StandardLibrary(
            "foo.pyi",
        )
        "###);
    }

    #[test]
    fn stdlib_path_dunder_init() {
        assert_debug_snapshot!(stdlib_path_test_case("foo/__init__.pyi"), @r###"
        ModuleResolutionPath::StandardLibrary(
            "foo/__init__.pyi",
        )
        "###);
    }

    #[test]
    fn stdlib_paths_can_only_be_pyi() {
        assert_eq!(stdlib_path_test_case("foo").with_py_extension(), None);
    }

    #[test]
    fn stdlib_path_with_pyi_extension() {
        assert_debug_snapshot!(
            ModuleResolutionPathBuf::standard_library("foo").unwrap().with_pyi_extension(),
            @r###"
        ModuleResolutionPath::StandardLibrary(
            "foo.pyi",
        )
        "###
        );
    }

    #[test]
    fn non_stdlib_path_with_py_extension() {
        assert_debug_snapshot!(
            ModuleResolutionPathBuf::first_party("foo").unwrap().with_py_extension().unwrap(),
            @r###"
        ModuleResolutionPath::FirstParty(
            "foo.py",
        )
        "###
        );
    }

    #[test]
    fn non_stdlib_path_with_pyi_extension() {
        assert_debug_snapshot!(
            ModuleResolutionPathBuf::first_party("foo").unwrap().with_pyi_extension(),
            @r###"
        ModuleResolutionPath::FirstParty(
            "foo.pyi",
        )
        "###
        );
    }

    fn non_stdlib_module_name_test_case(path: &str) -> ModuleName {
        let variants = [
            ModuleResolutionPathRef::extra,
            ModuleResolutionPathRef::first_party,
            ModuleResolutionPathRef::site_packages,
        ];
        let results: Vec<Option<ModuleName>> = variants
            .into_iter()
            .map(|variant| variant(path).unwrap().to_module_name())
            .collect();
        assert!(results
            .iter()
            .zip(results.iter().take(1))
            .all(|(this, next)| this == next));
        results.into_iter().next().unwrap().unwrap()
    }

    #[test]
    fn module_name_1_part_no_extension() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo"), @r###"
        ModuleName(
            "foo",
        )
        "###);
    }

    #[test]
    fn module_name_one_part_pyi() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo.pyi"), @r###"
        ModuleName(
            "foo",
        )
        "###);
    }

    #[test]
    fn module_name_one_part_py() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo.py"), @r###"
        ModuleName(
            "foo",
        )
        "###);
    }

    #[test]
    fn module_name_2_parts_dunder_init_py() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/__init__.py"), @r###"
        ModuleName(
            "foo",
        )
        "###);
    }

    #[test]
    fn module_name_2_parts_dunder_init_pyi() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/__init__.pyi"), @r###"
        ModuleName(
            "foo",
        )
        "###);
    }

    #[test]
    fn module_name_2_parts_no_extension() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/bar"), @r###"
        ModuleName(
            "foo.bar",
        )
        "###);
    }

    #[test]
    fn module_name_2_parts_pyi() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/bar.pyi"), @r###"
        ModuleName(
            "foo.bar",
        )
        "###);
    }

    #[test]
    fn module_name_2_parts_py() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/bar.py"), @r###"
        ModuleName(
            "foo.bar",
        )
        "###);
    }

    #[test]
    fn module_name_3_parts_dunder_init_pyi() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/bar/__init__.pyi"), @r###"
        ModuleName(
            "foo.bar",
        )
        "###);
    }

    #[test]
    fn module_name_3_parts_dunder_init_py() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/bar/__init__.py"), @r###"
        ModuleName(
            "foo.bar",
        )
        "###);
    }

    #[test]
    fn module_name_3_parts_no_extension() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/bar/baz"), @r###"
        ModuleName(
            "foo.bar.baz",
        )
        "###);
    }

    #[test]
    fn module_name_3_parts_pyi() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/bar/baz.pyi"), @r###"
        ModuleName(
            "foo.bar.baz",
        )
        "###);
    }

    #[test]
    fn module_name_3_parts_py() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/bar/baz.py"), @r###"
        ModuleName(
            "foo.bar.baz",
        )
        "###);
    }

    #[test]
    fn module_name_4_parts_dunder_init_pyi() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/bar/baz/__init__.pyi"), @r###"
        ModuleName(
            "foo.bar.baz",
        )
        "###);
    }

    #[test]
    fn module_name_4_parts_dunder_init_py() {
        assert_debug_snapshot!(non_stdlib_module_name_test_case("foo/bar/baz/__init__.py"), @r###"
        ModuleName(
            "foo.bar.baz",
        )
        "###);
    }

    #[test]
    fn join_1() {
        insta::with_settings!({filters => WINDOWS_PATH_FILTER.to_vec()}, {
            assert_debug_snapshot!(
                ModuleResolutionPathBuf::standard_library("foo").unwrap().join("bar"),
                @r###"
                ModuleResolutionPath::StandardLibrary(
                    "foo/bar",
                )
                "###
            );
        });
    }

    #[test]
    fn join_2() {
        insta::with_settings!({filters => WINDOWS_PATH_FILTER.to_vec()}, {
            assert_debug_snapshot!(
                ModuleResolutionPathBuf::site_packages("foo").unwrap().join("bar.pyi"),
                @r###"
                ModuleResolutionPath::SitePackages(
                    "foo/bar.pyi",
                )
                "###
            );
        });
    }

    #[test]
    fn join_3() {
        insta::with_settings!({filters => WINDOWS_PATH_FILTER.to_vec()}, {
            assert_debug_snapshot!(
                ModuleResolutionPathBuf::extra("foo").unwrap().join("bar.py"),
                @r###"
                ModuleResolutionPath::Extra(
                    "foo/bar.py",
                )
                "###
            );
        });
    }

    #[test]
    fn join_4() {
        insta::with_settings!({filters => WINDOWS_PATH_FILTER.to_vec()}, {
            assert_debug_snapshot!(
                ModuleResolutionPathBuf::first_party("foo").unwrap().join("bar/baz/eggs/__init__.py"),
                @r###"
                ModuleResolutionPath::FirstParty(
                    "foo/bar/baz/eggs/__init__.py",
                )
                "###
            );
        });
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Extension must be `pyi`; got `py`")]
    fn stdlib_path_invalid_join_py() {
        stdlib_path_test_case("foo").push("bar.py");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Extension must be `pyi`; got `rs`")]
    fn stdlib_path_invalid_join_rs() {
        stdlib_path_test_case("foo").push("bar.rs");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Extension must be `py` or `pyi`; got `rs`")]
    fn non_stdlib_path_invalid_join_rs() {
        ModuleResolutionPathBuf::site_packages("foo")
            .unwrap()
            .push("bar.rs");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Component can have at most one '.'")]
    fn invalid_stdlib_join_too_many_extensions() {
        stdlib_path_test_case("foo").push("bar.py.pyi");
    }

    #[test]
    fn relativize_stdlib_path_errors() {
        let root = ModuleResolutionPathBuf::standard_library("foo/stdlib").unwrap();

        // Must have a `.pyi` extension or no extension:
        let bad_absolute_path = FileSystemPath::new("foo/stdlib/x.py");
        assert_eq!(root.relativize_path(bad_absolute_path), None);
        let second_bad_absolute_path = FileSystemPath::new("foo/stdlib/x.rs");
        assert_eq!(root.relativize_path(second_bad_absolute_path), None);

        // Must be a path that is a child of `root`:
        let third_bad_absolute_path = FileSystemPath::new("bar/stdlib/x.pyi");
        assert_eq!(root.relativize_path(third_bad_absolute_path), None);
    }

    fn non_stdlib_relativize_tester(
        variant: impl FnOnce(&'static str) -> Option<ModuleResolutionPathBuf>,
    ) {
        let root = variant("foo").unwrap();
        // Must have a `.py` extension, a `.pyi` extension, or no extension:
        let bad_absolute_path = FileSystemPath::new("foo/stdlib/x.rs");
        assert_eq!(root.relativize_path(bad_absolute_path), None);
        // Must be a path that is a child of `root`:
        let second_bad_absolute_path = FileSystemPath::new("bar/stdlib/x.pyi");
        assert_eq!(root.relativize_path(second_bad_absolute_path), None);
    }

    #[test]
    fn relativize_site_packages_errors() {
        non_stdlib_relativize_tester(ModuleResolutionPathBuf::site_packages);
    }

    #[test]
    fn relativize_extra_errors() {
        non_stdlib_relativize_tester(ModuleResolutionPathBuf::extra);
    }

    #[test]
    fn relativize_first_party_errors() {
        non_stdlib_relativize_tester(ModuleResolutionPathBuf::first_party);
    }

    #[test]
    fn relativize_stdlib_path() {
        insta::with_settings!({filters => WINDOWS_PATH_FILTER.to_vec()}, {
            assert_debug_snapshot!(
                ModuleResolutionPathBuf::standard_library("foo")
                    .unwrap()
                    .relativize_path("foo/baz/eggs/__init__.pyi")
                    .unwrap(),
                @r###"
            ModuleResolutionPathRef(
                StandardLibrary(
                    "baz/eggs/__init__.pyi",
                ),
            )
            "###
            );
        });
    }

    #[test]
    fn relativize_site_packages_path() {
        insta::with_settings!({filters => WINDOWS_PATH_FILTER.to_vec()}, {
            assert_debug_snapshot!(
                ModuleResolutionPathBuf::site_packages("foo")
                    .unwrap()
                    .relativize_path("foo/baz")
                    .unwrap(),
                @r###"
            ModuleResolutionPathRef(
                SitePackages(
                    "baz",
                ),
            )
            "###
            );
        });
    }

    #[test]
    fn relativize_extra_path() {
        insta::with_settings!({filters => WINDOWS_PATH_FILTER.to_vec()}, {
            assert_debug_snapshot!(
                ModuleResolutionPathBuf::extra("foo/baz")
                    .unwrap()
                    .relativize_path("foo/baz/functools.py")
                    .unwrap(),
                @r###"
            ModuleResolutionPathRef(
                Extra(
                    "functools.py",
                ),
            )
            "###
            );
        });
    }

    #[test]
    fn relativize_first_party_path() {
        insta::with_settings!({filters => WINDOWS_PATH_FILTER.to_vec()}, {
            assert_debug_snapshot!(
                ModuleResolutionPathBuf::site_packages("dev/src")
                    .unwrap()
                    .relativize_path("dev/src/package/bar/baz.pyi")
                    .unwrap(),
                @r###"
            ModuleResolutionPathRef(
                SitePackages(
                    "package/bar/baz.pyi",
                ),
            )
            "###
            );
        });
    }

    fn py38_stdlib_test_case() -> (TestDb, ModuleResolutionPathBuf, LazyTypeshedVersions) {
        let TestCase {
            db,
            custom_typeshed,
            ..
        } = create_resolver_builder().unwrap().build();
        let stdlib_module_path =
            ModuleResolutionPathBuf::stdlib_from_typeshed_root(&custom_typeshed).unwrap();
        (db, stdlib_module_path, LazyTypeshedVersions::new())
    }

    #[test]
    fn mocked_typeshed_existing_regular_stdlib_pkg_py38() {
        let (db, stdlib_path, versions) = py38_stdlib_test_case();

        let asyncio_regular_package = stdlib_path.join("asyncio");
        assert!(asyncio_regular_package.is_directory(&db, &stdlib_path, &versions));
        assert!(asyncio_regular_package.is_regular_package(&db, &stdlib_path, &versions));
        // Paths to directories don't resolve to VfsFiles
        assert_eq!(
            asyncio_regular_package.to_vfs_file(&db, &stdlib_path, &versions),
            None
        );
        assert!(asyncio_regular_package
            .join("__init__.pyi")
            .to_vfs_file(&db, &stdlib_path, &versions)
            .is_some());

        // The `asyncio` package exists on Python 3.8, but the `asyncio.tasks` submodule does not,
        // according to the `VERSIONS` file in our typeshed mock:
        let asyncio_tasks_module = stdlib_path.join("asyncio/tasks.pyi");
        assert_eq!(
            asyncio_tasks_module.to_vfs_file(&db, &stdlib_path, &versions),
            None
        );
        assert!(!asyncio_tasks_module.is_directory(&db, &stdlib_path, &versions));
        assert!(!asyncio_tasks_module.is_regular_package(&db, &stdlib_path, &versions));
    }

    #[test]
    fn mocked_typeshed_existing_namespace_stdlib_pkg_py38() {
        let (db, stdlib_path, versions) = py38_stdlib_test_case();

        let xml_namespace_package = stdlib_path.join("xml");
        assert!(xml_namespace_package.is_directory(&db, &stdlib_path, &versions));
        // Paths to directories don't resolve to VfsFiles
        assert_eq!(
            xml_namespace_package.to_vfs_file(&db, &stdlib_path, &versions),
            None
        );
        assert!(!xml_namespace_package.is_regular_package(&db, &stdlib_path, &versions));

        let xml_etree = stdlib_path.join("xml/etree.pyi");
        assert!(!xml_etree.is_directory(&db, &stdlib_path, &versions));
        assert!(xml_etree
            .to_vfs_file(&db, &stdlib_path, &versions)
            .is_some());
        assert!(!xml_etree.is_regular_package(&db, &stdlib_path, &versions));
    }

    #[test]
    fn mocked_typeshed_single_file_stdlib_module_py38() {
        let (db, stdlib_path, versions) = py38_stdlib_test_case();

        let functools_module = stdlib_path.join("functools.pyi");
        assert!(functools_module
            .to_vfs_file(&db, &stdlib_path, &versions)
            .is_some());
        assert!(!functools_module.is_directory(&db, &stdlib_path, &versions));
        assert!(!functools_module.is_regular_package(&db, &stdlib_path, &versions));
    }

    #[test]
    fn mocked_typeshed_nonexistent_regular_stdlib_pkg_py38() {
        let (db, stdlib_path, versions) = py38_stdlib_test_case();

        let collections_regular_package = stdlib_path.join("collections");
        assert_eq!(
            collections_regular_package.to_vfs_file(&db, &stdlib_path, &versions),
            None
        );
        assert!(!collections_regular_package.is_directory(&db, &stdlib_path, &versions));
        assert!(!collections_regular_package.is_regular_package(&db, &stdlib_path, &versions));
    }

    #[test]
    fn mocked_typeshed_nonexistent_namespace_stdlib_pkg_py38() {
        let (db, stdlib_path, versions) = py38_stdlib_test_case();

        let importlib_namespace_package = stdlib_path.join("importlib");
        assert_eq!(
            importlib_namespace_package.to_vfs_file(&db, &stdlib_path, &versions),
            None
        );
        assert!(!importlib_namespace_package.is_directory(&db, &stdlib_path, &versions));
        assert!(!importlib_namespace_package.is_regular_package(&db, &stdlib_path, &versions));

        let importlib_abc = stdlib_path.join("importlib/abc.pyi");
        assert_eq!(
            importlib_abc.to_vfs_file(&db, &stdlib_path, &versions),
            None
        );
        assert!(!importlib_abc.is_directory(&db, &stdlib_path, &versions));
        assert!(!importlib_abc.is_regular_package(&db, &stdlib_path, &versions));
    }

    #[test]
    fn mocked_typeshed_nonexistent_single_file_module_py38() {
        let (db, stdlib_path, versions) = py38_stdlib_test_case();

        let non_existent = stdlib_path.join("doesnt_even_exist");
        assert_eq!(non_existent.to_vfs_file(&db, &stdlib_path, &versions), None);
        assert!(!non_existent.is_directory(&db, &stdlib_path, &versions));
        assert!(!non_existent.is_regular_package(&db, &stdlib_path, &versions));
    }

    fn py39_stdlib_test_case() -> (TestDb, ModuleResolutionPathBuf, LazyTypeshedVersions) {
        let TestCase {
            db,
            custom_typeshed,
            ..
        } = create_resolver_builder()
            .unwrap()
            .with_target_version(SupportedPyVersion::Py39)
            .build();
        let stdlib_module_path =
            ModuleResolutionPathBuf::stdlib_from_typeshed_root(&custom_typeshed).unwrap();
        (db, stdlib_module_path, LazyTypeshedVersions::new())
    }

    #[test]
    fn mocked_typeshed_existing_regular_stdlib_pkgs_py39() {
        let (db, stdlib_path, versions) = py39_stdlib_test_case();

        // Since we've set the target version to Py39,
        // `collections` should now exist as a directory, according to VERSIONS...
        let collections_regular_package = stdlib_path.join("collections");
        assert!(collections_regular_package.is_directory(&db, &stdlib_path, &versions));
        assert!(collections_regular_package.is_regular_package(&db, &stdlib_path, &versions));
        // (This is still `None`, as directories don't resolve to `Vfs` files)
        assert_eq!(
            collections_regular_package.to_vfs_file(&db, &stdlib_path, &versions),
            None
        );
        assert!(collections_regular_package
            .join("__init__.pyi")
            .to_vfs_file(&db, &stdlib_path, &versions)
            .is_some());

        // ...and so should the `asyncio.tasks` submodule (though it's still not a directory):
        let asyncio_tasks_module = stdlib_path.join("asyncio/tasks.pyi");
        assert!(asyncio_tasks_module
            .to_vfs_file(&db, &stdlib_path, &versions)
            .is_some());
        assert!(!asyncio_tasks_module.is_directory(&db, &stdlib_path, &versions));
        assert!(!asyncio_tasks_module.is_regular_package(&db, &stdlib_path, &versions));
    }

    #[test]
    fn mocked_typeshed_existing_namespace_stdlib_pkg_py39() {
        let (db, stdlib_path, versions) = py39_stdlib_test_case();

        // The `importlib` directory now also exists...
        let importlib_namespace_package = stdlib_path.join("importlib");
        assert!(importlib_namespace_package.is_directory(&db, &stdlib_path, &versions));
        assert!(!importlib_namespace_package.is_regular_package(&db, &stdlib_path, &versions));
        // (This is still `None`, as directories don't resolve to `Vfs` files)
        assert_eq!(
            importlib_namespace_package.to_vfs_file(&db, &stdlib_path, &versions),
            None
        );

        // ...As do submodules in the `importlib` namespace package:
        let importlib_abc = importlib_namespace_package.join("abc.pyi");
        assert!(!importlib_abc.is_directory(&db, &stdlib_path, &versions));
        assert!(!importlib_abc.is_regular_package(&db, &stdlib_path, &versions));
        assert!(importlib_abc
            .to_vfs_file(&db, &stdlib_path, &versions)
            .is_some());
    }

    #[test]
    fn mocked_typeshed_nonexistent_namespace_stdlib_pkg_py39() {
        let (db, stdlib_path, versions) = py39_stdlib_test_case();

        // The `xml` package no longer exists on py39:
        let xml_namespace_package = stdlib_path.join("xml");
        assert_eq!(
            xml_namespace_package.to_vfs_file(&db, &stdlib_path, &versions),
            None
        );
        assert!(!xml_namespace_package.is_directory(&db, &stdlib_path, &versions));
        assert!(!xml_namespace_package.is_regular_package(&db, &stdlib_path, &versions));

        let xml_etree = xml_namespace_package.join("etree.pyi");
        assert_eq!(xml_etree.to_vfs_file(&db, &stdlib_path, &versions), None);
        assert!(!xml_etree.is_directory(&db, &stdlib_path, &versions));
        assert!(!xml_etree.is_regular_package(&db, &stdlib_path, &versions));
    }
}
