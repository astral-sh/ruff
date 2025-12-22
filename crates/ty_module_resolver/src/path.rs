//! Internal abstractions for differentiating between different kinds of search paths.

use std::fmt;
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use ruff_db::files::{File, FileError, FilePath, system_path_to_file, vendored_path_to_file};
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_db::vendored::{VendoredPath, VendoredPathBuf};

use crate::Db;
use crate::module_name::ModuleName;
use crate::resolve::{PyTyped, ResolverContext};
use crate::typeshed::{TypeshedVersionsQueryResult, typeshed_versions};

/// A path that points to a Python module.
///
/// A `ModulePath` is made up of two elements:
/// - The [`SearchPath`] that was used to find this module.
///   This could point to a directory on disk or a directory
///   in the vendored zip archive.
/// - A relative path from the search path to the file
///   that contains the source code of the Python module in question.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ModulePath {
    search_path: SearchPath,
    relative_path: Utf8PathBuf,
}

impl ModulePath {
    #[must_use]
    pub(crate) fn is_standard_library(&self) -> bool {
        matches!(
            &*self.search_path.0,
            SearchPathInner::StandardLibraryCustom(_) | SearchPathInner::StandardLibraryVendored(_)
        )
    }

    /// Returns true if this is a path to a "stub file."
    ///
    /// i.e., A module whose file extension is `pyi`.
    #[must_use]
    pub(crate) fn is_stub_file(&self) -> bool {
        self.relative_path.extension() == Some("pyi")
    }

    /// Returns true if this is a path to a "stub package."
    ///
    /// i.e., A module whose top-most parent package corresponds to a
    /// directory with a `-stubs` suffix in its name.
    #[must_use]
    pub(crate) fn is_stub_package(&self) -> bool {
        let Some(first) = self.relative_path.components().next() else {
            return false;
        };
        first.as_str().ends_with("-stubs")
    }

    pub(crate) fn push(&mut self, component: &str) {
        if let Some(component_extension) = camino::Utf8Path::new(component).extension() {
            assert!(
                self.relative_path.extension().is_none(),
                "Cannot push part {component} to {self:?}, which already has an extension"
            );
            if self.is_standard_library() {
                assert_eq!(
                    component_extension, "pyi",
                    "Extension must be `pyi`; got `{component_extension}`"
                );
            } else {
                assert!(
                    matches!(component_extension, "pyi" | "py"),
                    "Extension must be `py` or `pyi`; got `{component_extension}`"
                );
            }
        }
        self.relative_path.push(component);
    }

    pub(crate) fn pop(&mut self) -> bool {
        self.relative_path.pop()
    }

    pub(super) fn search_path(&self) -> &SearchPath {
        &self.search_path
    }

    #[must_use]
    pub(super) fn is_directory(&self, resolver: &ResolverContext) -> bool {
        let ModulePath {
            search_path,
            relative_path,
        } = self;
        match &*search_path.0 {
            SearchPathInner::Extra(search_path)
            | SearchPathInner::FirstParty(search_path)
            | SearchPathInner::SitePackages(search_path)
            | SearchPathInner::Editable(search_path)
            | SearchPathInner::StandardLibraryReal(search_path) => {
                system_path_to_file(resolver.db, search_path.join(relative_path))
                    == Err(FileError::IsADirectory)
            }
            SearchPathInner::StandardLibraryCustom(stdlib_root) => {
                match query_stdlib_version(relative_path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        system_path_to_file(resolver.db, stdlib_root.join(relative_path))
                            == Err(FileError::IsADirectory)
                    }
                }
            }
            SearchPathInner::StandardLibraryVendored(stdlib_root) => {
                match query_stdlib_version(relative_path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => resolver
                        .vendored()
                        .is_directory(stdlib_root.join(relative_path)),
                }
            }
        }
    }

    #[must_use]
    pub(super) fn is_regular_package(&self, resolver: &ResolverContext) -> bool {
        let ModulePath {
            search_path,
            relative_path,
        } = self;

        match &*search_path.0 {
            SearchPathInner::Extra(search_path)
            | SearchPathInner::FirstParty(search_path)
            | SearchPathInner::SitePackages(search_path)
            | SearchPathInner::Editable(search_path) => {
                let absolute_path = search_path.join(relative_path);

                system_path_to_file(resolver.db, absolute_path.join("__init__.py")).is_ok()
                    || system_path_to_file(resolver.db, absolute_path.join("__init__.pyi")).is_ok()
            }
            SearchPathInner::StandardLibraryReal(search_path) => {
                let absolute_path = search_path.join(relative_path);

                system_path_to_file(resolver.db, absolute_path.join("__init__.py")).is_ok()
            }
            SearchPathInner::StandardLibraryCustom(search_path) => {
                match query_stdlib_version(relative_path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => system_path_to_file(
                        resolver.db,
                        search_path.join(relative_path).join("__init__.pyi"),
                    )
                    .is_ok(),
                }
            }
            SearchPathInner::StandardLibraryVendored(search_path) => {
                match query_stdlib_version(relative_path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => resolver
                        .vendored()
                        .exists(search_path.join(relative_path).join("__init__.pyi")),
                }
            }
        }
    }

    /// Get the `py.typed` info for this package (not considering parent packages)
    pub(super) fn py_typed(&self, resolver: &ResolverContext) -> PyTyped {
        let Some(py_typed_contents) = self.to_system_path().and_then(|path| {
            let py_typed_path = path.join("py.typed");
            let py_typed_file = system_path_to_file(resolver.db, py_typed_path).ok()?;
            // If we fail to read it let's say that's like it doesn't exist
            // (right now the difference between Untyped and Full is academic)
            py_typed_file.read_to_string(resolver.db).ok()
        }) else {
            return PyTyped::Untyped;
        };
        // The python typing spec says to look for "partial\n" but in the wild we've seen:
        //
        // * PARTIAL\n
        // * partial\\n (as in they typed "\n")
        // * partial/n
        //
        // since the py.typed file never really grew any other contents, let's be permissive
        if py_typed_contents.to_ascii_lowercase().contains("partial") {
            PyTyped::Partial
        } else {
            PyTyped::Full
        }
    }

    pub(super) fn to_system_path(&self) -> Option<SystemPathBuf> {
        let ModulePath {
            search_path,
            relative_path,
        } = self;
        match &*search_path.0 {
            SearchPathInner::Extra(search_path)
            | SearchPathInner::FirstParty(search_path)
            | SearchPathInner::SitePackages(search_path)
            | SearchPathInner::Editable(search_path) => Some(search_path.join(relative_path)),
            SearchPathInner::StandardLibraryReal(stdlib_root)
            | SearchPathInner::StandardLibraryCustom(stdlib_root) => {
                Some(stdlib_root.join(relative_path))
            }
            SearchPathInner::StandardLibraryVendored(_) => None,
        }
    }

    #[must_use]
    pub(super) fn to_file(&self, resolver: &ResolverContext) -> Option<File> {
        let db = resolver.db;
        let ModulePath {
            search_path,
            relative_path,
        } = self;
        match &*search_path.0 {
            SearchPathInner::Extra(search_path)
            | SearchPathInner::FirstParty(search_path)
            | SearchPathInner::SitePackages(search_path)
            | SearchPathInner::Editable(search_path) => {
                system_path_to_file(db, search_path.join(relative_path)).ok()
            }
            SearchPathInner::StandardLibraryReal(search_path) => {
                system_path_to_file(db, search_path.join(relative_path)).ok()
            }
            SearchPathInner::StandardLibraryCustom(stdlib_root) => {
                match query_stdlib_version(relative_path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => None,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        system_path_to_file(db, stdlib_root.join(relative_path)).ok()
                    }
                }
            }
            SearchPathInner::StandardLibraryVendored(stdlib_root) => {
                match query_stdlib_version(relative_path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => None,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        vendored_path_to_file(db, stdlib_root.join(relative_path)).ok()
                    }
                }
            }
        }
    }

    #[must_use]
    pub(crate) fn to_module_name(&self) -> Option<ModuleName> {
        fn strip_stubs(component: &str) -> &str {
            component.strip_suffix("-stubs").unwrap_or(component)
        }

        let ModulePath {
            search_path: _,
            relative_path,
        } = self;
        if self.is_standard_library() {
            stdlib_path_to_module_name(relative_path)
        } else {
            let parent = relative_path.parent()?;
            let name = relative_path.file_stem()?;
            if parent.as_str().is_empty() {
                // Stubs should only be stripped when there is no
                // extension. e.g., `foo-stubs` should be stripped
                // by not `foo-stubs.pyi`. In the latter case,
                // `ModuleName::new` will fail (which is what we want).
                return ModuleName::new(if relative_path.extension().is_some() {
                    name
                } else {
                    strip_stubs(relative_path.as_str())
                });
            }

            let parent_components = parent.components().enumerate().map(|(index, component)| {
                let component = component.as_str();

                // For stub packages, strip the `-stubs` suffix from
                // the first component because it isn't a valid module
                // name part AND the module name is the name without
                // the `-stubs`.
                if index == 0 {
                    strip_stubs(component)
                } else {
                    component
                }
            });

            let skip_final_part =
                relative_path.ends_with("__init__.py") || relative_path.ends_with("__init__.pyi");
            if skip_final_part {
                ModuleName::from_components(parent_components)
            } else {
                ModuleName::from_components(parent_components.chain([name]))
            }
        }
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> Self {
        let ModulePath {
            search_path,
            relative_path,
        } = self;
        ModulePath {
            search_path: search_path.clone(),
            relative_path: relative_path.with_extension("pyi"),
        }
    }

    #[must_use]
    pub(crate) fn with_py_extension(&self) -> Option<Self> {
        if self.is_standard_library() {
            return None;
        }
        let ModulePath {
            search_path,
            relative_path,
        } = self;
        Some(ModulePath {
            search_path: search_path.clone(),
            relative_path: relative_path.with_extension("py"),
        })
    }
}

impl PartialEq<SystemPathBuf> for ModulePath {
    fn eq(&self, other: &SystemPathBuf) -> bool {
        let ModulePath {
            search_path,
            relative_path,
        } = self;
        search_path
            .as_system_path()
            .and_then(|search_path| other.strip_prefix(search_path).ok())
            .is_some_and(|other_relative_path| other_relative_path.as_utf8_path() == relative_path)
    }
}

impl PartialEq<ModulePath> for SystemPathBuf {
    fn eq(&self, other: &ModulePath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<VendoredPathBuf> for ModulePath {
    fn eq(&self, other: &VendoredPathBuf) -> bool {
        let ModulePath {
            search_path,
            relative_path,
        } = self;
        search_path
            .as_vendored_path()
            .and_then(|search_path| other.strip_prefix(search_path).ok())
            .is_some_and(|other_relative_path| other_relative_path.as_utf8_path() == relative_path)
    }
}

impl PartialEq<ModulePath> for VendoredPathBuf {
    fn eq(&self, other: &ModulePath) -> bool {
        other.eq(self)
    }
}

#[must_use]
fn stdlib_path_to_module_name(relative_path: &Utf8Path) -> Option<ModuleName> {
    let parent_components = relative_path
        .parent()?
        .components()
        .map(|component| component.as_str());
    let skip_final_part = relative_path.ends_with("__init__.pyi");
    if skip_final_part {
        ModuleName::from_components(parent_components)
    } else {
        ModuleName::from_components(parent_components.chain(relative_path.file_stem()))
    }
}

#[must_use]
fn query_stdlib_version(
    relative_path: &Utf8Path,
    context: &ResolverContext,
) -> TypeshedVersionsQueryResult {
    let Some(module_name) = stdlib_path_to_module_name(relative_path) else {
        return TypeshedVersionsQueryResult::DoesNotExist;
    };
    let ResolverContext {
        db,
        python_version,
        mode: _,
    } = context;

    typeshed_versions(*db).query_module(&module_name, *python_version)
}

#[derive(Debug, thiserror::Error)]
pub enum SearchPathError {
    /// The path provided by the user was not a directory
    #[error("{0} does not point to a directory")]
    NotADirectory(SystemPathBuf),

    /// The path provided by the user is a directory,
    /// but no `stdlib/` subdirectory exists.
    /// (This is only relevant for stdlib search paths.)
    #[error("The directory at {0} has no `stdlib/` subdirectory")]
    NoStdlibSubdirectory(SystemPathBuf),
}

type SearchPathResult<T> = Result<T, SearchPathError>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
enum SearchPathInner {
    Extra(SystemPathBuf),
    FirstParty(SystemPathBuf),
    StandardLibraryCustom(SystemPathBuf),
    StandardLibraryVendored(VendoredPathBuf),
    StandardLibraryReal(SystemPathBuf),
    SitePackages(SystemPathBuf),
    Editable(SystemPathBuf),
}

/// Unification of the various kinds of search paths
/// that can be used to locate Python modules.
///
/// The different kinds of search paths are:
/// - "Extra" search paths: these go at the start of the module resolution order
/// - First-party search paths: the user code that we are directly invoked on.
/// - Standard-library search paths: these come in three different forms:
///   - Custom standard-library search paths: paths provided by the user
///     pointing to a custom typeshed directory on disk
///   - Vendored standard-library search paths: paths pointing to a directory
///     in the vendored zip archive.
///   - Real standard-library search paths: path pointing to a directory
///     of the real python stdlib for the environment.
/// - Site-packages search paths: search paths that point to the `site-packages`
///   directory, in which packages are installed from ``PyPI``.
/// - Editable search paths: Additional search paths added to the end of the module
///   resolution order. We discover these by iterating through `.pth` files in
///   the `site-packages` directory and searching for lines in those `.pth` files
///   that point to existing directories on disk. Such lines indicate editable
///   installations, which will be appended to `sys.path` at runtime,
///   and thus should also be considered valid search paths for our purposes.
///
/// For some of the above categories, there may be an arbitrary number
/// in any given list of search paths: for example, the "Extra" category
/// or the "Editable" category. For the "First-party", "Site-packages"
/// and "Standard-library" categories, however, there will always be exactly
/// one search path from that category in any given list of search paths.
#[derive(Debug, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub struct SearchPath(Arc<SearchPathInner>);

impl SearchPath {
    fn directory_path(system: &dyn System, root: SystemPathBuf) -> SearchPathResult<SystemPathBuf> {
        if system.is_directory(&root) {
            Ok(root)
        } else {
            Err(SearchPathError::NotADirectory(root))
        }
    }

    /// Create a new "Extra" search path
    pub(crate) fn extra(system: &dyn System, root: SystemPathBuf) -> SearchPathResult<Self> {
        Ok(Self(Arc::new(SearchPathInner::Extra(
            Self::directory_path(system, root)?,
        ))))
    }

    /// Create a new first-party search path, pointing to the user code we were directly invoked on
    pub(crate) fn first_party(system: &dyn System, root: SystemPathBuf) -> SearchPathResult<Self> {
        Ok(Self(Arc::new(SearchPathInner::FirstParty(
            Self::directory_path(system, root)?,
        ))))
    }

    /// Create a new standard-library search path pointing to a custom directory on disk
    pub(crate) fn custom_stdlib(
        system: &dyn System,
        typeshed: &SystemPath,
    ) -> SearchPathResult<Self> {
        if !system.is_directory(typeshed) {
            return Err(SearchPathError::NotADirectory(typeshed.to_path_buf()));
        }

        let stdlib =
            Self::directory_path(system, typeshed.join("stdlib")).map_err(|err| match err {
                SearchPathError::NotADirectory(_) => {
                    SearchPathError::NoStdlibSubdirectory(typeshed.to_path_buf())
                }
                SearchPathError::NoStdlibSubdirectory(_) => err,
            })?;

        Ok(Self(Arc::new(SearchPathInner::StandardLibraryCustom(
            stdlib,
        ))))
    }

    /// Create a new search path pointing to the `stdlib/` subdirectory in the vendored zip archive
    #[must_use]
    pub(crate) fn vendored_stdlib() -> Self {
        Self(Arc::new(SearchPathInner::StandardLibraryVendored(
            VendoredPathBuf::from("stdlib"),
        )))
    }

    /// Create a new search path pointing to the real stdlib of a python install
    pub(crate) fn real_stdlib(system: &dyn System, root: SystemPathBuf) -> SearchPathResult<Self> {
        Ok(Self(Arc::new(SearchPathInner::StandardLibraryReal(
            Self::directory_path(system, root)?,
        ))))
    }

    /// Create a new search path pointing to the `site-packages` directory on disk
    ///
    /// TODO: the validation done here is somewhat redundant given that `site-packages`
    /// are already validated at a higher level by the time we get here.
    /// However, removing the validation here breaks some file-watching tests -- and
    /// ultimately we'll probably want all search paths to be validated before a
    /// `Program` is instantiated, so it doesn't seem like a huge priority right now.
    pub(crate) fn site_packages(
        system: &dyn System,
        root: SystemPathBuf,
    ) -> SearchPathResult<Self> {
        Ok(Self(Arc::new(SearchPathInner::SitePackages(
            Self::directory_path(system, root)?,
        ))))
    }

    /// Create a new search path pointing to an editable installation
    pub(crate) fn editable(system: &dyn System, root: SystemPathBuf) -> SearchPathResult<Self> {
        Ok(Self(Arc::new(SearchPathInner::Editable(
            Self::directory_path(system, root)?,
        ))))
    }

    #[must_use]
    pub(crate) fn to_module_path(&self) -> ModulePath {
        ModulePath {
            search_path: self.clone(),
            relative_path: Utf8PathBuf::new(),
        }
    }

    /// Does this search path point to the standard library?
    #[must_use]
    pub fn is_standard_library(&self) -> bool {
        matches!(
            &*self.0,
            SearchPathInner::StandardLibraryCustom(_)
                | SearchPathInner::StandardLibraryVendored(_)
                | SearchPathInner::StandardLibraryReal(_)
        )
    }

    pub fn is_first_party(&self) -> bool {
        matches!(&*self.0, SearchPathInner::FirstParty(_))
    }

    fn is_valid_extension(&self, extension: &str) -> bool {
        if self.is_standard_library() {
            extension == "pyi"
        } else {
            matches!(extension, "pyi" | "py")
        }
    }

    #[must_use]
    pub(crate) fn relativize_system_path(&self, path: &SystemPath) -> Option<ModulePath> {
        self.relativize_system_path_only(path)
            .map(|relative_path| ModulePath {
                search_path: self.clone(),
                relative_path: relative_path.as_utf8_path().to_path_buf(),
            })
    }

    #[must_use]
    pub(crate) fn relativize_system_path_only<'a>(
        &self,
        path: &'a SystemPath,
    ) -> Option<&'a SystemPath> {
        if path
            .extension()
            .is_some_and(|extension| !self.is_valid_extension(extension))
        {
            return None;
        }

        match &*self.0 {
            SearchPathInner::Extra(search_path)
            | SearchPathInner::FirstParty(search_path)
            | SearchPathInner::StandardLibraryCustom(search_path)
            | SearchPathInner::StandardLibraryReal(search_path)
            | SearchPathInner::SitePackages(search_path)
            | SearchPathInner::Editable(search_path) => path.strip_prefix(search_path).ok(),
            SearchPathInner::StandardLibraryVendored(_) => None,
        }
    }

    #[must_use]
    pub(crate) fn relativize_vendored_path(&self, path: &VendoredPath) -> Option<ModulePath> {
        if path
            .extension()
            .is_some_and(|extension| !self.is_valid_extension(extension))
        {
            return None;
        }

        match &*self.0 {
            SearchPathInner::Extra(_)
            | SearchPathInner::FirstParty(_)
            | SearchPathInner::StandardLibraryCustom(_)
            | SearchPathInner::StandardLibraryReal(_)
            | SearchPathInner::SitePackages(_)
            | SearchPathInner::Editable(_) => None,
            SearchPathInner::StandardLibraryVendored(search_path) => path
                .strip_prefix(search_path)
                .ok()
                .map(|relative_path| ModulePath {
                    search_path: self.clone(),
                    relative_path: relative_path.as_utf8_path().to_path_buf(),
                }),
        }
    }

    #[must_use]
    pub(super) fn as_path(&self) -> SystemOrVendoredPathRef<'_> {
        match *self.0 {
            SearchPathInner::Extra(ref path)
            | SearchPathInner::FirstParty(ref path)
            | SearchPathInner::StandardLibraryCustom(ref path)
            | SearchPathInner::StandardLibraryReal(ref path)
            | SearchPathInner::SitePackages(ref path)
            | SearchPathInner::Editable(ref path) => SystemOrVendoredPathRef::System(path),
            SearchPathInner::StandardLibraryVendored(ref path) => {
                SystemOrVendoredPathRef::Vendored(path)
            }
        }
    }

    #[must_use]
    pub fn as_system_path(&self) -> Option<&SystemPath> {
        self.as_path().as_system_path()
    }

    #[must_use]
    pub(crate) fn as_vendored_path(&self) -> Option<&VendoredPath> {
        self.as_path().as_vendored_path()
    }

    /// Returns a succinct string representing the *internal kind* of this
    /// search path. This is useful in snapshot tests where one wants to
    /// capture this specific detail about search paths.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn debug_kind(&self) -> &'static str {
        match *self.0 {
            SearchPathInner::Extra(_) => "extra",
            SearchPathInner::FirstParty(_) => "first-party",
            SearchPathInner::StandardLibraryCustom(_) => "std-custom",
            SearchPathInner::StandardLibraryReal(_) => "std-real",
            SearchPathInner::SitePackages(_) => "site-packages",
            SearchPathInner::Editable(_) => "editable",
            SearchPathInner::StandardLibraryVendored(_) => "std-vendored",
        }
    }

    /// Returns a string suitable for describing what kind of search path this is
    /// in user-facing diagnostics.
    #[must_use]
    pub fn describe_kind(&self) -> &'static str {
        match *self.0 {
            SearchPathInner::Extra(_) => {
                "extra search path specified on the CLI or in your config file"
            }
            SearchPathInner::FirstParty(_) => "first-party code",
            SearchPathInner::StandardLibraryCustom(_) => {
                "custom stdlib stubs specified on the CLI or in your config file"
            }
            SearchPathInner::StandardLibraryReal(_) => "runtime stdlib source code",
            SearchPathInner::SitePackages(_) => "site-packages",
            SearchPathInner::Editable(_) => "editable install",
            SearchPathInner::StandardLibraryVendored(_) => "stdlib typeshed stubs vendored by ty",
        }
    }
}

impl PartialEq<SystemPath> for SearchPath {
    fn eq(&self, other: &SystemPath) -> bool {
        self.as_system_path().is_some_and(|path| path == other)
    }
}

impl PartialEq<SearchPath> for SystemPath {
    fn eq(&self, other: &SearchPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<SystemPathBuf> for SearchPath {
    fn eq(&self, other: &SystemPathBuf) -> bool {
        self.eq(&**other)
    }
}

impl PartialEq<SearchPath> for SystemPathBuf {
    fn eq(&self, other: &SearchPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<VendoredPath> for SearchPath {
    fn eq(&self, other: &VendoredPath) -> bool {
        self.as_vendored_path().is_some_and(|path| path == other)
    }
}

impl PartialEq<SearchPath> for VendoredPath {
    fn eq(&self, other: &SearchPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<VendoredPathBuf> for SearchPath {
    fn eq(&self, other: &VendoredPathBuf) -> bool {
        self.eq(&**other)
    }
}

impl PartialEq<SearchPath> for VendoredPathBuf {
    fn eq(&self, other: &SearchPath) -> bool {
        other.eq(self)
    }
}

impl fmt::Display for SearchPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &*self.0 {
            SearchPathInner::Extra(system_path_buf)
            | SearchPathInner::FirstParty(system_path_buf)
            | SearchPathInner::SitePackages(system_path_buf)
            | SearchPathInner::Editable(system_path_buf)
            | SearchPathInner::StandardLibraryReal(system_path_buf)
            | SearchPathInner::StandardLibraryCustom(system_path_buf) => system_path_buf.fmt(f),
            SearchPathInner::StandardLibraryVendored(vendored_path_buf) => vendored_path_buf.fmt(f),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum SystemOrVendoredPathRef<'db> {
    System(&'db SystemPath),
    Vendored(&'db VendoredPath),
}

impl<'db> SystemOrVendoredPathRef<'db> {
    pub(super) fn try_from_file(db: &'db dyn Db, file: File) -> Option<Self> {
        match file.path(db) {
            FilePath::System(system) => Some(Self::System(system)),
            FilePath::Vendored(vendored) => Some(Self::Vendored(vendored)),
            FilePath::SystemVirtual(_) => None,
        }
    }

    pub(super) fn file_name(&self) -> Option<&str> {
        match self {
            Self::System(system) => system.file_name(),
            Self::Vendored(vendored) => vendored.file_name(),
        }
    }

    pub(super) fn extension(&self) -> Option<&str> {
        match self {
            Self::System(system) => system.extension(),
            Self::Vendored(vendored) => vendored.extension(),
        }
    }

    pub(super) fn parent<'a>(&'a self) -> Option<SystemOrVendoredPathRef<'a>>
    where
        'a: 'db,
    {
        match self {
            Self::System(system) => system.parent().map(Self::System),
            Self::Vendored(vendored) => vendored.parent().map(Self::Vendored),
        }
    }

    fn as_system_path(&self) -> Option<&'db SystemPath> {
        match self {
            SystemOrVendoredPathRef::System(path) => Some(path),
            SystemOrVendoredPathRef::Vendored(_) => None,
        }
    }

    fn as_vendored_path(&self) -> Option<&'db VendoredPath> {
        match self {
            SystemOrVendoredPathRef::Vendored(path) => Some(path),
            SystemOrVendoredPathRef::System(_) => None,
        }
    }
}

impl<'a> From<&'a SystemPath> for SystemOrVendoredPathRef<'a> {
    fn from(path: &'a SystemPath) -> SystemOrVendoredPathRef<'a> {
        SystemOrVendoredPathRef::System(path)
    }
}

impl<'a> From<&'a VendoredPath> for SystemOrVendoredPathRef<'a> {
    fn from(path: &'a VendoredPath) -> SystemOrVendoredPathRef<'a> {
        SystemOrVendoredPathRef::Vendored(path)
    }
}

impl std::fmt::Display for SystemOrVendoredPathRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemOrVendoredPathRef::System(system) => system.fmt(f),
            SystemOrVendoredPathRef::Vendored(vendored) => vendored.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::Db;
    use ruff_python_ast::PythonVersion;

    use crate::db::tests::TestDb;
    use crate::resolve::ModuleResolveMode;
    use crate::testing::{FileSpec, MockedTypeshed, TestCase, TestCaseBuilder};

    use super::*;

    impl ModulePath {
        #[must_use]
        fn join(&self, component: &str) -> ModulePath {
            let mut result = self.clone();
            result.push(component);
            result
        }
    }

    impl SearchPath {
        fn join(&self, component: &str) -> ModulePath {
            self.to_module_path().join(component)
        }
    }

    #[test]
    fn with_extension_methods() {
        let TestCase {
            db, src, stdlib, ..
        } = TestCaseBuilder::new()
            .with_mocked_typeshed(MockedTypeshed::default())
            .build();

        assert_eq!(
            SearchPath::custom_stdlib(db.system(), stdlib.parent().unwrap())
                .unwrap()
                .to_module_path()
                .with_py_extension(),
            None
        );

        assert_eq!(
            &SearchPath::custom_stdlib(db.system(), stdlib.parent().unwrap())
                .unwrap()
                .join("foo")
                .with_pyi_extension(),
            &stdlib.join("foo.pyi")
        );

        assert_eq!(
            &SearchPath::first_party(db.system(), src.clone())
                .unwrap()
                .join("foo/bar")
                .with_py_extension()
                .unwrap(),
            &src.join("foo/bar.py")
        );
    }

    #[test]
    fn module_name_1_part() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let src_search_path = SearchPath::first_party(db.system(), src).unwrap();
        let foo_module_name = ModuleName::new_static("foo").unwrap();

        assert_eq!(
            src_search_path
                .to_module_path()
                .join("foo")
                .to_module_name()
                .as_ref(),
            Some(&foo_module_name)
        );

        assert_eq!(
            src_search_path.join("foo.pyi").to_module_name().as_ref(),
            Some(&foo_module_name)
        );

        assert_eq!(
            src_search_path
                .join("foo/__init__.pyi")
                .to_module_name()
                .as_ref(),
            Some(&foo_module_name)
        );
    }

    #[test]
    fn module_name_2_parts() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let src_search_path = SearchPath::first_party(db.system(), src).unwrap();
        let foo_bar_module_name = ModuleName::new_static("foo.bar").unwrap();

        assert_eq!(
            src_search_path.join("foo/bar").to_module_name().as_ref(),
            Some(&foo_bar_module_name)
        );

        assert_eq!(
            src_search_path
                .join("foo/bar.pyi")
                .to_module_name()
                .as_ref(),
            Some(&foo_bar_module_name)
        );

        assert_eq!(
            src_search_path
                .join("foo/bar/__init__.pyi")
                .to_module_name()
                .as_ref(),
            Some(&foo_bar_module_name)
        );
    }

    #[test]
    fn module_name_3_parts() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let src_search_path = SearchPath::first_party(db.system(), src).unwrap();
        let foo_bar_baz_module_name = ModuleName::new_static("foo.bar.baz").unwrap();

        assert_eq!(
            src_search_path
                .join("foo/bar/baz")
                .to_module_name()
                .as_ref(),
            Some(&foo_bar_baz_module_name)
        );

        assert_eq!(
            src_search_path
                .join("foo/bar/baz.pyi")
                .to_module_name()
                .as_ref(),
            Some(&foo_bar_baz_module_name)
        );

        assert_eq!(
            src_search_path
                .join("foo/bar/baz/__init__.pyi")
                .to_module_name()
                .as_ref(),
            Some(&foo_bar_baz_module_name)
        );
    }

    #[test]
    #[should_panic(expected = "Extension must be `pyi`; got `py`")]
    fn stdlib_path_invalid_join_py() {
        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(MockedTypeshed::default())
            .build();
        SearchPath::custom_stdlib(db.system(), stdlib.parent().unwrap())
            .unwrap()
            .to_module_path()
            .push("bar.py");
    }

    #[test]
    #[should_panic(expected = "Extension must be `pyi`; got `rs`")]
    fn stdlib_path_invalid_join_rs() {
        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(MockedTypeshed::default())
            .build();
        SearchPath::custom_stdlib(db.system(), stdlib.parent().unwrap())
            .unwrap()
            .to_module_path()
            .push("bar.rs");
    }

    #[test]
    #[should_panic(expected = "Extension must be `py` or `pyi`; got `rs`")]
    fn non_stdlib_path_invalid_join_rs() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        SearchPath::first_party(db.system(), src)
            .unwrap()
            .to_module_path()
            .push("bar.rs");
    }

    #[test]
    #[should_panic(expected = "already has an extension")]
    fn too_many_extensions() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        SearchPath::first_party(db.system(), src)
            .unwrap()
            .join("foo.py")
            .push("bar.py");
    }

    #[test]
    fn relativize_stdlib_path_errors() {
        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(MockedTypeshed::default())
            .build();

        let root = SearchPath::custom_stdlib(db.system(), stdlib.parent().unwrap()).unwrap();

        // Must have a `.pyi` extension or no extension:
        let bad_absolute_path = SystemPath::new("foo/stdlib/x.py");
        assert_eq!(root.relativize_system_path(bad_absolute_path), None);
        let second_bad_absolute_path = SystemPath::new("foo/stdlib/x.rs");
        assert_eq!(root.relativize_system_path(second_bad_absolute_path), None);

        // Must be a path that is a child of `root`:
        let third_bad_absolute_path = SystemPath::new("bar/stdlib/x.pyi");
        assert_eq!(root.relativize_system_path(third_bad_absolute_path), None);
    }

    #[test]
    fn relativize_non_stdlib_path_errors() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();

        let root = SearchPath::extra(db.system(), src.clone()).unwrap();
        // Must have a `.py` extension, a `.pyi` extension, or no extension:
        let bad_absolute_path = src.join("x.rs");
        assert_eq!(root.relativize_system_path(&bad_absolute_path), None);
        // Must be a path that is a child of `root`:
        let second_bad_absolute_path = SystemPath::new("bar/src/x.pyi");
        assert_eq!(root.relativize_system_path(second_bad_absolute_path), None);
    }

    #[test]
    fn relativize_path() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let src_search_path = SearchPath::first_party(db.system(), src.clone()).unwrap();
        let eggs_package = src.join("eggs/__init__.pyi");
        let module_path = src_search_path
            .relativize_system_path(&eggs_package)
            .unwrap();
        assert_eq!(
            &module_path.relative_path,
            Utf8Path::new("eggs/__init__.pyi")
        );
    }

    fn typeshed_test_case(
        typeshed: MockedTypeshed,
        python_version: PythonVersion,
    ) -> (TestDb, SearchPath) {
        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_mocked_typeshed(typeshed)
            .with_python_version(python_version)
            .build();
        let stdlib = SearchPath::custom_stdlib(db.system(), stdlib.parent().unwrap()).unwrap();
        (db, stdlib)
    }

    fn py38_typeshed_test_case(typeshed: MockedTypeshed) -> (TestDb, SearchPath) {
        typeshed_test_case(typeshed, PythonVersion::PY38)
    }

    fn py39_typeshed_test_case(typeshed: MockedTypeshed) -> (TestDb, SearchPath) {
        typeshed_test_case(typeshed, PythonVersion::PY39)
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
        let resolver =
            ResolverContext::new(&db, PythonVersion::PY38, ModuleResolveMode::StubsAllowed);

        let asyncio_regular_package = stdlib_path.join("asyncio");
        assert!(asyncio_regular_package.is_directory(&resolver));
        assert!(asyncio_regular_package.is_regular_package(&resolver));
        // Paths to directories don't resolve to VfsFiles
        assert_eq!(asyncio_regular_package.to_file(&resolver), None);
        assert!(
            asyncio_regular_package
                .join("__init__.pyi")
                .to_file(&resolver)
                .is_some()
        );

        // The `asyncio` package exists on Python 3.8, but the `asyncio.tasks` submodule does not,
        // according to the `VERSIONS` file in our typeshed mock:
        let asyncio_tasks_module = stdlib_path.join("asyncio/tasks.pyi");
        assert_eq!(asyncio_tasks_module.to_file(&resolver), None);
        assert!(!asyncio_tasks_module.is_directory(&resolver));
        assert!(!asyncio_tasks_module.is_regular_package(&resolver));
    }

    #[test]
    fn mocked_typeshed_existing_namespace_stdlib_pkg_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "xml: 3.8-3.8",
            stdlib_files: &[("xml/etree.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver =
            ResolverContext::new(&db, PythonVersion::PY38, ModuleResolveMode::StubsAllowed);

        let xml_namespace_package = stdlib_path.join("xml");
        assert!(xml_namespace_package.is_directory(&resolver));
        // Paths to directories don't resolve to VfsFiles
        assert_eq!(xml_namespace_package.to_file(&resolver), None);
        assert!(!xml_namespace_package.is_regular_package(&resolver));

        let xml_etree = stdlib_path.join("xml/etree.pyi");
        assert!(!xml_etree.is_directory(&resolver));
        assert!(xml_etree.to_file(&resolver).is_some());
        assert!(!xml_etree.is_regular_package(&resolver));
    }

    #[test]
    fn mocked_typeshed_single_file_stdlib_module_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver =
            ResolverContext::new(&db, PythonVersion::PY38, ModuleResolveMode::StubsAllowed);

        let functools_module = stdlib_path.join("functools.pyi");
        assert!(functools_module.to_file(&resolver).is_some());
        assert!(!functools_module.is_directory(&resolver));
        assert!(!functools_module.is_regular_package(&resolver));
    }

    #[test]
    fn mocked_typeshed_nonexistent_regular_stdlib_pkg_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "collections: 3.9-",
            stdlib_files: &[("collections/__init__.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver =
            ResolverContext::new(&db, PythonVersion::PY38, ModuleResolveMode::StubsAllowed);

        let collections_regular_package = stdlib_path.join("collections");
        assert_eq!(collections_regular_package.to_file(&resolver), None);
        assert!(!collections_regular_package.is_directory(&resolver));
        assert!(!collections_regular_package.is_regular_package(&resolver));
    }

    #[test]
    fn mocked_typeshed_nonexistent_namespace_stdlib_pkg_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "importlib: 3.9-",
            stdlib_files: &[("importlib/abc.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver =
            ResolverContext::new(&db, PythonVersion::PY38, ModuleResolveMode::StubsAllowed);

        let importlib_namespace_package = stdlib_path.join("importlib");
        assert_eq!(importlib_namespace_package.to_file(&resolver), None);
        assert!(!importlib_namespace_package.is_directory(&resolver));
        assert!(!importlib_namespace_package.is_regular_package(&resolver));

        let importlib_abc = stdlib_path.join("importlib/abc.pyi");
        assert_eq!(importlib_abc.to_file(&resolver), None);
        assert!(!importlib_abc.is_directory(&resolver));
        assert!(!importlib_abc.is_regular_package(&resolver));
    }

    #[test]
    fn mocked_typeshed_nonexistent_single_file_module_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "foo: 2.6-",
            stdlib_files: &[("foo.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver =
            ResolverContext::new(&db, PythonVersion::PY38, ModuleResolveMode::StubsAllowed);

        let non_existent = stdlib_path.join("doesnt_even_exist");
        assert_eq!(non_existent.to_file(&resolver), None);
        assert!(!non_existent.is_directory(&resolver));
        assert!(!non_existent.is_regular_package(&resolver));
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
        let resolver =
            ResolverContext::new(&db, PythonVersion::PY39, ModuleResolveMode::StubsAllowed);

        // Since we've set the target version to Py39,
        // `collections` should now exist as a directory, according to VERSIONS...
        let collections_regular_package = stdlib_path.join("collections");
        assert!(collections_regular_package.is_directory(&resolver));
        assert!(collections_regular_package.is_regular_package(&resolver));
        // (This is still `None`, as directories don't resolve to `Vfs` files)
        assert_eq!(collections_regular_package.to_file(&resolver), None);
        assert!(
            collections_regular_package
                .join("__init__.pyi")
                .to_file(&resolver)
                .is_some()
        );

        // ...and so should the `asyncio.tasks` submodule (though it's still not a directory):
        let asyncio_tasks_module = stdlib_path.join("asyncio/tasks.pyi");
        assert!(asyncio_tasks_module.to_file(&resolver).is_some());
        assert!(!asyncio_tasks_module.is_directory(&resolver));
        assert!(!asyncio_tasks_module.is_regular_package(&resolver));
    }

    #[test]
    fn mocked_typeshed_existing_namespace_stdlib_pkg_py39() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "importlib: 3.9-",
            stdlib_files: &[("importlib/abc.pyi", "")],
        };

        let (db, stdlib_path) = py39_typeshed_test_case(TYPESHED);
        let resolver =
            ResolverContext::new(&db, PythonVersion::PY39, ModuleResolveMode::StubsAllowed);

        // The `importlib` directory now also exists
        let importlib_namespace_package = stdlib_path.join("importlib");
        assert!(importlib_namespace_package.is_directory(&resolver));
        assert!(!importlib_namespace_package.is_regular_package(&resolver));
        // (This is still `None`, as directories don't resolve to `Vfs` files)
        assert_eq!(importlib_namespace_package.to_file(&resolver), None);

        // Submodules in the `importlib` namespace package also now exist:
        let importlib_abc = importlib_namespace_package.join("abc.pyi");
        assert!(!importlib_abc.is_directory(&resolver));
        assert!(!importlib_abc.is_regular_package(&resolver));
        assert!(importlib_abc.to_file(&resolver).is_some());
    }

    #[test]
    fn mocked_typeshed_nonexistent_namespace_stdlib_pkg_py39() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "xml: 3.8-3.8",
            stdlib_files: &[("xml/etree.pyi", "")],
        };

        let (db, stdlib_path) = py39_typeshed_test_case(TYPESHED);
        let resolver =
            ResolverContext::new(&db, PythonVersion::PY39, ModuleResolveMode::StubsAllowed);

        // The `xml` package no longer exists on py39:
        let xml_namespace_package = stdlib_path.join("xml");
        assert_eq!(xml_namespace_package.to_file(&resolver), None);
        assert!(!xml_namespace_package.is_directory(&resolver));
        assert!(!xml_namespace_package.is_regular_package(&resolver));

        let xml_etree = xml_namespace_package.join("etree.pyi");
        assert_eq!(xml_etree.to_file(&resolver), None);
        assert!(!xml_etree.is_directory(&resolver));
        assert!(!xml_etree.is_regular_package(&resolver));
    }

    #[test]
    fn strip_not_top_level_stubs_suffix() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let sp = SearchPath::first_party(db.system(), src).unwrap();
        let mut mp = sp.to_module_path();
        mp.push("foo-stubs");
        mp.push("quux");
        assert_eq!(
            mp.to_module_name(),
            Some(ModuleName::new_static("foo.quux").unwrap())
        );
    }

    /// Tests that a module path of just `foo-stubs` will correctly be
    /// converted to a module name of just `foo`.
    ///
    /// This is a regression test where this conversion ended up
    /// treating the module path as invalid and returning `None` from
    /// `ModulePath::to_module_name` instead.
    #[test]
    fn strip_top_level_stubs_suffix() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let sp = SearchPath::first_party(db.system(), src).unwrap();
        let mut mp = sp.to_module_path();
        mp.push("foo-stubs");
        assert_eq!(
            mp.to_module_name(),
            Some(ModuleName::new_static("foo").unwrap())
        );
    }

    /// Tests that paths like `foo-stubs.pyi` don't have `-stubs`
    /// stripped. (And this leads to failing to create a `ModuleName`,
    /// which is what we want.)
    #[test]
    fn no_strip_with_extension() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let sp = SearchPath::first_party(db.system(), src).unwrap();
        let mut mp = sp.to_module_path();
        mp.push("foo-stubs.pyi");
        assert_eq!(mp.to_module_name(), None);
    }
}
