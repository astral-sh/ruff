use std::collections::{BTreeMap, HashMap};
use std::fmt::Formatter;
use std::hash::{BuildHasherDefault, Hash};
use std::ops::{Deref, RangeFrom, RangeInclusive};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;

use anyhow::{bail, Context};
use dashmap::mapref::entry::Entry;
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use ruff_python_stdlib::identifiers::is_identifier;

use crate::db::{QueryResult, SemanticDb, SemanticJar};
use crate::files::FileId;
use crate::semantic::Dependency;
use crate::FxDashMap;

/// Representation of a Python module.
///
/// The inner type wrapped by this struct is a unique identifier for the module
/// that is used by the struct's methods to lazily query information about the module.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct Module(u32);

impl Module {
    /// Return the absolute name of the module (e.g. `foo.bar`)
    pub fn name(&self, db: &dyn SemanticDb) -> QueryResult<ModuleName> {
        let jar: &SemanticJar = db.jar()?;
        let modules = &jar.module_resolver;

        Ok(modules.modules.get(self).unwrap().name.clone())
    }

    /// Return the path to the source code that defines this module
    pub fn path(&self, db: &dyn SemanticDb) -> QueryResult<ModulePath> {
        let jar: &SemanticJar = db.jar()?;
        let modules = &jar.module_resolver;

        Ok(modules.modules.get(self).unwrap().path.clone())
    }

    /// Determine whether this module is a single-file module or a package
    pub fn kind(&self, db: &dyn SemanticDb) -> QueryResult<ModuleKind> {
        let jar: &SemanticJar = db.jar()?;
        let modules = &jar.module_resolver;

        Ok(modules.modules.get(self).unwrap().kind)
    }

    /// Attempt to resolve a dependency of this module to an absolute [`ModuleName`].
    ///
    /// A dependency could be either absolute (e.g. the `foo` dependency implied by `from foo import bar`)
    /// or relative to this module (e.g. the `.foo` dependency implied by `from .foo import bar`)
    ///
    /// - Returns an error if the query failed.
    /// - Returns `Ok(None)` if the query succeeded,
    ///   but the dependency refers to a module that does not exist.
    /// - Returns `Ok(Some(ModuleName))` if the query succeeded,
    ///   and the dependency refers to a module that exists.
    pub fn resolve_dependency(
        &self,
        db: &dyn SemanticDb,
        dependency: &Dependency,
    ) -> QueryResult<Option<ModuleName>> {
        let (level, module) = match dependency {
            Dependency::Module(module) => return Ok(Some(module.clone())),
            Dependency::Relative { level, module } => (*level, module.as_deref()),
        };

        let name = self.name(db)?;
        let kind = self.kind(db)?;

        let mut components = name.components().peekable();

        let start = match kind {
            // `.` resolves to the enclosing package
            ModuleKind::Module => 0,
            // `.` resolves to the current package
            ModuleKind::Package => 1,
        };

        // Skip over the relative parts.
        for _ in start..level.get() {
            if components.next_back().is_none() {
                return Ok(None);
            }
        }

        let mut name = String::new();

        for part in components.chain(module) {
            if !name.is_empty() {
                name.push('.');
            }

            name.push_str(part);
        }

        Ok(if name.is_empty() {
            None
        } else {
            Some(ModuleName(SmolStr::new(name)))
        })
    }
}

/// A module name, e.g. `foo.bar`.
///
/// Always normalized to the absolute form
/// (never a relative module name, i.e., never `.foo`).
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct ModuleName(smol_str::SmolStr);

impl ModuleName {
    pub fn new(name: &str) -> Self {
        debug_assert!(!name.is_empty());
        let instance = Self(smol_str::SmolStr::new(name));
        debug_assert!(instance.components().all(is_identifier));
        instance
    }

    fn from_relative_path(path: &Path) -> Option<Self> {
        let path = if path.ends_with("__init__.py") || path.ends_with("__init__.pyi") {
            path.parent()?
        } else {
            path
        };

        let name = if let Some(parent) = path.parent() {
            let mut name = String::with_capacity(path.as_os_str().len());

            for component in parent.components() {
                name.push_str(component.as_os_str().to_str()?);
                name.push('.');
            }

            // SAFETY: Unwrap is safe here or `parent` would have returned `None`.
            name.push_str(path.file_stem().unwrap().to_str()?);

            smol_str::SmolStr::from(name)
        } else {
            smol_str::SmolStr::new(path.file_stem()?.to_str()?)
        };

        Some(Self(name))
    }

    /// An iterator over the components of the module name:
    /// `foo.bar.baz` -> `foo`, `bar`, `baz`
    pub fn components(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.0.split('.')
    }

    /// The name of this module's immediate parent, if it has a parent
    pub fn parent(&self) -> Option<ModuleName> {
        let (_, parent) = self.0.rsplit_once('.')?;

        Some(Self(smol_str::SmolStr::new(parent)))
    }

    pub fn starts_with(&self, other: &ModuleName) -> bool {
        self.0.starts_with(other.0.as_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for ModuleName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl From<&str> for ModuleName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ModuleKind {
    /// A single-file module (e.g. `foo.py` or `foo.pyi`)
    Module,

    /// A python package (`foo/__init__.py` or `foo/__init__.pyi`)
    Package,
}

/// A search path in which to search modules.
/// Corresponds to a path in [`sys.path`](https://docs.python.org/3/library/sys_path_init.html) at runtime.
///
/// Cloning a search path is cheap because it's an `Arc`.
#[derive(Clone, PartialEq, Eq)]
pub struct ModuleSearchPath {
    inner: Arc<ModuleSearchPathInner>,
}

impl ModuleSearchPath {
    pub fn new(path: PathBuf, kind: ModuleSearchPathKind) -> Self {
        Self {
            inner: Arc::new(ModuleSearchPathInner { path, kind }),
        }
    }

    /// Determine whether this is a first-party, third-party or standard-library search path
    pub fn kind(&self) -> ModuleSearchPathKind {
        self.inner.kind
    }

    /// Return the location of the search path on the file system
    pub fn path(&self) -> &Path {
        &self.inner.path
    }
}

impl std::fmt::Debug for ModuleSearchPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct ModuleSearchPathInner {
    path: PathBuf,
    kind: ModuleSearchPathKind,
}

/// Enumeration of the different kinds of search paths type checkers are expected to support.
///
/// N.B. Although we don't implement `Ord` for this enum, they are ordered in terms of the
/// priority that we want to give these modules when resolving them.
/// This is roughly [the order given in the typing spec], but typeshed's stubs
/// for the standard library are moved higher up to match Python's semantics at runtime.
///
/// [the order given in the typing spec]: https://typing.readthedocs.io/en/latest/spec/distributing.html#import-resolution-ordering
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, is_macro::Is)]
pub enum ModuleSearchPathKind {
    /// "Extra" paths provided by the user in a config file, env var or CLI flag.
    /// E.g. mypy's `MYPYPATH` env var, or pyright's `stubPath` configuration setting
    Extra,

    /// Files in the project we're directly being invoked on
    FirstParty,

    /// The `stdlib` directory of typeshed (either vendored or custom)
    StandardLibrary,

    /// Stubs or runtime modules installed in site-packages
    SitePackagesThirdParty,

    /// Vendored third-party stubs from typeshed
    VendoredThirdParty,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ModuleData {
    name: ModuleName,
    path: ModulePath,
    kind: ModuleKind,
}

//////////////////////////////////////////////////////
// Queries
//////////////////////////////////////////////////////

/// Resolves a module name to a module.
///
/// TODO: This would not work with Salsa because `ModuleName` isn't an ingredient
/// and, therefore, cannot be used as part of a query.
/// For this to work with salsa, it would be necessary to intern all `ModuleName`s.
#[tracing::instrument(level = "debug", skip(db))]
pub fn resolve_module(db: &dyn SemanticDb, name: ModuleName) -> QueryResult<Option<Module>> {
    let jar: &SemanticJar = db.jar()?;
    let modules = &jar.module_resolver;

    let entry = modules.by_name.entry(name.clone());

    match entry {
        Entry::Occupied(entry) => Ok(Some(*entry.get())),
        Entry::Vacant(entry) => {
            let Some((root_path, absolute_path, kind)) = resolve_name(&name, &modules.search_paths)
            else {
                return Ok(None);
            };
            let Ok(normalized) = absolute_path.canonicalize() else {
                return Ok(None);
            };

            let file_id = db.file_id(&normalized);
            let path = ModulePath::new(root_path.clone(), file_id);

            let module = Module(
                modules
                    .next_module_id
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            );

            modules
                .modules
                .insert(module, Arc::from(ModuleData { name, path, kind }));

            // A path can map to multiple modules because of symlinks:
            // ```
            // foo.py
            // bar.py -> foo.py
            // ```
            // Here, both `foo` and `bar` resolve to the same module but through different paths.
            // That's why we need to insert the absolute path and not the normalized path here.
            let absolute_file_id = if absolute_path == normalized {
                file_id
            } else {
                db.file_id(&absolute_path)
            };

            modules.by_file.insert(absolute_file_id, module);

            entry.insert_entry(module);

            Ok(Some(module))
        }
    }
}

/// Resolves the module for the given path.
///
/// Returns `None` if the path is not a module locatable via `sys.path`.
#[tracing::instrument(level = "debug", skip(db))]
pub fn path_to_module(db: &dyn SemanticDb, path: &Path) -> QueryResult<Option<Module>> {
    let file = db.file_id(path);
    file_to_module(db, file)
}

/// Resolves the module for the file with the given id.
///
/// Returns `None` if the file is not a module locatable via `sys.path`.
#[tracing::instrument(level = "debug", skip(db))]
pub fn file_to_module(db: &dyn SemanticDb, file: FileId) -> QueryResult<Option<Module>> {
    let jar: &SemanticJar = db.jar()?;
    let modules = &jar.module_resolver;

    if let Some(existing) = modules.by_file.get(&file) {
        return Ok(Some(*existing));
    }

    let path = db.file_path(file);

    debug_assert!(path.is_absolute());

    let Some((root_path, relative_path)) = modules.search_paths.iter().find_map(|root| {
        let relative_path = path.strip_prefix(root.path()).ok()?;
        Some((root.clone(), relative_path))
    }) else {
        return Ok(None);
    };

    let Some(module_name) = ModuleName::from_relative_path(relative_path) else {
        return Ok(None);
    };

    // Resolve the module name to see if Python would resolve the name to the same path.
    // If it doesn't, then that means that multiple modules have the same in different
    // root paths, but that the module corresponding to the past path is in a lower priority search path,
    // in which case we ignore it.
    let Some(module) = resolve_module(db, module_name)? else {
        return Ok(None);
    };
    let module_path = module.path(db)?;

    if module_path.root() == &root_path {
        let Ok(normalized) = path.canonicalize() else {
            return Ok(None);
        };
        let interned_normalized = db.file_id(&normalized);

        if interned_normalized != module_path.file() {
            // This path is for a module with the same name but with a different precedence. For example:
            // ```
            // src/foo.py
            // src/foo/__init__.py
            // ```
            // The module name of `src/foo.py` is `foo`, but the module loaded by Python is `src/foo/__init__.py`.
            // That means we need to ignore `src/foo.py` even though it resolves to the same module name.
            return Ok(None);
        }

        // Path has been inserted by `resolved`
        Ok(Some(module))
    } else {
        // This path is for a module with the same name but in a module search path with a lower priority.
        // Ignore it.
        Ok(None)
    }
}

//////////////////////////////////////////////////////
// Mutations
//////////////////////////////////////////////////////

/// Changes the module search paths to `search_paths`.
pub fn set_module_search_paths(db: &mut dyn SemanticDb, search_paths: ModuleResolutionInputs) {
    let jar: &mut SemanticJar = db.jar_mut();

    jar.module_resolver = ModuleResolver::new(search_paths.into_ordered_search_paths());
}

/// Struct for holding the various paths that are put together
/// to create an `OrderedSearchPatsh` instance
///
/// - `extra_paths` is a list of user-provided paths
///   that should take first priority in the module resolution.
///   Examples in other type checkers are mypy's MYPYPATH environment variable,
///   or pyright's stubPath configuration setting.
/// - `workspace_root` is the root of the workspace,
///   used for finding first-party modules
/// - `site-packages` is the path to the user's `site-packages` directory,
///   where third-party packages from ``PyPI`` are installed
/// - `custom_typeshed` is a path to standard-library typeshed stubs.
///   Currently this has to be a directory that exists on disk.
///   (TODO: fall back to vendored stubs if no custom directory is provided.)
#[derive(Debug)]
pub struct ModuleResolutionInputs {
    pub extra_paths: Vec<PathBuf>,
    pub workspace_root: PathBuf,
    pub site_packages: Option<PathBuf>,
    pub custom_typeshed: Option<PathBuf>,
}

impl ModuleResolutionInputs {
    /// Implementation of PEP 561's module resolution order
    /// (with some small, deliberate, differences)
    fn into_ordered_search_paths(self) -> OrderedSearchPaths {
        let ModuleResolutionInputs {
            extra_paths,
            workspace_root,
            site_packages,
            custom_typeshed,
        } = self;

        OrderedSearchPaths(
            extra_paths
                .into_iter()
                .map(|path| ModuleSearchPath::new(path, ModuleSearchPathKind::Extra))
                .chain(std::iter::once(ModuleSearchPath::new(
                    workspace_root,
                    ModuleSearchPathKind::FirstParty,
                )))
                // TODO fallback to vendored typeshed stubs if no custom typeshed directory is provided by the user
                .chain(custom_typeshed.into_iter().map(|path| {
                    ModuleSearchPath::new(
                        path.join(TYPESHED_STDLIB_DIRECTORY),
                        ModuleSearchPathKind::StandardLibrary,
                    )
                }))
                .chain(site_packages.into_iter().map(|path| {
                    ModuleSearchPath::new(path, ModuleSearchPathKind::SitePackagesThirdParty)
                }))
                // TODO vendor typeshed's third-party stubs as well as the stdlib and fallback to them as a final step
                .collect(),
        )
    }
}

const TYPESHED_STDLIB_DIRECTORY: &str = "stdlib";

/// A resolved module resolution order, implementing PEP 561
/// (with some small, deliberate differences)
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct OrderedSearchPaths(Vec<ModuleSearchPath>);

impl Deref for OrderedSearchPaths {
    type Target = [ModuleSearchPath];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Adds a module located at `path` to the resolver.
///
/// Returns `None` if the path doesn't resolve to a module.
///
/// Returns `Some(module, other_modules)`, where `module` is the resolved module
/// with file location `path`, and `other_modules` is a `Vec` of `ModuleData` instances.
/// Each element in `other_modules` provides information regarding a single module that needs
/// re-resolving because it was part of a namespace package and might now resolve differently.
///
/// Note: This won't work with salsa because `Path` is not an ingredient.
pub fn add_module(db: &mut dyn SemanticDb, path: &Path) -> Option<(Module, Vec<Arc<ModuleData>>)> {
    // No locking is required because we're holding a mutable reference to `modules`.

    // TODO This needs tests

    // Note: Intentionally bypass caching here. Module should not be in the cache yet.
    let module = path_to_module(db, path).ok()??;

    // The code below is to handle the addition of `__init__.py` files.
    // When an `__init__.py` file is added, we need to remove all modules that are part of the same package.
    // For example, an `__init__.py` is added to `foo`, we need to remove `foo.bar`, `foo.baz`, etc.
    // because they were namespace packages before and could have been from different search paths.
    let Some(filename) = path.file_name() else {
        return Some((module, Vec::new()));
    };

    if !matches!(filename.to_str(), Some("__init__.py" | "__init__.pyi")) {
        return Some((module, Vec::new()));
    }

    let Some(parent_name) = module.name(db).ok()?.parent() else {
        return Some((module, Vec::new()));
    };

    let mut to_remove = Vec::new();

    let jar: &mut SemanticJar = db.jar_mut();
    let modules = &mut jar.module_resolver;

    modules.by_file.retain(|_, module| {
        if modules
            .modules
            .get(module)
            .unwrap()
            .name
            .starts_with(&parent_name)
        {
            to_remove.push(*module);
            false
        } else {
            true
        }
    });

    // TODO remove need for this vec
    let mut removed = Vec::with_capacity(to_remove.len());
    for module in &to_remove {
        removed.push(modules.remove_module(*module));
    }

    Some((module, removed))
}

#[derive(Default)]
pub struct ModuleResolver {
    /// The search paths where modules are located (and searched). Corresponds to `sys.path` at runtime.
    search_paths: OrderedSearchPaths,

    // Locking: Locking is done by acquiring a (write) lock on `by_name`. This is because `by_name` is the primary
    // lookup method. Acquiring locks in any other ordering can result in deadlocks.
    /// Looks up a module by name
    by_name: FxDashMap<ModuleName, Module>,

    /// A map of all known modules to data about those modules
    modules: FxDashMap<Module, Arc<ModuleData>>,

    /// Lookup from absolute path to module.
    /// The same module might be reachable from different paths when symlinks are involved.
    by_file: FxDashMap<FileId, Module>,
    next_module_id: AtomicU32,
}

impl ModuleResolver {
    fn new(search_paths: OrderedSearchPaths) -> Self {
        Self {
            search_paths,
            modules: FxDashMap::default(),
            by_name: FxDashMap::default(),
            by_file: FxDashMap::default(),
            next_module_id: AtomicU32::new(0),
        }
    }

    /// Remove a module from the inner cache
    pub(crate) fn remove_module_by_file(&mut self, file_id: FileId) {
        // No locking is required because we're holding a mutable reference to `self`.
        let Some((_, module)) = self.by_file.remove(&file_id) else {
            return;
        };

        self.remove_module(module);
    }

    fn remove_module(&mut self, module: Module) -> Arc<ModuleData> {
        let (_, module_data) = self.modules.remove(&module).unwrap();

        self.by_name.remove(&module_data.name).unwrap();

        // It's possible that multiple paths map to the same module.
        // Search all other paths referencing the same module.
        self.by_file
            .retain(|_, current_module| *current_module != module);

        module_data
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for ModuleResolver {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleResolver")
            .field("search_paths", &self.search_paths)
            .field("modules", &self.by_name)
            .finish()
    }
}

/// The resolved path of a module.
///
/// It should be highly likely that the file still exists when accessing but it isn't 100% guaranteed
/// because the file could have been deleted between resolving the module name and accessing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModulePath {
    root: ModuleSearchPath,
    file_id: FileId,
}

impl ModulePath {
    pub fn new(root: ModuleSearchPath, file_id: FileId) -> Self {
        Self { root, file_id }
    }

    /// The search path that was used to locate the module
    pub fn root(&self) -> &ModuleSearchPath {
        &self.root
    }

    /// The file containing the source code for the module
    pub fn file(&self) -> FileId {
        self.file_id
    }
}

/// Given a module name and a list of search paths in which to lookup modules,
/// attempt to resolve the module name
fn resolve_name(
    name: &ModuleName,
    search_paths: &[ModuleSearchPath],
) -> Option<(ModuleSearchPath, PathBuf, ModuleKind)> {
    for search_path in search_paths {
        let mut components = name.components();
        let module_name = components.next_back()?;

        match resolve_package(search_path, components) {
            Ok(resolved_package) => {
                let mut package_path = resolved_package.path;

                package_path.push(module_name);

                // Must be a `__init__.pyi` or `__init__.py` or it isn't a package.
                let kind = if package_path.is_dir() {
                    package_path.push("__init__");
                    ModuleKind::Package
                } else {
                    ModuleKind::Module
                };

                // TODO Implement full https://peps.python.org/pep-0561/#type-checker-module-resolution-order resolution
                let stub = package_path.with_extension("pyi");

                if stub.is_file() {
                    return Some((search_path.clone(), stub, kind));
                }

                let module = package_path.with_extension("py");

                if module.is_file() {
                    return Some((search_path.clone(), module, kind));
                }

                // For regular packages, don't search the next search path. All files of that
                // package must be in the same location
                if resolved_package.kind.is_regular_package() {
                    return None;
                }
            }
            Err(parent_kind) => {
                if parent_kind.is_regular_package() {
                    // For regular packages, don't search the next search path.
                    return None;
                }
            }
        }
    }

    None
}

fn resolve_package<'a, I>(
    module_search_path: &ModuleSearchPath,
    components: I,
) -> Result<ResolvedPackage, PackageKind>
where
    I: Iterator<Item = &'a str>,
{
    let mut package_path = module_search_path.path().to_path_buf();

    // `true` if inside a folder that is a namespace package (has no `__init__.py`).
    // Namespace packages are special because they can be spread across multiple search paths.
    // https://peps.python.org/pep-0420/
    let mut in_namespace_package = false;

    // `true` if resolving a sub-package. For example, `true` when resolving `bar` of `foo.bar`.
    let mut in_sub_package = false;

    // For `foo.bar.baz`, test that `foo` and `baz` both contain a `__init__.py`.
    for folder in components {
        package_path.push(folder);

        let has_init_py = package_path.join("__init__.py").is_file()
            || package_path.join("__init__.pyi").is_file();

        if has_init_py {
            in_namespace_package = false;
        } else if package_path.is_dir() {
            // A directory without an `__init__.py` is a namespace package, continue with the next folder.
            in_namespace_package = true;
        } else if in_namespace_package {
            // Package not found but it is part of a namespace package.
            return Err(PackageKind::Namespace);
        } else if in_sub_package {
            // A regular sub package wasn't found.
            return Err(PackageKind::Regular);
        } else {
            // We couldn't find `foo` for `foo.bar.baz`, search the next search path.
            return Err(PackageKind::Root);
        }

        in_sub_package = true;
    }

    let kind = if in_namespace_package {
        PackageKind::Namespace
    } else if in_sub_package {
        PackageKind::Regular
    } else {
        PackageKind::Root
    };

    Ok(ResolvedPackage {
        kind,
        path: package_path,
    })
}

#[derive(Debug)]
struct ResolvedPackage {
    path: PathBuf,
    kind: PackageKind,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum PackageKind {
    /// A root package or module. E.g. `foo` in `foo.bar.baz` or just `foo`.
    Root,

    /// A regular sub-package where the parent contains an `__init__.py`.
    ///
    /// For example, `bar` in `foo.bar` when the `foo` directory contains an `__init__.py`.
    Regular,

    /// A sub-package in a namespace package. A namespace package is a package without an `__init__.py`.
    ///
    /// For example, `bar` in `foo.bar` if the `foo` directory contains no `__init__.py`.
    Namespace,
}

impl PackageKind {
    const fn is_regular_package(self) -> bool {
        matches!(self, PackageKind::Regular)
    }
}

#[derive(Debug)]
struct TypeshedVersions(FxHashMap<ModuleName, PyVersionRange>);

#[allow(unused)]
impl TypeshedVersions {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn contains_module(&self, module_name: impl Into<ModuleName>) -> bool {
        self.0.contains_key(&module_name.into())
    }

    fn module_exists_on_version(
        &self,
        module: impl Into<ModuleName>,
        version: impl Into<PyVersion>,
    ) -> bool {
        let version = version.into();
        let mut module = Some(module.into());
        while let Some(module_to_try) = module {
            if let Some(range) = self.0.get(&module_to_try) {
                return range.contains(version);
            }
            module = module_to_try.parent();
        }
        false
    }
}

impl FromStr for TypeshedVersions {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut map = HashMap::with_hasher(BuildHasherDefault::default());

        for (line_number, line) in s.lines().enumerate() {
            let line_number = line_number + 1; // humans expect line numbers to be 1-indexed
            let Some(content) = line.split('#').map(str::trim).next() else {
                continue;
            };
            if content.is_empty() {
                continue;
            }
            let parts: Vec<&str> = content.split(':').map(str::trim).collect();
            let (module_name, rest) = match parts.as_slice() {
                [module_name, rest] => (ModuleName::new(module_name), rest),
                _ => bail!(
                    "Error on line {line_number}: expected each line of VERSIONS to have exactly one colon"
                ),
            };
            map.insert(
                module_name,
                rest.parse()
                    .context(format!("Error on line {line_number}"))?,
            );
        }

        Ok(Self(map))
    }
}

impl std::fmt::Display for TypeshedVersions {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let sorted_items: BTreeMap<&ModuleName, &PyVersionRange> = self.0.iter().collect();
        for (module_name, range) in sorted_items {
            writeln!(f, "{module_name}-{range}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum PyVersionRange {
    AvailableFrom(RangeFrom<PyVersion>),
    AvailableWithin(RangeInclusive<PyVersion>),
}

impl PyVersionRange {
    fn contains(&self, version: PyVersion) -> bool {
        match self {
            Self::AvailableFrom(inner) => inner.contains(&version),
            Self::AvailableWithin(inner) => inner.contains(&version),
        }
    }
}

impl FromStr for PyVersionRange {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('-').map(str::trim).collect();
        Ok(match parts.as_slice() {
            [lower, ""] => Self::AvailableFrom((lower.parse()?)..),
            [lower, upper] => Self::AvailableWithin((lower.parse()?)..=(upper.parse()?)),
            _ => bail!(
                "Expected all non-comment lines in VERSIONS to have exactly one '-' character"
            ),
        })
    }
}

impl std::fmt::Display for PyVersionRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AvailableFrom(range_from) => write!(f, "{}-", range_from.start),
            Self::AvailableWithin(range_inclusive) => {
                write!(f, "{}-{}", range_inclusive.start(), range_inclusive.end())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct PyVersion {
    major: u8,
    minor: u8,
}

impl FromStr for PyVersion {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('.').collect();
        let [major, minor] = parts.as_slice() else {
            bail!("Expected all versions in the VERSIONS file to be in the form ${{MAJOR}}.${{MINOR}}")
        };
        Ok(Self {
            major: major.parse()?,
            minor: minor.parse()?,
        })
    }
}

impl std::fmt::Display for PyVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let PyVersion { major, minor } = self;
        write!(f, "{major}.{minor}")
    }
}

// TODO: unify with the PythonVersion enum in the linter/formatter crates?
#[allow(unused)]
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
enum SupportedPyVersion {
    Py37,
    #[default]
    Py38,
    Py39,
    Py310,
    Py311,
    Py312,
    Py313,
}

impl From<SupportedPyVersion> for PyVersion {
    fn from(value: SupportedPyVersion) -> Self {
        match value {
            SupportedPyVersion::Py37 => PyVersion { major: 3, minor: 7 },
            SupportedPyVersion::Py38 => PyVersion { major: 3, minor: 8 },
            SupportedPyVersion::Py39 => PyVersion { major: 3, minor: 9 },
            SupportedPyVersion::Py310 => PyVersion {
                major: 3,
                minor: 10,
            },
            SupportedPyVersion::Py311 => PyVersion {
                major: 3,
                minor: 11,
            },
            SupportedPyVersion::Py312 => PyVersion {
                major: 3,
                minor: 12,
            },
            SupportedPyVersion::Py313 => PyVersion {
                major: 3,
                minor: 13,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};
    use std::num::NonZeroU32;
    use std::path::{Path, PathBuf};
    use std::str::FromStr;

    use zip::ZipArchive;

    use crate::db::tests::TestDb;
    use crate::db::SourceDb;
    use crate::module::{
        path_to_module, resolve_module, set_module_search_paths, ModuleKind, ModuleName,
        ModuleResolutionInputs, SupportedPyVersion, TypeshedVersions, TYPESHED_STDLIB_DIRECTORY,
    };
    use crate::semantic::Dependency;

    struct TestCase {
        temp_dir: tempfile::TempDir,
        db: TestDb,

        src: PathBuf,
        custom_typeshed: PathBuf,
        site_packages: PathBuf,
    }

    fn create_resolver() -> std::io::Result<TestCase> {
        let temp_dir = tempfile::tempdir()?;

        let src = temp_dir.path().join("src");
        let site_packages = temp_dir.path().join("site_packages");
        let custom_typeshed = temp_dir.path().join("typeshed");

        std::fs::create_dir(&src)?;
        std::fs::create_dir(&site_packages)?;
        std::fs::create_dir(&custom_typeshed)?;

        let src = src.canonicalize()?;
        let site_packages = site_packages.canonicalize()?;
        let custom_typeshed = custom_typeshed.canonicalize()?;

        let search_paths = ModuleResolutionInputs {
            extra_paths: vec![],
            workspace_root: src.clone(),
            site_packages: Some(site_packages.clone()),
            custom_typeshed: Some(custom_typeshed.clone()),
        };

        let mut db = TestDb::default();
        set_module_search_paths(&mut db, search_paths);

        Ok(TestCase {
            temp_dir,
            db,
            src,
            custom_typeshed,
            site_packages,
        })
    }

    #[test]
    fn first_party_module() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_path = src.join("foo.py");
        std::fs::write(&foo_path, "print('Hello, world!')")?;

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();

        assert_eq!(
            Some(foo_module),
            resolve_module(&db, ModuleName::new("foo"))?
        );

        assert_eq!(ModuleName::new("foo"), foo_module.name(&db)?);
        assert_eq!(&src, foo_module.path(&db)?.root().path());
        assert_eq!(ModuleKind::Module, foo_module.kind(&db)?);
        assert_eq!(&foo_path, &*db.file_path(foo_module.path(&db)?.file()));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_path)?);

        Ok(())
    }

    #[test]
    fn stdlib() -> anyhow::Result<()> {
        let TestCase {
            db,
            custom_typeshed,
            ..
        } = create_resolver()?;
        let stdlib_dir = custom_typeshed.join(TYPESHED_STDLIB_DIRECTORY);
        std::fs::create_dir_all(&stdlib_dir).unwrap();
        let functools_path = stdlib_dir.join("functools.py");
        std::fs::write(&functools_path, "def update_wrapper(): ...").unwrap();
        let functools_module = resolve_module(&db, ModuleName::new("functools"))?.unwrap();

        assert_eq!(
            Some(functools_module),
            resolve_module(&db, ModuleName::new("functools"))?
        );
        assert_eq!(&stdlib_dir, functools_module.path(&db)?.root().path());
        assert_eq!(ModuleKind::Module, functools_module.kind(&db)?);
        assert_eq!(
            &functools_path,
            &*db.file_path(functools_module.path(&db)?.file())
        );

        assert_eq!(
            Some(functools_module),
            path_to_module(&db, &functools_path)?
        );

        Ok(())
    }

    #[test]
    fn first_party_precedence_over_stdlib() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            custom_typeshed,
            ..
        } = create_resolver()?;

        let stdlib_dir = custom_typeshed.join(TYPESHED_STDLIB_DIRECTORY);
        std::fs::create_dir_all(&stdlib_dir).unwrap();
        std::fs::create_dir_all(&src).unwrap();

        let stdlib_functools_path = stdlib_dir.join("functools.py");
        let first_party_functools_path = src.join("functools.py");
        std::fs::write(stdlib_functools_path, "def update_wrapper(): ...").unwrap();
        std::fs::write(&first_party_functools_path, "def update_wrapper(): ...").unwrap();
        let functools_module = resolve_module(&db, ModuleName::new("functools"))?.unwrap();

        assert_eq!(
            Some(functools_module),
            resolve_module(&db, ModuleName::new("functools"))?
        );
        assert_eq!(&src, functools_module.path(&db).unwrap().root().path());
        assert_eq!(ModuleKind::Module, functools_module.kind(&db)?);
        assert_eq!(
            &first_party_functools_path,
            &*db.file_path(functools_module.path(&db)?.file())
        );

        assert_eq!(
            Some(functools_module),
            path_to_module(&db, &first_party_functools_path)?
        );

        Ok(())
    }

    #[test]
    fn typeshed_zip_created_at_build_time() -> anyhow::Result<()> {
        // The file path here is hardcoded in this crate's `build.rs` script.
        // Luckily this crate will fail to build if this file isn't available at build time.
        const TYPESHED_ZIP_BYTES: &[u8] =
            include_bytes!(concat!(env!("OUT_DIR"), "/zipped_typeshed.zip"));
        assert!(!TYPESHED_ZIP_BYTES.is_empty());
        let mut typeshed_zip_archive = ZipArchive::new(Cursor::new(TYPESHED_ZIP_BYTES))?;

        let path_to_functools = Path::new("stdlib").join("functools.pyi");
        let mut functools_module_stub = typeshed_zip_archive
            .by_name(path_to_functools.to_str().unwrap())
            .unwrap();
        assert!(functools_module_stub.is_file());

        let mut functools_module_stub_source = String::new();
        functools_module_stub.read_to_string(&mut functools_module_stub_source)?;

        assert!(functools_module_stub_source.contains("def update_wrapper("));
        Ok(())
    }

    #[test]
    fn typeshed_versions() {
        let versions_data = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/vendor/typeshed/stdlib/VERSIONS"
        ));

        let versions = TypeshedVersions::from_str(versions_data).unwrap();
        assert!(versions.len() > 100);

        // (will start failing if the stdlib adds a `foo` module, but oh well)
        assert!(!versions.contains_module("foo"));
        assert!(versions.contains_module("asyncio"));
        assert!(versions.module_exists_on_version("asyncio", SupportedPyVersion::Py310));

        assert!(versions.contains_module("asyncio.staggered"));
        assert!(versions.module_exists_on_version("asyncio.staggered", SupportedPyVersion::Py38));
        assert!(!versions.module_exists_on_version("asyncio.staggered", SupportedPyVersion::Py37));

        assert!(versions.contains_module("audioop"));
        assert!(versions.module_exists_on_version("audioop", SupportedPyVersion::Py312));
        assert!(!versions.module_exists_on_version("audioop", SupportedPyVersion::Py313));
    }

    #[test]
    fn resolve_package() -> anyhow::Result<()> {
        let TestCase {
            src,
            db,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_dir = src.join("foo");
        let foo_path = foo_dir.join("__init__.py");
        std::fs::create_dir(&foo_dir)?;
        std::fs::write(&foo_path, "print('Hello, world!')")?;

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();

        assert_eq!(ModuleName::new("foo"), foo_module.name(&db)?);
        assert_eq!(&src, foo_module.path(&db)?.root().path());
        assert_eq!(&foo_path, &*db.file_path(foo_module.path(&db)?.file()));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_path)?);

        // Resolving by directory doesn't resolve to the init file.
        assert_eq!(None, path_to_module(&db, &foo_dir)?);

        Ok(())
    }

    #[test]
    fn package_priority_over_module() -> anyhow::Result<()> {
        let TestCase {
            db,
            temp_dir: _temp_dir,
            src,
            ..
        } = create_resolver()?;

        let foo_dir = src.join("foo");
        let foo_init = foo_dir.join("__init__.py");
        std::fs::create_dir(&foo_dir)?;
        std::fs::write(&foo_init, "print('Hello, world!')")?;

        let foo_py = src.join("foo.py");
        std::fs::write(&foo_py, "print('Hello, world!')")?;

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();

        assert_eq!(&src, foo_module.path(&db)?.root().path());
        assert_eq!(&foo_init, &*db.file_path(foo_module.path(&db)?.file()));
        assert_eq!(ModuleKind::Package, foo_module.kind(&db)?);

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_init)?);
        assert_eq!(None, path_to_module(&db, &foo_py)?);

        Ok(())
    }

    #[test]
    fn typing_stub_over_module() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_stub = src.join("foo.pyi");
        let foo_py = src.join("foo.py");
        std::fs::write(&foo_stub, "x: int")?;
        std::fs::write(&foo_py, "print('Hello, world!')")?;

        let foo = resolve_module(&db, ModuleName::new("foo"))?.unwrap();

        assert_eq!(&src, foo.path(&db)?.root().path());
        assert_eq!(&foo_stub, &*db.file_path(foo.path(&db)?.file()));

        assert_eq!(Some(foo), path_to_module(&db, &foo_stub)?);
        assert_eq!(None, path_to_module(&db, &foo_py)?);

        Ok(())
    }

    #[test]
    fn sub_packages() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo = src.join("foo");
        let bar = foo.join("bar");
        let baz = bar.join("baz.py");

        std::fs::create_dir_all(&bar)?;
        std::fs::write(foo.join("__init__.py"), "")?;
        std::fs::write(bar.join("__init__.py"), "")?;
        std::fs::write(&baz, "print('Hello, world!')")?;

        let baz_module = resolve_module(&db, ModuleName::new("foo.bar.baz"))?.unwrap();

        assert_eq!(&src, baz_module.path(&db)?.root().path());
        assert_eq!(&baz, &*db.file_path(baz_module.path(&db)?.file()));

        assert_eq!(Some(baz_module), path_to_module(&db, &baz)?);

        Ok(())
    }

    #[test]
    fn namespace_package() -> anyhow::Result<()> {
        let TestCase {
            db,
            temp_dir: _,
            src,
            site_packages,
            ..
        } = create_resolver()?;

        // From [PEP420](https://peps.python.org/pep-0420/#nested-namespace-packages).
        // But uses `src` for `project1` and `site_packages2` for `project2`.
        // ```
        // src
        //   parent
        //     child
        //       one.py
        // site_packages
        //   parent
        //     child
        //       two.py
        // ```

        let parent1 = src.join("parent");
        let child1 = parent1.join("child");
        let one = child1.join("one.py");

        std::fs::create_dir_all(child1)?;
        std::fs::write(&one, "print('Hello, world!')")?;

        let parent2 = site_packages.join("parent");
        let child2 = parent2.join("child");
        let two = child2.join("two.py");

        std::fs::create_dir_all(&child2)?;
        std::fs::write(&two, "print('Hello, world!')")?;

        let one_module = resolve_module(&db, ModuleName::new("parent.child.one"))?.unwrap();

        assert_eq!(Some(one_module), path_to_module(&db, &one)?);

        let two_module = resolve_module(&db, ModuleName::new("parent.child.two"))?.unwrap();
        assert_eq!(Some(two_module), path_to_module(&db, &two)?);

        Ok(())
    }

    #[test]
    fn regular_package_in_namespace_package() -> anyhow::Result<()> {
        let TestCase {
            db,
            temp_dir: _,
            src,
            site_packages,
            ..
        } = create_resolver()?;

        // Adopted test case from the [PEP420 examples](https://peps.python.org/pep-0420/#nested-namespace-packages).
        // The `src/parent/child` package is a regular package. Therefore, `site_packages/parent/child/two.py` should not be resolved.
        // ```
        // src
        //   parent
        //     child
        //       one.py
        // site_packages
        //   parent
        //     child
        //       two.py
        // ```

        let parent1 = src.join("parent");
        let child1 = parent1.join("child");
        let one = child1.join("one.py");

        std::fs::create_dir_all(&child1)?;
        std::fs::write(child1.join("__init__.py"), "print('Hello, world!')")?;
        std::fs::write(&one, "print('Hello, world!')")?;

        let parent2 = site_packages.join("parent");
        let child2 = parent2.join("child");
        let two = child2.join("two.py");

        std::fs::create_dir_all(&child2)?;
        std::fs::write(two, "print('Hello, world!')")?;

        let one_module = resolve_module(&db, ModuleName::new("parent.child.one"))?.unwrap();

        assert_eq!(Some(one_module), path_to_module(&db, &one)?);

        assert_eq!(
            None,
            resolve_module(&db, ModuleName::new("parent.child.two"))?
        );
        Ok(())
    }

    #[test]
    fn module_search_path_priority() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            site_packages,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_src = src.join("foo.py");
        let foo_site_packages = site_packages.join("foo.py");

        std::fs::write(&foo_src, "")?;
        std::fs::write(&foo_site_packages, "")?;

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();

        assert_eq!(&src, foo_module.path(&db)?.root().path());
        assert_eq!(&foo_src, &*db.file_path(foo_module.path(&db)?.file()));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo_src)?);
        assert_eq!(None, path_to_module(&db, &foo_site_packages)?);

        Ok(())
    }

    #[test]
    #[cfg(target_family = "unix")]
    fn symlink() -> anyhow::Result<()> {
        let TestCase {
            db,
            src,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo = src.join("foo.py");
        let bar = src.join("bar.py");

        std::fs::write(&foo, "")?;
        std::os::unix::fs::symlink(&foo, &bar)?;

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();
        let bar_module = resolve_module(&db, ModuleName::new("bar"))?.unwrap();

        assert_ne!(foo_module, bar_module);

        assert_eq!(&src, foo_module.path(&db)?.root().path());
        assert_eq!(&foo, &*db.file_path(foo_module.path(&db)?.file()));

        // Bar has a different name but it should point to the same file.

        assert_eq!(&src, bar_module.path(&db)?.root().path());
        assert_eq!(foo_module.path(&db)?.file(), bar_module.path(&db)?.file());
        assert_eq!(&foo, &*db.file_path(bar_module.path(&db)?.file()));

        assert_eq!(Some(foo_module), path_to_module(&db, &foo)?);
        assert_eq!(Some(bar_module), path_to_module(&db, &bar)?);

        Ok(())
    }

    #[test]
    fn resolve_dependency() -> anyhow::Result<()> {
        let TestCase {
            src,
            db,
            temp_dir: _temp_dir,
            ..
        } = create_resolver()?;

        let foo_dir = src.join("foo");
        let foo_path = foo_dir.join("__init__.py");
        let bar_path = foo_dir.join("bar.py");

        std::fs::create_dir(&foo_dir)?;
        std::fs::write(foo_path, "from .bar import test")?;
        std::fs::write(bar_path, "test = 'Hello world'")?;

        let foo_module = resolve_module(&db, ModuleName::new("foo"))?.unwrap();
        let bar_module = resolve_module(&db, ModuleName::new("foo.bar"))?.unwrap();

        // `from . import bar` in `foo/__init__.py` resolves to `foo`
        assert_eq!(
            Some(ModuleName::new("foo")),
            foo_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(1).unwrap(),
                    module: None,
                }
            )?
        );

        // `from baz import bar` in `foo/__init__.py` should resolve to `baz.py`
        assert_eq!(
            Some(ModuleName::new("baz")),
            foo_module.resolve_dependency(&db, &Dependency::Module(ModuleName::new("baz")))?
        );

        // from .bar import test in `foo/__init__.py` should resolve to `foo/bar.py`
        assert_eq!(
            Some(ModuleName::new("foo.bar")),
            foo_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(1).unwrap(),
                    module: Some(ModuleName::new("bar"))
                }
            )?
        );

        // from .. import test in `foo/__init__.py` resolves to `` which is not a module
        assert_eq!(
            None,
            foo_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(2).unwrap(),
                    module: None
                }
            )?
        );

        // `from . import test` in `foo/bar.py` resolves to `foo`
        assert_eq!(
            Some(ModuleName::new("foo")),
            bar_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(1).unwrap(),
                    module: None
                }
            )?
        );

        // `from baz import test` in `foo/bar.py` resolves to `baz`
        assert_eq!(
            Some(ModuleName::new("baz")),
            bar_module.resolve_dependency(&db, &Dependency::Module(ModuleName::new("baz")))?
        );

        // `from .baz import test` in `foo/bar.py` resolves to `foo.baz`.
        assert_eq!(
            Some(ModuleName::new("foo.baz")),
            bar_module.resolve_dependency(
                &db,
                &Dependency::Relative {
                    level: NonZeroU32::new(1).unwrap(),
                    module: Some(ModuleName::new("baz"))
                }
            )?
        );

        Ok(())
    }
}
