//! Internal abstractions for differentiating between different kinds of search paths.

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use ruff_db::files::{system_path_to_file, vendored_path_to_file, File, FilePath};
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_db::vendored::{VendoredPath, VendoredPathBuf};

use crate::db::Db;
use crate::module_name::ModuleName;
use crate::state::ResolverState;
use crate::typeshed::{TypeshedVersionsParseError, TypeshedVersionsQueryResult};

/// Inner data for a [`ModulePath`] that points to a location on disk.
///
/// A `SystemModulePathData` is made up of two elements:
/// - The [`SystemSearchPathData`] underlying the [`ModuleSearchPath`]
///   that was used to find this module.
/// - A relative path from the search path to the file
///   that contains the source code of the Python module in question.
///
/// `SystemModulePathData` is generic over a lifetime parameter because
/// it has copy-on-write semantics: it will borrow data without allocating
/// whenever possible.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct SystemModulePathData<'a> {
    search_path: SystemSearchPathData,
    relative_path: Cow<'a, SystemPath>,
}

impl<'a> SystemModulePathData<'a> {
    #[must_use]
    fn to_absolute_path_buf(&self) -> SystemPathBuf {
        let SystemModulePathData {
            search_path: SystemSearchPathData(search_path),
            relative_path,
        } = self;
        search_path.join(relative_path)
    }

    #[must_use]
    fn from_search_path(search_path: SystemSearchPathData) -> SystemModulePathData<'static> {
        SystemModulePathData {
            search_path,
            relative_path: Cow::Owned(SystemPathBuf::new()),
        }
    }

    fn push(&mut self, component: &str) {
        let SystemModulePathData {
            search_path: _,
            relative_path,
        } = self;
        assert!(
            relative_path.extension().is_none(),
            "Cannot push part {component} to {self:?}, which already has an extension"
        );
        relative_path.to_mut().push(component);
    }

    #[must_use]
    fn with_extension(&self, extension: &str) -> SystemModulePathData<'static> {
        let SystemModulePathData {
            search_path,
            relative_path,
        } = self;
        SystemModulePathData {
            search_path: search_path.clone(),
            relative_path: Cow::Owned(relative_path.with_extension(extension)),
        }
    }
}

impl PartialEq<SystemPathBuf> for SystemModulePathData<'_> {
    fn eq(&self, other: &SystemPathBuf) -> bool {
        self.to_absolute_path_buf() == *other
    }
}

impl PartialEq<SystemModulePathData<'_>> for SystemPathBuf {
    fn eq(&self, other: &SystemModulePathData<'_>) -> bool {
        other.eq(self)
    }
}

/// Inner data for a [`ModulePath`] that points to a module that
/// exists in the vendored zip archive.
///
/// A `VendoredModulePathData` is made up of two elements:
/// - The [`VendoredSearchPathData`] underlying the [`ModuleSearchPath`]
///   that was used to find this module.
/// - A relative path from the search path to the file
///   in the zip directory that contains the source code
///   of the Python module in question.
///
/// `VendoredModulePathData` is generic over a lifetime parameter because
/// it has copy-on-write semantics: it will borrow data without allocating
/// whenever possible.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct VendoredModulePathData<'a> {
    search_path: VendoredSearchPathData,
    relative_path: Cow<'a, VendoredPath>,
}

impl<'a> VendoredModulePathData<'a> {
    #[must_use]
    fn to_absolute_path_buf(&self) -> VendoredPathBuf {
        let VendoredModulePathData {
            search_path: VendoredSearchPathData(search_path),
            relative_path,
        } = self;
        search_path.join(relative_path)
    }

    #[must_use]
    fn from_search_path(search_path: VendoredSearchPathData) -> VendoredModulePathData<'static> {
        VendoredModulePathData {
            search_path,
            relative_path: Cow::Owned(VendoredPathBuf::new()),
        }
    }

    fn push(&mut self, component: &str) {
        let VendoredModulePathData {
            search_path: _,
            relative_path,
        } = self;
        assert!(
            relative_path.extension().is_none(),
            "Cannot push part {component} to {self:?}, which already has an extension"
        );
        relative_path.to_mut().push(component);
    }
}

impl PartialEq<VendoredPathBuf> for VendoredModulePathData<'_> {
    fn eq(&self, other: &VendoredPathBuf) -> bool {
        self.to_absolute_path_buf() == *other
    }
}

impl PartialEq<VendoredModulePathData<'_>> for VendoredPathBuf {
    fn eq(&self, other: &VendoredModulePathData<'_>) -> bool {
        other.eq(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ModulePathInner<'a> {
    Extra(SystemModulePathData<'a>),
    FirstParty(SystemModulePathData<'a>),
    StandardLibraryCustom(SystemModulePathData<'a>),
    StandardLibraryVendored(VendoredModulePathData<'a>),
    SitePackages(SystemModulePathData<'a>),
    Editable(SystemModulePathData<'a>),
}

/// A path that points to a Python module.
///
/// `ModulePath` is generic over a lifetime because it has
/// copy-on-write semantics: it will borrow data without
/// allocating whenever possible.
///
/// See [`SystemModulePathData`] and [`VendoredModulePathData`]
/// for more details about the internal representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ModulePath<'a>(ModulePathInner<'a>);

#[must_use]
fn custom_stdlib_path_to_module_name(relative_path: &SystemPath) -> Option<ModuleName> {
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
fn vendored_stdlib_path_to_module_name(relative_path: &VendoredPath) -> Option<ModuleName> {
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
fn query_custom_stdlib_version(
    path: &SystemModulePathData,
    resolver: &ResolverState,
) -> TypeshedVersionsQueryResult {
    let SystemModulePathData {
        search_path,
        relative_path,
    } = path;
    let Some(module_name) = custom_stdlib_path_to_module_name(relative_path) else {
        return TypeshedVersionsQueryResult::DoesNotExist;
    };
    let ResolverState {
        db,
        typeshed_versions,
        target_version,
    } = resolver;
    typeshed_versions.query_module(*db, &module_name, Some(&search_path.0), *target_version)
}

#[must_use]
fn query_vendored_stdlib_version(
    path: &VendoredModulePathData,
    resolver: &ResolverState,
) -> TypeshedVersionsQueryResult {
    let VendoredModulePathData {
        search_path: _,
        relative_path,
    } = path;
    let Some(module_name) = vendored_stdlib_path_to_module_name(relative_path) else {
        return TypeshedVersionsQueryResult::DoesNotExist;
    };
    let ResolverState {
        db,
        typeshed_versions,
        target_version,
    } = resolver;
    typeshed_versions.query_module(*db, &module_name, None, *target_version)
}

impl<'a> ModulePath<'a> {
    #[must_use]
    fn extra(module_path: SystemModulePathData<'a>) -> Self {
        Self(ModulePathInner::Extra(module_path))
    }

    #[must_use]
    fn first_party(module_path: SystemModulePathData<'a>) -> Self {
        Self(ModulePathInner::FirstParty(module_path))
    }

    #[must_use]
    fn custom_stdlib(module_path: SystemModulePathData<'a>) -> Self {
        Self(ModulePathInner::StandardLibraryCustom(module_path))
    }

    #[must_use]
    fn vendored_stdlib(module_path: VendoredModulePathData<'a>) -> Self {
        Self(ModulePathInner::StandardLibraryVendored(module_path))
    }

    #[must_use]
    fn site_packages(module_path: SystemModulePathData<'a>) -> Self {
        Self(ModulePathInner::SitePackages(module_path))
    }

    #[must_use]
    fn editable(module_path: SystemModulePathData<'a>) -> Self {
        Self(ModulePathInner::Editable(module_path))
    }

    #[must_use]
    pub(crate) const fn is_standard_library(&self) -> bool {
        matches!(
            &self.0,
            ModulePathInner::StandardLibraryCustom(_) | ModulePathInner::StandardLibraryVendored(_)
        )
    }

    pub(crate) fn push(&mut self, component: &str) {
        if let Some(component_extension) = camino::Utf8Path::new(component).extension() {
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
        match &mut self.0 {
            ModulePathInner::Extra(path)
            | ModulePathInner::FirstParty(path)
            | ModulePathInner::StandardLibraryCustom(path)
            | ModulePathInner::SitePackages(path)
            | ModulePathInner::Editable(path) => path.push(component),
            ModulePathInner::StandardLibraryVendored(path) => path.push(component),
        }
    }

    #[must_use]
    pub(crate) fn is_directory(&self, resolver: &ResolverState) -> bool {
        match &self.0 {
            ModulePathInner::Extra(path)
            | ModulePathInner::FirstParty(path)
            | ModulePathInner::SitePackages(path)
            | ModulePathInner::Editable(path) => {
                resolver.system().is_directory(&path.to_absolute_path_buf())
            }
            ModulePathInner::StandardLibraryCustom(path) => {
                match query_custom_stdlib_version(path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        resolver.system().is_directory(&path.to_absolute_path_buf())
                    }
                }
            }
            ModulePathInner::StandardLibraryVendored(path) => {
                match query_vendored_stdlib_version(path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => resolver
                        .vendored()
                        .is_directory(path.to_absolute_path_buf()),
                }
            }
        }
    }

    #[must_use]
    pub(crate) fn is_regular_package(&self, resolver: &ResolverState) -> bool {
        match &self.0 {
            ModulePathInner::Extra(path)
            | ModulePathInner::FirstParty(path)
            | ModulePathInner::SitePackages(path)
            | ModulePathInner::Editable(path) => {
                let path = path.to_absolute_path_buf();
                system_path_to_file(resolver.db.upcast(), path.join("__init__.py")).is_some()
                    || system_path_to_file(resolver.db.upcast(), path.join("__init__.py")).is_some()
            }
            ModulePathInner::StandardLibraryCustom(path) => {
                match query_custom_stdlib_version(path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => system_path_to_file(
                        resolver.db.upcast(),
                        path.to_absolute_path_buf().join("__init__.pyi"),
                    )
                    .is_some(),
                }
            }
            ModulePathInner::StandardLibraryVendored(path) => {
                match query_vendored_stdlib_version(path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => false,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => resolver
                        .vendored()
                        .exists(path.to_absolute_path_buf().join("__init__.pyi")),
                }
            }
        }
    }

    #[must_use]
    pub(crate) fn to_file(&self, resolver: &ResolverState) -> Option<File> {
        let db = resolver.db.upcast();
        match &self.0 {
            ModulePathInner::Extra(path)
            | ModulePathInner::FirstParty(path)
            | ModulePathInner::SitePackages(path)
            | ModulePathInner::Editable(path) => {
                system_path_to_file(db, path.to_absolute_path_buf())
            }
            ModulePathInner::StandardLibraryCustom(path) => {
                match query_custom_stdlib_version(path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => None,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        system_path_to_file(db, path.to_absolute_path_buf())
                    }
                }
            }
            ModulePathInner::StandardLibraryVendored(path) => {
                match query_vendored_stdlib_version(path, resolver) {
                    TypeshedVersionsQueryResult::DoesNotExist => None,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        vendored_path_to_file(db, path.to_absolute_path_buf())
                    }
                }
            }
        }
    }

    #[must_use]
    pub(crate) fn to_module_name(&self) -> Option<ModuleName> {
        match &self.0 {
            ModulePathInner::Extra(path)
            | ModulePathInner::FirstParty(path)
            | ModulePathInner::SitePackages(path)
            | ModulePathInner::Editable(path) => {
                let relative_path = &path.relative_path;
                let parent = relative_path.parent()?;
                let parent_components = parent.components().map(|component| component.as_str());
                let skip_final_part = relative_path.ends_with("__init__.py")
                    || relative_path.ends_with("__init__.pyi");
                if skip_final_part {
                    ModuleName::from_components(parent_components)
                } else {
                    ModuleName::from_components(parent_components.chain(relative_path.file_stem()))
                }
            }
            ModulePathInner::StandardLibraryCustom(path) => {
                custom_stdlib_path_to_module_name(&path.relative_path)
            }
            ModulePathInner::StandardLibraryVendored(path) => {
                vendored_stdlib_path_to_module_name(&path.relative_path)
            }
        }
    }

    #[must_use]
    pub(crate) fn with_pyi_extension(&self) -> ModulePath<'static> {
        match &self.0 {
            ModulePathInner::Extra(path) => ModulePath::extra(path.with_extension("pyi")),
            ModulePathInner::FirstParty(path) => {
                ModulePath::first_party(path.with_extension("pyi"))
            }
            ModulePathInner::StandardLibraryCustom(path) => {
                ModulePath::custom_stdlib(path.with_extension("pyi"))
            }
            ModulePathInner::SitePackages(path) => {
                ModulePath::site_packages(path.with_extension("pyi"))
            }
            ModulePathInner::Editable(path) => ModulePath::editable(path.with_extension("pyi")),
            ModulePathInner::StandardLibraryVendored(VendoredModulePathData {
                search_path,
                relative_path,
            }) => ModulePath::vendored_stdlib(VendoredModulePathData {
                search_path: search_path.clone(),
                relative_path: Cow::Owned(relative_path.with_pyi_extension()),
            }),
        }
    }

    #[must_use]
    pub(crate) fn with_py_extension(&self) -> Option<ModulePath<'static>> {
        let inner = match &self.0 {
            ModulePathInner::Extra(path) => ModulePathInner::Extra(path.with_extension("py")),
            ModulePathInner::FirstParty(path) => {
                ModulePathInner::FirstParty(path.with_extension("py"))
            }
            ModulePathInner::SitePackages(path) => {
                ModulePathInner::SitePackages(path.with_extension("py"))
            }
            ModulePathInner::Editable(path) => ModulePathInner::Editable(path.with_extension("py")),
            ModulePathInner::StandardLibraryCustom(_) => return None,
            ModulePathInner::StandardLibraryVendored(_) => return None,
        };
        Some(ModulePath(inner))
    }
}

impl PartialEq<SystemPathBuf> for ModulePath<'_> {
    fn eq(&self, other: &SystemPathBuf) -> bool {
        match &self.0 {
            ModulePathInner::Extra(path)
            | ModulePathInner::FirstParty(path)
            | ModulePathInner::StandardLibraryCustom(path)
            | ModulePathInner::SitePackages(path)
            | ModulePathInner::Editable(path) => path == other,
            ModulePathInner::StandardLibraryVendored(_) => false,
        }
    }
}

impl PartialEq<ModulePath<'_>> for SystemPathBuf {
    fn eq(&self, other: &ModulePath<'_>) -> bool {
        other.eq(self)
    }
}

impl PartialEq<VendoredPathBuf> for ModulePath<'_> {
    fn eq(&self, other: &VendoredPathBuf) -> bool {
        match &self.0 {
            ModulePathInner::StandardLibraryVendored(path) => path == other,
            ModulePathInner::Extra(_)
            | ModulePathInner::FirstParty(_)
            | ModulePathInner::StandardLibraryCustom(_)
            | ModulePathInner::SitePackages(_)
            | ModulePathInner::Editable(_) => false,
        }
    }
}

impl PartialEq<ModulePath<'_>> for VendoredPathBuf {
    fn eq(&self, other: &ModulePath<'_>) -> bool {
        other.eq(self)
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

type SearchPathResult<T> = Result<T, SearchPathValidationError>;

/// Inner data for a search path that points to a location on disk
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SystemSearchPathData(Arc<SystemPathBuf>);

impl SystemSearchPathData {
    fn new(system: &dyn System, root: &SystemPath) -> SearchPathResult<SystemSearchPathData> {
        if system.is_directory(root) {
            Ok(Self(Arc::new(SystemPath::absolute(
                root,
                system.current_directory(),
            ))))
        } else {
            Err(SearchPathValidationError::NotADirectory(root.to_path_buf()))
        }
    }

    fn relativize_path<'a>(
        &self,
        absolute_path: &'a SystemPath,
    ) -> Option<SystemModulePathData<'a>> {
        absolute_path
            .strip_prefix(&*self.0)
            .map(|relative_path| SystemModulePathData {
                search_path: self.clone(),
                relative_path: Cow::Borrowed(relative_path),
            })
            .ok()
    }
}

/// Inner data for a search path that points to a directory in the vendored zip archive
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VendoredSearchPathData(Arc<VendoredPathBuf>);

impl VendoredSearchPathData {
    fn relativize_path<'a>(
        &self,
        absolute_path: &'a VendoredPath,
    ) -> Option<VendoredModulePathData<'a>> {
        absolute_path
            .strip_prefix(&*self.0)
            .map(|relative_path| VendoredModulePathData {
                search_path: self.clone(),
                relative_path: Cow::Borrowed(relative_path),
            })
            .ok()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum SearchPathInner {
    Extra(SystemSearchPathData),
    FirstParty(SystemSearchPathData),
    StandardLibraryCustom(SystemSearchPathData),
    StandardLibraryVendored(VendoredSearchPathData),
    SitePackages(SystemSearchPathData),
    Editable(SystemSearchPathData),
}

/// Unification of the various kinds of search paths
/// that can be used to locate Python modules.
///
/// The different kinds of search paths are:
/// - "Extra" search paths: these go at the start of the module resolution order
/// - First-party search paths: the user code that we are directly invoked on.
/// - Standard-library search paths: these come in two different forms:
///   - Custom standard-library search paths: paths provided by the user
///     pointing to a custom typeshed directory on disk
///   - Vendored standard-library search paths: paths pointing to a directory
///     in the vendored zip archive.
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct SearchPath(SearchPathInner);

impl SearchPath {
    /// Create a new "Extra" search path
    pub(crate) fn extra(
        system: &dyn System,
        root: impl AsRef<SystemPath>,
    ) -> SearchPathResult<Self> {
        SystemSearchPathData::new(system, root.as_ref())
            .map(|path| Self(SearchPathInner::Extra(path)))
    }

    /// Create a new first-party search path, pointing to the user code we were directly invoked on
    pub(crate) fn first_party(
        system: &dyn System,
        root: impl AsRef<SystemPath>,
    ) -> SearchPathResult<Self> {
        SystemSearchPathData::new(system, root.as_ref())
            .map(|path| Self(SearchPathInner::FirstParty(path)))
    }

    /// Create a new standard-library search path pointing to a custom directory on disk
    pub(crate) fn custom_stdlib(
        db: &dyn Db,
        typeshed: impl AsRef<SystemPath>,
    ) -> SearchPathResult<Self> {
        let typeshed = typeshed.as_ref();
        let system = db.system();
        if !system.is_directory(typeshed) {
            return Err(SearchPathValidationError::NotADirectory(
                typeshed.to_path_buf(),
            ));
        }
        let stdlib = typeshed.join("stdlib");
        if !system.is_directory(&stdlib) {
            return Err(SearchPathValidationError::NoStdlibSubdirectory(
                typeshed.to_path_buf(),
            ));
        }
        let Some(typeshed_versions) = system_path_to_file(db.upcast(), stdlib.join("VERSIONS"))
        else {
            return Err(SearchPathValidationError::NoVersionsFile(
                typeshed.to_path_buf(),
            ));
        };
        crate::typeshed::parse_typeshed_versions(db, typeshed_versions)
            .as_ref()
            .map_err(|validation_error| {
                SearchPathValidationError::VersionsParseError(validation_error.clone())
            })?;
        Ok(Self(SearchPathInner::StandardLibraryCustom(
            SystemSearchPathData(Arc::new(stdlib)),
        )))
    }

    /// Create a new search path pointing to the `stdlib/` subdirectory in the vendored zip archive
    #[must_use]
    pub(crate) fn vendored_stdlib() -> Self {
        Self(SearchPathInner::StandardLibraryVendored(
            VendoredSearchPathData(Arc::new(VendoredPathBuf::from("stdlib"))),
        ))
    }

    /// Create a new search path pointing to the `site-packages` directory on disk
    pub(crate) fn site_packages(
        system: &dyn System,
        root: impl AsRef<SystemPath>,
    ) -> SearchPathResult<Self> {
        SystemSearchPathData::new(system, root.as_ref())
            .map(|path| Self(SearchPathInner::SitePackages(path)))
    }

    /// Create a new search path pointing to an editable installation
    pub(crate) fn editable(
        system: &dyn System,
        root: impl AsRef<SystemPath>,
    ) -> SearchPathResult<Self> {
        SystemSearchPathData::new(system, root.as_ref())
            .map(|path| Self(SearchPathInner::Editable(path)))
    }

    #[must_use]
    pub(crate) fn to_module_path(&self) -> ModulePath<'static> {
        match &self.0 {
            SearchPathInner::Extra(search_path) => {
                ModulePath::extra(SystemModulePathData::from_search_path(search_path.clone()))
            }
            SearchPathInner::FirstParty(search_path) => {
                ModulePath::first_party(SystemModulePathData::from_search_path(search_path.clone()))
            }
            SearchPathInner::StandardLibraryCustom(search_path) => ModulePath::custom_stdlib(
                SystemModulePathData::from_search_path(search_path.clone()),
            ),
            SearchPathInner::StandardLibraryVendored(search_path) => ModulePath::vendored_stdlib(
                VendoredModulePathData::from_search_path(search_path.clone()),
            ),
            SearchPathInner::SitePackages(search_path) => ModulePath::site_packages(
                SystemModulePathData::from_search_path(search_path.clone()),
            ),
            SearchPathInner::Editable(search_path) => {
                ModulePath::editable(SystemModulePathData::from_search_path(search_path.clone()))
            }
        }
    }

    /// Does this search path point to the standard library?
    #[must_use]
    pub(crate) const fn is_standard_library(&self) -> bool {
        matches!(
            &self.0,
            SearchPathInner::StandardLibraryCustom(_) | SearchPathInner::StandardLibraryVendored(_)
        )
    }

    /// Does this search path point to the `site-packages` directory?
    #[must_use]
    pub(crate) const fn is_site_packages(&self) -> bool {
        matches!(&self.0, SearchPathInner::SitePackages(_))
    }

    #[must_use]
    pub(crate) fn relativize_path<'a>(&self, path: &'a FilePath) -> Option<ModulePath<'a>> {
        let extension = path.extension();

        if self.is_standard_library() {
            if extension.is_some_and(|extension| extension != "pyi") {
                return None;
            }
        } else {
            if extension.is_some_and(|extension| !matches!(extension, "pyi" | "py")) {
                return None;
            }
        }

        match (&self.0, path) {
            (SearchPathInner::Extra(search_path), FilePath::System(absolute_path)) => search_path
                .relativize_path(absolute_path)
                .map(ModulePath::extra),
            (SearchPathInner::Extra(_), FilePath::Vendored(_)) => None,
            (SearchPathInner::FirstParty(search_path), FilePath::System(absolute_path)) => {
                search_path
                    .relativize_path(absolute_path)
                    .map(ModulePath::first_party)
            }
            (SearchPathInner::FirstParty(_), FilePath::Vendored(_)) => None,
            (
                SearchPathInner::StandardLibraryCustom(search_path),
                FilePath::System(absolute_path),
            ) => search_path
                .relativize_path(absolute_path)
                .map(ModulePath::custom_stdlib),
            (SearchPathInner::StandardLibraryCustom(_), FilePath::Vendored(_)) => None,
            (
                SearchPathInner::StandardLibraryVendored(search_path),
                FilePath::Vendored(absolute_path),
            ) => search_path
                .relativize_path(absolute_path)
                .map(ModulePath::vendored_stdlib),
            (SearchPathInner::StandardLibraryVendored(_), FilePath::System(_)) => None,
            (SearchPathInner::SitePackages(search_path), FilePath::System(absolute_path)) => {
                search_path
                    .relativize_path(absolute_path)
                    .map(ModulePath::site_packages)
            }
            (SearchPathInner::SitePackages(_), FilePath::Vendored(_)) => None,
            (SearchPathInner::Editable(search_path), FilePath::System(absolute_path)) => {
                search_path
                    .relativize_path(absolute_path)
                    .map(ModulePath::editable)
            }
            (SearchPathInner::Editable(_), FilePath::Vendored(_)) => None,
        }
    }

    #[must_use]
    pub(crate) fn as_system_path_buf(&self) -> Option<&SystemPathBuf> {
        match &self.0 {
            SearchPathInner::Extra(path)
            | SearchPathInner::FirstParty(path)
            | SearchPathInner::StandardLibraryCustom(path)
            | SearchPathInner::SitePackages(path)
            | SearchPathInner::Editable(path) => Some(&*path.0),
            SearchPathInner::StandardLibraryVendored(_) => None,
        }
    }

    #[must_use]
    pub(crate) fn as_vendored_path_buf(&self) -> Option<&VendoredPathBuf> {
        match &self.0 {
            SearchPathInner::StandardLibraryVendored(path) => Some(&*path.0),
            SearchPathInner::Extra(_)
            | SearchPathInner::FirstParty(_)
            | SearchPathInner::StandardLibraryCustom(_)
            | SearchPathInner::SitePackages(_)
            | SearchPathInner::Editable(_) => None,
        }
    }
}

impl PartialEq<SystemPathBuf> for SearchPath {
    fn eq(&self, other: &SystemPathBuf) -> bool {
        self.as_system_path_buf().is_some_and(|path| path == other)
    }
}

impl PartialEq<SearchPath> for SystemPathBuf {
    fn eq(&self, other: &SearchPath) -> bool {
        other.eq(self)
    }
}

impl PartialEq<VendoredPathBuf> for SearchPath {
    fn eq(&self, other: &VendoredPathBuf) -> bool {
        self.as_vendored_path_buf()
            .is_some_and(|path| path == other)
    }
}

impl PartialEq<SearchPath> for VendoredPathBuf {
    fn eq(&self, other: &SearchPath) -> bool {
        other.eq(self)
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::program::TargetVersion;
    use ruff_db::Db;

    use crate::db::tests::TestDb;
    use crate::testing::{FileSpec, MockedTypeshed, TestCase, TestCaseBuilder};

    use super::*;

    impl<'a> ModulePath<'a> {
        #[must_use]
        fn join(&self, component: &'a (impl AsRef<SystemPath> + ?Sized)) -> ModulePath<'a> {
            let mut result = self.clone();
            result.push(component.as_ref().as_str());
            result
        }
    }

    impl SearchPath {
        #[must_use]
        pub(crate) fn is_stdlib_search_path(&self) -> bool {
            matches!(
                &self.0,
                SearchPathInner::StandardLibraryCustom(_)
                    | SearchPathInner::StandardLibraryVendored(_)
            )
        }

        fn join<'a>(&'a self, component: &'a str) -> ModulePath<'a> {
            self.to_module_path().join(component)
        }
    }

    #[test]
    fn with_extension_methods() {
        let TestCase {
            db, src, stdlib, ..
        } = TestCaseBuilder::new()
            .with_custom_typeshed(MockedTypeshed::default())
            .build();

        assert_eq!(
            SearchPath::custom_stdlib(&db, stdlib.parent().unwrap())
                .unwrap()
                .to_module_path()
                .with_py_extension(),
            None
        );

        assert_eq!(
            &SearchPath::custom_stdlib(&db, stdlib.parent().unwrap())
                .unwrap()
                .join("foo")
                .with_pyi_extension(),
            &stdlib.join("foo.pyi")
        );

        assert_eq!(
            &SearchPath::first_party(db.system(), &src)
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
            .with_custom_typeshed(MockedTypeshed::default())
            .build();
        SearchPath::custom_stdlib(&db, stdlib.parent().unwrap())
            .unwrap()
            .to_module_path()
            .push("bar.py");
    }

    #[test]
    #[should_panic(expected = "Extension must be `pyi`; got `rs`")]
    fn stdlib_path_invalid_join_rs() {
        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_custom_typeshed(MockedTypeshed::default())
            .build();
        SearchPath::custom_stdlib(&db, stdlib.parent().unwrap())
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
            .with_custom_typeshed(MockedTypeshed::default())
            .build();

        let root = SearchPath::custom_stdlib(&db, stdlib.parent().unwrap()).unwrap();

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
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();

        let root = SearchPath::extra(db.system(), &src).unwrap();
        // Must have a `.py` extension, a `.pyi` extension, or no extension:
        let bad_absolute_path = FilePath::System(src.join("x.rs"));
        assert_eq!(root.relativize_path(&bad_absolute_path), None);
        // Must be a path that is a child of `root`:
        let second_bad_absolute_path = FilePath::system("bar/src/x.pyi");
        assert_eq!(root.relativize_path(&second_bad_absolute_path), None);
    }

    #[test]
    fn relativize_path() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let src_search_path = SearchPath::first_party(db.system(), &src).unwrap();
        let eggs_package = FilePath::System(src.join("eggs/__init__.pyi"));

        let ModulePath(ModulePathInner::FirstParty(path)) =
            src_search_path.relativize_path(&eggs_package).unwrap()
        else {
            panic!()
        };

        assert_eq!(&*path.relative_path, SystemPath::new("eggs/__init__.pyi"));
    }

    fn typeshed_test_case(
        typeshed: MockedTypeshed,
        target_version: TargetVersion,
    ) -> (TestDb, SearchPath) {
        let TestCase { db, stdlib, .. } = TestCaseBuilder::new()
            .with_custom_typeshed(typeshed)
            .with_target_version(target_version)
            .build();
        let stdlib = SearchPath::custom_stdlib(&db, stdlib.parent().unwrap()).unwrap();
        (db, stdlib)
    }

    fn py38_typeshed_test_case(typeshed: MockedTypeshed) -> (TestDb, SearchPath) {
        typeshed_test_case(typeshed, TargetVersion::Py38)
    }

    fn py39_typeshed_test_case(typeshed: MockedTypeshed) -> (TestDb, SearchPath) {
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
        assert!(asyncio_regular_package.is_directory(&resolver));
        assert!(asyncio_regular_package.is_regular_package(&resolver));
        // Paths to directories don't resolve to VfsFiles
        assert_eq!(asyncio_regular_package.to_file(&resolver), None);
        assert!(asyncio_regular_package
            .join("__init__.pyi")
            .to_file(&resolver)
            .is_some());

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
        let resolver = ResolverState::new(&db, TargetVersion::Py38);

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
        let resolver = ResolverState::new(&db, TargetVersion::Py38);

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
        let resolver = ResolverState::new(&db, TargetVersion::Py38);

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
        let resolver = ResolverState::new(&db, TargetVersion::Py38);

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
        let resolver = ResolverState::new(&db, TargetVersion::Py38);

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
        let resolver = ResolverState::new(&db, TargetVersion::Py39);

        // Since we've set the target version to Py39,
        // `collections` should now exist as a directory, according to VERSIONS...
        let collections_regular_package = stdlib_path.join("collections");
        assert!(collections_regular_package.is_directory(&resolver));
        assert!(collections_regular_package.is_regular_package(&resolver));
        // (This is still `None`, as directories don't resolve to `Vfs` files)
        assert_eq!(collections_regular_package.to_file(&resolver), None);
        assert!(collections_regular_package
            .join("__init__.pyi")
            .to_file(&resolver)
            .is_some());

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
        let resolver = ResolverState::new(&db, TargetVersion::Py39);

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
        let resolver = ResolverState::new(&db, TargetVersion::Py39);

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
}
