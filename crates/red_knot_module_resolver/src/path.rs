//! Internal abstractions for differentiating between different kinds of search paths.
//!
//! TODO(Alex): Should we use different types for absolute vs relative paths?
//! <https://github.com/astral-sh/ruff/pull/12141#discussion_r1667010245>

use std::fmt;

use ruff_db::files::{system_path_to_file, File};
use ruff_db::system::{SystemPath, SystemPathBuf};

use crate::module_name::ModuleName;
use crate::state::ResolverState;
use crate::typeshed::TypeshedVersionsQueryResult;

/// Enumeration of the different kinds of search paths type checkers are expected to support.
///
/// N.B. Although we don't implement `Ord` for this enum, they are ordered in terms of the
/// priority that we want to give these modules when resolving them,
/// as per [the order given in the typing spec]
///
/// [the order given in the typing spec]: https://typing.readthedocs.io/en/latest/spec/distributing.html#import-resolution-ordering
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ModuleResolutionPathBufInner {
    Extra(SystemPathBuf),
    FirstParty(SystemPathBuf),
    StandardLibrary(SystemPathBuf),
    SitePackages(SystemPathBuf),
}

impl ModuleResolutionPathBufInner {
    fn push(&mut self, component: &str) {
        let extension = camino::Utf8Path::new(component).extension();
        let inner = match self {
            Self::Extra(ref mut path) => {
                if let Some(extension) = extension {
                    assert!(
                        matches!(extension, "pyi" | "py"),
                        "Extension must be `py` or `pyi`; got `{extension}`"
                    );
                }
                path
            }
            Self::FirstParty(ref mut path) => {
                if let Some(extension) = extension {
                    assert!(
                        matches!(extension, "pyi" | "py"),
                        "Extension must be `py` or `pyi`; got `{extension}`"
                    );
                }
                path
            }
            Self::StandardLibrary(ref mut path) => {
                if let Some(extension) = extension {
                    assert_eq!(
                        extension, "pyi",
                        "Extension must be `pyi`; got `{extension}`"
                    );
                }
                path
            }
            Self::SitePackages(ref mut path) => {
                if let Some(extension) = extension {
                    assert!(
                        matches!(extension, "pyi" | "py"),
                        "Extension must be `py` or `pyi`; got `{extension}`"
                    );
                }
                path
            }
        };
        assert!(
            inner.extension().is_none(),
            "Cannot push part {component} to {inner}, which already has an extension"
        );
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
    pub(crate) fn extra(path: impl Into<SystemPathBuf>) -> Option<Self> {
        let path = path.into();
        path.extension()
            .map_or(true, |ext| matches!(ext, "py" | "pyi"))
            .then_some(Self(ModuleResolutionPathBufInner::Extra(path)))
    }

    #[must_use]
    pub(crate) fn first_party(path: impl Into<SystemPathBuf>) -> Option<Self> {
        let path = path.into();
        path.extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
            .then_some(Self(ModuleResolutionPathBufInner::FirstParty(path)))
    }

    #[must_use]
    pub(crate) fn standard_library(path: impl Into<SystemPathBuf>) -> Option<Self> {
        let path = path.into();
        path.extension()
            .map_or(true, |ext| ext == "pyi")
            .then_some(Self(ModuleResolutionPathBufInner::StandardLibrary(path)))
    }

    #[must_use]
    pub(crate) fn stdlib_from_typeshed_root(typeshed_root: &SystemPath) -> Option<Self> {
        Self::standard_library(typeshed_root.join(SystemPath::new("stdlib")))
    }

    #[must_use]
    pub(crate) fn site_packages(path: impl Into<SystemPathBuf>) -> Option<Self> {
        let path = path.into();
        path.extension()
            .map_or(true, |ext| matches!(ext, "pyi" | "py"))
            .then_some(Self(ModuleResolutionPathBufInner::SitePackages(path)))
    }

    #[must_use]
    pub(crate) fn is_regular_package(&self, search_path: &Self, resolver: &ResolverState) -> bool {
        ModuleResolutionPathRef::from(self).is_regular_package(search_path, resolver)
    }

    #[must_use]
    pub(crate) fn is_directory(&self, search_path: &Self, resolver: &ResolverState) -> bool {
        ModuleResolutionPathRef::from(self).is_directory(search_path, resolver)
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
        absolute_path: &'a (impl AsRef<SystemPath> + ?Sized),
    ) -> Option<ModuleResolutionPathRef<'a>> {
        ModuleResolutionPathRef::from(self).relativize_path(absolute_path.as_ref())
    }

    /// Returns `None` if the path doesn't exist, isn't accessible, or if the path points to a directory.
    pub(crate) fn to_file(&self, search_path: &Self, resolver: &ResolverState) -> Option<File> {
        ModuleResolutionPathRef::from(self).to_file(search_path, resolver)
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
        f.debug_tuple(&format!("ModuleResolutionPathBuf::{name}"))
            .field(path)
            .finish()
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
enum ModuleResolutionPathRefInner<'a> {
    Extra(&'a SystemPath),
    FirstParty(&'a SystemPath),
    StandardLibrary(&'a SystemPath),
    SitePackages(&'a SystemPath),
}

impl<'a> ModuleResolutionPathRefInner<'a> {
    #[must_use]
    fn query_stdlib_version<'db>(
        module_path: &'a SystemPath,
        stdlib_search_path: Self,
        stdlib_root: &SystemPath,
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
        typeshed_versions.query_module(&module_name, *db, stdlib_root, *target_version)
    }

    #[must_use]
    fn is_directory(&self, search_path: Self, resolver: &ResolverState) -> bool {
        match (self, search_path) {
            (Self::Extra(path), Self::Extra(_)) => resolver.system().is_directory(path),
            (Self::FirstParty(path), Self::FirstParty(_)) => resolver.system().is_directory(path),
            (Self::SitePackages(path), Self::SitePackages(_)) => resolver.system().is_directory(path),
            (Self::StandardLibrary(path), Self::StandardLibrary(stdlib_root)) => {
                match Self::query_stdlib_version( path, search_path, stdlib_root, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists => resolver.system().is_directory(path),
                    TypeshedVersionsQueryResult::MaybeExists => resolver.system().is_directory(path),
                }
            }
            (path, root) => unreachable!(
                "The search path should always be the same variant as `self` (got: {path:?}, {root:?})"
            )
        }
    }

    #[must_use]
    fn is_regular_package(&self, search_path: Self, resolver: &ResolverState) -> bool {
        fn is_non_stdlib_pkg(state: &ResolverState, path: &SystemPath) -> bool {
            let file_system = state.system();
            file_system.path_exists(&path.join("__init__.py"))
                || file_system.path_exists(&path.join("__init__.pyi"))
        }

        match (self, search_path) {
            (Self::Extra(path), Self::Extra(_)) => is_non_stdlib_pkg(resolver, path),
            (Self::FirstParty(path), Self::FirstParty(_)) => is_non_stdlib_pkg(resolver, path),
            (Self::SitePackages(path), Self::SitePackages(_)) => is_non_stdlib_pkg(resolver, path),
            // Unlike the other variants:
            // (1) Account for VERSIONS
            // (2) Only test for `__init__.pyi`, not `__init__.py`
            (Self::StandardLibrary(path), Self::StandardLibrary(stdlib_root)) => {
                match Self::query_stdlib_version( path, search_path, stdlib_root, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists => resolver.db.system().path_exists(&path.join("__init__.pyi")),
                    TypeshedVersionsQueryResult::MaybeExists => resolver.db.system().path_exists(&path.join("__init__.pyi")),
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
            (Self::StandardLibrary(path), Self::StandardLibrary(stdlib_root)) => {
                match Self::query_stdlib_version(path, search_path, stdlib_root, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => None,
                    TypeshedVersionsQueryResult::Exists => system_path_to_file(resolver.db.upcast(), path),
                    TypeshedVersionsQueryResult::MaybeExists => system_path_to_file(resolver.db.upcast(), path)
                }
            }
            (path, root) => unreachable!(
                "The search path should always be the same variant as `self` (got: {path:?}, {root:?})"
            )
        }
    }

    #[must_use]
    fn to_module_name(self) -> Option<ModuleName> {
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

    #[must_use]
    fn relativize_path(&self, absolute_path: &'a SystemPath) -> Option<Self> {
        match self {
            Self::Extra(root) => absolute_path.strip_prefix(root).ok().and_then(|path| {
                path.extension()
                    .map_or(true, |ext| matches!(ext, "py" | "pyi"))
                    .then_some(Self::Extra(path))
            }),
            Self::FirstParty(root) => absolute_path.strip_prefix(root).ok().and_then(|path| {
                path.extension()
                    .map_or(true, |ext| matches!(ext, "pyi" | "py"))
                    .then_some(Self::FirstParty(path))
            }),
            Self::StandardLibrary(root) => absolute_path.strip_prefix(root).ok().and_then(|path| {
                path.extension()
                    .map_or(true, |ext| ext == "pyi")
                    .then_some(Self::StandardLibrary(path))
            }),
            Self::SitePackages(root) => absolute_path.strip_prefix(root).ok().and_then(|path| {
                path.extension()
                    .map_or(true, |ext| matches!(ext, "pyi" | "py"))
                    .then_some(Self::SitePackages(path))
            }),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModuleResolutionPathRef<'a>(ModuleResolutionPathRefInner<'a>);

impl<'a> ModuleResolutionPathRef<'a> {
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
    pub(crate) fn with_pyi_extension(&self) -> ModuleResolutionPathBuf {
        ModuleResolutionPathBuf(self.0.with_pyi_extension())
    }

    #[must_use]
    pub(crate) fn with_py_extension(self) -> Option<ModuleResolutionPathBuf> {
        self.0.with_py_extension().map(ModuleResolutionPathBuf)
    }

    #[must_use]
    pub(crate) fn relativize_path(&self, absolute_path: &'a SystemPath) -> Option<Self> {
        self.0.relativize_path(absolute_path).map(Self)
    }
}

impl fmt::Debug for ModuleResolutionPathRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (name, path) = match &self.0 {
            ModuleResolutionPathRefInner::Extra(path) => ("Extra", path),
            ModuleResolutionPathRefInner::FirstParty(path) => ("FirstParty", path),
            ModuleResolutionPathRefInner::SitePackages(path) => ("SitePackages", path),
            ModuleResolutionPathRefInner::StandardLibrary(path) => ("StandardLibrary", path),
        };
        f.debug_tuple(&format!("ModuleResolutionPathRef::{name}"))
            .field(path)
            .finish()
    }
}

impl<'a> From<&'a ModuleResolutionPathBuf> for ModuleResolutionPathRef<'a> {
    fn from(value: &'a ModuleResolutionPathBuf) -> Self {
        let inner = match &value.0 {
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
        };
        ModuleResolutionPathRef(inner)
    }
}

impl PartialEq<SystemPath> for ModuleResolutionPathRef<'_> {
    fn eq(&self, other: &SystemPath) -> bool {
        let fs_path = match self.0 {
            ModuleResolutionPathRefInner::Extra(path) => path,
            ModuleResolutionPathRefInner::FirstParty(path) => path,
            ModuleResolutionPathRefInner::SitePackages(path) => path,
            ModuleResolutionPathRefInner::StandardLibrary(path) => path,
        };
        fs_path == other
    }
}

impl PartialEq<ModuleResolutionPathRef<'_>> for SystemPath {
    fn eq(&self, other: &ModuleResolutionPathRef) -> bool {
        other == self
    }
}

impl PartialEq<SystemPathBuf> for ModuleResolutionPathRef<'_> {
    fn eq(&self, other: &SystemPathBuf) -> bool {
        self == &**other
    }
}

impl PartialEq<ModuleResolutionPathRef<'_>> for SystemPathBuf {
    fn eq(&self, other: &ModuleResolutionPathRef<'_>) -> bool {
        &**self == other
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_debug_snapshot;

    use crate::db::tests::{create_resolver_builder, TestCase, TestDb};
    use crate::supported_py_version::TargetVersion;
    use crate::typeshed::LazyTypeshedVersions;

    use super::*;

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
            component: &'a (impl AsRef<SystemPath> + ?Sized),
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

        #[must_use]
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
    }

    #[test]
    fn path_buf_debug_impl() {
        assert_debug_snapshot!(
            ModuleResolutionPathBuf::standard_library("foo/bar.pyi").unwrap(),
            @r###"
        ModuleResolutionPathBuf::StandardLibrary(
            "foo/bar.pyi",
        )
        "###
        );
    }

    #[test]
    fn path_ref_debug_impl() {
        assert_debug_snapshot!(
            ModuleResolutionPathRef(ModuleResolutionPathRefInner::Extra(SystemPath::new("foo/bar.py"))),
            @r###"
        ModuleResolutionPathRef::Extra(
            "foo/bar.py",
        )
        "###
        );
    }

    #[test]
    fn with_extension_methods() {
        assert_eq!(
            ModuleResolutionPathBuf::standard_library("foo")
                .unwrap()
                .with_py_extension(),
            None
        );

        assert_eq!(
            ModuleResolutionPathBuf::standard_library("foo")
                .unwrap()
                .with_pyi_extension(),
            ModuleResolutionPathBuf(ModuleResolutionPathBufInner::StandardLibrary(
                SystemPathBuf::from("foo.pyi")
            ))
        );

        assert_eq!(
            ModuleResolutionPathBuf::first_party("foo/bar")
                .unwrap()
                .with_py_extension()
                .unwrap(),
            ModuleResolutionPathBuf(ModuleResolutionPathBufInner::FirstParty(
                SystemPathBuf::from("foo/bar.py")
            ))
        );
    }

    #[test]
    fn module_name_1_part() {
        assert_eq!(
            ModuleResolutionPathRef(ModuleResolutionPathRefInner::Extra(SystemPath::new("foo")))
                .to_module_name(),
            ModuleName::new_static("foo")
        );

        assert_eq!(
            ModuleResolutionPathRef(ModuleResolutionPathRefInner::StandardLibrary(
                SystemPath::new("foo.pyi")
            ))
            .to_module_name(),
            ModuleName::new_static("foo")
        );

        assert_eq!(
            ModuleResolutionPathRef(ModuleResolutionPathRefInner::FirstParty(SystemPath::new(
                "foo/__init__.py"
            )))
            .to_module_name(),
            ModuleName::new_static("foo")
        );
    }

    #[test]
    fn module_name_2_parts() {
        assert_eq!(
            ModuleResolutionPathRef(ModuleResolutionPathRefInner::StandardLibrary(
                SystemPath::new("foo/bar")
            ))
            .to_module_name(),
            ModuleName::new_static("foo.bar")
        );

        assert_eq!(
            ModuleResolutionPathRef(ModuleResolutionPathRefInner::Extra(SystemPath::new(
                "foo/bar.pyi"
            )))
            .to_module_name(),
            ModuleName::new_static("foo.bar")
        );

        assert_eq!(
            ModuleResolutionPathRef(ModuleResolutionPathRefInner::SitePackages(SystemPath::new(
                "foo/bar/__init__.pyi"
            )))
            .to_module_name(),
            ModuleName::new_static("foo.bar")
        );
    }

    #[test]
    fn module_name_3_parts() {
        assert_eq!(
            ModuleResolutionPathRef(ModuleResolutionPathRefInner::SitePackages(SystemPath::new(
                "foo/bar/__init__.pyi"
            )))
            .to_module_name(),
            ModuleName::new_static("foo.bar")
        );

        assert_eq!(
            ModuleResolutionPathRef(ModuleResolutionPathRefInner::SitePackages(SystemPath::new(
                "foo/bar/baz"
            )))
            .to_module_name(),
            ModuleName::new_static("foo.bar.baz")
        );
    }

    #[test]
    fn join() {
        assert_eq!(
            ModuleResolutionPathBuf::standard_library("foo")
                .unwrap()
                .join("bar"),
            ModuleResolutionPathBuf(ModuleResolutionPathBufInner::StandardLibrary(
                SystemPathBuf::from("foo/bar")
            ))
        );
        assert_eq!(
            ModuleResolutionPathBuf::standard_library("foo")
                .unwrap()
                .join("bar.pyi"),
            ModuleResolutionPathBuf(ModuleResolutionPathBufInner::StandardLibrary(
                SystemPathBuf::from("foo/bar.pyi")
            ))
        );
        assert_eq!(
            ModuleResolutionPathBuf::extra("foo")
                .unwrap()
                .join("bar.py"),
            ModuleResolutionPathBuf(ModuleResolutionPathBufInner::Extra(SystemPathBuf::from(
                "foo/bar.py"
            )))
        );
    }

    #[test]
    #[should_panic(expected = "Extension must be `pyi`; got `py`")]
    fn stdlib_path_invalid_join_py() {
        ModuleResolutionPathBuf::standard_library("foo")
            .unwrap()
            .push("bar.py");
    }

    #[test]
    #[should_panic(expected = "Extension must be `pyi`; got `rs`")]
    fn stdlib_path_invalid_join_rs() {
        ModuleResolutionPathBuf::standard_library("foo")
            .unwrap()
            .push("bar.rs");
    }

    #[test]
    #[should_panic(expected = "Extension must be `py` or `pyi`; got `rs`")]
    fn non_stdlib_path_invalid_join_rs() {
        ModuleResolutionPathBuf::site_packages("foo")
            .unwrap()
            .push("bar.rs");
    }

    #[test]
    #[should_panic(expected = "already has an extension")]
    fn invalid_stdlib_join_too_many_extensions() {
        ModuleResolutionPathBuf::standard_library("foo.pyi")
            .unwrap()
            .push("bar.pyi");
    }

    #[test]
    fn relativize_stdlib_path_errors() {
        let root = ModuleResolutionPathBuf::standard_library("foo/stdlib").unwrap();

        // Must have a `.pyi` extension or no extension:
        let bad_absolute_path = SystemPath::new("foo/stdlib/x.py");
        assert_eq!(root.relativize_path(bad_absolute_path), None);
        let second_bad_absolute_path = SystemPath::new("foo/stdlib/x.rs");
        assert_eq!(root.relativize_path(second_bad_absolute_path), None);

        // Must be a path that is a child of `root`:
        let third_bad_absolute_path = SystemPath::new("bar/stdlib/x.pyi");
        assert_eq!(root.relativize_path(third_bad_absolute_path), None);
    }

    #[test]
    fn relativize_non_stdlib_path_errors() {
        let root = ModuleResolutionPathBuf::extra("foo/stdlib").unwrap();
        // Must have a `.py` extension, a `.pyi` extension, or no extension:
        let bad_absolute_path = SystemPath::new("foo/stdlib/x.rs");
        assert_eq!(root.relativize_path(bad_absolute_path), None);
        // Must be a path that is a child of `root`:
        let second_bad_absolute_path = SystemPath::new("bar/stdlib/x.pyi");
        assert_eq!(root.relativize_path(second_bad_absolute_path), None);
    }

    #[test]
    fn relativize_path() {
        assert_eq!(
            ModuleResolutionPathBuf::standard_library("foo/baz")
                .unwrap()
                .relativize_path("foo/baz/eggs/__init__.pyi")
                .unwrap(),
            ModuleResolutionPathRef(ModuleResolutionPathRefInner::StandardLibrary(
                SystemPath::new("eggs/__init__.pyi")
            ))
        );
    }

    fn py38_stdlib_test_case() -> (TestDb, ModuleResolutionPathBuf) {
        let TestCase {
            db,
            custom_typeshed,
            ..
        } = create_resolver_builder().unwrap().build();
        let stdlib_module_path =
            ModuleResolutionPathBuf::stdlib_from_typeshed_root(&custom_typeshed).unwrap();
        (db, stdlib_module_path)
    }

    #[test]
    fn mocked_typeshed_existing_regular_stdlib_pkg_py38() {
        let (db, stdlib_path) = py38_stdlib_test_case();
        let resolver = ResolverState {
            db: &db,
            typeshed_versions: LazyTypeshedVersions::new(),
            target_version: TargetVersion::Py38,
        };

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
        let (db, stdlib_path) = py38_stdlib_test_case();
        let resolver = ResolverState {
            db: &db,
            typeshed_versions: LazyTypeshedVersions::new(),
            target_version: TargetVersion::Py38,
        };

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
        let (db, stdlib_path) = py38_stdlib_test_case();
        let resolver = ResolverState {
            db: &db,
            typeshed_versions: LazyTypeshedVersions::new(),
            target_version: TargetVersion::Py38,
        };

        let functools_module = stdlib_path.join("functools.pyi");
        assert!(functools_module.to_file(&stdlib_path, &resolver).is_some());
        assert!(!functools_module.is_directory(&stdlib_path, &resolver));
        assert!(!functools_module.is_regular_package(&stdlib_path, &resolver));
    }

    #[test]
    fn mocked_typeshed_nonexistent_regular_stdlib_pkg_py38() {
        let (db, stdlib_path) = py38_stdlib_test_case();
        let resolver = ResolverState {
            db: &db,
            typeshed_versions: LazyTypeshedVersions::new(),
            target_version: TargetVersion::Py38,
        };

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
        let (db, stdlib_path) = py38_stdlib_test_case();
        let resolver = ResolverState {
            db: &db,
            typeshed_versions: LazyTypeshedVersions::new(),
            target_version: TargetVersion::Py38,
        };

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
        let (db, stdlib_path) = py38_stdlib_test_case();
        let resolver = ResolverState {
            db: &db,
            typeshed_versions: LazyTypeshedVersions::new(),
            target_version: TargetVersion::Py38,
        };

        let non_existent = stdlib_path.join("doesnt_even_exist");
        assert_eq!(non_existent.to_file(&stdlib_path, &resolver), None);
        assert!(!non_existent.is_directory(&stdlib_path, &resolver));
        assert!(!non_existent.is_regular_package(&stdlib_path, &resolver));
    }

    fn py39_stdlib_test_case() -> (TestDb, ModuleResolutionPathBuf) {
        let TestCase {
            db,
            custom_typeshed,
            ..
        } = create_resolver_builder()
            .unwrap()
            .with_target_version(TargetVersion::Py39)
            .build();
        let stdlib_module_path =
            ModuleResolutionPathBuf::stdlib_from_typeshed_root(&custom_typeshed).unwrap();
        (db, stdlib_module_path)
    }

    #[test]
    fn mocked_typeshed_existing_regular_stdlib_pkgs_py39() {
        let (db, stdlib_path) = py39_stdlib_test_case();
        let resolver = ResolverState {
            db: &db,
            typeshed_versions: LazyTypeshedVersions::new(),
            target_version: TargetVersion::Py39,
        };

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
        let (db, stdlib_path) = py39_stdlib_test_case();
        let resolver = ResolverState {
            db: &db,
            typeshed_versions: LazyTypeshedVersions::new(),
            target_version: TargetVersion::Py39,
        };

        // The `importlib` directory now also exists...
        let importlib_namespace_package = stdlib_path.join("importlib");
        assert!(importlib_namespace_package.is_directory(&stdlib_path, &resolver));
        assert!(!importlib_namespace_package.is_regular_package(&stdlib_path, &resolver));
        // (This is still `None`, as directories don't resolve to `Vfs` files)
        assert_eq!(
            importlib_namespace_package.to_file(&stdlib_path, &resolver),
            None
        );

        // ...As do submodules in the `importlib` namespace package:
        let importlib_abc = importlib_namespace_package.join("abc.pyi");
        assert!(!importlib_abc.is_directory(&stdlib_path, &resolver));
        assert!(!importlib_abc.is_regular_package(&stdlib_path, &resolver));
        assert!(importlib_abc.to_file(&stdlib_path, &resolver).is_some());
    }

    #[test]
    fn mocked_typeshed_nonexistent_namespace_stdlib_pkg_py39() {
        let (db, stdlib_path) = py39_stdlib_test_case();
        let resolver = ResolverState {
            db: &db,
            typeshed_versions: LazyTypeshedVersions::new(),
            target_version: TargetVersion::Py39,
        };

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
