//! Internal abstractions for differentiating between different kinds of search paths.

use std::fmt;
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use ruff_db::files::{
    DirectoryListing, File, FilePath, directory_listing, system_path_to_file, vendored_path_to_file,
};
use ruff_db::source::source_text;
use ruff_db::system::{FileType, System, SystemPath, SystemPathBuf};
use ruff_db::vendored::{VendoredPath, VendoredPathBuf};

use crate::Db;
use crate::module_name::ModuleName;
use crate::resolve::{PyTyped, ResolverContext};
use crate::typeshed::TypeshedVersionsQueryResult;

/// An immutable path describing a possible Python module.
///
/// Combines a [`SearchPath`] with a relative path on disk or in the vendored archive.
/// Converting it to a module name does not check whether that module exists or whether
/// another location shadows it. [`ModuleDirectory`] handles filesystem access;
/// the resolver selects the module for a name.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ModulePath {
    search_path: SearchPath,
    relative_path: Utf8PathBuf,
}

impl ModulePath {
    /// Derives a candidate module name without performing filesystem access or resolution.
    #[must_use]
    pub(crate) fn to_module_name(&self) -> Option<ModuleName> {
        fn strip_stubs(component: &str) -> &str {
            component.strip_suffix("-stubs").unwrap_or(component)
        }

        let ModulePath {
            search_path: _,
            relative_path,
        } = self;
        if self.search_path.is_standard_library_stub() {
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

/// A location in which to look up or enumerate Python modules.
///
/// Owns directory navigation, filesystem probes, and the cached system listing.
/// A location may be missing or inaccessible; constructing it does not establish
/// that it is a package or select it over other search locations.
#[derive(Debug, Clone)]
pub(crate) struct ModuleDirectory<'db> {
    search_path: SearchPath,
    relative_path: Utf8PathBuf,
    // The contents of the directory.
    listing: Option<&'db DirectoryListing>,
}

impl<'db> ModuleDirectory<'db> {
    /// Opens the directory at the root of a search path.
    pub(crate) fn new(context: &ResolverContext<'db>, search_path: SearchPath) -> Self {
        Self::from_parts(context, search_path, Utf8PathBuf::new())
    }

    /// Returns an existing child directory and retrieves its listing without
    /// changing this directory.
    ///
    /// `name` should be a single path component.
    pub(crate) fn child_directory(
        &self,
        context: &ResolverContext<'db>,
        name: &str,
    ) -> Option<Self> {
        let relative_path = self.relative_path.join(name);
        Self::exists_at(context, &self.search_path, &relative_path)
            .then(|| Self::from_parts(context, self.search_path.clone(), relative_path))
    }

    /// Returns the search root containing this directory.
    pub(crate) fn search_path(&self) -> &SearchPath {
        &self.search_path
    }

    /// Consumes the directory and returns its search root.
    pub(crate) fn into_search_path(self) -> SearchPath {
        self.search_path
    }

    /// Returns the directory's system path, if it is not vendored.
    pub(crate) fn to_system_path(&self) -> Option<SystemPathBuf> {
        self.search_path
            .as_system_path()
            .map(|root| root.join(&self.relative_path))
    }

    /// Returns the directory's path within the vendored filesystem, if applicable.
    pub(crate) fn to_vendored_path(&self) -> Option<VendoredPathBuf> {
        self.search_path
            .as_vendored_path()
            .map(|root| root.join(&self.relative_path))
    }

    /// Returns the cached listing from [`System`], or `None` for an inaccessible directory
    /// or a path in the vendored typeshed archive.
    pub(crate) fn system_listing(&self) -> Option<&'db DirectoryListing> {
        self.listing
    }

    /// Returns whether or not this directory might contain the given entry.
    pub(crate) fn may_contain_name(&self, name: &str) -> bool {
        self.search_path.as_vendored_path().is_some()
            || self
                .listing
                .is_some_and(|listing| listing.contains_name_with_prefix(name))
    }

    /// Checks a relative directory path without reading that directory's contents.
    ///
    /// System paths use their parent's listing; vendored and custom typeshed paths
    /// also respect the configured Python version.
    pub(crate) fn exists_at(
        context: &ResolverContext,
        search_path: &SearchPath,
        relative_path: &Utf8Path,
    ) -> bool {
        if search_path.is_standard_library_stub()
            && matches!(
                query_stdlib_version(relative_path, context),
                TypeshedVersionsQueryResult::DoesNotExist
            )
        {
            return false;
        }

        match search_path.as_path() {
            SystemOrVendoredPathRef::System(root) => {
                system_path_is_directory(context.db, &root.join(relative_path))
            }
            SystemOrVendoredPathRef::Vendored(root) => {
                context.vendored().is_directory(root.join(relative_path))
            }
        }
    }

    /// Returns this package's `py.typed` information without considering parent packages.
    pub(crate) fn py_typed(&self, context: &ResolverContext) -> PyTyped {
        let Some(py_typed_file) = self.to_system_path().and_then(|path| {
            if !self
                .listing
                .is_some_and(|listing| listing.entry_is_file(context.db, &path, "py.typed"))
            {
                return None;
            }
            system_path_to_file(context.db, path.join("py.typed")).ok()
        }) else {
            return PyTyped::Untyped;
        };

        // Different module names revisit the same package. Share the tracked contents instead of
        // reading its marker from disk again for every module resolution.
        let py_typed_contents = source_text(context.db, py_typed_file);
        // If we fail to read it let's say that's like it doesn't exist
        // (right now the difference between Untyped and Full is academic)
        if py_typed_contents.read_error().is_some() {
            return PyTyped::Untyped;
        }

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

    /// Resolves one exact filename in this directory, validating file status,
    /// symlink targets, and typeshed availability for the configured Python version.
    pub(crate) fn resolve_file(&self, context: &ResolverContext, filename: &str) -> Option<File> {
        let path = Utf8Path::new(filename);

        // Verify that the provided `filename` input identifies just the final filename
        // component of a path (i.e., plain `tools.py` instead of, say, `pkg/tools.py`).
        if path.file_name() != Some(filename) {
            return None;
        }

        // Do not resolve runtime modules for standard library stubs.
        if path.extension() == Some("py") && self.search_path.is_standard_library_stub() {
            return None;
        }

        // Verify that the directory listing contains a file or symlink entry
        // with the correct name. Symlink targets and vendored paths are
        // validated further down.
        if self.search_path.as_vendored_path().is_none()
            && !matches!(
                self.listing.and_then(|listing| listing.file_type(filename)),
                Some(FileType::File | FileType::Symlink)
            )
        {
            return None;
        }

        let relative_path = self.relative_path.join(filename);

        match &*self.search_path.0 {
            SearchPathInner::Extra(root)
            | SearchPathInner::FirstParty(root)
            | SearchPathInner::SitePackages(root)
            | SearchPathInner::Editable(root)
            | SearchPathInner::StandardLibraryReal(root) => {
                system_path_to_file(context.db, root.join(&relative_path)).ok()
            }
            SearchPathInner::StandardLibraryCustom(root) => {
                match query_stdlib_version(&relative_path, context) {
                    TypeshedVersionsQueryResult::DoesNotExist => None,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        system_path_to_file(context.db, root.join(&relative_path)).ok()
                    }
                }
            }
            SearchPathInner::StandardLibraryVendored(root) => {
                match query_stdlib_version(&relative_path, context) {
                    TypeshedVersionsQueryResult::DoesNotExist => None,
                    TypeshedVersionsQueryResult::Exists
                    | TypeshedVersionsQueryResult::MaybeExists => {
                        vendored_path_to_file(context.db, root.join(&relative_path)).ok()
                    }
                }
            }
        }
    }

    fn from_parts(
        context: &ResolverContext<'db>,
        search_path: SearchPath,
        relative_path: Utf8PathBuf,
    ) -> Self {
        let listing = search_path
            .as_system_path()
            .and_then(|root| directory_listing(context.db, &root.join(&relative_path)).ok());
        Self {
            search_path,
            relative_path,
            listing,
        }
    }
}

fn system_path_is_directory(db: &dyn Db, path: &SystemPath) -> bool {
    let Some((parent, name)) = path.parent().zip(path.file_name()) else {
        return db.system().is_directory(path);
    };

    directory_listing(db, parent).is_ok_and(|listing| listing.entry_is_directory(db, parent, name))
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
    context
        .resolver_environment
        .search_paths(context.db)
        .typeshed_versions()
        .query_module(
            &module_name,
            context.resolver_environment.python_version(context.db),
        )
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

    fn is_standard_library_stub(&self) -> bool {
        matches!(
            &*self.0,
            SearchPathInner::StandardLibraryCustom(_) | SearchPathInner::StandardLibraryVendored(_)
        )
    }

    /// Is this a user-provided extra search path?
    pub(crate) fn is_extra(&self) -> bool {
        matches!(&*self.0, SearchPathInner::Extra(_))
    }

    /// Is this search path in "first party" code? i.e., The
    /// end user's project code.
    pub fn is_first_party(&self) -> bool {
        matches!(&*self.0, SearchPathInner::FirstParty(_))
    }

    /// Is the module in a site-packages directory?
    pub fn is_site_packages(&self) -> bool {
        matches!(&*self.0, SearchPathInner::SitePackages(_))
    }

    /// Is this search path provided by an editable installation?
    pub fn is_editable(&self) -> bool {
        matches!(&*self.0, SearchPathInner::Editable(_))
    }

    /// Is it plausible that this search path contains third-party code?
    pub(crate) fn can_contain_third_party_code(&self) -> bool {
        match &*self.0 {
            SearchPathInner::SitePackages(_)
            | SearchPathInner::Editable(_)
            | SearchPathInner::Extra(_) => true,
            SearchPathInner::FirstParty(_)
            | SearchPathInner::StandardLibraryCustom(_)
            | SearchPathInner::StandardLibraryVendored(_)
            | SearchPathInner::StandardLibraryReal(_) => false,
        }
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
    fn as_path(&self) -> SystemOrVendoredPathRef<'_> {
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
    pub(crate) fn as_system_path(&self) -> Option<&SystemPath> {
        self.as_path().as_system_path()
    }

    #[must_use]
    fn as_vendored_path(&self) -> Option<&VendoredPath> {
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
    use ruff_db::system::{DbWithTestSystem as _, DbWithWritableSystem as _, OsSystem};
    use ruff_python_ast::PythonVersion;

    use crate::ResolverEnvironment;
    use crate::db::tests::TestDb;
    use crate::resolve::ModuleResolveMode;
    use crate::testing::{FileSpec, MockedTypeshed, TestCase, TestCaseBuilder};

    use super::*;

    #[test]
    fn resolve_file_rejects_runtime_files_in_typeshed() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "foo: 3.12-",
            stdlib_files: &[(
                "foo.py",
                r#"
value = 1
"#,
            )],
        };
        let (db, stdlib_path) = typeshed_test_case(TYPESHED, PythonVersion::PY312);
        let resolver = ResolverContext::new(
            &db,
            ResolverEnvironment::new(&db, PythonVersion::PY312, db.search_paths()),
            ModuleResolveMode::Typing,
        );
        let directory = ModuleDirectory::new(&resolver, stdlib_path);

        assert_eq!(directory.resolve_file(&resolver, "foo.py"), None);
    }

    #[test]
    fn resolve_file_requires_exact_filename_case() {
        let temp_dir = tempfile::TempDir::new().expect("Create temporary directory");
        let root = SystemPathBuf::from_path_buf(
            temp_dir
                .path()
                .canonicalize()
                .expect("Canonicalize temporary directory"),
        )
        .expect("UTF-8 temporary directory path");
        let mut db = TestDb::new();
        db.use_system(OsSystem::new(&root));
        db.write_file(
            root.join("Tools.py"),
            r#"
value = 1
"#,
        )
        .expect("Write Tools.py");

        let resolver = ResolverContext::new(
            &db,
            ResolverEnvironment::new(&db, PythonVersion::PY312, db.search_paths()),
            ModuleResolveMode::Typing,
        );
        let search_path =
            SearchPath::first_party(db.system(), root.clone()).expect("Existing source directory");
        let directory = ModuleDirectory::new(&resolver, search_path);

        // Require exact filename casing even when the filesystem is case-insensitive.
        assert_eq!(directory.resolve_file(&resolver, "tools.py"), None);
        let file = directory
            .resolve_file(&resolver, "Tools.py")
            .expect("Resolve the exact filename");
        assert_eq!(file.path(&db), &root.join("Tools.py"));
    }

    #[test]
    fn child_directory_preserves_parent_and_reads_typing_marker() {
        let TestCase { db, src, .. } = TestCaseBuilder::new()
            .with_src_files(&[
                (
                    "package/tools.py",
                    r#"
value = 1
"#,
                ),
                (
                    "package/py.typed",
                    r#"
partial
"#,
                ),
            ])
            .build();
        let context =
            ResolverContext::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
        let search_path =
            SearchPath::first_party(db.system(), src.clone()).expect("Existing source directory");
        let parent = ModuleDirectory::new(&context, search_path);
        let child = parent
            .child_directory(&context, "package")
            .expect("Existing package directory");

        assert_eq!(parent.to_system_path(), Some(src.clone()));
        assert_eq!(parent.py_typed(&context), PyTyped::Untyped);
        assert_eq!(child.py_typed(&context), PyTyped::Partial);
        let file = child
            .resolve_file(&context, "tools.py")
            .expect("Resolve a file in the child directory");
        assert_eq!(file.path(&db), &src.join("package/tools.py"));
    }

    #[test]
    fn module_path_does_not_require_existing_module() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let search_path =
            SearchPath::first_party(db.system(), src.clone()).expect("Existing source directory");
        let path = search_path
            .relativize_system_path(&src.join("missing.pyi"))
            .expect("Path beneath the search root");
        assert_eq!(path.to_module_name(), ModuleName::new("missing"));

        let context =
            ResolverContext::new(&db, db.resolver_environment(), ModuleResolveMode::Typing);
        let directory = ModuleDirectory::new(&context, search_path);
        assert!(directory.resolve_file(&context, "missing.pyi").is_none());
    }

    #[test]
    fn module_name_1_part() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let src_search_path = SearchPath::first_party(db.system(), src).unwrap();
        let foo_module_name = ModuleName::new_static("foo").unwrap();

        assert_eq!(
            src_search_path.join("foo").to_module_name().as_ref(),
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
        let resolver = ResolverContext::new(
            &db,
            ResolverEnvironment::new(&db, PythonVersion::PY38, db.search_paths()),
            ModuleResolveMode::Typing,
        );

        let asyncio_regular_package = stdlib_path.join("asyncio");
        assert!(ModuleDirectory::exists_at(
            &resolver,
            &asyncio_regular_package.search_path,
            &asyncio_regular_package.relative_path,
        ));
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
        assert!(!ModuleDirectory::exists_at(
            &resolver,
            &asyncio_tasks_module.search_path,
            &asyncio_tasks_module.relative_path,
        ));
    }

    #[test]
    fn mocked_typeshed_existing_namespace_stdlib_pkg_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "xml: 3.8-3.8",
            stdlib_files: &[("xml/etree.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver = ResolverContext::new(
            &db,
            ResolverEnvironment::new(&db, PythonVersion::PY38, db.search_paths()),
            ModuleResolveMode::Typing,
        );

        let xml_namespace_package = stdlib_path.join("xml");
        assert!(ModuleDirectory::exists_at(
            &resolver,
            &xml_namespace_package.search_path,
            &xml_namespace_package.relative_path,
        ));
        // Paths to directories don't resolve to VfsFiles
        assert_eq!(xml_namespace_package.to_file(&resolver), None);

        let xml_etree = stdlib_path.join("xml/etree.pyi");
        assert!(!ModuleDirectory::exists_at(
            &resolver,
            &xml_etree.search_path,
            &xml_etree.relative_path,
        ));
        assert!(xml_etree.to_file(&resolver).is_some());
    }

    #[test]
    fn mocked_typeshed_single_file_stdlib_module_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "functools: 3.8-",
            stdlib_files: &[("functools.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver = ResolverContext::new(
            &db,
            ResolverEnvironment::new(&db, PythonVersion::PY38, db.search_paths()),
            ModuleResolveMode::Typing,
        );

        let functools_module = stdlib_path.join("functools.pyi");
        assert!(functools_module.to_file(&resolver).is_some());
        assert!(!ModuleDirectory::exists_at(
            &resolver,
            &functools_module.search_path,
            &functools_module.relative_path,
        ));
    }

    #[test]
    fn mocked_typeshed_nonexistent_regular_stdlib_pkg_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "collections: 3.9-",
            stdlib_files: &[("collections/__init__.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver = ResolverContext::new(
            &db,
            ResolverEnvironment::new(&db, PythonVersion::PY38, db.search_paths()),
            ModuleResolveMode::Typing,
        );

        let collections_regular_package = stdlib_path.join("collections");
        assert_eq!(collections_regular_package.to_file(&resolver), None);
        assert!(!ModuleDirectory::exists_at(
            &resolver,
            &collections_regular_package.search_path,
            &collections_regular_package.relative_path,
        ));
    }

    #[test]
    fn mocked_typeshed_nonexistent_namespace_stdlib_pkg_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "importlib: 3.9-",
            stdlib_files: &[("importlib/abc.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver = ResolverContext::new(
            &db,
            ResolverEnvironment::new(&db, PythonVersion::PY38, db.search_paths()),
            ModuleResolveMode::Typing,
        );

        let importlib_namespace_package = stdlib_path.join("importlib");
        assert_eq!(importlib_namespace_package.to_file(&resolver), None);
        assert!(!ModuleDirectory::exists_at(
            &resolver,
            &importlib_namespace_package.search_path,
            &importlib_namespace_package.relative_path,
        ));

        let importlib_abc = stdlib_path.join("importlib/abc.pyi");
        assert_eq!(importlib_abc.to_file(&resolver), None);
        assert!(!ModuleDirectory::exists_at(
            &resolver,
            &importlib_abc.search_path,
            &importlib_abc.relative_path,
        ));
    }

    #[test]
    fn mocked_typeshed_nonexistent_single_file_module_py38() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "foo: 2.6-",
            stdlib_files: &[("foo.pyi", "")],
        };

        let (db, stdlib_path) = py38_typeshed_test_case(TYPESHED);
        let resolver = ResolverContext::new(
            &db,
            ResolverEnvironment::new(&db, PythonVersion::PY38, db.search_paths()),
            ModuleResolveMode::Typing,
        );

        let non_existent = stdlib_path.join("doesnt_even_exist");
        assert_eq!(non_existent.to_file(&resolver), None);
        assert!(!ModuleDirectory::exists_at(
            &resolver,
            &non_existent.search_path,
            &non_existent.relative_path,
        ));
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
        let resolver = ResolverContext::new(
            &db,
            ResolverEnvironment::new(&db, PythonVersion::PY39, db.search_paths()),
            ModuleResolveMode::Typing,
        );

        // Since we've set the target version to Py39,
        // `collections` should now exist as a directory, according to VERSIONS...
        let collections_regular_package = stdlib_path.join("collections");
        assert!(ModuleDirectory::exists_at(
            &resolver,
            &collections_regular_package.search_path,
            &collections_regular_package.relative_path,
        ));
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
        assert!(!ModuleDirectory::exists_at(
            &resolver,
            &asyncio_tasks_module.search_path,
            &asyncio_tasks_module.relative_path,
        ));
    }

    #[test]
    fn mocked_typeshed_existing_namespace_stdlib_pkg_py39() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "importlib: 3.9-",
            stdlib_files: &[("importlib/abc.pyi", "")],
        };

        let (db, stdlib_path) = py39_typeshed_test_case(TYPESHED);
        let resolver = ResolverContext::new(
            &db,
            ResolverEnvironment::new(&db, PythonVersion::PY39, db.search_paths()),
            ModuleResolveMode::Typing,
        );

        // The `importlib` directory now also exists
        let importlib_namespace_package = stdlib_path.join("importlib");
        assert!(ModuleDirectory::exists_at(
            &resolver,
            &importlib_namespace_package.search_path,
            &importlib_namespace_package.relative_path,
        ));
        // (This is still `None`, as directories don't resolve to `Vfs` files)
        assert_eq!(importlib_namespace_package.to_file(&resolver), None);

        // Submodules in the `importlib` namespace package also now exist:
        let importlib_abc = importlib_namespace_package.join("abc.pyi");
        assert!(!ModuleDirectory::exists_at(
            &resolver,
            &importlib_abc.search_path,
            &importlib_abc.relative_path,
        ));
        assert!(importlib_abc.to_file(&resolver).is_some());
    }

    #[test]
    fn mocked_typeshed_nonexistent_namespace_stdlib_pkg_py39() {
        const TYPESHED: MockedTypeshed = MockedTypeshed {
            versions: "xml: 3.8-3.8",
            stdlib_files: &[("xml/etree.pyi", "")],
        };

        let (db, stdlib_path) = py39_typeshed_test_case(TYPESHED);
        let resolver = ResolverContext::new(
            &db,
            ResolverEnvironment::new(&db, PythonVersion::PY39, db.search_paths()),
            ModuleResolveMode::Typing,
        );

        // The `xml` package no longer exists on py39:
        let xml_namespace_package = stdlib_path.join("xml");
        assert_eq!(xml_namespace_package.to_file(&resolver), None);
        assert!(!ModuleDirectory::exists_at(
            &resolver,
            &xml_namespace_package.search_path,
            &xml_namespace_package.relative_path,
        ));

        let xml_etree = xml_namespace_package.join("etree.pyi");
        assert_eq!(xml_etree.to_file(&resolver), None);
        assert!(!ModuleDirectory::exists_at(
            &resolver,
            &xml_etree.search_path,
            &xml_etree.relative_path,
        ));
    }

    #[test]
    fn strip_not_top_level_stubs_suffix() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let search_path = SearchPath::first_party(db.system(), src).unwrap();
        let module_path = search_path.join("foo-stubs/quux");
        assert_eq!(
            module_path.to_module_name(),
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
        let search_path = SearchPath::first_party(db.system(), src).unwrap();
        let module_path = search_path.join("foo-stubs");
        assert_eq!(
            module_path.to_module_name(),
            Some(ModuleName::new_static("foo").unwrap())
        );
    }

    /// Tests that paths like `foo-stubs.pyi` don't have `-stubs`
    /// stripped. (And this leads to failing to create a `ModuleName`,
    /// which is what we want.)
    #[test]
    fn no_strip_with_extension() {
        let TestCase { db, src, .. } = TestCaseBuilder::new().build();
        let search_path = SearchPath::first_party(db.system(), src).unwrap();
        let module_path = search_path.join("foo-stubs.pyi");
        assert_eq!(module_path.to_module_name(), None);
    }

    impl ModulePath {
        fn join(&self, component: &str) -> Self {
            Self {
                search_path: self.search_path.clone(),
                relative_path: self.relative_path.join(component),
            }
        }

        /// Resolves this path through the same directory validation used by the resolver.
        fn to_file(&self, context: &ResolverContext) -> Option<File> {
            let filename = self.relative_path.file_name()?;
            let parent = self.relative_path.parent()?;
            let directory = ModuleDirectory::from_parts(
                context,
                self.search_path.clone(),
                parent.to_path_buf(),
            );

            directory.resolve_file(context, filename)
        }
    }

    impl SearchPath {
        fn join(&self, relative_path: &str) -> ModulePath {
            ModulePath {
                search_path: self.clone(),
                relative_path: Utf8PathBuf::from(relative_path),
            }
        }
    }
}
